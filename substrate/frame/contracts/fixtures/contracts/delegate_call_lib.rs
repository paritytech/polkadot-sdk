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
	let mut key = [0u8; 32];
	key[0] = 1u8;

	// Place a value in storage.
	let mut value = [0u8; 32];
	let value = &mut &mut value[..];
	value[0] = 1u8;
	api::set_storage(&key, value);

	// Assert that `value_transferred` is equal to the value
	// passed to the `caller` contract: 1337.
	output!(value_transferred, [0u8; 8], api::value_transferred,);
	let value_transferred = u64::from_le_bytes(value_transferred[..].try_into().unwrap());
	assert_eq!(value_transferred, 1337);

	// Assert that ALICE is the caller of the contract.
	output!(caller, [0u8; 32], api::caller,);
	assert_eq!(&caller[..], &[1u8; 32]);
}
