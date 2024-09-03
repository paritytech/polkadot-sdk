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

//! Shared utilities for testing contracts.
//! This is not part of the tests module because it is made public for other crates to use.

#![cfg(feature = "std")]

pub mod builder;

use crate::{BalanceOf, Config};
use frame_support::weights::Weight;
use sp_core::H160;
pub use sp_runtime::AccountId32;

const fn ee_suffix(addr: H160) -> AccountId32 {
	let mut id = [0u8; 32];
	let mut i = 0;
	while i < 20 {
		id[i] = addr.0[i];
		i += 1;
	}

	let mut j = 20;
	while j < 32 {
		id[j] = 0xee;
		j += 1;
	}

	AccountId32::new(id)
}

pub const ALICE: AccountId32 = AccountId32::new([1u8; 32]);
pub const ALICE_ADDR: H160 = H160([1u8; 20]);
pub const ETH_ALICE: AccountId32 = ee_suffix(ALICE_ADDR);

pub const BOB: AccountId32 = AccountId32::new([2u8; 32]);
pub const BOB_ADDR: H160 = H160([2u8; 20]);
pub const BOB_CONTRACT_ID: AccountId32 = ee_suffix(BOB_ADDR);

pub const CHARLIE: AccountId32 = AccountId32::new([3u8; 32]);
pub const CHARLIE_ADDR: H160 = H160([3u8; 20]);

pub const DJANGO: AccountId32 = AccountId32::new([4u8; 32]);
pub const DJANGO_ADDR: H160 = H160([4u8; 20]);
pub const ETH_DJANGO: AccountId32 = ee_suffix(DJANGO_ADDR);

pub const GAS_LIMIT: Weight = Weight::from_parts(100_000_000_000, 3 * 1024 * 1024);

pub fn deposit_limit<T: Config>() -> BalanceOf<T> {
	10_000_000u32.into()
}
