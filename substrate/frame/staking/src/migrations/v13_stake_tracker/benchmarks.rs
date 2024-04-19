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

//! Benchmark the v13 multi-block-migrations

#![cfg(feature = "runtime-benchmarks")]

use crate::{
	migrations::{v13_stake_tracker, v13_stake_tracker::*},
	Config, Pallet,
};
use frame_benchmarking::v2::*;
use frame_support::{migrations::SteppedMigration, weights::WeightMeter};

#[benchmarks]
mod benches {
	use super::*;

	/// Benchmark a simple step of the v13 multi-block migration.
	#[benchmark]
	fn step() {
		let mut meter = WeightMeter::new();

		#[block]
		{
			v13_stake_tracker::MigrationV13::<T, weights::SubstrateWeight<T>>::step(None, &mut meter).unwrap();
		}

		// TODO: after benchmarks sanity checks.
	}

	impl_benchmark_test_suite!(
		Staking,
		crate::mock::ExtBuilder::default().has_stakers(true),
		crate::mock::Test,
		exec_name = build_and_execute
	);
}
