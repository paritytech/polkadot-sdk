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

use std::{
	cmp::Ordering,
	collections::{HashMap, HashSet},
	sync::{
		atomic::{self, AtomicU64},
		Arc,
	},
	time::Instant,
};

use futures::FutureExt;
use itertools::Itertools;
use parking_lot::RwLock;
use tracing::{debug, trace};

use sc_transaction_pool_api::{TransactionPriority, TransactionSource};
use sp_blockchain::HashAndNumber;
use sp_runtime::{
	traits::Block as BlockT,
	transaction_validity::{InvalidTransaction, TransactionValidityError},
};

use crate::{
	common::tracing_log_xt::log_xt_trace,
	graph,
	graph::{base_pool::TimedTransactionSource, tracked_map::Size, ExtrinsicFor, ExtrinsicHash},
	LOG_TARGET,
};

use super::{
	metrics::MetricsLink as PrometheusMetrics,
	multi_view_listener::MultiViewListener,
	view_store::{ViewStore, ViewStoreSubmitOutcome},
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
	/// Is the progress of transaction watched.
	///
	/// Indicates if transaction was sent with `submit_and_watch`. Serves only stats/testing
	/// purposes.
	watched: bool,
	/// Extrinsic actual body.
	tx: ExtrinsicFor<ChainApi>,
	/// Size of the extrinsics actual body.
	bytes: usize,
	/// Transaction source.
	source: TimedTransactionSource,
	/// When the transaction was revalidated, used to periodically revalidate the mem pool buffer.
	validated_at: AtomicU64,
	/// Priority of transaction at some block. It is assumed it will not be changed often. None if
	/// not known.
	priority: RwLock<Option<TransactionPriority>>,
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
		Self::new_with_optional_priority(watched, source, tx, bytes, None)
	}

	/// Creates a new instance of wrapper for a transaction with given priority.
	fn new_with_priority(
		watched: bool,
		source: TransactionSource,
		tx: ExtrinsicFor<ChainApi>,
		bytes: usize,
		priority: TransactionPriority,
	) -> Self {
		Self::new_with_optional_priority(watched, source, tx, bytes, Some(priority))
	}

	/// Creates a new instance of wrapper for a transaction with optional priority.
	fn new_with_optional_priority(
		watched: bool,
		source: TransactionSource,
		tx: ExtrinsicFor<ChainApi>,
		bytes: usize,
		priority: Option<TransactionPriority>,
	) -> Self {
		Self {
			watched,
			tx,
			source: TimedTransactionSource::from_transaction_source(source, true),
			validated_at: AtomicU64::new(0),
			bytes,
			priority: priority.into(),
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
	pub(crate) fn priority(&self) -> Option<TransactionPriority> {
		*self.priority.read()
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
	#[cfg(test)]
	fn new_test(
		api: Arc<ChainApi>,
		max_transactions_count: usize,
		max_transactions_total_bytes: usize,
	) -> Self {
		Self {
			api,
			listener: Arc::from(MultiViewListener::new_with_worker(Default::default()).0),
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
		tx_hash: ExtrinsicHash<ChainApi>,
		tx: TxInMemPool<ChainApi, Block>,
	) -> Result<InsertionInfo<ExtrinsicHash<ChainApi>>, sc_transaction_pool_api::error::Error> {
		let mut transactions = self.transactions.write();

		let bytes = self.transactions.bytes();

		let result = match (
			self.is_limit_exceeded(transactions.len() + 1, bytes + tx.bytes),
			transactions.contains_key(&tx_hash),
		) {
			(false, false) => {
				let source = tx.source();
				transactions.insert(tx_hash, Arc::from(tx));
				Ok(InsertionInfo::new(tx_hash, source))
			},
			(_, true) =>
				Err(sc_transaction_pool_api::error::Error::AlreadyImported(Box::new(tx_hash))),
			(true, _) => Err(sc_transaction_pool_api::error::Error::ImmediatelyDropped),
		};
		trace!(
			target: LOG_TARGET,
			?tx_hash,
			result_hash = ?result.as_ref().map(|r| r.hash),
			"mempool::try_insert"
		);
		result
	}

	/// Attempts to insert a new transaction in the memory pool and drop some worse existing
	/// transactions.
	///
	/// A "worse" transaction means transaction with lower priority, or older transaction with the
	/// same prio.
	///
	/// This operation will not overflow the limit of the mempool. It means that cumulative
	/// size of removed transactions will be equal (or greated) then size of newly inserted
	/// transaction.
	///
	/// Returns a `Result` containing `InsertionInfo` if the new transaction is successfully
	/// inserted; otherwise, returns an appropriate error indicating the failure.
	pub(super) fn try_insert_with_replacement(
		&self,
		new_tx: ExtrinsicFor<ChainApi>,
		priority: TransactionPriority,
		source: TransactionSource,
		watched: bool,
	) -> Result<InsertionInfo<ExtrinsicHash<ChainApi>>, sc_transaction_pool_api::error::Error> {
		let (hash, length) = self.api.hash_and_length(&new_tx);
		let new_tx = TxInMemPool::new_with_priority(watched, source, new_tx, length, priority);
		if new_tx.bytes > self.max_transactions_total_bytes {
			return Err(sc_transaction_pool_api::error::Error::ImmediatelyDropped);
		}

		let mut transactions = self.transactions.write();

		if transactions.contains_key(&hash) {
			return Err(sc_transaction_pool_api::error::Error::AlreadyImported(Box::new(hash)));
		}

		let mut sorted = transactions
			.iter()
			.filter_map(|(h, v)| v.priority().map(|_| (*h, v.clone())))
			.collect::<Vec<_>>();

		// When pushing higher prio transaction, we need to find a number of lower prio txs, such
		// that the sum of their bytes is ge then size of new tx. Otherwise we could overflow size
		// limits. Naive way to do it - rev-sort by priority and eat the tail.

		// reverse (oldest, lowest prio last)
		sorted.sort_by(|(_, a), (_, b)| match b.priority().cmp(&a.priority()) {
			Ordering::Equal => match (a.source.timestamp, b.source.timestamp) {
				(Some(a), Some(b)) => b.cmp(&a),
				_ => Ordering::Equal,
			},
			ordering => ordering,
		});

		let mut total_size_removed = 0usize;
		let mut to_be_removed = vec![];
		let free_bytes = self.max_transactions_total_bytes - self.transactions.bytes();

		loop {
			let Some((worst_hash, worst_tx)) = sorted.pop() else {
				return Err(sc_transaction_pool_api::error::Error::ImmediatelyDropped);
			};

			if worst_tx.priority() >= new_tx.priority() {
				return Err(sc_transaction_pool_api::error::Error::ImmediatelyDropped);
			}

			total_size_removed += worst_tx.bytes;
			to_be_removed.push(worst_hash);

			if free_bytes + total_size_removed >= new_tx.bytes {
				break;
			}
		}

		let source = new_tx.source();
		transactions.insert(hash, Arc::from(new_tx));
		for worst_hash in &to_be_removed {
			transactions.remove(worst_hash);
		}
		debug_assert!(!self.is_limit_exceeded(transactions.len(), self.transactions.bytes()));

		Ok(InsertionInfo::new_with_removed(hash, source, to_be_removed))
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

	/// Clones and returns a `HashMap` of references to all transactions in the memory pool.
	pub(super) fn clone_transactions(
		&self,
	) -> HashMap<ExtrinsicHash<ChainApi>, Arc<TxInMemPool<ChainApi, Block>>> {
		self.transactions.clone_map()
	}

	/// Removes transactions with given hashes from the memory pool.
	pub(super) fn remove_transactions(&self, tx_hashes: &[ExtrinsicHash<ChainApi>]) {
		log_xt_trace!(target: LOG_TARGET, tx_hashes, "mempool::remove_transaction");
		let mut transactions = self.transactions.write();
		for tx_hash in tx_hashes {
			transactions.remove(tx_hash);
		}
	}

	/// Revalidates a batch of transactions against the provided finalized block.
	///
	/// Returns a vector of invalid transaction hashes.
	async fn revalidate_inner(&self, finalized_block: HashAndNumber<Block>) -> Vec<Block::Hash> {
		trace!(
			target: LOG_TARGET,
			?finalized_block,
			"mempool::revalidate_inner"
		);
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
			.filter_map(|(tx_hash, validation_result)| match validation_result {
				Ok(Ok(_)) |
				Ok(Err(TransactionValidityError::Invalid(InvalidTransaction::Future))) => None,
				Err(_) |
				Ok(Err(TransactionValidityError::Unknown(_))) |
				Ok(Err(TransactionValidityError::Invalid(_))) => {
					trace!(
						target: LOG_TARGET,
						?tx_hash,
						?validation_result,
						"mempool::revalidate_inner invalid"
					);
					Some(tx_hash)
				},
			})
			.collect::<Vec<_>>();

		debug!(
			target: LOG_TARGET,
			?finalized_block,
			input_len,
			count,
			invalid_hashes = invalid_hashes.len(),
			?duration,
			"mempool::revalidate_inner"
		);

		invalid_hashes
	}

	/// Removes the finalized transactions from the memory pool, using a provided list of hashes.
	pub(super) async fn purge_finalized_transactions(
		&self,
		finalized_xts: &Vec<ExtrinsicHash<ChainApi>>,
	) {
		debug!(
			target: LOG_TARGET,
			count = finalized_xts.len(),
			"purge_finalized_transactions"
		);
		log_xt_trace!(target: LOG_TARGET, finalized_xts, "purged finalized transactions");
		let mut transactions = self.transactions.write();
		finalized_xts.iter().for_each(|t| {
			transactions.remove(t);
		});
	}

	/// Revalidates transactions in the memory pool against a given finalized block and removes
	/// invalid ones.
	pub(super) async fn revalidate(
		&self,
		view_store: Arc<ViewStore<ChainApi, Block>>,
		finalized_block: HashAndNumber<Block>,
	) {
		let revalidated_invalid_hashes = self.revalidate_inner(finalized_block.clone()).await;

		let mut invalid_hashes_subtrees =
			revalidated_invalid_hashes.clone().into_iter().collect::<HashSet<_>>();
		for tx in &revalidated_invalid_hashes {
			invalid_hashes_subtrees.extend(
				view_store
					.remove_transaction_subtree(*tx, |_, _| {})
					.into_iter()
					.map(|tx| tx.hash),
			);
		}

		{
			let mut transactions = self.transactions.write();
			invalid_hashes_subtrees.iter().for_each(|tx_hash| {
				transactions.remove(&tx_hash);
			});
		};

		self.metrics.report(|metrics| {
			metrics
				.mempool_revalidation_invalid_txs
				.inc_by(invalid_hashes_subtrees.len() as _)
		});

		let revalidated_invalid_hashes_len = revalidated_invalid_hashes.len();
		let invalid_hashes_subtrees_len = invalid_hashes_subtrees.len();

		self.listener
			.transactions_invalidated(&invalid_hashes_subtrees.into_iter().collect::<Vec<_>>());

		trace!(
			target: LOG_TARGET,
			?finalized_block,
			revalidated_invalid_hashes_len,
			invalid_hashes_subtrees_len,
			"mempool::revalidate"
		);
	}

	/// Updates the priority of transaction stored in mempool using provided view_store submission
	/// outcome.
	pub(super) fn update_transaction_priority(&self, outcome: &ViewStoreSubmitOutcome<ChainApi>) {
		outcome.priority().map(|priority| {
			self.transactions
				.write()
				.get_mut(&outcome.hash())
				.map(|p| *p.priority.write() = Some(priority))
		});
	}

	/// Counts the number of transactions in the provided iterator of hashes
	/// that are not known to the pool.
	pub(super) fn count_unknown_transactions<'a>(
		&self,
		hashes: impl Iterator<Item = &'a ExtrinsicHash<ChainApi>>,
	) -> usize {
		let transactions = self.transactions.read();
		hashes.filter(|tx_hash| !transactions.contains_key(tx_hash)).count()
	}
}

#[cfg(test)]
mod tx_mem_pool_tests {
	use substrate_test_runtime::{AccountId, Extrinsic, ExtrinsicBuilder, Transfer, H256};
	use substrate_test_runtime_client::Sr25519Keyring::*;

	use crate::{common::tests::TestApi, graph::ChainApi};

	use super::*;

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

	/// size of large extrinsic
	const LARGE_XT_SIZE: usize = 1129;

	fn large_uxt(x: usize) -> Extrinsic {
		ExtrinsicBuilder::new_include_data(vec![x as u8; 1024]).build()
	}

	#[test]
	fn push_obeys_size_limit() {
		sp_tracing::try_init_simple();
		let max = 10;
		let api = Arc::from(TestApi::default());
		let mempool = TxMemPool::new_test(api.clone(), usize::MAX, max * LARGE_XT_SIZE);

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

	#[test]
	fn replacing_txs_works_for_same_tx_size() {
		sp_tracing::try_init_simple();
		let max = 10;
		let api = Arc::from(TestApi::default());
		let mempool = TxMemPool::new_test(api.clone(), usize::MAX, max * LARGE_XT_SIZE);

		let xts = (0..max).map(|x| Arc::from(large_uxt(x))).collect::<Vec<_>>();

		let low_prio = 0u64;
		let hi_prio = u64::MAX;

		let total_xts_bytes = xts.iter().fold(0, |r, x| r + api.hash_and_length(&x).1);
		let (submit_outcomes, hashes): (Vec<_>, Vec<_>) = xts
			.iter()
			.map(|t| {
				let h = api.hash_and_length(t).0;
				(ViewStoreSubmitOutcome::new(h, Some(low_prio)), h)
			})
			.unzip();

		let results = mempool.extend_unwatched(TransactionSource::External, &xts);
		assert!(results.iter().all(Result::is_ok));
		assert_eq!(mempool.bytes(), total_xts_bytes);

		submit_outcomes
			.into_iter()
			.for_each(|o| mempool.update_transaction_priority(&o));

		let xt = Arc::from(large_uxt(98));
		let hash = api.hash_and_length(&xt).0;
		let result = mempool
			.try_insert_with_replacement(xt, hi_prio, TransactionSource::External, false)
			.unwrap();

		assert_eq!(result.hash, hash);
		assert_eq!(result.removed, hashes[0..1]);
	}

	#[test]
	fn replacing_txs_removes_proper_size_of_txs() {
		sp_tracing::try_init_simple();
		let max = 10;
		let api = Arc::from(TestApi::default());
		let mempool = TxMemPool::new_test(api.clone(), usize::MAX, max * LARGE_XT_SIZE);

		let xts = (0..max).map(|x| Arc::from(large_uxt(x))).collect::<Vec<_>>();

		let low_prio = 0u64;
		let hi_prio = u64::MAX;

		let total_xts_bytes = xts.iter().fold(0, |r, x| r + api.hash_and_length(&x).1);
		let (submit_outcomes, hashes): (Vec<_>, Vec<_>) = xts
			.iter()
			.map(|t| {
				let h = api.hash_and_length(t).0;
				(ViewStoreSubmitOutcome::new(h, Some(low_prio)), h)
			})
			.unzip();

		let results = mempool.extend_unwatched(TransactionSource::External, &xts);
		assert!(results.iter().all(Result::is_ok));
		assert_eq!(mempool.bytes(), total_xts_bytes);
		assert_eq!(total_xts_bytes, max * LARGE_XT_SIZE);

		submit_outcomes
			.into_iter()
			.for_each(|o| mempool.update_transaction_priority(&o));

		//this one should drop 2 xts (size: 1130):
		let xt = Arc::from(ExtrinsicBuilder::new_include_data(vec![98 as u8; 1025]).build());
		let (hash, length) = api.hash_and_length(&xt);
		assert_eq!(length, 1130);
		let result = mempool
			.try_insert_with_replacement(xt, hi_prio, TransactionSource::External, false)
			.unwrap();

		assert_eq!(result.hash, hash);
		assert_eq!(result.removed, hashes[0..2]);
	}

	#[test]
	fn replacing_txs_removes_proper_size_and_prios() {
		sp_tracing::try_init_simple();
		const COUNT: usize = 10;
		let api = Arc::from(TestApi::default());
		let mempool = TxMemPool::new_test(api.clone(), usize::MAX, COUNT * LARGE_XT_SIZE);

		let xts = (0..COUNT).map(|x| Arc::from(large_uxt(x))).collect::<Vec<_>>();

		let hi_prio = u64::MAX;

		let total_xts_bytes = xts.iter().fold(0, |r, x| r + api.hash_and_length(&x).1);
		let (submit_outcomes, hashes): (Vec<_>, Vec<_>) = xts
			.iter()
			.enumerate()
			.map(|(prio, t)| {
				let h = api.hash_and_length(t).0;
				(ViewStoreSubmitOutcome::new(h, Some((COUNT - prio).try_into().unwrap())), h)
			})
			.unzip();

		let results = mempool.extend_unwatched(TransactionSource::External, &xts);
		assert!(results.iter().all(Result::is_ok));
		assert_eq!(mempool.bytes(), total_xts_bytes);

		submit_outcomes
			.into_iter()
			.for_each(|o| mempool.update_transaction_priority(&o));

		//this one should drop 3 xts (each of size 1129)
		let xt = Arc::from(ExtrinsicBuilder::new_include_data(vec![98 as u8; 2154]).build());
		let (hash, length) = api.hash_and_length(&xt);
		// overhead is 105, thus length: 105 + 2154
		assert_eq!(length, 2 * LARGE_XT_SIZE + 1);
		let result = mempool
			.try_insert_with_replacement(xt, hi_prio, TransactionSource::External, false)
			.unwrap();

		assert_eq!(result.hash, hash);
		assert!(result.removed.iter().eq(hashes[COUNT - 3..COUNT].iter().rev()));
	}

	#[test]
	fn replacing_txs_skips_lower_prio_tx() {
		sp_tracing::try_init_simple();
		const COUNT: usize = 10;
		let api = Arc::from(TestApi::default());
		let mempool = TxMemPool::new_test(api.clone(), usize::MAX, COUNT * LARGE_XT_SIZE);

		let xts = (0..COUNT).map(|x| Arc::from(large_uxt(x))).collect::<Vec<_>>();

		let hi_prio = 100u64;
		let low_prio = 10u64;

		let total_xts_bytes = xts.iter().fold(0, |r, x| r + api.hash_and_length(&x).1);
		let submit_outcomes = xts
			.iter()
			.map(|t| {
				let h = api.hash_and_length(t).0;
				ViewStoreSubmitOutcome::new(h, Some(hi_prio))
			})
			.collect::<Vec<_>>();

		let results = mempool.extend_unwatched(TransactionSource::External, &xts);
		assert!(results.iter().all(Result::is_ok));
		assert_eq!(mempool.bytes(), total_xts_bytes);

		submit_outcomes
			.into_iter()
			.for_each(|o| mempool.update_transaction_priority(&o));

		let xt = Arc::from(large_uxt(98));
		let result =
			mempool.try_insert_with_replacement(xt, low_prio, TransactionSource::External, false);

		// lower prio tx is rejected immediately
		assert!(matches!(
			result.unwrap_err(),
			sc_transaction_pool_api::error::Error::ImmediatelyDropped
		));
	}

	#[test]
	fn replacing_txs_is_skipped_if_prios_are_not_set() {
		sp_tracing::try_init_simple();
		const COUNT: usize = 10;
		let api = Arc::from(TestApi::default());
		let mempool = TxMemPool::new_test(api.clone(), usize::MAX, COUNT * LARGE_XT_SIZE);

		let xts = (0..COUNT).map(|x| Arc::from(large_uxt(x))).collect::<Vec<_>>();

		let hi_prio = u64::MAX;

		let total_xts_bytes = xts.iter().fold(0, |r, x| r + api.hash_and_length(&x).1);

		let results = mempool.extend_unwatched(TransactionSource::External, &xts);
		assert!(results.iter().all(Result::is_ok));
		assert_eq!(mempool.bytes(), total_xts_bytes);

		//this one could drop 3 xts (each of size 1129)
		let xt = Arc::from(ExtrinsicBuilder::new_include_data(vec![98 as u8; 2154]).build());
		let length = api.hash_and_length(&xt).1;
		// overhead is 105, thus length: 105 + 2154
		assert_eq!(length, 2 * LARGE_XT_SIZE + 1);

		let result =
			mempool.try_insert_with_replacement(xt, hi_prio, TransactionSource::External, false);

		// we did not update priorities (update_transaction_priority was not called):
		assert!(matches!(
			result.unwrap_err(),
			sc_transaction_pool_api::error::Error::ImmediatelyDropped
		));
	}
}
