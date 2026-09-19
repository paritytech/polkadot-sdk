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

//! Weights of the price oracle pallet.

use frame_support::weights::Weight;

/// Weight functions needed for the pallet.
pub trait WeightInfo {
	/// Processing an inherent with `r` reports.
	fn process_reports(r: u32) -> Weight;
	/// Setting the parameters.
	fn set_parameters() -> Weight;
	/// Setting the pause flags.
	fn set_pause() -> Weight;
	/// Adding or updating a venue.
	fn set_venue() -> Weight;
	/// Removing a venue.
	fn remove_venue() -> Weight;
	/// Adding or updating a market.
	fn set_market() -> Weight;
	/// Removing a market.
	fn remove_market() -> Weight;
	/// Setting the health limits of a pair.
	fn set_pair_settings() -> Weight;
}

impl WeightInfo for () {
	fn process_reports(_r: u32) -> Weight {
		Weight::zero()
	}
	fn set_parameters() -> Weight {
		Weight::zero()
	}
	fn set_pause() -> Weight {
		Weight::zero()
	}
	fn set_venue() -> Weight {
		Weight::zero()
	}
	fn remove_venue() -> Weight {
		Weight::zero()
	}
	fn set_market() -> Weight {
		Weight::zero()
	}
	fn remove_market() -> Weight {
		Weight::zero()
	}
	fn set_pair_settings() -> Weight {
		Weight::zero()
	}
}
