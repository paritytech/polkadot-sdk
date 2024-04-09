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

#![cfg_attr(not(feature = "std"), no_std)]

pub use constants::*;
pub use default_configs::*;

#[cfg(feature = "rococo")]
pub mod rococo;
#[cfg(feature = "westend")]
pub mod westend;

pub mod default_configs;

pub mod constants {
	use frame_support::{
		pallet_prelude::*,
		parameter_types,
		weights::{constants::WEIGHT_REF_TIME_PER_NANOS, RuntimeDbWeight, Weight},
	};
	use frame_system::limits::{BlockLength, BlockWeights};
	use parachains_common::{
		AVERAGE_ON_INITIALIZE_RATIO, MAXIMUM_BLOCK_WEIGHT, NORMAL_DISPATCH_RATIO,
	};

	parameter_types! {
		/// By default, Substrate uses `RocksDB`, so this will be the weight used throughout
		/// the runtime.
		pub const RocksDbWeight: RuntimeDbWeight = RuntimeDbWeight {
			read: 25_000 * WEIGHT_REF_TIME_PER_NANOS,
			write: 100_000 * WEIGHT_REF_TIME_PER_NANOS,
		};
		/// `ParityDB` can be enabled with a feature flag, but is still experimental. These weights
		/// are available for brave runtime engineers who may want to try this out as default.
		pub const ParityDbWeight: RuntimeDbWeight = RuntimeDbWeight {
			read: 8_000 * WEIGHT_REF_TIME_PER_NANOS,
			write: 50_000 * WEIGHT_REF_TIME_PER_NANOS,
		};

		/// Importing a block with 0 Extrinsics.
		pub const BlockExecutionWeight: Weight =
			Weight::from_parts(WEIGHT_REF_TIME_PER_NANOS.saturating_mul(5_000_000), 0);
		/// Executing a NO-OP `System::remarks` Extrinsic.
		pub const ExtrinsicBaseWeight: Weight =
			Weight::from_parts(WEIGHT_REF_TIME_PER_NANOS.saturating_mul(125_000), 0);

		/// The block length of a parachain runtime.
		pub RuntimeBlockLength: BlockLength =
			BlockLength::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
		/// The block weight of a parachain runtime.
		pub RuntimeBlockWeights: BlockWeights = BlockWeights::builder()
			.base_block(BlockExecutionWeight::get())
			.for_class(DispatchClass::all(), |weights| {
				weights.base_extrinsic = ExtrinsicBaseWeight::get();
			})
			.for_class(DispatchClass::Normal, |weights| {
				weights.max_total = Some(NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT);
			})
			.for_class(DispatchClass::Operational, |weights| {
				weights.max_total = Some(MAXIMUM_BLOCK_WEIGHT);
				// Operational transactions have some extra reserved space, so that they
				// are included even if block reached `MAXIMUM_BLOCK_WEIGHT`.
				weights.reserved = Some(
					MAXIMUM_BLOCK_WEIGHT - NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT
				);
			})
			.avg_block_initialization(AVERAGE_ON_INITIALIZE_RATIO)
			.build_or_panic();
	}
}

/// Check that all weight constants exist and have sane values.
#[cfg(test)]
mod test_weights {
	use frame_support::weights::constants;

	#[test]
	fn sane_rocks_db_weights() {
		let w = super::RocksDbWeight::get();

		// At least 1 µs.
		assert!(
			w.reads(1).ref_time() >= constants::WEIGHT_REF_TIME_PER_MICROS,
			"Read weight should be at least 1 µs."
		);
		assert!(
			w.writes(1).ref_time() >= constants::WEIGHT_REF_TIME_PER_MICROS,
			"Write weight should be at least 1 µs."
		);

		// At most 1 ms.
		assert!(
			w.reads(1).ref_time() <= constants::WEIGHT_REF_TIME_PER_MILLIS,
			"Read weight should be at most 1 ms."
		);
		assert!(
			w.writes(1).ref_time() <= constants::WEIGHT_REF_TIME_PER_MILLIS,
			"Write weight should be at most 1 ms."
		);
	}

	#[test]
	fn sane_parity_db_weights() {
	let w = super::ParityDbWeight::get();

		// At least 1 µs.
		assert!(
			w.reads(1).ref_time() >= constants::WEIGHT_REF_TIME_PER_MICROS,
			"Read weight should be at least 1 µs."
		);
		assert!(
			w.writes(1).ref_time() >= constants::WEIGHT_REF_TIME_PER_MICROS,
			"Write weight should be at least 1 µs."
		);

		// At most 1 ms.
		assert!(
			w.reads(1).ref_time() <= constants::WEIGHT_REF_TIME_PER_MILLIS,
			"Read weight should be at most 1 ms."
		);
		assert!(
			w.writes(1).ref_time() <= constants::WEIGHT_REF_TIME_PER_MILLIS,
			"Write weight should be at most 1 ms."
		);
	}

	#[test]
	fn sane_block_execution_weights() {
		let w = super::BlockExecutionWeight::get();

		// At least 100 µs.
		assert!(
			w.ref_time() >= 100u64 * constants::WEIGHT_REF_TIME_PER_MICROS,
			"Weight should be at least 100 µs."
		);

		// At most 50 ms.
		assert!(
			w.ref_time() <= 50u64 * constants::WEIGHT_REF_TIME_PER_MILLIS,
			"Weight should be at most 50 ms."
		);
	}

	#[test]
	fn sane_extrinsic_base_weights() {
		let w = super::ExtrinsicBaseWeight::get();

		// At least 10 µs.
		assert!(
			w.ref_time() >= 10u64 * constants::WEIGHT_REF_TIME_PER_MICROS,
			"Weight should be at least 10 µs."
		);

		// At most 1 ms.
		assert!(
			w.ref_time() <= constants::WEIGHT_REF_TIME_PER_MILLIS,
			"Weight should be at most 1 ms."
		);
	}
}
