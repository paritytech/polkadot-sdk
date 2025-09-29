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

//! Storage migrations for the Identity pallet.

extern crate alloc;

use super::*;
use frame_support::{
	migrations::VersionedMigration, pallet_prelude::*, storage_alias,
	traits::UncheckedOnRuntimeUpgrade, IterableStorageMap,
};

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(any(test, feature = "try-runtime"))]
use {alloc::collections::BTreeMap, sp_runtime::TryRuntimeError};

pub mod v1;
pub mod v2;
pub mod v3;

pub const PALLET_MIGRATIONS_ID: &[u8; 15] = b"pallet-identity";

pub mod versioned {
	use super::*;

	pub type V0ToV1<T, const KL: u64> = VersionedMigration<
		0,
		1,
		v1::VersionUncheckedMigrateV0ToV1<T, KL>,
		crate::pallet::Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}

/// The old identity types in v0.
mod types_v0 {
	use super::*;

	#[storage_alias]
	pub type IdentityOf<T: Config> = StorageMap<
		Pallet<T>,
		Twox64Concat,
		<T as frame_system::Config>::AccountId,
		Registration<
			BalanceOf<T>,
			<T as pallet::Config>::MaxRegistrars,
			<T as pallet::Config>::IdentityInformation,
		>,
		OptionQuery,
	>;
}

/// The old identity types in v1.
mod types_v1 {
	use super::*;

	#[storage_alias]
	pub type IdentityOf<T: Config> = StorageMap<
		Pallet<T>,
		Twox64Concat,
		<T as frame_system::Config>::AccountId,
		(
			Registration<
				BalanceOf<T>,
				<T as pallet::Config>::MaxRegistrars,
				<T as pallet::Config>::IdentityInformation,
			>,
			Option<Username<T>>,
		),
		OptionQuery,
	>;

	#[storage_alias]
	pub type UsernameAuthorities<T: Config> = StorageMap<
		Pallet<T>,
		Twox64Concat,
		<T as frame_system::Config>::AccountId,
		AuthorityProperties<Suffix<T>>,
		OptionQuery,
	>;

	#[storage_alias]
	pub type AccountOfUsername<T: Config> = StorageMap<
		Pallet<T>,
		Blake2_128Concat,
		Username<T>,
		<T as frame_system::Config>::AccountId,
		OptionQuery,
	>;

	#[cfg(feature = "try-runtime")]
	#[storage_alias]
	pub type PendingUsernames<T: Config> = StorageMap<
		Pallet<T>,
		Blake2_128Concat,
		Username<T>,
		(<T as frame_system::Config>::AccountId, BlockNumberFor<T>),
		OptionQuery,
	>;
}
