// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

use frame_support::weights::{constants::RocksDbWeight, Weight};

/// Weight functions needed for pallet_bridge_proof_root_sync.
pub trait WeightInfo {
	fn on_idle() -> Weight;
}

// For backwards compatibility and tests
impl WeightInfo for () {
	fn on_idle() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `204`
		//  Estimated: `6070`
		// Minimum execution time: 19_370_000 picoseconds.
		Weight::from_parts(928_000, 0)
			.saturating_add(Weight::from_parts(0, 6070))
			.saturating_add(RocksDbWeight::get().reads(1))
			.saturating_add(RocksDbWeight::get().writes(1))
	}
}
