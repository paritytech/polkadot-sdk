// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Proofs that the JAM anchors of a parachain block are ancestors of a work package's anchor.
//!
//! A parachain block records the JAM `anchor` and `lookup_anchor` it was built against in its
//! [`CumulusDigestItem::JamParent`](crate::CumulusDigestItem::JamParent) digest. When the block
//! is resubmitted in a later work package those hashes no longer equal the refine context, so the
//! package carries a chain of encoded JAM headers instead: the first header is the context anchor
//! itself and every further header is the parent of the one before it. Each header is pinned by
//! hash to the `parent` field of its child, so the hashes, slots and state roots read from the
//! chain are exactly as trustworthy as the context anchor the JAM validators already checked.
//!
//! Only the fixed-size prefix of a header is read. The encoding of polkajam's
//! `jam_std_common::Header` starts with `parent` (32 bytes), `parent_state_root` (32 bytes),
//! `extrinsic_hash` (32 bytes) and `slot` (`u32`, little-endian), followed by variable-size marks
//! and signatures. The header hash is blake2b-256 over the full encoding.

use crate::relay_chain::Hash;
use alloc::vec::Vec;
use sp_crypto_hashing::blake2_256;

/// Length of the fixed-size header prefix read by [`JamHeaderPrefix::parse`].
pub const JAM_HEADER_PREFIX_LEN: usize = 100;

/// Longest header chain [`verify_anchor_chain`] accepts.
pub const MAX_ANCHOR_CHAIN_LEN: usize = 64;

/// The fields of a JAM header an anchor proof needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JamHeaderPrefix {
	/// Hash of the parent header.
	pub parent: Hash,
	/// Posterior state root of the parent block.
	pub parent_state_root: Hash,
	/// Slot of the block.
	pub slot: u32,
}

impl JamHeaderPrefix {
	/// Reads the prefix of an encoded JAM header.
	///
	/// Bytes `0..32` are the parent hash, `32..64` the parent state root and `96..100` the slot
	/// (little-endian); the extrinsic hash in between is ignored. Returns `None` for anything
	/// shorter than [`JAM_HEADER_PREFIX_LEN`].
	pub fn parse(encoded: &[u8]) -> Option<Self> {
		if encoded.len() < JAM_HEADER_PREFIX_LEN {
			return None;
		}

		let hash = |range: core::ops::Range<usize>| {
			<[u8; 32]>::try_from(&encoded[range]).ok().map(Hash::from)
		};
		let slot = <[u8; 4]>::try_from(&encoded[96..100]).ok().map(u32::from_le_bytes)?;

		Some(Self { parent: hash(0..32)?, parent_state_root: hash(32..64)?, slot })
	}
}

/// Hash of an encoded JAM header: blake2b-256 over the full encoding.
pub fn jam_header_hash(encoded: &[u8]) -> Hash {
	blake2_256(encoded).into()
}

/// Why a header chain, or an anchor looked up in one, was rejected.
#[derive(Debug, PartialEq, Eq)]
pub enum AnchorProofError {
	/// The chain has no headers.
	Empty,
	/// The chain has more than [`MAX_ANCHOR_CHAIN_LEN`] headers.
	TooLong,
	/// The header at this index is shorter than [`JAM_HEADER_PREFIX_LEN`].
	Malformed(u32),
	/// The first header is not the context anchor.
	AnchorMismatch,
	/// The header at this index is not the parent of the header before it.
	BrokenLink(u32),
	/// A slot differs from the one the chain records for the block.
	SlotMismatch,
	/// The hash is not in the chain.
	Unproven,
	/// The block is in the chain but its posterior state root is not.
	StateRootUnknown,
}

/// The JAM blocks a verified header chain vouches for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvenAnchors {
	/// Header hash, slot and posterior state root (if known) of every block in the chain.
	entries: Vec<(Hash, u32, Option<Hash>)>,
}

impl ProvenAnchors {
	/// Checks that `hash` is a proven block at `slot` and returns its posterior state root.
	pub fn anchor(&self, hash: &Hash, slot: u32) -> Result<Hash, AnchorProofError> {
		let (_, _, root) = self.entry(hash, slot)?;
		root.ok_or(AnchorProofError::StateRootUnknown)
	}

	/// Checks that `hash` is a proven block at `slot`.
	pub fn lookup_anchor(&self, hash: &Hash, slot: u32) -> Result<(), AnchorProofError> {
		self.entry(hash, slot).map(|_| ())
	}

	fn entry(
		&self,
		hash: &Hash,
		slot: u32,
	) -> Result<&(Hash, u32, Option<Hash>), AnchorProofError> {
		let entry = self
			.entries
			.iter()
			.find(|(entry_hash, _, _)| entry_hash == hash)
			.ok_or(AnchorProofError::Unproven)?;

		if entry.1 != slot {
			return Err(AnchorProofError::SlotMismatch);
		}

		Ok(entry)
	}
}

/// Verifies a chain of encoded JAM headers, newest first, starting at the refine context's anchor.
///
/// `headers[0]` must hash to `context_anchor` and carry `context_anchor_slot`; every `headers[i]`
/// must hash to the `parent` of `headers[i - 1]`. The posterior state root of the context anchor
/// is `context_state_root`; the posterior state root of every older block is the
/// `parent_state_root` its child records, so every proven block has one.
pub fn verify_anchor_chain(
	context_anchor: Hash,
	context_anchor_slot: u32,
	context_state_root: Hash,
	headers: &[Vec<u8>],
) -> Result<ProvenAnchors, AnchorProofError> {
	if headers.is_empty() {
		return Err(AnchorProofError::Empty);
	}
	if headers.len() > MAX_ANCHOR_CHAIN_LEN {
		return Err(AnchorProofError::TooLong);
	}

	let mut entries = Vec::with_capacity(headers.len());
	// The prefix of the previous (newer) header; it links to and roots the current one.
	let mut child: Option<JamHeaderPrefix> = None;

	for (index, encoded) in headers.iter().enumerate() {
		let prefix =
			JamHeaderPrefix::parse(encoded).ok_or(AnchorProofError::Malformed(index as u32))?;
		let hash = jam_header_hash(encoded);

		let root = match &child {
			None => {
				if hash != context_anchor {
					return Err(AnchorProofError::AnchorMismatch);
				}
				if prefix.slot != context_anchor_slot {
					return Err(AnchorProofError::SlotMismatch);
				}
				context_state_root
			},
			Some(child) => {
				if hash != child.parent {
					return Err(AnchorProofError::BrokenLink(index as u32));
				}
				child.parent_state_root
			},
		};

		entries.push((hash, prefix.slot, Some(root)));
		child = Some(prefix);
	}

	Ok(ProvenAnchors { entries })
}

#[cfg(test)]
mod tests {
	use super::*;
	use alloc::vec;

	const FIRST_SLOT: u32 = 10;

	/// xorshift64, so the "random" header tails are reproducible.
	struct Rng(u64);

	impl Rng {
		fn next(&mut self) -> u64 {
			self.0 ^= self.0 << 13;
			self.0 ^= self.0 >> 7;
			self.0 ^= self.0 << 17;
			self.0
		}

		fn bytes(&mut self, len: usize) -> Vec<u8> {
			(0..len).map(|_| self.next() as u8).collect()
		}
	}

	/// A synthetic encoded JAM header: the 100-byte prefix followed by a random tail.
	fn header(parent: Hash, parent_state_root: Hash, slot: u32, rng: &mut Rng) -> Vec<u8> {
		let mut encoded = Vec::new();
		encoded.extend_from_slice(parent.as_bytes());
		encoded.extend_from_slice(parent_state_root.as_bytes());
		encoded.extend_from_slice(&rng.bytes(32));
		encoded.extend_from_slice(&slot.to_le_bytes());
		let tail_len = 1 + (rng.next() % 200) as usize;
		encoded.extend_from_slice(&rng.bytes(tail_len));
		encoded
	}

	/// The posterior state root the fixture gives the block at `slot`.
	fn root(slot: u32) -> Hash {
		Hash::repeat_byte(slot as u8)
	}

	/// The posterior state root of the context anchor as the refine context carries it.
	fn context_root() -> Hash {
		Hash::repeat_byte(0xCC)
	}

	/// `len` linked headers at slots `FIRST_SLOT..`, newest first; the header at `slot` records
	/// `root(slot - 1)` as its `parent_state_root`.
	fn chain(len: usize) -> Vec<Vec<u8>> {
		let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
		let mut parent = Hash::repeat_byte(0xEE);
		let mut headers = Vec::with_capacity(len);

		for slot in FIRST_SLOT..FIRST_SLOT + len as u32 {
			let encoded = header(parent, root(slot - 1), slot, &mut rng);
			parent = jam_header_hash(&encoded);
			headers.push(encoded);
		}

		headers.reverse();
		headers
	}

	fn newest_slot(headers: &[Vec<u8>]) -> u32 {
		FIRST_SLOT + headers.len() as u32 - 1
	}

	/// Verifies a fixture chain against the context its newest header defines.
	fn verify(headers: &[Vec<u8>]) -> Result<ProvenAnchors, AnchorProofError> {
		verify_anchor_chain(
			jam_header_hash(&headers[0]),
			newest_slot(headers),
			context_root(),
			headers,
		)
	}

	#[test]
	fn prefix_parse_reads_the_header_layout() {
		let parent = Hash::repeat_byte(1);
		let parent_state_root = Hash::repeat_byte(2);
		let encoded = header(parent, parent_state_root, 0x0403_0201, &mut Rng(7));

		assert!(encoded.len() > JAM_HEADER_PREFIX_LEN);
		assert_eq!(&encoded[96..100], &[1, 2, 3, 4]);
		assert_eq!(
			JamHeaderPrefix::parse(&encoded),
			Some(JamHeaderPrefix { parent, parent_state_root, slot: 0x0403_0201 })
		);
		assert_eq!(
			JamHeaderPrefix::parse(&encoded[..JAM_HEADER_PREFIX_LEN]),
			JamHeaderPrefix::parse(&encoded)
		);
		assert_eq!(JamHeaderPrefix::parse(&encoded[..JAM_HEADER_PREFIX_LEN - 1]), None);
		// The hash covers the whole encoding, not just the prefix.
		assert_ne!(jam_header_hash(&encoded), jam_header_hash(&encoded[..JAM_HEADER_PREFIX_LEN]));
	}

	#[test]
	fn single_header_chain_is_valid() {
		let headers = chain(1);
		let proven = verify(&headers).expect("one header hashing to the context anchor verifies");
		let anchor = jam_header_hash(&headers[0]);

		assert_eq!(proven.anchor(&anchor, FIRST_SLOT), Ok(context_root()));
		assert_eq!(proven.lookup_anchor(&anchor, FIRST_SLOT), Ok(()));
	}

	#[test]
	fn three_header_chain_is_valid() {
		let headers = chain(3);
		let proven = verify(&headers).expect("linked headers verify");

		for (index, encoded) in headers.iter().enumerate() {
			let hash = jam_header_hash(encoded);
			let slot = newest_slot(&headers) - index as u32;
			assert!(proven.anchor(&hash, slot).is_ok(), "header {index}");
			assert_eq!(proven.lookup_anchor(&hash, slot), Ok(()), "header {index}");
		}
	}

	#[test]
	fn roots_come_from_the_context_and_the_child_header() {
		let headers = chain(3);
		let proven = verify(&headers).expect("linked headers verify");
		let hash = |index: usize| jam_header_hash(&headers[index]);
		let recorded_parent_root = |index: usize| {
			JamHeaderPrefix::parse(&headers[index])
				.expect("fixture headers parse")
				.parent_state_root
		};

		// The context anchor gets the root the refine context carries.
		assert_eq!(proven.anchor(&hash(0), FIRST_SLOT + 2), Ok(context_root()));
		// Every older block gets the `parent_state_root` its child records.
		assert_eq!(proven.anchor(&hash(1), FIRST_SLOT + 1), Ok(root(FIRST_SLOT + 1)));
		assert_eq!(recorded_parent_root(0), root(FIRST_SLOT + 1));
		assert_eq!(proven.anchor(&hash(2), FIRST_SLOT), Ok(root(FIRST_SLOT)));
		assert_eq!(recorded_parent_root(1), root(FIRST_SLOT));
		assert_ne!(root(FIRST_SLOT), context_root());
	}

	#[test]
	fn anchor_mismatch_is_rejected() {
		let headers = chain(3);
		let not_the_first = jam_header_hash(&headers[1]);

		assert_eq!(
			verify_anchor_chain(not_the_first, newest_slot(&headers), context_root(), &headers),
			Err(AnchorProofError::AnchorMismatch)
		);
	}

	#[test]
	fn wrong_context_slot_is_rejected() {
		let headers = chain(3);

		assert_eq!(
			verify_anchor_chain(
				jam_header_hash(&headers[0]),
				newest_slot(&headers) + 1,
				context_root(),
				&headers
			),
			Err(AnchorProofError::SlotMismatch)
		);
	}

	#[test]
	fn broken_link_is_rejected() {
		// Changing one byte of the extrinsic hash keeps the prefix valid but changes the hash
		// the newer header links to.
		let mut headers = chain(3);
		headers[1][64] ^= 1;
		assert_eq!(verify(&headers), Err(AnchorProofError::BrokenLink(1)));

		// An unrelated header with the right slot does not pass as the oldest link either.
		let mut headers = chain(3);
		headers[2] =
			header(Hash::repeat_byte(0x55), root(FIRST_SLOT - 1), FIRST_SLOT, &mut Rng(99));
		assert_eq!(verify(&headers), Err(AnchorProofError::BrokenLink(2)));
	}

	#[test]
	fn wrong_slot_per_entry_is_rejected() {
		let headers = chain(3);
		let proven = verify(&headers).expect("linked headers verify");
		let middle = jam_header_hash(&headers[1]);

		assert_eq!(proven.anchor(&middle, FIRST_SLOT + 2), Err(AnchorProofError::SlotMismatch));
		assert_eq!(proven.lookup_anchor(&middle, FIRST_SLOT), Err(AnchorProofError::SlotMismatch));
	}

	#[test]
	fn unproven_hash_is_rejected() {
		let headers = chain(3);
		let proven = verify(&headers).expect("linked headers verify");
		let unknown = Hash::repeat_byte(0x42);

		assert_eq!(proven.anchor(&unknown, FIRST_SLOT), Err(AnchorProofError::Unproven));
		assert_eq!(proven.lookup_anchor(&unknown, FIRST_SLOT), Err(AnchorProofError::Unproven));

		// The parent of the oldest header is named by the chain but not proven by it.
		let before_the_chain =
			JamHeaderPrefix::parse(&headers[2]).expect("fixture headers parse").parent;
		assert_eq!(
			proven.lookup_anchor(&before_the_chain, FIRST_SLOT - 1),
			Err(AnchorProofError::Unproven)
		);
	}

	#[test]
	fn empty_chain_is_rejected() {
		assert_eq!(
			verify_anchor_chain(Hash::zero(), FIRST_SLOT, context_root(), &[]),
			Err(AnchorProofError::Empty)
		);
	}

	#[test]
	fn too_long_chain_is_rejected() {
		assert!(verify(&chain(MAX_ANCHOR_CHAIN_LEN)).is_ok());
		assert_eq!(verify(&chain(MAX_ANCHOR_CHAIN_LEN + 1)), Err(AnchorProofError::TooLong));
	}

	#[test]
	fn malformed_header_is_rejected() {
		let short = vec![0u8; JAM_HEADER_PREFIX_LEN - 1];
		assert_eq!(
			verify_anchor_chain(jam_header_hash(&short), FIRST_SLOT, context_root(), &[short]),
			Err(AnchorProofError::Malformed(0))
		);

		let mut headers = chain(3);
		headers[1].truncate(JAM_HEADER_PREFIX_LEN - 1);
		assert_eq!(verify(&headers), Err(AnchorProofError::Malformed(1)));
	}

	#[test]
	fn unknown_state_root_is_reported() {
		let hash = Hash::repeat_byte(0x11);
		let proven = ProvenAnchors { entries: vec![(hash, FIRST_SLOT, None)] };

		assert_eq!(proven.anchor(&hash, FIRST_SLOT), Err(AnchorProofError::StateRootUnknown));
		assert_eq!(proven.lookup_anchor(&hash, FIRST_SLOT), Ok(()));
	}
}
