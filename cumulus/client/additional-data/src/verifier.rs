// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! Generic block-import additional-data provider.
//!
//! [`VerifyingAdditionalDataProvider`] answers `read_relay_chain_state` from the relay-state proof
//! carried in the block's additional data, authenticating each read against a root. It serves the
//! **generic block-import** path — a full node re-executing a synced block — via
//! [`from_map`](VerifyingAdditionalDataProvider::from_map).
//!
//! It is *not* the provider used inside the PVF: `validate_block` has its own no_std reader
//! (`AdditionalDataReader`, in `cumulus-pallet-parachain-system`). This type is the std analogue of
//! that reader, and is kept in Cumulus (not in `sp-additional-data`) because it is
//! Cumulus-specific: substrate only knows the `AdditionalDataFinalizer` trait, and the node
//! injects a factory that builds this provider.

use codec::{Decode, Encode};
use cumulus_primitives_additional_data::{RelayStateReader, RELAY_PROOF_KEY};
use hash_db::Hasher;
use sp_additional_data::{hash_value, AdditionalData, AdditionalDataFinalizer};
use sp_state_machine::{Backend, TrieBackend, TrieBackendBuilder};
use sp_trie::{recorder::Recorder, HashDBT, MemoryDB, StorageProof, EMPTY_PREFIX};

/// Generic block-import provider: reads relay/JAM keys back from the carried proof, authenticating
/// each read (including proven absence) against `root`, and recording the nodes it accesses so
/// [`Self::finalize`] can hash the map assembled from exactly what was requested.
///
/// Built two ways: [`from_map`](Self::from_map) trusts the root carried in the map — the generic
/// block-import path, which trusts relay finality since the PVF is the authoritative validator;
/// [`from_map_with_root`](Self::from_map_with_root) verifies against a caller-supplied trusted
/// root, mirroring the PVF's check — used by std tests that stand in for `validate_block` (the real
/// PVF uses the separate no_std `AdditionalDataReader`).
pub struct VerifyingAdditionalDataProvider<H: Hasher> {
	root: H::Out,
	backend: TrieBackend<MemoryDB<H>, H>,
	recorder: Recorder<H>,
}

impl<H: Hasher> VerifyingAdditionalDataProvider<H>
where
	H::Out: Ord + codec::Codec + Clone,
{
	/// Construct from the carried `map`, verifying reads against the caller-supplied trusted `root`
	/// (the map's own carried root is ignored). Mirrors the check the PVF performs against
	/// `relay_parent_storage_root`; used by std tests that stand in for `validate_block` (the real
	/// PVF uses the no_std `AdditionalDataReader`).
	pub fn from_map_with_root(root: H::Out, map: AdditionalData) -> Option<Self> {
		let recorder = Recorder::default();
		let backend = backend_from_map::<H>(&root, &map, recorder.clone())?;
		Some(Self { root, backend, recorder })
	}

	/// Construct from the carried `map`, trusting the root carried inside it. Used on generic
	/// import, which has no validation params.
	pub fn from_map(map: AdditionalData) -> Option<Self> {
		let proof_bytes = map.get(RELAY_PROOF_KEY)?;
		let (root, _proof) = <(H::Out, StorageProof)>::decode(&mut &proof_bytes[..]).ok()?;
		Self::from_map_with_root(root, map)
	}
}

impl<H> RelayStateReader for VerifyingAdditionalDataProvider<H>
where
	H: Hasher + Send + 'static,
	H::Out: Ord + codec::Codec + Send + Clone,
{
	fn read(&self, key: &[u8]) -> Option<Vec<u8>> {
		// The backend records every trie node it touches into `self.recorder` (attached at
		// construction), so `finalize` can reassemble exactly what was requested. An `Err` here
		// means the proof is missing nodes on the key's path (or does not verify against `root`) —
		// reject rather than treat as absent, else a collator could suppress a present value by
		// omitting its proof nodes. A *proven* absent key is `Ok(None)`.
		self.backend
			.storage(key)
			.expect("relay-state read from an incomplete/invalid proof; candidate is invalid")
	}

	fn proof_size(&self) -> usize {
		self.recorder.estimate_encoded_size()
	}
}

impl<H> AdditionalDataFinalizer for VerifyingAdditionalDataProvider<H>
where
	H: Hasher + Send + 'static,
	H::Out: Ord + codec::Codec + Send + Clone,
{
	fn finalize(&self) -> Option<[u8; 32]> {
		// Commit to the value assembled from exactly what this verifier's reads recorded (NOT the
		// carried bytes). `frame_executive`'s digest-equality then rejects any candidate whose
		// carried value differs from what its execution requested.
		let proof = self.recorder.to_storage_proof();
		(!proof.is_empty()).then(|| hash_value(&(self.root.clone(), proof).encode()))
	}
}

/// Build a trie backend over the relay-read proof carried in `map[RELAY_PROOF_KEY]`, verifying
/// it against `root`. `recorder` is attached to the backend so every subsequent read accumulates
/// the accessed trie nodes into it (for `finalize`). Returns `None` if the key is absent, the value
/// is malformed, or the proof does not contain `root`.
fn backend_from_map<H: Hasher>(
	root: &H::Out,
	map: &AdditionalData,
	recorder: Recorder<H>,
) -> Option<TrieBackend<MemoryDB<H>, H>>
where
	H::Out: Ord + codec::Codec + Clone,
{
	let proof_bytes = map.get(RELAY_PROOF_KEY)?;
	let (_carried_root, proof) = <(H::Out, StorageProof)>::decode(&mut &proof_bytes[..]).ok()?;
	let db = proof.into_memory_db::<H>();
	if !db.contains(root, EMPTY_PREFIX) {
		return None;
	}
	Some(TrieBackendBuilder::new(db, *root).with_recorder(recorder).build())
}
