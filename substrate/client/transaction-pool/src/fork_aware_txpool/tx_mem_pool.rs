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

//! Transaction memory pool, container for watched and unwatched transactions.
//! Acts as a buffer which collect transactions before importing them to the views. Following are
//! the crucial use cases when it is needed:
//! - empty pool (no views yet)
//! - potential races between creation of view and submitting transaction (w/o intermediary buffer
//!   some transactions could be lost)
//! - the transaction can be invalid on some forks (and thus the associated views may not contain
//!   it), while on other forks tx can be valid. Depending on which view is chosen to be cloned,
//!   such transaction could not be present in the newly created view.

use super::{
	metrics::MetricsLink as PrometheusMetrics, multi_view_listener::MultiViewListener,
	view_store::ViewStoreSubmitOutcome,
};
use crate::{
	common::log_xt::log_xt_trace,
	graph,
	graph::{base_pool::TimedTransactionSource, tracked_map::Size, ExtrinsicFor, ExtrinsicHash},
	LOG_TARGET,
};
use futures::FutureExt;
use itertools::Itertools;
use sc_transaction_pool_api::{TransactionPriority, TransactionSource};
use sp_blockchain::HashAndNumber;
use sp_runtime::{
	traits::Block as BlockT,
	transaction_validity::{InvalidTransaction, TransactionValidityError},
};
use std::{
	cmp::Ordering,
	collections::HashMap,
	sync::{
		atomic::{self, AtomicU64},
		Arc,
	},
	time::Instant,
};

/// The minimum interval between single transaction revalidations. Given in blocks.
pub(crate) const TXMEMPOOL_REVALIDATION_PERIOD: u64 = 10;

/// The number of transactions revalidated in single revalidation batch.
pub(crate) const TXMEMPOOL_MAX_REVALIDATION_BATCH_SIZE: usize = 1000;

/// The maximum number of transactions kept in the mem pool. Given as multiple of
/// the view's total limit.
pub const TXMEMPOOL_TRANSACTION_LIMIT_MULTIPLIER: usize = 4;

/// Represents the transaction in the intermediary buffer.
#[derive(Debug)]
pub(crate) struct TxInMemPool<ChainApi, Block>
where
	Block: BlockT,
	ChainApi: graph::ChainApi<Block = Block> + 'static,
{
	//todo: add listener for updating listeners with events [#5495]
	/// Is the progress of transaction watched.
	///
	/// Was transaction sent with `submit_and_watch`.
	watched: bool,
	/// Extrinsic actual body.
	tx: ExtrinsicFor<ChainApi>,
	/// Size of the extrinsics actual body.
	bytes: usize,
	/// Transaction source.
	source: TimedTransactionSource,
	/// When the transaction was revalidated, used to periodically revalidate the mem pool buffer.
	validated_at: AtomicU64,
	/// Priority of transaction at some block. It is assumed it will not be changed often.
	//todo: Option is needed here. This means lock. So maybe +AtomicBool?
	priority: AtomicU64,
	//todo: we need to add future / ready status at finalized block.
	//If future transactions are stuck in tx_mem_pool (due to limits being hit), we need a means
	// to replace them somehow with newly coming transactions.
	// For sure priority is one of them, but some additional criteria maybe required.
	//
	// The other maybe simple solution for this could be just obeying 10% limit for future in
	// tx_mem_pool. Oldest future transaction could be just dropped. *(Status at finalized would
	// also be needed). Probably is_future_at_finalized:Option<bool> flag will be enought
}

impl<ChainApi, Block> TxInMemPool<ChainApi, Block>
where
	Block: BlockT,
	ChainApi: graph::ChainApi<Block = Block> + 'static,
{
	/// Shall the progress of transaction be watched.
	///
	/// Was transaction sent with `submit_and_watch`.
	pub(crate) fn is_watched(&self) -> bool {
		self.watched
	}

	/// Creates a new instance of wrapper for unwatched transaction.
	fn new_unwatched(source: TransactionSource, tx: ExtrinsicFor<ChainApi>, bytes: usize) -> Self {
		Self::new(false, source, tx, bytes)
	}

	/// Creates a new instance of wrapper for watched transaction.
	fn new_watched(source: TransactionSource, tx: ExtrinsicFor<ChainApi>, bytes: usize) -> Self {
		Self::new(true, source, tx, bytes)
	}

	/// Creates a new instance of wrapper for a transaction with no priority.
	fn new(
		watched: bool,
		source: TransactionSource,
		tx: ExtrinsicFor<ChainApi>,
		bytes: usize,
	) -> Self {
		Self::new_with_priority(watched, source, tx, bytes, 0)
	}

	/// Creates a new instance of wrapper for a transaction with given priority.
	fn new_with_priority(
		watched: bool,
		source: TransactionSource,
		tx: ExtrinsicFor<ChainApi>,
		bytes: usize,
		priority: TransactionPriority,
	) -> Self {
		Self {
			watched,
			tx,
			source: TimedTransactionSource::from_transaction_source(source, true),
			validated_at: AtomicU64::new(0),
			bytes,
			priority: AtomicU64::new(priority),
		}
	}

	/// Provides a clone of actual transaction body.
	///
	/// Operation is cheap, as the body is `Arc`.
	pub(crate) fn tx(&self) -> ExtrinsicFor<ChainApi> {
		self.tx.clone()
	}

	/// Returns the source of the transaction.
	pub(crate) fn source(&self) -> TimedTransactionSource {
		self.source.clone()
	}

	/// Returns the priority of the transaction.
	pub(crate) fn priority(&self) -> TransactionPriority {
		self.priority.load(atomic::Ordering::Relaxed)
	}
}

impl<ChainApi, Block> Size for Arc<TxInMemPool<ChainApi, Block>>
where
	Block: BlockT,
	ChainApi: graph::ChainApi<Block = Block> + 'static,
{
	fn size(&self) -> usize {
		self.bytes
	}
}

type InternalTxMemPoolMap<ChainApi, Block> =
	graph::tracked_map::TrackedMap<ExtrinsicHash<ChainApi>, Arc<TxInMemPool<ChainApi, Block>>>;

/// An intermediary transactions buffer.
///
/// Keeps all the transaction which are potentially valid. Transactions that were finalized or
/// transactions that are invalid at finalized blocks are removed, either while handling the
/// `Finalized` event, or during revalidation process.
///
/// All transactions from  a`TxMemPool` are submitted to the newly created views.
///
/// All newly submitted transactions goes into the `TxMemPool`.
pub(super) struct TxMemPool<ChainApi, Block>
where
	Block: BlockT,
	ChainApi: graph::ChainApi<Block = Block> + 'static,
{
	/// A shared API instance necessary for blockchain related operations.
	api: Arc<ChainApi>,

	/// A shared instance of the `MultiViewListener`.
	///
	/// Provides a side-channel allowing to send per-transaction state changes notification.
	//todo: could be removed after removing watched field (and adding listener into tx) [#5495]
	listener: Arc<MultiViewListener<ChainApi>>,

	///  A map that stores the transactions currently in the memory pool.
	///
	///  The key is the hash of the transaction, and the value is a wrapper
	///  structure, which contains the mempool specific details of the transaction.
	transactions: InternalTxMemPoolMap<ChainApi, Block>,

	/// Prometheus's metrics endpoint.
	metrics: PrometheusMetrics,

	/// Indicates the maximum number of transactions that can be maintained in the memory pool.
	max_transactions_count: usize,

	/// Maximal size of encodings of all transactions in the memory pool.
	max_transactions_total_bytes: usize,
}

/// Helper structure to encapsulate a result of [`TxMemPool::try_insert`].
#[derive(Debug)]
pub(super) struct InsertionInfo<Hash> {
	pub(super) hash: Hash,
	pub(super) source: TimedTransactionSource,
	pub(super) removed: Vec<Hash>,
}

impl<Hash> InsertionInfo<Hash> {
	fn new(hash: Hash, source: TimedTransactionSource) -> Self {
		Self::new_with_removed(hash, source, Default::default())
	}
	fn new_with_removed(hash: Hash, source: TimedTransactionSource, removed: Vec<Hash>) -> Self {
		Self { hash, source, removed }
	}
}

impl<ChainApi, Block> TxMemPool<ChainApi, Block>
where
	Block: BlockT,
	ChainApi: graph::ChainApi<Block = Block> + 'static,
	<Block as BlockT>::Hash: Unpin,
{
	/// Creates a new `TxMemPool` instance with the given API, listener, metrics,
	/// and max transaction count.
	pub(super) fn new(
		api: Arc<ChainApi>,
		listener: Arc<MultiViewListener<ChainApi>>,
		metrics: PrometheusMetrics,
		max_transactions_count: usize,
		max_transactions_total_bytes: usize,
	) -> Self {
		Self {
			api,
			listener,
			transactions: Default::default(),
			metrics,
			max_transactions_count,
			max_transactions_total_bytes,
		}
	}

	/// Creates a new `TxMemPool` instance for testing purposes.
	#[allow(dead_code)]
	fn new_test(
		api: Arc<ChainApi>,
		max_transactions_count: usize,
		max_transactions_total_bytes: usize,
	) -> Self {
		Self {
			api,
			listener: Arc::from(MultiViewListener::new()),
			transactions: Default::default(),
			metrics: Default::default(),
			max_transactions_count,
			max_transactions_total_bytes,
		}
	}

	/// Retrieves a transaction by its hash if it exists in the memory pool.
	pub(super) fn get_by_hash(
		&self,
		hash: ExtrinsicHash<ChainApi>,
	) -> Option<Arc<TxInMemPool<ChainApi, Block>>> {
		self.transactions.read().get(&hash).map(Clone::clone)
	}

	/// Returns a tuple with the count of unwatched and watched transactions in the memory pool.
	pub fn unwatched_and_watched_count(&self) -> (usize, usize) {
		let transactions = self.transactions.read();
		let watched_count = transactions.values().filter(|t| t.is_watched()).count();
		(transactions.len() - watched_count, watched_count)
	}

	/// Returns a total number of transactions kept within mempool.
	pub fn len(&self) -> usize {
		self.transactions.read().len()
	}

	/// Returns the number of bytes used by all extrinsics in the the pool.
	#[cfg(test)]
	pub fn bytes(&self) -> usize {
		return self.transactions.bytes()
	}

	/// Returns true if provided values would exceed defined limits.
	fn is_limit_exceeded(&self, length: usize, current_total_bytes: usize) -> bool {
		length > self.max_transactions_count ||
			current_total_bytes > self.max_transactions_total_bytes
	}

	/// Attempts to insert a transaction into the memory pool, ensuring it does not
	/// exceed the maximum allowed transaction count.
	fn try_insert(
		&self,
		hash: ExtrinsicHash<ChainApi>,
		tx: TxInMemPool<ChainApi, Block>,
	) -> Result<InsertionInfo<ExtrinsicHash<ChainApi>>, sc_transaction_pool_api::error::Error> {
		let mut transactions = self.transactions.write();

		let bytes = self.transactions.bytes();

		let result = match (
			self.is_limit_exceeded(transactions.len() + 1, bytes + tx.bytes),
			transactions.contains_key(&hash),
		) {
			(false, false) => {
				let source = tx.source();
				transactions.insert(hash, Arc::from(tx));
				Ok(InsertionInfo::new(hash, source))
			},
			(_, true) =>
				Err(sc_transaction_pool_api::error::Error::AlreadyImported(Box::new(hash))),
			(true, _) => Err(sc_transaction_pool_api::error::Error::ImmediatelyDropped),
		};
		log::trace!(target: LOG_TARGET, "[{:?}] mempool::try_insert: {:?}", hash, result.as_ref().map(|r| r.hash));

		result
	}

	/// Attempts to insert a new transaction in the memory pool and drop some worse existing
	/// transactions.
	///
	/// This operation will not overflow the limit of the mempool. It means that cumulative
	/// size of removed transactions will be equal (or greated) then size of newly inserted
	/// transaction.
	///
	/// Returns a `Result` containing `InsertionInfo` if the new transaction is successfully
	/// inserted; otherwise, returns an appropriate error indicating the failure.
	pub(super) fn try_insert_with_dropping(
		&self,
		hash: ExtrinsicHash<ChainApi>,
		new_tx: TxInMemPool<ChainApi, Block>,
	) -> Result<InsertionInfo<ExtrinsicHash<ChainApi>>, sc_transaction_pool_api::error::Error> {
		let mut transactions = self.transactions.write();

		if transactions.contains_key(&hash) {
			return Err(sc_transaction_pool_api::error::Error::AlreadyImported(Box::new(hash)));
		}

		let mut sorted =
			transactions.iter().map(|(h, v)| (h.clone(), v.clone())).collect::<Vec<_>>();

		// When pushing higher prio transaction, we need to find a number of lower prio txs, such
		// that the sum of their bytes is ge then size of new tx. Otherwise we could overflow size
		// limits. Naive way to do it - rev-sort by priority and eat the tail.

		// reverse (lowest prio last)
		sorted.sort_by(|(_, a), (_, b)| match b.priority().cmp(&a.priority()) {
			Ordering::Equal => match (a.source.timestamp, b.source.timestamp) {
				(Some(a), Some(b)) => b.cmp(&a),
				_ => Ordering::Equal,
			},
			ordering @ _ => ordering,
		});

		let required_size = new_tx.bytes;
		let mut total_size = 0usize;
		let mut to_be_removed = vec![];

		while total_size < required_size {
			let Some((worst_hash, worst_tx)) = sorted.pop() else {
				return Err(sc_transaction_pool_api::error::Error::ImmediatelyDropped);
			};

			if worst_tx.priority() >= new_tx.priority() {
				return Err(sc_transaction_pool_api::error::Error::ImmediatelyDropped);
			}

			total_size += worst_tx.bytes;
			to_be_removed.push(worst_hash);
		}

		let source = new_tx.source();
		transactions.insert(hash, Arc::from(new_tx));
		for worst_hash in &to_be_removed {
			transactions.remove(worst_hash);
		}
		debug_assert!(!self.is_limit_exceeded(transactions.len(), self.transactions.bytes()));

		Ok(InsertionInfo::new_with_removed(hash, source, to_be_removed))

		// let result = match (
		// 	self.is_limit_exceeded(transactions.len() + 1, bytes + tx.bytes),
		// 	transactions.contains_key(&hash),
		// ) {
		// 	(false, false) => {
		// 		let source = tx.source();
		// 		transactions.insert(hash, Arc::from(tx));
		// 		Ok(InsertionInfo::new(hash, source))
		// 	},
		// 	(_, true) =>
		// 		Err(sc_transaction_pool_api::error::Error::AlreadyImported(Box::new(hash))),
		// 	(true, _) => Err(sc_transaction_pool_api::error::Error::ImmediatelyDropped),
		// };
		// log::trace!(target: LOG_TARGET, "[{:?}] mempool::try_insert: {:?}", hash,
		// result.as_ref().map(|r| r.hash));
	}

	/// Attempts to replace an existing transaction in the memory pool with a new one.
	///
	/// Returns a `Result` containing `InsertionInfo` if the new transaction is successfully
	/// inserted; otherwise, returns an appropriate error indicating the failure.
	pub(super) fn try_replace_transaction(
		&self,
		new_xt: ExtrinsicFor<ChainApi>,
		priority: TransactionPriority,
		source: TransactionSource,
		watched: bool,
	) -> Result<InsertionInfo<ExtrinsicHash<ChainApi>>, sc_transaction_pool_api::error::Error> {
		let (hash, length) = self.api.hash_and_length(&new_xt);
		self.try_insert_with_dropping(
			hash,
			TxInMemPool::new_with_priority(watched, source, new_xt, length, priority),
		)
	}

	/// Adds a new unwatched transactions to the internal buffer not exceeding the limit.
	///
	/// Returns the vector of results for each transaction, the order corresponds to the input
	/// vector.
	pub(super) fn extend_unwatched(
		&self,
		source: TransactionSource,
		xts: &[ExtrinsicFor<ChainApi>],
	) -> Vec<Result<InsertionInfo<ExtrinsicHash<ChainApi>>, sc_transaction_pool_api::error::Error>>
	{
		let result = xts
			.iter()
			.map(|xt| {
				let (hash, length) = self.api.hash_and_length(&xt);
				self.try_insert(hash, TxInMemPool::new_unwatched(source, xt.clone(), length))
			})
			.collect::<Vec<_>>();
		result
	}

	/// Adds a new watched transaction to the memory pool if it does not exceed the maximum allowed
	/// transaction count.
	pub(super) fn push_watched(
		&self,
		source: TransactionSource,
		xt: ExtrinsicFor<ChainApi>,
	) -> Result<InsertionInfo<ExtrinsicHash<ChainApi>>, sc_transaction_pool_api::error::Error> {
		let (hash, length) = self.api.hash_and_length(&xt);
		self.try_insert(hash, TxInMemPool::new_watched(source, xt.clone(), length))
	}

	/// Clones and returns a `HashMap` of references to all unwatched transactions in the memory
	/// pool.
	pub(super) fn clone_unwatched(
		&self,
	) -> HashMap<ExtrinsicHash<ChainApi>, Arc<TxInMemPool<ChainApi, Block>>> {
		self.transactions
			.read()
			.iter()
			.filter_map(|(hash, tx)| (!tx.is_watched()).then(|| (*hash, tx.clone())))
			.collect::<HashMap<_, _>>()
	}

	/// Clones and returns a `HashMap` of references to all watched transactions in the memory pool.
	pub(super) fn clone_watched(
		&self,
	) -> HashMap<ExtrinsicHash<ChainApi>, Arc<TxInMemPool<ChainApi, Block>>> {
		self.transactions
			.read()
			.iter()
			.filter_map(|(hash, tx)| (tx.is_watched()).then(|| (*hash, tx.clone())))
			.collect::<HashMap<_, _>>()
	}

	/// Removes a transaction with given hash from the memory pool.
	pub(super) fn remove_transaction(
		&self,
		hash: &ExtrinsicHash<ChainApi>,
	) -> Option<Arc<TxInMemPool<ChainApi, Block>>> {
		log::debug!(target: LOG_TARGET, "[{hash:?}] mempool::remove_transaction");
		self.transactions.write().remove(hash)
	}

	/// Revalidates a batch of transactions against the provided finalized block.
	///
	/// Returns a vector of invalid transaction hashes.
	async fn revalidate_inner(&self, finalized_block: HashAndNumber<Block>) -> Vec<Block::Hash> {
		log::trace!(target: LOG_TARGET, "mempool::revalidate at:{finalized_block:?}");
		let start = Instant::now();

		let (count, input) = {
			let transactions = self.transactions.clone_map();

			(
				transactions.len(),
				transactions
					.into_iter()
					.filter(|xt| {
						let finalized_block_number = finalized_block.number.into().as_u64();
						xt.1.validated_at.load(atomic::Ordering::Relaxed) +
							TXMEMPOOL_REVALIDATION_PERIOD <
							finalized_block_number
					})
					.sorted_by_key(|tx| tx.1.validated_at.load(atomic::Ordering::Relaxed))
					.take(TXMEMPOOL_MAX_REVALIDATION_BATCH_SIZE),
			)
		};

		let validations_futures = input.into_iter().map(|(xt_hash, xt)| {
			self.api
				.validate_transaction(finalized_block.hash, xt.source.clone().into(), xt.tx())
				.map(move |validation_result| {
					xt.validated_at
						.store(finalized_block.number.into().as_u64(), atomic::Ordering::Relaxed);
					(xt_hash, validation_result)
				})
		});
		let validation_results = futures::future::join_all(validations_futures).await;
		let input_len = validation_results.len();

		let duration = start.elapsed();

		let invalid_hashes = validation_results
			.into_iter()
			.filter_map(|(xt_hash, validation_result)| match validation_result {
				Ok(Ok(_)) |
				Ok(Err(TransactionValidityError::Invalid(InvalidTransaction::Future))) => None,
				Err(_) |
				Ok(Err(TransactionValidityError::Unknown(_))) |
				Ok(Err(TransactionValidityError::Invalid(_))) => {
					log::trace!(
						target: LOG_TARGET,
						"[{:?}]: Purging: invalid: {:?}",
						xt_hash,
						validation_result,
					);
					Some(xt_hash)
				},
			})
			.collect::<Vec<_>>();

		log::debug!(
			target: LOG_TARGET,
			"mempool::revalidate: at {finalized_block:?} count:{input_len}/{count} invalid_hashes:{} took {duration:?}", invalid_hashes.len(),
		);

		invalid_hashes
	}

	/// Removes the finalized transactions from the memory pool, using a provided list of hashes.
	pub(super) async fn purge_finalized_transactions(
		&self,
		finalized_xts: &Vec<ExtrinsicHash<ChainApi>>,
	) {
		log::debug!(target: LOG_TARGET, "purge_finalized_transactions count:{:?}", finalized_xts.len());
		log_xt_trace!(target: LOG_TARGET, finalized_xts, "[{:?}] purged finalized transactions");
		let mut transactions = self.transactions.write();
		finalized_xts.iter().for_each(|t| {
			transactions.remove(t);
		});
	}

	/// Revalidates transactions in the memory pool against a given finalized block and removes
	/// invalid ones.
	pub(super) async fn revalidate(&self, finalized_block: HashAndNumber<Block>) {
		log::trace!(target: LOG_TARGET, "purge_transactions at:{:?}", finalized_block);
		let invalid_hashes = self.revalidate_inner(finalized_block.clone()).await;

		self.metrics.report(|metrics| {
			metrics.mempool_revalidation_invalid_txs.inc_by(invalid_hashes.len() as _)
		});

		let mut transactions = self.transactions.write();
		invalid_hashes.iter().for_each(|i| {
			transactions.remove(i);
		});
		self.listener.invalidate_transactions(&invalid_hashes);
	}

	pub(super) fn update_transaction(&self, outcome: &ViewStoreSubmitOutcome<ChainApi>) {
		if let Some(priority) = outcome.priority() {
			if let Some(tx) = self.transactions.write().get_mut(&outcome.hash()) {
				tx.priority.store(priority, atomic::Ordering::Relaxed);
			}
		}
	}
}

#[cfg(test)]
mod tx_mem_pool_tests {
	use super::*;
	use crate::{common::tests::TestApi, graph::ChainApi};
	use substrate_test_runtime::{AccountId, Extrinsic, ExtrinsicBuilder, Transfer, H256};
	use substrate_test_runtime_client::AccountKeyring::*;
	fn uxt(nonce: u64) -> Extrinsic {
		crate::common::tests::uxt(Transfer {
			from: Alice.into(),
			to: AccountId::from_h256(H256::from_low_u64_be(2)),
			amount: 5,
			nonce,
		})
	}

	#[test]
	fn extend_unwatched_obeys_limit() {
		let max = 10;
		let api = Arc::from(TestApi::default());
		let mempool = TxMemPool::new_test(api, max, usize::MAX);

		let xts = (0..max + 1).map(|x| Arc::from(uxt(x as _))).collect::<Vec<_>>();

		let results = mempool.extend_unwatched(TransactionSource::External, &xts);
		assert!(results.iter().take(max).all(Result::is_ok));
		assert!(matches!(
			results.into_iter().last().unwrap().unwrap_err(),
			sc_transaction_pool_api::error::Error::ImmediatelyDropped
		));
	}

	#[test]
	fn extend_unwatched_detects_already_imported() {
		sp_tracing::try_init_simple();
		let max = 10;
		let api = Arc::from(TestApi::default());
		let mempool = TxMemPool::new_test(api, max, usize::MAX);

		let mut xts = (0..max - 1).map(|x| Arc::from(uxt(x as _))).collect::<Vec<_>>();
		xts.push(xts.iter().last().unwrap().clone());

		let results = mempool.extend_unwatched(TransactionSource::External, &xts);
		assert!(results.iter().take(max - 1).all(Result::is_ok));
		assert!(matches!(
			results.into_iter().last().unwrap().unwrap_err(),
			sc_transaction_pool_api::error::Error::AlreadyImported(_)
		));
	}

	#[test]
	fn push_obeys_limit() {
		let max = 10;
		let api = Arc::from(TestApi::default());
		let mempool = TxMemPool::new_test(api, max, usize::MAX);

		let xts = (0..max).map(|x| Arc::from(uxt(x as _))).collect::<Vec<_>>();

		let results = mempool.extend_unwatched(TransactionSource::External, &xts);
		assert!(results.iter().all(Result::is_ok));

		let xt = Arc::from(uxt(98));
		let result = mempool.push_watched(TransactionSource::External, xt);
		assert!(matches!(
			result.unwrap_err(),
			sc_transaction_pool_api::error::Error::ImmediatelyDropped
		));
		let xt = Arc::from(uxt(99));
		let mut result = mempool.extend_unwatched(TransactionSource::External, &[xt]);
		assert!(matches!(
			result.pop().unwrap().unwrap_err(),
			sc_transaction_pool_api::error::Error::ImmediatelyDropped
		));
	}

	#[test]
	fn push_detects_already_imported() {
		let max = 10;
		let api = Arc::from(TestApi::default());
		let mempool = TxMemPool::new_test(api, 2 * max, usize::MAX);

		let xts = (0..max).map(|x| Arc::from(uxt(x as _))).collect::<Vec<_>>();
		let xt0 = xts.iter().last().unwrap().clone();
		let xt1 = xts.iter().next().unwrap().clone();

		let results = mempool.extend_unwatched(TransactionSource::External, &xts);
		assert!(results.iter().all(Result::is_ok));

		let result = mempool.push_watched(TransactionSource::External, xt0);
		assert!(matches!(
			result.unwrap_err(),
			sc_transaction_pool_api::error::Error::AlreadyImported(_)
		));
		let mut result = mempool.extend_unwatched(TransactionSource::External, &[xt1]);
		assert!(matches!(
			result.pop().unwrap().unwrap_err(),
			sc_transaction_pool_api::error::Error::AlreadyImported(_)
		));
	}

	#[test]
	fn count_works() {
		let max = 100;
		let api = Arc::from(TestApi::default());
		let mempool = TxMemPool::new_test(api, max, usize::MAX);

		let xts0 = (0..10).map(|x| Arc::from(uxt(x as _))).collect::<Vec<_>>();

		let results = mempool.extend_unwatched(TransactionSource::External, &xts0);
		assert!(results.iter().all(Result::is_ok));

		let xts1 = (0..5).map(|x| Arc::from(uxt(2 * x))).collect::<Vec<_>>();
		let results = xts1
			.into_iter()
			.map(|t| mempool.push_watched(TransactionSource::External, t))
			.collect::<Vec<_>>();
		assert!(results.iter().all(Result::is_ok));
		assert_eq!(mempool.unwatched_and_watched_count(), (10, 5));
	}

	fn large_uxt(x: usize) -> Extrinsic {
		ExtrinsicBuilder::new_include_data(vec![x as u8; 1024]).build()
	}

	#[test]
	fn push_obeys_size_limit() {
		sp_tracing::try_init_simple();
		let max = 10;
		let api = Arc::from(TestApi::default());
		//size of large extrinsic is: 1129
		let mempool = TxMemPool::new_test(api.clone(), usize::MAX, max * 1129);

		let xts = (0..max).map(|x| Arc::from(large_uxt(x))).collect::<Vec<_>>();

		let total_xts_bytes = xts.iter().fold(0, |r, x| r + api.hash_and_length(&x).1);

		let results = mempool.extend_unwatched(TransactionSource::External, &xts);
		assert!(results.iter().all(Result::is_ok));
		assert_eq!(mempool.bytes(), total_xts_bytes);

		let xt = Arc::from(large_uxt(98));
		let result = mempool.push_watched(TransactionSource::External, xt);
		assert!(matches!(
			result.unwrap_err(),
			sc_transaction_pool_api::error::Error::ImmediatelyDropped
		));

		let xt = Arc::from(large_uxt(99));
		let mut result = mempool.extend_unwatched(TransactionSource::External, &[xt]);
		assert!(matches!(
			result.pop().unwrap().unwrap_err(),
			sc_transaction_pool_api::error::Error::ImmediatelyDropped
		));
	}

	//add some test for try_insert_with_dropping
}
