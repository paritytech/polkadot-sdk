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

//! Default weights for `pallet-vesting-precompiles`.
//!
//! THIS FILE SHOULD BE AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI.
//! The values below are conservative placeholders until benchmarks are run.
//!
//! Note: `vest` and `vest_other` are not included here because those operations
//! delegate to `pallet-vesting` dispatchables and charge the pallet's own
//! benchmarked dispatch weight via `get_dispatch_info().call_weight`.

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]
#![allow(dead_code)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use core::marker::PhantomData;

/// Weight functions for the vesting precompile's view operations.
pub trait WeightInfo {
	fn vesting_balance() -> Weight;
	fn vesting_balance_of() -> Weight;
}

/// Default weights using `RocksDbWeight` as a conservative placeholder.
///
/// These should be replaced by benchmark-derived values via `frame-omni-bencher`.
impl WeightInfo for () {
	fn vesting_balance() -> Weight {
		// Placeholder: reads Vesting map + free_balance
		RocksDbWeight::get().reads(2)
	}
	fn vesting_balance_of() -> Weight {
		// Placeholder: same as vesting_balance
		RocksDbWeight::get().reads(2)
	}
}

/// Weights for `pallet-vesting-precompiles` using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn vesting_balance() -> Weight {
		T::DbWeight::get().reads(2)
	}
	fn vesting_balance_of() -> Weight {
		T::DbWeight::get().reads(2)
	}
}
