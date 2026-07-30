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

//! Preliminary weights for `pallet_scarcity_claims`.
//!
//! The PR weight-generation job replaces these values from the pallet benchmarks. Contract
//! execution is not included here: the dispatchable adds `CollectionSelector::max_weight`.

use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};

/// Weight functions needed by `pallet_scarcity_claims`.
pub trait WeightInfo {
	fn ingest_root() -> Weight;
	fn claim(proof_depth: u32) -> Weight;
}

pub struct SubstrateWeight<T>(core::marker::PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn ingest_root() -> Weight {
		Weight::from_parts(15_000_000, 3_600)
			.saturating_add(T::DbWeight::get().reads(2))
			.saturating_add(T::DbWeight::get().writes(2))
	}

	fn claim(proof_depth: u32) -> Weight {
		Weight::from_parts(120_000_000, 18_000)
			.saturating_add(Weight::from_parts(8_000_000, 32).saturating_mul(proof_depth.into()))
			.saturating_add(T::DbWeight::get().reads(10))
			.saturating_add(T::DbWeight::get().writes(8))
	}
}

impl WeightInfo for () {
	fn ingest_root() -> Weight {
		Weight::from_parts(15_000_000, 3_600)
			.saturating_add(RocksDbWeight::get().reads(2))
			.saturating_add(RocksDbWeight::get().writes(2))
	}

	fn claim(proof_depth: u32) -> Weight {
		Weight::from_parts(120_000_000, 18_000)
			.saturating_add(Weight::from_parts(8_000_000, 32).saturating_mul(proof_depth.into()))
			.saturating_add(RocksDbWeight::get().reads(10))
			.saturating_add(RocksDbWeight::get().writes(8))
	}
}
