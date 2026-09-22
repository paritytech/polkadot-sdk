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

//! [`JamStateReader`] implementation over a verified, carried JAM [`StateProof`].
//!
//! The build side records the proof live (task 9); this reader is the other half: it holds a
//! fixed [`StateProof`] that has already been verified against the trusted `anchor_state_root`
//! once, node-side, before construction, and serves reads through it. Authoring, refine and import
//! all read through the same reader over the same carried proof, so a collator cannot diverge from
//! what the proof commits to.

extern crate alloc;

use alloc::vec::Vec;

use codec::Encode;
use parachain_service_core::{verify, Hash, StateProof};

use crate::StateKey;

use crate::JamStateReader;

/// Reads JAM chain-state values through a verified, carried [`StateProof`].
///
/// The proof is FIXED and already authenticated: it was verified against `anchor_state_root` once
/// before the reader was built, and every read re-verifies the same proof against that root.
///
/// # Panics
///
/// [`read`](JamStateReader::read) panics when
/// [`verify`](parachain_service_core::verify) returns an error: an incomplete or malformed proof
/// cannot authenticate the key, which makes the candidate block invalid. It must fail loudly,
/// never read as `None` — collapsing a verify error to `None` would let a collator suppress a
/// present value by omitting proof nodes.
pub struct JamProofReader {
	anchor_state_root: Hash,
	proof: StateProof,
}

impl JamProofReader {
	/// Build a reader over a `proof` that the caller has already verified against
	/// `anchor_state_root`.
	pub fn new(anchor_state_root: Hash, proof: StateProof) -> Self {
		Self { anchor_state_root, proof }
	}
}

impl JamStateReader for JamProofReader {
	fn read(&self, state_key: &StateKey) -> Option<Vec<u8>> {
		verify(&self.proof, &self.anchor_state_root, state_key).expect(
			"the carried JAM state proof cannot authenticate the requested key; \
			 an incomplete or invalid proof is an invalid candidate block and must fail loudly, \
			 never read as proven absence (collapsing the error to `None` would let a collator \
			 suppress a present value by omitting proof nodes); qed",
		)
	}

	fn proof_size(&self) -> usize {
		self.proof.encode().len()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use parachain_service_core::{
		blake2_256, service_request_state_key, service_value_state_key, types::ParaId, ProofNode,
	};

	const SERVICE_ID: u32 = 9;
	const EMPTY_HASH: Hash = [0u8; 32];
	/// The preimage `(hash, len)` the request-key tests derive a request state key from.
	const REQUEST_HASH: [u8; 32] = [0x42; 32];
	const REQUEST_LEN: u32 = 4096;

	/// Builds a reader over a real trie-produced proof for `entries` keyed by state key.
	fn reader_for(entries: Vec<(StateKey, Vec<u8>)>) -> (JamProofReader, StateProof) {
		let trie = Trie::new(entries);
		let proof = trie.proof();
		let reader = JamProofReader::new(trie.root, proof.clone());
		(reader, proof)
	}

	/// A SCALE-encoded [`ParaInfo`](parachain_service_core::ParaInfo) with an active code and no
	/// announced upgrade — a realistic value for the anchor read, large enough to take the
	/// hashed-leaf path.
	fn para_info_value() -> Vec<u8> {
		use parachain_service_core::{
			types::{HeadData, ValidationCodeHash, ValidationCodeRef},
			ParaInfo,
		};

		ParaInfo {
			head_data: HeadData::try_from(vec![0xca, 0xfe]).expect("2 bytes < 4 KiB; qed"),
			validation_code: Some(ValidationCodeRef {
				hash: ValidationCodeHash([0x11; 32]),
				len: 1,
			}),
			announced_upgrade: None,
			total_state_balance: 0,
			used_state_balance: 0,
			is_deregistering: false,
		}
		.encode()
	}

	/// A trie built from a full key/value set, able to emit a proof for any key. Independent
	/// Gray-Paper merklization, byte-pinned against polkajam's trie by jam-state-helpers' own
	/// `root_matches_polkajam` test.
	struct Trie {
		entries: Vec<(StateKey, Vec<u8>)>,
		nodes: Vec<ProofNode>,
		root: Hash,
	}

	impl Trie {
		fn new(mut entries: Vec<(StateKey, Vec<u8>)>) -> Self {
			entries.sort_by_key(|(a, _)| *a);
			let mut trie = Trie { entries: entries.clone(), nodes: Vec::new(), root: EMPTY_HASH };
			trie.root = trie.hash_subtree(0, &entries);
			trie
		}

		fn hash_subtree(&mut self, depth: usize, entries: &[(StateKey, Vec<u8>)]) -> Hash {
			match entries {
				[] => EMPTY_HASH,
				[(key, value)] => self.push(leaf_node(key, value)),
				_ => {
					let (left, right): (Vec<_>, Vec<_>) =
						entries.iter().cloned().partition(|(key, _)| bit_at(key, depth) == 0);
					let left = self.hash_subtree(depth + 1, &left);
					let right = self.hash_subtree(depth + 1, &right);
					self.push(branch_node(&left, &right))
				},
			}
		}

		fn push(&mut self, node: ProofNode) -> Hash {
			let hash = blake2_256(&node);
			self.nodes.push(node);
			hash
		}

		fn proof(&self) -> StateProof {
			StateProof {
				nodes: self.nodes.clone(),
				values: self
					.entries
					.iter()
					.filter(|(_, value)| value.len() > 32)
					.map(|(k, v)| (*k, v.clone()))
					.collect(),
			}
		}
	}

	fn bit_at(key: &StateKey, depth: usize) -> u8 {
		(key[depth / 8] >> (7 - (depth % 8))) & 1
	}

	fn leaf_node(key: &StateKey, value: &[u8]) -> ProofNode {
		let mut node = [0u8; 64];
		node[1..32].copy_from_slice(key);
		if value.len() > 32 {
			node[0] = 0b1100_0000;
			node[32..].copy_from_slice(&blake2_256(value));
		} else {
			node[0] = 0b1000_0000 | value.len() as u8;
			node[32..32 + value.len()].copy_from_slice(value);
		}
		node
	}

	fn branch_node(left: &Hash, right: &Hash) -> ProofNode {
		let mut node = [0u8; 64];
		node[..32].copy_from_slice(left);
		node[32..].copy_from_slice(right);
		node[0] &= 0b0111_1111;
		node
	}

	#[test]
	fn present_key_reads_value() {
		let value = b"the para head".to_vec();
		let state_key = service_value_state_key(SERVICE_ID, b"present");
		let (reader, _) = reader_for(vec![(state_key, value.clone())]);

		assert_eq!(reader.read(&state_key), Some(value));
	}

	#[test]
	fn proven_absent_key_reads_none() {
		let stored = service_value_state_key(SERVICE_ID, b"present");
		let (reader, _) = reader_for(vec![(stored, b"head of present".to_vec())]);

		assert_eq!(reader.read(&service_value_state_key(SERVICE_ID, b"missing")), None);
	}

	/// The read is generic over the JAM state: a service-value key and a preimage-request key are
	/// both addressed directly, out of one proof over one root.
	#[test]
	fn generic_read_addresses_any_state_key() {
		let value_key = service_value_state_key(SERVICE_ID, b"present");
		let request_key = service_request_state_key(SERVICE_ID, &REQUEST_HASH, REQUEST_LEN);
		let (reader, _) = reader_for(vec![
			(value_key, b"the para head".to_vec()),
			(request_key, vec![7u32].encode()),
		]);

		assert_ne!(value_key, request_key);
		assert_eq!(reader.read(&value_key), Some(b"the para head".to_vec()));
		assert_eq!(reader.read(&request_key), Some(vec![7u32].encode()));
	}

	/// A request the proof shows absent reads `None` — the requested-only case, which the runtime
	/// reads as "not yet provided".
	#[test]
	fn absent_request_key_reads_none() {
		let stored = service_request_state_key(SERVICE_ID, &REQUEST_HASH, REQUEST_LEN);
		let (reader, _) = reader_for(vec![(stored, vec![0u32].encode())]);

		assert_eq!(
			reader.read(&service_request_state_key(SERVICE_ID, &[0xAB; 32], REQUEST_LEN)),
			None
		);
	}

	#[test]
	fn absent_in_empty_trie_reads_none() {
		let (reader, proof) = reader_for(Vec::new());
		assert!(proof.nodes.is_empty());

		assert_eq!(reader.read(&service_value_state_key(SERVICE_ID, b"anything")), None);
	}

	/// The removed-node adversarial case: a collator omitting a proof node must panic, never read
	/// as proven absence.
	#[test]
	#[should_panic(expected = "cannot authenticate the requested key")]
	fn removed_leaf_node_panics() {
		let value = b"head".to_vec();
		let state_key = service_value_state_key(SERVICE_ID, b"present");
		let trie = Trie::new(vec![
			(state_key, value.clone()),
			(service_value_state_key(SERVICE_ID, b"other"), b"other head".to_vec()),
		]);
		let mut proof = trie.proof();
		proof.nodes.retain(|node| node != &leaf_node(&state_key, &value));

		let reader = JamProofReader::new(trie.root, proof);
		let _ = reader.read(&state_key);
	}

	/// A proof that does not belong to the trusted anchor root must panic, not read as absence.
	#[test]
	#[should_panic(expected = "cannot authenticate the requested key")]
	fn wrong_root_panics() {
		let state_key = service_value_state_key(SERVICE_ID, b"present");
		let trie = Trie::new(vec![(state_key, b"real head".to_vec())]);
		let forged = Trie::new(vec![(state_key, b"forged head".to_vec())]);

		let reader = JamProofReader::new(forged.root, trie.proof());
		let _ = reader.read(&state_key);
	}

	/// A value too large to sit inside its leaf travels as a hashed commitment plus preimage;
	/// the reader must return the full value.
	#[test]
	fn large_value_roundtrip() {
		let value = vec![7u8; 100];
		let state_key = service_value_state_key(SERVICE_ID, b"big");
		let (reader, _) = reader_for(vec![(state_key, value.clone())]);

		assert_eq!(reader.read(&state_key), Some(value));
	}

	/// The canonical production shape: one proof over the anchor root serves the para-info read,
	/// and any other state key in the same trie is addressable through the same generic read.
	#[test]
	fn para_info_and_a_request_key_share_one_proof() {
		let para_info = para_info_value();
		let para_key = parachain_service_core::para_info_key(ParaId::from(0));
		let value_key = service_value_state_key(SERVICE_ID, &para_key);
		let request_key = service_request_state_key(SERVICE_ID, &REQUEST_HASH, REQUEST_LEN);
		let request_value = vec![7u32].encode();

		let (reader, _) =
			reader_for(vec![(value_key, para_info.clone()), (request_key, request_value.clone())]);

		assert_eq!(reader.read(&value_key), Some(para_info));
		assert_eq!(reader.read(&request_key), Some(request_value));
	}

	/// `proof_size` is the SCALE-encoded size of the carried proof — the additional-data
	/// contribution to the PoV — and is deterministic.
	#[test]
	fn proof_size_is_scaled_encode_len() {
		let state_key = service_value_state_key(SERVICE_ID, b"present");
		let (reader, proof) = reader_for(vec![(state_key, b"head".to_vec())]);

		assert_eq!(reader.proof_size(), proof.encode().len());
		assert_eq!(reader.proof_size(), reader.proof_size());
	}
}
