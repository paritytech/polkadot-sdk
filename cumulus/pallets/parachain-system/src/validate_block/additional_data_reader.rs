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

//! Verification-side reader over the relay-state proof carried in a block's additional data.

use super::{trie_cache::CacheProvider, trie_recorder::ProofRecorderProvider};
use crate::relay_state_snapshot::Error;
use alloc::vec::Vec;
use codec::Encode;
use cumulus_primitives_additional_data::RelayStateReader;
use cumulus_primitives_core::relay_chain::{Block, Hash};
use sp_additional_data::{hash_value, AdditionalDataFinalizer};
use sp_runtime::traits::HashingFor;
use sp_state_machine::{Backend, TrieBackendBuilder};
use sp_trie::{HashDBT, MemoryDB, ProofSizeProvider, StorageProof, EMPTY_PREFIX};

/// A trie-backed reader over a relay-state proof.
///
/// Used on the validation/import side to *serve* the `read_relay_chain_state` host function from
/// the proof recorded in the block's additional data (it is not used by the runtime's read path,
/// which goes through the host function). Kept minimal: it only needs to answer raw reads.
pub struct AdditionalDataReader {
	root: Hash,
	db: MemoryDB<HashingFor<Block>>,
	recorder: ProofRecorderProvider<HashingFor<Block>>,
}

impl AdditionalDataReader {
	/// Build from a relay-state `proof`, verifying it against the trusted `root`.
	///
	/// Returns an error if `root` is not the root of `proof`.
	pub fn new(root: Hash, proof: StorageProof) -> Result<Self, Error> {
		let db = proof.into_memory_db::<HashingFor<Block>>();
		if !db.contains(&root, EMPTY_PREFIX) {
			return Err(Error::RootMismatch);
		}
		Ok(Self { root, db, recorder: ProofRecorderProvider::default() })
	}
}

impl RelayStateReader for AdditionalDataReader {
	/// Read the stored bytes under `key` (proven absence returns `None`), recording the nodes
	/// accessed so [`AdditionalDataFinalizer::finalize`] can reassemble exactly what was read.
	///
	/// A fresh (empty) cache is used per read so the recorder observes every node on the access
	/// path. `new_with_cache` (rather than `new`) is what lets us give the backend our
	/// [`ProofRecorderProvider`] instead of the no_std default (unimplemented) recorder.
	///
	/// A read *error* (the carried proof is missing nodes on the key's path) is a rejected
	/// candidate, not proven-absence: collapsing it to `None` would let a collator suppress a
	/// present value by omitting its proof nodes — consistently on build and validate, so
	/// re-execution wouldn't catch it. Hence we panic rather than return `None`.
	fn read(&self, key: &[u8]) -> Option<Vec<u8>> {
		let cache_provider = CacheProvider::<HashingFor<Block>>::new();
		let recording = TrieBackendBuilder::new_with_cache(&self.db, self.root, &cache_provider)
			.with_recorder(self.recorder.clone())
			.build();
		recording
			.storage(key)
			.expect("relay-state read from an incomplete proof; candidate omitted required nodes")
	}

	/// Estimated encoded size of the relay-read proof recorded so far — the additional-data
	/// contribution to the PoV. Summed into `storage_proof_size` so the runtime budgets for it.
	/// Uses the same per-node metric as the build-side recorder (see [`ProofRecorderProvider`]).
	fn proof_size(&self) -> usize {
		self.recorder.estimate_encoded_size()
	}
}

impl AdditionalDataFinalizer for AdditionalDataReader {
	/// blake2 commitment to the relay-read value `(root, proof)` reassembled from exactly the nodes
	/// read so far, or `None` if nothing was read. Mirrors what the collator committed for an
	/// honest, minimal candidate: `frame_executive`'s digest-equality rejects a candidate whose
	/// carried value differs from what its execution actually requested.
	fn finalize(&self) -> Option<[u8; 32]> {
		let proof = self.recorder.to_storage_proof();
		(!proof.is_empty()).then(|| hash_value(&(self.root, proof).encode()))
	}
}
