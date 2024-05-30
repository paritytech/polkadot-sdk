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

use super::*;
use crate::types::RegionRecord;
use codec::{Decode, Encode};
use core::marker::PhantomData;
use frame_support::traits::{Get, UncheckedOnRuntimeUpgrade};
use sp_runtime::Saturating;

#[cfg(feature = "try-runtime")]
use frame_support::ensure;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

mod v1 {
	use super::*;

	/// V0 region record.
	#[derive(Encode, Decode)]
	struct RegionRecordV0<AccountId, Balance> {
		/// The end of the Region.
		pub end: Timeslice,
		/// The owner of the Region.
		pub owner: AccountId,
		/// The amount paid to Polkadot for this Region, or `None` if renewal is not allowed.
		pub paid: Option<Balance>,
	}

	pub struct MigrateToV1Impl<T>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for MigrateToV1Impl<T> {
		fn on_runtime_upgrade() -> frame_support::weights::Weight {
			let mut count: u64 = 0;

			<Regions<T>>::translate::<RegionRecordV0<T::AccountId, BalanceOf<T>>, _>(|_, v0| {
				count.saturating_inc();
				Some(RegionRecord { end: v0.end, owner: Some(v0.owner), paid: v0.paid })
			});

			log::info!(
				target: LOG_TARGET,
				"Storage migration v1 for pallet-broker finished.",
			);

			// calculate and return migration weights
			T::DbWeight::get().reads_writes(count as u64 + 1, count as u64 + 1)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			Ok((Regions::<T>::iter_keys().count() as u32).encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let old_count = u32::decode(&mut &state[..]).expect("Known good");
			let new_count = Regions::<T>::iter_values().count() as u32;

			ensure!(old_count == new_count, "Regions count should not change");
			Ok(())
		}
	}
}

mod v2 {
	use super::*;
	use frame_support::{
		pallet_prelude::{OptionQuery, Twox64Concat},
		storage_alias,
	};

	#[storage_alias]
	pub type PotentialRenewals<T: Config> = StorageMap<
		Pallet<T>,
		Twox64Concat,
		PotentialRenewalId,
		PotentialRenewalRecordOf<T>,
		OptionQuery,
	>;

	pub struct MigrateToV2Impl<T>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for MigrateToV2Impl<T> {
		fn on_runtime_upgrade() -> frame_support::weights::Weight {
			let mut count = 0;
			for (renewal_id, renewal) in PotentialRenewals::<T>::drain() {
				PotentialRenewals::<T>::insert(renewal_id, renewal);
				count += 1;
			}

			log::info!(
				target: LOG_TARGET,
				"Storage migration v2 for pallet-broker finished.",
			);

			// calculate and return migration weights
			T::DbWeight::get().reads_writes(count as u64 + 1, count as u64 + 1)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			Ok((PotentialRenewals::<T>::iter_keys().count() as u32).encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let old_count = u32::decode(&mut &state[..]).expect("Known good");
			let new_count = PotentialRenewals::<T>::iter_values().count() as u32;

			ensure!(old_count == new_count, "Renewal count should not change");
			Ok(())
		}
	}
}

/// Migrate the pallet storage from `0` to `1`.
pub type MigrateV0ToV1<T> = frame_support::migrations::VersionedMigration<
	0,
	1,
	v1::MigrateToV1Impl<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

pub type MigrateV1ToV2<T> = frame_support::migrations::VersionedMigration<
	1,
	2,
	v2::MigrateToV2Impl<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;
