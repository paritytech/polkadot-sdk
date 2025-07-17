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

use futures::{stream::FusedStream, StreamExt};
use sc_consensus::{BlockImport, StateAction};
use sc_utils::mpsc::{tracing_unbounded, TracingUnboundedReceiver, TracingUnboundedSender};
use sp_api::{ApiExt, CallApiAt, CallContext, Core, ProvideRuntimeApi, StorageProof};
use sp_runtime::traits::{Block as BlockT, Header as _};
use sp_trie::proof_size_extension::ProofSizeExt;
use std::sync::Arc;

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

/// Special block import for the slot based collator.
pub struct SlotBasedBlockImport<Block, BI, Client> {
	inner: BI,
	client: Arc<Client>,
	sender: TracingUnboundedSender<(Block, StorageProof)>,
}

impl<Block, BI, Client> SlotBasedBlockImport<Block, BI, Client> {
	/// Create a new instance.
	///
	/// The returned [`SlotBasedBlockImportHandle`] needs to be passed to the
	/// [`Params`](super::Params), so that this block import instance can communicate with the
	/// collation task. If the node is not running as a collator, just dropping the handle is fine.
	pub fn new(inner: BI, client: Arc<Client>) -> (Self, SlotBasedBlockImportHandle<Block>) {
		let (sender, receiver) = tracing_unbounded("SlotBasedBlockImportChannel", 1000);

		(Self { sender, client, inner }, SlotBasedBlockImportHandle { receiver })
	}
}

impl<Block, BI: Clone, Client> Clone for SlotBasedBlockImport<Block, BI, Client> {
	fn clone(&self) -> Self {
		Self { inner: self.inner.clone(), client: self.client.clone(), sender: self.sender.clone() }
	}
}

#[async_trait::async_trait]
impl<Block, BI, Client> BlockImport<Block> for SlotBasedBlockImport<Block, BI, Client>
where
	Block: BlockT,
	BI: BlockImport<Block> + Send + Sync,
	BI::Error: Into<sp_consensus::Error>,
	Client: ProvideRuntimeApi<Block> + CallApiAt<Block> + Send + Sync,
	Client::StateBackend: Send,
	Client::Api: Core<Block>,
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
		// If the channel exists and it is required to execute the block, we will execute the block
		// here. This is done to collect the storage proof and to prevent re-execution, we push
		// downwards the state changes. `StateAction::ApplyChanges` is ignored, because it either
		// means that the node produced the block itself or the block was imported via state sync.
		if !self.sender.is_closed() && !matches!(params.state_action, StateAction::ApplyChanges(_))
		{
			let mut runtime_api = self.client.runtime_api();

			runtime_api.set_call_context(CallContext::Onchain);

			runtime_api.record_proof();
			let recorder = runtime_api
				.proof_recorder()
				.expect("Proof recording is enabled in the line above; qed.");
			runtime_api.register_extension(ProofSizeExt::new(recorder));

			let parent_hash = *params.header.parent_hash();

			let block = Block::new(params.header.clone(), params.body.clone().unwrap_or_default());

			runtime_api
				.execute_block(parent_hash, block.clone())
				.map_err(|e| Box::new(e) as Box<_>)?;

			let storage_proof =
				runtime_api.extract_proof().expect("Proof recording was enabled above; qed");

			let state = self.client.state_at(parent_hash).map_err(|e| Box::new(e) as Box<_>)?;
			let gen_storage_changes = runtime_api
				.into_storage_changes(&state, parent_hash)
				.map_err(sp_consensus::Error::ChainLookup)?;

			if params.header.state_root() != &gen_storage_changes.transaction_storage_root {
				return Err(sp_consensus::Error::Other(Box::new(
					sp_blockchain::Error::InvalidStateRoot,
				)))
			}

			params.state_action = StateAction::ApplyChanges(sc_consensus::StorageChanges::Changes(
				gen_storage_changes,
			));

			let _ = self.sender.unbounded_send((block, storage_proof));
		}

		self.inner.import_block(params).await.map_err(Into::into)
	}
}
