// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use async_trait::async_trait;
use cumulus_relay_chain_interface::{RelayChainInterface, RelayChainResult};
use sc_client_api::{
	Backend, BlockBackend, BlockImportNotification, BlockchainEvents, Finalizer, UsageProvider,
};
use sc_consensus::{BlockImport, BlockImportParams, ForkChoiceStrategy};
use sp_blockchain::Error as ClientError;
use sp_consensus::{BlockOrigin, BlockStatus};
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, Header as HeaderT},
};

use polkadot_primitives::v1::{Block as PBlock, Id as ParaId, OccupiedCoreAssumption};

use codec::Decode;
use futures::{select, FutureExt, Stream, StreamExt};

use std::{pin::Pin, sync::Arc};

const LOG_TARGET: &str = "cumulus-consensus";

/// Helper for the relay chain client. This is expected to be a lightweight handle like an `Arc`.
#[async_trait]
pub trait RelaychainClient: Clone + 'static {
	/// The error type for interacting with the Polkadot client.
	type Error: std::fmt::Debug + Send;

	/// A stream that yields head-data for a parachain.
	type HeadStream: Stream<Item = Vec<u8>> + Send + Unpin;

	/// Get a stream of new best heads for the given parachain.
	async fn new_best_heads(&self, para_id: ParaId) -> RelayChainResult<Self::HeadStream>;

	/// Get a stream of finalized heads for the given parachain.
	async fn finalized_heads(&self, para_id: ParaId) -> RelayChainResult<Self::HeadStream>;

	/// Returns the parachain head for the given `para_id` at the given block id.
	async fn parachain_head_at(
		&self,
		at: &BlockId<PBlock>,
		para_id: ParaId,
	) -> RelayChainResult<Option<Vec<u8>>>;
}

/// Follow the finalized head of the given parachain.
///
/// For every finalized block of the relay chain, it will get the included parachain header
/// corresponding to `para_id` and will finalize it in the parachain.
async fn follow_finalized_head<P, Block, B, R>(para_id: ParaId, parachain: Arc<P>, relay_chain: R)
where
	Block: BlockT,
	P: Finalizer<Block, B> + UsageProvider<Block>,
	R: RelaychainClient,
	B: Backend<Block>,
{
	let mut finalized_heads = match relay_chain.finalized_heads(para_id).await {
		Ok(finalized_heads_stream) => finalized_heads_stream,
		Err(err) => {
			tracing::error!(target: LOG_TARGET, error = ?err, "Unable to retrieve finalized heads stream.");
			return
		},
	};

	loop {
		let finalized_head = if let Some(h) = finalized_heads.next().await {
			h
		} else {
			tracing::debug!(target: "cumulus-consensus", "Stopping following finalized head.");
			return
		};

		let header = match Block::Header::decode(&mut &finalized_head[..]) {
			Ok(header) => header,
			Err(err) => {
				tracing::debug!(
					target: "cumulus-consensus",
					error = ?err,
					"Could not decode parachain header while following finalized heads.",
				);
				continue
			},
		};

		let hash = header.hash();

		// don't finalize the same block multiple times.
		if parachain.usage_info().chain.finalized_hash != hash {
			if let Err(e) = parachain.finalize_block(BlockId::hash(hash), None, true) {
				match e {
					ClientError::UnknownBlock(_) => tracing::debug!(
						target: "cumulus-consensus",
						block_hash = ?hash,
						"Could not finalize block because it is unknown.",
					),
					_ => tracing::warn!(
						target: "cumulus-consensus",
						error = ?e,
						block_hash = ?hash,
						"Failed to finalize block",
					),
				}
			}
		}
	}
}

/// Run the parachain consensus.
///
/// This will follow the given `relay_chain` to act as consesus for the parachain that corresponds
/// to the given `para_id`. It will set the new best block of the parachain as it gets aware of it.
/// The same happens for the finalized block.
///
/// # Note
///
/// This will access the backend of the parachain and thus, this future should be spawned as blocking
/// task.
pub async fn run_parachain_consensus<P, R, Block, B>(
	para_id: ParaId,
	parachain: Arc<P>,
	relay_chain: R,
	announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
) where
	Block: BlockT,
	P: Finalizer<Block, B>
		+ UsageProvider<Block>
		+ Send
		+ Sync
		+ BlockBackend<Block>
		+ BlockchainEvents<Block>,
	for<'a> &'a P: BlockImport<Block>,
	R: RelaychainClient,
	B: Backend<Block>,
{
	let follow_new_best =
		follow_new_best(para_id, parachain.clone(), relay_chain.clone(), announce_block);
	let follow_finalized_head = follow_finalized_head(para_id, parachain, relay_chain);
	select! {
		_ = follow_new_best.fuse() => {},
		_ = follow_finalized_head.fuse() => {},
	}
}

/// Follow the relay chain new best head, to update the Parachain new best head.
async fn follow_new_best<P, R, Block, B>(
	para_id: ParaId,
	parachain: Arc<P>,
	relay_chain: R,
	announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
) where
	Block: BlockT,
	P: Finalizer<Block, B>
		+ UsageProvider<Block>
		+ Send
		+ Sync
		+ BlockBackend<Block>
		+ BlockchainEvents<Block>,
	for<'a> &'a P: BlockImport<Block>,
	R: RelaychainClient,
	B: Backend<Block>,
{
	let mut new_best_heads = match relay_chain.new_best_heads(para_id).await {
		Ok(best_heads_stream) => best_heads_stream.fuse(),
		Err(err) => {
			tracing::error!(target: LOG_TARGET, error = ?err, "Unable to retrieve best heads stream.");
			return
		},
	};

	let mut imported_blocks = parachain.import_notification_stream().fuse();
	// The unset best header of the parachain. Will be `Some(_)` when we have imported a relay chain
	// block before the parachain block it included. In this case we need to wait for this block to
	// be imported to set it as new best.
	let mut unset_best_header = None;

	loop {
		select! {
			h = new_best_heads.next() => {
				match h {
					Some(h) => handle_new_best_parachain_head(
						h,
						&*parachain,
						&mut unset_best_header,
					).await,
					None => {
						tracing::debug!(
							target: "cumulus-consensus",
							"Stopping following new best.",
						);
						return
					}
				}
			},
			i = imported_blocks.next() => {
				match i {
					Some(i) => handle_new_block_imported(
						i,
						&mut unset_best_header,
						&*parachain,
						&*announce_block,
					).await,
					None => {
						tracing::debug!(
							target: "cumulus-consensus",
							"Stopping following imported blocks.",
						);
						return
					}
				}
			},
		}
	}
}

/// Handle a new import block of the parachain.
async fn handle_new_block_imported<Block, P>(
	notification: BlockImportNotification<Block>,
	unset_best_header_opt: &mut Option<Block::Header>,
	parachain: &P,
	announce_block: &(dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync),
) where
	Block: BlockT,
	P: UsageProvider<Block> + Send + Sync + BlockBackend<Block>,
	for<'a> &'a P: BlockImport<Block>,
{
	// HACK
	//
	// Remove after https://github.com/paritytech/substrate/pull/8052 or similar is merged
	if notification.origin != BlockOrigin::Own {
		announce_block(notification.hash, None);
	}

	let unset_best_header = match (notification.is_new_best, &unset_best_header_opt) {
		// If this is the new best block or we don't have any unset block, we can end it here.
		(true, _) | (_, None) => return,
		(false, Some(ref u)) => u,
	};

	let unset_hash = if notification.header.number() < unset_best_header.number() {
		return
	} else if notification.header.number() == unset_best_header.number() {
		let unset_hash = unset_best_header.hash();

		if unset_hash != notification.hash {
			return
		} else {
			unset_hash
		}
	} else {
		unset_best_header.hash()
	};

	match parachain.block_status(&BlockId::Hash(unset_hash)) {
		Ok(BlockStatus::InChainWithState) => {
			drop(unset_best_header);
			let unset_best_header = unset_best_header_opt
				.take()
				.expect("We checked above that the value is set; qed");

			import_block_as_new_best(unset_hash, unset_best_header, parachain).await;
		},
		state => tracing::debug!(
			target: "cumulus-consensus",
			?unset_best_header,
			?notification.header,
			?state,
			"Unexpected state for unset best header.",
		),
	}
}

/// Handle the new best parachain head as extracted from the new best relay chain.
async fn handle_new_best_parachain_head<Block, P>(
	head: Vec<u8>,
	parachain: &P,
	unset_best_header: &mut Option<Block::Header>,
) where
	Block: BlockT,
	P: UsageProvider<Block> + Send + Sync + BlockBackend<Block>,
	for<'a> &'a P: BlockImport<Block>,
{
	let parachain_head = match <<Block as BlockT>::Header>::decode(&mut &head[..]) {
		Ok(header) => header,
		Err(err) => {
			tracing::debug!(
				target: "cumulus-consensus",
				error = ?err,
				"Could not decode Parachain header while following best heads.",
			);
			return
		},
	};

	let hash = parachain_head.hash();

	if parachain.usage_info().chain.best_hash == hash {
		tracing::debug!(
			target: "cumulus-consensus",
			block_hash = ?hash,
			"Skipping set new best block, because block is already the best.",
		)
	} else {
		// Make sure the block is already known or otherwise we skip setting new best.
		match parachain.block_status(&BlockId::Hash(hash)) {
			Ok(BlockStatus::InChainWithState) => {
				unset_best_header.take();

				import_block_as_new_best(hash, parachain_head, parachain).await;
			},
			Ok(BlockStatus::InChainPruned) => {
				tracing::error!(
					target: "cumulus-collator",
					block_hash = ?hash,
					"Trying to set pruned block as new best!",
				);
			},
			Ok(BlockStatus::Unknown) => {
				*unset_best_header = Some(parachain_head);

				tracing::debug!(
					target: "cumulus-collator",
					block_hash = ?hash,
					"Parachain block not yet imported, waiting for import to enact as best block.",
				);
			},
			Err(e) => {
				tracing::error!(
					target: "cumulus-collator",
					block_hash = ?hash,
					error = ?e,
					"Failed to get block status of block.",
				);
			},
			_ => {},
		}
	}
}

async fn import_block_as_new_best<Block, P>(hash: Block::Hash, header: Block::Header, parachain: &P)
where
	Block: BlockT,
	P: UsageProvider<Block> + Send + Sync + BlockBackend<Block>,
	for<'a> &'a P: BlockImport<Block>,
{
	let best_number = parachain.usage_info().chain.best_number;
	if *header.number() < best_number {
		tracing::debug!(
			target: "cumulus-consensus",
			%best_number,
			block_number = %header.number(),
			"Skipping importing block as new best block, because there already exists a \
			 best block with an higher number",
		);
		return
	}

	// Make it the new best block
	let mut block_import_params = BlockImportParams::new(BlockOrigin::ConsensusBroadcast, header);
	block_import_params.fork_choice = Some(ForkChoiceStrategy::Custom(true));
	block_import_params.import_existing = true;

	if let Err(err) = (&*parachain).import_block(block_import_params, Default::default()).await {
		tracing::warn!(
			target: "cumulus-consensus",
			block_hash = ?hash,
			error = ?err,
			"Failed to set new best block.",
		);
	}
}

#[async_trait]
impl<RCInterface> RelaychainClient for RCInterface
where
	RCInterface: RelayChainInterface + Clone + 'static,
{
	type Error = ClientError;

	type HeadStream = Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>;

	async fn new_best_heads(&self, para_id: ParaId) -> RelayChainResult<Self::HeadStream> {
		let relay_chain = self.clone();

		let new_best_notification_stream = self
			.new_best_notification_stream()
			.await?
			.filter_map(move |n| {
				let relay_chain = relay_chain.clone();
				async move {
					relay_chain
						.parachain_head_at(&BlockId::hash(n.hash()), para_id)
						.await
						.ok()
						.flatten()
				}
			})
			.boxed();
		Ok(new_best_notification_stream)
	}

	async fn finalized_heads(&self, para_id: ParaId) -> RelayChainResult<Self::HeadStream> {
		let relay_chain = self.clone();

		let finality_notification_stream = self
			.finality_notification_stream()
			.await?
			.filter_map(move |n| {
				let relay_chain = relay_chain.clone();
				async move {
					relay_chain
						.parachain_head_at(&BlockId::hash(n.hash()), para_id)
						.await
						.ok()
						.flatten()
				}
			})
			.boxed();
		Ok(finality_notification_stream)
	}

	async fn parachain_head_at(
		&self,
		at: &BlockId<PBlock>,
		para_id: ParaId,
	) -> RelayChainResult<Option<Vec<u8>>> {
		self.persisted_validation_data(at, para_id, OccupiedCoreAssumption::TimedOut)
			.await
			.map(|s| s.map(|s| s.parent_head.0))
	}
}
