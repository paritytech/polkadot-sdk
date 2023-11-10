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

#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::v2::*;
use frame_system::{Pallet as System, RawOrigin};
use sp_std::vec;

#[benchmarks]
mod benches {
	use super::*;
	use frame_support::traits::Hooks;

	#[benchmark]
	fn on_runtime_upgrade() {
		assert!(!Cursor::<T>::exists());

		#[block]
		{
			Pallet::<T>::on_runtime_upgrade();
		}
	}

	#[benchmark]
	fn on_init_fast_path() {
		Cursor::<T>::set(Some(cursor::<T>()));
		System::<T>::set_block_number(1u32.into());

		#[block]
		{
			Pallet::<T>::on_initialize(1u32.into());
		}
	}

	#[benchmark]
	fn on_init_base() {
		// FAIL-CI
		Cursor::<T>::set(Some(cursor::<T>()));
		System::<T>::set_block_number(1u32.into());

		#[block]
		{
			Pallet::<T>::on_initialize(1u32.into());
		}
	}

	#[benchmark]
	fn on_init_loop() {
		System::<T>::set_block_number(1u32.into());
		Pallet::<T>::on_runtime_upgrade();

		#[block]
		{
			Pallet::<T>::on_initialize(1u32.into());
		}
	}

	#[benchmark]
	fn force_set_cursor() {
		#[extrinsic_call]
		_(RawOrigin::Root, Some(cursor::<T>()));
	}

	#[benchmark]
	fn force_set_active_cursor() {
		#[extrinsic_call]
		_(RawOrigin::Root, 0, None, None);
	}

	#[benchmark]
	fn clear_historic(n: Linear<0, { DEFAULT_HISTORIC_BATCH_CLEAR_SIZE * 2 }>) {
		let id_max_len = <T as Config>::IdentifierMaxLen::get();
		assert!(id_max_len >= 4, "Precondition violated");

		for i in 0..DEFAULT_HISTORIC_BATCH_CLEAR_SIZE * 2 {
			let id = IdentifierOf::<T>::truncate_from(
				i.encode().into_iter().cycle().take(id_max_len as usize).collect::<Vec<_>>(),
			);

			Historic::<T>::insert(&id, ());
		}

		#[extrinsic_call]
		_(
			RawOrigin::Root,
			HistoricCleanupSelector::Wildcard { limit: n.into(), previous_cursor: None },
		);
	}

	fn cursor<T: Config>() -> CursorOf<T> {
		// Note: The weight of a function can depend on the weight of reading the `inner_cursor`.
		// `Cursor` is a user provided type. Now instead of requiring something like `Cursor:
		// From<u32>`, we instead rely on the fact that it is MEL and the PoV benchmarking will
		// therefore already take the MEL bound, even when the cursor in storage is `None`.
		MigrationCursor::Active(ActiveCursor {
			index: u32::MAX,
			inner_cursor: None,
			started_at: 0u32.into(),
		})
	}

	// Implements a test for each benchmark. Execute with:
	// `cargo test -p pallet-migrations --features runtime-benchmarks`.
	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
