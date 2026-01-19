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

// Benchmarks for Indices Pallet

#![cfg(feature = "runtime-benchmarks")]

use crate::*;
use frame_benchmarking::v2::*;
use frame_support::traits::Get;
use frame_system::RawOrigin;

const SEED: u32 = 0;

#[benchmarks(
	where
		// For the migration benchmark, T::NativeBalance needs to implement the old Currency traits
		// to set up the pre-migration state.
		T::NativeBalance: Currency<T::AccountId, Balance = BalanceOf<T>>
			+ ReservableCurrency<T::AccountId>,
)]
mod benchmarks {
	use super::*;
	use crate::migration::v1::MigrateCurrencyToFungibles;
	use frame_support::{
		migrations::SteppedMigration,
		traits::{
			fungible::{Inspect as FungibleInspect, Mutate as FungibleMutate},
			Currency, ReservableCurrency,
		},
		weights::WeightMeter,
	};

	#[benchmark]
	fn claim() {
		let account_index = T::AccountIndex::from(SEED);
		let caller: T::AccountId = whitelisted_caller();
		<T::NativeBalance as FungibleMutate<T::AccountId>>::set_balance(
			&caller,
			<T::NativeBalance as FungibleInspect<T::AccountId>>::minimum_balance() +
				T::Deposit::get(),
		);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), account_index);

		assert_eq!(Accounts::<T>::get(account_index).unwrap().0, caller);
	}

	#[benchmark]
	fn transfer() -> Result<(), BenchmarkError> {
		let account_index = T::AccountIndex::from(SEED);
		// Setup accounts
		let caller: T::AccountId = whitelisted_caller();
		<T::NativeBalance as FungibleMutate<T::AccountId>>::set_balance(
			&caller,
			<T::NativeBalance as FungibleInspect<T::AccountId>>::minimum_balance() +
				T::Deposit::get(),
		);
		let recipient: T::AccountId = account("recipient", 0, SEED);
		let recipient_lookup = T::Lookup::unlookup(recipient.clone());
		<T::NativeBalance as FungibleMutate<T::AccountId>>::set_balance(
			&recipient,
			<T::NativeBalance as FungibleInspect<T::AccountId>>::minimum_balance() +
				T::Deposit::get(),
		);
		// Claim the index
		Pallet::<T>::claim(RawOrigin::Signed(caller.clone()).into(), account_index)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), recipient_lookup, account_index);

		assert_eq!(Accounts::<T>::get(account_index).unwrap().0, recipient);
		Ok(())
	}

	#[benchmark]
	fn free() -> Result<(), BenchmarkError> {
		let account_index = T::AccountIndex::from(SEED);
		// Setup accounts
		let caller: T::AccountId = whitelisted_caller();
		<T::NativeBalance as FungibleMutate<T::AccountId>>::set_balance(
			&caller,
			<T::NativeBalance as FungibleInspect<T::AccountId>>::minimum_balance() +
				T::Deposit::get(),
		);
		// Claim the index
		Pallet::<T>::claim(RawOrigin::Signed(caller.clone()).into(), account_index)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), account_index);

		assert_eq!(Accounts::<T>::get(account_index), None);
		Ok(())
	}

	#[benchmark]
	fn force_transfer() -> Result<(), BenchmarkError> {
		let account_index = T::AccountIndex::from(SEED);
		// Setup accounts
		let original: T::AccountId = account("original", 0, SEED);
		<T::NativeBalance as FungibleMutate<T::AccountId>>::set_balance(
			&original,
			<T::NativeBalance as FungibleInspect<T::AccountId>>::minimum_balance() +
				T::Deposit::get(),
		);
		let recipient: T::AccountId = account("recipient", 0, SEED);
		let recipient_lookup = T::Lookup::unlookup(recipient.clone());
		<T::NativeBalance as FungibleMutate<T::AccountId>>::set_balance(
			&recipient,
			<T::NativeBalance as FungibleInspect<T::AccountId>>::minimum_balance() +
				T::Deposit::get(),
		);
		// Claim the index
		Pallet::<T>::claim(RawOrigin::Signed(original).into(), account_index)?;

		#[extrinsic_call]
		_(RawOrigin::Root, recipient_lookup, account_index, false);

		assert_eq!(Accounts::<T>::get(account_index).unwrap().0, recipient);
		Ok(())
	}

	#[benchmark]
	fn freeze() -> Result<(), BenchmarkError> {
		let account_index = T::AccountIndex::from(SEED);
		// Setup accounts
		let caller: T::AccountId = whitelisted_caller();
		<T::NativeBalance as FungibleMutate<T::AccountId>>::set_balance(
			&caller,
			<T::NativeBalance as FungibleInspect<T::AccountId>>::minimum_balance() +
				T::Deposit::get(),
		);
		// Claim the index
		Pallet::<T>::claim(RawOrigin::Signed(caller.clone()).into(), account_index)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), account_index);

		assert_eq!(Accounts::<T>::get(account_index).unwrap().2, true);
		Ok(())
	}

	#[benchmark]
	fn poke_deposit() -> Result<(), BenchmarkError> {
		let account_index = T::AccountIndex::from(SEED);
		// Setup accounts
		let caller: T::AccountId = whitelisted_caller();

		// The additional amount we'll add to the deposit for the index
		let additional_amount = 2u32.into();

		<T::NativeBalance as FungibleMutate<T::AccountId>>::set_balance(
			&caller,
			<T::NativeBalance as FungibleInspect<T::AccountId>>::minimum_balance() +
				T::Deposit::get() +
				additional_amount,
		);

		let original_deposit = T::Deposit::get();

		// Claim the index
		Pallet::<T>::claim(RawOrigin::Signed(caller.clone()).into(), account_index)?;

		// Verify the initial deposit amount in storage and held balance
		assert_eq!(Accounts::<T>::get(account_index).unwrap().1, original_deposit);
		assert_eq!(
			T::NativeBalance::balance_on_hold(&HoldReason::DepositForIndex.into(), &caller),
			original_deposit
		);

		// Hold the additional amount from the caller's balance
		T::NativeBalance::hold(&HoldReason::DepositForIndex.into(), &caller, additional_amount)?;

		// Verify the additional amount was held
		assert_eq!(
			T::NativeBalance::balance_on_hold(&HoldReason::DepositForIndex.into(), &caller),
			original_deposit.saturating_add(additional_amount)
		);

		// Increase the deposited amount in storage by additional_amount
		Accounts::<T>::try_mutate(account_index, |maybe_value| -> Result<(), BenchmarkError> {
			let (account, amount, perm) = maybe_value
				.take()
				.ok_or(BenchmarkError::Stop("Mutating storage to change deposits failed"))?;
			*maybe_value = Some((account, amount.saturating_add(additional_amount), perm));
			Ok(())
		})?;

		// Verify the deposit was increased by additional_amount
		assert_eq!(
			Accounts::<T>::get(account_index).unwrap().1,
			original_deposit.saturating_add(additional_amount)
		);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), account_index);

		assert!(Accounts::<T>::contains_key(account_index));
		assert_eq!(Accounts::<T>::get(account_index).unwrap().0, caller);
		assert_eq!(Accounts::<T>::get(account_index).unwrap().1, original_deposit);
		assert_eq!(
			T::NativeBalance::balance_on_hold(&HoldReason::DepositForIndex.into(), &caller),
			original_deposit
		);
		Ok(())
	}

	#[benchmark(extra)]
	fn migrate_account_step() -> Result<(), BenchmarkError> {
		use crate::migration::v1::v0;

		// Setup: Create an account in the OLD currency system that needs migration
		let account_index = T::AccountIndex::from(SEED);
		let caller: T::AccountId = whitelisted_caller();
		let deposit = T::Deposit::get();

		// Give the account some balance (enough for deposit + existential deposit)
		<T::NativeBalance as FungibleMutate<T::AccountId>>::set_balance(
			&caller,
			<T::NativeBalance as FungibleInspect<T::AccountId>>::minimum_balance() +
				deposit + deposit,
		);

		// Reserve funds using the old Currency system
		<T::NativeBalance as ReservableCurrency<T::AccountId>>::reserve(&caller, deposit)?;

		// Insert into the OLD storage (v0) to simulate pre-migration state
		v0::OldAccounts::<T>::insert(account_index, (caller.clone(), deposit, false));

		#[block]
		{
			let _ = MigrateCurrencyToFungibles::<T, T::NativeBalance>::step(
				None,
				&mut WeightMeter::new(),
			);
		}

		// Verify the account was migrated to the new storage
		assert!(Accounts::<T>::contains_key(account_index));
		let (migrated_account, migrated_deposit, frozen) =
			Accounts::<T>::get(account_index).unwrap();
		assert_eq!(migrated_account, caller);
		assert_eq!(migrated_deposit, deposit);
		assert_eq!(frozen, false);

		// Verify the hold was created in the new fungible system
		assert_eq!(
			T::NativeBalance::balance_on_hold(&HoldReason::DepositForIndex.into(), &caller),
			deposit
		);

		Ok(())
	}

	// TODO in another PR: lookup and unlookup trait weights (not critical)

	impl_benchmark_test_suite!(Pallet, mock::new_test_ext(), mock::Test);
}
