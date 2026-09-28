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
use crate::AssetAllocation;
use frame_benchmarking::v2::*;
use frame_support::{assert_ok, traits::fungibles::Create};
use frame_system::RawOrigin;
use sp_staking::budget::BudgetRecipientList;

#[benchmarks(where T: pallet_timestamp::Config<Moment = u64>, AssetKindOf<T> : From<u32>)]
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

	fn create_full_asset_allocations<T>() -> AssetAllocationMap<AssetKindOf<T>, BalanceOf<T>>
	where
		T: Config,
		AssetKindOf<T>: From<u32>,
	{
		create_asset_allocations::<T>(MAX_DISTRIBUTABLE_ASSETS)
	}

	fn create_asset_allocations<T>(count: u32) -> AssetAllocationMap<AssetKindOf<T>, BalanceOf<T>>
	where
		T: Config,
		AssetKindOf<T>: From<u32>,
	{
		let mut allocations: AssetAllocationMap<AssetKindOf<T>, BalanceOf<T>> =
			BoundedBTreeMap::new();

		let recipients = T::BudgetRecipients::recipients();
		let mut single_asset_allocation = BoundedBTreeMap::new();
		for (budget_key, _) in recipients {
			single_asset_allocation
				.try_insert(budget_key, AssetAllocation { amount_per_ms: T::Balance::one() });
		}

		for asset_id in 0..count {
			let asset_id: AssetKindOf<T> = asset_id.into();
			create_asset::<T>(asset_id.clone());
			assert_ok!(allocations.try_insert(asset_id, single_asset_allocation.clone()));
		}

		allocations
	}

	fn mint_to_staging<T: Config>(asset: AssetKindOf<T>, amount: u32) {
		T::Assets::mint_into(asset, &Pallet::<T>::staging_account(), amount.into());
	}

	fn assert_has_event<T: Config>(generic_event: crate::Event<T>) {
		let re: <T as frame_system::Config>::RuntimeEvent = generic_event.into();
		frame_system::Pallet::<T>::assert_has_event(re.into());
	}

	fn assert_last_event<T: Config>(generic_event: crate::Event<T>) {
		let re: <T as frame_system::Config>::RuntimeEvent = generic_event.into();
		frame_system::Pallet::<T>::assert_last_event(re.into());
	}

	#[benchmark]
	fn set_allocations() {
		let asset_allocations = create_full_asset_allocations::<T>();
		let budget_allocations = build_even_allocation::<T>();

		#[extrinsic_call]
		_(RawOrigin::Root, Some(budget_allocations.clone()), Some(asset_allocations.clone()));

		assert_has_event::<T>(Event::AssetAllocationUpdated { allocations: asset_allocations });
		assert_has_event::<T>(Event::BudgetAllocationUpdated { allocations: budget_allocations });
	}

	#[benchmark]
	fn drip_issuance(n: Linear<0, { MAX_DISTRIBUTABLE_ASSETS.into() }>) {
		let budget_allocations = build_even_allocation::<T>();
		let asset_allocations = create_asset_allocations::<T>(n);
		Pallet::<T>::set_allocations(
			RawOrigin::Root.into(),
			Some(budget_allocations),
			Some(asset_allocations),
		);

		let recipients = T::BudgetRecipients::recipients();

		// Mint ED.
		for (_, recipient) in &recipients {
			T::Assets::mint_into(
				T::NativeCurrencyAssetId::get(),
				recipient,
				T::Balance::from(100u32),
			);
		}

		for i in 0..n {
			mint_to_staging::<T>(i.into(), 1_000_000_000);
		}

		// Trigger transfer from staging to buffer.
		Pallet::<T>::on_idle(Default::default(), Weight::MAX);

		// Set a timestamp so the drip fires.
		let now: u64 = 1_000_000;
		pallet_timestamp::Now::<T>::put(now);
		let elapsed_millis = T::IssuanceCadence::get() + 1;
		let past = now.saturating_sub(elapsed_millis);
		LastIssuanceTimestamp::<T>::put(past);

		#[block]
		{
			Pallet::<T>::drip_issuance();
		}

		assert!(LastIssuanceTimestamp::<T>::get() > past);

		for i in 0..n {
			assert_has_event::<T>(Event::AssetDistributed {
				asset: i.into(),
				amount: (elapsed_millis as u32 * recipients.len() as u32).into(),
				elapsed_millis,
			});
		}
	}

	#[benchmark]
	fn on_idle_base() {
		let allocations = create_full_asset_allocations::<T>();
		Pallet::<T>::set_allocations(RawOrigin::Root.into(), None, Some(allocations));

		mint_to_staging::<T>(T::NativeCurrencyAssetId::get(), 1);

		#[block]
		{
			Pallet::<T>::on_idle(Default::default(), Weight::MAX);
		}

		assert_last_event::<T>(Event::StagingDrained {
			amount: T::Balance::one(),
			asset: T::NativeCurrencyAssetId::get(),
		});
	}

	#[benchmark]
	fn on_idle_single_asset_drain() {
		let allocations = create_full_asset_allocations::<T>();
		Pallet::<T>::set_allocations(RawOrigin::Root.into(), None, Some(allocations));

		const ASSET: u32 = 1;

		mint_to_staging::<T>(T::NativeCurrencyAssetId::get(), 1);
		mint_to_staging::<T>(ASSET.into(), 100);

		#[block]
		{
			Pallet::<T>::on_idle(Default::default(), Weight::MAX);
		}

		assert_has_event::<T>(Event::StagingDrained {
			amount: T::Balance::from(99u32),
			asset: ASSET.into(),
		});

		assert_has_event::<T>(Event::StagingDrained {
			amount: T::Balance::one(),
			asset: T::NativeCurrencyAssetId::get(),
		});
	}

	// Implements a test for each benchmark. Execute with:
	// `cargo test -p pallet-dap --features runtime-benchmarks`.
	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext_bench(), crate::mock::Test);
}
