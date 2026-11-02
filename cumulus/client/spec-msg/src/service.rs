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

//! The receiver's ties to the own chain and the relay chain.
//!
//! - [`run_receiver`]: per relay best block, pass each source's newest included root to
//!   [`Receiver::on_root`] until it settles; note own blocks; prune on finality.
//! - [`inherent_data`] and [`assembler`]: the inherent provider and the collator's
//!   [`SpecMsgAssembler`].
//!
//! Idle without `SpecMsgApi` or `ParachainHost` v17.

use crate::{
	fetch::Transport,
	receiver::{ChainView, Receiver},
	relay::{newest_included_provides, relay_has_api},
	LOG_TARGET,
};
use cumulus_client_collator::service::{SpecMsgAssembler, SpecMsgInputs};
use cumulus_primitives_core::SpecMsgApi;
use cumulus_primitives_spec_messaging::{ConsumptionRecord, MessagingInherentData};
use cumulus_relay_chain_interface::{PHash, RelayChainInterface};
use futures::{select, StreamExt};
use sc_client_api::BlockchainEvents;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_runtime::{
	traits::{Block as BlockT, Header as HeaderT},
	SaturatedConversion,
};
use std::sync::Arc;

/// The consumption view at `at`, or `None` if its runtime has no `SpecMsgApi`.
pub fn chain_view<Block, C>(client: &C, at: Block::Hash) -> Option<ChainView>
where
	Block: BlockT,
	C: ProvideRuntimeApi<Block>,
	C::Api: SpecMsgApi<Block>,
{
	let api = client.runtime_api();
	if !api.has_api::<dyn SpecMsgApi<Block>>(at).ok()? {
		return None;
	}
	let consumed = api
		.consumed_streams(at)
		.map_err(|e| log::warn!(target: LOG_TARGET, "consumed_streams at {at:?} failed: {e}"))
		.ok()?;
	let max_touched = api
		.max_touched_streams(at)
		.map_err(|e| log::warn!(target: LOG_TARGET, "max_touched_streams at {at:?} failed: {e}"))
		.ok()?;
	Some(ChainView::new(consumed, max_touched))
}

/// The consumption record of block `at`; empty if its runtime has no `SpecMsgApi`.
pub fn consumption_record<Block, C>(
	client: &C,
	at: Block::Hash,
) -> Result<ConsumptionRecord, String>
where
	Block: BlockT,
	C: ProvideRuntimeApi<Block>,
	C::Api: SpecMsgApi<Block>,
{
	let api = client.runtime_api();
	if !api.has_api::<dyn SpecMsgApi<Block>>(at).map_err(|e| e.to_string())? {
		return Ok(ConsumptionRecord::default());
	}
	api.consumption_record(at).map_err(|e| e.to_string())
}

/// Keep `receiver` fed from `client` and `relay` until a notification stream ends.
pub async fn run_receiver<Block, C, T>(
	client: Arc<C>,
	relay: Arc<dyn RelayChainInterface>,
	receiver: Arc<Receiver<Block::Hash, T>>,
) where
	Block: BlockT,
	C: ProvideRuntimeApi<Block> + BlockchainEvents<Block> + HeaderBackend<Block> + Send + Sync,
	C::Api: SpecMsgApi<Block>,
	T: Transport + ?Sized,
{
	let mut relay_heads = match relay.new_best_notification_stream().await {
		Ok(stream) => stream.fuse(),
		Err(e) => {
			log::error!(target: LOG_TARGET, "No relay best-block stream; receiver stopped: {e}");
			return;
		},
	};
	let mut imports = client.import_notification_stream().fuse();
	let mut finality = client.finality_notification_stream().fuse();
	loop {
		select! {
			head = relay_heads.next() => match head {
				Some(head) => on_relay_block(&*client, &*relay, &receiver, head.hash()).await,
				None => return,
			},
			import = imports.next() => match import {
				Some(import) => {
					let header = import.header;
					match consumption_record(&*client, import.hash) {
						Ok(record) => {
							let number = (*header.number()).saturated_into();
							let parent = *header.parent_hash();
							receiver.on_block(import.hash, parent, number, record).await;
						},
						Err(e) => log::warn!(
							target: LOG_TARGET,
							"consumption_record at {:?} failed: {e}",
							import.hash,
						),
					}
				},
				None => return,
			},
			finalized = finality.next() => match finalized {
				Some(finalized) => {
					let view = chain_view(&*client, finalized.hash).unwrap_or_default();
					receiver.on_finalized((*finalized.header.number()).saturated_into(), &view);
				},
				None => return,
			},
		}
	}
}

/// Fetch under every source root that changed as of relay block `relay_hash`.
async fn on_relay_block<Block, C, T>(
	client: &C,
	relay: &dyn RelayChainInterface,
	receiver: &Receiver<Block::Hash, T>,
	relay_hash: PHash,
) where
	Block: BlockT,
	C: ProvideRuntimeApi<Block> + HeaderBackend<Block>,
	C::Api: SpecMsgApi<Block>,
	T: Transport + ?Sized,
{
	let Some(view) = chain_view(client, client.info().best_hash) else {
		return;
	};
	if view.sources.is_empty() || !relay_has_api(relay, relay_hash).await {
		return;
	}
	let updates = view.sources.iter().map(|(source, wants)| async move {
		let root = newest_included_provides(relay, relay_hash, *source).await?;
		receiver.on_root(*source, root, wants).await;
		Some(())
	});
	futures::future::join_all(updates).await;
}

/// The messaging inherent for a block built on `parent`. Empty when `parent`'s runtime has no
/// `SpecMsgApi`; an empty inherent places nothing.
pub async fn inherent_data<Block, C, T>(
	client: &C,
	receiver: &Receiver<Block::Hash, T>,
	parent: Block::Hash,
) -> MessagingInherentData
where
	Block: BlockT,
	C: ProvideRuntimeApi<Block>,
	C::Api: SpecMsgApi<Block>,
	T: Transport + ?Sized,
{
	match chain_view(client, parent) {
		Some(view) => receiver.inherent_data(&view).await,
		None => MessagingInherentData::default(),
	}
}

/// The collator's assembler: each block's consumption record, and the lifts for them from the
/// receiver's pool. An error skips the collation; the next attempt assembles again.
pub fn assembler<Block, C, T>(
	client: Arc<C>,
	receiver: Arc<Receiver<Block::Hash, T>>,
) -> SpecMsgAssembler<Block>
where
	Block: BlockT,
	C: ProvideRuntimeApi<Block> + Send + Sync + 'static,
	C::Api: SpecMsgApi<Block>,
	T: Transport + ?Sized + 'static,
{
	Arc::new(move |blocks: &[Block]| {
		let records = blocks
			.iter()
			.map(|block| consumption_record(&*client, block.header().hash()))
			.collect::<Result<Vec<_>, _>>()?;
		let lifts = receiver.assemble(&records)?;
		Ok(SpecMsgInputs { records, lifts })
	})
}
