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

// Benchmarks for the Utility Extension Pallet

#![cfg(feature = "runtime-benchmarks")]

use alloc::vec;
use frame_benchmarking::v2::*;
use frame_support::traits::{Get, OriginTrait};
use frame_system::RawOrigin;

use crate::*;

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

/// `c` items, each a `remark`.
fn multi_origin_items<T: Config>(c: u32) -> MultiOriginItemsOf<T> {
	let item = <T as Config>::BenchmarkHelper::item(
		frame_system::Call::<T>::remark { remark: vec![] }.into(),
	);
	vec![item; c as usize]
		.try_into()
		.expect("`c` is bounded by `MaxMultiOriginBatch`; qed")
}

#[benchmarks]
mod benchmark {
	use super::*;

	#[benchmark]
	fn batch_multi_origin(c: Linear<0, { T::MaxMultiOriginBatch::get() }>) {
		let items = multi_origin_items::<T>(c);
		// The origins the extension would have stored.
		let caller: T::AccountId = whitelisted_caller();
		let origin: <T as frame_system::Config>::RuntimeOrigin = RawOrigin::Signed(caller).into();
		let pallets_origin = PalletsOriginOf::<T>::from(origin.into_caller());
		MultiOrigins::<T>::put(vec![pallets_origin; c as usize]);

		#[extrinsic_call]
		_(Origin::MultiOriginBatch, items);

		assert_last_event::<T>(Event::BatchCompleted.into());
		assert!(MultiOrigins::<T>::get().is_none());
		assert_eq!(MultiOriginPostInfos::<T>::get().map(|p| p.len()), Some(c as usize));
	}

	impl_benchmark_test_suite! {
		Pallet,
		tests::new_test_ext(),
		tests::Test
	}
}
