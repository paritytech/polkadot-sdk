// Copyright (C) Parity Technologies (UK) Ltd.
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

use sc_client_api::{
	Backend, BlockBackend, BlockImportNotification, BlockchainEvents, Finalizer, UsageProvider,
};
use sc_consensus::{BlockImport, BlockImportParams, ForkChoiceStrategy};
use schnellru::{ByLength, LruMap};
use sp_blockchain::Error as ClientError;
use sp_consensus::{BlockOrigin, BlockStatus};
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};

use cumulus_client_pov_recovery::{RecoveryKind, RecoveryRequest};
use cumulus_relay_chain_interface::{RelayChainInterface, RelayChainResult};

use polkadot_primitives::{Hash as PHash, Id as ParaId, OccupiedCoreAssumption};

use codec::Decode;
use futures::{channel::mpsc::Sender, pin_mut, select, FutureExt, Stream, StreamExt};

use std::sync::Arc;

const LOG_TARGET: &str = "cumulus-consensus";
const FINALIZATION_CACHE_SIZE: u32 = 40;

fn handle_new_finalized_head<P, Block, B>(
	parachain: &Arc<P>,
	finalized_head: Vec<u8>,
	last_seen_finalized_hashes: &mut LruMap<Block::Hash, ()>,
) where
	Block: BlockT,
	B: Backend<Block>,
	P: Finalizer<Block, B> + UsageProvider<Block> + BlockchainEvents<Block>,
{
	let header = match Block::Header::decode(&mut &finalized_head[..]) {
		Ok(header) => header,
		Err(err) => {
			tracing::debug!(
				target: LOG_TARGET,
				error = ?err,
				"Could not decode parachain header while following finalized heads.",
			);
			return
		},
	};

	let hash = header.hash();

	last_seen_finalized_hashes.insert(hash, ());

	// Only finalize if we are below the incoming finalized parachain head
	if parachain.usage_info().chain.finalized_number < *header.number() {
		tracing::debug!(
			target: LOG_TARGET,
			block_hash = ?hash,
			"Attempting to finalize header.",
		);
		if let Err(e) = parachain.finalize_block(hash, None, true) {
			match e {
				ClientError::UnknownBlock(_) => tracing::debug!(
					target: LOG_TARGET,
					block_hash = ?hash,
					"Could not finalize block because it is unknown.",
				),
				_ => tracing::warn!(
					target: LOG_TARGET,
					error = ?e,
					block_hash = ?hash,
					"Failed to finalize block",
				),
			}
		}
	}
}

/// Follow the finalized head of the given parachain.
///
/// For every finalized block of the relay chain, it will get the included parachain header
/// corresponding to `para_id` and will finalize it in the parachain.
async fn follow_finalized_head<P, Block, B, R>(para_id: ParaId, parachain: Arc<P>, relay_chain: R)
where
	Block: BlockT,
	P: Finalizer<Block, B> + UsageProvider<Block> + BlockchainEvents<Block>,
	R: RelayChainInterface + Clone,
	B: Backend<Block>,
{
	let finalized_heads = match finalized_heads(relay_chain, para_id).await {
		Ok(finalized_heads_stream) => finalized_heads_stream.fuse(),
		Err(err) => {
			tracing::error!(target: LOG_TARGET, error = ?err, "Unable to retrieve finalized heads stream.");
			return
		},
	};

	let mut imported_blocks = parachain.import_notification_stream().fuse();

	pin_mut!(finalized_heads);

	// We use this cache to finalize blocks that are imported late.
	// For example, a block that has been recovered via PoV-Recovery
	// on a full node can have several minutes delay. With this cache
	// we have some "memory" of recently finalized blocks.
	let mut last_seen_finalized_hashes = LruMap::new(ByLength::new(FINALIZATION_CACHE_SIZE));

	loop {
		select! {
			fin = finalized_heads.next() => {
				match fin {
					Some(finalized_head) =>
						handle_new_finalized_head(&parachain, finalized_head, &mut last_seen_finalized_hashes),
					None => {
						tracing::debug!(target: LOG_TARGET, "Stopping following finalized head.");
						return
					}
				}
			},
			imported = imported_blocks.next() => {
				match imported {
					Some(imported_block) => {
						// When we see a block import that is already finalized, we immediately finalize it.
						if last_seen_finalized_hashes.peek(&imported_block.hash).is_some() {
							tracing::debug!(
								target: LOG_TARGET,
								block_hash = ?imported_block.hash,
								"Setting newly imported block as finalized.",
							);

							if let Err(e) = parachain.finalize_block(imported_block.hash, None, true) {
								match e {
									ClientError::UnknownBlock(_) => tracing::debug!(
										target: LOG_TARGET,
										block_hash = ?imported_block.hash,
										"Could not finalize block because it is unknown.",
									),
									_ => tracing::warn!(
										target: LOG_TARGET,
										error = ?e,
										block_hash = ?imported_block.hash,
										"Failed to finalize block",
									),
								}
							}
						}
					},
					None => {
						tracing::debug!(
							target: LOG_TARGET,
							"Stopping following imported blocks.",
						);
						return
					}
				}
			}
		}
	}
}

/// Run the parachain consensus.
///
/// This will follow the given `relay_chain` to act as consensus for the parachain that corresponds
/// to the given `para_id`. It will set the new best block of the parachain as it gets aware of it.
/// The same happens for the finalized block.
///
/// # Note
///
/// This will access the backend of the parachain and thus, this future should be spawned as
/// blocking task.
pub async fn run_parachain_consensus<P, R, Block, B>(
	para_id: ParaId,
	parachain: Arc<P>,
	relay_chain: R,
	announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	recovery_chan_tx: Option<Sender<RecoveryRequest<Block>>>,
) where
	Block: BlockT,
	P: Finalizer<Block, B>
		+ UsageProvider<Block>
		+ Send
		+ Sync
		+ BlockBackend<Block>
		+ BlockchainEvents<Block>,
	for<'a> &'a P: BlockImport<Block>,
	R: RelayChainInterface + Clone,
	B: Backend<Block>,
{
	let follow_new_best = follow_new_best(
		para_id,
		parachain.clone(),
		relay_chain.clone(),
		announce_block,
		recovery_chan_tx,
	);
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
	mut recovery_chan_tx: Option<Sender<RecoveryRequest<Block>>>,
) where
	Block: BlockT,
	P: Finalizer<Block, B>
		+ UsageProvider<Block>
		+ Send
		+ Sync
		+ BlockBackend<Block>
		+ BlockchainEvents<Block>,
	for<'a> &'a P: BlockImport<Block>,
	R: RelayChainInterface + Clone,
	B: Backend<Block>,
{
	let new_best_heads = match new_best_heads(relay_chain, para_id).await {
		Ok(best_heads_stream) => best_heads_stream.fuse(),
		Err(err) => {
			tracing::error!(target: LOG_TARGET, error = ?err, "Unable to retrieve best heads stream.");
			return
		},
	};

	pin_mut!(new_best_heads);

	let mut imported_blocks = parachain.import_notification_stream().fuse();
	// The unset best header of the parachain. Will be `Some(_)` when we have imported a relay chain
	// block before the associated parachain block. In this case we need to wait for this block to
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
						recovery_chan_tx.as_mut(),
					).await,
					None => {
						tracing::debug!(
							target: LOG_TARGET,
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
							target: LOG_TARGET,
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

	match parachain.block_status(unset_hash) {
		Ok(BlockStatus::InChainWithState) => {
			let unset_best_header = unset_best_header_opt
				.take()
				.expect("We checked above that the value is set; qed");
			tracing::debug!(
				target: LOG_TARGET,
				?unset_hash,
				"Importing block as new best for parachain.",
			);
			import_block_as_new_best(unset_hash, unset_best_header, parachain).await;
		},
		state => tracing::debug!(
			target: LOG_TARGET,
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
	mut recovery_chan_tx: Option<&mut Sender<RecoveryRequest<Block>>>,
) where
	Block: BlockT,
	P: UsageProvider<Block> + Send + Sync + BlockBackend<Block>,
	for<'a> &'a P: BlockImport<Block>,
{
	let parachain_head = match <<Block as BlockT>::Header>::decode(&mut &head[..]) {
		Ok(header) => header,
		Err(err) => {
			tracing::debug!(
				target: LOG_TARGET,
				error = ?err,
				"Could not decode Parachain header while following best heads.",
			);
			return
		},
	};

	let hash = parachain_head.hash();

	if parachain.usage_info().chain.best_hash == hash {
		tracing::debug!(
			target: LOG_TARGET,
			block_hash = ?hash,
			"Skipping set new best block, because block is already the best.",
		)
	} else {
		// Make sure the block is already known or otherwise we skip setting new best.
		match parachain.block_status(hash) {
			Ok(BlockStatus::InChainWithState) => {
				unset_best_header.take();
				tracing::debug!(
					target: LOG_TARGET,
					?hash,
					"Importing block as new best for parachain.",
				);
				import_block_as_new_best(hash, parachain_head, parachain).await;
			},
			Ok(BlockStatus::InChainPruned) => {
				tracing::error!(
					target: LOG_TARGET,
					block_hash = ?hash,
					"Trying to set pruned block as new best!",
				);
			},
			Ok(BlockStatus::Unknown) => {
				*unset_best_header = Some(parachain_head);

				tracing::debug!(
					target: LOG_TARGET,
					block_hash = ?hash,
					"Parachain block not yet imported, waiting for import to enact as best block.",
				);

				if let Some(ref mut recovery_chan_tx) = recovery_chan_tx {
					// Best effort channel to actively encourage block recovery.
					// An error here is not fatal; the relay chain continuously re-announces
					// the best block, thus we will have other opportunities to retry.
					let req = RecoveryRequest { hash, kind: RecoveryKind::Full };
					if let Err(err) = recovery_chan_tx.try_send(req) {
						tracing::warn!(
							target: LOG_TARGET,
							block_hash = ?hash,
							error = ?err,
							"Unable to notify block recovery subsystem"
						)
					}
				}
			},
			Err(e) => {
				tracing::error!(
					target: LOG_TARGET,
					block_hash = ?hash,
					error = ?e,
					"Failed to get block status of block.",
				);
			},
			_ => {},
		}
	}
}

async fn import_block_as_new_best<Block, P>(
	hash: Block::Hash,
	header: Block::Header,
	mut parachain: &P,
) where
	Block: BlockT,
	P: UsageProvider<Block> + Send + Sync + BlockBackend<Block>,
	for<'a> &'a P: BlockImport<Block>,
{
	let best_number = parachain.usage_info().chain.best_number;
	if *header.number() < best_number {
		tracing::debug!(
			target: LOG_TARGET,
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

	if let Err(err) = parachain.import_block(block_import_params).await {
		tracing::warn!(
			target: LOG_TARGET,
			block_hash = ?hash,
			error = ?err,
			"Failed to set new best block.",
		);
	}
}

/// Returns a stream that will yield best heads for the given `para_id`.
async fn new_best_heads(
	relay_chain: impl RelayChainInterface + Clone,
	para_id: ParaId,
) -> RelayChainResult<impl Stream<Item = Vec<u8>>> {
	let new_best_notification_stream =
		relay_chain.new_best_notification_stream().await?.filter_map(move |n| {
			let relay_chain = relay_chain.clone();
			async move { parachain_head_at(&relay_chain, n.hash(), para_id).await.ok().flatten() }
		});

	Ok(new_best_notification_stream)
}

/// Returns a stream that will yield finalized heads for the given `para_id`.
async fn finalized_heads(
	relay_chain: impl RelayChainInterface + Clone,
	para_id: ParaId,
) -> RelayChainResult<impl Stream<Item = Vec<u8>>> {
	let finality_notification_stream =
		relay_chain.finality_notification_stream().await?.filter_map(move |n| {
			let relay_chain = relay_chain.clone();
			async move { parachain_head_at(&relay_chain, n.hash(), para_id).await.ok().flatten() }
		});

	Ok(finality_notification_stream)
}

/// Returns head of the parachain at the given relay chain block.
async fn parachain_head_at(
	relay_chain: &impl RelayChainInterface,
	at: PHash,
	para_id: ParaId,
) -> RelayChainResult<Option<Vec<u8>>> {
	relay_chain
		.persisted_validation_data(at, para_id, OccupiedCoreAssumption::TimedOut)
		.await
		.map(|s| s.map(|s| s.parent_head.0))
}
