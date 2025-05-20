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
use futures::{stream::FusedStream, StreamExt};
use sc_consensus::{BlockImport, StateAction};
use sc_consensus_aura::CompatibilityMode;
use sc_consensus_slots::InherentDataProviderExt;
use sc_utils::mpsc::{tracing_unbounded, TracingUnboundedReceiver, TracingUnboundedSender};
use sp_api::{ApiExt, CallApiAt, CallContext, Core, ProvideRuntimeApi, StorageProof};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_consensus_aura::AuraApi;
use sp_core::Pair;
use sp_runtime::traits::{Block as BlockT, Header as _, NumberFor};
use sp_trie::proof_size_extension::ProofSizeExt;
use std::sync::Arc;

use crate::collators::{validate_block_import, CheckForEquivocation};

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
pub struct SlotBasedBlockImport<Block, BI, Client, CIDP, P, N> {
	inner: BI,
	client: Arc<Client>,
	sender: TracingUnboundedSender<(Block, StorageProof)>,
	create_inherent_data_providers: CIDP,
	check_for_equivocation: CheckForEquivocation,
	compatibility_mode: CompatibilityMode<N>,
	_phantom: std::marker::PhantomData<P>,
}

impl<Block, BI, Client, CIDP, P, N> SlotBasedBlockImport<Block, BI, Client, CIDP, P, N> {
	/// Create a new instance.
	///
	/// The returned [`SlotBasedBlockImportHandle`] needs to be passed to the
	/// [`Params`](super::Params), so that this block import instance can communicate with the
	/// collation task. If the node is not running as a collator, just dropping the handle is fine.
	pub fn new(
		inner: BI,
		client: Arc<Client>,
		create_inherent_data_providers: CIDP,
		check_for_equivocation: CheckForEquivocation,
		compatibility_mode: CompatibilityMode<N>,
	) -> (Self, SlotBasedBlockImportHandle<Block>) {
		let (sender, receiver) = tracing_unbounded("SlotBasedBlockImportChannel", 1000);

		(
			Self {
				sender,
				client,
				inner,
				create_inherent_data_providers,
				check_for_equivocation,
				compatibility_mode,
				_phantom: Default::default(),
			},
			SlotBasedBlockImportHandle { receiver },
		)
	}
}

impl<Block, BI: Clone, Client, CIDP, P, N> Clone
	for SlotBasedBlockImport<Block, BI, Client, CIDP, P, N>
where
	CIDP: Clone,
	N: Clone,
{
	fn clone(&self) -> Self {
		Self {
			inner: self.inner.clone(),
			client: self.client.clone(),
			sender: self.sender.clone(),
			create_inherent_data_providers: self.create_inherent_data_providers.clone(),
			check_for_equivocation: self.check_for_equivocation,
			compatibility_mode: self.compatibility_mode.clone(),
			_phantom: Default::default(),
		}
	}
}

#[async_trait::async_trait]
impl<Block, BI, Client, CIDP, P> BlockImport<Block>
	for SlotBasedBlockImport<Block, BI, Client, CIDP, P, NumberFor<Block>>
where
	Block: BlockT,
	BI: BlockImport<Block> + Send + Sync,
	BI::Error: Into<sp_consensus::Error>,
	Client: ProvideRuntimeApi<Block>
		+ CallApiAt<Block>
		+ sc_client_api::backend::AuxStore
		+ Send
		+ Sync,
	Client::StateBackend: Send,
	Client::Api: Core<Block> + BlockBuilderApi<Block> + AuraApi<Block, <P as Pair>::Public>,
	P: Pair + Sync,
	P::Public: Codec + std::fmt::Debug,
	P::Signature: Codec,
	CIDP: sp_inherents::CreateInherentDataProviders<Block, ()> + Send,
	CIDP::InherentDataProviders: InherentDataProviderExt + Send + Sync,
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
			validate_block_import::<_, _, P, _>(
				&mut params,
				self.client.as_ref(),
				&self.create_inherent_data_providers,
				self.check_for_equivocation,
				&self.compatibility_mode,
			)
			.await?;

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
