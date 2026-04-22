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
//! Switches storage deposits from the native currency to PGAS.
//!
//! Phase 1 iterates [`CodeInfoOf`] and for each uploaded code records the uploader's existing
//! native `CodeUploadDepositReserve` contribution under [`NativeDepositOf`], keyed by the
//! pallet's own account (the holder of that deposit). The native currency itself stays where it is.
//!
//! Phase 2 iterates [`AccountInfoOf`] and for each contract burn the native
//! `StorageDepositReserve` hold and replaces it with the same amount of PGAS minted into the
//! contract and held under the same reason.

use super::PALLET_MIGRATIONS_ID;
#[cfg(feature = "try-runtime")]
use crate::BalanceOf;
use crate::{
	AccountInfoOf, CodeInfoOf, Config, HoldReason, LOG_TARGET, NativeDepositOf, Pallet,
	address::AddressMapper, deposit_payment::Deposit, storage::AccountType, weights::WeightInfo,
};
use codec::{Decode, Encode, MaxEncodedLen};
use core::marker::PhantomData;
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	weights::WeightMeter,
};
use scale_info::TypeInfo;
use sp_core::{H160, H256};
use sp_runtime::traits::Saturating;

#[cfg(feature = "try-runtime")]
extern crate alloc;

#[cfg(feature = "try-runtime")]
use alloc::{collections::btree_map::BTreeMap, vec::Vec};

/// Two-phase cursor: first code uploads, then contracts.
#[derive(Clone, Encode, Decode, MaxEncodedLen, TypeInfo, PartialEq, Eq, Debug)]
pub enum Cursor {
	/// Last code hash processed in phase 1 (`CodeInfoOf` iteration).
	CodeUpload(H256),
	/// Last contract address processed in phase 2 (`AccountInfoOf` iteration).
	///
	/// `None` is the transition sentinel from phase 1 to phase 2.
	Contract(Option<H160>),
}

/// Switches native storage deposits over to PGAS.
pub struct Migration<T>(PhantomData<T>);

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
		let code_step = <T as Config>::WeightInfo::v4_code_upload_step();
		let contract_step = <T as Config>::WeightInfo::v4_contract_step();
		let required = code_step.max(contract_step);
		if meter.remaining().any_lt(required) {
			return Err(SteppedMigrationError::InsufficientWeight { required });
		}

		loop {
			let step_weight = match &cursor {
				None | Some(Cursor::CodeUpload(_)) => code_step,
				Some(Cursor::Contract(_)) => contract_step,
			};
			if meter.try_consume(step_weight).is_err() {
				break;
			}

			match cursor {
				None | Some(Cursor::CodeUpload(_)) => {
					let last =
						if let Some(Cursor::CodeUpload(h)) = cursor { Some(h) } else { None };
					cursor = Self::step_1_code_upload(last);
				},
				Some(Cursor::Contract(last)) => match Self::step_2_contract(last) {
					Some(next) => cursor = Some(Cursor::Contract(Some(next))),
					None => {
						cursor = None;
						break;
					},
				},
			}
		}
		Ok(cursor)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		use crate::deposit_payment::Deposit;

		let mut per_owner: BTreeMap<T::AccountId, BalanceOf<T>> = BTreeMap::new();
		for (_hash, info) in CodeInfoOf::<T>::iter() {
			let entry = per_owner.entry(info.owner().clone()).or_default();
			*entry = entry.saturating_add(info.deposit());
		}

		let mut per_contract: BTreeMap<H160, BalanceOf<T>> = BTreeMap::new();
		for (addr, info) in AccountInfoOf::<T>::iter() {
			if !matches!(info.account_type, AccountType::Contract(_)) {
				continue;
			}
			let contract = T::AddressMapper::to_account_id(&addr);
			let total = T::Deposit::total_on_hold(HoldReason::StorageDepositReserve, &contract);
			per_contract.insert(addr, total);
		}

		Ok((per_owner, per_contract).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(prev: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use crate::deposit_payment::Deposit;

		let (per_owner, per_contract) = <(
			BTreeMap<T::AccountId, BalanceOf<T>>,
			BTreeMap<H160, BalanceOf<T>>,
		)>::decode(&mut &prev[..])
		.expect("Failed to decode pre_upgrade state");

		// `NativeDepositOf` is introduced in this migration and starts empty.
		let pallet_account = Pallet::<T>::account_id();
		for (owner, expected) in per_owner {
			let got = NativeDepositOf::<T>::get(&pallet_account, &owner);
			assert_eq!(
				got, expected,
				"v4: NativeDepositOf[pallet][{owner:?}] = {got:?}, expected {expected:?}",
			);
		}

		for (addr, expected) in per_contract {
			let contract = T::AddressMapper::to_account_id(&addr);
			let total = T::Deposit::total_on_hold(HoldReason::StorageDepositReserve, &contract);
			assert_eq!(
				total, expected,
				"v4: contract {addr:?} total_on_hold changed: {total:?} != pre-migration {expected:?}",
			);
		}
		Ok(())
	}
}

impl<T: Config> Migration<T> {
	/// Phase 1: credit the next `CodeInfoOf` entry's owner in [`NativeDepositOf`]. Returns
	/// `Some(Cursor::Contract(None))` when phase 1 is exhausted.
	fn step_1_code_upload(last: Option<H256>) -> Option<Cursor> {
		let mut iter = match last {
			Some(last) => CodeInfoOf::<T>::iter_from(CodeInfoOf::<T>::hashed_key_for(last)),
			None => CodeInfoOf::<T>::iter(),
		};

		let Some((hash, info)) = iter.next() else { return Some(Cursor::Contract(None)) };

		let pallet_account = Pallet::<T>::account_id();
		NativeDepositOf::<T>::mutate(&pallet_account, info.owner(), |entitlement| {
			*entitlement = entitlement.saturating_add(info.deposit());
		});
		Some(Cursor::CodeUpload(hash))
	}

	/// Phase 2: hand the next contract to [`Deposit::migrate_native_to_pgas`]. EOAs are
	/// skipped but still advance the cursor.
	fn step_2_contract(last: Option<H160>) -> Option<H160> {
		use frame_support::traits::fungible::InspectHold;

		let mut iter = match last {
			Some(last) => AccountInfoOf::<T>::iter_from(AccountInfoOf::<T>::hashed_key_for(last)),
			None => AccountInfoOf::<T>::iter(),
		};

		let (addr, info) = iter.next()?;
		if matches!(info.account_type, AccountType::Contract(_)) {
			let contract = T::AddressMapper::to_account_id(&addr);
			let held =
				T::Currency::balance_on_hold(&HoldReason::StorageDepositReserve.into(), &contract);
			if let Err(err) = T::Deposit::migrate_native_to_pgas(
				HoldReason::StorageDepositReserve,
				&contract,
				held,
			) {
				log::error!(
					target: LOG_TARGET,
					"v4: failed to migrate native -> PGAS deposit for contract {addr:?}: {err:?}",
				);
			}
		}
		Some(addr)
	}
}

#[cfg(any(feature = "runtime-benchmarks", feature = "try-runtime", test))]
impl<T: Config> Migration<T> {
	/// Drive the migration to completion. Test/benchmark helper.
	pub fn run_to_completion() {
		let mut cursor: Option<Cursor> = None;
		let mut meter = WeightMeter::new();
		while let Ok(Some(next)) = <Self as SteppedMigration>::step(cursor, &mut meter) {
			cursor = Some(next);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		CodeInfo,
		storage::{AccountInfo, ContractInfo},
		tests::{AssetsHolder, ExtBuilder, PGasAssetId, Test},
	};
	use frame_support::traits::fungible::{
		Inspect as _, InspectHold as _, Mutate as _, MutateHold as _,
	};
	use sp_runtime::AccountId32;

	type V4 = Migration<Test>;

	fn seed_code_upload(hash: H256, owner: AccountId32, deposit: u128) {
		let pallet_account = Pallet::<Test>::account_id();
		let ed = <Test as Config>::Currency::minimum_balance();
		<Test as Config>::Currency::mint_into(&pallet_account, ed).unwrap();
		<Test as Config>::Currency::mint_into(&pallet_account, deposit).unwrap();
		<Test as Config>::Currency::hold(
			&HoldReason::CodeUploadDepositReserve.into(),
			&pallet_account,
			deposit,
		)
		.unwrap();
		CodeInfoOf::<Test>::insert(hash, CodeInfo::<Test>::new_with_deposit(owner, deposit));
	}

	fn seed_contract(address: H160, code_hash: H256, storage_deposit: u128) {
		let contract_account = <Test as Config>::AddressMapper::to_account_id(&address);
		let info = ContractInfo::<Test>::new(&address, 0u32.into(), code_hash).unwrap();
		AccountInfoOf::<Test>::insert(
			address,
			AccountInfo::<Test> { account_type: AccountType::Contract(info), dust: 0 },
		);

		let ed = <Test as Config>::Currency::minimum_balance();
		<Test as Config>::Currency::mint_into(&contract_account, ed).unwrap();
		<Test as Config>::Currency::mint_into(&contract_account, storage_deposit).unwrap();
		<Test as Config>::Currency::hold(
			&HoldReason::StorageDepositReserve.into(),
			&contract_account,
			storage_deposit,
		)
		.unwrap();
	}

	#[test]
	fn phase_one_populates_native_deposit_for_code_upload() {
		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let pallet_account = Pallet::<Test>::account_id();
			let owner_a = AccountId32::new([1; 32]);
			let owner_b = AccountId32::new([2; 32]);
			seed_code_upload(H256::repeat_byte(0xAA), owner_a.clone(), 1_000);
			seed_code_upload(H256::repeat_byte(0xAB), owner_a.clone(), 500);
			seed_code_upload(H256::repeat_byte(0xBB), owner_b.clone(), 2_000);

			V4::run_to_completion();

			assert_eq!(
				NativeDepositOf::<Test>::get(&pallet_account, &owner_a),
				1_500,
				"owner_a sum of code deposits"
			);
			assert_eq!(
				NativeDepositOf::<Test>::get(&pallet_account, &owner_b),
				2_000,
				"owner_b sum of code deposits"
			);

			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::CodeUploadDepositReserve.into(),
					&pallet_account,
				),
				3_500,
			);
		});
	}

	#[test]
	fn phase_two_burns_dot_and_mints_pgas_on_contracts() {
		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let owner = AccountId32::new([1; 32]);
			let hash = H256::repeat_byte(0xCC);
			seed_code_upload(hash, owner.clone(), 0);

			let c1 = H160::repeat_byte(0x10);
			let c2 = H160::repeat_byte(0x20);
			seed_contract(c1, hash, 700);
			seed_contract(c2, hash, 1_300);

			let c1_acc = <Test as Config>::AddressMapper::to_account_id(&c1);
			let c2_acc = <Test as Config>::AddressMapper::to_account_id(&c2);

			let total_issuance_before = <Test as Config>::Currency::total_issuance();

			V4::run_to_completion();

			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::StorageDepositReserve.into(),
					&c1_acc,
				),
				0,
			);
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::StorageDepositReserve.into(),
					&c2_acc,
				),
				0,
			);

			assert_eq!(
				total_issuance_before - <Test as Config>::Currency::total_issuance(),
				700 + 1_300,
			);

			use frame_support::traits::tokens::fungibles::InspectHold;
			assert_eq!(
				AssetsHolder::balance_on_hold(
					PGasAssetId::get(),
					&HoldReason::StorageDepositReserve.into(),
					&c1_acc,
				),
				700,
			);
			assert_eq!(
				AssetsHolder::balance_on_hold(
					PGasAssetId::get(),
					&HoldReason::StorageDepositReserve.into(),
					&c2_acc,
				),
				1_300,
			);
		});
	}

	#[test]
	fn eoa_accounts_are_skipped() {
		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let eoa = H160::repeat_byte(0x99);
			AccountInfoOf::<Test>::insert(
				eoa,
				AccountInfo::<Test> { account_type: AccountType::EOA, dust: 0 },
			);

			let owner = AccountId32::new([1; 32]);
			let hash = H256::repeat_byte(0xDD);
			seed_code_upload(hash, owner.clone(), 0);
			let c = H160::repeat_byte(0x30);
			seed_contract(c, hash, 400);

			V4::run_to_completion();

			let c_acc = <Test as Config>::AddressMapper::to_account_id(&c);
			use frame_support::traits::tokens::fungibles::InspectHold;
			assert_eq!(
				AssetsHolder::balance_on_hold(
					PGasAssetId::get(),
					&HoldReason::StorageDepositReserve.into(),
					&c_acc,
				),
				400,
			);
		});
	}
}
