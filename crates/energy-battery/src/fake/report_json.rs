//! Building the fake device's report document.

use serde_json::{Value, json};

use crate::fake::state::FakeState;

impl FakeState {
    /// `acMode` as the next report shows it: 127 while wedged.
    pub fn reported_ac_mode(&self) -> i32 {
        if self.behaviour.wedged {
            127
        } else {
            self.ac_mode
        }
    }

    /// A report with the packs nested under `properties`, the envelope variant a firmware uses.
    pub fn report_json(&self) -> Value {
        json!({
            "timestamp": self.timestamp,
            "messageId": self.message_id,
            "sn": self.sn,
            "version": self.version,
            "product": self.product,
            "properties": {
                "electricLevel": self.soc_pct,
                "acMode": self.reported_ac_mode(),
                "inputLimit": self.input_limit_w,
                "outputLimit": self.output_limit_w,
                "smartMode": self.smart_mode,
                "minSoc": self.min_soc_deci,
                "socSet": self.soc_set_deci,
                "socLimit": 0,
                "chargeMaxLimit": self.charge_max_w,
                "inverseMaxPower": self.inverse_max_w,
                "gridInputPower": self.grid_input_w,
                "outputHomePower": self.output_home_w,
                "solarInputPower": 0,
                "solarPower1": 0,
                "solarPower2": 0,
                "outputPackPower": 0,
                "packInputPower": 0,
                "packNum": self.packs.len(),
                "BatVolt": 4935,
                "hyperTmp": 2993,
                "gridReverse": 1,
                "gridStandard": 4,
                "oldMode": 0,
                "is_error": 0,
                "faultLevel": 1,
                "localApiEnable": true,
                "hemsState": 0,
                "remainOutTime": 59940,
                "writeRsp": 0,
                "ts": self.timestamp,
                "packData": self.packs.iter().map(crate::fake::pack::FakePack::json).collect::<Vec<_>>(),
            },
        })
    }
}
