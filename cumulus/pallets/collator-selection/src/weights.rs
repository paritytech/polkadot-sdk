// This file is part of Substrate.

// Copyright (C) 2021 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};
use sp_std::marker::PhantomData;

// The weight info trait for `pallet_collator_selection`.
pub trait WeightInfo {
	fn set_invulnerables(_b: u32) -> Weight;
	fn set_desired_candidates() -> Weight;
	fn set_candidacy_bond() -> Weight;
	fn register_as_candidate(_c: u32) -> Weight;
	fn leave_intent(_c: u32) -> Weight;
	fn note_author() -> Weight;
	fn new_session(_c: u32, _r: u32) -> Weight;
}

/// Weights for pallet_collator_selection using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn set_invulnerables(b: u32) -> Weight {
		Weight::from_parts(18_563_000_u64, 0)
			// Standard Error: 0
			.saturating_add(Weight::from_parts(68_000_u64, 0).saturating_mul(b as u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	fn set_desired_candidates() -> Weight {
		Weight::from_parts(16_363_000_u64, 0).saturating_add(T::DbWeight::get().writes(1_u64))
	}
	fn set_candidacy_bond() -> Weight {
		Weight::from_parts(16_840_000_u64, 0).saturating_add(T::DbWeight::get().writes(1_u64))
	}
	fn register_as_candidate(c: u32) -> Weight {
		Weight::from_parts(71_196_000_u64, 0)
			// Standard Error: 0
			.saturating_add(Weight::from_parts(198_000_u64, 0).saturating_mul(c as u64))
			.saturating_add(T::DbWeight::get().reads(4_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}
	fn leave_intent(c: u32) -> Weight {
		Weight::from_parts(55_336_000_u64, 0)
			// Standard Error: 0
			.saturating_add(Weight::from_parts(151_000_u64, 0).saturating_mul(c as u64))
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}
	fn note_author() -> Weight {
		Weight::from_parts(71_461_000_u64, 0)
			.saturating_add(T::DbWeight::get().reads(3_u64))
			.saturating_add(T::DbWeight::get().writes(4_u64))
	}
	fn new_session(r: u32, c: u32) -> Weight {
		Weight::from_parts(0_u64, 0)
			// Standard Error: 1_010_000
			.saturating_add(Weight::from_parts(109_961_000_u64, 0).saturating_mul(r as u64))
			// Standard Error: 1_010_000
			.saturating_add(Weight::from_parts(151_952_000_u64, 0).saturating_mul(c as u64))
			.saturating_add(T::DbWeight::get().reads(1_u64.saturating_mul(r as u64)))
			.saturating_add(T::DbWeight::get().reads(2_u64.saturating_mul(c as u64)))
			.saturating_add(T::DbWeight::get().writes(2_u64.saturating_mul(r as u64)))
			.saturating_add(T::DbWeight::get().writes(2_u64.saturating_mul(c as u64)))
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	fn set_invulnerables(b: u32) -> Weight {
		Weight::from_parts(18_563_000_u64, 0)
			// Standard Error: 0
			.saturating_add(Weight::from_parts(68_000_u64, 0).saturating_mul(b as u64))
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	fn set_desired_candidates() -> Weight {
		Weight::from_parts(16_363_000_u64, 0).saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	fn set_candidacy_bond() -> Weight {
		Weight::from_parts(16_840_000_u64, 0).saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	fn register_as_candidate(c: u32) -> Weight {
		Weight::from_parts(71_196_000_u64, 0)
			// Standard Error: 0
			.saturating_add(Weight::from_parts(198_000_u64, 0).saturating_mul(c as u64))
			.saturating_add(RocksDbWeight::get().reads(4_u64))
			.saturating_add(RocksDbWeight::get().writes(2_u64))
	}
	fn leave_intent(c: u32) -> Weight {
		Weight::from_parts(55_336_000_u64, 0)
			// Standard Error: 0
			.saturating_add(Weight::from_parts(151_000_u64, 0).saturating_mul(c as u64))
			.saturating_add(RocksDbWeight::get().reads(1_u64))
			.saturating_add(RocksDbWeight::get().writes(2_u64))
	}
	fn note_author() -> Weight {
		Weight::from_parts(71_461_000_u64, 0)
			.saturating_add(RocksDbWeight::get().reads(3_u64))
			.saturating_add(RocksDbWeight::get().writes(4_u64))
	}
	fn new_session(r: u32, c: u32) -> Weight {
		Weight::from_parts(0_u64, 0)
			// Standard Error: 1_010_000
			.saturating_add(Weight::from_parts(109_961_000_u64, 0).saturating_mul(r as u64))
			// Standard Error: 1_010_000
			.saturating_add(Weight::from_parts(151_952_000_u64, 0).saturating_mul(c as u64))
			.saturating_add(RocksDbWeight::get().reads(1_u64.saturating_mul(r as u64)))
			.saturating_add(RocksDbWeight::get().reads(2_u64.saturating_mul(c as u64)))
			.saturating_add(RocksDbWeight::get().writes(2_u64.saturating_mul(r as u64)))
			.saturating_add(RocksDbWeight::get().writes(2_u64.saturating_mul(c as u64)))
	}
}
