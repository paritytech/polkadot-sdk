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

use super::*;
use crate::Pallet;
use frame_benchmarking::v2::*;
use frame_support::{
	dispatch::{DispatchInfo, PostDispatchInfo},
	pallet_prelude::*,
	traits::fungibles::Inspect,
};
use frame_system::RawOrigin;
use sp_runtime::traits::{AsSystemOriginSigner, DispatchTransaction, Dispatchable};

pub trait ExtConfig: Config {
	fn create_asset_id_parameter(
		id: u32,
	) -> (
		<<Self as Config>::Fungibles as Inspect<Self::AccountId>>::AssetId,
		<<Self as Config>::OnChargeAssetTransaction as OnChargeAssetTransaction<Self>>::AssetId,
	);
	fn setup_balances_and_pool(
		asset_id: <<Self as Config>::Fungibles as Inspect<Self::AccountId>>::AssetId,
		account: Self::AccountId,
	);
}

#[benchmarks(where
	T: Config + Send + Sync + ExtConfig,
	T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
	AssetBalanceOf<T>: Send + Sync,
	BalanceOf<T>: Send
		+ Sync
		+ From<u64>
		+ Into<ChargeAssetBalanceOf<T>>
		+ Into<ChargeAssetLiquidityOf<T>>
		+ From<ChargeAssetLiquidityOf<T>>,
	ChargeAssetIdOf<T>: Send + Sync + Default,
	<T::RuntimeCall as Dispatchable>::RuntimeOrigin: AsSystemOriginSigner<T::AccountId> + Clone,
)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn charge_asset_tx_payment_zero() {
		let caller: T::AccountId = whitelisted_caller();
		let ext: ChargeAssetTxPayment<T> = ChargeAssetTxPayment::from(0u64.into(), None);
		let inner = frame_system::Call::remark { remark: vec![] };
		let call = T::RuntimeCall::from(inner);
		let info =
			DispatchInfo { weight: 0.into(), class: DispatchClass::Normal, pays_fee: Pays::No };
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
		let caller: T::AccountId = whitelisted_caller();
		let (fun_asset_id, _) = T::create_asset_id_parameter(1);
		T::setup_balances_and_pool(fun_asset_id, caller.clone());
		let ext: ChargeAssetTxPayment<T> = ChargeAssetTxPayment::from(10u64.into(), None);
		let inner = frame_system::Call::remark { remark: vec![] };
		let call = T::RuntimeCall::from(inner);
		let info = DispatchInfo {
			weight: Weight::from_parts(10, 0),
			class: DispatchClass::Operational,
			pays_fee: Pays::Yes,
		};
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(10, 0)),
			pays_fee: Pays::Yes,
		};

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
		let caller: T::AccountId = whitelisted_caller();
		let (fun_asset_id, asset_id) = T::create_asset_id_parameter(1);
		T::setup_balances_and_pool(fun_asset_id.clone(), caller.clone());

		let tip = 10u64.into();
		let ext: ChargeAssetTxPayment<T> = ChargeAssetTxPayment::from(tip, Some(asset_id));
		let inner = frame_system::Call::remark { remark: vec![] };
		let call = T::RuntimeCall::from(inner);
		let info = DispatchInfo {
			weight: Weight::from_parts(10, 0),
			class: DispatchClass::Operational,
			pays_fee: Pays::Yes,
		};
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(10, 0)),
			pays_fee: Pays::Yes,
		};

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
