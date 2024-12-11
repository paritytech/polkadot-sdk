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

//! Benchmarks for remarks pallet

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use alloc::vec;
use frame_benchmarking::v2::*;
use frame_system::{EventRecord, Pallet as System, RawOrigin};

#[cfg(test)]
use crate::Pallet as Remark;

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	let events = System::<T>::events();
	let system_event: <T as frame_system::Config>::RuntimeEvent = generic_event.into();
	let EventRecord { event, .. } = &events[events.len() - 1];
	assert_eq!(event, &system_event);
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn store(l: Linear<1, { 1024 * 1024 }>) {
		let caller: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), vec![0u8; l as usize]);

		assert_last_event::<T>(
			Event::Stored {
				sender: caller,
				content_hash: sp_io::hashing::blake2_256(&vec![0u8; l as usize]).into(),
			}
			.into(),
		);
	}

	impl_benchmark_test_suite!(Remark, crate::mock::new_test_ext(), crate::mock::Test);
}
