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
use sp_runtime::traits::Bounded;

const SEED: u32 = 0;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn claim() {
		let account_index = T::AccountIndex::from(SEED);
		let caller: T::AccountId = whitelisted_caller();
		T::Currency::make_free_balance_be(&caller, BalanceOf::<T>::max_value());

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), account_index);

		assert_eq!(Accounts::<T>::get(account_index).unwrap().0, caller);
	}

	#[benchmark]
	fn transfer() -> Result<(), BenchmarkError> {
		let account_index = T::AccountIndex::from(SEED);
		// Setup accounts
		let caller: T::AccountId = whitelisted_caller();
		T::Currency::make_free_balance_be(&caller, BalanceOf::<T>::max_value());
		let recipient: T::AccountId = account("recipient", 0, SEED);
		let recipient_lookup = T::Lookup::unlookup(recipient.clone());
		T::Currency::make_free_balance_be(&recipient, BalanceOf::<T>::max_value());
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
		T::Currency::make_free_balance_be(&caller, BalanceOf::<T>::max_value());
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
		T::Currency::make_free_balance_be(&original, BalanceOf::<T>::max_value());
		let recipient: T::AccountId = account("recipient", 0, SEED);
		let recipient_lookup = T::Lookup::unlookup(recipient.clone());
		T::Currency::make_free_balance_be(&recipient, BalanceOf::<T>::max_value());
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
		T::Currency::make_free_balance_be(&caller, BalanceOf::<T>::max_value());
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
		T::Currency::make_free_balance_be(&caller, BalanceOf::<T>::max_value());

		let original_deposit = T::Deposit::get();

		// Claim the index
		Pallet::<T>::claim(RawOrigin::Signed(caller.clone()).into(), account_index)?;

		// Verify the initial deposit amount in storage and reserved balance
		assert_eq!(Accounts::<T>::get(account_index).unwrap().1, original_deposit);
		assert_eq!(T::Currency::reserved_balance(&caller), original_deposit);

		// The additional amount we'll add to the deposit for the index
		let additional_amount = 2u32.into();

		// Reserve the additional amount from the caller's balance
		T::Currency::reserve(&caller, additional_amount)?;

		// Verify the additional amount was reserved
		assert_eq!(
			T::Currency::reserved_balance(&caller),
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
		assert_eq!(T::Currency::reserved_balance(&caller), original_deposit);
		Ok(())
	}

	// TODO in another PR: lookup and unlookup trait weights (not critical)

	impl_benchmark_test_suite!(Pallet, mock::new_test_ext(), mock::Test);
}
