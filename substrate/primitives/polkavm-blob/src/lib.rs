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

//! Read and modify custom sections of PolkaVM program blobs.
//!
//! A PolkaVM program blob starts with the `PVM\0` magic bytes, a version byte and a little
//! endian `u64` holding the length of the whole blob. It is followed by a sequence of sections
//! encoded as `[id][varint payload length][payload]` and terminated by a `0` byte. Sections with
//! the high bit set in their id are optional and are skipped by every parser.

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::vec::Vec;

/// The id of an optional custom section that can be used for padding. Mostly useful in tests.
pub const PADDING_SECTION: u8 = 0xFA;

/// The magic bytes every program blob starts with.
const BLOB_MAGIC: &[u8; 4] = b"PVM\0";
/// The offset of the little endian `u64` holding the length of the whole blob.
const BLOB_LEN_OFFSET: usize = BLOB_MAGIC.len() + 1;
/// The size of the blob length field.
const BLOB_LEN_SIZE: usize = core::mem::size_of::<u64>();
/// The offset at which the sections start.
const SECTIONS_OFFSET: usize = BLOB_LEN_OFFSET + BLOB_LEN_SIZE;
/// The section id that marks the end of the sections.
const SECTION_END_OF_FILE: u8 = 0;
/// The maximum number of bytes a varint can occupy, including its first byte.
const MAX_VARINT_LEN: usize = 5;

/// Returns the payload of the custom section with the given `id`.
///
/// Returns `None` if `blob` is not a valid program blob or if it does not contain a section with
/// the given `id`.
pub fn custom_section(blob: &[u8], id: u8) -> Option<&[u8]> {
	sections_offset(blob)?;

	let mut position = SECTIONS_OFFSET;
	loop {
		let section_id = *blob.get(position)?;
		if section_id == SECTION_END_OF_FILE {
			return None;
		}

		let (length, encoded_len) = read_varint_at(blob, position + 1)?;
		let payload_start = position + 1 + encoded_len;
		let payload = blob.get(payload_start..payload_start.checked_add(length as usize)?)?;
		if section_id == id {
			return Some(payload);
		}

		position = payload_start + length as usize;
	}
}

/// Returns `true` if `blob` is a structurally valid program blob.
///
/// The modification functions panic on invalid blobs; use this function to check beforehand.
pub fn is_valid(blob: &[u8]) -> bool {
	section_end_offset(blob).is_some()
}

/// Returns a copy of `blob` with `payload` appended as a custom section with the given `id`.
///
/// The section is inserted right before the end-of-file marker and the length metadata of the
/// blob is updated accordingly.
///
/// # Panics
///
/// Panics if `blob` is not a valid program blob, if `id` is the end-of-file marker or if `payload`
/// is longer than `u32::MAX` bytes.
pub fn append_custom_section(blob: &[u8], id: u8, payload: &[u8]) -> Vec<u8> {
	let Some(end_of_file) = section_end_offset(blob) else {
		panic!("`append_custom_section` expects a valid PolkaVM program blob");
	};

	assert!(id != SECTION_END_OF_FILE, "custom section id `0` is the end-of-file marker");

	let Ok(payload_len) = u32::try_from(payload.len()) else {
		panic!("custom section payloads are limited to `u32::MAX` bytes");
	};

	let mut encoded_len_buf = [0u8; MAX_VARINT_LEN];
	let encoded_len = polkavm_common::varint::write_varint(payload_len, &mut encoded_len_buf);

	let mut out = Vec::with_capacity(blob.len() + 1 + encoded_len + payload.len());
	out.extend_from_slice(&blob[..end_of_file]);
	out.push(id);
	out.extend_from_slice(&encoded_len_buf[..encoded_len]);
	out.extend_from_slice(payload);
	out.extend_from_slice(&blob[end_of_file..]);
	write_blob_len(&mut out);
	out
}

/// Returns a copy of `blob` in which every custom section with the given `id` is replaced by a
/// single section holding `payload`.
///
/// # Panics
///
/// Panics if `blob` is not a valid program blob, if `id` is the end-of-file marker or if `payload`
/// is longer than `u32::MAX` bytes.
pub fn set_custom_section(blob: &[u8], id: u8, payload: &[u8]) -> Vec<u8> {
	let Some(stripped) = remove_custom_sections(blob, id) else {
		panic!("`set_custom_section` expects a valid PolkaVM program blob");
	};

	append_custom_section(&stripped, id, payload)
}

/// Validates the header of `blob` and returns the offset at which the sections start.
fn sections_offset(blob: &[u8]) -> Option<usize> {
	if blob.get(..BLOB_MAGIC.len()) != Some(BLOB_MAGIC.as_slice()) {
		return None;
	}

	let blob_len = u64::from_le_bytes(blob.get(BLOB_LEN_OFFSET..SECTIONS_OFFSET)?.try_into().ok()?);
	if blob_len != blob.len() as u64 {
		return None;
	}

	Some(SECTIONS_OFFSET)
}

/// Returns the offset of the end-of-file marker of a valid program blob.
fn section_end_offset(blob: &[u8]) -> Option<usize> {
	sections_offset(blob)?;

	let mut position = SECTIONS_OFFSET;
	loop {
		let section_id = *blob.get(position)?;
		if section_id == SECTION_END_OF_FILE {
			return Some(position);
		}

		let (length, encoded_len) = read_varint_at(blob, position + 1)?;
		position = position.checked_add(1 + encoded_len + length as usize)?;
		if position > blob.len() {
			return None;
		}
	}
}

/// Returns a copy of `blob` without any custom section with the given `id`.
fn remove_custom_sections(blob: &[u8], id: u8) -> Option<Vec<u8>> {
	sections_offset(blob)?;

	let mut out = Vec::with_capacity(blob.len());
	out.extend_from_slice(&blob[..SECTIONS_OFFSET]);

	let mut position = SECTIONS_OFFSET;
	loop {
		let section_id = *blob.get(position)?;
		if section_id == SECTION_END_OF_FILE {
			out.extend_from_slice(&blob[position..]);
			write_blob_len(&mut out);
			return Some(out);
		}

		let (length, encoded_len) = read_varint_at(blob, position + 1)?;
		let section_end = position.checked_add(1 + encoded_len + length as usize)?;
		if section_end > blob.len() {
			return None;
		}

		if section_id != id {
			out.extend_from_slice(&blob[position..section_end]);
		}
		position = section_end;
	}
}

/// Reads the varint starting at `position`.
///
/// Returns the decoded value together with the total number of bytes the varint occupies.
fn read_varint_at(blob: &[u8], position: usize) -> Option<(u32, usize)> {
	let first_byte = *blob.get(position)?;
	let (value, tail_len) = read_varint(blob.get(position + 1..)?, first_byte)?;
	Some((value, tail_len + 1))
}

/// Decodes a PolkaVM varint whose first byte was already read from `input`.
///
/// Returns the decoded value and the number of bytes that were consumed from `input`.
fn read_varint(input: &[u8], first_byte: u8) -> Option<(u32, usize)> {
	// The number of leading ones in the first byte is the number of following bytes.
	let tail_len = (!first_byte).leading_zeros() as usize;
	if tail_len >= MAX_VARINT_LEN {
		return None;
	}

	let tail = input.get(..tail_len)?;
	let upper_mask = 0xFF_u32 >> tail_len;
	// `wrapping_shl` mirrors `polkavm-common`: for the maximum varint length the upper bits wrap
	// around and end up in the low bits.
	let mut value = (u32::from(first_byte) & upper_mask).wrapping_shl((tail_len * 8) as u32);
	for (i, byte) in tail.iter().enumerate() {
		value |= u32::from(*byte) << (i * 8);
	}

	Some((value, tail_len))
}

/// Rewrites the length metadata of `blob` to match its actual length.
fn write_blob_len(blob: &mut [u8]) {
	let len = (blob.len() as u64).to_le_bytes();
	blob[BLOB_LEN_OFFSET..SECTIONS_OFFSET].copy_from_slice(&len);
}

#[cfg(test)]
mod tests {
	use super::*;
	use alloc::{vec, vec::Vec};

	/// Section ids as defined by `polkavm-common`.
	const SECTION_MEMORY_CONFIG: u8 = 1;
	const SECTION_RO_DATA: u8 = 2;
	const SECTION_RW_DATA: u8 = 3;
	const SECTION_EXPORTS: u8 = 5;
	const SECTION_CODE_AND_JUMP_TABLE: u8 = 6;

	/// Builds a minimal but structurally valid program blob.
	fn valid_blob() -> Vec<u8> {
		let mut blob = Vec::new();
		blob.extend_from_slice(b"PVM\0");
		blob.push(0); // `InstructionSetKind::ReviveV1`
		blob.extend_from_slice(&[0; 8]); // `blob_len`, patched below

		// Memory config: empty ro/rw data and stack.
		blob.extend_from_slice(&[SECTION_MEMORY_CONFIG, 3, 0, 0, 0]);
		// Empty ro data, rw data and exports.
		blob.extend_from_slice(&[SECTION_RO_DATA, 0, SECTION_RW_DATA, 0, SECTION_EXPORTS, 0]);
		// Code and jump table: no jump table entries, no code.
		blob.extend_from_slice(&[SECTION_CODE_AND_JUMP_TABLE, 3, 0, 0, 0]);
		blob.push(0); // end-of-file marker
		patch_blob_len(&mut blob);
		blob
	}

	fn patch_blob_len(blob: &mut [u8]) {
		let len = (blob.len() as u64).to_le_bytes();
		blob[5..13].copy_from_slice(&len);
	}

	fn assert_parseable(blob: &[u8]) {
		polkavm::ProgramBlob::parse(blob.into())
			.expect("the resulting blob must still be a valid PolkaVM program");
	}

	#[test]
	fn fixture_is_parseable() {
		assert_parseable(&valid_blob());
	}

	#[test]
	fn is_valid_accepts_valid_and_rejects_malformed_blobs() {
		assert!(is_valid(&valid_blob()));
		assert!(is_valid(&append_custom_section(&valid_blob(), PADDING_SECTION, b"padding")));
		assert!(!is_valid(b"not a blob"));
		assert!(!is_valid(&[]));
	}

	#[test]
	fn custom_section_returns_none_when_absent() {
		assert_eq!(custom_section(&valid_blob(), PADDING_SECTION), None);
	}

	#[test]
	fn appended_section_roundtrips() {
		let blob = append_custom_section(&valid_blob(), PADDING_SECTION, b"payload");
		assert_eq!(custom_section(&blob, PADDING_SECTION), Some(&b"payload"[..]));
		assert_parseable(&blob);
	}

	#[test]
	fn empty_payload_roundtrips() {
		let blob = append_custom_section(&valid_blob(), PADDING_SECTION, &[]);
		assert_eq!(custom_section(&blob, PADDING_SECTION), Some(&[][..]));
		assert_parseable(&blob);
	}

	#[test]
	fn payload_longer_than_127_bytes_roundtrips() {
		let payload = vec![0xAB; 300];
		let blob = append_custom_section(&valid_blob(), PADDING_SECTION, &payload);
		assert_eq!(custom_section(&blob, PADDING_SECTION), Some(payload.as_slice()));
		assert_parseable(&blob);
	}

	#[test]
	fn multiple_sections_roundtrip() {
		const SECTION_AUX: u8 = 0xC9;
		let blob = valid_blob();
		let blob = append_custom_section(&blob, PADDING_SECTION, b"padding");
		let blob = append_custom_section(&blob, SECTION_AUX, b"aux");
		assert_eq!(custom_section(&blob, PADDING_SECTION), Some(&b"padding"[..]));
		assert_eq!(custom_section(&blob, SECTION_AUX), Some(&b"aux"[..]));
		assert_parseable(&blob);
	}

	#[test]
	fn unknown_optional_sections_are_skipped_by_the_parser() {
		let blob = append_custom_section(&valid_blob(), PADDING_SECTION, &[0x42; 64]);
		assert_parseable(&blob);
	}

	#[test]
	fn set_replaces_existing_section() {
		let base = valid_blob();
		let appended = append_custom_section(&base, PADDING_SECTION, b"first");
		let replaced = set_custom_section(&appended, PADDING_SECTION, b"second");
		assert_eq!(custom_section(&replaced, PADDING_SECTION), Some(&b"second"[..]));
		// No duplicate section is left behind: setting on the appended blob yields the same bytes
		// as appending to the pristine blob.
		assert_eq!(replaced, append_custom_section(&base, PADDING_SECTION, b"second"));
		assert_parseable(&replaced);
	}

	#[test]
	fn set_adds_section_when_absent() {
		let blob = set_custom_section(&valid_blob(), PADDING_SECTION, b"payload");
		assert_eq!(custom_section(&blob, PADDING_SECTION), Some(&b"payload"[..]));
		assert_parseable(&blob);
	}

	#[test]
	fn custom_section_returns_none_for_malformed_blobs() {
		// Empty blob.
		assert_eq!(custom_section(&[], PADDING_SECTION), None);

		// Bad magic.
		let mut blob = valid_blob();
		blob[0] = b'X';
		assert_eq!(custom_section(&blob, PADDING_SECTION), None);

		// Length metadata that doesn't match the actual length.
		let mut blob = valid_blob();
		blob[5] = blob[5].wrapping_add(1);
		assert_eq!(custom_section(&blob, PADDING_SECTION), None);

		// Missing end-of-file marker.
		let mut blob = valid_blob();
		blob.pop();
		patch_blob_len(&mut blob);
		assert_eq!(custom_section(&blob, PADDING_SECTION), None);

		// Truncated section payload.
		let mut blob = append_custom_section(&valid_blob(), PADDING_SECTION, b"padding");
		blob.truncate(blob.len() - 2);
		patch_blob_len(&mut blob);
		assert_eq!(custom_section(&blob, PADDING_SECTION), None);

		// Section length pointing past the end of the blob.
		let mut blob = valid_blob();
		blob.pop();
		blob.extend_from_slice(&[PADDING_SECTION, 0xFF, 0xFF, 0x01]);
		patch_blob_len(&mut blob);
		assert_eq!(custom_section(&blob, PADDING_SECTION), None);

		// Varint with more than four continuation bytes.
		let mut blob = valid_blob();
		blob.pop();
		blob.extend_from_slice(&[PADDING_SECTION, 0xF8, 0, 0, 0, 0, 0]);
		patch_blob_len(&mut blob);
		assert_eq!(custom_section(&blob, PADDING_SECTION), None);
	}

	#[test]
	#[should_panic(expected = "expects a valid PolkaVM program blob")]
	fn append_panics_on_malformed_blob() {
		append_custom_section(b"not a blob", PADDING_SECTION, b"payload");
	}

	#[test]
	#[should_panic(expected = "expects a valid PolkaVM program blob")]
	fn set_panics_on_malformed_blob() {
		set_custom_section(b"not a blob", PADDING_SECTION, b"payload");
	}

	#[test]
	#[should_panic(expected = "end-of-file marker")]
	fn append_rejects_end_of_file_id() {
		append_custom_section(&valid_blob(), 0, b"payload");
	}
}
