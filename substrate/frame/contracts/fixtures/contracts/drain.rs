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

#![no_std]
#![no_main]

use common::output;
use uapi::{HostFn, HostFnImpl as api};

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	output!(balance, [0u8; 8], api::balance,);
	let balance = u64::from_le_bytes(balance[..].try_into().unwrap());

	output!(minimum_balance, [0u8; 8], api::minimum_balance,);
	let minimum_balance = u64::from_le_bytes(minimum_balance[..].try_into().unwrap());

	// Make the transferred value exceed the balance by adding the minimum balance.
	let balance = balance + minimum_balance;

	// Try to self-destruct by sending more balance to the 0 address.
	// The call will fail because a contract transfer has a keep alive requirement.
	let res = api::transfer(&[0u8; 32], &balance.to_le_bytes());
	assert!(matches!(res, Err(uapi::ReturnErrorCode::TransferFailed)));
}
