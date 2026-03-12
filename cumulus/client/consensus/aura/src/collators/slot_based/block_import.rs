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
		// downwards the state changes.
		//
		// The following states are ignored:
		//  - `StateAction::ApplyChanges`: means that the node produced the block itself or the
		//    block was imported via state sync.
		//  - `StateAction::Skip`: means that the block should be skipped. This is evident in the
		//    context of gap-sync with collators running in non-archive modes. The state of the
		//    parent block has already been discarded and therefore any import would fail.
		if !self.sender.is_closed() &&
			!matches!(params.state_action, StateAction::ApplyChanges(_) | StateAction::Skip)
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
				.execute_block(parent_hash, block.clone().into())
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

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;
	use cumulus_test_client::{
		runtime::Block, DefaultTestClientBuilderExt, InitBlockBuilder, TestClientBuilder,
		TestClientBuilderExt,
	};
	use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
	use polkadot_primitives::HeadData;
	use sc_consensus::{BlockImportParams, ImportResult, StateAction};
	use sp_blockchain::HeaderBackend;
	use sp_consensus::BlockOrigin;

	fn sproof_with_best_parent(client: &cumulus_test_client::Client) -> RelayStateSproofBuilder {
		let best_hash = client.info().best_hash;
		let header = client.header(best_hash).ok().flatten().expect("No header for best block");
		let mut builder = RelayStateSproofBuilder::default();
		builder.para_id = cumulus_test_client::runtime::PARACHAIN_ID.into();
		builder.included_para_head = Some(HeadData(header.encode()));
		builder
	}

	/// Mock inner block import that always succeeds.
	#[derive(Clone)]
	struct MockBlockImport;

	#[async_trait::async_trait]
	impl BlockImport<Block> for MockBlockImport {
		type Error = sp_consensus::Error;

		async fn check_block(
			&self,
			_block: sc_consensus::BlockCheckParams<Block>,
		) -> Result<ImportResult, Self::Error> {
			Ok(ImportResult::imported(false))
		}

		async fn import_block(
			&self,
			_block: BlockImportParams<Block>,
		) -> Result<ImportResult, Self::Error> {
			Ok(ImportResult::imported(true))
		}
	}

	/// Regression test for the gap-sync infinite loop issue.
	///
	/// When a non-archive collator has a block gap of size 1, gap-sync downloads
	/// the block and marks it with `skip_execution: true` (which translates to
	/// `StateAction::Skip`). Before the fix, `SlotBasedBlockImport` would attempt
	/// to execute such blocks, fail with a consensus error ("State already
	/// discarded for parent"), and trigger a chain-sync restart that re-creates
	/// the same gap — leading to an infinite retry loop.
	///
	/// This test verifies that `StateAction::Skip` blocks are forwarded to the
	/// inner block import without attempting runtime execution.
	#[tokio::test]
	async fn gap_sync_block_with_skip_execution_does_not_attempt_runtime_call() {
		sp_tracing::try_init_simple();

		let client = Arc::new(TestClientBuilder::new().build());

		// Build a valid block so we have realistic headers/bodies.
		let sproof = sproof_with_best_parent(&client);
		let block_builder_data = client.init_block_builder(None, sproof);
		let block = block_builder_data.block_builder.build().unwrap().block;

		let (slot_based_import, mut handle) =
			SlotBasedBlockImport::new(MockBlockImport, client.clone());

		// Simulate the gap-sync scenario: a block arrives with StateAction::Skip
		// because the parent state has been pruned.
		let mut params = BlockImportParams::new(BlockOrigin::NetworkInitialSync, block.header);
		params.body = Some(block.extrinsics);
		params.state_action = StateAction::Skip;
		params.import_existing = true;

		// Before the fix, this would fail with a consensus error because
		// SlotBasedBlockImport would try to call `runtime_api.execute_block()`
		// on the parent hash whose state is no longer available.
		//
		// After the fix, StateAction::Skip is recognized and the block is
		// forwarded directly to the inner import without execution.
		let result = slot_based_import.import_block(params).await;
		assert!(result.is_ok(), "Gap-sync block with StateAction::Skip must not fail: {result:?}");

		// The channel must be empty — execution should have been skipped entirely,
		// so no (block, proof) was sent. This is the key assertion: without the
		// StateAction::Skip guard, execute_block() would run and send a message.
		//
		// Drop the sender side so the channel closes, then verify no message was queued.
		drop(slot_based_import);
		assert!(
			handle.receiver.next().await.is_none(),
			"No block+proof should be sent through the channel for StateAction::Skip"
		);
	}

	/// Verify that `StateAction::Execute` still triggers runtime execution.
	///
	/// This complements the gap-sync regression test by ensuring we did not
	/// accidentally disable execution for normal blocks.
	#[tokio::test]
	async fn normal_block_with_execute_action_triggers_runtime_execution() {
		sp_tracing::try_init_simple();

		let client = Arc::new(TestClientBuilder::new().build());

		let sproof = sproof_with_best_parent(&client);
		let block_builder_data = client.init_block_builder(None, sproof);
		let block = block_builder_data.block_builder.build().unwrap().block;

		let (slot_based_import, mut handle) =
			SlotBasedBlockImport::new(MockBlockImport, client.clone());

		// Normal import with StateAction::Execute should trigger execution
		// and send the block + proof through the channel.
		let mut params =
			BlockImportParams::new(BlockOrigin::NetworkInitialSync, block.header.clone());
		params.body = Some(block.extrinsics.clone());
		params.state_action = StateAction::Execute;

		let result = slot_based_import.import_block(params).await;
		assert!(result.is_ok(), "Normal block import should succeed: {result:?}");

		// The block and proof should have been sent through the channel,
		// confirming that execution actually happened.
		let (received_block, _proof) = handle.next().await;
		assert_eq!(*received_block.header(), block.header);
	}
}
