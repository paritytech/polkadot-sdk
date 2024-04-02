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

use crate::{BoundedAuthorityList, Pallet};
use codec::Decode;
use frame_support::{
	migrations::VersionedMigration,
	storage,
	traits::{Get, UncheckedOnRuntimeUpgrade},
	weights::Weight,
};
use sp_consensus_grandpa::AuthorityList;
use sp_std::{marker::PhantomData, vec::Vec};

const GRANDPA_AUTHORITIES_KEY: &[u8] = b":grandpa_authorities";

fn load_authority_list() -> AuthorityList {
	storage::unhashed::get_raw(GRANDPA_AUTHORITIES_KEY).map_or_else(
		|| Vec::new(),
		|l| <(u8, AuthorityList)>::decode(&mut &l[..]).unwrap_or_default().1,
	)
}

/// Actual implementation of [`MigrateV4ToV5`].
pub struct UncheckedMigrateImpl<T>(PhantomData<T>);

impl<T: crate::Config> UncheckedOnRuntimeUpgrade for UncheckedMigrateImpl<T> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		use codec::Encode;

		let authority_list_len = load_authority_list().len() as u32;

		if authority_list_len > T::MaxAuthorities::get() {
			return Err(
				"Grandpa: `Config::MaxAuthorities` is smaller than the actual number of authorities.".into()
			)
		}

		if authority_list_len == 0 {
			return Err("Grandpa: Authority list is empty!".into())
		}

		Ok(authority_list_len.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let len = u32::decode(&mut &state[..]).unwrap();

		frame_support::ensure!(
			len == crate::Pallet::<T>::grandpa_authorities().len() as u32,
			"Grandpa: pre-migrated and post-migrated list should have the same length"
		);

		frame_support::ensure!(
			load_authority_list().is_empty(),
			"Old authority list shouldn't exist anymore"
		);

		Ok(())
	}

	fn on_runtime_upgrade() -> Weight {
		crate::Authorities::<T>::put(
			&BoundedAuthorityList::<T::MaxAuthorities>::force_from(
				load_authority_list(),
				Some("Grandpa: `Config::MaxAuthorities` is smaller than the actual number of authorities.")
			)
		);

		storage::unhashed::kill(GRANDPA_AUTHORITIES_KEY);

		T::DbWeight::get().reads_writes(1, 2)
	}
}

/// Migrate the storage from V4 to V5.
///
/// Switches from `GRANDPA_AUTHORITIES_KEY` to a normal FRAME storage item.
pub type MigrateV4ToV5<T> = VersionedMigration<
	4,
	5,
	UncheckedMigrateImpl<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;
