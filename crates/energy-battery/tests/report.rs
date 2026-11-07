// Test code: a failed assertion is the report, so unwrap/expect/panic! are the intended API.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::assert_is_empty
)]

mod support;

use energy_battery::report::decode;
use energy_battery::{GridReverse, SocLimit};
use serde_json::json;
use support::{PACK_SN, SN, parse, report, report_json, report_with};

#[test]
fn parses_the_default_report() {
    let report = report(SN);
    assert_eq!(report.sn, SN);
    assert_eq!(report.timestamp, 1_791_959_470);
    assert_eq!(report.message_id, 4211);
    assert_eq!(report.version, Some(3));
    assert_eq!(report.product.as_deref(), Some("solarFlow800Plus"));
    assert_eq!(report.soc_pct(), 45.0);
    assert_eq!(report.properties.ac_mode, 2);
    assert_eq!(report.properties.charge_max_w, 1000);
    assert_eq!(report.properties.inverse_max_w, 800);
    assert_eq!(report.total_capacity_kwh(), Some(1.92));
}

#[test]
fn accepts_packs_nested_under_properties() {
    let mut document = report_json(SN);
    let packs = document["packData"].take();
    document["properties"]["packData"] = packs;
    let report = parse(&document);
    assert_eq!(report.packs.len(), 1);
    assert_eq!(report.packs[0].sn, PACK_SN);
    assert_eq!(report.total_capacity_kwh(), Some(1.92));
}

#[test]
fn ignores_unknown_keys_and_defaults_missing_ones() {
    let document = json!({
        "sn": SN,
        "properties": {"electricLevel": 30, "net": {"ssid": "x"}, "aiState": 4, "Fanspeed": 2},
    });
    let report = parse(&document);
    assert_eq!(report.soc_pct(), 30.0);
    assert_eq!(report.properties.ac_mode, 0);
    assert_eq!(report.properties.smart_mode, 0);
    assert_eq!(report.properties.charge_max_w, 0);
    assert!(report.packs.is_empty());
    assert_eq!(report.total_capacity_kwh(), None);
}

#[test]
fn rejects_a_body_without_a_serial() {
    let error = energy_battery::parse_report(br#"{"timestamp":1}"#).unwrap_err();
    assert!(error.to_string().contains("no sn"), "{error}");
    let error = energy_battery::parse_report(b"not json").unwrap_err();
    assert!(error.to_string().contains("not JSON"), "{error}");
}

#[test]
fn decodes_percent_and_voltage_units() {
    assert_eq!(decode::deci_percent(1000), 100.0);
    assert_eq!(decode::deci_percent(100), 10.0);
    assert_eq!(decode::centivolts_to_volts(4810), 48.10);
    assert_eq!(decode::centivolts_to_volts(321), 3.21);
    assert_eq!(decode::deci_kelvin_to_celsius(2993), 26.2);
    assert_eq!(decode::deci_kelvin_to_celsius(3051), 32.0);
}

#[test]
fn decodes_current_firmware_and_runtime_units() {
    assert_eq!(decode::signed_deci_amps(0), 0.0);
    assert_eq!(decode::signed_deci_amps(123), 12.3);
    assert_eq!(decode::signed_deci_amps(0xFFFF), -0.1);
    assert_eq!(decode::signed_deci_amps(0x8000), -3276.8);
    assert_eq!(decode::packed_version(4355), "1.1.3");
    assert_eq!(decode::remain_minutes(59940), None);
    assert_eq!(decode::remain_minutes(-1), None);
    assert_eq!(decode::remain_minutes(120), Some(120));
    assert_eq!(decode::watts(-5), 0);
    assert_eq!(decode::watts(650), 650);
}

#[test]
fn decodes_the_property_set_in_real_units() {
    let report = report(SN);
    let properties = &report.properties;
    assert_eq!(properties.soc_set_pct, 100.0);
    assert_eq!(properties.min_soc_pct, 10.0);
    assert_eq!(properties.soc_limit, SocLimit::None);
    assert_eq!(properties.grid_reverse, GridReverse::AllowExport);
    assert_eq!(properties.old_mode, Some(2));
    assert_eq!(properties.solar_power_w, 200);
    assert_eq!(properties.battery_voltage_v, 49.35);
    assert_eq!(properties.temperature_c, 26.2);
    assert_eq!(properties.remain_out_min, None);
    assert_eq!(properties.local_api_enable, Some(true));
    assert!(!properties.is_error);
    assert_eq!(report.available_kwh(), Some(1.92 * 35.0 / 100.0));
}

#[test]
fn decodes_old_firmware_spellings_and_bounds() {
    let report = report_with(
        SN,
        json!({"OldMode": 3, "oldMode": null, "socLimit": 2, "gridReverse": 2, "is_error": 1}),
    );
    assert_eq!(report.properties.old_mode, Some(3));
    assert_eq!(report.properties.soc_limit, SocLimit::Lower);
    assert_eq!(report.properties.grid_reverse, GridReverse::ForbidExport);
    assert!(report.properties.is_error);
    assert!(SocLimit::Lower.blocks_discharge());
    assert!(!SocLimit::Lower.blocks_charge());
}

#[test]
fn decodes_pack_fields() {
    let report = report(SN);
    let pack = &report.packs[0];
    assert_eq!(pack.total_voltage_v, 48.10);
    assert_eq!(pack.max_cell_v, 3.21);
    assert_eq!(pack.min_cell_v, 3.21);
    assert_eq!(pack.max_temp_c, 32.0);
    assert_eq!(pack.current_a, 0.0);
    assert_eq!(pack.soft_version.as_deref(), Some("1.1.3"));
    assert_eq!(pack.label.unwrap().model, "AB2000");
    assert!(pack.label.unwrap().confirmed);
}

#[test]
fn labels_capacity_from_the_serial_prefix_and_pack_type() {
    use energy_battery::PackLabel;
    let cases = [
        ("A12X4567890ABCDE", None, "AB1000", 0.96, false),
        ("A1234567890ABCDE", None, "AIO2400", 2.4, false),
        ("B1234567890ABCDE", Some(70), "I8000", 8.0, true),
        ("B1234567890ABCDE", None, "AB1000S", 0.96, false),
        ("C1234567890ABCDE", None, "AB2000", 1.92, true),
        ("C12F4567890ABCDE", None, "AB2000S", 1.92, false),
        ("C12E4567890ABCDE", None, "AB2000X", 1.92, false),
        ("F1234567890ABCDE", None, "AB3000", 2.88, false),
        ("G1234567890ABCDE", None, "AB3000L", 2.88, false),
        ("J1234567890ABCDE", None, "I2400", 2.4, false),
    ];
    for (sn, pack_type, model, kwh, confirmed) in cases {
        let label = PackLabel::for_pack(sn, pack_type).unwrap();
        assert_eq!(label.model, model, "{sn}");
        assert_eq!(label.capacity_kwh, kwh, "{sn}");
        assert_eq!(label.confirmed, confirmed, "{sn}");
    }
    assert_eq!(PackLabel::for_pack("Z1234567890ABCDE", None), None);
    let ab1000 = PackLabel::for_pack("A12X4567890ABCDE", None).unwrap();
    assert_eq!(ab1000.capacity_kwh, 0.96);
}

#[test]
fn sums_capacity_across_packs() {
    let mut document = report_json(SN);
    document["packData"][0]["socLevel"] = json!(60);
    let mut second = document["packData"][0].clone();
    second["sn"] = json!("CO4A000000000002");
    document["packData"].as_array_mut().unwrap().push(second);
    let report = parse(&document);
    assert_eq!(report.packs.len(), 2);
    assert_eq!(report.total_capacity_kwh(), Some(3.84));
    assert_eq!(report.available_kwh(), Some(3.84 * 35.0 / 100.0));
}
