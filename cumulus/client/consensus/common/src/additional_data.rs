// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Build-side additional-data recorder.
//!
//! Bridges a relay-chain [`RelayStateProver`] (from `cumulus-relay-chain-interface`, which knows
//! nothing about additional data) to `sp-additional-data`'s [`AdditionalDataProvider`]. The
//! parachain runtime's `read_relay_chain_state` host function records into this while a block is
//! built; afterwards the collator pulls out the [`AdditionalData`] map to carry in the PoV.

use codec::Encode;
use cumulus_relay_chain_interface::{RelayStateProver, TrieBackendProver};
use polkadot_primitives::{Block as PBlock, Hash as PHash};
use sc_client_api::StorageProof;
use sp_additional_data::{
	hash, AdditionalData, AdditionalDataGetter, AdditionalDataProvider, RELAY_PROOF_KEY,
};
use sp_runtime::traits::HashingFor;
use sp_state_machine::backend::AsTrieBackend;

/// Assemble the additional-data map from a recorded relay-read `proof` + `root`, or `None` when
/// nothing was read. A single `RELAY_PROOF_KEY` entry holding the encoded `(root, proof)`.
fn assemble(root: PHash, proof: StorageProof) -> Option<AdditionalData> {
	if proof.is_empty() {
		return None;
	}
	let mut map = AdditionalData::new();
	map.insert(RELAY_PROOF_KEY.into(), (root, proof).encode());
	Some(map)
}

/// Records the relay-chain state a parachain runtime reads via `read_relay_chain_state` while a
/// block is built, and assembles the [`AdditionalData`] map to carry in the PoV.
///
/// The actual reading + minimal-proof recording lives in the wrapped [`RelayStateProver`]; this
/// type only adapts it to [`AdditionalDataProvider`] (for the host function) and assembles the map.
pub struct RecordingAdditionalDataProvider {
	prover: Box<dyn RelayStateProver>,
}

impl RecordingAdditionalDataProvider {
	/// Build from a relay-state prover, as returned by
	/// [`RelayChainInterface::relay_state_prover`](cumulus_relay_chain_interface::RelayChainInterface::relay_state_prover).
	pub fn new(prover: Box<dyn RelayStateProver>) -> Self {
		Self { prover }
	}

	/// Convenience constructor over any trie-backed relay state (e.g. a proof-check backend in
	/// tests) — wraps it in a [`TrieBackendProver`].
	pub fn over_backend<S>(state: S) -> Self
	where
		S: AsTrieBackend<HashingFor<PBlock>> + Send + 'static,
	{
		Self::new(Box::new(TrieBackendProver::new(state)))
	}

	/// A backend-free getter for the recorded [`AdditionalData`] map, to call *after* the block is
	/// built (once this provider has been moved into the `AdditionalDataExt` extension). Shares the
	/// prover's recorder, not its backend, so it stays `Send`. `None` when nothing was read.
	pub fn getter(&self) -> AdditionalDataGetter {
		let root = self.prover.root();
		let snapshot = self.prover.proof_snapshot();
		Box::new(move || assemble(root, snapshot()))
	}
}

impl AdditionalDataProvider for RecordingAdditionalDataProvider {
	fn read(&self, key: &[u8]) -> Vec<u8> {
		// Build side reads live, full relay state. A read error here (e.g. pruned/unavailable relay
		// state) must NOT be silently degraded to proven-absence: that would record a proof that
		// cannot serve the read on validation, yielding a self-invalidating candidate. Fail the
		// block build loudly instead. (`Ok(None)` — a genuinely proven-absent key — is fine.)
		let value = self
			.prover
			.read(key)
			.expect("relay-state read failed while building block; cannot record additional data");
		value.encode()
	}

	fn finalize(&self) -> Option<[u8; 32]> {
		assemble(self.prover.root(), self.prover.proof_snapshot()()).as_ref().map(hash)
	}

	fn proof_size(&self) -> usize {
		self.prover.proof_size()
	}
}
