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

//! Remove ED from storage base deposit.
//! See <https://github.com/paritytech/polkadot-sdk/pull/3536>.

use crate::{
	migration::{IsFinished, MigrationStep},
	weights::WeightInfo,
	BalanceOf, CodeHash, Config, Pallet, TrieId, Weight, WeightMeter, LOG_TARGET,
};
use codec::{Decode, Encode};
use frame_support::{pallet_prelude::*, storage_alias, DefaultNoBound};
use sp_runtime::{BoundedBTreeMap, Saturating};
use sp_std::prelude::*;

#[cfg(feature = "runtime-benchmarks")]
pub fn store_old_contract_info<T: Config>(
	account: T::AccountId,
	info: &crate::ContractInfo<T>,
) -> BalanceOf<T> {
	let storage_base_deposit = Pallet::<T>::min_balance() + 1u32.into();
	ContractInfoOf::<T>::insert(
		account,
		ContractInfo {
			trie_id: info.trie_id.clone(),
			code_hash: info.code_hash,
			storage_bytes: Default::default(),
			storage_items: Default::default(),
			storage_byte_deposit: Default::default(),
			storage_item_deposit: Default::default(),
			storage_base_deposit,
			delegate_dependencies: Default::default(),
		},
	);

	storage_base_deposit
}

#[storage_alias]
pub type ContractInfoOf<T: Config> =
	StorageMap<Pallet<T>, Twox64Concat, <T as frame_system::Config>::AccountId, ContractInfo<T>>;

#[derive(Encode, Decode, CloneNoBound, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct ContractInfo<T: Config> {
	trie_id: TrieId,
	code_hash: CodeHash<T>,
	storage_bytes: u32,
	storage_items: u32,
	storage_byte_deposit: BalanceOf<T>,
	storage_item_deposit: BalanceOf<T>,
	pub storage_base_deposit: BalanceOf<T>,
	delegate_dependencies: BoundedBTreeMap<CodeHash<T>, BalanceOf<T>, T::MaxDelegateDependencies>,
}

#[derive(Encode, Decode, MaxEncodedLen, DefaultNoBound)]
pub struct Migration<T: Config> {
	last_account: Option<T::AccountId>,
}

impl<T: Config> MigrationStep for Migration<T> {
	const VERSION: u16 = 16;

	fn max_step_weight() -> Weight {
		T::WeightInfo::v16_migration_step()
	}

	fn step(&mut self, meter: &mut WeightMeter) -> IsFinished {
		let mut iter = if let Some(last_account) = self.last_account.take() {
			ContractInfoOf::<T>::iter_keys_from(ContractInfoOf::<T>::hashed_key_for(last_account))
		} else {
			ContractInfoOf::<T>::iter_keys()
		};

		if let Some(key) = iter.next() {
			log::debug!(target: LOG_TARGET, "Migrating contract {:?}", key);
			ContractInfoOf::<T>::mutate(key.clone(), |info| {
				let ed = Pallet::<T>::min_balance();
				let mut updated_info = info.take().expect("Item exists; qed");
				updated_info.storage_base_deposit.saturating_reduce(ed);
				*info = Some(updated_info);
			});
			self.last_account = Some(key);
			meter.consume(T::WeightInfo::v16_migration_step());
			IsFinished::No
		} else {
			log::debug!(target: LOG_TARGET, "No more contracts to migrate");
			meter.consume(T::WeightInfo::v16_migration_step());
			IsFinished::Yes
		}
	}
}
