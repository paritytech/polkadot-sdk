// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Historic block syncing logic for the Ethereum JSON-RPC server.

use crate::{
	BlockInfoProvider,
	client::{Client, ClientError, GapFillRequest, SubscriptionGapQueue, SubstrateBlockNumber},
};
use jsonrpsee::core::async_trait;
use pallet_revive::evm::H256;
use std::time::Duration;
use tokio::sync::mpsc;

const LOG_TARGET: &str = "eth-rpc::block-sync";

/// Trait for types that can be used as keys in the `sync_state` table.
pub trait SyncStateKey: std::fmt::Display {}

/// Labels used to track sync progress in the `sync_state` table.
#[derive(Debug, Clone, Copy, derive_more::Display)]
pub enum SyncLabel {
	/// Lowest synced block. Only decreases.
	#[display(fmt = "sync-tail")]
	Tail,
	/// Highest synced block. Absent means no sync has started.
	/// During backfill: upper boundary being filled.
	/// After backfill: advanced by the finalized-block subscription.
	#[display(fmt = "sync-head")]
	Head,
}

/// Chain metadata stored in the `sync_state` table.
#[derive(Debug, Clone, Copy, derive_more::Display)]
pub enum ChainMetadata {
	/// Genesis block hash — used for chain identity verification.
	#[display(fmt = "chain-genesis")]
	Genesis,
	/// Auto-discovered first EVM block on the chain.
	#[display(fmt = "chain-first-evm-block")]
	FirstEvmBlock,
}

impl SyncStateKey for SyncLabel {}
impl SyncStateKey for ChainMetadata {}

/// Sync checkpoint persisted in the `sync_state` table to allow resuming after a restart.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct SyncCheckpoint {
	pub block_number: SubstrateBlockNumber,
	pub block_hash: Option<H256>,
}

impl SyncCheckpoint {
	/// Create a checkpoint with a known block hash.
	pub fn new(block_number: SubstrateBlockNumber, block_hash: H256) -> Self {
		Self { block_number, block_hash: Some(block_hash) }
	}

	/// Create a checkpoint with only a block number (no hash).
	pub fn from_number(block_number: SubstrateBlockNumber) -> Self {
		Self { block_number, block_hash: None }
	}
}

/// How often (in blocks) the backward sync checkpoints are persisted to the database.
const BLOCK_INTERVAL: u32 = 128;

/// Maximum number of attempts for a single subscription gap fill before it is left
/// pending (blocking head advance) instead of retried further.
const GAP_FILL_MAX_ATTEMPTS: u32 = 3;
/// Base backoff between gap-fill retries; doubled (capped) per attempt.
const GAP_FILL_BACKOFF: Duration = Duration::from_secs(2);

/// Retry policy for a single subscription gap fill.
#[derive(Clone, Copy)]
pub(crate) struct GapFillRetry {
	max_attempts: u32,
	backoff: Duration,
}

impl Default for GapFillRetry {
	fn default() -> Self {
		Self { max_attempts: GAP_FILL_MAX_ATTEMPTS, backoff: GAP_FILL_BACKOFF }
	}
}

impl GapFillRetry {
	/// Capped exponential backoff for the given (1-based) attempt number.
	fn backoff_for(&self, attempt: u32) -> Duration {
		let factor = 1u32 << attempt.saturating_sub(1).min(5);
		self.backoff * factor
	}
}

/// Options for [`SyncChainClient::sync_backward_range`].
pub(crate) struct BackwardSyncRange {
	from: SubstrateBlockNumber,
	to: SubstrateBlockNumber,
	/// Set `Head` label after syncing the first block.
	set_head: bool,
	/// Checkpoint `Tail` label periodically and at end.
	checkpoint_tail: bool,
	/// When true, persist the first EVM block boundary if a non-EVM block is encountered.
	persist_first_evm_block: bool,
}

/// A minimal, mockable view of a block for the backward-sync logic, decoupled from
/// subxt's `SubstrateBlock` (which cannot be constructed in tests).
pub(crate) trait SyncBlock {
	fn number(&self) -> SubstrateBlockNumber;
	fn hash(&self) -> H256;
	fn parent_hash(&self) -> H256;
}

/// The chain-data operations the backward-sync logic depends on.
///
/// Implemented by [`Client`] against the live node, and by a mock in tests so the sync
/// algorithm — the provided [`SyncChainClient::sync_backward_range`] and
/// [`SyncChainClient::run_gap_filler`] methods — can be exercised against injected
/// faults (e.g. a block fetch that fails mid-range).
#[async_trait]
pub(crate) trait SyncChainClient: Send + Sync {
	type Block: SyncBlock + Send + Sync;

	/// Fetch a block by its (substrate) number.
	async fn block_by_number(
		&self,
		number: SubstrateBlockNumber,
	) -> Result<Option<Self::Block>, ClientError>;

	/// Fetch a block by its (substrate) hash.
	async fn block_by_hash(&self, hash: H256) -> Result<Option<Self::Block>, ClientError>;

	/// The ethereum block hash for the given block, or `None` if it is a pre-EVM block.
	async fn eth_block_hash(&self, block: &Self::Block) -> Result<Option<H256>, ClientError>;

	/// Extract and persist the receipts for an already-known historic block.
	async fn insert_block_receipts(
		&self,
		block: &Self::Block,
		ethereum_hash: &H256,
	) -> Result<(), ClientError>;

	/// Persist the auto-discovered first EVM block boundary.
	async fn set_first_evm_block(&self, number: SubstrateBlockNumber) -> Result<(), ClientError>;

	/// Checkpoint the given sync label to the DB.
	async fn checkpoint_sync_label(
		&self,
		label: SyncLabel,
		number: SubstrateBlockNumber,
		hash: H256,
	);

	/// Backward sync from block `from` down to block `to` (inclusive).
	/// Stops early if a non-EVM block is discovered (auto-discovery of first EVM block).
	async fn sync_backward_range(&self, range: BackwardSyncRange) -> Result<(), ClientError> {
		let BackwardSyncRange { from, to, set_head, checkpoint_tail, persist_first_evm_block } =
			range;

		if from < to {
			log::debug!(target: LOG_TARGET, "⬇️ Backward sync: nothing to sync (#{from}..#{to})");
			return Ok(());
		}

		log::info!(target: LOG_TARGET, "⬇️ Backward sync: #{from} down to #{to}");

		let mut block = self.block_by_number(from).await?.ok_or(ClientError::BlockNotFound)?;

		let mut blocks_synced = 0u64;
		let mut last_synced: Option<(SubstrateBlockNumber, H256)> = None;
		let at_checkpoint =
			|synced: u64| synced <= 1 || synced.is_multiple_of(u64::from(BLOCK_INTERVAL));

		let loop_result: Result<(), ClientError> = loop {
			let block_number = block.number();
			let block_hash = block.hash();

			let ethereum_hash = match self.eth_block_hash(&block).await {
				Ok(h) => h,
				Err(err) => {
					log::error!(target: LOG_TARGET, "⚠️ eth_block_hash failed for #{block_number}: {err:?}, stopping");
					break Err(err);
				},
			};

			match ethereum_hash {
				Some(hash) => {
					if let Err(err) = self.insert_block_receipts(&block, &hash).await {
						log::error!(target: LOG_TARGET,
							"⚠️ Insert failed for #{block_number}: {err:?}, stopping");
						break Err(err);
					}

					last_synced = Some((block_number, block_hash));
					blocks_synced += 1;

					if blocks_synced == 1 && set_head {
						self.checkpoint_sync_label(SyncLabel::Head, block_number, block_hash).await;
					}

					if at_checkpoint(blocks_synced) {
						log::debug!(target: LOG_TARGET,
							"⬇️ Backward sync progress: #{block_number} ({blocks_synced} blocks synced)");
						if checkpoint_tail {
							self.checkpoint_sync_label(SyncLabel::Tail, block_number, block_hash)
								.await;
						}
					}
				},
				None => {
					if persist_first_evm_block {
						let first_evm_block = block_number.saturating_add(1);
						log::debug!(target: LOG_TARGET,
							"🔍 No EVM hash at #{block_number}, setting first_evm_block to #{first_evm_block}");
						if let Err(err) = self.set_first_evm_block(first_evm_block).await {
							log::warn!(target: LOG_TARGET, "Failed to persist first-evm-block: {err:?}");
						}
					} else {
						log::debug!(target: LOG_TARGET,
							"🔍 No EVM hash at #{block_number}, skipping first EVM block update");
					}

					break Ok(());
				},
			}

			if block_number > to {
				let parent_hash = block.parent_hash();
				match self
					.block_by_hash(parent_hash)
					.await
					.and_then(|opt| opt.ok_or(ClientError::BlockNotFound))
				{
					Ok(b) => block = b,
					Err(err) => {
						log::error!(target: LOG_TARGET,
							"⚠️ Could not fetch parent of #{block_number}: {err:?}, stopping");
						break Err(err);
					},
				}
			} else {
				break Ok(());
			}
		};

		// Checkpoint the last synced block if it wasn't already at a checkpoint interval.
		if loop_result.is_ok() && checkpoint_tail && !at_checkpoint(blocks_synced) {
			if let Some((num, hash)) = last_synced {
				self.checkpoint_sync_label(SyncLabel::Tail, num, hash).await;
			}
		}

		log::info!(target: LOG_TARGET,
			"⬇️ Backward sync: {blocks_synced} blocks synced (requested #{from}..#{to})");

		loop_result
	}

	/// Run the background subscription gap filler, processing requests sequentially.
	async fn run_gap_filler(
		&self,
		queue: &SubscriptionGapQueue,
		mut rx: mpsc::Receiver<GapFillRequest>,
	) {
		log::info!(target: LOG_TARGET, "🔄 Subscription gap filler started");

		while let Some(request) = rx.recv().await {
			self.process_gap_fill(queue, request, GapFillRetry::default()).await;
		}

		log::info!(target: LOG_TARGET, "🔄 Subscription gap filler stopped");
	}

	/// Process a single gap-fill request, retrying transient failures with backoff.
	///
	/// The request is marked done only once its range has been fully synced. A fill that
	/// still fails after `retry.max_attempts` is left pending so
	/// [`Client::advance_sync_head`] cannot advance the `Head` label past blocks that were
	/// never indexed — which would otherwise leave a permanent, silent hole (the server
	/// reporting itself fully synced over un-indexed blocks). Blocking head advance is
	/// preferable to silent data loss.
	async fn process_gap_fill(
		&self,
		queue: &SubscriptionGapQueue,
		request: GapFillRequest,
		retry: GapFillRetry,
	) {
		let GapFillRequest { from_inclusive, to_inclusive } = request;
		log::info!(target: LOG_TARGET, "🔄 Subscription gap filler: processing #{from_inclusive} down to #{to_inclusive}");

		let mut attempt = 1u32;
		loop {
			let result = self
				.sync_backward_range(BackwardSyncRange {
					from: from_inclusive,
					to: to_inclusive,
					set_head: false,
					checkpoint_tail: false,
					persist_first_evm_block: false,
				})
				.await;

			match result {
				Ok(()) => {
					log::info!(target: LOG_TARGET, "🔄 Subscription gap filler: done with #{from_inclusive}..#{to_inclusive}");
					queue.mark_done();
					return;
				},
				Err(err) if attempt >= retry.max_attempts => {
					log::error!(target: LOG_TARGET,
						"🔄 Subscription gap fill #{from_inclusive}..#{to_inclusive} failed after \
						 {attempt} attempt(s): {err:?}; leaving pending so the sync head cannot \
						 advance over un-indexed blocks");
					return;
				},
				Err(err) => {
					let backoff = retry.backoff_for(attempt);
					log::warn!(target: LOG_TARGET,
						"🔄 Subscription gap fill #{from_inclusive}..#{to_inclusive} attempt \
						 {attempt} failed: {err:?}; retrying in {backoff:?}");
					tokio::time::sleep(backoff).await;
					attempt = attempt.saturating_add(1);
				},
			}
		}
	}
}

impl Client {
	/// Verify that the stored genesis hash matches the connected chain.
	async fn validate_chain_identity(&self) -> Result<H256, ClientError> {
		let genesis_hash: H256 = self.api().genesis_hash();

		if let Some(checkpoint) =
			self.receipt_provider().get_sync_label(ChainMetadata::Genesis).await?
		{
			if let Some(stored) = checkpoint.block_hash {
				if stored != genesis_hash {
					return Err(ClientError::ChainMismatch);
				}
			}
		}

		Ok(genesis_hash)
	}

	/// Verify that a stored boundary block still exists on the finalized chain.
	async fn verify_boundary(&self, checkpoint: &SyncCheckpoint) -> Result<(), ClientError> {
		let num = checkpoint.block_number;
		let hash = checkpoint.block_hash;
		match (num, hash) {
			(_, None) => {
				log::error!(target: LOG_TARGET,
					"Boundary #{num}: missing stored hash");
				Err(ClientError::SyncBoundaryMismatch)
			},
			(_, Some(stored_hash)) => {
				let block = self.block_provider().block_by_number(num).await?.ok_or_else(|| {
					log::error!(target: LOG_TARGET,
						"Boundary #{num}: block not found on chain \
						 (node may have pruned it — use an archive node with --eth-pruning archive)");
					ClientError::SyncBoundaryMismatch
				})?;
				if block.hash() != stored_hash {
					log::error!(target: LOG_TARGET,
						"Boundary #{num}: hash mismatch — stored {stored_hash:?}, \
						 chain {:?}", block.hash());
					return Err(ClientError::SyncBoundaryMismatch);
				}
				Ok(())
			},
		}
	}

	/// Backward sync historical blocks from the latest finalized block to the first EVM block.
	/// Resumes from the last checkpoint if a previous sync was interrupted.
	/// Fatal errors (chain/DB mismatch) are propagated; transient errors are swallowed
	/// to avoid taking down the RPC server.
	pub async fn sync_backward(&self) -> Result<(), ClientError> {
		log::info!(target: LOG_TARGET,
			"🔄 Historical block sync enabled. \
			 For a complete sync, the connected node should be an archive node.");
		match self.sync_backward_inner().await {
			Ok(()) => Ok(()),
			Err(err) if err.is_chain_validation_error() => Err(err),
			Err(err) => {
				log::error!(target: LOG_TARGET, "🗄️ Sync stopped due to {err}.");
				Ok(())
			},
		}
	}

	async fn sync_backward_inner(&self) -> Result<(), ClientError> {
		let genesis_hash = self.validate_chain_identity().await?;
		let latest_finalized_block = self.latest_finalized_block().await;
		let latest_finalized =
			SyncCheckpoint::new(latest_finalized_block.number(), latest_finalized_block.hash());

		// Store genesis (idempotent).
		self.receipt_provider()
			.set_sync_label(ChainMetadata::Genesis, SyncCheckpoint::new(0, genesis_hash))
			.await?;

		let (head, tail) = tokio::try_join!(
			self.receipt_provider().get_sync_label(SyncLabel::Head),
			self.receipt_provider().get_sync_label(SyncLabel::Tail),
		)?;

		match (tail, head) {
			(Some(tail), Some(head)) => {
				// Verify boundary hashes still match the finalized chain.
				tokio::try_join!(self.verify_boundary(&tail), self.verify_boundary(&head),)?;
				self.sync_backward_resume(tail, head, latest_finalized).await?;
			},
			(Some(_), None) => {
				log::warn!(target: LOG_TARGET,
					"🗄️ Tail exists without Head — possible partial corruption, \
					 starting fresh sync from #{}", latest_finalized.block_number);
				self.sync_backward_fresh(latest_finalized.block_number).await?;
			},
			_ => {
				log::info!(target: LOG_TARGET,
					"🗄️ Fresh sync: syncing backward from #{}", latest_finalized.block_number);
				self.sync_backward_fresh(latest_finalized.block_number).await?;
			},
		}

		self.mark_backfill_complete();

		log::info!(target: LOG_TARGET, "🗄️ Historic sync complete");
		Ok(())
	}

	/// Backward sync from `latest_finalized` down to the first EVM block.
	async fn sync_backward_fresh(
		&self,
		latest_finalized: SubstrateBlockNumber,
	) -> Result<(), ClientError> {
		let first_evm = self.receipt_provider().first_evm_block().unwrap_or(0);
		self.sync_backward_range(BackwardSyncRange {
			from: latest_finalized,
			to: first_evm,
			set_head: true,
			checkpoint_tail: true,
			persist_first_evm_block: true,
		})
		.await
	}

	/// Resume backward sync by filling the top gap (new blocks) and bottom gap (backfill).
	async fn sync_backward_resume(
		&self,
		tail: SyncCheckpoint,
		head: SyncCheckpoint,
		latest_finalized: SyncCheckpoint,
	) -> Result<(), ClientError> {
		log::info!(target: LOG_TARGET,
			"🗄️ Resuming sync: DB has blocks #{}..#{}, chain head is #{}",
			tail.block_number, head.block_number, latest_finalized.block_number);

		let top_gap = async {
			// Top gap: sync from latest_finalized down to head + 1.
			if head.block_number < latest_finalized.block_number {
				self.sync_backward_range(BackwardSyncRange {
					from: latest_finalized.block_number,
					to: head.block_number.saturating_add(1),
					set_head: false,
					checkpoint_tail: false,
					persist_first_evm_block: false,
				})
				.await?;

				// Mark top gap complete so a restart won't redo it.
				self.receipt_provider()
					.advance_sync_label(SyncLabel::Head, latest_finalized)
					.await?;
			}
			Ok::<_, ClientError>(())
		};

		let bottom_gap = async {
			// Bottom gap: sync from tail - 1 down to the first EVM block.
			let first_evm = self.receipt_provider().first_evm_block().unwrap_or(0);
			if tail.block_number > first_evm {
				self.sync_backward_range(BackwardSyncRange {
					from: tail.block_number.saturating_sub(1),
					to: first_evm,
					set_head: false,
					checkpoint_tail: true,
					persist_first_evm_block: true,
				})
				.await?;
			} else {
				log::debug!(target: LOG_TARGET, "🗄️ No backward gap to fill");
			}
			Ok::<_, ClientError>(())
		};

		tokio::try_join!(top_gap, bottom_gap)?;

		Ok(())
	}

	/// Run the background subscription gap filler, processing requests sequentially.
	///
	/// Thin wrapper around the generic [`SyncChainClient::run_gap_filler`] so the loop
	/// can be exercised against a mock chain in tests.
	pub(crate) async fn run_subscription_gap_filler(&self, rx: mpsc::Receiver<GapFillRequest>) {
		self.run_gap_filler(self.subscription_gap_queue(), rx).await;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::client::SubscriptionGapQueue;
	use std::sync::{
		atomic::{AtomicUsize, Ordering},
		Mutex,
	};

	// Deterministic, reversible number<->hash mapping (offset by 1 to avoid the zero hash).
	fn hash_for(n: SubstrateBlockNumber) -> H256 {
		H256::from_low_u64_be(n as u64 + 1)
	}
	fn number_for(hash: H256) -> SubstrateBlockNumber {
		hash.to_low_u64_be().saturating_sub(1) as SubstrateBlockNumber
	}

	#[derive(Clone)]
	struct MockBlock {
		number: SubstrateBlockNumber,
	}
	impl SyncBlock for MockBlock {
		fn number(&self) -> SubstrateBlockNumber {
			self.number
		}
		fn hash(&self) -> H256 {
			hash_for(self.number)
		}
		fn parent_hash(&self) -> H256 {
			hash_for(self.number.saturating_sub(1))
		}
	}

	/// A mock chain whose parent-block fetch fails for blocks below `fail_below`. The
	/// first `fails_remaining` such fetches error (simulating a pruned / transiently
	/// unavailable block); once that budget is exhausted the fetch succeeds, modelling a
	/// transient failure that recovers on retry. Use `usize::MAX` for a permanent failure.
	struct MockChainClient {
		fail_below: SubstrateBlockNumber,
		fails_remaining: AtomicUsize,
		inserted: Mutex<Vec<SubstrateBlockNumber>>,
	}

	#[async_trait]
	impl SyncChainClient for MockChainClient {
		type Block = MockBlock;

		async fn block_by_number(
			&self,
			number: SubstrateBlockNumber,
		) -> Result<Option<Self::Block>, ClientError> {
			Ok(Some(MockBlock { number }))
		}

		async fn block_by_hash(&self, hash: H256) -> Result<Option<Self::Block>, ClientError> {
			let number = number_for(hash);
			// Fail sub-threshold fetches while the failure budget lasts; `fetch_update`
			// returns Ok only when it actually decremented (i.e. budget was > 0).
			let should_fail = number < self.fail_below &&
				self.fails_remaining
					.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
					.is_ok();
			if should_fail {
				return Err(ClientError::BlockNotFound);
			}
			Ok(Some(MockBlock { number }))
		}

		async fn eth_block_hash(&self, block: &Self::Block) -> Result<Option<H256>, ClientError> {
			Ok(Some(hash_for(block.number)))
		}

		async fn insert_block_receipts(
			&self,
			block: &Self::Block,
			_ethereum_hash: &H256,
		) -> Result<(), ClientError> {
			self.inserted.lock().unwrap().push(block.number);
			Ok(())
		}

		async fn set_first_evm_block(
			&self,
			_number: SubstrateBlockNumber,
		) -> Result<(), ClientError> {
			Ok(())
		}

		async fn checkpoint_sync_label(
			&self,
			_label: SyncLabel,
			_number: SubstrateBlockNumber,
			_hash: H256,
		) {
		}
	}

	// No retry policy in tests, so failures aren't slowed by real backoff.
	fn no_backoff(max_attempts: u32) -> GapFillRetry {
		GapFillRetry { max_attempts, backoff: Duration::ZERO }
	}

	// A gap fill that keeps failing after all attempts must NOT be marked done: the lower
	// blocks of the range were never indexed, so if `pending` dropped to 0,
	// `advance_sync_head` would advance `Head` past those un-indexed blocks, leaving a
	// permanent silent hole. The exhausted request stays pending so the head cannot
	// advance.
	#[tokio::test]
	async fn exhausted_gap_fill_stays_pending() {
		let (queue, _rx) = SubscriptionGapQueue::new();
		// Queue a gap fill for #10 down to #5 (pending = 1).
		queue.detect_and_queue(11, 4);
		assert!(queue.has_pending(), "precondition: one gap fill queued");

		// Parent fetch permanently fails at #7, so the range errors after indexing #10..#8.
		let mock = MockChainClient {
			fail_below: 8,
			fails_remaining: AtomicUsize::new(usize::MAX),
			inserted: Mutex::new(vec![]),
		};

		// One attempt, no retry — models the range still failing after the budget.
		mock.process_gap_fill(
			&queue,
			GapFillRequest { from_inclusive: 10, to_inclusive: 5 },
			no_backoff(1),
		)
		.await;

		// Only #10,#9,#8 were indexed before the failure — the range is incomplete...
		assert_eq!(*mock.inserted.lock().unwrap(), vec![10, 9, 8]);
		// ...so the request must stay pending (not marked done), blocking head advance.
		assert!(
			queue.has_pending(),
			"an exhausted gap fill must remain pending so Head cannot advance over un-indexed blocks"
		);
	}

	// A transient failure is retried; once it succeeds the request is marked done so the
	// head is free to advance.
	#[tokio::test]
	async fn transient_gap_fill_retries_then_succeeds() {
		let (queue, _rx) = SubscriptionGapQueue::new();
		queue.detect_and_queue(11, 4);
		assert!(queue.has_pending(), "precondition: one gap fill queued");

		// The parent fetch below #8 fails exactly once, then succeeds on retry.
		let mock = MockChainClient {
			fail_below: 8,
			fails_remaining: AtomicUsize::new(1),
			inserted: Mutex::new(vec![]),
		};

		mock.process_gap_fill(
			&queue,
			GapFillRequest { from_inclusive: 10, to_inclusive: 5 },
			no_backoff(3),
		)
		.await;

		// The retry completed the range, so the request is marked done...
		assert!(
			!queue.has_pending(),
			"a gap fill that succeeds on retry must be marked done so Head can advance"
		);
		// ...and the full range down to #5 was eventually indexed.
		assert!(mock.inserted.lock().unwrap().contains(&5), "the full range should be indexed");
	}
}
