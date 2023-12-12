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

#![allow(clippy::all)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};
use sp_std::marker::PhantomData;

pub trait WeightInfo {
	fn parachain_extension(c: u32) -> Weight;
	fn grandpa_extension(c: u32) -> Weight;
}

pub struct BridgeWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for BridgeWeight<T> {
	fn parachain_extension(c: u32) -> Weight {
		Weight::from_parts(38_597_828, 4648)
			// Standard Error: 190_859
			.saturating_add(Weight::from_parts(60_685, 0).saturating_mul(c.into()))
			.saturating_add(T::DbWeight::get().reads(4_u64))
			.saturating_add(T::DbWeight::get().writes(3_u64))
	}
	fn grandpa_extension(c: u32) -> Weight {
		Weight::from_parts(38_597_828, 4648)
			// Standard Error: 190_859
			.saturating_add(Weight::from_parts(60_685, 0).saturating_mul(c.into()))
			.saturating_add(T::DbWeight::get().reads(4_u64))
			.saturating_add(T::DbWeight::get().writes(3_u64))
	}
}

impl WeightInfo for () {
	fn parachain_extension(_c: u32) -> Weight {
		Weight::zero()
	}
	fn grandpa_extension(_c: u32) -> Weight {
		Weight::zero()
	}
}
