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

//! This contract tests the storage APIs. It sets and clears storage values using the different
//! versions of the storage APIs.
#![no_std]
#![no_main]

#[allow(unused_imports)]
use common::unwrap_output;
use uapi::{HostFn, HostFnImpl as api, StorageFlags};

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	const KEY: [u8; 32] = [1u8; 32];
	const VALUE_A: [u8; 32] = [4u8; 32];
	const ZERO: [u8; 32] = [0u8; 32];

	api::clear_storage(StorageFlags::empty(), &KEY);

	assert_eq!(api::contains_storage(StorageFlags::empty(), &KEY), None);

	let existing = api::set_storage_or_clear(StorageFlags::empty(), &KEY, &VALUE_A);
	assert_eq!(existing, None);

	let mut stored: [u8; 32] = [0u8; 32];
	api::get_storage_or_zero(StorageFlags::empty(), &KEY, &mut stored);
	assert_eq!(stored, VALUE_A);

	let existing = api::set_storage_or_clear(StorageFlags::empty(), &KEY, &ZERO);
	assert_eq!(existing, Some(32));

	let mut cleared: [u8; 32] = [1u8; 32];
	api::get_storage_or_zero(StorageFlags::empty(), &KEY, &mut cleared);
	assert_eq!(cleared, ZERO);
}
