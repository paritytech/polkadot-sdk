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

//! Weights for `pallet-jamkb`.
//!
//! The pallet is a prototype and has no benchmarks yet. The unit implementation charges a flat
//! placeholder weight per call so that the pallet can be wired into a test runtime.

use frame_support::weights::Weight;

/// Weight functions needed for `pallet_jamkb`.
pub trait WeightInfo {
	/// Weight of `initialize`.
	fn initialize() -> Weight;
	/// Weight of `attest`.
	fn attest() -> Weight;
	/// Weight of `pause`.
	fn pause() -> Weight;
	/// Weight of `resume`.
	fn resume() -> Weight;
	/// Weight of `grant`.
	fn grant() -> Weight;
	/// Weight of `approve_release`.
	fn approve_release() -> Weight;
	/// Weight of `execute_release`.
	fn execute_release() -> Weight;
	/// Weight of `redeem`.
	fn redeem() -> Weight;
	/// Weight of `cancel_allocation`.
	fn cancel_allocation() -> Weight;
	/// Weight of settling one operation, by `settle` or by `on_poll`.
	fn settle() -> Weight;
	/// Weight of recording one operation's outcome.
	fn on_outcome() -> Weight;
}

const PLACEHOLDER: Weight = Weight::from_parts(10_000_000, 10_000);

impl WeightInfo for () {
	fn initialize() -> Weight {
		PLACEHOLDER
	}
	fn attest() -> Weight {
		PLACEHOLDER
	}
	fn pause() -> Weight {
		PLACEHOLDER
	}
	fn resume() -> Weight {
		PLACEHOLDER
	}
	fn grant() -> Weight {
		PLACEHOLDER
	}
	fn approve_release() -> Weight {
		PLACEHOLDER
	}
	fn execute_release() -> Weight {
		PLACEHOLDER
	}
	fn redeem() -> Weight {
		PLACEHOLDER
	}
	fn cancel_allocation() -> Weight {
		PLACEHOLDER
	}
	fn settle() -> Weight {
		PLACEHOLDER
	}
	fn on_outcome() -> Weight {
		PLACEHOLDER
	}
}
