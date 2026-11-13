//! Which named devices the forecast may add above the base, and why the rest stay inside it.

use std::collections::BTreeMap;

use super::device::LoadDevice;
use super::profile::LoadProfile;

/// Which named devices this forecast adds above the base, and why the rest are left inside it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DevicePlan {
    /// Devices added back, with the profile to add.
    pub included: Vec<(LoadDevice, LoadProfile)>,
    /// Devices left inside the base, with the reason.
    pub excluded: Vec<(LoadDevice, String)>,
    /// Things the user should know about a device that *is* modelled.
    pub notes: Vec<String>,
}

/// Decide which devices the forecast may add back.
///
/// A device is added back exactly when its meter was subtracted while the **current** base profile
/// was learned and a profile of its own is stored. Adding a device the base still contains counts
/// it twice, and a doubling hides in the temperature response as a steeper slope instead of looking
/// like an error.
///
/// While the house is away, only the devices marked `keep_while_away` are added: the learned house
/// shape is not served at all, so everything else in it is off by definition.
pub fn device_plan(
    base: Option<&LoadProfile>,
    devices: &[LoadDevice],
    profiles: &BTreeMap<String, LoadProfile>,
    holiday: bool,
) -> DevicePlan {
    let mut plan = DevicePlan::default();
    if devices.is_empty() {
        return plan;
    }
    let Some(base) = base else {
        let reason =
            "no base profile has been learned yet, so the fallback shape is the whole house \
                      and it already contains every device"
                .to_string();
        plan.excluded = devices
            .iter()
            .map(|device| (device.clone(), reason.clone()))
            .collect();
        return plan;
    };
    for device in devices {
        if holiday && !device.keep_while_away {
            plan.excluded.push((
                device.clone(),
                "the house is away and it is not marked as running while away".to_string(),
            ));
            continue;
        }
        let subtracted = base.devices_subtracted.contains(&device.key);
        if !subtracted {
            plan.excluded.push((
                device.clone(),
                "the current base profile was learned without subtracting it, so it is still inside the base"
                    .to_string(),
            ));
            continue;
        }
        let Some(mut component) = profiles.get(&device.key).cloned() else {
            plan.excluded.push((
                device.clone(),
                "it has no profile of its own yet, so it is still inside the base".to_string(),
            ));
            continue;
        };
        let stored_kind = component.kind.clone().unwrap_or_default();
        if !stored_kind.is_empty() && stored_kind != device.kind.as_str() {
            plan.notes.push(format!(
                "load.devices: {} was last learned as {stored_kind:?}; the next learning run will re-model it as the kind it is configured as now",
                device.label()
            ));
        }
        if !device.temperature {
            // The kind is the authority on the temperature term at serving time exactly as at
            // learning time: a slope fitted under another kind must not survive this one.
            component.without_temperature();
        }
        plan.included.push((device.clone(), component));
    }
    plan
}
