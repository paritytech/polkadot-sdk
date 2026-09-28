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

//! Benchmarks for pallet-dap.

use super::*;
use frame_benchmarking::v2::*;
use frame_support::{assert_ok, traits::fungibles::Create};
use frame_system::RawOrigin;
use sp_staking::budget::BudgetRecipientList;

#[benchmarks(where T: pallet_timestamp::Config<Moment = u64>)]
mod benchmarks {
	use super::*;

	/// Build a valid allocation from registered recipients, distributing evenly and giving
	/// the remainder to the last recipient to ensure the sum is exactly 100%.
	fn build_even_allocation<T: Config>() -> BudgetAllocationMap {
		let recipients = T::BudgetRecipients::recipients();
		let count = recipients.len() as u32;
		let mut allocations = BudgetAllocationMap::new();

		for (i, (key, _)) in recipients.into_iter().enumerate() {
			let perbill = if i as u32 == count - 1 {
				let used: u32 = allocations.values().map(|p| p.deconstruct()).sum();
				Perbill::from_parts(Perbill::one().deconstruct().saturating_sub(used))
			} else {
				Perbill::from_rational(1u32, count)
			};
			allocations.try_insert(key, perbill).expect("bounded by MAX_BUDGET_RECIPIENTS");
		}

		allocations
	}

	fn create_asset<T: Config>(asset: T::AssetKind) {
		let caller: T::AccountId = whitelisted_caller();
		assert_ok!(T::Assets::create(asset, caller, false, T::Balance::one()));
	}

	#[benchmark]
	fn set_allocations() {
		let allocations = build_even_allocation::<T>();

		// TODO: Add asset allocations.
		#[extrinsic_call]
		_(RawOrigin::Root, Some(allocations.clone()), None);

		assert_eq!(BudgetAllocation::<T>::get(), allocations);
	}

	#[benchmark]
	fn drip_issuance() {
		let allocations = build_even_allocation::<T>();
		BudgetAllocation::<T>::put(allocations);

		// Set a timestamp so the drip fires.
		let now: u64 = 1_000_000;
		pallet_timestamp::Now::<T>::put(now);
		let past = now.saturating_sub(T::IssuanceCadence::get() + 1);
		LastIssuanceTimestamp::<T>::put(past);

		#[block]
		{
			Pallet::<T>::drip_issuance();
		}

		assert!(LastIssuanceTimestamp::<T>::get() > past);
	}

	#[benchmark]
	fn on_idle_base() {
		// let mut allocations: AssetAllocationMap<AssetKindOf<T>, BalanceOf<T>> =
		// 	BoundedBTreeMap::new();
		// for asset_id in 0..MAX_DISTRIBUTABLE_ASSETS {
		// 	let asset_id: AssetKindOf<T> = asset_id.into();

		// 	create_asset::<T>(asset_id.clone());
		// 	assert_ok!(allocations.try_insert(asset_id, Default::default()));
		// }

		#[block]
		{
			Pallet::<T>::on_idle(Default::default(), Weight::MAX);
		}

		// TODO: Assert that only native token was transfered.
	}

	// TODO: Mint tokens.
	#[benchmark]
	fn on_idle_single_asset_drain() {
		// let mut allocations: AssetAllocationMap<AssetKindOf<T>, BalanceOf<T>> =
		// 	BoundedBTreeMap::new();
		// for asset_id in 0..MAX_DISTRIBUTABLE_ASSETS {
		// 	let asset_id: AssetKindOf<T> = asset_id.into();

		// 	create_asset::<T>(asset_id.clone());
		// 	assert_ok!(allocations.try_insert(asset_id, Default::default()));
		// }

		#[block]
		{
			Pallet::<T>::on_idle(Default::default(), Weight::MAX);
		}

		// TODO: Assert.
	}

	// Implements a test for each benchmark. Execute with:
	// `cargo test -p pallet-dap --features runtime-benchmarks`.
	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext_bench(), crate::mock::Test);
}
