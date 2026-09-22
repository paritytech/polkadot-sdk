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

//! Tiny, dependency-free encoding for the offchain-code marker stored in `:code`.
//!
//! When a parachain moves its runtime code offchain, the `:code` storage value no longer
//! holds the full multi-MB blob. Instead it holds a 40-byte marker:
//!
//! ```text
//! [ "CODEHASH" (8 bytes) ][ blake2b-256 hash of the real code (32 bytes) ]
//! ```
//!
//! Both the runtime (PolkaVM / WASM, `no_std`) and the node (std) must agree on this
//! encoding, which is why this crate has **zero dependencies** and is `no_std`.

#![cfg_attr(not(feature = "std"), no_std)]

/// ASCII prefix that starts every code marker.
pub const CODE_MARKER_PREFIX: [u8; 8] = *b"CODEHASH";

/// Total length of an encoded marker in bytes (8-byte prefix + 32-byte hash).
pub const CODE_MARKER_LEN: usize = 40;

/// Encode a blake2b-256 `hash` into the 40-byte marker format.
pub fn encode_marker(hash: &[u8; 32]) -> [u8; CODE_MARKER_LEN] {
	let mut out = [0u8; CODE_MARKER_LEN];
	out[..8].copy_from_slice(&CODE_MARKER_PREFIX);
	out[8..].copy_from_slice(hash);
	out
}

/// Return `true` iff `code` is **exactly** a valid code marker.
///
/// Length is part of the discriminator: a WASM or PolkaVM blob that merely begins with the
/// `"CODEHASH"` bytes but is longer than 40 bytes is treated as real code, not a marker.
/// Without the length check a sufficiently crafted code blob could be misidentified.
pub fn is_marker(code: &[u8]) -> bool {
	decode_marker(code).is_some()
}

/// Extract the blake2b-256 hash from a code marker, or `None` if `code` is not a marker.
///
/// Returns `Some(hash)` only when `code` is **exactly** [`CODE_MARKER_LEN`] bytes long
/// **and** starts with [`CODE_MARKER_PREFIX`]. A slice that is longer than 40 bytes —
/// even if it begins with the prefix — returns `None`; that length is what distinguishes
/// a marker from a real code blob that coincidentally starts with `"CODEHASH"`.
pub fn decode_marker(code: &[u8]) -> Option<[u8; 32]> {
	if code.len() != CODE_MARKER_LEN {
		return None;
	}
	if code[..8] != CODE_MARKER_PREFIX {
		return None;
	}
	let mut hash = [0u8; 32];
	hash.copy_from_slice(&code[8..]);
	Some(hash)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn round_trip() {
		let hash = [0xabu8; 32];
		let marker = encode_marker(&hash);
		assert_eq!(decode_marker(&marker), Some(hash));
		assert!(is_marker(&marker));
	}

	#[test]
	fn empty_slice_is_not_a_marker() {
		assert_eq!(decode_marker(&[]), None);
		assert!(!is_marker(&[]));
	}

	#[test]
	fn one_byte_short_is_not_a_marker() {
		let hash = [0x01u8; 32];
		let marker = encode_marker(&hash);
		assert_eq!(decode_marker(&marker[..CODE_MARKER_LEN - 1]), None);
	}

	#[test]
	fn one_byte_long_is_not_a_marker() {
		let hash = [0x02u8; 32];
		let mut marker = [0u8; CODE_MARKER_LEN + 1];
		marker[..CODE_MARKER_LEN].copy_from_slice(&encode_marker(&hash));
		assert_eq!(decode_marker(&marker), None);
		assert!(!is_marker(&marker));
	}

	#[test]
	fn wrong_prefix_is_not_a_marker() {
		let mut marker = encode_marker(&[0x03u8; 32]);
		// Corrupt the prefix.
		marker[0] = b'X';
		assert_eq!(decode_marker(&marker), None);
		assert!(!is_marker(&marker));
	}

	#[test]
	fn wasm_blob_start_is_not_a_marker() {
		// A real WASM blob starts with the 4-byte magic `\0asm`.
		let mut blob = [0u8; CODE_MARKER_LEN];
		blob[..4].copy_from_slice(b"\0asm");
		assert_eq!(decode_marker(&blob), None);
	}

	#[test]
	fn polkavm_blob_start_is_not_a_marker() {
		// A real PolkaVM blob starts with `PVM\0`.
		let mut blob = [0u8; CODE_MARKER_LEN];
		blob[..4].copy_from_slice(b"PVM\0");
		assert_eq!(decode_marker(&blob), None);
	}

	#[test]
	fn is_marker_consistent_with_decode_marker() {
		// Property: is_marker(x) == decode_marker(x).is_some() for all interesting inputs.
		let cases: &[&[u8]] = &[
			&[],
			&[0u8; 39],
			&[0u8; 40],
			&[0u8; 41],
			&encode_marker(&[0xffu8; 32]),
			b"\0asm\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
		];
		for case in cases {
			assert_eq!(
				is_marker(case),
				decode_marker(case).is_some(),
				"inconsistency for {:?}",
				case
			);
		}
	}
}
