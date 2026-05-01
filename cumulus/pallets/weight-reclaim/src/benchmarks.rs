// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_support::pallet_prelude::{DispatchClass, Pays};
use frame_system::RawOrigin;
use sp_runtime::traits::{AsTransactionAuthorizedOrigin, DispatchTransaction};

#[frame_benchmarking::v2::benchmarks(
	where T: Send + Sync,
		<T as frame_system::Config>::RuntimeCall:
			Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
		<T as frame_system::Config>::RuntimeOrigin: AsTransactionAuthorizedOrigin,
)]
mod bench {
	use super::*;
	use frame_benchmarking::impl_test_function;

	#[benchmark]
	fn storage_weight_reclaim() {
		let ext = StorageWeightReclaim::<T, ()>::new(());

		let origin = RawOrigin::Root.into();
		let call = T::RuntimeCall::from(frame_system::Call::remark { remark: alloc::vec![] });

		let overestimate = 10_000;
		let info = DispatchInfo {
			call_weight: Weight::zero().add_proof_size(overestimate),
			extension_weight: Weight::zero(),
			class: DispatchClass::Normal,
			pays_fee: Pays::No,
		};

		let post_info = PostDispatchInfo { actual_weight: None, pays_fee: Pays::No };

		let mut block_weight = frame_system::ConsumedWeight::default();
		block_weight.accrue(Weight::from_parts(0, overestimate), info.class);

		frame_system::BlockWeight::<T>::put(block_weight);

		#[block]
		{
			assert!(ext.test_run(origin, &call, &info, 0, 0, |_| Ok(post_info)).unwrap().is_ok());
		}

		let final_block_proof_size =
			frame_system::BlockWeight::<T>::get().get(info.class).proof_size();

		assert!(
			final_block_proof_size < overestimate,
			"The proof size measured should be less than {overestimate}"
		);
	}

	impl_benchmark_test_suite!(Pallet, crate::tests::setup_test_ext_default(), crate::tests::Test);
}
