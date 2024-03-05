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

use common::input;
use uapi::{HostFn, HostFnImpl as api};

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	input!(code_hash: [u8; 32],);

	let mut key = [0u8; 32];
	key[0] = 1u8;

	let mut value = [0u8; 32];
	let value = &mut &mut value[..];
	value[0] = 2u8;

	api::set_storage(&key, value);
	api::get_storage(&key, value).unwrap();
	assert!(value[0] == 2u8);

	let input = [0u8; 0];
	api::delegate_call(uapi::CallFlags::empty(), code_hash, &input, None).unwrap();

	api::get_storage(&[1u8], value).unwrap();
	assert!(value[0] == 1u8);
}
