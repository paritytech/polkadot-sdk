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

//! Benchmark the multi-block-migration.

#![cfg(feature = "runtime-benchmarks")]

use crate::{
	migrations::{
		v1,
		v1::{weights, weights::WeightInfo},
	},
	Config, Pallet,
};
use frame_benchmarking::v2::*;
use frame_support::{migrations::SteppedMigration, weights::WeightMeter};

#[benchmarks]
mod benches {
	use super::*;

	/// Benchmark a single step of the `v1::LazyMigrationV1` migration.
	#[benchmark]
	fn step() {
		v1::v0::MyMap::<T>::insert(0, 0);
		let mut meter = WeightMeter::new();

		#[block]
		{
			v1::LazyMigrationV1::<T, weights::SubstrateWeight<T>>::step(None, &mut meter).unwrap();
		}

		// Check that the new storage is decodable:
		assert_eq!(crate::MyMap::<T>::get(0), Some(0));
		// uses twice the weight once for migration and then for checking if there is another key.
		assert_eq!(meter.consumed(), weights::SubstrateWeight::<T>::step() * 2);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Runtime);
}
