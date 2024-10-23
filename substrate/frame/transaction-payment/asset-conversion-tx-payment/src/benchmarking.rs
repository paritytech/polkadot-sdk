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

//! Benchmarks for Asset Conversion Tx Payment Pallet's transaction extension

extern crate alloc;

use super::*;
use crate::Pallet;
use frame_benchmarking::v2::*;
use frame_support::{
	dispatch::{DispatchInfo, PostDispatchInfo},
	pallet_prelude::*,
};
use frame_system::RawOrigin;
use sp_runtime::traits::{
	AsSystemOriginSigner, AsTransactionAuthorizedOrigin, DispatchTransaction, Dispatchable,
};

#[benchmarks(where
	T::RuntimeOrigin: AsTransactionAuthorizedOrigin,
	T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
	T::AssetId: Send + Sync,
	BalanceOf<T>: Send
		+ Sync
		+ From<u64>,
	<T::RuntimeCall as Dispatchable>::RuntimeOrigin: AsSystemOriginSigner<T::AccountId> + Clone,
)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn charge_asset_tx_payment_zero() {
		let caller: T::AccountId = account("caller", 0, 0);
		let ext: ChargeAssetTxPayment<T> = ChargeAssetTxPayment::from(0u64.into(), None);
		let inner = frame_system::Call::remark { remark: alloc::vec![] };
		let call = T::RuntimeCall::from(inner);
		let info = DispatchInfo {
			call_weight: Weight::zero(),
			extension_weight: Weight::zero(),
			class: DispatchClass::Normal,
			pays_fee: Pays::No,
		};
		let post_info = PostDispatchInfo { actual_weight: None, pays_fee: Pays::No };
		#[block]
		{
			assert!(ext
				.test_run(RawOrigin::Signed(caller).into(), &call, &info, 0, |_| Ok(post_info))
				.unwrap()
				.is_ok());
		}
	}

	#[benchmark]
	fn charge_asset_tx_payment_native() {
		let caller: T::AccountId = account("caller", 0, 0);
		let (fun_asset_id, _) = <T as Config>::BenchmarkHelper::create_asset_id_parameter(1);
		<T as Config>::BenchmarkHelper::setup_balances_and_pool(fun_asset_id, caller.clone());
		let ext: ChargeAssetTxPayment<T> = ChargeAssetTxPayment::from(10u64.into(), None);
		let inner = frame_system::Call::remark { remark: alloc::vec![] };
		let call = T::RuntimeCall::from(inner);
		let info = DispatchInfo {
			call_weight: Weight::from_parts(10, 0),
			extension_weight: Weight::zero(),
			class: DispatchClass::Operational,
			pays_fee: Pays::Yes,
		};
		// Submit a lower post info weight to trigger the refund path.
		let post_info =
			PostDispatchInfo { actual_weight: Some(Weight::from_parts(5, 0)), pays_fee: Pays::Yes };

		#[block]
		{
			assert!(ext
				.test_run(RawOrigin::Signed(caller).into(), &call, &info, 0, |_| Ok(post_info))
				.unwrap()
				.is_ok());
		}
	}

	#[benchmark]
	fn charge_asset_tx_payment_asset() {
		let caller: T::AccountId = account("caller", 0, 0);
		let (fun_asset_id, asset_id) = <T as Config>::BenchmarkHelper::create_asset_id_parameter(1);
		<T as Config>::BenchmarkHelper::setup_balances_and_pool(fun_asset_id, caller.clone());

		let tip = 10u64.into();
		let ext: ChargeAssetTxPayment<T> = ChargeAssetTxPayment::from(tip, Some(asset_id));
		let inner = frame_system::Call::remark { remark: alloc::vec![] };
		let call = T::RuntimeCall::from(inner);
		let info = DispatchInfo {
			call_weight: Weight::from_parts(10, 0),
			extension_weight: Weight::zero(),
			class: DispatchClass::Operational,
			pays_fee: Pays::Yes,
		};
		// Submit a lower post info weight to trigger the refund path.
		let post_info =
			PostDispatchInfo { actual_weight: Some(Weight::from_parts(5, 0)), pays_fee: Pays::Yes };

		#[block]
		{
			assert!(ext
				.test_run(RawOrigin::Signed(caller.clone()).into(), &call, &info, 0, |_| Ok(
					post_info
				))
				.unwrap()
				.is_ok());
		}
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Runtime);
}
