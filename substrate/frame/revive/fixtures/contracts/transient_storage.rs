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

//! This contract tests the transient storage APIs.

#![no_std]
#![no_main]

include!("../panic_handler.rs");
include!("../sol_utils.rs");

use uapi::{unwrap_output, HostFn, HostFnImpl as api, StorageFlags};

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	const KEY: [u8; 32] = [1u8; 32];
	const VALUE_1: [u8; 4] = [1u8; 4];
	const VALUE_2: [u8; 5] = [2u8; 5];
	const VALUE_3: [u8; 6] = [3u8; 6];

	let existing = api::set_storage(StorageFlags::TRANSIENT, &KEY, &VALUE_1);
	assert_eq!(existing, None);
	assert_eq!(contains_storage::<api>(StorageFlags::TRANSIENT, &KEY), Some(VALUE_1.len() as _));
	unwrap_output!(val, [0u8; 32], api::get_storage, StorageFlags::TRANSIENT, &KEY);
	assert_eq!(**val, VALUE_1);

	let existing = api::set_storage(StorageFlags::TRANSIENT, &KEY, &VALUE_2);
	assert_eq!(existing, Some(VALUE_1.len() as _));
	unwrap_output!(val, [0u8; 32], api::get_storage, StorageFlags::TRANSIENT, &KEY);
	assert_eq!(**val, VALUE_2);

	assert_eq!(clear_storage::<api>(StorageFlags::TRANSIENT, &KEY), Some(VALUE_2.len() as _));
	assert_eq!(contains_storage::<api>(StorageFlags::TRANSIENT, &KEY), None);

	let existing = api::set_storage(StorageFlags::TRANSIENT, &KEY, &VALUE_3);
	assert_eq!(existing, None);
	let mut output = [0u8; 6];
	let _ = take_storage::<api>(StorageFlags::TRANSIENT, &KEY, &mut output).expect("value must exist in storage");
	assert_eq!(output, VALUE_3);
}
