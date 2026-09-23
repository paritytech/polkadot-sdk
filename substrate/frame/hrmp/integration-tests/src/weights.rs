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

//! Free weights for both pallets, since their `()` placeholders are `Weight::MAX` and no XCM
//! `Transact` could pay for that.

use frame_support::weights::Weight;

pub struct ZeroWeights;

impl pallet_hrmp_relay::WeightInfo for ZeroWeights {
	fn receive_open_channel() -> Weight {
		Weight::zero()
	}
	fn receive_force_open_channel() -> Weight {
		Weight::zero()
	}
	fn receive_open_system_channel() -> Weight {
		Weight::zero()
	}
	fn receive_open_system_pair() -> Weight {
		Weight::zero()
	}
	fn receive_close_channel() -> Weight {
		Weight::zero()
	}
	fn receive_force_clean(_i: u32, _e: u32) -> Weight {
		Weight::zero()
	}
	fn receive_notify_para() -> Weight {
		Weight::zero()
	}
	fn relay_request() -> Weight {
		Weight::zero()
	}
}

impl pallet_hrmp_para::WeightInfo for ZeroWeights {
	fn hrmp_init_open_channel() -> Weight {
		Weight::zero()
	}
	fn hrmp_accept_open_channel() -> Weight {
		Weight::zero()
	}
	fn hrmp_close_channel() -> Weight {
		Weight::zero()
	}
	fn force_clean_hrmp(_i: u32, _e: u32) -> Weight {
		Weight::zero()
	}
	fn force_process_hrmp_open(_c: u32) -> Weight {
		Weight::zero()
	}
	fn force_process_hrmp_close(_c: u32) -> Weight {
		Weight::zero()
	}
	fn hrmp_cancel_open_request(_c: u32) -> Weight {
		Weight::zero()
	}
	fn force_open_hrmp_channel(_c: u32) -> Weight {
		Weight::zero()
	}
	fn establish_system_channel() -> Weight {
		Weight::zero()
	}
	fn poke_channel_deposits() -> Weight {
		Weight::zero()
	}
	fn establish_channel_with_system() -> Weight {
		Weight::zero()
	}
	fn receive_open_channel_response() -> Weight {
		Weight::zero()
	}
	fn receive_close_response() -> Weight {
		Weight::zero()
	}
}
