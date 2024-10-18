// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Implementation of the `chainHead_follow` method.

use crate::chain_head::{
	chain_head::{LOG_TARGET, MAX_PINNED_BLOCKS},
	event::{
		BestBlockChanged, Finalized, FollowEvent, Initialized, NewBlock, RuntimeEvent,
		RuntimeVersionEvent,
	},
	subscription::{InsertedSubscriptionData, SubscriptionManagement, SubscriptionManagementError},
};
use futures::{
	channel::oneshot,
	stream::{self, Stream, StreamExt, TryStreamExt},
};
use log::debug;
use sc_client_api::{
	Backend, BlockBackend, BlockImportNotification, BlockchainEvents, FinalityNotification,
};
use sc_rpc::utils::Subscription;
use schnellru::{ByLength, LruMap};
use sp_api::CallApiAt;
use sp_blockchain::{
	Backend as BlockChainBackend, Error as BlockChainError, HeaderBackend, HeaderMetadata, Info,
};
use sp_runtime::{
	traits::{Block as BlockT, Header as HeaderT, NumberFor},
	SaturatedConversion, Saturating,
};
use std::{
	collections::{HashSet, VecDeque},
	sync::Arc,
};
/// The maximum number of finalized blocks provided by the
/// `Initialized` event.
const MAX_FINALIZED_BLOCKS: usize = 16;

/// Generates the events of the `chainHead_follow` method.
pub struct ChainHeadFollower<BE: Backend<Block>, Block: BlockT, Client> {
	/// Substrate client.
	client: Arc<Client>,
	/// Backend of the chain.
	backend: Arc<BE>,
	/// Subscriptions handle.
	sub_handle: SubscriptionManagement<Block, BE>,
	/// Subscription was started with the runtime updates flag.
	with_runtime: bool,
	/// Subscription ID.
	sub_id: String,
	/// The best reported block by this subscription.
	current_best_block: Option<Block::Hash>,
	/// LRU cache of pruned blocks.
	pruned_blocks: LruMap<Block::Hash, ()>,
	/// LRU cache of announced blocks.
	announced_blocks: AnnouncedBlocks<Block>,
	/// Stop all subscriptions if the distance between the leaves and the current finalized
	/// block is larger than this value.
	max_lagging_distance: usize,
	/// The maximum number of pending messages per subscription.
	pub subscription_buffer_cap: usize,
}

struct AnnouncedBlocks<Block: BlockT> {
	/// Unfinalized blocks.
	blocks: LruMap<Block::Hash, ()>,
	/// Finalized blocks.
	finalized: MostRecentFinalizedBlocks<Block>,
}

/// Wrapper over LRU to efficiently lookup hashes and remove elements as FIFO queue.
///
/// For the finalized blocks we use `peek` to avoid moving the block counter to the front.
/// This effectively means that the LRU acts as a FIFO queue. Otherwise, we might
/// end up with scenarios where the "finalized block" in the end of LRU is overwritten which
/// may not necessarily be the oldest finalized block i.e, possible that "get" promotes an
/// older finalized block because it was accessed more recently.
struct MostRecentFinalizedBlocks<Block: BlockT>(LruMap<Block::Hash, ()>);

impl<Block: BlockT> MostRecentFinalizedBlocks<Block> {
	/// Insert the finalized block hash into the LRU cache.
	fn insert(&mut self, block: Block::Hash) {
		self.0.insert(block, ());
	}

	/// Check if the block is contained in the LRU cache.
	fn contains(&mut self, block: &Block::Hash) -> Option<&()> {
		self.0.peek(block)
	}
}

impl<Block: BlockT> AnnouncedBlocks<Block> {
	/// Creates a new `AnnouncedBlocks`.
	fn new() -> Self {
		Self {
			// The total number of pinned blocks is `MAX_PINNED_BLOCKS`, ensure we don't
			// exceed the limit.
			blocks: LruMap::new(ByLength::new((MAX_PINNED_BLOCKS - MAX_FINALIZED_BLOCKS) as u32)),
			// We are keeping a smaller number of announced finalized blocks in memory.
			// This is because the `Finalized` event might be triggered before the `NewBlock` event.
			finalized: MostRecentFinalizedBlocks(LruMap::new(ByLength::new(
				MAX_FINALIZED_BLOCKS as u32,
			))),
		}
	}

	/// Insert the block into the announced blocks.
	fn insert(&mut self, block: Block::Hash, finalized: bool) {
		if finalized {
			// When a block is declared as finalized, it is removed from the unfinalized blocks.
			//
			// Given that the finalized blocks are bounded to `MAX_FINALIZED_BLOCKS`,
			// this ensures we keep the minimum number of blocks in memory.
			self.blocks.remove(&block);
			self.finalized.insert(block);
		} else {
			self.blocks.insert(block, ());
		}
	}

	/// Check if the block was previously announced.
	fn was_announced(&mut self, block: &Block::Hash) -> bool {
		self.blocks.get(block).is_some() || self.finalized.contains(block).is_some()
	}
}

impl<BE: Backend<Block>, Block: BlockT, Client> ChainHeadFollower<BE, Block, Client> {
	/// Create a new [`ChainHeadFollower`].
	pub fn new(
		client: Arc<Client>,
		backend: Arc<BE>,
		sub_handle: SubscriptionManagement<Block, BE>,
		with_runtime: bool,
		sub_id: String,
		max_lagging_distance: usize,
		subscription_buffer_cap: usize,
	) -> Self {
		Self {
			client,
			backend,
			sub_handle,
			with_runtime,
			sub_id,
			current_best_block: None,
			pruned_blocks: LruMap::new(ByLength::new(
				MAX_PINNED_BLOCKS.try_into().unwrap_or(u32::MAX),
			)),
			announced_blocks: AnnouncedBlocks::new(),
			max_lagging_distance,
			subscription_buffer_cap,
		}
	}
}

/// A block notification.
enum NotificationType<Block: BlockT> {
	/// The initial events generated from the node's memory.
	InitialEvents(Vec<FollowEvent<Block::Hash>>),
	/// The new block notification obtained from `import_notification_stream`.
	NewBlock(BlockImportNotification<Block>),
	/// The finalized block notification obtained from `finality_notification_stream`.
	Finalized(FinalityNotification<Block>),
	/// The response of `chainHead` method calls.
	MethodResponse(FollowEvent<Block::Hash>),
}

/// The initial blocks that should be reported or ignored by the chainHead.
#[derive(Clone, Debug)]
struct InitialBlocks<Block: BlockT> {
	/// Children of the latest finalized block, for which the `NewBlock`
	/// event must be generated.
	///
	/// It is a tuple of (block hash, parent hash).
	finalized_block_descendants: Vec<(Block::Hash, Block::Hash)>,
	/// Hashes of the last finalized blocks
	finalized_block_hashes: VecDeque<Block::Hash>,
	/// Blocks that should not be reported as pruned by the `Finalized` event.
	///
	/// Substrate database will perform the pruning of height N at
	/// the finalization N + 1. We could have the following block tree
	/// when the user subscribes to the `follow` method:
	///   [A] - [A1] - [A2] - [A3]
	///                 ^^ finalized
	///       - [A1] - [B1]
	///
	/// When the A3 block is finalized, B1 is reported as pruned, however
	/// B1 was never reported as `NewBlock` (and as such was never pinned).
	/// This is because the `NewBlock` events are generated for children of
	/// the finalized hash.
	pruned_forks: HashSet<Block::Hash>,
}

/// The startup point from which chainHead started to generate events.
struct StartupPoint<Block: BlockT> {
	/// Best block hash.
	pub best_hash: Block::Hash,
	/// The head of the finalized chain.
	pub finalized_hash: Block::Hash,
	/// Last finalized block number.
	pub finalized_number: NumberFor<Block>,
}

impl<Block: BlockT> From<Info<Block>> for StartupPoint<Block> {
	fn from(info: Info<Block>) -> Self {
		StartupPoint::<Block> {
			best_hash: info.best_hash,
			finalized_hash: info.finalized_hash,
			finalized_number: info.finalized_number,
		}
	}
}

impl<BE, Block, Client> ChainHeadFollower<BE, Block, Client>
where
	Block: BlockT + 'static,
	BE: Backend<Block> + 'static,
	Client: BlockBackend<Block>
		+ HeaderBackend<Block>
		+ HeaderMetadata<Block, Error = BlockChainError>
		+ BlockchainEvents<Block>
		+ CallApiAt<Block>
		+ 'static,
{
	/// Conditionally generate the runtime event of the given block.
	fn generate_runtime_event(
		&self,
		block: Block::Hash,
		parent: Option<Block::Hash>,
	) -> Option<RuntimeEvent> {
		// No runtime versions should be reported.
		if !self.with_runtime {
			return None
		}

		let block_rt = match self.client.runtime_version_at(block) {
			Ok(rt) => rt,
			Err(err) => return Some(err.into()),
		};

		let parent = match parent {
			Some(parent) => parent,
			// Nothing to compare against, always report.
			None => return Some(RuntimeEvent::Valid(RuntimeVersionEvent { spec: block_rt.into() })),
		};

		let parent_rt = match self.client.runtime_version_at(parent) {
			Ok(rt) => rt,
			Err(err) => return Some(err.into()),
		};

		// Report the runtime version change.
		if block_rt != parent_rt {
			Some(RuntimeEvent::Valid(RuntimeVersionEvent { spec: block_rt.into() }))
		} else {
			None
		}
	}

	/// Check the distance between the provided blocks does not exceed a
	/// a reasonable range.
	///
	/// When the blocks are too far apart (potentially millions of blocks):
	///  - Tree route is expensive to calculate.
	///  - The RPC layer will not be able to generate the `NewBlock` events for all blocks.
	///
	/// This edge-case can happen for parachains where the relay chain syncs slower to
	/// the head of the chain than the parachain node that is synced already.
	fn distance_within_reason(
		&self,
		block: Block::Hash,
		finalized: Block::Hash,
	) -> Result<(), SubscriptionManagementError> {
		let Some(block_num) = self.client.number(block)? else {
			return Err(SubscriptionManagementError::BlockHashAbsent)
		};
		let Some(finalized_num) = self.client.number(finalized)? else {
			return Err(SubscriptionManagementError::BlockHashAbsent)
		};

		let distance: usize = block_num.saturating_sub(finalized_num).saturated_into();
		if distance > self.max_lagging_distance {
			return Err(SubscriptionManagementError::BlockDistanceTooLarge);
		}

		Ok(())
	}

	/// Get the in-memory blocks of the client, starting from the provided finalized hash.
	///
	/// The reported blocks are pinned by this function.
	fn get_init_blocks_with_forks(
		&self,
		finalized: Block::Hash,
	) -> Result<InitialBlocks<Block>, SubscriptionManagementError> {
		let blockchain = self.backend.blockchain();
		let leaves = blockchain.leaves()?;
		let mut pruned_forks = HashSet::new();
		let mut finalized_block_descendants = Vec::new();
		let mut unique_descendants = HashSet::new();

		// Ensure all leaves are within a reasonable distance from the finalized block,
		// before traversing the tree.
		for leaf in &leaves {
			self.distance_within_reason(*leaf, finalized)?;
		}

		for leaf in leaves {
			let tree_route = sp_blockchain::tree_route(blockchain, finalized, leaf)?;

			let blocks = tree_route.enacted().iter().map(|block| block.hash);
			if !tree_route.retracted().is_empty() {
				pruned_forks.extend(blocks);
			} else {
				// Ensure a `NewBlock` event is generated for all children of the
				// finalized block. Describe the tree route as (child_node, parent_node)
				// Note: the order of elements matters here.
				let mut parent = finalized;
				for child in blocks {
					let pair = (child, parent);

					if unique_descendants.insert(pair) {
						// The finalized block is pinned below.
						self.sub_handle.pin_block(&self.sub_id, child)?;
						finalized_block_descendants.push(pair);
					}

					parent = child;
				}
			}
		}

		let mut current_block = finalized;
		// The header of the finalized block must not be pruned.
		let Some(header) = blockchain.header(current_block)? else {
			return Err(SubscriptionManagementError::BlockHeaderAbsent);
		};

		// Report at most `MAX_FINALIZED_BLOCKS`. Note: The node might not have that many blocks.
		let mut finalized_block_hashes = VecDeque::with_capacity(MAX_FINALIZED_BLOCKS);

		// Pin the finalized block.
		self.sub_handle.pin_block(&self.sub_id, current_block)?;
		finalized_block_hashes.push_front(current_block);
		current_block = *header.parent_hash();

		for _ in 0..MAX_FINALIZED_BLOCKS - 1 {
			let Ok(Some(header)) = blockchain.header(current_block) else { break };
			// Block cannot be reported if pinning fails.
			if self.sub_handle.pin_block(&self.sub_id, current_block).is_err() {
				break
			};

			finalized_block_hashes.push_front(current_block);
			current_block = *header.parent_hash();
		}

		Ok(InitialBlocks { finalized_block_descendants, finalized_block_hashes, pruned_forks })
	}

	/// Generate the initial events reported by the RPC `follow` method.
	///
	/// Returns the initial events that should be reported directly.
	fn generate_init_events(
		&mut self,
		startup_point: &StartupPoint<Block>,
	) -> Result<Vec<FollowEvent<Block::Hash>>, SubscriptionManagementError> {
		let init = self.get_init_blocks_with_forks(startup_point.finalized_hash)?;

		// The initialized event is the first one sent.
		let initial_blocks = init.finalized_block_descendants;
		let finalized_block_hashes = init.finalized_block_hashes;
		// These are the pruned blocks that we should not report again.
		for pruned in init.pruned_forks {
			self.pruned_blocks.insert(pruned, ());
		}

		let finalized_block_hash = startup_point.finalized_hash;
		let finalized_block_runtime = self.generate_runtime_event(finalized_block_hash, None);

		for finalized in &finalized_block_hashes {
			self.announced_blocks.insert(*finalized, true);
		}

		let initialized_event = FollowEvent::Initialized(Initialized {
			finalized_block_hashes: finalized_block_hashes.into(),
			finalized_block_runtime,
			with_runtime: self.with_runtime,
		});

		let mut finalized_block_descendants = Vec::with_capacity(initial_blocks.len() + 1);

		finalized_block_descendants.push(initialized_event);
		for (child, parent) in initial_blocks.into_iter() {
			// If the parent was not announced we have a gap currently.
			// This can happen during a WarpSync.
			if !self.announced_blocks.was_announced(&parent) {
				return Err(SubscriptionManagementError::BlockHeaderAbsent);
			}
			self.announced_blocks.insert(child, false);

			let new_runtime = self.generate_runtime_event(child, Some(parent));

			let event = FollowEvent::NewBlock(NewBlock {
				block_hash: child,
				parent_block_hash: parent,
				new_runtime,
				with_runtime: self.with_runtime,
			});

			finalized_block_descendants.push(event);
		}

		// Generate a new best block event.
		let best_block_hash = startup_point.best_hash;
		if best_block_hash != finalized_block_hash {
			if !self.announced_blocks.was_announced(&best_block_hash) {
				return Err(SubscriptionManagementError::BlockHeaderAbsent);
			}
			self.announced_blocks.insert(best_block_hash, true);

			let best_block = FollowEvent::BestBlockChanged(BestBlockChanged { best_block_hash });
			self.current_best_block = Some(best_block_hash);
			finalized_block_descendants.push(best_block);
		};

		Ok(finalized_block_descendants)
	}

	/// Generate the "NewBlock" event and potentially the "BestBlockChanged" event for the
	/// given block hash.
	fn generate_import_events(
		&mut self,
		block_hash: Block::Hash,
		parent_block_hash: Block::Hash,
		is_best_block: bool,
	) -> Vec<FollowEvent<Block::Hash>> {
		let new_runtime = self.generate_runtime_event(block_hash, Some(parent_block_hash));

		let new_block = FollowEvent::NewBlock(NewBlock {
			block_hash,
			parent_block_hash,
			new_runtime,
			with_runtime: self.with_runtime,
		});

		if !is_best_block {
			return vec![new_block]
		}

		// If this is the new best block, then we need to generate two events.
		let best_block_event =
			FollowEvent::BestBlockChanged(BestBlockChanged { best_block_hash: block_hash });

		match self.current_best_block {
			Some(block_cache) => {
				// The RPC layer has not reported this block as best before.
				// Note: This handles the race with the finalized branch.
				if block_cache != block_hash {
					self.current_best_block = Some(block_hash);
					vec![new_block, best_block_event]
				} else {
					vec![new_block]
				}
			},
			None => {
				self.current_best_block = Some(block_hash);
				vec![new_block, best_block_event]
			},
		}
	}

	/// Handle the import of new blocks by generating the appropriate events.
	fn handle_import_blocks(
		&mut self,
		notification: BlockImportNotification<Block>,
		startup_point: &StartupPoint<Block>,
	) -> Result<Vec<FollowEvent<Block::Hash>>, SubscriptionManagementError> {
		let block_hash = notification.hash;

		// Ensure we are only reporting blocks after the starting point.
		if *notification.header.number() < startup_point.finalized_number {
			return Ok(Default::default())
		}

		// Ensure the block can be pinned before generating the events.
		if !self.sub_handle.pin_block(&self.sub_id, block_hash)? {
			// The block is already pinned, this is similar to the check above.
			//
			// The `SubscriptionManagement` ensures the block is tracked until (short lived):
			// - 2 calls to `pin_block` are made (from `Finalized` and `NewBlock` branches).
			// - the block is unpinned by the user
			//
			// This is rather a sanity checks for edge-cases (in theory), where
			// [`MAX_FINALIZED_BLOCKS` + 1] finalized events are triggered before the `NewBlock`
			// event of the first `Finalized` event.
			return Ok(Default::default())
		}

		if self.announced_blocks.was_announced(&block_hash) {
			// Block was already reported by the finalized branch.
			return Ok(Default::default())
		}

		// Double check the parent hash. If the parent hash is not reported, we have a gap.
		let parent_block_hash = *notification.header.parent_hash();
		if !self.announced_blocks.was_announced(&parent_block_hash) {
			// The parent block was not reported, we have a gap.
			return Err(SubscriptionManagementError::Custom("Parent block was not reported".into()))
		}

		self.announced_blocks.insert(block_hash, false);
		Ok(self.generate_import_events(block_hash, parent_block_hash, notification.is_new_best))
	}

	/// Generates new block events from the given finalized hashes.
	///
	/// It may be possible that the `Finalized` event fired before the `NewBlock`
	/// event. Only in that case we generate:
	/// - `NewBlock` event for all finalized hashes.
	/// - `BestBlock` event for the last finalized hash.
	///
	/// This function returns an empty list if all finalized hashes were already reported
	/// and are pinned.
	fn generate_finalized_events(
		&mut self,
		finalized_block_hashes: &[Block::Hash],
	) -> Result<Vec<FollowEvent<Block::Hash>>, SubscriptionManagementError> {
		let mut events = Vec::new();

		// Nothing to be done if no finalized hashes are provided.
		let Some(first_hash) = finalized_block_hashes.get(0) else { return Ok(Default::default()) };

		// Find the parent header.
		let Some(first_header) = self.client.header(*first_hash)? else {
			return Err(SubscriptionManagementError::BlockHeaderAbsent)
		};

		if !self.announced_blocks.was_announced(first_header.parent_hash()) {
			return Err(SubscriptionManagementError::Custom(
				"Parent block was not reported for a finalized block".into(),
			));
		}

		let parents =
			std::iter::once(first_header.parent_hash()).chain(finalized_block_hashes.iter());
		for (i, (hash, parent)) in finalized_block_hashes.iter().zip(parents).enumerate() {
			// Ensure the block is pinned before generating the events.
			self.sub_handle.pin_block(&self.sub_id, *hash)?;

			// Check if the block was already reported.
			if self.announced_blocks.was_announced(hash) {
				continue;
			}

			// Generate `NewBlock` events for all blocks beside the last block in the list
			let is_last = i + 1 == finalized_block_hashes.len();
			if !is_last {
				// Generate only the `NewBlock` event for this block.
				events.extend(self.generate_import_events(*hash, *parent, false));
				self.announced_blocks.insert(*hash, true);
				continue;
			}

			if let Some(best_block_hash) = self.current_best_block {
				let ancestor =
					sp_blockchain::lowest_common_ancestor(&*self.client, *hash, best_block_hash)?;

				// If we end up here and the `best_block` is a descendent of the finalized block
				// (last block in the list), it means that there were skipped notifications.
				// Otherwise `pin_block` would had returned `false`.
				//
				// When the node falls out of sync and then syncs up to the tip of the chain, it can
				// happen that we skip notifications. Then it is better to terminate the connection
				// instead of trying to send notifications for all missed blocks.
				if ancestor.hash == *hash {
					return Err(SubscriptionManagementError::Custom(
						"A descendent of the finalized block was already reported".into(),
					))
				}
			}

			// Let's generate the `NewBlock` and `NewBestBlock` events for the block.
			events.extend(self.generate_import_events(*hash, *parent, true));
			self.announced_blocks.insert(*hash, true);
		}

		Ok(events)
	}

	/// Get all pruned block hashes from the provided stale heads.
	fn get_pruned_hashes(
		&mut self,
		stale_heads: &[Block::Hash],
		last_finalized: Block::Hash,
	) -> Result<Vec<Block::Hash>, SubscriptionManagementError> {
		let blockchain = self.backend.blockchain();
		let mut pruned = Vec::new();

		for stale_head in stale_heads {
			let tree_route = sp_blockchain::tree_route(blockchain, last_finalized, *stale_head)?;

			// Collect only blocks that are not part of the canonical chain.
			pruned.extend(tree_route.enacted().iter().filter_map(|block| {
				if self.pruned_blocks.get(&block.hash).is_some() {
					// The block was already reported as pruned.
					return None
				}

				self.pruned_blocks.insert(block.hash, ());
				Some(block.hash)
			}))
		}

		Ok(pruned)
	}

	/// Handle the finalization notification by generating the `Finalized` event.
	///
	/// If the block of the notification was not reported yet, this method also
	/// generates the events similar to `handle_import_blocks`.
	fn handle_finalized_blocks(
		&mut self,
		notification: FinalityNotification<Block>,
		startup_point: &StartupPoint<Block>,
	) -> Result<Vec<FollowEvent<Block::Hash>>, SubscriptionManagementError> {
		let last_finalized = notification.hash;

		// Ensure we are only reporting blocks after the starting point.
		if *notification.header.number() < startup_point.finalized_number {
			return Ok(Default::default())
		}

		// The tree route contains the exclusive path from the last finalized block to the block
		// reported by the notification. Ensure the finalized block is also reported.
		let mut finalized_block_hashes = notification.tree_route.to_vec();
		finalized_block_hashes.push(last_finalized);

		// If the finalized hashes were not reported yet, generate the `NewBlock` events.
		let mut events = self.generate_finalized_events(&finalized_block_hashes)?;

		// Report all pruned blocks from the notification that are not
		// part of the fork we need to ignore.
		let pruned_block_hashes =
			self.get_pruned_hashes(&notification.stale_heads, last_finalized)?;

		for finalized in &finalized_block_hashes {
			self.announced_blocks.insert(*finalized, true);
		}

		let finalized_event = FollowEvent::Finalized(Finalized {
			finalized_block_hashes,
			pruned_block_hashes: pruned_block_hashes.clone(),
		});

		if let Some(current_best_block) = self.current_best_block {
			// We need to generate a new best block if the best block is in the pruned list.
			let is_in_pruned_list =
				pruned_block_hashes.iter().any(|hash| *hash == current_best_block);
			if is_in_pruned_list {
				self.current_best_block = Some(last_finalized);
				events.push(FollowEvent::BestBlockChanged(BestBlockChanged {
					best_block_hash: last_finalized,
				}));
			} else {
				// The pruning logic ensures that when the finalized block is announced,
				// all blocks on forks that have the common ancestor lower or equal
				// to the finalized block are reported.
				//
				// However, we double check if the best block is a descendant of the last finalized
				// block to ensure we don't miss any events.
				let ancestor = sp_blockchain::lowest_common_ancestor(
					&*self.client,
					last_finalized,
					current_best_block,
				)?;
				let is_descendant = ancestor.hash == last_finalized;
				if !is_descendant {
					self.current_best_block = Some(last_finalized);
					events.push(FollowEvent::BestBlockChanged(BestBlockChanged {
						best_block_hash: last_finalized,
					}));
				}
			}
		}

		events.push(finalized_event);
		Ok(events)
	}

	/// Submit the events from the provided stream to the RPC client
	/// for as long as the `rx_stop` event was not called.
	async fn submit_events<EventStream>(
		&mut self,
		startup_point: &StartupPoint<Block>,
		stream: EventStream,
		sink: Subscription,
		rx_stop: oneshot::Receiver<()>,
	) -> Result<(), SubscriptionManagementError>
	where
		EventStream: Stream<Item = NotificationType<Block>> + Unpin + Send,
	{
		let buffer_cap = self.subscription_buffer_cap;
		// create a channel to propagate error messages
		let mut handle_events = |event| match event {
			NotificationType::InitialEvents(events) => Ok(events),
			NotificationType::NewBlock(notification) =>
				self.handle_import_blocks(notification, &startup_point),
			NotificationType::Finalized(notification) =>
				self.handle_finalized_blocks(notification, &startup_point),
			NotificationType::MethodResponse(notification) => Ok(vec![notification]),
		};

		let stream = stream
			.map(|event| handle_events(event))
			.map_ok(|items| stream::iter(items).map(Ok))
			.try_flatten();

		tokio::pin!(stream);

		let sink_future =
			sink.pipe_from_try_stream(stream, sc_rpc::utils::BoundedVecDeque::new(buffer_cap));

		let result = tokio::select! {
			_ = rx_stop => Ok(()),
			result = sink_future => {
				if let Err(ref e) = result {
					debug!(
						target: LOG_TARGET,
						"[follow][id={:?}] Failed to handle stream notification {:?}",
						&self.sub_id,
						e
					);
				};
				result
			}
		};
		let _ = sink.send(&FollowEvent::<String>::Stop).await;
		result
	}

	/// Generate the block events for the `chainHead_follow` method.
	pub async fn generate_events(
		&mut self,
		sink: Subscription,
		sub_data: InsertedSubscriptionData<Block>,
	) -> Result<(), SubscriptionManagementError> {
		// Register for the new block and finalized notifications.
		let stream_import = self
			.client
			.import_notification_stream()
			.map(|notification| NotificationType::NewBlock(notification));

		let stream_finalized = self
			.client
			.finality_notification_stream()
			.map(|notification| NotificationType::Finalized(notification));

		let stream_responses = sub_data
			.response_receiver
			.map(|response| NotificationType::MethodResponse(response));

		let startup_point = StartupPoint::from(self.client.info());
		let initial_events = match self.generate_init_events(&startup_point) {
			Ok(blocks) => blocks,
			Err(err) => {
				debug!(
					target: LOG_TARGET,
					"[follow][id={:?}] Failed to generate the initial events {:?}",
					self.sub_id,
					err
				);
				let _ = sink.send(&FollowEvent::<String>::Stop).await;
				return Err(err)
			},
		};

		let initial = NotificationType::InitialEvents(initial_events);
		let merged = tokio_stream::StreamExt::merge(stream_import, stream_finalized);
		let merged = tokio_stream::StreamExt::merge(merged, stream_responses);
		let stream = stream::once(futures::future::ready(initial)).chain(merged);

		self.submit_events(&startup_point, stream.boxed(), sink, sub_data.rx_stop).await
	}
}
