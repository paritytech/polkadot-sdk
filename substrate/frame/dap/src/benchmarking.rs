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
use crate::SingleAssetAllocation;
use frame_benchmarking::v2::*;
use frame_support::assert_ok;
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

	fn create_full_asset_allocations<T: Config>() -> AssetAllocationMapOf<T> {
		create_asset_allocations::<T>(T::MaxDistributableAssets::get())
	}

	/// Will attempt to create `count` assets with ids defined as `AssetKind::from(0)`,
	/// `AssetKind::from(1)`, ..., `AssetKind::from(count - 1)`. If some assets couldn't be created,
	/// they are skipped. It's done so because in some runtime configurations `Pallet::Assets` won't
	/// be configured to the real `pallet-assets` and may not allow creating the assets.
	fn create_asset_allocations<T: Config>(count: u32) -> AssetAllocationMapOf<T> {
		let mut allocations: AssetAllocationMapOf<T> = BoundedBTreeMap::new();

		for asset_id in 0..count {
			let Ok(asset_id) = T::BenchmarkHelper::create_asset(asset_id) else {
				continue;
			};

			let mut single_asset_allocation = BoundedBTreeMap::new();

			let recipients = T::BudgetRecipients::recipients();
			for (budget_key, _) in recipients {
				assert_ok!(single_asset_allocation.try_insert(
					budget_key,
					SingleAssetAllocation {
						amount_per_ms: T::Assets::minimum_balance(asset_id.clone())
					}
				));
			}

			assert_ok!(allocations.try_insert(asset_id, single_asset_allocation.clone()));
		}

		allocations
	}

	fn mint_to_buffer<T: Config>(asset: AssetKindOf<T>, amount: AssetsBalanceOf<T>) {
		assert_ok!(T::Assets::mint_into(asset, &Pallet::<T>::buffer_account(), amount));
	}

	fn assert_has_event<T: Config>(generic_event: crate::Event<T>) {
		let re: <T as frame_system::Config>::RuntimeEvent = generic_event.into();
		frame_system::Pallet::<T>::assert_has_event(re.into());
	}

	#[benchmark]
	fn set_budget_allocation() {
		let allocations = build_even_allocation::<T>();

		#[extrinsic_call]
		_(RawOrigin::Root, allocations.clone());

		assert_has_event::<T>(Event::BudgetAllocationUpdated { allocations });
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
	fn drip_issuance(n: Linear<0, { T::MaxDistributableAssets::get().into() }>) {
		let budget_allocations = build_even_allocation::<T>();
		let asset_allocations = create_asset_allocations::<T>(n);
		assert_ok!(Pallet::<T>::set_allocations(
			RawOrigin::Root.into(),
			Some(budget_allocations),
			Some(asset_allocations.clone()),
		));

		let recipients = T::BudgetRecipients::recipients();

		for i in asset_allocations.keys() {
			let cadence = T::IssuanceCadence::get();
			let cadence_as_balance: AssetsBalanceOf<T> =
				SaturatedConversion::saturated_from(cadence);
			let to_mint = T::Assets::minimum_balance(i.clone())
				.saturating_mul(cadence_as_balance.saturating_add(AssetsBalanceOf::<T>::from(1u32)))
				.saturating_mul(AssetsBalanceOf::<T>::from(1_000_000_000u32));

			mint_to_buffer::<T>(i.clone(), to_mint);
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

		for i in asset_allocations.keys() {
			let elapsed_millis_as_balance: AssetsBalanceOf<T> = (elapsed_millis as u32).into();
			let recipients_len: AssetsBalanceOf<T> = (recipients.len() as u32).into();
			let minimum_balance = T::Assets::minimum_balance(i.clone());

			assert_has_event::<T>(Event::AssetDistributed {
				asset: i.clone(),
				amount: elapsed_millis_as_balance
					.saturating_mul(recipients_len)
					.saturating_mul(minimum_balance),
				elapsed_millis,
			});
		}
	}

	// Implements a test for each benchmark. Execute with:
	// `cargo test -p pallet-dap --features runtime-benchmarks`.
	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext_bench(), crate::mock::Test);
}
