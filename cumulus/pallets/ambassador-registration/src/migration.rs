// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Migration module for the ambassador registration pallet.

/// This module contains migration logic that would be used when upgrading the pallet.
/// Currently, there are no migrations needed, but this file is included for future use.
pub mod v1 {
    use super::super::*;
    
    /// Migrate from the previous version to the current version.
    /// This is a no-op migration as this is the first version of the pallet.
    pub fn migrate<T: Config>() -> Weight {
        Weight::zero()
    }
}
