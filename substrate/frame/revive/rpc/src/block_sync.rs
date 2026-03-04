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
	client::{Client, ClientError, SubstrateBlockNumber},
};
use pallet_revive::evm::H256;
use std::{future::Future, pin::Pin};

const LOG_TARGET: &str = "eth-rpc::block-sync";

/// Trait for types that can be used as keys in the `sync_state` table.
pub trait SyncStateKey: std::fmt::Display {}

/// Labels used to track sync progress in the `sync_state` table.
#[derive(Debug, Clone, Copy, derive_more::Display)]
pub enum SyncLabel {
	/// Lowest block synced by the historic sync.
	#[display(fmt = "sync-lower-bound")]
	LowerBound,
	/// Upper boundary of contiguous DB coverage, used to resume sync after a crash.
	/// Non-zero means sync is in progress (or was interrupted).
	/// Zero means sync completed; absent means sync never started.
	#[display(fmt = "sync-upper-bound")]
	UpperBound,
	/// Latest finalized block, tracked by the live subscription.
	#[display(fmt = "sync-finalized")]
	LastFinalized,
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

/// Sync checkpoint persisted in the `sync_state` table to allow resuming after a crash.
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

/// How often (in blocks) the backward historic sync checkpoints progress to the database.
pub(crate) const SYNC_CHECKPOINT_INTERVAL: u32 = 128;

/// The future type returned by sync hook callbacks.
type SyncHookFuture<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

/// Concrete type used to pass `None` for optional sync hooks without verbose turbofish.
type NoopSyncHook = fn(SubstrateBlockNumber, H256) -> SyncHookFuture<'static>;

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
	async fn verify_boundary(
		&self,
		num: SubstrateBlockNumber,
		hash: Option<H256>,
	) -> Result<(), ClientError> {
		match (num, hash) {
			(0, None) => {
				log::trace!(target: LOG_TARGET, "Boundary #{num}: genesis with no hash, OK");
				Ok(())
			},
			(_, None) => {
				log::error!(target: LOG_TARGET,
					"Boundary #{num}: non-genesis block has no stored hash");
				Err(ClientError::SyncBoundaryMismatch)
			},
			(_, Some(stored_hash)) => {
				let block = self.block_provider().block_by_number(num).await?.ok_or_else(|| {
					log::error!(target: LOG_TARGET,
							"Boundary #{num}: block not found on chain");
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

	/// Returns the upper bound of contiguous DB coverage.
	/// Must be called before subscriptions start.
	pub async fn prepare_sync(&self) -> Result<Option<SyncCheckpoint>, ClientError> {
		let Some(checkpoint) =
			self.receipt_provider().get_sync_label(SyncLabel::UpperBound).await?
		else {
			// No UpperBound row — fresh DB.
			return Ok(None);
		};

		// Interrupted sync — resume from where it left off.
		if checkpoint.block_number > 0 {
			log::info!(target: LOG_TARGET,
				"🗄️ Previous sync was interrupted, resuming from upper bound #{}",
				checkpoint.block_number);
			return Ok(Some(checkpoint));
		}

		// UpperBound=0 means a previous sync completed successfully.
		// Use LastFinalized as the new upper bound to sync blocks
		// produced since the last completed sync.
		let finalized = self.receipt_provider().get_sync_label(SyncLabel::LastFinalized).await?;
		if let Some(fin) = &finalized {
			log::info!(target: LOG_TARGET,
				"🗄️ Previous sync completed, using last finalized #{} as upper bound",
				fin.block_number);
		}
		Ok(finalized)
	}

	/// Sync historical blocks backward from `upper_boundary` to the first EVM block.
	/// Fatal errors (chain/DB mismatch) are propagated; transient errors are swallowed
	/// to avoid crashing the RPC server.
	pub async fn sync_past_blocks(
		&self,
		upper_boundary: Option<SyncCheckpoint>,
	) -> Result<(), ClientError> {
		match self.sync_past_blocks_inner(upper_boundary).await {
			Ok(()) => Ok(()),
			Err(err) if err.is_chain_validation_error() => Err(err),
			Err(err) => {
				log::error!(target: LOG_TARGET,
					"🗄️ Sync stopped due to {err}.");
				Ok(())
			},
		}
	}

	async fn sync_past_blocks_inner(
		&self,
		upper_boundary: Option<SyncCheckpoint>,
	) -> Result<(), ClientError> {
		let genesis_hash = self.validate_chain_identity().await?;
		let latest_finalized_block = self.latest_finalized_block().await;
		let latest_finalized =
			SyncCheckpoint::new(latest_finalized_block.number(), latest_finalized_block.hash());

		// Store genesis (idempotent).
		self.receipt_provider()
			.set_sync_label(ChainMetadata::Genesis, SyncCheckpoint::new(0, genesis_hash))
			.await?;

		let lower_boundary = self.receipt_provider().get_sync_label(SyncLabel::LowerBound).await?;

		match (lower_boundary, upper_boundary) {
			(Some(lower), Some(upper)) => {
				// Verify boundary hashes still match the finalized chain.
				self.verify_boundary(lower.block_number, lower.block_hash).await?;
				self.verify_boundary(upper.block_number, upper.block_hash).await?;
				self.resume_sync(lower, upper, latest_finalized).await?;
			},
			(Some(_), None) => {
				log::warn!(target: LOG_TARGET,
					"🗄️ LowerBound exists without UpperBound — possible partial corruption, \
					 starting fresh sync from #{}", latest_finalized.block_number);
				self.fresh_sync(latest_finalized.block_number).await?;
			},
			_ => {
				log::info!(target: LOG_TARGET,
					"🗄️ Fresh sync: syncing backward from #{}", latest_finalized.block_number);
				self.fresh_sync(latest_finalized.block_number).await?;
			},
		}

		// Clear UpperBound to mark sync as complete.
		self.receipt_provider()
			.set_sync_label(SyncLabel::UpperBound, SyncCheckpoint::from_number(0))
			.await?;

		log::info!(target: LOG_TARGET, "🗄️ Historic sync complete");
		Ok(())
	}

	/// Fresh sync: backward from `latest_finalized` down to the first EVM block.
	/// Registers hooks to set `UpperBound` on the first synced block and checkpoint `LowerBound`.
	async fn fresh_sync(&self, latest_finalized: SubstrateBlockNumber) -> Result<(), ClientError> {
		let lower_bound = self.receipt_provider().first_evm_block().unwrap_or(0);
		self.sync_backward(
			latest_finalized,
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
		db_lower_bound: SyncCheckpoint,
		db_upper_bound: SyncCheckpoint,
		latest_finalized: SyncCheckpoint,
	) -> Result<(), ClientError> {
		// Mark sync in-progress. On crash, this value is the safe upper boundary.
		self.receipt_provider()
			.set_sync_label(SyncLabel::UpperBound, db_upper_bound)
			.await?;

		log::info!(target: LOG_TARGET,
			"🗄️ Resuming sync: DB has blocks #{}..#{}, chain head is #{}",
			db_lower_bound.block_number, db_upper_bound.block_number, latest_finalized.block_number);

		// Top gap: sync from latest_finalized down to db_upper_bound + 1.
		if db_upper_bound.block_number < latest_finalized.block_number {
			self.sync_backward(
				latest_finalized.block_number,
				db_upper_bound.block_number.saturating_add(1),
				None::<NoopSyncHook>,
				None::<NoopSyncHook>,
			)
			.await?;

			// Mark top gap complete so a crash during the bottom gap won't redo it.
			self.receipt_provider()
				.set_sync_label(SyncLabel::UpperBound, latest_finalized)
				.await?;
		}

		// Bottom gap: sync from db_lower_bound - 1 down to the first EVM block.
		let earliest_block = self.receipt_provider().first_evm_block().unwrap_or(0);
		if db_lower_bound.block_number > earliest_block {
			self.sync_backward(
				db_lower_bound.block_number.saturating_sub(1),
				earliest_block,
				None::<NoopSyncHook>,
				Some(self.checkpoint_lower_bound_hook()),
			)
			.await?;
		} else {
			log::debug!(target: LOG_TARGET, "🗄️ No backward gap to fill");
		}

		Ok(())
	}

	/// Hook that sets `SyncLabel::UpperBound` once the first block is in the DB.
	fn set_sync_upper_bound_hook<'a>(
		&'a self,
	) -> impl Fn(SubstrateBlockNumber, H256) -> SyncHookFuture<'a> + 'a {
		move |num, hash| {
			Box::pin(async move {
				let cp = SyncCheckpoint::new(num, hash);
				if let Err(err) =
					self.receipt_provider().set_sync_label(SyncLabel::UpperBound, cp).await
				{
					log::warn!(target: LOG_TARGET,
						"Failed to set sync_label[{}]: {err:?}", SyncLabel::UpperBound);
				}
			})
		}
	}

	/// Hook that checkpoints `SyncLabel::LowerBound` to DB, only decreasing.
	fn checkpoint_lower_bound_hook<'a>(
		&'a self,
	) -> impl Fn(SubstrateBlockNumber, H256) -> SyncHookFuture<'a> + 'a {
		move |num, hash| {
			Box::pin(async move {
				let cp = SyncCheckpoint::new(num, hash);
				if let Err(err) =
					self.receipt_provider().recede_sync_label(SyncLabel::LowerBound, cp).await
				{
					log::warn!(target: LOG_TARGET,
						"Failed to checkpoint sync_label[{}]: {err:?}", SyncLabel::LowerBound);
				}
			})
		}
	}

	/// Backward sync from `upper` down to `lower` (inclusive).
	/// Stops early if a non-EVM block is discovered (auto-discovery of first EVM block).
	///
	/// - `on_first_block`: called once after syncing the first block.
	/// - `on_progress`: called at first block, every `SYNC_CHECKPOINT_INTERVAL` blocks, and at end.
	async fn sync_backward<'a, 'b>(
		&self,
		upper: SubstrateBlockNumber,
		lower: SubstrateBlockNumber,
		on_first_block: Option<impl Fn(SubstrateBlockNumber, H256) -> SyncHookFuture<'a>>,
		on_progress: Option<impl Fn(SubstrateBlockNumber, H256) -> SyncHookFuture<'b>>,
	) -> Result<(), ClientError> {
		log::info!(target: LOG_TARGET,
			"⬇️ Backward sync: #{upper} down to #{lower}");

		if upper < lower {
			log::debug!(target: LOG_TARGET,
				"⬇️ Backward sync: upper < lower, nothing to sync");
			return Ok(());
		}

		let mut block = self
			.block_provider()
			.block_by_number(upper)
			.await?
			.ok_or(ClientError::BlockNotFound)?;

		let mut blocks_synced = 0u64;
		let mut last_synced: Option<(SubstrateBlockNumber, H256)> = None;
		let at_checkpoint =
			|synced: u64| synced <= 1 || synced.is_multiple_of(u64::from(SYNC_CHECKPOINT_INTERVAL));

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
						self.receipt_provider().insert_block_receipts_past(&block, &hash).await
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
						log::debug!(target: LOG_TARGET,
							"⬇️ Backward sync progress: #{block_number} \
								({blocks_synced} blocks synced)");

						if let Some(ref f) = on_progress {
							f(block_number, block_hash).await;
						}
					}
				},
				None => {
					let first_evm_block = block_number.saturating_add(1);
					log::debug!(target: LOG_TARGET,
						"🔍 No EVM hash at #{block_number}, setting first_evm_block to #{first_evm_block}");
					if let Err(err) =
						self.receipt_provider().set_first_evm_block(first_evm_block).await
					{
						log::warn!(target: LOG_TARGET,
							"Failed to persist first-evm-block: {err:?}");
					}

					break Ok(());
				},
			}

			if block_number > lower {
				let parent_hash = block.header().parent_hash;
				match self
					.block_provider()
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

		// Checkpoint the last synced block if it wasn't already at a checkpoint interval.
		if !at_checkpoint(blocks_synced) {
			if let Some((num, hash)) = last_synced {
				if let Some(ref f) = on_progress {
					f(num, hash).await;
				}
			}
		}

		log::info!(target: LOG_TARGET,
			"⬇️ Backward sync: {blocks_synced} blocks synced \
			 (requested #{upper}..#{lower})");

		loop_result
	}
}
