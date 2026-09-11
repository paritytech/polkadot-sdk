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

//! JAM block import: re-execute imported blocks with the carried JAM state proof registered.
//!
//! The relay collator's `SlotBasedBlockImport` re-executes bundle blocks at the tip and, because
//! that path bypasses the client's importing branch, registers the relay-read extensions itself
//! (`cumulus/client/consensus/aura/src/collators/slot_based/block_import.rs:266-288`). The JAM
//! collator has no bundle digests to gate on, so this wrapper re-executes every block that reaches
//! it needing execution — and registers the JAM reader the runtime's `jam_state_read` host function
//! dispatches through, plus the matching finalizer, so `frame_executive` recomputes the
//! `DigestItem::AdditionalData` over the carried proof.

use codec::Decode;
use cumulus_primitives_additional_data::{JamProofReader, JamStateExt, JAM_PROOF_KEY};
use jam_state_helpers::StateProof;
use sc_client_api::backend::AuxStore;
use sc_consensus::{BlockImport, BlockImportParams, ImportResult, StateAction};
use sp_additional_data::{AdditionalData, AdditionalDataExt, AdditionalDataFinalizer};
use sp_api::{ApiExt, CallApiAt, CallContext, Core, ProvideRuntimeApi};
use sp_consensus::BlockOrigin;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use std::{marker::PhantomData, sync::Arc};

use super::JamProofFinalizer;

/// The parachain-service id whose state the carried proof reads — the same value the runtime's
/// `validate_block` uses, so both sides derive identical 31-byte state keys. A mismatch here
/// makes every read derive the wrong key and panic.
const PARACHAIN_SERVICE_ID: u32 = 5;

/// Turn the carried additional-data map into the JAM reader and finalizer re-execution needs.
///
/// Decodes the `JAM_PROOF_KEY` entry — the SCALE-encoding of `(state_root, StateProof)` — and, as
/// the relay import path does, trusts the root carried inside it: JAM finality, and the PVF is
/// the authoritative validator. `None` when the entry is missing or malformed; the relay path
/// fails the import the same way, because a block that reads JAM state but carries no usable
/// proof is invalid.
fn jam_import_reader(map: &AdditionalData) -> Option<(JamProofReader, JamProofFinalizer)> {
	let entry = map.get(JAM_PROOF_KEY)?;
	let (state_root, proof) = <([u8; 32], StateProof)>::decode(&mut &entry[..]).ok()?;
	let reader = JamProofReader::new(PARACHAIN_SERVICE_ID, state_root, proof);
	let finalizer = JamProofFinalizer { commitment: sp_additional_data::hash_value(entry) };
	Some((reader, finalizer))
}

/// Block import that replays the carried JAM state proof on import, so a block that reads JAM
/// state re-executes without trapping and its additional-data digest recomputes to a match.
pub(crate) struct JamBlockImport<Block: BlockT, BI, Client> {
	inner: BI,
	client: Arc<Client>,
	_block: PhantomData<Block>,
}

impl<Block: BlockT, BI, Client> JamBlockImport<Block, BI, Client> {
	/// Create a new instance wrapping `inner`.
	pub fn new(inner: BI, client: Arc<Client>) -> Self {
		Self { inner, client, _block: PhantomData }
	}
}

impl<Block: BlockT, BI: Clone, Client> Clone for JamBlockImport<Block, BI, Client> {
	fn clone(&self) -> Self {
		Self { inner: self.inner.clone(), client: self.client.clone(), _block: PhantomData }
	}
}

#[async_trait::async_trait]
impl<Block, BI, Client> BlockImport<Block> for JamBlockImport<Block, BI, Client>
where
	Block: BlockT,
	BI: BlockImport<Block> + Send + Sync,
	BI::Error: Into<sp_consensus::Error>,
	Client: ProvideRuntimeApi<Block>
		+ CallApiAt<Block>
		+ AuxStore
		+ sc_client_api::HeaderBackend<Block>
		+ Send
		+ Sync,
	Client::StateBackend: Send,
	Client::Api: Core<Block>,
{
	type Error = sp_consensus::Error;

	async fn check_block(
		&self,
		block: sc_consensus::BlockCheckParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		self.inner.check_block(block).await.map_err(Into::into)
	}

	async fn import_block(
		&self,
		mut params: BlockImportParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		let origin = params.origin;

		if !(origin == BlockOrigin::Own ||
			params.with_state() ||
			params.state_action.skip_execution_checks())
		{
			self.replay_jam_state(&mut params).await?;
		}

		self.inner.import_block(params).await.map_err(Into::into)
	}
}

impl<Block, BI, Client> JamBlockImport<Block, BI, Client>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block>
		+ CallApiAt<Block>
		+ AuxStore
		+ sc_client_api::HeaderBackend<Block>
		+ Send
		+ Sync,
	Client::StateBackend: Send,
	Client::Api: Core<Block>,
{
	/// Re-execute the block with the carried JAM proof registered, then hand the recomputed
	/// changes back as `ApplyChanges` so the inner import applies rather than executes again.
	///
	/// Mirrors the relay `SlotBasedBlockImport` re-execution: the generic importing branch is
	/// bypassed (we set `StateAction::ApplyChanges`), so the extensions must be registered here.
	/// The state-root check is the digest check — a tampered carried proof recomputes a different
	/// `DigestItem::AdditionalData` and therefore a different root.
	async fn replay_jam_state(
		&self,
		params: &mut BlockImportParams<Block>,
	) -> Result<(), sp_consensus::Error> {
		let parent_hash = *params.header.parent_hash();
		let body = params.body.clone().unwrap_or_default();

		let mut runtime_api = self.client.runtime_api();
		runtime_api.set_call_context(CallContext::Onchain { import: true });

		// A block that reads JAM state but carries no additional-data blob is invalid: with
		// nothing registered, `jam_state_read` panics on the missing `JamStateExt` and the
		// re-execution fails. With a blob, a missing or malformed `JAM_PROOF_KEY` entry fails the
		// import outright, exactly as the relay path refuses a map it cannot turn into a
		// relay-read provider.
		if let Some(blob) = params.additional_data.as_ref() {
			let (reader, finalizer) = jam_import_reader(blob).ok_or_else(|| {
				sp_consensus::Error::Other(
					"additional_data map could not be built into a JAM-state reader".into(),
				)
			})?;
			let reader = Arc::new(reader);
			// One reader serves the read host function (`JamStateExt`) and the digest
			// (`AdditionalDataExt`, keyed by `JAM_PROOF_KEY`), so `frame_executive` deposits a
			// `DigestItem::AdditionalData` committing exactly the carried entry.
			runtime_api.register_extension(JamStateExt(Box::new(reader)));
			runtime_api.register_extension(AdditionalDataExt(
				[(
					JAM_PROOF_KEY.to_string(),
					Box::new(finalizer) as Box<dyn AdditionalDataFinalizer>,
				)]
				.into(),
			));
		}

		runtime_api
			.execute_block(parent_hash, Block::new(params.header.clone(), body).into())
			.map_err(|e| Box::new(e) as Box<_>)?;

		let state = self.client.state_at(parent_hash).map_err(|e| Box::new(e) as Box<_>)?;
		let gen_storage_changes = runtime_api
			.into_storage_changes(&state, parent_hash)
			.map_err(sp_consensus::Error::ChainLookup)?;

		if params.header.state_root() != &gen_storage_changes.transaction_storage_root {
			return Err(sp_consensus::Error::Other(Box::new(
				sp_blockchain::Error::InvalidStateRoot,
			)));
		}

		params.state_action =
			StateAction::ApplyChanges(sc_consensus::StorageChanges::Changes(gen_storage_changes));

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;
	use cumulus_primitives_additional_data::JamStateReader;
	use jam_state_helpers::{blake2_256, service_value_state_key, Hash, ProofNode};
	use sp_additional_data::{hash_commitments, hash_value, AdditionalData};

	/// A fixed service-local key standing in for `para_info_key(para_id)`; the reader derives the
	/// 31-byte state key from it via `service_value_state_key`.
	const HEAD_KEY: &[u8] = b"para-head";

	/// The SCALE-encoding of the service's `ParaInfo` entry for a 32-byte head — a large leaf
	/// (value over 32 bytes), as on the real service.
	fn para_info_value(head: &[u8]) -> Vec<u8> {
		let mut value = codec::Compact::<u32>(head.len() as u32).encode();
		value.extend(head);
		// `None` validation code, `None` pending upgrade, compact(0) balance, compact(0) used,
		// `false` — five zero bytes after the head.
		value.extend([0u8; 5]);
		value
	}

	/// A single-key large-leaf proof of `value` under the derived `HEAD_KEY` state key, the shape
	/// task 9's authoring carries. Returns the `(state_root, proof)` pair the `JAM_PROOF_KEY`
	/// entry is the SCALE-encoding of.
	fn head_proof() -> (Hash, StateProof) {
		let value = para_info_value(&[7u8; 32]);
		let state_key = service_value_state_key(PARACHAIN_SERVICE_ID, HEAD_KEY);
		let mut node = [0u8; 64];
		node[0] = 0b1100_0000;
		node[1..32].copy_from_slice(&state_key);
		node[32..].copy_from_slice(&blake2_256(&value));
		let state_root = blake2_256(&node);
		let proof = StateProof {
			nodes: vec![ProofNode::from(node)],
			values: vec![(state_key, value.clone())],
		};
		(state_root, proof)
	}

	/// The `JAM_PROOF_KEY` entry an authored block carries: SCALE `(state_root, proof)`.
	fn jam_map(proof: &StateProof, state_root: Hash) -> AdditionalData {
		[(JAM_PROOF_KEY.to_string(), (state_root, proof).encode())].into()
	}

	/// A round-tripped authored block: the carried `JAM_PROOF_KEY` entry builds the reader the
	/// runtime's `jam_state_read` dispatches through — serving the read that would otherwise trap
	/// on the missing `JamStateExt` — and the finalizer recomputes the same
	/// `DigestItem::AdditionalData` the build committed.
	#[test]
	fn import_recomputes_the_authored_digest_and_serves_the_read() {
		let (state_root, proof) = head_proof();
		let map = jam_map(&proof, state_root);
		let entry = map.get(JAM_PROOF_KEY).expect("the entry is carried");

		let (reader, finalizer) = jam_import_reader(&map).expect("a well-formed entry builds");

		// The read the runtime makes during re-execution resolves through the reader (no trap).
		let read = reader.read(HEAD_KEY).expect("the reader serves the read");
		assert_eq!(read, para_info_value(&[7u8; 32]));

		// The individual finalizer commits hash_value of the carried entry — and the
		// `AdditionalDataExt` the import path registers folds it, via the same `hash_commitments`
		// `frame_executive` uses, into the authored `DigestItem::AdditionalData`.
		assert_eq!(finalizer.finalize(), Some(hash_value(entry)));
		let digest = AdditionalDataExt(
			[(JAM_PROOF_KEY.to_string(), Box::new(finalizer) as Box<dyn AdditionalDataFinalizer>)]
				.into(),
		)
		.finalize();
		assert_eq!(
			digest,
			hash_commitments(core::iter::once(hash_value(entry))),
			"the recomputed digest matches the authored DigestItem::AdditionalData",
		);
	}

	/// A tampered carried proof — a node removed, so the read cannot authenticate `HEAD_KEY` —
	/// must be rejected, not silently accepted. The reader is built from the (still-decoding)
	/// entry, and the first read panics (task 7's semantic), which fails the re-execution.
	#[test]
	fn a_tampered_carried_proof_is_rejected() {
		let (state_root, proof) = head_proof();
		let mut tampered = proof.clone();
		tampered.nodes.clear();
		let map = jam_map(&tampered, state_root);

		let (reader, _) = jam_import_reader(&map).expect("a tampered proof still decodes");

		let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
			let _ = reader.read(HEAD_KEY);
		}));
		assert!(result.is_err(), "a removed proof node must panic the read, never read as absent");
	}

	/// A missing or malformed `JAM_PROOF_KEY` entry builds no reader — the import errors out,
	/// mirroring the relay path refusing a map it cannot turn into a provider.
	#[test]
	fn a_missing_or_malformed_entry_builds_no_reader() {
		assert!(jam_import_reader(&AdditionalData::default()).is_none(), "no entry at all");

		let garbage = [("not-jam".to_string(), vec![1u8, 2, 3])].into();
		assert!(jam_import_reader(&garbage).is_none(), "no JAM_PROOF_KEY key");

		let malformed = [(JAM_PROOF_KEY.to_string(), vec![0xABu8; 7])].into();
		assert!(jam_import_reader(&malformed).is_none(), "entry does not decode as (root, proof)",);
	}
}
