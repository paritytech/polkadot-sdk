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

use codec::Codec;
use cumulus_client_proof_size_recording::prepare_proof_size_recording_transaction;
use cumulus_primitives_core::{CoreInfo, CumulusDigestItem, RelayBlockIdentifier};
use futures::{stream::FusedStream, StreamExt};
use parking_lot::Mutex;
use sc_consensus::{BlockImport, StateAction};
use sc_consensus_aura::{find_pre_digest, standalone::fetch_authorities};
use sc_utils::mpsc::{tracing_unbounded, TracingUnboundedReceiver, TracingUnboundedSender};
use sp_api::{
	ApiExt, CallApiAt, CallContext, Core, ProofRecorder, ProofRecorderIgnoredNodes,
	ProvideRuntimeApi, StorageProof,
};
use sp_consensus::BlockOrigin;
use sp_consensus_aura::AuraApi;
use sp_runtime::traits::{Block as BlockT, HashingFor, Header as _};
use sp_trie::{
	proof_size_extension::{ProofSizeExt, RecordingProofSizeProvider},
	recorder::IgnoredNodes,
};
use std::{collections::HashMap, marker::PhantomData, sync::Arc};

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
	nodes_to_ignore: Arc<Mutex<HashMap<PoVBundle, ProofRecorderIgnoredNodes<Block>>>>,
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
			Self {
				sender,
				client,
				inner,
				nodes_to_ignore: Default::default(),
				_phantom: PhantomData,
			},
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
		Client: ProvideRuntimeApi<Block> + CallApiAt<Block> + Send + Sync,
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
		let authorities = fetch_authorities(&*self.client, *params.header.parent_hash())?;

		let pov_bundle = PoVBundle {
			author_index: *slot as usize % authorities.len(),
			core_info,
			relay_block_identifier,
		};

		let mut nodes_to_ignore = self.nodes_to_ignore.lock();
		let nodes_to_ignore = nodes_to_ignore.entry(pov_bundle).or_default();

		let recorder = ProofRecorder::<Block>::with_ignored_nodes(nodes_to_ignore.clone());
		let proof_size_recorder = RecordingProofSizeProvider::new(recorder.clone());

		let mut runtime_api = self.client.runtime_api();

		runtime_api.set_call_context(CallContext::Onchain);

		runtime_api.record_proof_with_recorder(recorder.clone());
		runtime_api.register_extension(ProofSizeExt::new(proof_size_recorder.clone()));

		let parent_hash = *params.header.parent_hash();

		let block = Block::new(params.header.clone(), params.body.clone().unwrap_or_default());

		runtime_api
			.execute_block(parent_hash, block)
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

		nodes_to_ignore
			.extend(IgnoredNodes::from_storage_proof::<HashingFor<Block>>(&storage_proof));
		nodes_to_ignore
			.extend(IgnoredNodes::from_memory_db(gen_storage_changes.transaction.clone()));

		// Extract and store proof size recordings
		let recorded_sizes = proof_size_recorder
			.recorded_estimations()
			.into_iter()
			.map(|size| size as u32)
			.collect::<Vec<u32>>();

		if !recorded_sizes.is_empty() {
			let block_hash = params.header.hash();
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
			nodes_to_ignore: self.nodes_to_ignore.clone(),
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
	Client: ProvideRuntimeApi<Block> + CallApiAt<Block> + Send + Sync,
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
