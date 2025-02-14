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

	// TODO in another PR: lookup and unlookup trait weights (not critical)

	impl_benchmark_test_suite!(Pallet, mock::new_test_ext(), mock::Test);
}
