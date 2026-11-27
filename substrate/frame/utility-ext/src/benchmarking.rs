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

//! Benchmarks for the Utility Extension Pallet.
//!
//! The items, built by [`Config::BenchmarkHelper`], run the real inner pipeline and a real call,
//! which their declared weights already cover, so the overhead is overestimated by roughly one
//! cheap signed transaction per item, on the safe side. The bytes component covers encoding and
//! hashing the items.

#![cfg(feature = "runtime-benchmarks")]

use alloc::vec::Vec;
use frame_benchmarking::v2::*;
use frame_support::{dispatch::DispatchClass, traits::Get};

use crate::*;

/// The submitter of a batch, the whitelisted caller with their nonce bumped as by `CheckNonce`,
/// the nonce it was sent with and its `c` `remark` items, the first of which carries `b` bytes.
fn batch<T: Config>(c: u32, b: u32) -> (T::AccountId, NonceOf<T>, MultiOriginItemsOf<T>) {
	let submitter: T::AccountId = whitelisted_caller();
	let nonce = frame_system::Pallet::<T>::account_nonce(&submitter);
	frame_system::Pallet::<T>::inc_account_nonce(&submitter);
	let origin = T::BenchmarkHelper::setup();
	let calls: Vec<RuntimeCallOf<T>> = (0..c)
		.map(|index| {
			let remark = if index == 0 { alloc::vec![0; b as usize] } else { Vec::new() };
			frame_system::Call::<T>::remark { remark }.into()
		})
		.collect();
	let items_hash = Pallet::<T>::items_hash(calls.iter().map(|call| (call, &origin)));
	let items: Vec<_> = calls
		.into_iter()
		.enumerate()
		.map(|(index, call)| {
			let index = index as u32;
			let implication = ItemImplication {
				mode: BatchMode::StopAtFailure,
				submitter: &submitter,
				nonce,
				index,
				items_hash: &items_hash,
			};
			T::BenchmarkHelper::item(call, index.into(), implication.encode())
		})
		.collect();
	let items = items.try_into().expect("`c` is bounded by `MaxMultiOriginBatch`; qed");
	(submitter, nonce, items)
}

#[benchmarks]
mod benchmark {
	use super::*;

	/// Both batch calls run the same code, so this is the weight of both.
	#[benchmark]
	fn batch_multi_origin(
		c: Linear<1, { T::MaxMultiOriginBatch::get() }>,
		b: Linear<0, { *T::BlockLength::get().max.get(DispatchClass::Normal) }>,
	) {
		let (submitter, nonce, items) = batch::<T>(c, b);

		#[extrinsic_call]
		_(frame_system::RawOrigin::Signed(submitter), items, nonce);

		frame_system::Pallet::<T>::assert_last_event(Event::BatchCompleted.into());
	}

	impl_benchmark_test_suite! {
		Pallet,
		tests::new_test_ext(),
		tests::Test
	}
}
