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

//! Weights to be used with `full_election_cycle_with_occasional_out_of_weight_completes` test.
//!
//! Note: we put as many things as possible as `unreachable!()` to limit the scope.

use frame_election_provider_support::Weight;
use pallet_election_provider_multi_block::weights::traits::{
	pallet_election_provider_multi_block_signed, pallet_election_provider_multi_block_unsigned,
	pallet_election_provider_multi_block_verifier,
};

pub const SMALL: Weight = Weight::from_parts(10, 0);
pub const MEDIUM: Weight = Weight::from_parts(100, 0);
pub const LARGE: Weight = Weight::from_parts(1_000, 0);

pub struct MultiBlockElectionWeightInfo;
impl pallet_election_provider_multi_block::WeightInfo for MultiBlockElectionWeightInfo {
	fn admin_set() -> Weight {
		unreachable!()
	}
	fn manage_fallback() -> Weight {
		unreachable!()
	}
	fn export_non_terminal() -> Weight {
		MEDIUM
	}
	fn export_terminal() -> Weight {
		LARGE
	}
	fn per_block_nothing() -> Weight {
		SMALL
	}
	fn per_block_snapshot_msp() -> Weight {
		LARGE
	}
	fn per_block_snapshot_rest() -> Weight {
		MEDIUM
	}
	fn per_block_start_signed_validation() -> Weight {
		SMALL
	}
}

impl pallet_election_provider_multi_block_verifier::WeightInfo for MultiBlockElectionWeightInfo {
	fn verification_invalid_non_terminal(_: u32) -> Weight {
		MEDIUM
	}
	fn verification_invalid_terminal() -> Weight {
		LARGE
	}
	fn verification_valid_non_terminal() -> Weight {
		MEDIUM
	}
	fn verification_valid_terminal() -> Weight {
		LARGE
	}
}

impl pallet_election_provider_multi_block_signed::WeightInfo for MultiBlockElectionWeightInfo {
	fn bail() -> Weight {
		unreachable!()
	}
	fn clear_old_round_data(_: u32) -> Weight {
		unreachable!()
	}
	fn register_eject() -> Weight {
		unreachable!()
	}
	fn register_not_full() -> Weight {
		// we submit pages in tests
		Default::default()
	}
	fn submit_page() -> Weight {
		unreachable!()
	}
	fn unset_page() -> Weight {
		unreachable!()
	}
}

impl pallet_election_provider_multi_block_unsigned::WeightInfo for MultiBlockElectionWeightInfo {
	fn mine_solution(_p: u32) -> Weight {
		unreachable!()
	}
	fn submit_unsigned() -> Weight {
		// NOTE: this one is checked in the integrity tests of the runtime, we don't care about it
		// here.
		Default::default()
	}
	fn validate_unsigned() -> Weight {
		unreachable!()
	}
}
