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

//! Helper utilities around pre-compiles.
//!
//! This file contains a number of functions for Solidity type encoding
//! and decoding. Notably these implementations don't require an allocator,
//! which is why they are here in the first place (e.g. `alloy-core` requires
//! an allocator for the Solidity `bytes` type).

/// Returns the Solidity selector for `fn_sig`.
///
/// Note that this is a const function, it is evaluated at compile time.
///
/// # Usage
///
/// ```
/// # use pallet_revive_uapi::solidity_selector;
/// let sel = solidity_selector("ownCodeHash()");
/// assert_eq!(sel, [219, 107, 220, 138]);
/// ```
pub const fn solidity_selector(fn_sig: &str) -> [u8; 4] {
	let output: [u8; 32] =
		const_crypto::sha3::Keccak256::new().update(fn_sig.as_bytes()).finalize();
	[output[0], output[1], output[2], output[3]]
}

/// When encoding a Rust `[u8]` to Solidity `bytes`, a small amount
/// of overhead space is required (for padding and the length word).
const SOLIDITY_BYTES_ENCODING_OVERHEAD: usize = 64;

/// Encodes a `u32` to big-endian `[u8; 32]` with padded zeros.
pub fn encode_u32(value: u32) -> [u8; 32] {
	let mut buf = [0u8; 32];
	buf[28..].copy_from_slice(&value.to_be_bytes()); // last 4 bytes
	buf
}

/// Encodes a `bool` to big-endian `[u8; 32]` with padded zeros.
pub fn encode_bool(value: bool, out: &mut [u8]) {
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
/// ```no_compile
/// [offset (32 bytes)] [len (32 bytes)] [data (padded to 32)]
/// ```
///
/// The `out` byte array needs to be able to hold (in the worst case)
/// 95 bytes more than `input.len()`. This is because we write the
/// following to `out`:
///
///   * The offset word → always 32 bytes.
///   * The length word → always 32 bytes.
///   * The input itself → exactly `input.len()` bytes.
///   * We pad the input to a multiple of 32 → between 0 and 31 extra bytes.
pub fn encode_bytes(input: &[u8], out: &mut [u8]) -> usize {
	let len = input.len();
	let padded_len = ((len + 31).div_ceil(32)) * 32;

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
pub fn decode_bytes(input: &[u8], out: &mut [u8]) -> usize {
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
