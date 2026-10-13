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

//! Default weights for `pallet_validator_collators`.
//!
//! Runtimes should use their own benchmarked weights.

#![allow(unused_parens)]
#![allow(unused_imports)]

use core::marker::PhantomData;
use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};

/// Weight functions needed for `pallet_validator_collators`.
pub trait WeightInfo {
	fn set_validators(n: u32) -> Weight;
	fn set_max_collators() -> Weight;
	fn on_relay_state_proof() -> Weight;
	fn set_report_period() -> Weight;
	fn set_expected_era_duration() -> Weight;
	fn set_report_margin() -> Weight;
	fn note_author() -> Weight;
	fn report_blocks(n: u32) -> Weight;
}

/// Default weights for `pallet_validator_collators`, see the module doc for their source.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	/// Storage: `ValidatorCollators::ValidatorSet` (r:1 w:1)
	/// Proof: `ValidatorCollators::ValidatorSet` (`max_values`: Some(1), `max_size`: Some(32006),
	/// added: 32501, mode: `MaxEncodedLen`)
	/// Storage: `Timestamp::Now` (r:1 w:0)
	/// Proof: `Timestamp::Now` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode:
	/// `MaxEncodedLen`)
	/// Storage: `ValidatorCollators::SetReceivedAt` (r:0 w:1)
	/// Proof: `ValidatorCollators::SetReceivedAt` (`max_values`: Some(1), `max_size`: Some(8),
	/// added: 503, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorCollators::PendingRotation` (r:0 w:1)
	/// Proof: `ValidatorCollators::PendingRotation` (`max_values`: Some(1), `max_size`: Some(1),
	/// added: 496, mode: `MaxEncodedLen`)
	/// The range of component `n` is `[1, 1000]`.
	fn set_validators(n: u32) -> Weight {
		Weight::from_parts(21_223_720, 33_994)
			.saturating_add(Weight::from_parts(134_848, 0).saturating_mul(n.into()))
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(3_u64))
	}
	/// Storage: `ValidatorCollators::ValidatorSet` (r:1 w:0)
	/// Proof: `ValidatorCollators::ValidatorSet` (`max_values`: Some(1), `max_size`: Some(32006),
	/// added: 32501, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorCollators::MaxCollators` (r:0 w:1)
	/// Proof: `ValidatorCollators::MaxCollators` (`max_values`: Some(1), `max_size`: Some(4),
	/// added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorCollators::PendingRotation` (r:0 w:1)
	/// Proof: `ValidatorCollators::PendingRotation` (`max_values`: Some(1), `max_size`: Some(1),
	/// added: 496, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorCollators::Collators` (r:0 w:1)
	/// Proof: `ValidatorCollators::Collators` (`max_values`: Some(1), `max_size`: Some(32038),
	/// added: 32533, mode: `MaxEncodedLen`)
	fn set_max_collators() -> Weight {
		Weight::from_parts(10_935_000, 33_491)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(3_u64))
	}
	/// Storage: `ValidatorCollators::EpochRandomness` (r:1 w:1)
	/// Proof: `ValidatorCollators::EpochRandomness` (`max_values`: Some(1), `max_size`: Some(32),
	/// added: 527, mode: `MaxEncodedLen`)
	fn on_relay_state_proof() -> Weight {
		Weight::from_parts(4_748_000, 1_517)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `ValidatorCollators::ReportPeriod` (r:0 w:1)
	/// Proof: `ValidatorCollators::ReportPeriod` (`max_values`: Some(1), `max_size`: Some(4),
	/// added: 499, mode: `MaxEncodedLen`)
	fn set_report_period() -> Weight {
		Weight::from_parts(5_761_000, 0).saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `ValidatorCollators::ExpectedEraDuration` (r:0 w:1)
	/// Proof: `ValidatorCollators::ExpectedEraDuration` (`max_values`: Some(1), `max_size`:
	/// Some(8), added: 503, mode: `MaxEncodedLen`)
	fn set_expected_era_duration() -> Weight {
		Weight::from_parts(5_761_000, 0).saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `ValidatorCollators::ReportMargin` (r:0 w:1)
	/// Proof: `ValidatorCollators::ReportMargin` (`max_values`: Some(1), `max_size`: Some(8),
	/// added: 503, mode: `MaxEncodedLen`)
	fn set_report_margin() -> Weight {
		Weight::from_parts(5_761_000, 0).saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `ValidatorCollators::AuthoredBlocks` (r:1 w:1)
	/// Proof: `ValidatorCollators::AuthoredBlocks` (`max_values`: None, `max_size`: Some(44),
	/// added: 2519, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorCollators::CounterForAuthoredBlocks` (r:1 w:1)
	/// Proof: `ValidatorCollators::CounterForAuthoredBlocks` (`max_values`: Some(1),
	/// `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	fn note_author() -> Weight {
		Weight::from_parts(5_651_000, 3_509)
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}
	/// Storage: `ValidatorCollators::CounterForAuthoredBlocks` (r:1 w:1)
	/// Proof: `ValidatorCollators::CounterForAuthoredBlocks` (`max_values`: Some(1),
	/// `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorCollators::ValidatorSet` (r:1 w:0)
	/// Proof: `ValidatorCollators::ValidatorSet` (`max_values`: Some(1), `max_size`: Some(32006),
	/// added: 32501, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorCollators::AuthoredBlocks` (r:1001 w:1000)
	/// Proof: `ValidatorCollators::AuthoredBlocks` (`max_values`: None, `max_size`: Some(44),
	/// added: 2519, mode: `MaxEncodedLen`)
	/// The range of component `n` is `[1, 1000]`.
	fn report_blocks(n: u32) -> Weight {
		Weight::from_parts(92_795_000, 33_491)
			.saturating_add(Weight::from_parts(7_573_800, 2_519).saturating_mul(n.into()))
			.saturating_add(T::DbWeight::get().reads(3_u64))
			.saturating_add(T::DbWeight::get().reads((1_u64).saturating_mul(n.into())))
			.saturating_add(T::DbWeight::get().writes(1_u64))
			.saturating_add(T::DbWeight::get().writes((1_u64).saturating_mul(n.into())))
	}
}

// For backwards compatibility and tests.
impl WeightInfo for () {
	/// Storage: `ValidatorCollators::ValidatorSet` (r:1 w:1)
	/// Proof: `ValidatorCollators::ValidatorSet` (`max_values`: Some(1), `max_size`: Some(32006),
	/// added: 32501, mode: `MaxEncodedLen`)
	/// Storage: `Timestamp::Now` (r:1 w:0)
	/// Proof: `Timestamp::Now` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode:
	/// `MaxEncodedLen`)
	/// Storage: `ValidatorCollators::SetReceivedAt` (r:0 w:1)
	/// Proof: `ValidatorCollators::SetReceivedAt` (`max_values`: Some(1), `max_size`: Some(8),
	/// added: 503, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorCollators::PendingRotation` (r:0 w:1)
	/// Proof: `ValidatorCollators::PendingRotation` (`max_values`: Some(1), `max_size`: Some(1),
	/// added: 496, mode: `MaxEncodedLen`)
	/// The range of component `n` is `[1, 1000]`.
	fn set_validators(n: u32) -> Weight {
		Weight::from_parts(21_223_720, 33_994)
			.saturating_add(Weight::from_parts(134_848, 0).saturating_mul(n.into()))
			.saturating_add(RocksDbWeight::get().reads(2_u64))
			.saturating_add(RocksDbWeight::get().writes(3_u64))
	}
	/// Storage: `ValidatorCollators::ValidatorSet` (r:1 w:0)
	/// Proof: `ValidatorCollators::ValidatorSet` (`max_values`: Some(1), `max_size`: Some(32006),
	/// added: 32501, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorCollators::MaxCollators` (r:0 w:1)
	/// Proof: `ValidatorCollators::MaxCollators` (`max_values`: Some(1), `max_size`: Some(4),
	/// added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorCollators::PendingRotation` (r:0 w:1)
	/// Proof: `ValidatorCollators::PendingRotation` (`max_values`: Some(1), `max_size`: Some(1),
	/// added: 496, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorCollators::Collators` (r:0 w:1)
	/// Proof: `ValidatorCollators::Collators` (`max_values`: Some(1), `max_size`: Some(32038),
	/// added: 32533, mode: `MaxEncodedLen`)
	fn set_max_collators() -> Weight {
		Weight::from_parts(10_935_000, 33_491)
			.saturating_add(RocksDbWeight::get().reads(1_u64))
			.saturating_add(RocksDbWeight::get().writes(3_u64))
	}
	/// Storage: `ValidatorCollators::EpochRandomness` (r:1 w:1)
	/// Proof: `ValidatorCollators::EpochRandomness` (`max_values`: Some(1), `max_size`: Some(32),
	/// added: 527, mode: `MaxEncodedLen`)
	fn on_relay_state_proof() -> Weight {
		Weight::from_parts(4_748_000, 1_517)
			.saturating_add(RocksDbWeight::get().reads(1_u64))
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	/// Storage: `ValidatorCollators::ReportPeriod` (r:0 w:1)
	/// Proof: `ValidatorCollators::ReportPeriod` (`max_values`: Some(1), `max_size`: Some(4),
	/// added: 499, mode: `MaxEncodedLen`)
	fn set_report_period() -> Weight {
		Weight::from_parts(5_761_000, 0).saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	/// Storage: `ValidatorCollators::ExpectedEraDuration` (r:0 w:1)
	/// Proof: `ValidatorCollators::ExpectedEraDuration` (`max_values`: Some(1), `max_size`:
	/// Some(8), added: 503, mode: `MaxEncodedLen`)
	fn set_expected_era_duration() -> Weight {
		Weight::from_parts(5_761_000, 0).saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	/// Storage: `ValidatorCollators::ReportMargin` (r:0 w:1)
	/// Proof: `ValidatorCollators::ReportMargin` (`max_values`: Some(1), `max_size`: Some(8),
	/// added: 503, mode: `MaxEncodedLen`)
	fn set_report_margin() -> Weight {
		Weight::from_parts(5_761_000, 0).saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	/// Storage: `ValidatorCollators::AuthoredBlocks` (r:1 w:1)
	/// Proof: `ValidatorCollators::AuthoredBlocks` (`max_values`: None, `max_size`: Some(44),
	/// added: 2519, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorCollators::CounterForAuthoredBlocks` (r:1 w:1)
	/// Proof: `ValidatorCollators::CounterForAuthoredBlocks` (`max_values`: Some(1),
	/// `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	fn note_author() -> Weight {
		Weight::from_parts(5_651_000, 3_509)
			.saturating_add(RocksDbWeight::get().reads(2_u64))
			.saturating_add(RocksDbWeight::get().writes(2_u64))
	}
	/// Storage: `ValidatorCollators::CounterForAuthoredBlocks` (r:1 w:1)
	/// Proof: `ValidatorCollators::CounterForAuthoredBlocks` (`max_values`: Some(1),
	/// `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorCollators::ValidatorSet` (r:1 w:0)
	/// Proof: `ValidatorCollators::ValidatorSet` (`max_values`: Some(1), `max_size`: Some(32006),
	/// added: 32501, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorCollators::AuthoredBlocks` (r:1001 w:1000)
	/// Proof: `ValidatorCollators::AuthoredBlocks` (`max_values`: None, `max_size`: Some(44),
	/// added: 2519, mode: `MaxEncodedLen`)
	/// The range of component `n` is `[1, 1000]`.
	fn report_blocks(n: u32) -> Weight {
		Weight::from_parts(92_795_000, 33_491)
			.saturating_add(Weight::from_parts(7_573_800, 2_519).saturating_mul(n.into()))
			.saturating_add(RocksDbWeight::get().reads(3_u64))
			.saturating_add(RocksDbWeight::get().reads((1_u64).saturating_mul(n.into())))
			.saturating_add(RocksDbWeight::get().writes(1_u64))
			.saturating_add(RocksDbWeight::get().writes((1_u64).saturating_mul(n.into())))
	}
}
