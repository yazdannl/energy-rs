//! Compensated summation that matches CPython's `math.fsum`.
//!
//! The load profile's persisted values are Python `round(x, 4)` of sums over a few thousand
//! samples, so a naive `iter().sum()` can flip the fourth decimal. This is the Shewchuk partials
//! algorithm CPython uses: the result is the correctly rounded exact sum.

/// Correctly rounded sum of `values`, equal to CPython's `math.fsum`.
pub fn fsum<I: IntoIterator<Item = f64>>(values: I) -> f64 {
    let mut partials: Vec<f64> = Vec::with_capacity(8);
    let mut special = 0.0_f64;
    for value in values {
        add_partial(&mut partials, &mut special, value);
    }
    if special.is_nan() || special.is_infinite() {
        return special;
    }
    final_sum(&mut partials)
}

/// Fold one finite value into the Shewchuk partials; non-finite values accumulate separately.
fn add_partial(partials: &mut Vec<f64>, special: &mut f64, mut x: f64) {
    if !x.is_finite() {
        *special += x;
        return;
    }
    let mut kept = 0usize;
    for index in 0..partials.len() {
        let mut y = partials[index];
        if x.abs() < y.abs() {
            std::mem::swap(&mut x, &mut y);
        }
        let hi = x + y;
        let lo = y - (hi - x);
        if lo != 0.0 {
            partials[kept] = lo;
            kept += 1;
        }
        x = hi;
    }
    partials.truncate(kept);
    partials.push(x);
}

/// Recombine the partials with CPython's final rounding correction.
fn final_sum(partials: &mut Vec<f64>) -> f64 {
    let Some(mut hi) = partials.pop() else {
        return 0.0;
    };
    if partials.is_empty() {
        return hi;
    }
    let mut lo = 0.0;
    let mut index = partials.len();
    let mut broke = false;
    while index > 0 {
        index -= 1;
        let x = hi;
        let y = partials[index];
        hi = x + y;
        let yr = hi - x;
        lo = y - yr;
        if lo != 0.0 {
            broke = true;
            break;
        }
    }
    if broke {
        let previous = partials[index];
        let same_sign = (lo < 0.0 && previous < 0.0) || (lo > 0.0 && previous > 0.0);
        if same_sign {
            let y = lo * 2.0;
            let x = hi + y;
            let yr = x - hi;
            // CPython compares with `==` (not `total_cmp`), so -0.0 counts as equal here.
            #[allow(clippy::float_cmp)]
            if y == yr {
                hi = x;
            }
        }
    }
    hi
}

/// Arithmetic mean via [`fsum`]; `0.0` for an empty slice (every call site guards emptiness the way
/// the Python code does before calling `statistics.fmean`).
pub fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    fsum(values.iter().copied()) / values.len() as f64
}

/// Python's `round(value, digits)`: correct decimal rounding of the exact binary value, ties to
/// even. Rust's `{:.N}` formatting performs the same conversion, so format and parse back.
pub fn round_digits(value: f64, digits: usize) -> f64 {
    if !value.is_finite() {
        return value;
    }
    let text = format!("{value:.digits$}");
    text.parse::<f64>().unwrap_or(value)
}
