//! Parsing `load.devices` into usable entries plus one line per dropped entry.

use serde_json::Value;

use super::device::LoadDevice;
use super::device_kind::DeviceKind;

/// Parse configured device entries.
///
/// The config validator rejects every shape this drops at save time, but add-on options are read as
/// written, so a hand-edited options file must still not be able to subtract one meter twice or the
/// house meter from itself: both double-count, and a doubling hides inside the temperature response
/// as a steeper slope instead of showing up as an error.
pub fn parse_devices(raw: &[Value], house_entity: &str) -> (Vec<LoadDevice>, Vec<String>) {
    let house = house_entity.trim();
    let mut devices = Vec::new();
    let mut problems = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for (index, entry) in raw.iter().enumerate() {
        let where_ = format!("load.devices[{index}]");
        match parse_entry(entry, &where_, house, &mut seen) {
            Ok((device, note)) => {
                if let Some(note) = note {
                    problems.push(note);
                }
                devices.push(device);
            }
            Err(problem) => problems.push(problem),
        }
    }
    (devices, problems)
}

/// Parse one entry. `Err` is a dropped entry with its reason; `Ok` may still carry a note (an
/// unknown kind is modelled as a plain meter and reported).
fn parse_entry(
    entry: &Value,
    where_: &str,
    house: &str,
    seen: &mut Vec<String>,
) -> Result<(LoadDevice, Option<String>), String> {
    let map = entry
        .as_object()
        .ok_or_else(|| format!("{where_} is not an object"))?;
    let entity = map
        .get("entity")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if entity.is_empty() {
        return Err(format!("{where_} names no meter; it is not modelled"));
    }
    if entity == house {
        return Err(format!(
            "{where_} ({entity}) is the house meter itself (load.entity); it is not modelled, \
             because subtracting a total from itself would leave a base profile of zeros"
        ));
    }
    if seen.iter().any(|known| known == entity) {
        return Err(format!(
            "{where_} ({entity}) repeats a meter already listed; it is not modelled"
        ));
    }
    seen.push(entity.to_string());
    let raw_kind = map
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    let kind = DeviceKind::parse(if raw_kind.is_empty() {
        "unknown"
    } else {
        &raw_kind
    });
    let name = map
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let keep_while_away = map
        .get("keep_while_away")
        .or_else(|| map.get("keepWhileAway"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let device = LoadDevice::new(entity, &kind, name).with_keep_while_away(keep_while_away);
    let note = kind.label().is_none().then(|| {
        format!(
            "{where_} ({}): unknown kind {:?}; modelled as a plain meter with no temperature term",
            device.name, kind
        )
    });
    Ok((device, note))
}

/// One line per `load.devices` entry that was dropped before it could be subtracted.
pub fn device_config_problems(raw: &[Value], house_entity: &str) -> Vec<String> {
    parse_devices(raw, house_entity).1
}
