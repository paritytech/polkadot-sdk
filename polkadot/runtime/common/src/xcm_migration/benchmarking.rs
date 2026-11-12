// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Benchmarks of the stages of [`super::MigrateXcmToLatest`].
//!
//! This isn't a pallet: a runtime registers it once per stage it uses, each time with a separate
//! instance. The name of the type alias ends up in the name of the generated weight file, e.g.
//!
//! ```ignore
//! use polkadot_runtime_common::xcm_migration::{self, benchmarking};
//!
//! impl benchmarking::Config<Instance1> for Runtime {
//! 	type Stage = xcm_migration::AssetRateStage<
//! 		Runtime,
//! 		xcm_migration::VersionedLocatableAssetToLatest,
//! 		AssetRateWeights,
//! 	>;
//! }
//! pub type AssetRateStage = benchmarking::Pallet<Runtime, Instance1>;
//!
//! frame_benchmarking::define_benchmarks!(
//! 	[polkadot_runtime_common::xcm_migration, AssetRateStage]
//! );
//! ```

use super::MigrationStage;
use core::marker::PhantomData;
use frame_benchmarking::v2::*;
use frame_support::weights::WeightMeter;

/// The type the benchmarks are implemented on.
pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

/// Configures which stage the benchmarks measure.
pub trait Config<I: 'static = ()>: frame_system::Config {
	/// The stage to benchmark.
	type Stage: MigrationStage;
}

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn step_base() {
		let mut meter = WeightMeter::new();
		let result;

		#[block]
		{
			result = T::Stage::step(None, &mut meter);
		}

		assert_eq!(result, Ok(None));
	}

	#[benchmark]
	fn step_item() {
		T::Stage::setup_worst_case_entry();
		let mut meter = WeightMeter::new();
		let result;

		#[block]
		{
			result = T::Stage::step(None, &mut meter);
		}

		assert_eq!(result, Ok(None));
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::xcm_migration::tests::new_test_ext(),
		crate::xcm_migration::tests::Test,
	);
}
