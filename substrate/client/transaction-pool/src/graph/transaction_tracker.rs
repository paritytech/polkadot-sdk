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

use sp_runtime::traits::Block as BlockT;
use std::{
	collections::{HashMap, HashSet},
	sync::RwLock,
};

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
}

impl<Block: BlockT> Default for TransactionTracker<Block> {
	fn default() -> Self {
		Self {
			tx_to_block: RwLock::new(HashMap::new()),
			tx_info: RwLock::new(HashMap::new()),
			block_to_txs: RwLock::new(HashMap::new()),
		}
	}
}

impl<Block: BlockT> TransactionTracker<Block> {
	pub fn new() -> Self {
		Self::default()
	}

	/// Add a transaction to the pool (pending status)
	pub fn add_pending_transaction(&self, tx_hash: Block::Hash) {
		let mut tx_info_map = self.tx_info.write().unwrap();
		tx_info_map.insert(
			tx_hash,
			TransactionInfo {
				status: TrackedTransactionStatus::Pending,
				block_hash: None,
				events: vec![sc_transaction_pool_api::TransactionStatus::Ready],
				submitted_at: std::time::Instant::now(),
			},
		);
	}

	/// Update transaction status when included in a block
	pub fn transaction_in_block(
		&self,
		tx_hash: Block::Hash,
		block_hash: Block::Hash,
		block_number: u64,
		index: usize,
	) {
		let mut tx_to_block_map = self.tx_to_block.write().unwrap();
		let mut tx_info_map = self.tx_info.write().unwrap();
		let mut block_to_txs_map = self.block_to_txs.write().unwrap();

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
	}

	/// Update transaction status when block is finalized
	pub fn transaction_finalized(
		&self,
		tx_hash: Block::Hash,
		block_hash: Block::Hash,
		block_number: u64,
		index: usize,
	) {
		let mut tx_info_map = self.tx_info.write().unwrap();

		if let Some(info) = tx_info_map.get_mut(&tx_hash) {
			info.status = TrackedTransactionStatus::Finalized { block_hash, block_number, index };
			info.events
				.push(sc_transaction_pool_api::TransactionStatus::Finalized((block_hash, index)));
		}
	}

	/// Mark transaction as dropped from the pool
	pub fn transaction_dropped(&self, tx_hash: Block::Hash) {
		let mut tx_info_map = self.tx_info.write().unwrap();

		if let Some(info) = tx_info_map.get_mut(&tx_hash) {
			info.status = TrackedTransactionStatus::Dropped;
			info.events.push(sc_transaction_pool_api::TransactionStatus::Dropped);
		}
	}

	/// Mark transaction as invalid
	pub fn transaction_invalid(&self, tx_hash: Block::Hash) {
		let mut tx_info_map = self.tx_info.write().unwrap();

		if let Some(info) = tx_info_map.get_mut(&tx_hash) {
			info.status = TrackedTransactionStatus::Invalid;
			info.events.push(sc_transaction_pool_api::TransactionStatus::Invalid);
		}
	}

	/// Get transaction status
	pub fn get_transaction_status(
		&self,
		tx_hash: &Block::Hash,
	) -> Option<TrackedTransactionStatus<Block::Hash>> {
		self.tx_info.read().unwrap().get(tx_hash).map(|info| info.status.clone())
	}

	/// Get transaction receipt
	pub fn get_transaction_receipt(
		&self,
		tx_hash: &Block::Hash,
	) -> Option<sc_transaction_pool_api::TransactionReceipt<Block::Hash, Block::Hash>> {
		self.tx_info.read().unwrap().get(tx_hash).map(|info| {
			let (status_rpc, actual_block_hash) = match &info.status {
				TrackedTransactionStatus::Pending =>
					(sc_transaction_pool_api::TransactionStatusRpc::InPool, None),
				TrackedTransactionStatus::InBlock { block_hash, .. } => (
					sc_transaction_pool_api::TransactionStatusRpc::IncludedInBlock,
					Some(*block_hash),
				),
				TrackedTransactionStatus::Finalized { block_hash, .. } =>
					(sc_transaction_pool_api::TransactionStatusRpc::Finalized, Some(*block_hash)),
				TrackedTransactionStatus::Dropped =>
					(sc_transaction_pool_api::TransactionStatusRpc::Dropped, None),
				TrackedTransactionStatus::Invalid =>
					(sc_transaction_pool_api::TransactionStatusRpc::Invalid, None),
			};

			sc_transaction_pool_api::TransactionReceipt {
				status: status_rpc,
				block_hash: actual_block_hash.or(info.block_hash),
				events: info.events.clone(),
				transaction_hash: *tx_hash,
				submitted_at: info.submitted_at.elapsed().as_millis() as u64,
			}
		})
	}

	/// Get transaction info including all events
	pub fn get_transaction_info(
		&self,
		tx_hash: &Block::Hash,
	) -> Option<TransactionInfo<Block::Hash, Block::Hash>> {
		self.tx_info.read().unwrap().get(tx_hash).cloned()
	}

	/// Get block hash for a transaction
	pub fn get_block_hash_for_transaction(&self, tx_hash: &Block::Hash) -> Option<Block::Hash> {
		self.tx_to_block.read().unwrap().get(tx_hash).cloned()
	}

	/// Get all transactions in a block
	pub fn get_transactions_in_block(&self, block_hash: &Block::Hash) -> Option<Vec<Block::Hash>> {
		self.block_to_txs
			.read()
			.unwrap()
			.get(block_hash)
			.map(|txs| txs.iter().cloned().collect())
	}

	/// Remove transaction from tracking (e.g., when evicted from pool)
	pub fn remove_transaction(&self, tx_hash: &Block::Hash) {
		let mut tx_to_block_map = self.tx_to_block.write().unwrap();
		let mut tx_info_map = self.tx_info.write().unwrap();
		let mut block_to_txs_map = self.block_to_txs.write().unwrap();

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
	pub fn cleanup_old_transactions(&self, max_age: std::time::Duration) {
		let tx_info_map = self.tx_info.write().unwrap();
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

		// Remove old transactions
		for hash in to_remove {
			self.remove_transaction(&hash);
		}
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
