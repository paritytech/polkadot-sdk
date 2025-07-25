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

//! Glutton pallet benchmarking.
//!
//! Has to be compiled and run twice to calibrate on new hardware.

#[cfg(feature = "runtime-benchmarks")]
use frame_benchmarking::v2::*;
use frame_support::{pallet_prelude::*, weights::constants::*};
use frame_system::RawOrigin;
use sp_runtime::{traits::One, Perbill};

use crate::*;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn initialize_pallet_grow(n: Linear<0, 1_000>) -> Result<(), BenchmarkError> {
		#[block]
		{
			Pallet::<T>::initialize_pallet(RawOrigin::Root.into(), n, None)?;
		}

		assert_eq!(TrashDataCount::<T>::get(), n);

		Ok(())
	}

	#[benchmark]
	fn initialize_pallet_shrink(n: Linear<0, 1_000>) -> Result<(), BenchmarkError> {
		Pallet::<T>::initialize_pallet(RawOrigin::Root.into(), n, None)?;

		#[block]
		{
			Pallet::<T>::initialize_pallet(RawOrigin::Root.into(), 0, Some(n))?;
		}

		assert_eq!(TrashDataCount::<T>::get(), 0);

		Ok(())
	}

	#[benchmark]
	fn waste_ref_time_iter(i: Linear<0, 100_000>) {
		#[block]
		{
			Pallet::<T>::waste_ref_time_iter(vec![0u8; 64], i);
		}
	}

	#[benchmark]
	fn waste_proof_size_some(i: Linear<0, 5_000>) {
		(0..5000).for_each(|i| TrashData::<T>::insert(i, [i as u8; 1024]));

		#[block]
		{
			(0..i).for_each(|i| {
				TrashData::<T>::get(i);
			})
		}
	}

	// For manual verification only.
	#[benchmark]
	fn on_idle_high_proof_waste() {
		(0..5000).for_each(|i| TrashData::<T>::insert(i, [i as u8; 1024]));
		let _ = Pallet::<T>::set_compute(RawOrigin::Root.into(), One::one());
		let _ = Pallet::<T>::set_storage(RawOrigin::Root.into(), One::one());

		#[block]
		{
			Pallet::<T>::on_idle(
				frame_system::Pallet::<T>::block_number(),
				Weight::from_parts(WEIGHT_REF_TIME_PER_MILLIS * 100, WEIGHT_PROOF_SIZE_PER_MB * 5),
			);
		}
	}

	// For manual verification only.
	#[benchmark]
	fn on_idle_low_proof_waste() {
		(0..5000).for_each(|i| TrashData::<T>::insert(i, [i as u8; 1024]));
		let _ = Pallet::<T>::set_compute(RawOrigin::Root.into(), One::one());
		let _ = Pallet::<T>::set_storage(RawOrigin::Root.into(), One::one());

		#[block]
		{
			Pallet::<T>::on_idle(
				frame_system::Pallet::<T>::block_number(),
				Weight::from_parts(WEIGHT_REF_TIME_PER_MILLIS * 100, WEIGHT_PROOF_SIZE_PER_KB * 20),
			);
		}
	}

	#[benchmark]
	fn empty_on_idle() {
		// Enough weight to do nothing.
		#[block]
		{
			Pallet::<T>::on_idle(
				frame_system::Pallet::<T>::block_number(),
				T::WeightInfo::empty_on_idle(),
			);
		}
	}

	#[benchmark]
	fn set_compute() {
		#[extrinsic_call]
		_(RawOrigin::Root, FixedU64::from_perbill(Perbill::from_percent(50)));
	}

	#[benchmark]
	fn set_storage() {
		#[extrinsic_call]
		_(RawOrigin::Root, FixedU64::from_perbill(Perbill::from_percent(50)));
	}

	impl_benchmark_test_suite! {
		Pallet,
		mock::new_test_ext(),
		mock::Test
	}
}
