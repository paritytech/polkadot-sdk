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

use crate::*;
use frame_support::{
	parameter_types,
	weights::{constants, RuntimeDbWeight},
};

/// DB Weight config trait adapter for AH migrator pallet weights.
pub trait DbConfig {
	type DbWeight: Get<RuntimeDbWeight>;
}

/// DB Weight config type adapter for AH migrator pallet weights.
pub struct AhDbConfig;
impl DbConfig for AhDbConfig {
	type DbWeight = RocksDbWeight;
}

parameter_types! {
	/// Asset Hub DB Weights.
	///
	/// Copied from `asset_hub_polkadot::weights::RocksDbWeight`.
	pub const RocksDbWeight: RuntimeDbWeight = RuntimeDbWeight {
		read: 25_000 * constants::WEIGHT_REF_TIME_PER_NANOS,
		write: 100_000 * constants::WEIGHT_REF_TIME_PER_NANOS,
	};
}
