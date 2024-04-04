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
		v2,
		v2::{weights, weights::WeightInfo},
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
		let multi_account = multi_account_id(&[1, 2, 3][..], 2);
		let call = call_transfer(6, 15).encode();
		let hash = blake2_256(&call);
		let val = OldMultisig { when: now(), deposit: 100, depositor: 1, approvals: &[1] };

		v2::v1::Multisigs::<T>::insert(multi_account, hash, val);
		let mut meter = WeightMeter::new();

		#[block]
		{
			v2::LazyMigrationV2::<T, weights::SubstrateWeight<T>>::step(None, &mut meter).unwrap();
		}

		// Check that the new storage is decodable:
		assert_eq!(crate::Multisigs::<T>::get(multi_account), Some(val));
		// uses twice the weight once for migration and then for checking if there is another key.
		assert_eq!(meter.consumed(), weights::SubstrateWeight::<T>::step() * 2);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Runtime);
}
