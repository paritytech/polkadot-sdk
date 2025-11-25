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

use crate::LOG_TARGET;
use codec::{Codec, Decode, Encode};
use cumulus_client_proof_size_recording::prepare_proof_size_recording_transaction;
use cumulus_primitives_core::{BundleInfo, CoreInfo, CumulusDigestItem, RelayBlockIdentifier};
use sc_client_api::{
	backend::AuxStore,
	client::{AuxDataOperations, FinalityNotification, PreCommitActions},
	HeaderBackend,
};
use sc_consensus::{BlockImport, StateAction};
use sp_api::{
	ApiExt, CallApiAt, CallContext, Core, ProofRecorder, ProofRecorderIgnoredNodes,
	ProvideRuntimeApi,
};
use sp_blockchain::{Error as ClientError, Result as ClientResult};
use sp_consensus::BlockOrigin;
use sp_consensus_aura::AuraApi;
use sp_runtime::traits::{Block as BlockT, HashingFor, Header as _};
use sp_trie::proof_size_extension::{ProofSizeExt, RecordingProofSizeProvider};
use std::{marker::PhantomData, sync::Arc};

/// The aux storage key used to store the ignored nodes for the given block hash.
fn ignored_nodes_key<H: Encode>(block_hash: H) -> Vec<u8> {
	(b"cumulus_slot_based_nodes_to_ignore", block_hash).encode()
}

/// Prepare a transaction to write the ignored nodes to the aux storage.
///
/// Returns the key-value pairs that need to be written to the aux storage.
fn prepare_ignored_nodes_transaction<Block: BlockT>(
	block_hash: Block::Hash,
	ignored_nodes: ProofRecorderIgnoredNodes<Block>,
) -> impl Iterator<Item = (Vec<u8>, Vec<u8>)> {
	let key = ignored_nodes_key(block_hash);
	let encoded_nodes = <ProofRecorderIgnoredNodes<Block> as Encode>::encode(&ignored_nodes);

	[(key, encoded_nodes)].into_iter()
}

/// Load the ignored nodes associated with a block.
fn load_ignored_nodes<Block: BlockT, B: AuxStore>(
	backend: &B,
	block_hash: Block::Hash,
) -> ClientResult<Option<ProofRecorderIgnoredNodes<Block>>> {
	match backend.get_aux(&ignored_nodes_key(block_hash))? {
		None => Ok(None),
		Some(t) => ProofRecorderIgnoredNodes::<Block>::decode(&mut &t[..]).map(Some).map_err(|e| {
			ClientError::Backend(format!("Nodes to ignore DB is corrupted. Decode error: {}", e))
		}),
	}
}

/// Register the clean up method for cleaning ignored nodes from blocks on which no further blocks
/// will be imported.
fn register_ignored_nodes_cleanup<C, Block>(client: Arc<C>)
where
	C: PreCommitActions<Block>,
	Block: BlockT,
{
	let on_finality = move |notification: &FinalityNotification<Block>| -> AuxDataOperations {
		notification
			.stale_blocks
			.iter()
			// Delete the ignored nodes for all stale blocks.
			.map(|b| (ignored_nodes_key(b.hash), None))
			// We can not delete the ignored nodes for the finalized block, because blocks can still
			// be imported on top of this block. As blocks are only finalized as bundles on the
			// relay chain, we should never need them, but better safe than sorry :)
			.chain(std::iter::once((ignored_nodes_key(*notification.header.parent_hash()), None)))
			.collect()
	};

	client.register_finality_action(Box::new(on_finality));
}

/// Special block import for the slot based collator.
pub struct SlotBasedBlockImport<Block: BlockT, BI, Client, AuthorityId> {
	inner: BI,
	client: Arc<Client>,
	_phantom: PhantomData<(AuthorityId, Block)>,
}

impl<Block: BlockT, BI, Client, AuthorityId> SlotBasedBlockImport<Block, BI, Client, AuthorityId> {
	/// Create a new instance.
	pub fn new(inner: BI, client: Arc<Client>) -> Self
	where
		Client: PreCommitActions<Block>,
	{
		register_ignored_nodes_cleanup(client.clone());

		Self { client, inner, _phantom: PhantomData }
	}

	/// Get the [`ProofRecorderIgnoredNodes`] for `parent`.
	///
	/// If `parent` was not part of the same block bundle, the [`ProofRecorderIgnoredNodes`] are not
	/// required and `None` will be returned.
	fn get_ignored_nodes(
		&self,
		parent: Block::Hash,
		core_info: &CoreInfo,
		bundle_info: &BundleInfo,
		relay_block_identifier: &RelayBlockIdentifier,
	) -> Option<ProofRecorderIgnoredNodes<Block>>
	where
		Client: AuxStore + HeaderBackend<Block> + Send + Sync,
	{
		let parent_header = self.client.header(parent).ok().flatten()?;
		let parent_core_info = CumulusDigestItem::find_core_info(parent_header.digest())?;
		let parent_bundle_info = CumulusDigestItem::find_bundle_info(parent_header.digest())?;
		let parent_relay_block_identifier =
			CumulusDigestItem::find_relay_block_identifier(parent_header.digest())?;

		if parent_relay_block_identifier != *relay_block_identifier {
			tracing::trace!(target: LOG_TARGET, ?parent_relay_block_identifier, ?relay_block_identifier, "Relay block identifier doesn't match");
			return None;
		}

		if parent_core_info != *core_info {
			tracing::trace!(target: LOG_TARGET, ?parent_core_info, ?core_info, "Core info doesn't match");
			return None
		}

		if parent_bundle_info.index.saturating_add(1) != bundle_info.index {
			tracing::trace!(target: LOG_TARGET, ?parent_bundle_info, ?bundle_info, "Block is not a child, based on the index");
			return None
		}

		match load_ignored_nodes::<Block, _>(&*self.client, parent) {
			Ok(nodes) => nodes,
			Err(error) => {
				tracing::trace!(target: LOG_TARGET, ?parent, ?error, "Failed to load `IgnoredNodes` from aux store");
				None
			},
		}
	}

	/// Execute the given block and collect the storage proof.
	///
	/// We need to execute the block on this level here, because we are collecting the storage
	/// proofs and combining them for blocks on the same core. So, blocks on the same core do not
	/// need to include the same trie nodes multiple times and thus, not wasting storage proof size.
	fn execute_block_and_collect_storage_proof(
		&self,
		params: &mut sc_consensus::BlockImportParams<Block>,
	) -> Result<(), sp_consensus::Error>
	where
		Client: ProvideRuntimeApi<Block>
			+ CallApiAt<Block>
			+ AuxStore
			+ HeaderBackend<Block>
			+ Send
			+ Sync,
		Client::StateBackend: Send,
		Client::Api: Core<Block> + AuraApi<Block, AuthorityId>,
		AuthorityId: Codec + Send + Sync + std::fmt::Debug,
	{
		let core_info = CumulusDigestItem::find_core_info(params.header.digest());
		let bundle_info = CumulusDigestItem::find_bundle_info(params.header.digest());
		let relay_block_identifier =
			CumulusDigestItem::find_relay_block_identifier(params.header.digest());

		let (Some(core_info), Some(bundle_info), Some(relay_block_identifier)) =
			(core_info, bundle_info, relay_block_identifier)
		else {
			return Ok(())
		};

		let parent_hash = *params.header.parent_hash();

		let mut nodes_to_ignore = self
			.get_ignored_nodes(parent_hash, &core_info, &bundle_info, &relay_block_identifier)
			.unwrap_or_default();

		let recorder = ProofRecorder::<Block>::with_ignored_nodes(nodes_to_ignore.clone());
		let proof_size_recorder = RecordingProofSizeProvider::new(recorder.clone());

		let mut runtime_api = self.client.runtime_api();

		runtime_api.set_call_context(CallContext::Onchain);
		runtime_api.record_proof_with_recorder(recorder.clone());
		runtime_api.register_extension(ProofSizeExt::new(proof_size_recorder.clone()));

		let block = Block::new(params.header.clone(), params.body.clone().unwrap_or_default());

		runtime_api
			.execute_block(parent_hash, block.clone().into())
			.map_err(|e| Box::new(e) as Box<_>)?;

		let storage_proof =
			runtime_api.extract_proof().expect("Proof recording was enabled above; qed");

		let state = self.client.state_at(parent_hash).map_err(|e| Box::new(e) as Box<_>)?;
		let gen_storage_changes = runtime_api
			.into_storage_changes(&state, parent_hash)
			.map_err(sp_consensus::Error::ChainLookup)?;

		if params.header.state_root() != &gen_storage_changes.transaction_storage_root {
			return Err(sp_consensus::Error::Other(Box::new(sp_blockchain::Error::InvalidStateRoot)))
		}

		nodes_to_ignore.extend(ProofRecorderIgnoredNodes::<Block>::from_storage_proof::<
			HashingFor<Block>,
		>(&storage_proof));
		nodes_to_ignore.extend(ProofRecorderIgnoredNodes::<Block>::from_memory_db(
			gen_storage_changes.transaction.clone(),
		));

		let block_hash = params.post_hash();
		prepare_ignored_nodes_transaction::<Block>(block_hash, nodes_to_ignore).for_each(
			|(k, v)| {
				params.auxiliary.push((k, Some(v)));
			},
		);

		// Extract and store proof size recordings
		let recorded_sizes = proof_size_recorder
			.recorded_estimations()
			.into_iter()
			.map(|size| size as u32)
			.collect::<Vec<u32>>();

		if !recorded_sizes.is_empty() {
			prepare_proof_size_recording_transaction(block_hash, recorded_sizes).for_each(
				|(k, v)| {
					params.auxiliary.push((k, Some(v)));
				},
			);
		}

		params.state_action =
			StateAction::ApplyChanges(sc_consensus::StorageChanges::Changes(gen_storage_changes));

		Ok(())
	}
}

impl<Block: BlockT, BI: Clone, Client, AuthorityId> Clone
	for SlotBasedBlockImport<Block, BI, Client, AuthorityId>
{
	fn clone(&self) -> Self {
		Self { inner: self.inner.clone(), client: self.client.clone(), _phantom: PhantomData }
	}
}

#[async_trait::async_trait]
impl<Block, BI, Client, AuthorityId> BlockImport<Block>
	for SlotBasedBlockImport<Block, BI, Client, AuthorityId>
where
	Block: BlockT,
	BI: BlockImport<Block> + Send + Sync,
	BI::Error: Into<sp_consensus::Error>,
	Client:
		ProvideRuntimeApi<Block> + CallApiAt<Block> + AuxStore + HeaderBackend<Block> + Send + Sync,
	Client::StateBackend: Send,
	Client::Api: Core<Block> + AuraApi<Block, AuthorityId>,
	AuthorityId: Codec + Send + Sync + std::fmt::Debug,
{
	type Error = sp_consensus::Error;

	async fn check_block(
		&self,
		block: sc_consensus::BlockCheckParams<Block>,
	) -> Result<sc_consensus::ImportResult, Self::Error> {
		self.inner.check_block(block).await.map_err(Into::into)
	}

	async fn import_block(
		&self,
		mut params: sc_consensus::BlockImportParams<Block>,
	) -> Result<sc_consensus::ImportResult, Self::Error> {
		if params.origin != BlockOrigin::Own {
			self.execute_block_and_collect_storage_proof(&mut params)?;
		}

		self.inner.import_block(params).await.map_err(Into::into)
	}
}
