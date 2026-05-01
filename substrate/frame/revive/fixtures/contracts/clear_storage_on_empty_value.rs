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

//! Asserts that the variable-key `api::set_storage(key, &[])` syscall deletes the trie
//! row, mirroring the fixed-key `api::set_storage_or_clear(key, ZEROS)` behavior
//! exercised by `clear_storage_on_zero_value.rs`.

#![no_std]
#![no_main]

include!("../panic_handler.rs");
include!("../sol_utils.rs");

use uapi::{HostFn, HostFnImpl as api, StorageFlags};

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

fn test_storage_operations(flags: StorageFlags) {
	const KEY: [u8; 32] = [0xABu8; 32];
	const VALUE: [u8; 32] = [0xAAu8; 32];
	const SHORT_VALUE: [u8; 3] = [5, 6, 7];

	// Start from a known-clean state.
	clear_storage::<api>(flags, &KEY);
	assert_eq!(contains_storage::<api>(flags, &KEY), None);

	// Write a value through the variable-key path.
	let existing = api::set_storage(flags, &KEY, &VALUE);
	assert_eq!(existing, None);
	assert_eq!(contains_storage::<api>(flags, &KEY), Some(VALUE.len() as u32));

	// First clear-by-empty must report the prior size and remove the row.
	let existing = api::set_storage(flags, &KEY, &[]);
	assert_eq!(existing, Some(VALUE.len() as u32));
	assert_eq!(contains_storage::<api>(flags, &KEY), None);

	// A second clear-by-empty on an absent row must return `None`, not `Some(0)`.
	let existing = api::set_storage(flags, &KEY, &[]);
	assert_eq!(existing, None);
	assert_eq!(contains_storage::<api>(flags, &KEY), None);

	// Clearing also works for a value shorter than 32 bytes.
	let existing = api::set_storage(flags, &KEY, &SHORT_VALUE);
	assert_eq!(existing, None);
	assert_eq!(contains_storage::<api>(flags, &KEY), Some(SHORT_VALUE.len() as u32));

	let existing = api::set_storage(flags, &KEY, &[]);
	assert_eq!(existing, Some(SHORT_VALUE.len() as u32));
	assert_eq!(contains_storage::<api>(flags, &KEY), None);
}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	// Regular storage.
	test_storage_operations(StorageFlags::empty());

	// Transient storage uses the same syscall handler.
	test_storage_operations(StorageFlags::TRANSIENT);
}
