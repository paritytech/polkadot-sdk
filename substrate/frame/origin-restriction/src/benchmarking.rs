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

//! Benchmarks for pallet origin restriction.

use super::*;
use frame_benchmarking::{v2::*, BenchmarkError};
use sp_runtime::traits::DispatchTransaction;

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

#[benchmarks]
mod benches {
	use super::*;

	#[benchmark]
	fn clean_usage() -> Result<(), BenchmarkError> {
		let origin = T::RestrictedEntity::benchmarked_restricted_origin();
		let entity = T::RestrictedEntity::restricted_entity(&origin)
			.expect("The origin from `benchmarked_restricted_origin` must be restricted");

		Usages::<T>::insert(&entity, Usage { used: 1u32.into(), at_block: 0u32.into() });

		frame_system::Pallet::<T>::set_block_number(1_000u32.into());

		#[extrinsic_call]
		_(frame_system::RawOrigin::Root, entity.clone());

		assert_last_event::<T>(Event::UsageCleaned { entity }.into());

		Ok(())
	}

	// This benchmark may miss the cost for `OperationAllowedOneTimeExcess::contains`.
	#[benchmark]
	fn restrict_origin_tx_ext() -> Result<(), BenchmarkError> {
		let tx_ext = RestrictOrigin::<T>::new(true);
		let origin = T::RestrictedEntity::benchmarked_restricted_origin();
		let call = frame_system::Call::remark { remark: alloc::vec![] }.into();

		#[block]
		{
			tx_ext
				.test_run(origin.into(), &call, &Default::default(), 0, 0, |_| {
					Ok(Default::default())
				})
				.expect("Failed to allow the cheapest call, benchmark needs to be improved")
				.expect("inner call successful");
		}

		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
