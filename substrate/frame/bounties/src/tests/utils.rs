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

//! bounties pallet tests.

use crate::*;

use alloc::collections::btree_map::BTreeMap;
use core::cell::RefCell;

thread_local! {
	pub static PAID: RefCell<BTreeMap<(u128, u32), u64>> = RefCell::new(BTreeMap::new());
	pub static STATUS: RefCell<BTreeMap<u64, PaymentStatus>> = RefCell::new(BTreeMap::new());
	pub static LAST_ID: RefCell<u64> = RefCell::new(0u64);
	pub static TEST_SPEND_ORIGIN_TRY_SUCCESFUL_ORIGIN_ERR: RefCell<bool> = RefCell::new(false);
}

/// paid balance for a given account and asset ids
pub fn paid(who: u128, asset_id: u32) -> u64 {
	PAID.with(|p| p.borrow().get(&(who, asset_id)).cloned().unwrap_or(0))
}

/// reduce paid balance for a given account and asset ids
fn unpay(who: u128, asset_id: u32, amount: u64) {
	PAID.with(|p| p.borrow_mut().entry((who, asset_id)).or_default().saturating_reduce(amount))
}

/// set status for a given payment id
pub fn set_status(id: u64, s: PaymentStatus) {
	STATUS.with(|m| m.borrow_mut().insert(id, s));
}

/// sets the status of the last payment to `PaymentStatus::Success`.
pub fn approve_last_payment() {
    let last_id = LAST_ID.with(|last_id| *last_id.borrow() - 1);
    STATUS.with(|m| m.borrow_mut().insert(last_id, PaymentStatus::Success));
}

/// sets the status of the last payment to `PaymentStatus::Failure`.
pub fn reject_last_payment() {
    let last_id = LAST_ID.with(|last_id| *last_id.borrow() - 1);
    STATUS.with(|m| m.borrow_mut().insert(last_id, PaymentStatus::Failure));
}