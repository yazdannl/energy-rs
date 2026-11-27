//! Windowed consumption evidence for the replan's load-forecast bias.
//!
//! The engine samples the live house load every tick. A solve integrates the most recent window,
//! turns it into a kW rate and moves the current slot plus the configured number of following
//! slots by the difference from the forecast. Everything here is pure: no clock, no I/O, and bad
//! evidence is refused rather than treated as zero.

use std::sync::Mutex;

/// Shortest usable window, seconds.
pub(crate) const MIN_WINDOW_S: i64 = 60;
/// Longest window the setting may ask for, seconds.
pub(crate) const MAX_WINDOW_S: i64 = 3600;
/// A gap longer than this is a hole in the evidence, not a plateau to integrate across.
const MAX_SAMPLE_GAP_S: i64 = 60;

/// The measured rate and how much real time it covers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Observation {
    /// Energy over the usable window divided by its elapsed time, kW.
    pub(crate) rate_kw: f64,
    /// Usable elapsed time, seconds.
    pub(crate) elapsed_s: i64,
}

/// Append one tick's load, poisoning the window on a bad reading and pruning old samples.
pub(crate) fn record_sample(samples: &Mutex<Vec<(i64, f64)>>, now: i64, load_kw: f64) {
    // Dropping a bad reading would let the rest of the window look healthy.
    let sample = if load_kw.is_finite() && load_kw >= 0.0 {
        load_kw
    } else {
        f64::NAN
    };
    let Ok(mut samples) = samples.lock() else {
        return;
    };
    if samples.last().is_some_and(|&(at, _)| at >= now) {
        return;
    }
    samples.push((now, sample));
    let cutoff = now.saturating_sub(MAX_WINDOW_S);
    samples.retain(|(at, _)| *at >= cutoff);
}

/// `0` means "follow the replan interval", which is an effective `replan_seconds` window.
pub(crate) fn effective_lookback_s(setting_s: i64, replan_s: i64) -> i64 {
    if setting_s > 0 {
        setting_s.clamp(MIN_WINDOW_S, MAX_WINDOW_S)
    } else {
        replan_s.max(MIN_WINDOW_S)
    }
}

/// Integrate `(at, load_kw)` samples over the most recent `lookback_s` seconds.
///
/// Samples must be chronological and non-negative; a non-finite value breaks the run instead of
/// being zero-filled. Less than a minute of usable time is never a measurement.
pub(crate) fn measure(samples: &[(i64, f64)], now: i64, lookback_s: i64) -> Option<Observation> {
    let window = lookback_s.max(MIN_WINDOW_S);
    let cutoff = now.saturating_sub(window);
    let mut energy_kwh = 0.0;
    let mut elapsed_s = 0.0;
    let mut previous: Option<(i64, f64)> = None;
    for &(at, load_kw) in samples {
        if at > now {
            return None;
        }
        if at < cutoff {
            continue;
        }
        // A bad sample inside the window poisons the whole observation: no zero-fill, and no
        // quietly integrating only the good part of a broken run.
        if !load_kw.is_finite() || load_kw < 0.0 {
            return None;
        }
        if let Some((prior_at, prior_load)) = previous {
            let dt = at - prior_at;
            if dt > 0 && dt <= MAX_SAMPLE_GAP_S {
                energy_kwh += prior_load.midpoint(load_kw) * dt as f64 / 3600.0;
                elapsed_s += dt as f64;
            }
        }
        previous = Some((at, load_kw));
    }
    if elapsed_s < MIN_WINDOW_S as f64 || !energy_kwh.is_finite() {
        return None;
    }
    let rate_kw = energy_kwh / (elapsed_s / 3600.0);
    (rate_kw.is_finite() && rate_kw >= 0.0).then_some(Observation {
        rate_kw,
        elapsed_s: elapsed_s as i64,
    })
}

/// Move the current slot and `slots` following slots by `rate_kw - forecast[0]`.
///
/// Returns the applied bias and the number of slots changed. Every corrected value is finite and
/// non-negative; a forecast the bias would corrupt is left untouched.
pub(crate) fn apply_bias(load_kw: &mut [f64], rate_kw: f64, slots: usize) -> Option<(f64, usize)> {
    let predicted = *load_kw.first()?;
    if !predicted.is_finite() || !rate_kw.is_finite() || rate_kw < 0.0 {
        return None;
    }
    let bias = rate_kw - predicted;
    if !bias.is_finite() {
        return None;
    }
    let count = slots.min(load_kw.len() - 1) + 1;
    let corrected: Vec<f64> = load_kw[..count].iter().map(|value| value + bias).collect();
    if !corrected.iter().all(|value| value.is_finite()) {
        return None;
    }
    for (value, fixed) in load_kw[..count].iter_mut().zip(corrected) {
        *value = fixed.max(0.0);
    }
    Some((bias, count))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{apply_bias, effective_lookback_s, measure};

    #[test]
    fn zero_setting_follows_the_replan_interval() {
        assert_eq!(effective_lookback_s(0, 300), 300);
        assert_eq!(effective_lookback_s(0, 30), 60);
        assert_eq!(effective_lookback_s(600, 300), 600);
        assert_eq!(effective_lookback_s(9_999, 300), 3_600);
    }

    #[test]
    fn one_minute_is_the_shortest_usable_window() {
        let samples = vec![(0, 1.0), (60, 1.0)];
        assert_eq!(measure(&samples, 60, 300).unwrap().elapsed_s, 60);
        assert!(measure(&[(0, 1.0), (30, 1.0)], 30, 300).is_none());
        assert!(
            measure(&samples, 30, 300).is_none(),
            "future samples are refused"
        );
    }

    #[test]
    fn bad_samples_are_refused_not_zero_filled() {
        let clean = vec![(0, 1.0), (60, 1.0)];
        for (at, load) in [(60, f64::NAN), (60, f64::INFINITY), (60, -1.0)] {
            let mut samples = clean.clone();
            samples[1] = (at, load);
            assert!(measure(&samples, 60, 300).is_none(), "{load}");
        }
        assert!(measure(&clean, 60, 300).is_some());
    }

    #[test]
    fn a_longer_lookback_sees_older_samples() {
        let samples = vec![
            (0, 4.0),
            (30, 4.0),
            (300, 1.0),
            (330, 1.0),
            (360, 1.0),
            (420, 1.0),
        ];
        let short = measure(&samples, 420, 120).expect("short window");
        let long = measure(&samples, 420, 600).expect("long window");
        assert!((short.rate_kw - 1.0).abs() < 1e-9);
        assert!(long.rate_kw > 1.0, "{}", long.rate_kw);
        assert_eq!(long.elapsed_s, 150, "the 270 s hole is not integrated");
    }

    #[test]
    fn evidence_is_integrated_by_time_not_averaged_by_sample_count() {
        let samples = vec![(0, 2.0), (30, 2.0), (90, 0.0)];
        let observed = measure(&samples, 90, 300).expect("evidence");
        assert!((observed.rate_kw - 4.0 / 3.0).abs() < 1e-9, "{observed:?}");
    }

    #[test]
    fn the_bias_hits_only_the_current_and_configured_following_slots() {
        let mut load = vec![3.5, 3.5, 3.5, 3.5];
        let (bias, count) = apply_bias(&mut load, 1.2, 1).expect("bias");
        assert!((bias + 2.3).abs() < 1e-9);
        assert_eq!(count, 2);
        assert!((load[0] - 1.2).abs() < 1e-9);
        assert!((load[1] - 1.2).abs() < 1e-9);
        assert_eq!(load[2], 3.5, "slot N+1 is untouched");
        assert_eq!(load[3], 3.5);

        let mut zero = vec![3.5, 3.5];
        assert_eq!(apply_bias(&mut zero, 1.2, 0).unwrap().1, 1);
        assert!((zero[0] - 1.2).abs() < 1e-9);
        assert_eq!(zero[1], 3.5);
    }

    #[test]
    fn a_bias_never_produces_a_negative_or_non_finite_forecast() {
        let mut load = vec![0.2, 1.0];
        apply_bias(&mut load, 0.0, 5).expect("bias");
        assert_eq!(load, vec![0.0, 0.8], "clamped and bounded by the horizon");
        assert!(apply_bias(&mut [f64::NAN, 1.0], 1.0, 1).is_none());
        assert!(apply_bias(&mut [1.0, 1.0], f64::NAN, 1).is_none());
        assert!(apply_bias(&mut [f64::INFINITY, 1.0], 1.0, 1).is_none());
    }
}
