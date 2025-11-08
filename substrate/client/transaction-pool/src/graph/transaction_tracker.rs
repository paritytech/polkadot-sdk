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

use crate::TransactionReceiptDb;
use sp_runtime::traits::Block as BlockT;
use std::{
	collections::{HashMap, HashSet},
	sync::Arc,
};
use tokio::sync::RwLock;

/// Transaction status in the tracking system
#[derive(Debug, Clone, PartialEq)]
pub enum TrackedTransactionStatus<BlockHash> {
	/// Transaction is in the pool (pending)
	Pending,
	/// Transaction is included in a block
	InBlock { block_hash: BlockHash, block_number: u64, index: usize },
	/// Transaction is finalized
	Finalized { block_hash: BlockHash, block_number: u64, index: usize },
	/// Transaction was dropped from the pool
	Dropped,
	/// Transaction was found to be invalid
	Invalid,
}

/// Transaction tracking information
#[derive(Debug, Clone)]
pub struct TransactionInfo<BlockHash, TxHash> {
	pub status: TrackedTransactionStatus<BlockHash>,
	pub block_hash: Option<BlockHash>,
	pub events: Vec<sc_transaction_pool_api::TransactionStatus<TxHash, BlockHash>>,
	pub submitted_at: std::time::Instant,
}

/// Main transaction tracker
pub struct TransactionTracker<Block: BlockT> {
	/// Map transaction hash -> block hash
	tx_to_block: RwLock<HashMap<Block::Hash, Block::Hash>>,
	/// Map transaction hash -> tracking info
	tx_info: RwLock<HashMap<Block::Hash, TransactionInfo<Block::Hash, Block::Hash>>>,
	/// Map block hash -> transaction hashes in that block
	block_to_txs: RwLock<HashMap<Block::Hash, HashSet<Block::Hash>>>,
	receipt_db: Option<Arc<TransactionReceiptDb>>,
}

impl<Block: BlockT> TransactionTracker<Block> {
	pub fn new(receipt_db: Option<Arc<TransactionReceiptDb>>) -> Self {
		Self {
			tx_to_block: RwLock::new(HashMap::new()),
			tx_info: RwLock::new(HashMap::new()),
			block_to_txs: RwLock::new(HashMap::new()),
			receipt_db,
		}
	}

	/// Add a transaction to the pool (pending status) and track in database
	pub async fn add_pending_transaction(&self, tx_hash: Block::Hash) {
		let mut tx_info_map = self.tx_info.write().await;
		tx_info_map.insert(
			tx_hash,
			TransactionInfo {
				status: TrackedTransactionStatus::Pending,
				block_hash: None,
				events: vec![sc_transaction_pool_api::TransactionStatus::Ready],
				submitted_at: std::time::Instant::now(),
			},
		);

		// Track in database if available
		if let Some(db) = &self.receipt_db {
			let tx_hash_str = format!("{:?}", tx_hash);
			let submitted_at = std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap()
				.as_millis() as u64;

			if let Err(e) = db.add_pending_transaction(&tx_hash_str, submitted_at).await {
				log::warn!("Failed to track pending transaction {}: {}", tx_hash_str, e);
			}
		}
	}

	/// Update transaction status when included in a block and track in database
	pub async fn transaction_in_block(
		&self,
		tx_hash: Block::Hash,
		block_hash: Block::Hash,
		block_number: u64,
		index: usize,
	) {
		let mut tx_to_block_map = self.tx_to_block.write().await;
		let mut tx_info_map = self.tx_info.write().await;
		let mut block_to_txs_map = self.block_to_txs.write().await;

		// Update transaction -> block mapping
		tx_to_block_map.insert(tx_hash, block_hash);

		// Update block -> transactions mapping
		block_to_txs_map.entry(block_hash).or_insert_with(HashSet::new).insert(tx_hash);

		// Update transaction info
		if let Some(info) = tx_info_map.get_mut(&tx_hash) {
			info.status = TrackedTransactionStatus::InBlock { block_hash, block_number, index };
			info.block_hash = Some(block_hash);
			info.events
				.push(sc_transaction_pool_api::TransactionStatus::InBlock((block_hash, index)));
		} else {
			tx_info_map.insert(
				tx_hash,
				TransactionInfo {
					status: TrackedTransactionStatus::InBlock { block_hash, block_number, index },
					block_hash: Some(block_hash),
					events: vec![
						sc_transaction_pool_api::TransactionStatus::Ready,
						sc_transaction_pool_api::TransactionStatus::InBlock((block_hash, index)),
					],
					submitted_at: std::time::Instant::now(),
				},
			);
		}

		// Track in database if available
		if let Some(db) = &self.receipt_db {
			let tx_hash_str = format!("{:?}", tx_hash);
			let block_hash_str = format!("{:?}", block_hash);

			// Convert events to string format for storage
			let events: Vec<sc_transaction_pool_api::TransactionStatus<String, String>> =
				tx_info_map
					.get(&tx_hash)
					.map(|info| {
						info.events.iter().map(|e| self::convert_event_to_string(e)).collect()
					})
					.unwrap_or_default();

			if let Err(e) = db
				.update_transaction_in_block(
					&tx_hash_str,
					&block_hash_str,
					block_number,
					index,
					&events,
				)
				.await
			{
				log::warn!("Failed to track transaction in block {}: {}", tx_hash_str, e);
			}
		}
	}

	/// Update transaction status when block is finalized and track in database
	pub async fn transaction_finalized(
		&self,
		tx_hash: Block::Hash,
		block_hash: Block::Hash,
		block_number: u64,
		index: usize,
	) {
		let mut tx_info_map = self.tx_info.write().await;

		if let Some(info) = tx_info_map.get_mut(&tx_hash) {
			info.status = TrackedTransactionStatus::Finalized { block_hash, block_number, index };
			info.events
				.push(sc_transaction_pool_api::TransactionStatus::Finalized((block_hash, index)));
		}

		// Track in database if available
		if let Some(db) = &self.receipt_db {
			let tx_hash_str = format!("{:?}", tx_hash);
			let block_hash_str = format!("{:?}", block_hash);

			if let Err(e) = db
				.update_transaction_finalized(&tx_hash_str, &block_hash_str, block_number, index)
				.await
			{
				log::warn!("Failed to track finalized transaction {}: {}", tx_hash_str, e);
			}
		}
	}

	/// Mark transaction as dropped from the pool and track in database
	pub async fn transaction_dropped(&self, tx_hash: Block::Hash) {
		let mut tx_info_map = self.tx_info.write().await;

		if let Some(info) = tx_info_map.get_mut(&tx_hash) {
			info.status = TrackedTransactionStatus::Dropped;
			info.events.push(sc_transaction_pool_api::TransactionStatus::Dropped);
		}

		// Track in database if available
		if let Some(db) = &self.receipt_db {
			let tx_hash_str = format!("{:?}", tx_hash);

			if let Err(e) = db.mark_transaction_dropped(&tx_hash_str).await {
				log::warn!("Failed to track dropped transaction {}: {}", tx_hash_str, e);
			}
		}
	}

	/// Mark transaction as invalid and track in database
	pub async fn transaction_invalid(&self, tx_hash: Block::Hash) {
		let mut tx_info_map = self.tx_info.write().await;

		if let Some(info) = tx_info_map.get_mut(&tx_hash) {
			info.status = TrackedTransactionStatus::Invalid;
			info.events.push(sc_transaction_pool_api::TransactionStatus::Invalid);
		}

		// Track in database if available
		if let Some(db) = &self.receipt_db {
			let tx_hash_str = format!("{:?}", tx_hash);

			if let Err(e) = db.mark_transaction_invalid(&tx_hash_str).await {
				log::warn!("Failed to track invalid transaction {}: {}", tx_hash_str, e);
			}
		}
	}

	/// Get transaction status
	pub async fn get_transaction_status(
		&self,
		tx_hash: &Block::Hash,
	) -> Option<TrackedTransactionStatus<Block::Hash>> {
		self.tx_info.read().await.get(tx_hash).map(|info| info.status.clone())
	}

	/// Get transaction receipt
	pub async fn get_transaction_receipt(
		&self,
		tx_hash: &Block::Hash,
	) -> Option<sc_transaction_pool_api::TransactionReceipt<Block::Hash, Block::Hash>> {
		self.tx_info.read().await.get(tx_hash).map(|info| {
			let (status_rpc, actual_block_hash, block_number, transaction_index) =
				match &info.status {
					TrackedTransactionStatus::Pending =>
						(sc_transaction_pool_api::TransactionStatusRpc::InPool, None, None, None),
					TrackedTransactionStatus::InBlock { block_hash, block_number, index } => (
						sc_transaction_pool_api::TransactionStatusRpc::IncludedInBlock,
						Some(*block_hash),
						Some(*block_number),
						Some(*index),
					),
					TrackedTransactionStatus::Finalized { block_hash, block_number, index } => (
						sc_transaction_pool_api::TransactionStatusRpc::Finalized,
						Some(*block_hash),
						Some(*block_number),
						Some(*index),
					),
					TrackedTransactionStatus::Dropped =>
						(sc_transaction_pool_api::TransactionStatusRpc::Dropped, None, None, None),
					TrackedTransactionStatus::Invalid =>
						(sc_transaction_pool_api::TransactionStatusRpc::Invalid, None, None, None),
				};

			sc_transaction_pool_api::TransactionReceipt {
				status: status_rpc,
				block_hash: actual_block_hash.or(info.block_hash),
				block_number,
				transaction_index,
				events: info.events.clone(),
				transaction_hash: *tx_hash,
				submitted_at: info.submitted_at.elapsed().as_millis() as u64,
			}
		})
	}

	/// Get transaction info including all events
	pub async fn get_transaction_info(
		&self,
		tx_hash: &Block::Hash,
	) -> Option<TransactionInfo<Block::Hash, Block::Hash>> {
		self.tx_info.read().await.get(tx_hash).cloned()
	}

	/// Get block hash for a transaction
	pub async fn get_block_hash_for_transaction(
		&self,
		tx_hash: &Block::Hash,
	) -> Option<Block::Hash> {
		self.tx_to_block.read().await.get(tx_hash).cloned()
	}

	/// Get all transactions in a block
	pub async fn get_transactions_in_block(
		&self,
		block_hash: &Block::Hash,
	) -> Option<Vec<Block::Hash>> {
		self.block_to_txs
			.read()
			.await
			.get(block_hash)
			.map(|txs| txs.iter().cloned().collect())
	}

	/// Remove transaction from tracking (e.g., when evicted from pool)
	pub async fn remove_transaction(&self, tx_hash: &Block::Hash) {
		let mut tx_to_block_map = self.tx_to_block.write().await;
		let mut tx_info_map = self.tx_info.write().await;
		let mut block_to_txs_map = self.block_to_txs.write().await;

		if let Some(block_hash) = tx_to_block_map.remove(tx_hash) {
			if let Some(txs_in_block) = block_to_txs_map.get_mut(&block_hash) {
				txs_in_block.remove(tx_hash);
				if txs_in_block.is_empty() {
					block_to_txs_map.remove(&block_hash);
				}
			}
		}

		tx_info_map.remove(tx_hash);
	}

	/// Clean up transactions older than the specified duration
	pub async fn cleanup_old_transactions(&self, max_age: std::time::Duration) {
		let tx_info_map = self.tx_info.write().await;
		let now = std::time::Instant::now();

		// Find transactions to remove
		let to_remove: Vec<Block::Hash> = tx_info_map
			.iter()
			.filter_map(|(hash, info)| {
				if now.duration_since(info.submitted_at) > max_age {
					Some(*hash)
				} else {
					None
				}
			})
			.collect();

		// Drop the write lock before calling remove_transaction to avoid deadlock
		drop(tx_info_map);

		// Remove old transactions
		for hash in to_remove {
			self.remove_transaction(&hash).await;
		}
	}
}

fn convert_event_to_string<BlockHash, TxHash>(
	event: &sc_transaction_pool_api::TransactionStatus<BlockHash, TxHash>,
) -> sc_transaction_pool_api::TransactionStatus<String, String>
where
	BlockHash: AsRef<[u8]>,
	TxHash: AsRef<[u8]>,
{
	match event {
		sc_transaction_pool_api::TransactionStatus::Ready =>
			sc_transaction_pool_api::TransactionStatus::Ready,
		sc_transaction_pool_api::TransactionStatus::Future =>
			sc_transaction_pool_api::TransactionStatus::Future,
		sc_transaction_pool_api::TransactionStatus::Broadcast(peers) =>
			sc_transaction_pool_api::TransactionStatus::Broadcast(peers.clone()),
		sc_transaction_pool_api::TransactionStatus::InBlock((hash, idx)) =>
			sc_transaction_pool_api::TransactionStatus::InBlock((hex::encode(hash.as_ref()), *idx)),
		sc_transaction_pool_api::TransactionStatus::Retracted(hash) =>
			sc_transaction_pool_api::TransactionStatus::Retracted(hex::encode(hash.as_ref())),
		sc_transaction_pool_api::TransactionStatus::FinalityTimeout(hash) =>
			sc_transaction_pool_api::TransactionStatus::FinalityTimeout(hex::encode(hash.as_ref())),
		sc_transaction_pool_api::TransactionStatus::Finalized((hash, idx)) =>
			sc_transaction_pool_api::TransactionStatus::Finalized((
				hex::encode(hash.as_ref()),
				*idx,
			)),
		sc_transaction_pool_api::TransactionStatus::Usurped(hash) =>
			sc_transaction_pool_api::TransactionStatus::Usurped(hex::encode(hash.as_ref())),
		sc_transaction_pool_api::TransactionStatus::Dropped =>
			sc_transaction_pool_api::TransactionStatus::Dropped,
		sc_transaction_pool_api::TransactionStatus::Invalid =>
			sc_transaction_pool_api::TransactionStatus::Invalid,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_runtime::testing::{Block, H256};
	use substrate_test_runtime::Extrinsic;

	// Define a concrete block type for testing
	type TestBlock = Block<Extrinsic>;

	#[test]
	fn test_transaction_tracking_lifecycle() {
		let tracker = TransactionTracker::<TestBlock>::new();
		let tx_hash = H256::random();

		// Test pending
		tracker.add_pending_transaction(tx_hash);
		assert_eq!(
			tracker.get_transaction_status(&tx_hash),
			Some(TrackedTransactionStatus::Pending)
		);

		// Test in block
		let block_hash = H256::random();
		tracker.transaction_in_block(tx_hash, block_hash, 1, 0);
		assert!(matches!(
			tracker.get_transaction_status(&tx_hash),
			Some(TrackedTransactionStatus::InBlock { .. })
		));

		// Test finalized
		tracker.transaction_finalized(tx_hash, block_hash, 1, 0);
		assert!(matches!(
			tracker.get_transaction_status(&tx_hash),
			Some(TrackedTransactionStatus::Finalized { .. })
		));

		// Test receipt
		let receipt = tracker.get_transaction_receipt(&tx_hash);
		assert!(receipt.is_some());
		let receipt = receipt.unwrap();
		assert_eq!(receipt.transaction_hash, tx_hash);
		assert_eq!(receipt.block_hash, Some(block_hash));
	}

	#[test]
	fn test_transaction_dropped() {
		let tracker = TransactionTracker::<TestBlock>::new();
		let tx_hash = H256::random();

		tracker.add_pending_transaction(tx_hash);
		tracker.transaction_dropped(tx_hash);

		assert_eq!(
			tracker.get_transaction_status(&tx_hash),
			Some(TrackedTransactionStatus::Dropped)
		);
	}

	#[test]
	fn test_transaction_invalid() {
		let tracker = TransactionTracker::<TestBlock>::new();
		let tx_hash = H256::random();

		tracker.add_pending_transaction(tx_hash);
		tracker.transaction_invalid(tx_hash);

		assert_eq!(
			tracker.get_transaction_status(&tx_hash),
			Some(TrackedTransactionStatus::Invalid)
		);
	}

	#[test]
	fn test_cleanup_old_transactions() {
		let tracker = TransactionTracker::<TestBlock>::new();
		let tx_hash = H256::random();

		tracker.add_pending_transaction(tx_hash);

		// Should still exist
		assert!(tracker.get_transaction_status(&tx_hash).is_some());

		// Clean up transactions older than 1 nanosecond (should remove our transaction)
		tracker.cleanup_old_transactions(std::time::Duration::from_nanos(1));

		// Should be removed
		assert!(tracker.get_transaction_status(&tx_hash).is_none());
	}
}
