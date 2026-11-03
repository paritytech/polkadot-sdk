// This file is part of Substrate.

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

//! Weights for `pallet_hrmp_relay`.
//!
//! NOTE: not benchmarked yet. These are conservative estimates from the storage each function
//! touches, to be replaced by generated weights.

#![allow(unused_parens)]
#![allow(unused_imports)]

use core::marker::PhantomData;
use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};

/// Weight functions needed for `pallet_hrmp_relay`.
pub trait WeightInfo {
	/// `receive`, without what `OnDepositSet` spends.
	fn receive() -> Weight;
	/// A deposit request or a release: one pending entry per side, the id, the outbox's length,
	/// one message sent or kept.
	fn try_set_deposit() -> Weight;
	/// Flushing the outbox with `n` messages waiting, all sent.
	fn flush_outbox(n: u32) -> Weight;
}

/// Weights for `pallet_hrmp_relay`, using the runtime's database weights.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn receive() -> Weight {
		Weight::from_parts(25_000_000, 4_000)
			.saturating_add(T::DbWeight::get().reads(3))
			.saturating_add(T::DbWeight::get().writes(3))
	}
	fn try_set_deposit() -> Weight {
		Weight::from_parts(40_000_000, 6_000)
			.saturating_add(T::DbWeight::get().reads(6))
			.saturating_add(T::DbWeight::get().writes(6))
	}
	fn flush_outbox(n: u32) -> Weight {
		Weight::from_parts(5_000_000, 2_000)
			.saturating_add(Weight::from_parts(20_000_000, 2_000).saturating_mul(n.into()))
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes((n > 0) as u64))
			.saturating_add(T::DbWeight::get().reads_writes(3, 2).saturating_mul(n.into()))
	}
}

// For backwards compatibility and tests.
impl WeightInfo for () {
	fn receive() -> Weight {
		Weight::from_parts(25_000_000, 4_000)
			.saturating_add(RocksDbWeight::get().reads(3))
			.saturating_add(RocksDbWeight::get().writes(3))
	}
	fn try_set_deposit() -> Weight {
		Weight::from_parts(40_000_000, 6_000)
			.saturating_add(RocksDbWeight::get().reads(6))
			.saturating_add(RocksDbWeight::get().writes(6))
	}
	fn flush_outbox(n: u32) -> Weight {
		Weight::from_parts(5_000_000, 2_000)
			.saturating_add(Weight::from_parts(20_000_000, 2_000).saturating_mul(n.into()))
			.saturating_add(RocksDbWeight::get().reads(1))
			.saturating_add(RocksDbWeight::get().writes((n > 0) as u64))
			.saturating_add(RocksDbWeight::get().reads_writes(3, 2).saturating_mul(n.into()))
	}
}
