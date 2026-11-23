//! Injection point for the SoC-dependent charge limit observed on the live fleet (SPEC GAP).
//!
//! The frozen M2 SPEC models only a **flat** per-pack maximum (four packs at 800 W, aggregate
//! 3.2 kW) and an SoC ceiling; it defines no taper above ~96 % SoC. Live 2026-11-23 evidence
//! shows a pack at 96-98 % accepting only 25-50 % of a 1000 W command and 0 W at 98 %. Until the
//! reviewer amends the spec, this function deliberately returns the flat maximum: the real curve
//! must not be invented here. Replacing this one return with the agreed curve is the only change
//! needed in the optimizer; callers already ask for the limit at each candidate energy.

/// Maximum AC charge power, W, that a pack of `pack_charge_w` can accept at `soc_fraction`.
///
/// `soc_fraction` is stored energy / ceiling in `0..=1`. Flat by construction today.
pub fn max_charge_w_at_soc(pack_charge_w: f64, soc_fraction: f64) -> f64 {
    let _ = soc_fraction;
    pack_charge_w.max(0.0)
}
