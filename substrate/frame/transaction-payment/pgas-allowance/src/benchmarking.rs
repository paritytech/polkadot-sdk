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

//! Benchmarks for the `ChargeFeeWithPgas` transaction extension.
//!
//! Both benchmarks exercise the full `validate → prepare → dispatch → post_dispatch` flow so the
//! measured weight covers the wrapper's routing overhead plus the inner `ChargeAssetTxPayment`
//! work on each path.

extern crate alloc;

use super::*;
use crate::{BenchmarkHelperTrait, Pallet};
use frame_benchmarking::v2::*;
use frame_support::{
	dispatch::{DispatchClass, DispatchInfo, PostDispatchInfo},
	pallet_prelude::Weight,
	traits::tokens::fungibles,
};
use frame_system::RawOrigin;
use pallet_asset_conversion_tx_payment::ChargeAssetTxPayment;
use pallet_transaction_payment::OnChargeTransaction;
use sp_runtime::traits::{
	AsSystemOriginSigner, AsTransactionAuthorizedOrigin, DispatchTransaction, Dispatchable, Zero,
};

#[benchmarks(where
	T: Send + Sync,
	T::RuntimeOrigin: AsTransactionAuthorizedOrigin,
	T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>
		+ From<frame_system::Call<T>>,
	BalanceOf<T>: Send + Sync + From<u64>,
	AssetIdOf<T>: Send + Sync,
	<T::RuntimeCall as Dispatchable>::RuntimeOrigin: AsSystemOriginSigner<T::AccountId> + Clone,
	ChargeAssetTxPayment<T>: scale_info::StaticTypeInfo,
)]
mod benchmarks {
	use super::*;

	/// PGAS path: caller holds enough PGAS and the call matches the filter, so the extension
	/// routes to the PGAS asset path and burns the consumed portion.
	#[benchmark]
	fn charge_pgas() {
		let caller: T::AccountId = account("caller", 0, 0);
		let initial: BalanceOf<T> = u64::MAX.into();
		<T as Config>::BenchmarkHelper::mint_pgas(&caller, T::PgasId::get(), initial);

		let ext = ChargeFeeWithPgas::<T>::from(Zero::zero(), None);
		let call: T::RuntimeCall = frame_system::Call::<T>::remark { remark: alloc::vec![] }.into();
		let info = DispatchInfo {
			call_weight: Weight::from_parts(100, 0),
			class: DispatchClass::Normal,
			..Default::default()
		};
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(10, 0)),
			pays_fee: Default::default(),
		};

		let result;
		#[block]
		{
			result =
				ext.test_run(RawOrigin::Signed(caller.clone()).into(), &call, &info, 0, 0, |_| {
					Ok(post_info)
				});
		}
		assert!(result.unwrap().is_ok());
		let remaining = <<T as Config>::Fungibles as fungibles::Inspect<T::AccountId>>::balance(
			T::PgasId::get(),
			&caller,
		);
		assert!(remaining < initial, "PGAS should be charged on the PGAS path");
	}

	/// Skip path: caller holds no PGAS so the extension falls back to the native path. Measures
	/// the extension's routing overhead plus the native-path inner cost.
	#[benchmark]
	fn charge_pgas_skip() {
		let caller: T::AccountId = account("caller", 0, 0);
		<T as pallet_transaction_payment::Config>::OnChargeTransaction::endow_account(
			&caller,
			u64::MAX.into(),
		);

		let ext = ChargeFeeWithPgas::<T>::from(Zero::zero(), None);
		let call: T::RuntimeCall = frame_system::Call::<T>::remark { remark: alloc::vec![] }.into();
		let info = DispatchInfo {
			call_weight: Weight::from_parts(10, 0),
			class: DispatchClass::Normal,
			..Default::default()
		};
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(10, 0)),
			pays_fee: Default::default(),
		};

		let before = <<T as Config>::Fungibles as fungibles::Inspect<T::AccountId>>::balance(
			T::PgasId::get(),
			&caller,
		);
		let result;
		#[block]
		{
			result =
				ext.test_run(RawOrigin::Signed(caller.clone()).into(), &call, &info, 0, 0, |_| {
					Ok(post_info)
				});
		}
		assert!(result.unwrap().is_ok());
		let after = <<T as Config>::Fungibles as fungibles::Inspect<T::AccountId>>::balance(
			T::PgasId::get(),
			&caller,
		);
		assert_eq!(before, after, "PGAS must not be charged on the skip path");
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::ExtBuilder::default().build(),
		crate::mock::Runtime
	);
}
