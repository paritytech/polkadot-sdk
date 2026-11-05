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

//! # Multi-Block Migration v5
//!
//! Re-encodes every [`crate::AccountInfoOf`] contract entry with the new `ed_externally_funded`
//! flag defaulting to `false` (pallet-minted), preserving the historical termination behaviour
//! for existing contracts. `EOA` entries encode identically and are left untouched.

use super::PALLET_MIGRATIONS_ID;
#[cfg(feature = "try-runtime")]
use crate::LOG_TARGET;
use crate::{
	AccountInfoOf, BalanceOf, Config, Pallet, TrieId,
	storage::{AccountInfo, AccountType, ContractInfo},
	weights::WeightInfo,
};
use codec::{Decode, Encode, MaxEncodedLen};
use core::marker::PhantomData;
use frame_support::{
	Identity,
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	storage_alias,
	weights::WeightMeter,
};
use scale_info::TypeInfo;
use sp_core::{H160, H256};

#[cfg(feature = "try-runtime")]
extern crate alloc;
#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;

/// Pre-v5 layouts: `ContractInfo` without `ed_externally_funded`. Parameterised over the balance
/// type so the codec derives only require `Balance: Codec`.
pub(crate) mod old {
	use super::*;

	#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug)]
	pub struct ContractInfo<Balance> {
		pub trie_id: TrieId,
		pub code_hash: H256,
		pub storage_bytes: u32,
		pub storage_items: u32,
		pub storage_byte_deposit: Balance,
		pub storage_item_deposit: Balance,
		pub storage_base_deposit: Balance,
		pub immutable_data_len: u32,
	}

	#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug)]
	pub enum AccountType<AccountId, Balance> {
		Contract(ContractInfo<Balance>),
		EOA,
		DelegatedEOA {
			delegate_target: Option<H160>,
			contract_info: ContractInfo<Balance>,
			payer: Option<AccountId>,
		},
	}

	#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug)]
	pub struct AccountInfo<AccountId, Balance> {
		pub account_type: AccountType<AccountId, Balance>,
		pub dust: u32,
	}

	#[storage_alias]
	/// The storage item that is being migrated. Shares its prefix with [`crate::AccountInfoOf`].
	pub type AccountInfoOf<T: Config> = StorageMap<
		Pallet<T>,
		Identity,
		H160,
		AccountInfo<<T as frame_system::Config>::AccountId, BalanceOf<T>>,
	>;
}

fn migrate_contract<T: Config>(old: old::ContractInfo<BalanceOf<T>>) -> ContractInfo<T> {
	ContractInfo {
		trie_id: old.trie_id,
		code_hash: old.code_hash,
		storage_bytes: old.storage_bytes,
		storage_items: old.storage_items,
		storage_byte_deposit: old.storage_byte_deposit,
		storage_item_deposit: old.storage_item_deposit,
		storage_base_deposit: old.storage_base_deposit,
		immutable_data_len: old.immutable_data_len,
		ed_externally_funded: false,
	}
}

/// Adds `ed_externally_funded: false` to every `ContractInfo`, including delegated accounts'.
pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> SteppedMigration for Migration<T> {
	type Cursor = H160;
	type Identifier = MigrationId<17>;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 4, version_to: 5 }
	}

	fn step(
		mut cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		let required = <T as Config>::WeightInfo::v5_migration_step();
		if meter.remaining().any_lt(required) {
			return Err(SteppedMigrationError::InsufficientWeight { required });
		}

		loop {
			if meter.try_consume(required).is_err() {
				break;
			}

			// Start after the cursor so migrated (new-layout) entries aren't re-read as old.
			let mut iter = if let Some(last_key) = cursor {
				old::AccountInfoOf::<T>::iter_from(old::AccountInfoOf::<T>::hashed_key_for(
					last_key,
				))
			} else {
				old::AccountInfoOf::<T>::iter()
			};

			if let Some((key, old_info)) = iter.next() {
				let account_type = match old_info.account_type {
					old::AccountType::Contract(contract) => {
						Some(AccountType::Contract(migrate_contract::<T>(contract)))
					},
					old::AccountType::DelegatedEOA { delegate_target, contract_info, payer } => {
						Some(AccountType::DelegatedEOA {
							delegate_target,
							contract_info: migrate_contract::<T>(contract_info),
							payer,
						})
					},
					// `EOA` entries encode identically, so leave them untouched.
					old::AccountType::EOA => None,
				};
				if let Some(account_type) = account_type {
					AccountInfoOf::<T>::insert(
						key,
						AccountInfo::<T> { account_type, dust: old_info.dust },
					);
				}
				cursor = Some(key);
			} else {
				cursor = None;
				break;
			}
		}
		Ok(cursor)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		let contracts: u32 = old::AccountInfoOf::<T>::iter_values()
			.filter(|info| !matches!(info.account_type, old::AccountType::EOA))
			.count() as u32;
		log::info!(target: LOG_TARGET, "v5: {contracts} contracts to migrate");
		Ok(contracts.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(prev: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let prev_contracts =
			u32::decode(&mut &prev[..]).expect("Failed to decode pre_upgrade state");

		// Every contract must now decode with the new layout and default to pallet-minted.
		let mut contracts = 0u32;
		for (addr, info) in AccountInfoOf::<T>::iter() {
			let contract = match info.account_type {
				AccountType::Contract(contract) => Some(contract),
				AccountType::DelegatedEOA { contract_info, .. } => Some(contract_info),
				AccountType::EOA => None,
			};
			if let Some(contract) = contract {
				contracts += 1;
				assert!(
					!contract.ed_externally_funded,
					"v5: contract {addr:?} must default to pallet-minted ED",
				);
			}
		}
		assert_eq!(
			contracts, prev_contracts,
			"v5: migrated {contracts} contracts, expected {prev_contracts}",
		);
		Ok(())
	}
}

#[cfg(any(feature = "runtime-benchmarks", feature = "try-runtime", test))]
impl<T: Config> Migration<T> {
	/// Drive the migration to completion. Test/benchmark helper.
	pub fn run_to_completion() {
		let mut cursor: Option<H160> = None;
		let mut meter = WeightMeter::new();
		while let Ok(Some(next)) = <Self as SteppedMigration>::step(cursor, &mut meter) {
			cursor = Some(next);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::tests::{ExtBuilder, Test};

	type V5 = Migration<Test>;
	type OldAccountInfo = old::AccountInfo<<Test as frame_system::Config>::AccountId, u128>;

	fn seed_old_contract(address: H160, code_hash: H256, immutable_data_len: u32) {
		let trie_id: TrieId = vec![0xAB; 16].try_into().unwrap();
		old::AccountInfoOf::<Test>::insert(
			address,
			OldAccountInfo {
				account_type: old::AccountType::Contract(old::ContractInfo::<u128> {
					trie_id,
					code_hash,
					storage_bytes: 1,
					storage_items: 2,
					storage_byte_deposit: 3,
					storage_item_deposit: 4,
					storage_base_deposit: 5,
					immutable_data_len,
				}),
				dust: 7,
			},
		);
	}

	#[test]
	fn migrate_to_v5_adds_flag_defaulting_to_false() {
		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let c1 = H160::repeat_byte(0x10);
			let c2 = H160::repeat_byte(0x20);
			let eoa = H160::repeat_byte(0x30);
			let hash = H256::repeat_byte(0xCC);

			seed_old_contract(c1, hash, 0);
			seed_old_contract(c2, hash, 42);
			// An EOA entry that must be left untouched.
			old::AccountInfoOf::<Test>::insert(
				eoa,
				OldAccountInfo { account_type: old::AccountType::EOA, dust: 9 },
			);

			V5::run_to_completion();

			let info1 = AccountInfoOf::<Test>::get(c1).unwrap();
			let AccountType::Contract(contract1) = info1.account_type else {
				panic!("c1 should still be a contract");
			};
			assert!(!contract1.ed_externally_funded, "default must be pallet-minted");
			assert_eq!(contract1.immutable_data_len, 0);
			assert_eq!(contract1.storage_base_deposit, 5);
			assert_eq!(info1.dust, 7, "dust must be preserved");

			let info2 = AccountInfoOf::<Test>::get(c2).unwrap();
			let AccountType::Contract(contract2) = info2.account_type else {
				panic!("c2 should still be a contract");
			};
			assert!(!contract2.ed_externally_funded);
			assert_eq!(contract2.immutable_data_len, 42);

			let eoa_info = AccountInfoOf::<Test>::get(eoa).unwrap();
			assert!(matches!(eoa_info.account_type, AccountType::EOA));
			assert_eq!(eoa_info.dust, 9);
		});
	}

	#[test]
	fn migrate_to_v5_keeps_delegated_accounts() {
		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let addr = H160::repeat_byte(0x40);
			let target = H160::repeat_byte(0x41);
			let contract = old::ContractInfo::<u128> {
				trie_id: vec![0xAB; 16].try_into().unwrap(),
				code_hash: H256::repeat_byte(0xCC),
				storage_bytes: 1,
				storage_items: 2,
				storage_byte_deposit: 3,
				storage_item_deposit: 4,
				storage_base_deposit: 5,
				immutable_data_len: 0,
			};
			// Spelled out as bytes to pin the pre-v5 encoding: variant 2 is `DelegatedEOA`.
			let raw = (2u8, Some(target), contract, Some(crate::test_utils::ALICE), 9u32).encode();
			frame_support::storage::unhashed::put_raw(
				&old::AccountInfoOf::<Test>::hashed_key_for(addr),
				&raw,
			);

			V5::run_to_completion();

			let info = AccountInfoOf::<Test>::get(addr).expect("delegated entry must still decode");
			let AccountType::DelegatedEOA { delegate_target, contract_info, payer } =
				info.account_type
			else {
				panic!("should still be delegated");
			};
			assert_eq!(delegate_target, Some(target));
			assert_eq!(payer, Some(crate::test_utils::ALICE));
			assert_eq!(contract_info.storage_base_deposit, 5);
			assert!(!contract_info.ed_externally_funded);
			assert_eq!(info.dust, 9);
		});
	}

	#[test]
	fn migrate_to_v5_resumes_across_blocks() {
		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let hash = H256::repeat_byte(0xCC);
			let contracts = [
				(H160::repeat_byte(0x10), 0u32),
				(H160::repeat_byte(0x20), 42),
				(H160::repeat_byte(0x30), 7),
			];
			for (addr, immutable_data_len) in contracts {
				seed_old_contract(addr, hash, immutable_data_len);
			}

			// Budget each step for exactly one entry, feeding the cursor back in, as the MBM
			// runner does under a real per-block weight limit.
			let step = <Test as Config>::WeightInfo::v5_migration_step();
			let mut cursor: Option<H160> = None;
			let mut steps = 0u32;
			loop {
				let mut meter = WeightMeter::with_limit(step);
				match <V5 as SteppedMigration>::step(cursor, &mut meter).unwrap() {
					Some(next) => {
						cursor = Some(next);
						steps += 1;
						// A cursor that fails to advance would otherwise spin forever.
						assert!(steps <= contracts.len() as u32, "cursor is not advancing");
					},
					None => break,
				}
			}
			assert!(steps > 1, "migration should span multiple steps, took {steps}");

			// Final state must match a one-shot `run_to_completion`.
			for (addr, immutable_data_len) in contracts {
				let info = AccountInfoOf::<Test>::get(addr).unwrap();
				let AccountType::Contract(contract) = info.account_type else {
					panic!("{addr:?} should still be a contract");
				};
				assert!(!contract.ed_externally_funded);
				assert_eq!(contract.immutable_data_len, immutable_data_len);
				assert_eq!(info.dust, 7, "dust must be preserved");
			}
		});
	}
}
