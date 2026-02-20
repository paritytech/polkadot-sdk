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
//! Historic block syncing logic for the Ethereum-compatible RPC layer.

use crate::{
	BlockInfoProvider,
	client::{Client, ClientError, LOG_TARGET, SubstrateBlockNumber},
};
use pallet_revive::evm::H256;
use std::{future::Future, pin::Pin};

/// Labels used to track sync progress in the `sync_state` table.
#[derive(Debug, Clone, Copy)]
pub enum SyncLabel {
	/// Genesis block hash — used for chain identity verification.
	Genesis,
	/// Lowest block synced by the historic sync.
	/// After sync completes, this equals the first EVM block (or genesis).
	LowerBound,
	/// Highest block synced when the historic sync started.
	/// Non-zero means sync is in progress (or was interrupted).
	/// Zero (or absent) means sync completed successfully.
	UpperBound,
	/// Latest finalized block, tracked by the live subscription.
	Finalized,
}

impl SyncLabel {
	/// The string stored in the database `label` column.
	pub fn as_str(&self) -> &'static str {
		match self {
			Self::Genesis => "genesis",
			Self::LowerBound => "sync-lower-bound",
			Self::UpperBound => "sync-upper-bound",
			Self::Finalized => "finalized",
		}
	}
}

impl std::fmt::Display for SyncLabel {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(self.as_str())
	}
}

/// A stored sync checkpoint: block number + optional block hash.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SyncCheckpoint {
	pub block_number: SubstrateBlockNumber,
	pub block_hash: Option<H256>,
}

/// How often (in blocks) the backward sync checkpoints its progress to the database.
const SYNC_CHECKPOINT_INTERVAL: u64 = 1000;

/// The future type returned by sync hook callbacks.
type SyncHookFuture<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

/// Concrete type used to pass `None` for optional sync hooks without verbose turbofish.
type NoopSyncHook = fn(SubstrateBlockNumber, H256) -> SyncHookFuture<'static>;

impl Client {
	/// Verify that the stored genesis hash matches the connected chain.
	async fn validate_chain_identity(&self) -> Result<H256, ClientError> {
		let genesis_hash: H256 = self.api.genesis_hash();

		if let Some(checkpoint) = self.receipt_provider.get_sync_state(SyncLabel::Genesis).await? {
			if let Some(stored) = checkpoint.block_hash {
				if stored != genesis_hash {
					return Err(ClientError::ChainMismatch);
				}
			}
		}

		Ok(genesis_hash)
	}

	/// Verify that a stored boundary block still exists on the finalized chain.
	async fn verify_boundary(
		&self,
		num: SubstrateBlockNumber,
		hash: Option<H256>,
	) -> Result<(), ClientError> {
		if let Some(stored_hash) = hash {
			let block = self
				.block_provider
				.block_by_number(num)
				.await?
				.ok_or(ClientError::BlockNotFound)?;
			if block.hash() != stored_hash {
				return Err(ClientError::SyncBoundaryMismatch);
			}
		}
		Ok(())
	}

	/// Returns the upper bound of contiguous DB coverage.
	/// Must be called before subscriptions start.
	pub async fn prepare_sync(&self) -> Result<Option<SyncCheckpoint>, ClientError> {
		let history_start = self.receipt_provider.get_sync_state(SyncLabel::UpperBound).await?;

		if let Some(checkpoint) = &history_start {
			if checkpoint.block_number > 0 {
				log::info!(target: LOG_TARGET,
					"🗄️ Previous sync was interrupted, safe boundary at #{}", checkpoint.block_number);
				return Ok(history_start);
			}
		}

		let finalized = self.receipt_provider.get_sync_state(SyncLabel::Finalized).await?;
		if let Some(checkpoint) = &finalized {
			log::info!(target: LOG_TARGET,
				"🗄️ Pinned sync boundary at finalized #{}", checkpoint.block_number);
		}
		Ok(finalized)
	}

	/// Syncs all historical blocks down to genesis.
	/// Resumes from where it left off if possible, otherwise starts a fresh backward sync.
	pub async fn sync_historic_blocks(
		&self,
		synced_upper_boundary: Option<SyncCheckpoint>,
	) -> Result<(), ClientError> {
		let genesis_hash = self.validate_chain_identity().await?;
		let latest = self.latest_finalized_block().await.number().saturating_sub(1);

		// Store genesis (idempotent).
		self.receipt_provider
			.set_sync_state(SyncLabel::Genesis, 0, Some(genesis_hash))
			.await?;

		let synced_lower_boundary =
			self.receipt_provider.get_sync_state(SyncLabel::LowerBound).await?;

		match (synced_lower_boundary, synced_upper_boundary) {
			(Some(first), Some(upper)) => {
				// Verify boundary hashes still match the finalized chain.
				self.verify_boundary(first.block_number, first.block_hash).await?;
				self.verify_boundary(upper.block_number, upper.block_hash).await?;
				self.resume_sync(first, upper, latest).await?;
			},
			_ => {
				log::info!(target: LOG_TARGET,
					"🗄️ Fresh sync: syncing backward from #{latest}");
				self.fresh_sync(latest).await?;
			},
		}

		// Reset — signals that the sync completed successfully.
		self.receipt_provider.set_sync_state(SyncLabel::UpperBound, 0, None).await?;

		log::info!(target: LOG_TARGET, "🗄️ Historic sync complete");
		Ok(())
	}

	/// Fresh sync: backward from `latest` down to `--earliest-receipt-block` (or genesis).
	/// Sets `UpperBound` via hook after the first block is synced.
	async fn fresh_sync(&self, latest: SubstrateBlockNumber) -> Result<(), ClientError> {
		let lower_bound = self.receipt_provider.earliest_receipt_block().unwrap_or(0);
		self.sync_backward(
			latest,
			lower_bound,
			Some(self.set_sync_upper_bound_hook()),
			Some(self.checkpoint_lower_bound_hook()),
		)
		.await?;
		Ok(())
	}

	/// Resume sync by filling the top gap (new blocks) and bottom gap (backfill).
	async fn resume_sync(
		&self,
		first: SyncCheckpoint,
		upper: SyncCheckpoint,
		latest: SubstrateBlockNumber,
	) -> Result<(), ClientError> {
		// Mark sync in-progress. On crash, this value is the safe upper boundary.
		self.receipt_provider
			.set_sync_state(SyncLabel::UpperBound, upper.block_number, upper.block_hash)
			.await?;

		log::info!(target: LOG_TARGET,
			"🗄️ Resuming sync: DB has blocks #{}..#{}, \
			 chain head is #{latest}", first.block_number, upper.block_number);

		// Top gap: [finalized + 1, latest]
		if upper.block_number < latest {
			self.sync_backward(
				latest,
				upper.block_number + 1,
				None::<NoopSyncHook>,
				None::<NoopSyncHook>,
			)
			.await?;
		}

		// Bottom gap: backfill down to earliest-receipt-block (or genesis)
		let lower_bound = self.receipt_provider.earliest_receipt_block().unwrap_or(0);
		if first.block_number > lower_bound {
			self.sync_backward(
				first.block_number - 1,
				lower_bound,
				None::<NoopSyncHook>,
				Some(self.checkpoint_lower_bound_hook()),
			)
			.await?;
		} else {
			log::info!(target: LOG_TARGET, "🗄️ No backward gap to fill");
		}

		Ok(())
	}

	/// Hook that sets `SyncLabel::UpperBound` once the first block is in the DB.
	fn set_sync_upper_bound_hook<'a>(
		&'a self,
	) -> impl Fn(SubstrateBlockNumber, H256) -> SyncHookFuture<'a> + 'a {
		move |num, hash| {
			Box::pin(async move {
				if let Err(err) = self
					.receipt_provider
					.set_sync_state(SyncLabel::UpperBound, num, Some(hash))
					.await
				{
					log::warn!(target: LOG_TARGET,
						"Failed to set sync_state[upper-bound]: {err:?}");
				}
			})
		}
	}

	/// Hook that checkpoints `SyncLabel::LowerBound`, only decreasing.
	fn checkpoint_lower_bound_hook<'a>(
		&'a self,
	) -> impl Fn(SubstrateBlockNumber, H256) -> SyncHookFuture<'a> + 'a {
		move |num, hash| {
			Box::pin(async move {
				if let Err(err) = self
					.receipt_provider
					.recede_sync_state(SyncLabel::LowerBound, num, Some(hash))
					.await
				{
					log::warn!(target: LOG_TARGET,
						"Failed to checkpoint sync_state[lower-bound]: {err:?}");
				}
			})
		}
	}

	/// Backward sync from `from` down to `lower_bound` (inclusive).
	///
	/// - `on_first_block`: called once after syncing the first block.
	/// - `on_progress`: called at first block, every `SYNC_CHECKPOINT_INTERVAL` blocks, and at end.
	async fn sync_backward<'a, 'b>(
		&self,
		from: SubstrateBlockNumber,
		lower_bound: SubstrateBlockNumber,
		on_first_block: Option<impl Fn(SubstrateBlockNumber, H256) -> SyncHookFuture<'a>>,
		on_progress: Option<impl Fn(SubstrateBlockNumber, H256) -> SyncHookFuture<'b>>,
	) -> Result<(), ClientError> {
		log::info!(target: LOG_TARGET,
			"⬇️ Backward sync: #{from} down to #{lower_bound}");

		let mut block = self
			.block_provider
			.block_by_number(from)
			.await?
			.ok_or(ClientError::BlockNotFound)?;

		let mut blocks_synced = 0u64;
		let mut last_synced: Option<(SubstrateBlockNumber, H256)> = None;
		let at_checkpoint = |synced: u64| synced <= 1 || synced % SYNC_CHECKPOINT_INTERVAL == 0;

		let loop_result: Result<(), ClientError> = loop {
			let block_number = block.number();
			let block_hash = block.hash();

			let ethereum_hash = match self
				.runtime_api(block_hash)
				.eth_block_hash(pallet_revive::evm::U256::from(block_number))
				.await
			{
				Ok(h) => h,
				Err(err) => {
					log::error!(target: LOG_TARGET,
						"⚠️ eth_block_hash failed for #{block_number}: {err:?}, stopping");
					break Err(err.into());
				},
			};

			match ethereum_hash {
				Some(hash) => {
					if let Err(err) =
						self.receipt_provider.insert_block_receipts(&block, &hash).await
					{
						log::error!(target: LOG_TARGET,
							"⚠️ Insert failed for #{block_number}: {err:?}, stopping");
						break Err(err);
					}

					last_synced = Some((block_number, block_hash));
					blocks_synced += 1;

					if blocks_synced == 1 {
						if let Some(ref f) = on_first_block {
							f(block_number, block_hash).await;
						}
					}

					if at_checkpoint(blocks_synced) {
						log::info!(target: LOG_TARGET,
							"⬇️ Backward sync progress: #{block_number} \
								({blocks_synced} blocks synced)");

						if let Some(ref f) = on_progress {
							f(block_number, block_hash).await;
						}
					}
				},
				None => {
					let first_evm = block_number.saturating_add(1);
					log::info!(target: LOG_TARGET,
						"🔍 Auto-discovered first EVM block: #{first_evm}");
					self.receipt_provider.update_earliest_receipt_block(first_evm);
					break Ok(());
				},
			}

			if lower_bound < block_number {
				let parent_hash = block.header().parent_hash;
				match self
					.block_provider
					.block_by_hash(&parent_hash)
					.await
					.map_err(Into::into)
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

		// Flush un-checkpointed progress regardless of success/failure.
		if !at_checkpoint(blocks_synced) {
			if let Some((num, hash)) = last_synced {
				if let Some(ref f) = on_progress {
					f(num, hash).await;
				}
			}
		}

		log::info!(target: LOG_TARGET,
			"⬇️ Backward sync: {blocks_synced} blocks synced \
			 (requested #{from}..#{lower_bound})");

		loop_result
	}
}
