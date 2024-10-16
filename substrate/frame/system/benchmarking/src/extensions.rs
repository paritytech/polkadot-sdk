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

// Benchmarks for System Extensions

#![cfg(feature = "runtime-benchmarks")]

use alloc::vec;
use frame_benchmarking::{account, v2::*, BenchmarkError};
use frame_support::{
	dispatch::{DispatchClass, DispatchInfo, PostDispatchInfo},
	weights::Weight,
};
use frame_system::{
	pallet_prelude::*, CheckGenesis, CheckMortality, CheckNonZeroSender, CheckNonce,
	CheckSpecVersion, CheckTxVersion, CheckWeight, Config, ExtensionsWeightInfo, Pallet as System,
	RawOrigin,
};
use sp_runtime::{
	generic::Era,
	traits::{
		AsSystemOriginSigner, AsTransactionAuthorizedOrigin, DispatchTransaction, Dispatchable, Get,
	},
};

pub struct Pallet<T: Config>(System<T>);

#[benchmarks(where
	T: Send + Sync,
    T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
	<T::RuntimeCall as Dispatchable>::RuntimeOrigin: AsSystemOriginSigner<T::AccountId> + AsTransactionAuthorizedOrigin + Clone)
]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn check_genesis() -> Result<(), BenchmarkError> {
		let len = 0_usize;
		let caller = account("caller", 0, 0);
		let info = DispatchInfo { call_weight: Weight::zero(), ..Default::default() };
		let call: T::RuntimeCall = frame_system::Call::remark { remark: vec![] }.into();

		#[block]
		{
			CheckGenesis::<T>::new()
				.test_run(RawOrigin::Signed(caller).into(), &call, &info, len, |_| Ok(().into()))
				.unwrap()
				.unwrap();
		}

		Ok(())
	}

	#[benchmark]
	fn check_mortality_mortal_transaction() -> Result<(), BenchmarkError> {
		let len = 0_usize;
		let ext = CheckMortality::<T>::from(Era::mortal(16, 256));
		let block_number: BlockNumberFor<T> = 17u32.into();
		System::<T>::set_block_number(block_number);
		let prev_block: BlockNumberFor<T> = 16u32.into();
		let default_hash: T::Hash = Default::default();
		frame_system::BlockHash::<T>::insert(prev_block, default_hash);
		let caller = account("caller", 0, 0);
		let info = DispatchInfo {
			call_weight: Weight::from_parts(100, 0),
			class: DispatchClass::Normal,
			..Default::default()
		};
		let call: T::RuntimeCall = frame_system::Call::remark { remark: vec![] }.into();

		#[block]
		{
			ext.test_run(RawOrigin::Signed(caller).into(), &call, &info, len, |_| Ok(().into()))
				.unwrap()
				.unwrap();
		}
		Ok(())
	}

	#[benchmark]
	fn check_mortality_immortal_transaction() -> Result<(), BenchmarkError> {
		let len = 0_usize;
		let ext = CheckMortality::<T>::from(Era::immortal());
		let block_number: BlockNumberFor<T> = 17u32.into();
		System::<T>::set_block_number(block_number);
		let prev_block: BlockNumberFor<T> = 16u32.into();
		let default_hash: T::Hash = Default::default();
		frame_system::BlockHash::<T>::insert(prev_block, default_hash);
		let genesis_block: BlockNumberFor<T> = 0u32.into();
		frame_system::BlockHash::<T>::insert(genesis_block, default_hash);
		let caller = account("caller", 0, 0);
		let info = DispatchInfo {
			call_weight: Weight::from_parts(100, 0),
			class: DispatchClass::Normal,
			..Default::default()
		};
		let call: T::RuntimeCall = frame_system::Call::remark { remark: vec![] }.into();

		#[block]
		{
			ext.test_run(RawOrigin::Signed(caller).into(), &call, &info, len, |_| Ok(().into()))
				.unwrap()
				.unwrap();
		}
		Ok(())
	}

	#[benchmark]
	fn check_non_zero_sender() -> Result<(), BenchmarkError> {
		let len = 0_usize;
		let ext = CheckNonZeroSender::<T>::new();
		let caller = account("caller", 0, 0);
		let info = DispatchInfo { call_weight: Weight::zero(), ..Default::default() };
		let call: T::RuntimeCall = frame_system::Call::remark { remark: vec![] }.into();

		#[block]
		{
			ext.test_run(RawOrigin::Signed(caller).into(), &call, &info, len, |_| Ok(().into()))
				.unwrap()
				.unwrap();
		}
		Ok(())
	}

	#[benchmark]
	fn check_nonce() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = account("caller", 0, 0);
		let mut info = frame_system::AccountInfo::default();
		info.nonce = 1u32.into();
		info.providers = 1;
		let expected_nonce = info.nonce + 1u32.into();
		frame_system::Account::<T>::insert(caller.clone(), info);
		let len = 0_usize;
		let ext = CheckNonce::<T>::from(1u32.into());
		let info = DispatchInfo { call_weight: Weight::zero(), ..Default::default() };
		let call: T::RuntimeCall = frame_system::Call::remark { remark: vec![] }.into();

		#[block]
		{
			ext.test_run(RawOrigin::Signed(caller.clone()).into(), &call, &info, len, |_| {
				Ok(().into())
			})
			.unwrap()
			.unwrap();
		}

		let updated_info = frame_system::Account::<T>::get(caller.clone());
		assert_eq!(updated_info.nonce, expected_nonce);
		Ok(())
	}

	#[benchmark]
	fn check_spec_version() -> Result<(), BenchmarkError> {
		let len = 0_usize;
		let caller = account("caller", 0, 0);
		let info = DispatchInfo { call_weight: Weight::zero(), ..Default::default() };
		let call: T::RuntimeCall = frame_system::Call::remark { remark: vec![] }.into();

		#[block]
		{
			CheckSpecVersion::<T>::new()
				.test_run(RawOrigin::Signed(caller).into(), &call, &info, len, |_| Ok(().into()))
				.unwrap()
				.unwrap();
		}
		Ok(())
	}

	#[benchmark]
	fn check_tx_version() -> Result<(), BenchmarkError> {
		let len = 0_usize;
		let caller = account("caller", 0, 0);
		let info = DispatchInfo { call_weight: Weight::zero(), ..Default::default() };
		let call: T::RuntimeCall = frame_system::Call::remark { remark: vec![] }.into();

		#[block]
		{
			CheckTxVersion::<T>::new()
				.test_run(RawOrigin::Signed(caller).into(), &call, &info, len, |_| Ok(().into()))
				.unwrap()
				.unwrap();
		}
		Ok(())
	}

	#[benchmark]
	fn check_weight() -> Result<(), BenchmarkError> {
		let caller = account("caller", 0, 0);
		let base_extrinsic = <T as frame_system::Config>::BlockWeights::get()
			.get(DispatchClass::Normal)
			.base_extrinsic;
		let extension_weight = <T as frame_system::Config>::ExtensionsWeightInfo::check_weight();
		let info = DispatchInfo {
			call_weight: Weight::from_parts(base_extrinsic.ref_time() * 5, 0),
			extension_weight,
			class: DispatchClass::Normal,
			..Default::default()
		};
		let call: T::RuntimeCall = frame_system::Call::remark { remark: vec![] }.into();
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(base_extrinsic.ref_time() * 2, 0)),
			pays_fee: Default::default(),
		};
		let len = 0_usize;
		let base_extrinsic = <T as frame_system::Config>::BlockWeights::get()
			.get(DispatchClass::Normal)
			.base_extrinsic;

		let ext = CheckWeight::<T>::new();

		let initial_block_weight = Weight::from_parts(base_extrinsic.ref_time() * 2, 0);
		frame_system::BlockWeight::<T>::mutate(|current_weight| {
			current_weight.set(Weight::zero(), DispatchClass::Mandatory);
			current_weight.set(initial_block_weight, DispatchClass::Normal);
		});

		#[block]
		{
			ext.test_run(RawOrigin::Signed(caller).into(), &call, &info, len, |_| Ok(post_info))
				.unwrap()
				.unwrap();
		}

		assert_eq!(
			System::<T>::block_weight().total(),
			initial_block_weight +
				base_extrinsic +
				post_info.actual_weight.unwrap().saturating_add(extension_weight),
		);
		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
