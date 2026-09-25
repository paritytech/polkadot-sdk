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

//! Benchmarks for the assets forwarder.

use super::*;

use frame_benchmarking::v2::*;
use frame_support::traits::{
	fungible::{InspectHold, Mutate},
	fungibles::Create,
};
use frame_system::RawOrigin;
use sp_runtime::traits::{Saturating, Zero};

/// Runtime-specific benchmark setup.
pub trait BenchmarkHelper<AssetId> {
	/// Id of the asset the benchmarks create; for the worst case, the one whose location has the
	/// most junctions.
	fn asset_id(seed: u32) -> AssetId;

	/// Makes the router accept messages to the destination, e.g. by opening the channel. For the
	/// worst case, also enqueue a message first so delivery appends to an existing outbound page.
	fn open_destination_channel() {}
}

impl<AssetId: From<u32>> BenchmarkHelper<AssetId> for () {
	fn asset_id(seed: u32) -> AssetId {
		seed.into()
	}
}

/// Creates a sufficient asset with minimum balance one, with `caller` funded to pay the forward
/// deposit and the delivery fees.
fn setup_asset<T: Config>(caller: &T::AccountId) -> Result<AssetIdOf<T>, BenchmarkError>
where
	T::Assets: Create<T::AccountId>,
{
	<T as Config>::BenchmarkHelper::open_destination_channel();
	let id = <T as Config>::BenchmarkHelper::asset_id(1);
	let balance = <T as Config>::Currency::minimum_balance()
		.saturating_add(T::ForwardDeposit::get())
		.saturating_mul(1_000_000u32.into());
	<T as Config>::Currency::set_balance(caller, balance);
	T::Assets::create(id.clone(), caller.clone(), true, 1u32.into())
		.map_err(|_| BenchmarkError::Stop("failed to create asset"))?;
	Ok(id)
}

#[benchmarks(where T::Assets: Create<T::AccountId>)]
mod benches {
	use super::*;

	#[benchmark]
	fn forward_asset() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let id = setup_asset::<T>(&caller)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), id.clone());

		assert!(ForwardedAssets::<T>::contains_key(id));
		Ok(())
	}

	#[benchmark]
	fn sync_asset_status() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let id = setup_asset::<T>(&caller)?;
		Pallet::<T>::forward_asset(RawOrigin::Signed(caller.clone()).into(), id.clone())
			.map_err(|_| BenchmarkError::Stop("failed to forward asset"))?;
		// Make the record lag behind the registry, otherwise the sync is rejected as a no-op.
		ForwardedAssets::<T>::mutate(&id, |record| {
			if let Some(record) = record {
				record.is_sufficient = false;
			}
		});

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), id.clone());

		let record = ForwardedAssets::<T>::get(&id).expect("asset is forwarded");
		assert_eq!(record.min_balance, T::Assets::minimum_balance(id.clone()));
		assert_eq!(record.is_sufficient, T::Assets::is_sufficient(id));
		assert!(record.is_sufficient);
		Ok(())
	}

	#[benchmark]
	fn remove_forwarded_asset() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let id = setup_asset::<T>(&caller)?;
		Pallet::<T>::forward_asset(RawOrigin::Signed(caller.clone()).into(), id.clone())
			.map_err(|_| BenchmarkError::Stop("failed to forward asset"))?;
		let origin =
			T::ManagerOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, id.clone());

		assert!(!ForwardedAssets::<T>::contains_key(id));
		assert!(<T as Config>::Currency::balance_on_hold(
			&HoldReason::ForwardDeposit.into(),
			&caller
		)
		.is_zero());
		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
