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

//! Benchmarking for the `cumulus-pallet-dmp-queue`.

#![cfg(feature = "runtime-benchmarks")]

use crate::*;

use frame_benchmarking::v2::*;
use frame_support::{pallet_prelude::*, traits::Hooks};
use sp_std::vec;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn on_idle_migration() {
		let msg = vec![123; 1 << 16]; // 64 KiB message

		Pages::<T>::insert(0, vec![(123, msg.clone())]);
		PageIndex::<T>::put(PageIndexData { begin_used: 0, end_used: 1, overweight_count: 0 });
		MigrationStatus::<T>::set(MigrationState::StartedExport { next_begin_used: 0 });

		#[block]
		{
			Pallet::<T>::on_idle(0u32.into(), Weight::MAX);
		}

		assert_last_event::<T>(Event::Exported { page: 0 }.into());
	}
}

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	let events = frame_system::Pallet::<T>::events();
	let system_event: <T as frame_system::Config>::RuntimeEvent = generic_event.into();
	// compare to the last event record
	let frame_system::EventRecord { event, .. } = events.last().expect("Event expected");
	assert_eq!(event, &system_event);
}
