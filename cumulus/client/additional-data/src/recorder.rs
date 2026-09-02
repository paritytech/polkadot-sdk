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

//! Build-side additional-data recorder.
//!
//! Wraps a relay-chain [`RelayStateProver`] (from `cumulus-relay-chain-interface`, which knows
//! nothing about additional data) and implements **both** the read side
//! ([`RelayStateReader`](cumulus_primitives_additional_data::RelayStateReader)) and the digest side
//! ([`AdditionalDataFinalizer`](sp_additional_data::AdditionalDataFinalizer)), so a single
//! `Arc`-shared instance backs both the `RelayStateExt` and `AdditionalDataExt` extensions. The
//! parachain runtime's `read_relay_chain_state` host function records into it while a block is
//! built; afterwards the collator pulls out the [`AdditionalData`] map to carry in the PoV.

use codec::Encode;
use cumulus_primitives_additional_data::{RelayStateReader, RELAY_PROOF_KEY};
use cumulus_relay_chain_interface::{RelayStateProver, TrieBackendProver};
use polkadot_primitives::{Block as PBlock, Hash as PHash};
use sc_client_api::StorageProof;
use sp_additional_data::{
	hash_value, AdditionalData, AdditionalDataFinalizer, AdditionalDataGetter,
};
use sp_runtime::traits::HashingFor;
use sp_state_machine::backend::AsTrieBackend;
use std::sync::Mutex;

/// The value carried under `RELAY_PROOF_KEY`: the SCALE-encoding of `(root, proof)`. `None` when
/// nothing was read.
fn recorded_value(root: PHash, proof: StorageProof) -> Option<Vec<u8>> {
	(!proof.is_empty()).then(|| (root, proof).encode())
}

/// Assemble the additional-data map from a recorded relay-read `proof` + `root`, or `None` when
/// nothing was read. A single `RELAY_PROOF_KEY` entry holding the encoded `(root, proof)`.
fn assemble(root: PHash, proof: StorageProof) -> Option<AdditionalData> {
	let value = recorded_value(root, proof)?;
	Some(core::iter::once((RELAY_PROOF_KEY.into(), value)).collect())
}

/// Records the relay-chain state a parachain runtime reads via `read_relay_chain_state` while a
/// block is built, serves those reads ([`RelayStateReader`]), commits the recorded proof
/// ([`AdditionalDataFinalizer`]), and assembles the [`AdditionalData`] map to carry in the PoV.
///
/// The live relay backend behind the [`RelayStateProver`] is only `Send` (its `RefCell`-based stats
/// are not `Sync`), so the prover is held behind a [`Mutex`], making the provider `Sync` — that
/// lets one `Arc<Self>` be registered under both `RelayStateExt` (reads) and `AdditionalDataExt`
/// (digest). The lock is uncontended (block building is single-threaded).
pub struct RecordingAdditionalDataProvider {
	prover: Mutex<Box<dyn RelayStateProver>>,
}

impl RecordingAdditionalDataProvider {
	/// Build from a relay-state prover, as returned by
	/// [`RelayChainInterface::relay_state_prover`](cumulus_relay_chain_interface::RelayChainInterface::relay_state_prover).
	pub fn new(prover: Box<dyn RelayStateProver>) -> Self {
		Self { prover: Mutex::new(prover) }
	}

	/// Convenience constructor over any trie-backed relay state (e.g. a proof-check backend in
	/// tests) — wraps it in a [`TrieBackendProver`].
	pub fn over_backend<S>(state: S) -> Self
	where
		S: AsTrieBackend<HashingFor<PBlock>> + Send + 'static,
	{
		Self::new(Box::new(TrieBackendProver::new(state)))
	}

	/// The current relay-state root and a backend-free proof snapshot, taken under the lock.
	fn root_and_snapshot(&self) -> (PHash, Box<dyn Fn() -> StorageProof + Send>) {
		let prover = self.prover.lock().expect("relay-state prover mutex poisoned");
		(prover.root(), prover.proof_snapshot())
	}

	/// A backend-free getter for the recorded [`AdditionalData`] map, to call *after* the block is
	/// built. Shares the prover's recorder (not its backend), so the closure stays `Send`. `None`
	/// when nothing was read.
	pub fn getter(&self) -> AdditionalDataGetter {
		let (root, snapshot) = self.root_and_snapshot();
		Box::new(move || assemble(root, snapshot()))
	}
}

impl RelayStateReader for RecordingAdditionalDataProvider {
	fn read(&self, key: &[u8]) -> Option<Vec<u8>> {
		// Build side reads live, full relay state. A read error here (e.g. pruned/unavailable relay
		// state) must NOT be silently degraded to proven-absence: that would record a proof that
		// cannot serve the read on validation, yielding a self-invalidating candidate. Fail the
		// block build loudly instead. (`Ok(None)` — a genuinely proven-absent key — is fine.)
		self.prover
			.lock()
			.expect("relay-state prover mutex poisoned")
			.read(key)
			.expect("relay-state read failed while building block; cannot record additional data")
	}

	fn proof_size(&self) -> usize {
		self.prover.lock().expect("relay-state prover mutex poisoned").proof_size()
	}
}

impl AdditionalDataFinalizer for RecordingAdditionalDataProvider {
	fn finalize(&self) -> Option<[u8; 32]> {
		let (root, snapshot) = self.root_and_snapshot();
		recorded_value(root, snapshot()).as_deref().map(hash_value)
	}
}
