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
};

/// When encoding a Rust `[u8]` to Solidity `bytes`, a small amount
/// of overhead space is required (for padding and the length word).
const SOLIDITY_BYTES_ENCODING_OVERHEAD: usize = 64;

/// Encodes a `u32` to big-endian `[u8; 32]` with padded zeros.
fn encode_u32(value: u32) -> [u8; 32] {
	let mut buf = [0u8; 32];
	buf[28..].copy_from_slice(&value.to_be_bytes()); // last 4 bytes
	buf
}

/// Encodes a `bool` to big-endian `[u8; 32]` with padded zeros.
fn encode_bool(value: bool, out: &mut [u8]) {
	let mut buf = [0u8; 32];
	if value {
		buf[31] = 1;
	}
	out[..32].copy_from_slice(&buf[..32]);
}

/// Encodes the `bytes` argument for the Solidity ABI.
/// The result is written to `out`.
///
/// Returns the number of bytes written.
///
/// # Important
///
/// This function assumes that the encoded bytes argument follows
/// two previous other argument that takes up 32 bytes.
///
/// So e.g. `function(uint32, bool, bytes)` (with `uint32` and `bool`
/// being of word size 32 bytes). This assumption is made to calculate
/// the `offset` word.
///
/// # Developer Note
///
/// The returned layout will be
///
///     `[offset (32 bytes)] [len (32 bytes)] [data (padded to 32)]`
///
/// The `out` byte array needs to be able to hold (in the worst case)
/// 95 bytes more than `input.len()`. This is because we write the
/// following to `out`:
///
///   * The offset word → always 32 bytes.
///   * The length word → always 32 bytes.
///   * The input itself → exactly `input.len()` bytes.
///   * We pad the input to a multiple of 32 → between 0 and 31 extra bytes.
fn encode_bytes(input: &[u8], out: &mut [u8]) -> usize {
	let len = input.len();
	let padded_len = ((len + 31) / 32) * 32;

	// out_len = 32 + padded_len
	//         = 32 + ceil(input_len / 32) * 32
	assert!(out.len() >= padded_len + SOLIDITY_BYTES_ENCODING_OVERHEAD);

	// Encode offset as a 32-byte big-endian word.
	// The offset points to the start of the bytes payload in the ABI.
	//
	// Important:
	// This function assumes that the `bytes` argument to the Solidity function follows
	// two prior argument of word size 32 bytes (e.g. `function(uint32, bool, bytes)`!
	//
	// Then the offset will be
	//   * 32 bytes for `uint32`
	//   * 32 bytes for `bool`
	//   * Another 32 bytes for this offset word
	// The 96 then points to the start of the `bytes` data segment (specifically
	// its `len` field (`bytes = offset (32 bytes) | len (32 bytes) | data (variable)`).
	let assumed_offset: u32 = 96;
	out[28..32].copy_from_slice(&assumed_offset.to_be_bytes()[..4]);
	out[..28].copy_from_slice(&[0u8; 28]); // make sure the first bytes are zeroed

	// Encode length as a 32-byte big-endian word
	let mut len_word = [0u8; 32];
	let len_bytes = (len as u128).to_be_bytes(); // 16 bytes
	len_word[32 - len_bytes.len()..].copy_from_slice(&len_bytes);
	out[32..64].copy_from_slice(&len_word);

	// Write data
	out[64..64 + len].copy_from_slice(input);

	// Zero padding
	assert!(padded_len >= len);
	for i in 64 + len..64 + padded_len - len {
		out[i] = 0;
	}

	64 + padded_len
}

/// Simple decoder for a Solidity `bytes` type.
///
/// Returns the number of bytes written to `out`.
fn decode_bytes(input: &[u8], out: &mut [u8]) -> usize {
	let mut buf = [0u8; 4];
	buf[..].copy_from_slice(&input[28..32]);
	let offset = u32::from_be_bytes(buf) as usize;

	let mut buf = [0u8; 4];
	buf[..].copy_from_slice(&input[60..64]);
	let bytes_len = u32::from_be_bytes(buf) as usize;

	// we start decoding at the start of the payload.
	// the payload starts at the `len` word here:
	// `bytes = offset (32 bytes) | len (32 bytes) | data`
	out[..bytes_len].copy_from_slice(&input[32 + offset..32 + offset + bytes_len]);
	bytes_len
}

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
