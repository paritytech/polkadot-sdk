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

//! Weights for `pallet-hrmp-para`.
//!
//! Placeholders. Regenerate with `/cmd bench` once the extrinsics have bodies.

#![allow(missing_docs)]

use core::marker::PhantomData;
use frame_support::weights::Weight;

pub trait WeightInfo {
	fn hrmp_init_open_channel() -> Weight;
	fn hrmp_accept_open_channel() -> Weight;
	fn hrmp_close_channel() -> Weight;
	fn force_clean_hrmp(i: u32, e: u32) -> Weight;
	fn force_process_hrmp_open(c: u32) -> Weight;
	fn force_process_hrmp_close(c: u32) -> Weight;
	fn hrmp_cancel_open_request(c: u32) -> Weight;
	fn force_open_hrmp_channel(c: u32) -> Weight;
	fn establish_system_channel() -> Weight;
	fn poke_channel_deposits() -> Weight;
	fn establish_channel_with_system() -> Weight;
	fn receive() -> Weight;
}

pub struct SubstrateWeight<T>(PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
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
	fn receive() -> Weight {
		Weight::zero()
	}
}

// For backwards compatibility and tests.
impl WeightInfo for () {
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
	fn receive() -> Weight {
		Weight::zero()
	}
}
