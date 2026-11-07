//! One pack inside the fake device.

use serde_json::{Value, json};

/// A fake pack: just enough fields for the parser, the capacity label and the protection inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakePack {
    pub sn: String,
    pub pack_type: i32,
    /// Pack SoC in percent.
    pub soc_pct: u32,
    /// `totalVol` in centivolts (4810 = 48.10 V).
    pub total_vol_cv: i64,
    /// `batcur` as a signed 16-bit deci-ampere value.
    pub current_da: i64,
    /// `maxTemp` in 0.1 K.
    pub max_temp_dk: i64,
    /// Packed firmware (`0x1103` = 4355 = "1.1.3").
    pub soft_version: i64,
}

impl FakePack {
    /// The live fleet's pack: an AB2000 at 1.92 kWh nominal, serial prefix `C`.
    pub fn ab2000(sn: impl Into<String>) -> Self {
        Self {
            sn: sn.into(),
            pack_type: 1,
            soc_pct: 45,
            total_vol_cv: 4810,
            current_da: 0,
            max_temp_dk: 3051,
            soft_version: 4355,
        }
    }

    /// The JSON shape the report nests under `properties.packData`.
    pub fn json(&self) -> Value {
        json!({
            "sn": self.sn,
            "packType": self.pack_type,
            "socLevel": self.soc_pct,
            "state": 0,
            "power": 0,
            "totalVol": self.total_vol_cv,
            "maxVol": 321,
            "minVol": 321,
            "maxTemp": self.max_temp_dk,
            "batcur": self.current_da,
            "softVersion": self.soft_version,
            "heatState": 0,
        })
    }
}
