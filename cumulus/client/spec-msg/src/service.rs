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
	LOG_TARGET,
};
use cumulus_client_collator::service::{SpecMsgAssembler, SpecMsgInputs};
use cumulus_primitives_core::{ParaId, SpecMsgApi};
use cumulus_primitives_spec_messaging::{ConsumptionRecord, MessagingInherentData, StreamsRoot};
use cumulus_relay_chain_interface::{call_runtime_api, PHash, RelayChainInterface};
use futures::{select, StreamExt};
use polkadot_primitives::runtime_api::ParachainHost;
use sc_client_api::BlockchainEvents;
use sp_api::{ApiExt, ProvideRuntimeApi, RuntimeApiInfo};
use sp_blockchain::HeaderBackend;
use sp_runtime::{
	traits::{Block as BlockT, Header as HeaderT},
	SaturatedConversion,
};
use std::sync::Arc;

/// The relay runtime API giving a source's newest included `StreamsRoot`.
pub const NEWEST_INCLUDED_PROVIDES: &str = "ParachainHost_newest_included_provides";

/// The `ParachainHost` version that added [`NEWEST_INCLUDED_PROVIDES`].
pub const NEWEST_INCLUDED_PROVIDES_VERSION: u32 = 17;

/// The consumption view at `at`, or `None` if its runtime has no `SpecMsgApi`.
pub fn chain_view<Block, C>(client: &C, at: Block::Hash, us: ParaId) -> Option<ChainView>
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
	let out_channels = api
		.out_channels(at)
		.map_err(|e| log::warn!(target: LOG_TARGET, "out_channels at {at:?} failed: {e}"))
		.ok()?;
	let caps = api
		.consumption_caps(at)
		.map_err(|e| log::warn!(target: LOG_TARGET, "consumption_caps at {at:?} failed: {e}"))
		.ok()?;
	Some(ChainView { caps: Some(caps), ..ChainView::new(us, consumed, out_channels) })
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

/// `source`'s newest included root as of relay block `at`. `None` also when the relay runtime
/// predates the API.
pub async fn newest_included_provides(
	relay: &(impl RelayChainInterface + ?Sized),
	at: PHash,
	source: ParaId,
) -> Option<StreamsRoot> {
	call_runtime_api::<Option<StreamsRoot>>(relay, NEWEST_INCLUDED_PROVIDES, at, source)
		.await
		.map_err(|e| log::debug!(target: LOG_TARGET, "{NEWEST_INCLUDED_PROVIDES} failed: {e}"))
		.ok()
		.flatten()
}

/// Whether relay block `at` has [`NEWEST_INCLUDED_PROVIDES`].
async fn relay_has_api(relay: &(impl RelayChainInterface + ?Sized), at: PHash) -> bool {
	let id = <dyn ParachainHost<polkadot_primitives::Block>>::ID;
	match relay.version(at).await {
		Ok(version) => version
			.api_version(&id)
			.is_some_and(|version| version >= NEWEST_INCLUDED_PROVIDES_VERSION),
		Err(e) => {
			log::debug!(target: LOG_TARGET, "Relay runtime version at {at:?} unavailable: {e}");
			false
		},
	}
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
					receiver.on_finalized((*finalized.header.number()).saturated_into())
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
	let Some(view) = chain_view(client, client.info().best_hash, receiver.para_id()) else {
		return;
	};
	if view.sources.is_empty() || !relay_has_api(relay, relay_hash).await {
		return;
	}
	let updates = view.sources.iter().map(|(source, wants)| async move {
		let root = newest_included_provides(relay, relay_hash, *source).await?;
		if !receiver.is_settled(source, &root) {
			receiver.on_root(*source, root, wants).await;
		}
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
	match chain_view(client, parent, receiver.para_id()) {
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
