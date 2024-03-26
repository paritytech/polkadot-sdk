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

//! Benchmarking setup for cumulus-primitives-storage-weight-reclaim

#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{account, v2::*, BenchmarkError};
use frame_support::pallet_prelude::DispatchClass;
use frame_system::{BlockWeight, RawOrigin};
use sp_runtime::traits::{DispatchTransaction, Get};
use sp_std::{
	marker::{Send, Sync},
	prelude::*,
};

/// Pallet we're benchmarking here.
pub struct Pallet<T: frame_system::Config>(frame_system::Pallet<T>);

#[benchmarks(where
    T: Send + Sync,
    T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>
)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn storage_weight_reclaim() -> Result<(), BenchmarkError> {
		let caller = account("caller", 0, 0);
		BlockWeight::<T>::mutate(|current_weight| {
			current_weight.set(Weight::from_parts(0, 1000), DispatchClass::Normal);
		});
		let base_extrinsic = <T as frame_system::Config>::BlockWeights::get()
			.get(DispatchClass::Normal)
			.base_extrinsic;
		let info = DispatchInfo { weight: Weight::from_parts(0, 500), ..Default::default() };
		let call: T::RuntimeCall = frame_system::Call::remark { remark: vec![] }.into();
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(0, 200)),
			pays_fee: Default::default(),
		};
		let len = 0_usize;
		let ext = StorageWeightReclaim::<T>::new();

		#[block]
		{
			ext.test_run(RawOrigin::Signed(caller).into(), &call, &info, len, |_| Ok(post_info))
				.unwrap()
				.unwrap();
		}

		assert_eq!(BlockWeight::<T>::get().total().proof_size(), 700 + base_extrinsic.proof_size());

		Ok(())
	}
}
