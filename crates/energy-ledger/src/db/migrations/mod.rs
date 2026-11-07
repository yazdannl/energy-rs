//! The numbered migration list, oldest first. Never edit a shipped migration: add the next one.

pub(crate) mod v1;
pub(crate) mod v2;
pub(crate) mod v3;
pub(crate) mod v4;

use crate::db::migration::Migration;

pub(crate) const MIGRATIONS: &[Migration] =
    &[v1::MIGRATION, v2::MIGRATION, v3::MIGRATION, v4::MIGRATION];
