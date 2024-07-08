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
//!   some transactions
//! could be lost)
//! - on some forks transaction can be invalid (view does not contain it), on other for tx can be
//!   valid.

use crate::{graph, log_xt_debug, LOG_TARGET};
use itertools::Itertools;
use parking_lot::RwLock;
use sp_runtime::transaction_validity::InvalidTransaction;
use std::{
	collections::HashMap,
	sync::{atomic, atomic::AtomicU64, Arc},
};

use crate::graph::ExtrinsicHash;
use futures::FutureExt;
use sc_transaction_pool_api::TransactionSource;
use sp_runtime::traits::Block as BlockT;
use std::time::Instant;

use super::multi_view_listener::MultiViewListener;
use sp_blockchain::HashAndNumber;
use sp_runtime::transaction_validity::TransactionValidityError;

/// Represents the transaction in the intermediary buffer.
#[derive(Debug)]
struct TxInMemPool<Block>
where
	Block: BlockT,
{
	//todo: add listener? for updating view with invalid transaction?
	/// is transaction watched
	watched: bool,
	//todo: Arc?
	/// transaction actual body
	tx: Block::Extrinsic,
	/// transaction source
	source: TransactionSource,
	/// when transaction was revalidated, used to periodically revalidate mem pool buffer.
	validated_at: AtomicU64,
}

impl<Block: BlockT> TxInMemPool<Block> {
	fn is_watched(&self) -> bool {
		self.watched
	}

	fn unwatched(tx: Block::Extrinsic) -> Self {
		Self {
			watched: false,
			tx,
			source: TransactionSource::External,
			validated_at: AtomicU64::new(0),
		}
	}

	fn watched(tx: Block::Extrinsic) -> Self {
		Self {
			watched: true,
			tx,
			source: TransactionSource::External,
			validated_at: AtomicU64::new(0),
		}
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
	xts2: RwLock<HashMap<graph::ExtrinsicHash<ChainApi>, Arc<TxInMemPool<Block>>>>,
}

// Clumsy implementation - some improvements shall be done in the following code, use of Arc,
// redundant clones, naming..., etc...
impl<ChainApi, Block> TxMemPool<ChainApi, Block>
where
	Block: BlockT,
	ChainApi: graph::ChainApi<Block = Block> + 'static,
	<Block as BlockT>::Hash: Unpin,
{
	pub(super) fn new(api: Arc<ChainApi>, listener: Arc<MultiViewListener<ChainApi>>) -> Self {
		Self { api, listener, xts2: Default::default() }
	}

	pub(super) fn watched_xts(&self) -> impl Iterator<Item = Block::Extrinsic> {
		self.xts2
			.read()
			.values()
			.filter_map(|x| x.is_watched().then(|| x.tx.clone()))
			.collect::<Vec<_>>()
			.into_iter()
	}

	pub(super) fn len(&self) -> (usize, usize) {
		let xts2 = self.xts2.read();
		let watched_count = xts2.values().filter(|x| x.is_watched()).count();
		(xts2.len() - watched_count, watched_count)
	}

	pub(super) fn push_unwatched(&self, xt: Block::Extrinsic) {
		let hash = self.api.hash_and_length(&xt).0;
		let unwatched = Arc::from(TxInMemPool::unwatched(xt));
		self.xts2.write().entry(hash).or_insert(unwatched);
	}

	pub(super) fn extend_unwatched(&self, xts: Vec<Block::Extrinsic>) {
		let mut xts2 = self.xts2.write();
		xts.into_iter().for_each(|xt| {
			let hash = self.api.hash_and_length(&xt).0;
			let unwatched = Arc::from(TxInMemPool::unwatched(xt));
			xts2.entry(hash).or_insert(unwatched);
		});
	}

	pub(super) fn push_watched(&self, xt: Block::Extrinsic) {
		let hash = self.api.hash_and_length(&xt).0;
		let watched = Arc::from(TxInMemPool::watched(xt));
		self.xts2.write().entry(hash).or_insert(watched);
	}

	pub(super) fn clone_unwatched(&self) -> Vec<Block::Extrinsic> {
		self.xts2
			.read()
			.values()
			.filter_map(|x| (!x.is_watched()).then(|| x.tx.clone()))
			.collect::<Vec<_>>()
	}

	pub(super) fn remove_watched(&self, xt: &Block::Extrinsic) {
		self.xts2.write().retain(|_, t| t.tx != *xt);
	}

	//returns vec of invalid hashes
	async fn validate_array(&self, finalized_block: HashAndNumber<Block>) -> Vec<Block::Hash> {
		log::debug!(target: LOG_TARGET, "validate_array at:{:?} {}", finalized_block, line!());
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
					//todo: add const
					//todo: add threshold (min revalidated, but older than e.g. 10 blocks)
					//threshold ~~> finality period?
					//count ~~> 25% of block?
					.filter(|xt| {
						let finalized_block_number = finalized_block.number.into().as_u64();
						xt.1.validated_at.load(atomic::Ordering::Relaxed) + 10 <
							finalized_block_number
					})
					.take(1000),
			)
		};

		let futs = input.into_iter().map(|(xt_hash, xt)| {
			self.api
				.validate_transaction(finalized_block.hash, xt.source, xt.tx.clone())
				.map(move |validation_result| (xt_hash, xt, validation_result))
		});
		let validation_results = futures::future::join_all(futs).await;

		let duration = start.elapsed();

		let (invalid_hashes, _): (Vec<_>, Vec<_>) =
			validation_results.into_iter().partition(|(xt_hash, _, validation_result)| {
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
			"purge_transactions: at {finalized_block:?} count:{count:?} purged:{:?} took {duration:?}", invalid_hashes.len(),
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
		let invalid_hashes = self.validate_array(finalized_block.clone()).await;

		self.xts2.write().retain(|hash, _| !invalid_hashes.contains(&hash));
		self.listener.invalidate_transactions(invalid_hashes);
	}
}
