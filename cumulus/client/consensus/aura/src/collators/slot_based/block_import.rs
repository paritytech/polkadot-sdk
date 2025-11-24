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

use codec::{Codec, Decode, Encode};
use cumulus_client_proof_size_recording::prepare_proof_size_recording_transaction;
use cumulus_primitives_core::{CoreInfo, CumulusDigestItem, RelayBlockIdentifier};
use futures::{stream::FusedStream, StreamExt};
use sc_client_api::{
	backend::AuxStore,
	client::{AuxDataOperations, FinalityNotification, PreCommitActions},
	HeaderBackend,
};
use sc_consensus::{BlockImport, StateAction};
use sc_consensus_aura::find_pre_digest;
use sc_utils::mpsc::{tracing_unbounded, TracingUnboundedReceiver, TracingUnboundedSender};
use sp_api::{
	ApiExt, CallApiAt, CallContext, Core, ProofRecorder, ProofRecorderIgnoredNodes,
	ProvideRuntimeApi, StorageProof,
};
use sp_blockchain::{Error as ClientError, Result as ClientResult};
use sp_consensus::BlockOrigin;
use sp_consensus_aura::AuraApi;
use sp_runtime::traits::{Block as BlockT, HashingFor, Header as _};
use sp_trie::{
	proof_size_extension::{ProofSizeExt, RecordingProofSizeProvider},
	recorder::IgnoredNodes,
};
use std::{marker::PhantomData, sync::Arc};

/// The aux storage key used to store the nodes to ignore for the given block hash.
fn nodes_to_ignore_key<H: Encode>(block_hash: H) -> Vec<u8> {
	(b"cumulus_slot_based_nodes_to_ignore", block_hash).encode()
}

fn load_decode<B, T>(backend: &B, key: &[u8]) -> ClientResult<Option<T>>
where
	B: AuxStore,
	T: Decode,
{
	let corrupt = |e: codec::Error| {
		ClientError::Backend(format!("Nodes to ignore DB is corrupted. Decode error: {}", e))
	};
	match backend.get_aux(key)? {
		None => Ok(None),
		Some(t) => T::decode(&mut &t[..]).map(Some).map_err(corrupt),
	}
}

/// Prepare a transaction to write the nodes to ignore to the aux storage.
///
/// Returns the key-value pairs that need to be written to the aux storage.
fn prepare_nodes_to_ignore_transaction<Block: BlockT>(
	block_hash: Block::Hash,
	ignored_nodes: IgnoredNodes<Block::Hash>,
) -> impl Iterator<Item = (Vec<u8>, Vec<u8>)> {
	let key = nodes_to_ignore_key(block_hash);
	let encoded_nodes = ignored_nodes.encode();

	[(key, encoded_nodes)].into_iter()
}

/// Load the nodes to ignore associated with a block and convert to IgnoredNodes.
fn load_nodes_to_ignore<Block: BlockT, B: AuxStore>(
	backend: &B,
	block_hash: Block::Hash,
) -> ClientResult<Option<IgnoredNodes<Block::Hash>>> {
	let nodes: Option<Vec<Vec<u8>>> =
		load_decode(backend, nodes_to_ignore_key(block_hash).as_slice())?;

	nodes.map(|n| IgnoredNodes::decode(&mut &n[..])).transpose().map_err(Into::into)
}

/// Handle for receiving the block and the storage proof from the [`SlotBasedBlockImport`].
///
/// This handle should be passed to [`Params`](super::Params) or can also be dropped if the node is
/// not running as collator.
pub struct SlotBasedBlockImportHandle<Block> {
	receiver: TracingUnboundedReceiver<(Block, StorageProof)>,
}

impl<Block> SlotBasedBlockImportHandle<Block> {
	/// Returns the next item.
	///
	/// The future will never return when the internal channel is closed.
	pub async fn next(&mut self) -> (Block, StorageProof) {
		loop {
			if self.receiver.is_terminated() {
				futures::pending!()
			} else if let Some(res) = self.receiver.next().await {
				return res
			}
		}
	}
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct PoVBundle {
	relay_block_identifier: RelayBlockIdentifier,
	core_info: CoreInfo,
	author_index: usize,
}

/// Special block import for the slot based collator.
pub struct SlotBasedBlockImport<Block: BlockT, BI, Client, AuthorityId> {
	inner: BI,
	client: Arc<Client>,
	sender: TracingUnboundedSender<(Block, StorageProof)>,
	_phantom: PhantomData<AuthorityId>,
}

impl<Block: BlockT, BI, Client, AuthorityId> SlotBasedBlockImport<Block, BI, Client, AuthorityId> {
	/// Create a new instance.
	///
	/// The returned [`SlotBasedBlockImportHandle`] needs to be passed to the
	/// [`Params`](super::Params), so that this block import instance can communicate with the
	/// collation task. If the node is not running as a collator, just dropping the handle is fine.
	pub fn new(inner: BI, client: Arc<Client>) -> (Self, SlotBasedBlockImportHandle<Block>) {
		let (sender, receiver) = tracing_unbounded("SlotBasedBlockImportChannel", 1000);

		(
			Self { sender, client, inner, _phantom: PhantomData },
			SlotBasedBlockImportHandle { receiver },
		)
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
		let relay_block_identifier =
			CumulusDigestItem::find_relay_block_identifier(params.header.digest());

		let (Some(core_info), Some(relay_block_identifier)) = (core_info, relay_block_identifier)
		else {
			return Ok(())
		};

		let slot = find_pre_digest::<Block, ()>(&params.header)
			.map_err(|error| sp_consensus::Error::Other(Box::new(error)))?;

		let parent_hash = *params.header.parent_hash();

		// Try to load nodes to ignore from parent block if both blocks belong to the same bundle
		let mut nodes_to_ignore = ProofRecorderIgnoredNodes::<Block>::default();
		let mut is_same_bundle = false;

		// Load parent block's header to check if it belongs to the same bundle
		if let Ok(Some(parent_header)) = self.client.header(parent_hash) {
			let parent_core_info = CumulusDigestItem::find_core_info(parent_header.digest());
			let parent_relay_block_identifier =
				CumulusDigestItem::find_relay_block_identifier(parent_header.digest());

			if let (Some(parent_core_info), Some(parent_relay_block_identifier)) =
				(parent_core_info, parent_relay_block_identifier)
			{
				if let Ok(parent_slot) = find_pre_digest::<Block, ()>(&parent_header) {
					let parent_pov_bundle = PoVBundle {
						author_index: *parent_slot as usize % authorities.len(),
						core_info: parent_core_info,
						relay_block_identifier: parent_relay_block_identifier,
					};

					// Only load nodes to ignore if both blocks are in the same bundle
					if parent_pov_bundle == pov_bundle {
						is_same_bundle = true;
						if let Ok(Some(parent_nodes)) =
							load_nodes_to_ignore::<Block, _>(&*self.client, parent_hash)
						{
							nodes_to_ignore = parent_nodes;
						}
					}
				}
			}
		}

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

		nodes_to_ignore.extend(IgnoredNodes::from_storage_proof(&storage_proof));
		nodes_to_ignore
			.extend(IgnoredNodes::from_memory_db(gen_storage_changes.transaction.clone()));

		let block_hash = params.header.hash();
		prepare_nodes_to_ignore_transaction::<Block>(block_hash, nodes_to_ignore).for_each(
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
		Self {
			inner: self.inner.clone(),
			client: self.client.clone(),
			sender: self.sender.clone(),
			_phantom: PhantomData,
		}
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

/// Cleanup auxiliary storage for finalized blocks.
///
/// This function removes nodes to ignore for blocks that are no longer needed
/// after finalization. It processes the finalized blocks and their stale heads to
/// determine which data can be safely removed.
fn aux_storage_cleanup<Block>(notification: &FinalityNotification<Block>) -> AuxDataOperations
where
	Block: BlockT,
{
	// Convert the hashes to deletion operations
	notification
		.stale_blocks
		.iter()
		.map(|b| (nodes_to_ignore_key(b.hash), None))
		.collect()
}

/// Register a finality action for cleaning up nodes to ignore.
///
/// This should be called during consensus initialization to automatically clean up
/// nodes to ignore when blocks are finalized.
pub fn register_nodes_to_ignore_cleanup<C, Block>(client: Arc<C>)
where
	C: PreCommitActions<Block> + 'static,
	Block: BlockT,
{
	let on_finality = move |notification: &FinalityNotification<Block>| -> AuxDataOperations {
		aux_storage_cleanup(notification)
	};

	client.register_finality_action(Box::new(on_finality));
}
