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

use core::marker::PhantomData;
use frame_support::weights::{constants, Weight};

pub struct WeightInfo<T>(PhantomData<T>);
impl<T: crate::Config> WeightInfo<T> {
	// Copy from `pallet_xcm_benchmarks_generic::WeightInfo::barrier_check`
	// Storage: `PolkadotXcm::ShouldRecordXcm` (r:1 w:0)
	// Proof: `PolkadotXcm::ShouldRecordXcm` (`max_values`: Some(1), `max_size`: None, mode:
	// `Measured`)
	pub fn barrier_check() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `1485`
		// Minimum execution time: 5_452_000 picoseconds.
		Weight::from_parts(5_653_000, 1485)
			.saturating_add(Weight::from_parts(13_036 * constants::WEIGHT_REF_TIME_PER_NANOS, 0)) // Copy from `inmemorydb_weights::constants::InMemoryDbWeight`
	}
}
