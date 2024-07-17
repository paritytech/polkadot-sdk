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
//! - on some forks transaction can be invalid (view does not contain it), on other for tx can be
//!   valid.

use super::{metrics::MetricsLink as PrometheusMetrics, multi_view_listener::MultiViewListener};
use crate::{
	graph,
	graph::{ExtrinsicFor, ExtrinsicHash, RawExtrinsicFor},
	log_xt_debug, LOG_TARGET,
};
use futures::FutureExt;
use itertools::Itertools;
use parking_lot::RwLock;
use sc_transaction_pool_api::TransactionSource;
use sp_blockchain::HashAndNumber;
use sp_runtime::{
	traits::Block as BlockT,
	transaction_validity::{InvalidTransaction, TransactionValidityError},
};
use std::{
	collections::HashMap,
	sync::{atomic, atomic::AtomicU64, Arc},
	time::Instant,
};

/// The minimum interval between single transaction revalidations. Given in blocks.
const TXMEMPOOL_REVALIDATION_PERIOD: u64 = 10;

/// The number of transactions revalidated in single revalidation batch.
const TXMEMPOOL_MAX_REVALIDATION_BATCH_SIZE: usize = 1000;

/// Represents the transaction in the intermediary buffer.
#[derive(Debug)]
pub(crate) struct TxInMemPool<Block, ChainApi>
where
	Block: BlockT,
	ChainApi: graph::ChainApi<Block = Block> + 'static,
{
	//todo: add listener? for updating view with invalid transaction?
	/// is transaction watched
	watched: bool,
	/// transaction actual body
	tx: ExtrinsicFor<ChainApi>,
	/// transaction source
	pub(crate) source: TransactionSource,
	/// when transaction was revalidated, used to periodically revalidate mem pool buffer.
	validated_at: AtomicU64,
}

impl<Block, ChainApi> TxInMemPool<Block, ChainApi>
where
	Block: BlockT,
	ChainApi: graph::ChainApi<Block = Block> + 'static,
{
	fn is_watched(&self) -> bool {
		self.watched
	}

	fn new_unwatched(source: TransactionSource, tx: ExtrinsicFor<ChainApi>) -> Self {
		Self { watched: false, tx, source, validated_at: AtomicU64::new(0) }
	}

	fn new_watched(source: TransactionSource, tx: ExtrinsicFor<ChainApi>) -> Self {
		Self { watched: true, tx, source, validated_at: AtomicU64::new(0) }
	}

	pub(crate) fn tx(&self) -> ExtrinsicFor<ChainApi> {
		self.tx.clone()
	}
}

/// Intermediary transaction buffer.
///
/// Keeps all the transaction which are potentially valid. Transactions that were finalized or
/// transaction that are invalid at finalized blocks are removed.
pub(super) struct TxMemPool<ChainApi, Block>
where
	Block: BlockT,
	ChainApi: graph::ChainApi<Block = Block> + 'static,
{
	api: Arc<ChainApi>,
	//could be removed after removing watched field (and adding listener into tx)
	listener: Arc<MultiViewListener<ChainApi>>,
	xts2: RwLock<HashMap<ExtrinsicHash<ChainApi>, Arc<TxInMemPool<Block, ChainApi>>>>,
	metrics: PrometheusMetrics,
}

// Clumsy implementation - some improvements shall be done in the following code, use of Arc,
// redundant clones, naming..., etc...
impl<ChainApi, Block> TxMemPool<ChainApi, Block>
where
	Block: BlockT,
	ChainApi: graph::ChainApi<Block = Block> + 'static,
	<Block as BlockT>::Hash: Unpin,
{
	pub(super) fn new(
		api: Arc<ChainApi>,
		listener: Arc<MultiViewListener<ChainApi>>,
		metrics: PrometheusMetrics,
	) -> Self {
		Self { api, listener, xts2: Default::default(), metrics }
	}

	pub(super) fn get_by_hash(
		&self,
		hash: ExtrinsicHash<ChainApi>,
	) -> Option<ExtrinsicFor<ChainApi>> {
		self.xts2.read().get(&hash).map(|t| t.tx.clone())
	}

	pub(super) fn len(&self) -> (usize, usize) {
		let xts2 = self.xts2.read();
		let watched_count = xts2.values().filter(|x| x.is_watched()).count();
		(xts2.len() - watched_count, watched_count)
	}

	pub(super) fn push_unwatched(&self, source: TransactionSource, xt: ExtrinsicFor<ChainApi>) {
		let hash = self.api.hash_and_length(&xt).0;
		let unwatched = Arc::from(TxInMemPool::new_unwatched(source, xt));
		self.xts2.write().entry(hash).or_insert(unwatched);
	}

	pub(super) fn extend_unwatched(
		&self,
		source: TransactionSource,
		xts: Vec<ExtrinsicFor<ChainApi>>,
	) {
		let mut xts2 = self.xts2.write();
		xts.into_iter().for_each(|xt| {
			let hash = self.api.hash_and_length(&xt).0;
			let unwatched = Arc::from(TxInMemPool::new_unwatched(source, xt));
			xts2.entry(hash).or_insert(unwatched);
		});
	}

	pub(super) fn push_watched(&self, source: TransactionSource, xt: ExtrinsicFor<ChainApi>) {
		let hash = self.api.hash_and_length(&xt).0;
		let watched = Arc::from(TxInMemPool::new_watched(source, xt));
		self.xts2.write().entry(hash).or_insert(watched);
	}

	pub(super) fn clone_unwatched(
		&self,
	) -> HashMap<ExtrinsicHash<ChainApi>, Arc<TxInMemPool<Block, ChainApi>>> {
		self.xts2
			.read()
			.iter()
			.filter_map(|(hash, tx)| (!tx.is_watched()).then(|| (*hash, tx.clone())))
			.collect::<HashMap<_, _>>()
	}
	pub(super) fn clone_watched(
		&self,
	) -> HashMap<ExtrinsicHash<ChainApi>, Arc<TxInMemPool<Block, ChainApi>>> {
		self.xts2
			.read()
			.iter()
			.filter_map(|(hash, tx)| (tx.is_watched()).then(|| (*hash, tx.clone())))
			.collect::<HashMap<_, _>>()
	}

	pub(super) fn remove_watched(&self, xt: &RawExtrinsicFor<ChainApi>) {
		self.xts2.write().retain(|_, t| *t.tx != *xt);
	}

	/// Revalidates a batch of transactions.
	///
	/// Returns vec of invalid hashes.
	async fn revalidate(&self, finalized_block: HashAndNumber<Block>) -> Vec<Block::Hash> {
		log::debug!(target: LOG_TARGET, "mempool::revalidate at:{:?} {}", finalized_block, line!());
		let start = Instant::now();

		let (count, input) = {
			let xts2 = self.xts2.read();

			(
				xts2.len(),
				xts2.clone()
					.into_iter()
					.sorted_by(|a, b| {
						Ord::cmp(
							&a.1.validated_at.load(atomic::Ordering::Relaxed),
							&b.1.validated_at.load(atomic::Ordering::Relaxed),
						)
					})
					.filter(|xt| {
						let finalized_block_number = finalized_block.number.into().as_u64();
						xt.1.validated_at.load(atomic::Ordering::Relaxed) +
							TXMEMPOOL_REVALIDATION_PERIOD <
							finalized_block_number
					})
					.take(TXMEMPOOL_MAX_REVALIDATION_BATCH_SIZE),
			)
		};

		let futs = input.into_iter().map(|(xt_hash, xt)| {
			self.api
				.validate_transaction(finalized_block.hash, xt.source, xt.tx.clone())
				.map(move |validation_result| (xt_hash, xt, validation_result))
		});
		let validation_results = futures::future::join_all(futs).await;
		let input_len = validation_results.len();

		let duration = start.elapsed();

		let (invalid_hashes, _): (Vec<_>, Vec<_>) =
			validation_results.into_iter().partition(|(xt_hash, xt, validation_result)| {
				xt.validated_at
					.store(finalized_block.number.into().as_u64(), atomic::Ordering::Relaxed);
				match validation_result {
					Ok(Ok(_)) |
					Ok(Err(TransactionValidityError::Invalid(InvalidTransaction::Future))) => false,
					Err(_) |
					Ok(Err(TransactionValidityError::Unknown(_))) |
					Ok(Err(TransactionValidityError::Invalid(_))) => {
						log::debug!(
							target: LOG_TARGET,
							"[{:?}]: Purging: invalid: {:?}",
							xt_hash,
							validation_result,
						);
						true
					},
				}
			});

		let invalid_hashes = invalid_hashes.into_iter().map(|v| v.0).collect::<Vec<_>>();

		log::info!(
			target: LOG_TARGET,
			"mempool::revalidate: at {finalized_block:?} count:{input_len}/{count} purged:{} took {duration:?}", invalid_hashes.len(),
		);

		invalid_hashes
	}

	pub(super) async fn purge_finalized_transactions(
		&self,
		finalized_xts: &Vec<ExtrinsicHash<ChainApi>>,
	) {
		log::info!(target: LOG_TARGET, "purge_finalized_transactions count:{:?}", finalized_xts.len());
		log_xt_debug!(target: LOG_TARGET, finalized_xts, "[{:?}] purged finalized transactions");
		self.xts2.write().retain(|hash, _| !finalized_xts.contains(&hash));
	}

	pub(super) async fn purge_transactions(&self, finalized_block: HashAndNumber<Block>) {
		log::debug!(target: LOG_TARGET, "purge_transactions at:{:?}", finalized_block);
		let invalid_hashes = self.revalidate(finalized_block.clone()).await;

		let _ = invalid_hashes.len().try_into().map(|v| {
			self.metrics
				.report(|metrics| metrics.mempool_revalidation_invalid_txs.inc_by(v))
		});

		self.xts2.write().retain(|hash, _| !invalid_hashes.contains(&hash));
		self.listener.invalidate_transactions(invalid_hashes);
	}
}
