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

//! A set of zero weights for all benchmarks of this pallet to be temporarily used in testing
//! runtimes while benchmarking is being finalized.

/// A `WeightInfo` impl with all zero weights
pub struct AllZeroWeights;
use frame_support::weights::Weight;

impl crate::WeightInfo for AllZeroWeights {
	fn export_non_terminal() -> Weight {
		Default::default()
	}
	fn export_terminal() -> Weight {
		Default::default()
	}
	fn manage() -> Weight {
		Default::default()
	}
	fn on_initialize_into_signed() -> Weight {
		Default::default()
	}
	fn on_initialize_into_signed_validation() -> Weight {
		Default::default()
	}
	fn on_initialize_into_snapshot_msp() -> Weight {
		Default::default()
	}
	fn on_initialize_into_snapshot_rest() -> Weight {
		Default::default()
	}
	fn on_initialize_into_unsigned() -> Weight {
		Default::default()
	}
	fn on_initialize_nothing() -> Weight {
		Default::default()
	}
}

impl crate::signed::WeightInfo for AllZeroWeights {
	fn bail() -> Weight {
		Default::default()
	}
	fn register_eject() -> Weight {
		Default::default()
	}
	fn register_not_full() -> Weight {
		Default::default()
	}
	fn submit_page() -> Weight {
		Default::default()
	}
	fn unset_page() -> Weight {
		Default::default()
	}
	fn clear_old_round_data(p: u32) -> Weight {
		Default::default()
	}
}

impl crate::unsigned::WeightInfo for AllZeroWeights {
	fn submit_unsigned() -> Weight {
		Default::default()
	}
	fn validate_unsigned() -> Weight {
		Default::default()
	}
}

impl crate::verifier::WeightInfo for AllZeroWeights {
	fn on_initialize_invalid_non_terminal(_: u32) -> Weight {
		Default::default()
	}
	fn on_initialize_invalid_terminal() -> Weight {
		Default::default()
	}
	fn on_initialize_valid_non_terminal() -> Weight {
		Default::default()
	}
	fn on_initialize_valid_terminal() -> Weight {
		Default::default()
	}
}
