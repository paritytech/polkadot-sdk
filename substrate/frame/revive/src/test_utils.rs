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

pub mod builder;

pub use sp_runtime::AccountId32;

use crate::{BalanceOf, Config};
use frame_support::weights::Weight;
use hex_literal::hex;
use sp_core::H160;

const fn ee_suffix(mut account: [u8; 32]) -> AccountId32 {
	let mut i = 20;
	while i < 32 {
		account[i] = 0xee;
		i += 1;
	}
	AccountId32::new(account)
}

const fn ee_extend(address: [u8; 20]) -> AccountId32 {
	let mut account = [0xEEu8; 32];
	let mut i = 0;
	while i < 20 {
		account[i] = address[i];
		i += 1;
	}
	AccountId32::new(account)
}

// All those accounts ids end in `ee` which means they don't
// need a stateful mapping and their address is derived
// by truncation without a hash applied/

pub const ALICE: AccountId32 = ee_suffix([1u8; 32]);
pub const ALICE_ADDR: H160 = H160([1u8; 20]);
pub const ALICE_FALLBACK: AccountId32 = ALICE;

pub const BOB: AccountId32 = ee_suffix([2u8; 32]);
pub const BOB_ADDR: H160 = H160([2u8; 20]);
pub const BOB_FALLBACK: AccountId32 = BOB;

pub const CHARLIE: AccountId32 = ee_suffix([3u8; 32]);
pub const CHARLIE_ADDR: H160 = H160([3u8; 20]);
pub const CHARLIE_FALLBACK: AccountId32 = CHARLIE;

pub const DJANGO: AccountId32 = ee_suffix([4u8; 32]);
pub const DJANGO_ADDR: H160 = H160([4u8; 20]);
pub const DJANGO_FALLBACK: AccountId32 = DJANGO;

/// Eve is a non ee account and hence needs a stateful mapping and its
/// address is derived by hashing to avoid truncating public keys.
pub const EVE: AccountId32 = AccountId32::new([5u8; 32]);
pub const EVE_ADDR: H160 = H160(hex!("e21eecd6e51cbcda5b0c5207ae87e605839e70ef"));
pub const EVE_FALLBACK: AccountId32 = ee_extend(EVE_ADDR.0);

pub const GAS_LIMIT: Weight = Weight::from_parts(500_000_000_000, 10 * 1024 * 1024);

pub fn deposit_limit<T: Config>() -> BalanceOf<T> {
	10_000_000u32.into()
}
