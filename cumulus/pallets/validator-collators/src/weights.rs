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
}

/// Default weights for `pallet_validator_collators`, see the module doc for their source.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	/// Storage: `ValidatorCollators::ValidatorSet` (r:1 w:1)
	/// Proof: `ValidatorCollators::ValidatorSet` (`max_values`: Some(1), `max_size`: Some(32006),
	/// added: 32501, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorCollators::PendingRotation` (r:0 w:1)
	/// Proof: `ValidatorCollators::PendingRotation` (`max_values`: Some(1), `max_size`: Some(1),
	/// added: 496, mode: `MaxEncodedLen`)
	/// The range of component `n` is `[1, 1000]`.
	fn set_validators(n: u32) -> Weight {
		Weight::from_parts(21_223_720, 33_491)
			.saturating_add(Weight::from_parts(134_848, 0).saturating_mul(n.into()))
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}
	/// Storage: `ValidatorCollators::MaxCollators` (r:0 w:1)
	/// Proof: `ValidatorCollators::MaxCollators` (`max_values`: Some(1), `max_size`: Some(4),
	/// added: 499, mode: `MaxEncodedLen`)
	fn set_max_collators() -> Weight {
		Weight::from_parts(5_786_000, 0).saturating_add(T::DbWeight::get().writes(1_u64))
	}
}

// For backwards compatibility and tests.
impl WeightInfo for () {
	/// Storage: `ValidatorCollators::ValidatorSet` (r:1 w:1)
	/// Proof: `ValidatorCollators::ValidatorSet` (`max_values`: Some(1), `max_size`: Some(32006),
	/// added: 32501, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorCollators::PendingRotation` (r:0 w:1)
	/// Proof: `ValidatorCollators::PendingRotation` (`max_values`: Some(1), `max_size`: Some(1),
	/// added: 496, mode: `MaxEncodedLen`)
	/// The range of component `n` is `[1, 1000]`.
	fn set_validators(n: u32) -> Weight {
		Weight::from_parts(21_223_720, 33_491)
			.saturating_add(Weight::from_parts(134_848, 0).saturating_mul(n.into()))
			.saturating_add(RocksDbWeight::get().reads(1_u64))
			.saturating_add(RocksDbWeight::get().writes(2_u64))
	}
	/// Storage: `ValidatorCollators::MaxCollators` (r:0 w:1)
	/// Proof: `ValidatorCollators::MaxCollators` (`max_values`: Some(1), `max_size`: Some(4),
	/// added: 499, mode: `MaxEncodedLen`)
	fn set_max_collators() -> Weight {
		Weight::from_parts(5_786_000, 0).saturating_add(RocksDbWeight::get().writes(1_u64))
	}
}
