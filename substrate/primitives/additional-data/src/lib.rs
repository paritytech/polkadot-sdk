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

//! A block's "additional data": a generic, keyed, minimality-checked channel for data produced
//! during block execution and carried alongside the block.
//!
//! # Model
//!
//! Each block carries an [`AdditionalData`] — a `BTreeMap<String, Vec<u8>>` mapping a namespaced
//! producer key to that producer's opaque bytes. Today the only producer is the relay/JAM state
//! reader, under [`RELAY_PROOF_KEY`], whose value is the SCALE-encoding of `(root, StorageProof)`.
//!
//! A parachain runtime reads relay/JAM storage *dynamically during block execution* via
//! [`additional_data::read_relay_chain_state`]; each read collects the touched proof nodes into the
//! `relay_proof` entry (the read *is* the "push"). [`additional_data::finalize`] returns the blake2
//! hash of the map assembled from **exactly what was requested**, which the runtime deposits as
//! `DigestItem::AdditionalData`.
//!
//! # Minimality
//!
//! Because `finalize` hashes the *requested* map (not some externally-supplied blob) symmetrically
//! on build and validate, and the carried map is separately checked to hash to the same digest, the
//! carried map must equal the requested one: no unrequested entry, and no proof node that wasn't
//! read, can survive. The map (`BTreeMap`) and [`sp_trie::StorageProof`] (`BTreeSet`) both encode
//! canonically, so the hash is well-defined regardless of insertion order.
//!
//! [`additional_data::read_relay_chain_state`] **panics** when [`AdditionalDataExt`] is absent: a
//! consensus-critical read that cannot be served must fail loudly, not diverge silently.
//! [`additional_data::finalize`] instead returns `None` when the extension is absent — it runs from
//! `on_finalize` on every block (including contexts that never read relay state), so a missing
//! extension there means "nothing was recorded", not an error.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{collections::btree_map::BTreeMap, string::String, vec::Vec};
use codec::Encode;
use sp_crypto_hashing::blake2_256;
use sp_runtime_interface::{
	pass_by::{AllocateAndReturnByCodec, PassFatPointerAndRead},
	runtime_interface,
};

#[cfg(feature = "std")]
use sp_externalities::ExternalitiesExt;

/// A block's additional data: a map from a namespaced producer key to that producer's opaque bytes.
///
/// Carried per block through sync/DB/import; at the PoV level a `Vec` of these (one per bundled
/// block). `BTreeMap` gives a canonical (sorted-key) encoding, so hashing it is deterministic.
pub type AdditionalData = BTreeMap<String, Vec<u8>>;

/// Key under which the relay/JAM state read-proof lives in [`AdditionalData`].
///
/// The value is the SCALE-encoding of `(root, sp_trie::StorageProof)`.
pub const RELAY_PROOF_KEY: &str = "polkadot/relay_proof";

/// blake2-256 of a block's [`AdditionalData`] (its canonical SCALE encoding).
///
/// This is the 32-byte value deposited into the block header as `DigestItem::AdditionalData`.
pub fn hash(data: &AdditionalData) -> [u8; 32] {
	blake2_256(&data.encode())
}

/// Provider backing the [`AdditionalDataExt`] externalities extension.
///
/// A single provider answers both the read and the finalize; the read collects into the same map
/// that finalize hashes (and that the build-side [`AdditionalDataGetter`] later retrieves).
#[cfg(feature = "std")]
pub trait AdditionalDataProvider: Send {
	/// Read a relay/JAM storage `key`, returning the SCALE-encoding of `Option<Vec<u8>>`.
	///
	/// On build it reads the value live and collects the touched proof; on validation/import it
	/// reads the value back from the collected proof, authenticated against the trusted root.
	fn read(&self, key: &[u8]) -> Vec<u8>;

	/// Finalize and return the blake2 [`hash`] of the map assembled from exactly what was requested
	/// this block, or `None` when nothing was requested. MUST be idempotent.
	fn finalize(&self) -> Option<[u8; 32]>;

	/// Estimated encoded size of the proof recorded so far — the additional-data contribution to the
	/// PoV, summed into `storage_proof_size` so the runtime budgets for it. `0` when nothing was
	/// recorded.
	fn proof_size(&self) -> usize;
}

/// A getter for the additional-data map recorded while building a block.
///
/// The build-side recorder is moved into the [`AdditionalDataExt`] extension and consumed by the
/// proposer, so the collator keeps one of these — a closure sharing the same recorder — to retrieve
/// the recorded map after the block is built. Backend-free (hence `Send`) and idempotent.
#[cfg(feature = "std")]
pub type AdditionalDataGetter = alloc::boxed::Box<dyn Fn() -> Option<AdditionalData> + Send>;

#[cfg(feature = "std")]
sp_externalities::decl_extension! {
	/// Externalities extension wrapping an [`AdditionalDataProvider`].
	///
	/// Register this before executing a block that calls
	/// [`additional_data::read_relay_chain_state`] — on build, on `validate_block`, and on the
	/// generic block-import path.
	pub struct AdditionalDataExt(Box<dyn AdditionalDataProvider>);
}

/// Runtime interface for reading relay/JAM chain state into a block's additional data.
///
/// `read_relay_chain_state` **panics** when [`AdditionalDataExt`] is not registered — the read is
/// consensus-critical (its proof feeds the header digest), so a missing extension must fail loudly
/// rather than silently diverge. `finalize` runs from `on_finalize` on every block and instead
/// returns `None` when the extension is absent (nothing was recorded).
#[runtime_interface]
pub trait AdditionalData {
	/// Read `key` from the relay/JAM chain state.
	///
	/// Returns the SCALE-encoding of `Option<Vec<u8>>`. On build the value is read live and its
	/// proof collected; on validation/import it is read back from — and verified against — the
	/// carried proof and the trusted root.
	///
	/// # Panics
	///
	/// If [`AdditionalDataExt`] is not registered in the externalities.
	fn read_relay_chain_state(
		&mut self,
		key: PassFatPointerAndRead<Vec<u8>>,
	) -> AllocateAndReturnByCodec<Vec<u8>> {
		self.extension::<AdditionalDataExt>()
			.expect(
				"AdditionalDataExt extension not registered; \
				 this host function is consensus-critical and cannot silently diverge",
			)
			.0
			.read(&key)
	}

	/// Finalize and return the hash of the map assembled from everything requested this block.
	///
	/// Returns `None` when nothing was requested — either the extension recorded no reads, or no
	/// extension is registered at all. Idempotent.
	///
	/// A missing extension is treated as "no reads": any relay read would have failed loudly in
	/// `read_relay_chain_state` (which *does* require the extension), so reaching `finalize`
	/// with no extension means no relay state was read and there is nothing to commit. This runs in
	/// `on_finalize` on every block — including blocks and test contexts that never read relay state
	/// — so it must not panic on a missing extension.
	fn finalize(&mut self) -> AllocateAndReturnByCodec<Option<[u8; 32]>> {
		self.extension::<AdditionalDataExt>().and_then(|ext| ext.0.finalize())
	}
}

// ── std-only provider implementations ──────────────────────────────────────────────────────────

#[cfg(feature = "std")]
mod std_impl {
	use super::{hash, AdditionalData, AdditionalDataProvider, RELAY_PROOF_KEY};
	use alloc::{collections::btree_map::BTreeMap, vec::Vec};
	use codec::{Decode, Encode};
	use hash_db::Hasher;
	use sp_state_machine::{Backend, TrieBackend, TrieBackendBuilder};
	use sp_trie::{recorder::Recorder, HashDBT, MemoryDB, StorageProof, EMPTY_PREFIX};

	/// Assemble the [`AdditionalData`] map from a recorded relay-read proof + root, or `None` when
	/// nothing was recorded. This is what both build and verify hash for `finalize`.
	fn relay_proof_map<H: Hasher>(recorder: &Recorder<H>, root: &H::Out) -> Option<AdditionalData>
	where
		H::Out: Ord + codec::Codec + Clone,
	{
		let proof = recorder.to_storage_proof();
		if proof.is_empty() {
			return None;
		}
		let mut map = BTreeMap::new();
		map.insert(RELAY_PROOF_KEY.into(), (root.clone(), proof).encode());
		Some(map)
	}

	/// Build a trie backend over the relay-read proof carried in `map[RELAY_PROOF_KEY]`, verifying
	/// it against `root`. Returns `None` if the key is absent, the value is malformed, or the proof
	/// does not contain `root`.
	fn backend_from_map<H: Hasher>(
		root: &H::Out,
		map: &AdditionalData,
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
		Some(TrieBackendBuilder::new(db, root.clone()).build())
	}


	/// Block-import / validation provider: reads relay/JAM keys back from the carried proof,
	/// authenticating each read (including proven absence) against `root`, and recording the nodes
	/// it accesses so [`Self::finalize`] can hash the map assembled from exactly what was requested.
	///
	/// On `validate_block` the `root` is the trusted `relay_parent_storage_root`; on generic import
	/// it is the root carried in the map (import trusts relay finality — the PVF is authoritative).
	pub struct VerifyingAdditionalDataProvider<H: Hasher> {
		root: H::Out,
		backend: TrieBackend<MemoryDB<H>, H>,
		recorder: Recorder<H>,
	}

	impl<H: Hasher> VerifyingAdditionalDataProvider<H>
	where
		H::Out: Ord + codec::Codec + Clone,
	{
		/// Construct from the carried `map`, verifying reads against the supplied trusted `root`
		/// (the map's own carried root is ignored). Used on `validate_block`.
		pub fn from_map_with_root(root: H::Out, map: AdditionalData) -> Option<Self> {
			let backend = backend_from_map::<H>(&root, &map)?;
			Some(Self { root, backend, recorder: Recorder::default() })
		}

		/// Construct from the carried `map`, trusting the root carried inside it. Used on generic
		/// import, which has no validation params.
		pub fn from_map(map: AdditionalData) -> Option<Self> {
			let proof_bytes = map.get(RELAY_PROOF_KEY)?;
			let (root, _proof) = <(H::Out, StorageProof)>::decode(&mut &proof_bytes[..]).ok()?;
			Self::from_map_with_root(root, map)
		}
	}

	impl<H> AdditionalDataProvider for VerifyingAdditionalDataProvider<H>
	where
		H: Hasher + Send + 'static,
		H::Out: Ord + codec::Codec + Send + Clone,
	{
		fn read(&self, key: &[u8]) -> Vec<u8> {
			// Record accessed nodes so `finalize` can reassemble exactly what was requested. An
			// `Err` here means the proof is missing nodes on the key's path (or does not verify
			// against `root`) — reject rather than treat as absent, else a collator could suppress a
			// present value by omitting its proof nodes. A *proven* absent key is `Ok(None)`.
			let recording = TrieBackendBuilder::wrap(&self.backend)
				.with_recorder(self.recorder.clone())
				.build();
			let value: Option<Vec<u8>> = recording
				.storage(key)
				.expect("relay-state read from an incomplete/invalid proof; candidate is invalid");
			value.encode()
		}

		fn finalize(&self) -> Option<[u8; 32]> {
			// Hash the map assembled from what was actually requested (recorded), NOT the carried
			// map. `frame_executive`'s digest-equality then rejects any candidate whose carried map
			// differs from what its execution requested.
			relay_proof_map(&self.recorder, &self.root).as_ref().map(hash)
		}

		fn proof_size(&self) -> usize {
			self.recorder.estimate_encoded_size()
		}
	}
}

#[cfg(feature = "std")]
pub use std_impl::VerifyingAdditionalDataProvider;
