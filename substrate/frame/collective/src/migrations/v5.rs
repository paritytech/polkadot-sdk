// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Migrate `ProposalOf` to store a [`VersionedCall`], wrapping existing proposals with the current
//! `transaction_version` (so they stay executable now and are checked against future upgrades).

use crate::{Config, Pallet, ProposalOf};
use frame_support::{
	migrations::VersionedMigration, pallet_prelude::*, traits::UncheckedOnRuntimeUpgrade,
	weights::Weight,
};
use sp_runtime::VersionedCall;

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

/// The log target.
const TARGET: &str = "runtime::collective::migration::v5";

/// `ProposalOf` viewed with the pre-V5 (bare proposal) value type, for reading pre-migration state.
#[cfg(feature = "try-runtime")]
pub mod v4_storage {
	use super::*;

	#[frame_support::storage_alias]
	pub type ProposalOf<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Identity,
		<T as frame_system::Config>::Hash,
		<T as Config<I>>::Proposal,
		OptionQuery,
	>;
}

/// The raw, version-unchecked migration logic. Prefer [`MigrateToV5`].
pub struct InnerMigrateToV5<T, I = ()>(core::marker::PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for InnerMigrateToV5<T, I> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		let count = v4_storage::ProposalOf::<T, I>::iter_keys().count() as u32;
		log::info!(target: TARGET, "pre_upgrade: {} proposals", count);
		Ok(count.encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let current_version =
			<frame_system::Pallet<T>>::runtime_version().transaction_version;
		let mut count = 0u64;
		ProposalOf::<T, I>::translate::<<T as Config<I>>::Proposal, _>(|_hash, proposal| {
			count = count.saturating_add(1);
			Some(VersionedCall::new(proposal, current_version))
		});
		log::info!(target: TARGET, "Migrated {} proposals to V5", count);
		T::DbWeight::get().reads_writes(count.saturating_add(1), count)
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
		let old_count: u32 =
			Decode::decode(&mut state.as_ref()).expect("pre_upgrade provides a valid u32; qed");
		let new_count = ProposalOf::<T, I>::iter_keys().count() as u32;
		ensure!(old_count == new_count, "Proposal count must be preserved");
		Ok(())
	}
}

/// Version-checked migration from V4 to V5. Add this to a runtime's `Migrations` tuple.
pub type MigrateToV5<T, I = ()> = VersionedMigration<
	4,
	5,
	InnerMigrateToV5<T, I>,
	Pallet<T, I>,
	<T as frame_system::Config>::DbWeight,
>;
