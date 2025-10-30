// Placeholder until actual benchmarking.

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use core::marker::PhantomData;

/// Weight functions needed for `pallet_example_mbm`.
pub trait WeightInfo {
	fn step() -> Weight;
}

/// Weights for `pallet_example_mbm` using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	/// Storage: `PalletExampleMbms::MyMap` (r:2 w:1)
	/// Proof: `PalletExampleMbms::MyMap` (`max_values`: None, `max_size`: Some(28), added: 2503, mode: `MaxEncodedLen`)
	fn step() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `28`
		//  Estimated: `5996`
		// Minimum execution time: 6_000_000 picoseconds.
		Weight::from_parts(8_000_000, 5996)
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
}

// For backwards compatibility and tests.
impl WeightInfo for () {
	/// Storage: `PalletExampleMbms::MyMap` (r:2 w:1)
	/// Proof: `PalletExampleMbms::MyMap` (`max_values`: None, `max_size`: Some(28), added: 2503, mode: `MaxEncodedLen`)
	fn step() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `28`
		//  Estimated: `5996`
		// Minimum execution time: 6_000_000 picoseconds.
		Weight::from_parts(8_000_000, 5996)
			.saturating_add(RocksDbWeight::get().reads(2_u64))
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
}

