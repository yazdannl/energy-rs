//! Fleet state owned by M4, keyed by stable pack id.

use std::collections::BTreeMap;

use crate::dispatch::PackState;

/// Mutable per-pack dispatch state; no hardware state is stored here.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DispatchState {
    packs: BTreeMap<String, PackState>,
}

impl DispatchState {
    /// State for a pack, inserting an empty record on first use.
    pub fn pack(&mut self, pack_id: &str) -> &mut PackState {
        self.packs.entry(pack_id.to_owned()).or_default()
    }

    /// Read-only state for a pack.
    pub fn get(&self, pack_id: &str) -> Option<&PackState> {
        self.packs.get(pack_id)
    }

    /// All tracked pack ids in stable order.
    pub fn pack_ids(&self) -> Vec<String> {
        self.packs.keys().cloned().collect()
    }
}
