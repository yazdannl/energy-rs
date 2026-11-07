// Test code: a failed assertion is the report, so unwrap/expect/panic! are the intended API.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_battery::{
    BatteryError, Command, Direction, MAX_BODY_BYTES, WriteAck, WriteProperties, WriteRequest,
};
use support::SN;

fn body(properties: WriteProperties, id: u32) -> String {
    String::from_utf8(WriteRequest::new(SN, id, properties).body().unwrap()).unwrap()
}

#[test]
fn charge_payload_is_exact() {
    assert_eq!(
        body(Command::Charge { watts: 650 }.properties(), 7),
        r#"{"sn":"EOC1NLN9N4XXXXXX","id":7,"source":2,"properties":{"smartMode":1,"acMode":1,"outputLimit":0,"inputLimit":650}}"#
    );
}

#[test]
fn discharge_payload_is_exact() {
    assert_eq!(
        body(Command::Discharge { watts: 400 }.properties(), 8),
        r#"{"sn":"EOC1NLN9N4XXXXXX","id":8,"source":2,"properties":{"smartMode":1,"acMode":2,"outputLimit":400,"inputLimit":0}}"#
    );
}

#[test]
fn hold_payload_has_no_ac_mode() {
    let request = WriteRequest::new(SN, 9, Command::Hold.properties());
    let properties = request.properties();
    assert_eq!(properties.ac_mode(), None);
    assert_eq!(properties.input_limit(), Some(0));
    assert_eq!(properties.output_limit(), Some(0));
    assert_eq!(properties.smart_mode(), Some(1));
    assert_eq!(
        String::from_utf8(request.body().unwrap()).unwrap(),
        r#"{"sn":"EOC1NLN9N4XXXXXX","id":9,"source":2,"properties":{"smartMode":1,"outputLimit":0,"inputLimit":0}}"#
    );
}

#[test]
fn trim_payloads_change_only_the_limit() {
    let charge = body(
        Command::Trim {
            direction: Direction::Charge,
            watts: 520,
        }
        .properties(),
        11,
    );
    assert_eq!(
        charge,
        r#"{"sn":"EOC1NLN9N4XXXXXX","id":11,"source":2,"properties":{"smartMode":1,"inputLimit":520}}"#
    );
    let discharge = body(
        Command::Trim {
            direction: Direction::Discharge,
            watts: 300,
        }
        .properties(),
        12,
    );
    assert_eq!(
        discharge,
        r#"{"sn":"EOC1NLN9N4XXXXXX","id":12,"source":2,"properties":{"smartMode":1,"outputLimit":300}}"#
    );
}

#[test]
fn smart_mode_comes_first_in_every_body() {
    for command in [
        Command::Charge { watts: 100 },
        Command::Discharge { watts: 100 },
        Command::Hold,
        Command::Trim {
            direction: Direction::Charge,
            watts: 100,
        },
    ] {
        let body = body(command.properties(), 1);
        let prefix = r#""properties":{"smartMode":"#;
        assert!(body.contains(prefix), "{command:?}: {body}");
    }
}

#[test]
fn every_control_body_reasserts_smart_mode_one() {
    for command in [
        Command::Charge { watts: 1 },
        Command::Discharge { watts: 1 },
        Command::Hold,
        Command::Trim {
            direction: Direction::Charge,
            watts: 1,
        },
        Command::Trim {
            direction: Direction::Discharge,
            watts: 1,
        },
    ] {
        assert_eq!(command.properties().smart_mode(), Some(1), "{command:?}");
    }
}

#[test]
fn holds_never_write_ac_mode() {
    assert_eq!(Command::Hold.properties().ac_mode(), None);
    assert_eq!(
        Command::Trim {
            direction: Direction::Discharge,
            watts: 10
        }
        .properties()
        .ac_mode(),
        None
    );
}

#[test]
fn body_cap_is_enforced_before_sending() {
    let short = WriteRequest::new(SN, 1, Command::Charge { watts: 1000 }.properties());
    assert!(short.body().unwrap().len() < MAX_BODY_BYTES);
    let long_sn = "E".repeat(600);
    let long = WriteRequest::new(long_sn, 1, Command::Charge { watts: 1000 }.properties());
    match long.body().unwrap_err() {
        BatteryError::BodyTooLarge { len, limit } => {
            assert!(len > MAX_BODY_BYTES);
            assert_eq!(limit, MAX_BODY_BYTES);
        }
        other => panic!("expected BodyTooLarge, got {other}"),
    }
}

#[test]
fn ack_requires_success_and_our_serial() {
    let ok = WriteAck::parse(
        br#"{"timestamp":1,"messageId":57,"success":true,"code":200,"sn":"EOC1NLN9N4XXXXXX"}"#,
        SN,
    )
    .unwrap();
    assert_eq!(ok.code, 200);
    assert_eq!(ok.message_id, Some(57));

    let mismatch = WriteAck::parse(br#"{"success":true,"code":200,"sn":"OTHER"}"#, SN).unwrap_err();
    assert!(
        matches!(mismatch, BatteryError::SnMismatch { .. }),
        "{mismatch}"
    );
}

#[test]
fn ack_maps_the_documented_rejections() {
    let rejected = WriteAck::parse(
        br#"{"success":false,"code":400,"error":"failed to parse json"}"#,
        SN,
    )
    .unwrap_err();
    match rejected {
        BatteryError::Rejected { code, message } => {
            assert_eq!(code, 400);
            assert_eq!(message, "failed to parse json");
        }
        other => panic!("expected Rejected, got {other}"),
    }

    let sn = WriteAck::parse(
        br#"{"success":false,"code":401,"error":"sn does not match"}"#,
        SN,
    )
    .unwrap_err();
    assert!(matches!(sn, BatteryError::SnMismatch { .. }), "{sn}");

    let overflow = WriteAck::parse(
        br#"{"success":false,"code":413,"error":"buffer overflow risk"}"#,
        SN,
    )
    .unwrap_err();
    assert!(
        matches!(overflow, BatteryError::BufferOverflow { .. }),
        "{overflow}"
    );

    let unknown =
        WriteAck::parse(br#"{"success":false,"code":500,"error":"boom"}"#, SN).unwrap_err();
    match unknown {
        BatteryError::Rejected { code, .. } => assert_eq!(code, 500),
        other => panic!("expected Rejected, got {other}"),
    }
}

#[test]
fn ack_treats_false_success_with_code_200_as_a_rejection() {
    let error = WriteAck::parse(br#"{"success":false,"code":200}"#, SN).unwrap_err();
    match error {
        BatteryError::Rejected { message, .. } => assert_eq!(message, "success=false"),
        other => panic!("expected Rejected, got {other}"),
    }
}
