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

use super::super::LOG_TARGET;
use frame_support::{
	traits::{OnRuntimeUpgrade, StorageVersion},
	weights::Weight,
};

pub mod v5 {
	use super::*;
	use crate::Pallet;
	use frame_support::{pallet_prelude::*, storage_alias};
	use sp_runtime::VersionedCall;

	// The old storage type - this stores raw Proposal
	#[storage_alias]
	type OldProposalOf<T: crate::Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Identity,
		<T as frame_system::Config>::Hash,
		<T as crate::Config<I>>::Proposal,
		OptionQuery,
	>;

	pub struct MigrateToVersionedCall<T, I = ()>(core::marker::PhantomData<(T, I)>);

	impl<T: crate::Config<I> + frame_system::Config, I: 'static> OnRuntimeUpgrade
		for MigrateToVersionedCall<T, I>
	{
		fn on_runtime_upgrade() -> Weight {
			let current_version = StorageVersion::get::<Pallet<T, I>>();
			let mut weight = T::DbWeight::get().reads(1);

			if current_version < 5 {
				log::info!(
					target: LOG_TARGET,
					"Migrating collective proposals to VersionedCall"
				);

				// Get all old proposals
				let old_proposals: Vec<_> = OldProposalOf::<T, I>::iter().collect();
				let count = old_proposals.len() as u64;

				// Clear old storage - handle the result
				let _ = OldProposalOf::<T, I>::clear(u32::MAX, None);
				weight.saturating_accrue(T::DbWeight::get().reads_writes(count, count));

				// Get current transaction version
				let current_tx_version =
					<frame_system::Pallet<T>>::runtime_version().transaction_version;

				// Insert migrated proposals into new storage
				for (hash, old_proposal) in old_proposals {
					let versioned_proposal = VersionedCall::new(old_proposal, current_tx_version);

					// Use the new ProposalOf storage directly
					crate::ProposalOf::<T, I>::insert(hash, versioned_proposal);
					weight.saturating_accrue(T::DbWeight::get().reads_writes(0, 1));
				}

				StorageVersion::new(5).put::<Pallet<T, I>>();
				weight.saturating_accrue(T::DbWeight::get().writes(1));

				log::info!(
					target: LOG_TARGET,
					"Migrated {} proposals to VersionedCall",
					count
				);

				weight
			} else {
				log::info!(
					target: LOG_TARGET,
					"Migration to VersionedCall already applied"
				);
				weight
			}
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			let count = OldProposalOf::<T, I>::iter_keys().count();
			Ok(count.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			use codec::Decode;
			let old_count: usize = Decode::decode(&mut &state[..])?;
			let new_count = crate::ProposalOf::<T, I>::iter_keys().count();

			assert_eq!(old_count, new_count, "All proposals should be migrated");
			assert_eq!(StorageVersion::get::<Pallet<T, I>>(), 5);

			log::info!(
				target: LOG_TARGET,
				"Successfully migrated {} proposals to VersionedCall",
				new_count
			);

			Ok(())
		}
	}
}
