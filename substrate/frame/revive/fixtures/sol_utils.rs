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

// This file contains some handy functions to deal with Solidity ABI-encoded
// data. This is useful for fixtures that deal with builtin pre-compiles.
//
// The functions in this file don't require an allocator and use `const`
// functions where possible, to offload computation to compile time.

use uapi::{
	CallFlags,
	ReturnFlags,
	solidity_selector,
	STORAGE_PRECOMPILE_ADDR,
	precompiles::utils::*,
};

/// Executes a delegate-call to the `containsStorage` function of the `Storage`
/// pre-compile.
#[allow(dead_code)]
fn contains_storage<A: HostFn>(flags: StorageFlags, key: &[u8]) -> Option<u32> {
	let mut buffer = [0u8; 512];

	let sel = solidity_selector("containsStorage(uint32,bool,bytes)");
	buffer[..4].copy_from_slice(&sel[..4]);

	let flags = encode_u32(flags.bits());
	buffer[4..36].copy_from_slice(&flags[..32]);

	encode_bool(false, &mut buffer[36..68]); // `is_fixed_key`
	let n = encode_bytes(key, &mut buffer[68..]);

	let mut output = [0u8; 64]; /* function returns (bool, uint) */
	let _ = A::delegate_call(
		CallFlags::empty(),
		&STORAGE_PRECOMPILE_ADDR,
		u64::MAX,       // How much ref_time to devote for the execution. u64::MAX = use all.
		u64::MAX,       // How much proof_size to devote for the execution. u64::MAX = use all.
		&[u8::MAX; 32], // No deposit limit.
		&buffer[..36 /* selector + `uint32` */ + 32 /* `bool` */ + n /* `bytes` */],
		Some(&mut &mut output[..]),
	).expect("delegate call to `Storage::contains_storage` failed");

	if output[31] == 0 {
		return None;
	}

	let mut value_len_buf = [0u8; 4];
	value_len_buf[..4].copy_from_slice(&output[60..]);
	Some(u32::from_be_bytes(value_len_buf))
}

/// Executes a delegate-call to the `clearStorage` function of the `Storage`
/// pre-compile.
pub fn clear_storage<A: HostFn>(flags: StorageFlags, key: &[u8]) -> Option<u32> {
	let mut buffer = [0u8; 512];

	let sel = solidity_selector("clearStorage(uint32,bool,bytes)");
	buffer[..4].copy_from_slice(&sel[..4]);

	let flags = encode_u32(flags.bits());
	buffer[4..36].copy_from_slice(&flags[..32]);

	encode_bool(false, &mut buffer[36..68]); // `is_fixed_key`
	let n = encode_bytes(key, &mut buffer[68..]);

	let mut output = [0u8; 64]; /* function returns (bool, uint) */
	let ret = A::delegate_call(
		CallFlags::empty(),
		&STORAGE_PRECOMPILE_ADDR,
		u64::MAX,       // How much ref_time to devote for the execution. u64::MAX = use all.
		u64::MAX,       // How much proof_size to devote for the execution. u64::MAX = use all.
		&[u8::MAX; 32], // No deposit limit.
		&buffer[..36 /* selector + `uint32` */ + 32 /* `bool` */ + n /* `bytes` */],
		Some(&mut &mut output[..]),
	);
	if let Err(code) = ret {
		// We encode the error code into the revert buffer, as some fixtures rely
		// on detecting `OutOfResources`.
		A::return_value(ReturnFlags::REVERT, &(code as u32).to_le_bytes());
	};

	// Check the returned `containedKey` boolean
	if output[31] == 0 {
		return None;
	}

	let mut value_len_buf = [0u8; 4];
	value_len_buf[..4].copy_from_slice(&output[60..]);
	Some(u32::from_be_bytes(value_len_buf))
}

/// Executes a delegate-call to the `takeStorage` function of the `Storage`
/// pre-compile.
pub fn take_storage<A: HostFn>(flags: StorageFlags, key: &[u8], decode_output: &mut [u8]) -> Option<usize> {
	let mut buffer = [0u8; 512];

	let sel = solidity_selector("takeStorage(uint32,bool,bytes)");
	buffer[..4].copy_from_slice(&sel[..4]);

	let flags = encode_u32(flags.bits());
	buffer[4..36].copy_from_slice(&flags[..32]);

	encode_bool(false, &mut buffer[36..68]); // `is_fixed_key`
	let n = encode_bytes(key, &mut buffer[68..]);

	let mut output = [0u8; 512];
	let _ = A::delegate_call(
		CallFlags::empty(),
		&STORAGE_PRECOMPILE_ADDR,
		u64::MAX,       // How much ref_time to devote for the execution. u64::MAX = use all.
		u64::MAX,       // How much proof_size to devote for the execution. u64::MAX = use all.
		&[u8::MAX; 32], // No deposit limit.
		&buffer[..36 /* selector + `uint32` */ + 32 /* `bool` */ + n /* `bytes` */],
		Some(&mut &mut output[..]),
	).expect("delegate call to `Storage::take_storage` failed");

	let decoded = decode_bytes(&output[..], decode_output);
	if decoded == 0 {
		return None;
	} else {
		return Some(decoded);
	}
}
