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

//! # Multi-Block Migration v4
//!
//! Introduces PGAS-backed storage deposits. Two storage items change shape and are migrated here:
//!
//! - `AccountInfoOf`: `ContractInfo` gains a `historic_deposit` field, seeded with the contract's
//!   current `total_deposit()`. Pre-PGAS deposits are always DOT-backed and this field drives the
//!   refund routing on the legacy balance.
//! - `CodeInfoOf`: `CodeInfo` gains a `deposit_asset` field, seeded with `DotConvertible` since all
//!   pre-migration code upload deposits were held in the native currency.
//!
//! The migration iterates `AccountInfoOf` first then `CodeInfoOf`, using a two-phase cursor.

extern crate alloc;

use super::PALLET_MIGRATIONS_ID;
use crate::{
	AccountIdOf, BalanceOf, Config, DepositAsset, H160, H256, TrieId, pallet::Pallet,
	vm::BytecodeType, weights::WeightInfo,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	CloneNoBound, DebugNoBound, Identity,
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	pallet_prelude::PhantomData,
	storage_alias,
	weights::WeightMeter,
};
use sp_runtime::Saturating;

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;

/// Module containing the old storage items.
pub mod old {
	use super::*;

	#[derive(CloneNoBound, DebugNoBound, PartialEq, Eq, Encode, Decode)]
	pub struct ContractInfo<T: Config> {
		pub trie_id: TrieId,
		pub code_hash: H256,
		pub storage_bytes: u32,
		pub storage_items: u32,
		pub storage_byte_deposit: BalanceOf<T>,
		pub storage_item_deposit: BalanceOf<T>,
		pub storage_base_deposit: BalanceOf<T>,
		pub immutable_data_len: u32,
	}

	impl<T: Config> ContractInfo<T> {
		pub fn total_deposit(&self) -> BalanceOf<T> {
			self.storage_byte_deposit
				.saturating_add(self.storage_item_deposit)
				.saturating_add(self.storage_base_deposit)
		}
	}

	#[derive(CloneNoBound, DebugNoBound, PartialEq, Eq, Encode, Decode, Default)]
	pub enum AccountType<T: Config> {
		Contract(ContractInfo<T>),
		#[default]
		EOA,
	}

	#[derive(CloneNoBound, DebugNoBound, PartialEq, Eq, Encode, Decode, Default)]
	pub struct AccountInfo<T: Config> {
		pub account_type: AccountType<T>,
		pub dust: u32,
	}

	#[derive(CloneNoBound, DebugNoBound, PartialEq, Eq, Encode, Decode)]
	pub struct CodeInfo<T: Config> {
		pub owner: AccountIdOf<T>,
		#[codec(compact)]
		pub deposit: BalanceOf<T>,
		#[codec(compact)]
		pub refcount: u64,
		pub code_len: u32,
		pub code_type: BytecodeType,
		pub behaviour_version: u32,
	}

	#[storage_alias]
	pub type AccountInfoOf<T: Config> = StorageMap<Pallet<T>, Identity, H160, AccountInfo<T>>;

	#[storage_alias]
	pub type CodeInfoOf<T: Config> = StorageMap<Pallet<T>, Identity, H256, CodeInfo<T>>;
}

/// Cursor tracking which storage map we are currently iterating.
#[derive(
	Clone, Encode, Decode, MaxEncodedLen, DebugNoBound, PartialEq, Eq, scale_info::TypeInfo,
)]
pub enum Cursor {
	/// Iterating `AccountInfoOf`; the last processed key is `Some(addr)` when not at the start.
	AccountInfo(Option<H160>),
	/// Iterating `CodeInfoOf`; the last processed key is `Some(hash)` when not at the start.
	CodeInfo(Option<H256>),
}

impl Default for Cursor {
	fn default() -> Self {
		Cursor::AccountInfo(None)
	}
}

/// Migrates `AccountInfoOf` (seeding `historic_deposit`) and `CodeInfoOf` (seeding
/// `deposit_asset`) to their PGAS-aware shapes.
pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> SteppedMigration for Migration<T> {
	type Cursor = Cursor;
	type Identifier = MigrationId<17>;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 3, version_to: 4 }
	}

	fn step(
		mut cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		let required = <T as Config>::WeightInfo::v4_migration_step();
		if meter.remaining().any_lt(required) {
			return Err(SteppedMigrationError::InsufficientWeight { required });
		}

		loop {
			if meter.try_consume(required).is_err() {
				break;
			}

			cursor = match cursor.unwrap_or_default() {
				Cursor::AccountInfo(last_key) => {
					let mut iter = if let Some(key) = last_key {
						old::AccountInfoOf::<T>::iter_from(old::AccountInfoOf::<T>::hashed_key_for(
							key,
						))
					} else {
						old::AccountInfoOf::<T>::iter()
					};

					if let Some((addr, old_info)) = iter.next() {
						let new_info = migrate_account_info::<T>(old_info);
						crate::AccountInfoOf::<T>::insert(addr, new_info);
						Some(Cursor::AccountInfo(Some(addr)))
					} else {
						Some(Cursor::CodeInfo(None))
					}
				},
				Cursor::CodeInfo(last_key) => {
					let mut iter = if let Some(key) = last_key {
						old::CodeInfoOf::<T>::iter_from(old::CodeInfoOf::<T>::hashed_key_for(key))
					} else {
						old::CodeInfoOf::<T>::iter()
					};

					if let Some((hash, old_info)) = iter.next() {
						let new_info = migrate_code_info::<T>(old_info);
						crate::CodeInfoOf::<T>::insert(hash, new_info);
						Some(Cursor::CodeInfo(Some(hash)))
					} else {
						None
					}
				},
			};

			if cursor.is_none() {
				break;
			}
		}

		Ok(cursor)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		use alloc::collections::btree_map::BTreeMap;

		let accounts: BTreeMap<H160, old::AccountInfo<T>> =
			old::AccountInfoOf::<T>::iter().collect();
		let codes: BTreeMap<H256, old::CodeInfo<T>> = old::CodeInfoOf::<T>::iter().collect();
		Ok((accounts, codes).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(prev: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use alloc::collections::btree_map::BTreeMap;

		let (prev_accounts, prev_codes): (
			BTreeMap<H160, old::AccountInfo<T>>,
			BTreeMap<H256, old::CodeInfo<T>>,
		) = Decode::decode(&mut &prev[..]).expect("Failed to decode pre_upgrade state");

		assert_eq!(
			crate::AccountInfoOf::<T>::iter().count(),
			prev_accounts.len(),
			"v4: AccountInfoOf count changed"
		);
		assert_eq!(
			crate::CodeInfoOf::<T>::iter().count(),
			prev_codes.len(),
			"v4: CodeInfoOf count changed"
		);

		for (addr, old_info) in prev_accounts {
			let migrated = crate::AccountInfoOf::<T>::get(addr).expect("account missing post v4");
			assert_eq!(
				migrated.encode(),
				migrate_account_info::<T>(old_info).encode(),
				"v4: AccountInfo mismatch",
			);
		}
		for (hash, old_info) in prev_codes {
			let migrated = crate::CodeInfoOf::<T>::get(hash).expect("code missing post v4");
			assert_eq!(
				migrated.encode(),
				migrate_code_info::<T>(old_info).encode(),
				"v4: CodeInfo mismatch",
			);
		}

		Ok(())
	}
}

/// Map an old `AccountInfo` into the new shape, seeding `historic_deposit` with the contract's
/// current total deposit.
fn migrate_account_info<T: Config>(old_info: old::AccountInfo<T>) -> crate::AccountInfo<T> {
	let account_type = match old_info.account_type {
		old::AccountType::Contract(c) => {
			let historic_deposit = c.total_deposit();
			crate::AccountType::Contract(crate::ContractInfo {
				trie_id: c.trie_id,
				code_hash: c.code_hash,
				storage_bytes: c.storage_bytes,
				storage_items: c.storage_items,
				storage_byte_deposit: c.storage_byte_deposit,
				storage_item_deposit: c.storage_item_deposit,
				storage_base_deposit: c.storage_base_deposit,
				historic_deposit,
				immutable_data_len: c.immutable_data_len,
			})
		},
		old::AccountType::EOA => crate::AccountType::EOA,
	};
	crate::AccountInfo { account_type, dust: old_info.dust }
}

/// Map an old `CodeInfo` into the new shape, seeding `deposit_asset` with `DotConvertible`.
fn migrate_code_info<T: Config>(old_info: old::CodeInfo<T>) -> crate::CodeInfo<T> {
	// `CodeInfo` fields are private, so go through the scale-codec: append the new field's
	// encoding to the old encoding and decode into the new type.
	let mut bytes = old_info.encode();
	DepositAsset::DotConvertible.encode_to(&mut bytes);
	crate::CodeInfo::<T>::decode(&mut &bytes[..])
		.expect("new CodeInfo differs from old only by a trailing DepositAsset field; qed")
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		AccountIdOf, ContractInfo,
		tests::{ExtBuilder, Test},
	};
	use sp_runtime::AccountId32;

	fn old_contract_info() -> old::ContractInfo<Test> {
		old::ContractInfo {
			trie_id: Default::default(),
			code_hash: H256::from([7u8; 32]),
			storage_bytes: 10,
			storage_items: 2,
			storage_byte_deposit: 100u32.into(),
			storage_item_deposit: 50u32.into(),
			storage_base_deposit: 25u32.into(),
			immutable_data_len: 0,
		}
	}

	#[test]
	fn migrates_contract_account_info() {
		ExtBuilder::default().build().execute_with(|| {
			let addr = H160::from([1u8; 20]);
			let old = old_contract_info();
			old::AccountInfoOf::<Test>::insert(
				addr,
				old::AccountInfo { account_type: old::AccountType::Contract(old.clone()), dust: 3 },
			);

			let mut cursor = None;
			let mut meter = WeightMeter::new();
			while let Some(new_cursor) = Migration::<Test>::step(cursor, &mut meter).unwrap() {
				cursor = Some(new_cursor);
			}

			let migrated = crate::AccountInfoOf::<Test>::get(addr).unwrap();
			let crate::AccountType::Contract(info) = migrated.account_type else {
				panic!("expected contract");
			};
			assert_eq!(migrated.dust, 3);
			assert_eq!(info.historic_deposit, old.total_deposit());
			assert_eq!(info.storage_byte_deposit, old.storage_byte_deposit);
			assert_eq!(info.storage_item_deposit, old.storage_item_deposit);
			assert_eq!(info.storage_base_deposit, old.storage_base_deposit);
			assert_eq!(info.code_hash, old.code_hash);
		});
	}

	#[test]
	fn migrates_eoa_account_info_unchanged_deposit() {
		ExtBuilder::default().build().execute_with(|| {
			let addr = H160::from([2u8; 20]);
			old::AccountInfoOf::<Test>::insert(
				addr,
				old::AccountInfo { account_type: old::AccountType::EOA, dust: 42 },
			);

			let mut cursor = None;
			let mut meter = WeightMeter::new();
			while let Some(new_cursor) = Migration::<Test>::step(cursor, &mut meter).unwrap() {
				cursor = Some(new_cursor);
			}

			let migrated = crate::AccountInfoOf::<Test>::get(addr).unwrap();
			assert!(matches!(migrated.account_type, crate::AccountType::EOA));
			assert_eq!(migrated.dust, 42);
		});
	}

	#[test]
	fn migrates_code_info_to_dot_convertible() {
		ExtBuilder::default().build().execute_with(|| {
			let hash = H256::from([9u8; 32]);
			let owner: AccountIdOf<Test> = AccountId32::new([5u8; 32]);
			let old_info = old::CodeInfo::<Test> {
				owner: owner.clone(),
				deposit: 1_000u32.into(),
				refcount: 3,
				code_len: 100,
				code_type: BytecodeType::Pvm,
				behaviour_version: 0,
			};
			old::CodeInfoOf::<Test>::insert(hash, old_info.clone());

			let mut cursor = None;
			let mut meter = WeightMeter::new();
			while let Some(new_cursor) = Migration::<Test>::step(cursor, &mut meter).unwrap() {
				cursor = Some(new_cursor);
			}

			let migrated = crate::CodeInfoOf::<Test>::get(hash).unwrap();
			assert_eq!(migrated.deposit(), old_info.deposit);
			// `deposit_asset` is private; the encoded form should match a CodeInfo whose tail is
			// DotConvertible.
			let expected_tail = DepositAsset::DotConvertible.encode();
			let encoded = migrated.encode();
			assert!(encoded.ends_with(&expected_tail));
		});
	}

	#[test]
	fn migrates_both_maps_in_sequence() {
		ExtBuilder::default().build().execute_with(|| {
			for i in 0..5u8 {
				let addr = H160::from([i; 20]);
				old::AccountInfoOf::<Test>::insert(
					addr,
					old::AccountInfo {
						account_type: old::AccountType::Contract(old_contract_info()),
						dust: 0,
					},
				);
			}
			for i in 0..3u8 {
				let hash = H256::from([i + 100; 32]);
				old::CodeInfoOf::<Test>::insert(
					hash,
					old::CodeInfo {
						owner: AccountId32::new([i; 32]),
						deposit: 100u32.into(),
						refcount: 1,
						code_len: 10,
						code_type: BytecodeType::Pvm,
						behaviour_version: 0,
					},
				);
			}

			let mut cursor = None;
			let mut meter = WeightMeter::new();
			while let Some(new_cursor) = Migration::<Test>::step(cursor, &mut meter).unwrap() {
				cursor = Some(new_cursor);
			}

			assert_eq!(crate::AccountInfoOf::<Test>::iter().count(), 5);
			assert_eq!(crate::CodeInfoOf::<Test>::iter().count(), 3);
			for (_, info) in crate::AccountInfoOf::<Test>::iter() {
				if let crate::AccountType::Contract(c) = info.account_type {
					assert_eq!(c.historic_deposit, ContractInfo::<Test>::total_deposit(&c));
				}
			}
		});
	}
}
