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
	pass_by::{PassFatPointerAndRead, PassFatPointerAndWrite},
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
pub trait AdditionalDataProvider: Send {
	/// Read a relay/JAM storage `key`, returning its value or `None` when (provably) absent.
	///
	/// On build it reads the value live and collects the touched proof; on validation/import it
	/// reads the value back from the collected proof, authenticated against the trusted root.
	fn read(&self, key: &[u8]) -> Option<Vec<u8>>;

	/// Finalize and return the blake2 [`hash`] of the map assembled from exactly what was requested
	/// this block, or `None` when nothing was requested. MUST be idempotent.
	fn finalize(&self) -> Option<[u8; 32]>;

	/// Estimated encoded size of the proof recorded so far — the additional-data contribution to
	/// the PoV, summed into `storage_proof_size` so the runtime budgets for it. `0` when nothing
	/// was recorded.
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

/// Builds an [`AdditionalDataProvider`] from a block's carried [`AdditionalData`].
///
/// Injected into the block importer (via `ClientConfig`) so the concrete, producer-specific
/// verifier (e.g. the relay-state reader) can live outside substrate: the importer registers
/// whatever this factory returns instead of naming a concrete provider. Returns `None` when the
/// carried data is malformed / cannot be turned into a provider.
#[cfg(feature = "std")]
#[derive(Clone)]
pub struct AdditionalDataProviderFactory(
	pub  alloc::sync::Arc<
		dyn Fn(&AdditionalData) -> Option<Box<dyn AdditionalDataProvider>> + Send + Sync,
	>,
);

#[cfg(feature = "std")]
impl core::fmt::Debug for AdditionalDataProviderFactory {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.write_str("AdditionalDataProviderFactory")
	}
}

/// Runtime interface for reading relay/JAM chain state into a block's additional data.
///
/// `read_relay_chain_state` **panics** when [`AdditionalDataExt`] is not registered — the read is
/// consensus-critical (its proof feeds the header digest), so a missing extension must fail loudly
/// rather than silently diverge. `finalize` runs from `on_finalize` on every block and instead
/// returns `None` when the extension is absent (nothing was recorded).
#[runtime_interface]
pub trait AdditionalData {
	/// Read `key` from the relay/JAM chain state, writing the value into `value_out` and returning
	/// its full length, or `-1` when the key is (provably) absent.
	///
	/// Runtime-side-allocation compatible: the runtime owns `value_out`; this host function never
	/// allocates guest memory. Prefer the [`read_relay_chain_state`] wrapper, which reconstructs an
	/// `Option<Vec<u8>>` (resizing its buffer if the value is larger than `value_out`). On build
	/// the value is read live and its proof collected; on validation/import it is read back from —
	/// and verified against — the carried proof and the trusted root.
	///
	/// # Panics
	///
	/// If [`AdditionalDataExt`] is not registered in the externalities.
	#[raw_api]
	fn read_relay_chain_state_into(
		&mut self,
		key: PassFatPointerAndRead<&[u8]>,
		value_out: PassFatPointerAndWrite<&mut [u8]>,
	) -> i64 {
		let value = self
			.extension::<AdditionalDataExt>()
			.expect(
				"AdditionalDataExt extension not registered; \
				 this host function is consensus-critical and cannot silently diverge",
			)
			.0
			.read(key);
		match value {
			Some(v) => {
				let n = core::cmp::min(v.len(), value_out.len());
				value_out[..n].copy_from_slice(&v[..n]);
				v.len() as i64
			},
			None => -1,
		}
	}

	/// Read `key` from the relay/JAM chain state, returning its value or `None` when (provably)
	/// absent.
	///
	/// Ergonomic wrapper over [`read_relay_chain_state_into`] that owns the destination buffer
	/// runtime-side, resizing once if the value is larger than the initial guess.
	#[wrapper]
	fn read_relay_chain_state(key: impl AsRef<[u8]>) -> Option<Vec<u8>> {
		let mut buf = Vec::new();
		buf.resize(256, 0u8);
		let len = read_relay_chain_state_into__raw(key.as_ref(), &mut buf[..]);
		if len < 0 {
			return None;
		}
		let len = len as usize;
		if len > buf.len() {
			buf.resize(len, 0u8);
			read_relay_chain_state_into__raw(key.as_ref(), &mut buf[..]);
		}
		buf.truncate(len);
		Some(buf)
	}

	/// Finalize this block's additional data, writing the 32-byte digest hash into `hash_out` and
	/// returning `1`, or `0` when there is nothing to commit.
	///
	/// Runtime-side-allocation compatible (no guest allocation). Prefer the [`finalize`] wrapper,
	/// which reconstructs an `Option<[u8; 32]>`.
	///
	/// A missing extension is treated as "no reads": any relay read would have failed loudly in
	/// `read_relay_chain_state` (which *does* require the extension), so reaching `finalize` with
	/// no extension means no relay state was read and there is nothing to commit. This runs in
	/// `on_finalize` on every block — including blocks and test contexts that never read relay
	/// state — so it must not panic on a missing extension.
	#[raw_api]
	fn finalize_into(&mut self, hash_out: PassFatPointerAndWrite<&mut [u8]>) -> u32 {
		match self.extension::<AdditionalDataExt>().and_then(|ext| ext.0.finalize()) {
			Some(h) => {
				hash_out[..32].copy_from_slice(&h);
				1
			},
			None => 0,
		}
	}

	/// Finalize and return the hash of the map assembled from everything requested this block, or
	/// `None` when nothing was requested. Idempotent. Ergonomic wrapper over [`finalize_into`].
	#[wrapper]
	fn finalize() -> Option<[u8; 32]> {
		let mut out = [0u8; 32];
		if finalize_into__raw(&mut out[..]) == 1 {
			Some(out)
		} else {
			None
		}
	}
}
