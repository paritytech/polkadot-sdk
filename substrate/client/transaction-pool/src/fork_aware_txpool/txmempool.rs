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

use crate::{
	graph,
	graph::{ValidatedTransaction, ValidatedTransactionFor},
	log_xt_debug,
};
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
use crate::LOG_TARGET;
use sp_blockchain::HashAndNumber;
use sp_runtime::transaction_validity::TransactionValidityError;

#[derive(Debug)]
pub struct TxInMemPool<Block>
where
	Block: BlockT,
{
	//todo: add listener? for updating view with invalid transaction?
	watched: bool,
	tx: Block::Extrinsic,
	source: TransactionSource,
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

pub struct TxMemPool<PoolApi, Block>
where
	Block: BlockT,
	PoolApi: graph::ChainApi<Block = Block> + 'static,
{
	api: Arc<PoolApi>,
	//could be removed after removing watched (and adding listener into tx)
	listener: Arc<MultiViewListener<PoolApi>>,
	pub(super) pending_revalidation_result:
		RwLock<Option<Vec<(ExtrinsicHash<PoolApi>, ValidatedTransactionFor<PoolApi>)>>>,
	// todo:
	xts2: RwLock<HashMap<graph::ExtrinsicHash<PoolApi>, Arc<TxInMemPool<Block>>>>,
}

impl<PoolApi, Block> TxMemPool<PoolApi, Block>
where
	Block: BlockT,
	PoolApi: graph::ChainApi<Block = Block> + 'static,
	<Block as BlockT>::Hash: Unpin,
{
	pub(super) fn new(api: Arc<PoolApi>, listener: Arc<MultiViewListener<PoolApi>>) -> Self {
		Self {
			api,
			listener,
			pending_revalidation_result: Default::default(),
			xts2: Default::default(),
		}
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
		let xts = self.xts2.read();
		let watched_count = self.xts2.read().values().filter(|x| x.is_watched()).count();
		(xts.len() - watched_count, watched_count)
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
		let count = self.xts2.read().len();
		let start = Instant::now();

		let input = self
			.xts2
			.read()
			.clone()
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
				xt.1.validated_at.load(atomic::Ordering::Relaxed) + 10 < finalized_block_number
			})
			.take(1000);

		let futs = input.into_iter().map(|(xt_hash, xt)| {
			self.api
				.validate_transaction(finalized_block.hash, xt.source, xt.tx.clone())
				.map(move |validation_result| (xt_hash, xt, validation_result))
		});
		let validation_results = futures::future::join_all(futs).await;

		let duration = start.elapsed();

		let (invalid_hashes, revalidated): (Vec<_>, Vec<_>) = validation_results
			.into_iter()
			.partition(|(xt_hash, _, validation_result)| match validation_result {
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
			});

		let invalid_hashes = invalid_hashes.into_iter().map(|v| v.0).collect::<Vec<_>>();

		//todo: is it ok to overwrite validity?
		let pending_revalidation_result = revalidated
			.into_iter()
			.filter_map(|(xt_hash, xt, transaction_validity)| match transaction_validity {
				Ok(Ok(valid_transaction)) => Some((xt_hash, xt, valid_transaction)),
				_ => None,
			})
			.map(|(xt_hash, xt, valid_transaction)| {
				let xt_len = self.api.hash_and_length(&xt.tx).1;
				let block_number = finalized_block.number.into().as_u64();
				xt.validated_at.store(block_number, atomic::Ordering::Relaxed);
				(
					xt_hash,
					ValidatedTransaction::valid_at(
						block_number,
						xt_hash,
						xt.source,
						xt.tx.clone(),
						xt_len,
						valid_transaction,
					),
				)
			})
			.collect::<Vec<_>>();

		let pending_revalidation_len = pending_revalidation_result.len();
		log_xt_debug!(data: tuple, target: LOG_TARGET, &pending_revalidation_result,"[{:?}] purge_transactions, revalidated: {:?}");
		*self.pending_revalidation_result.write() = Some(pending_revalidation_result);

		log::info!(
			target: LOG_TARGET,
			"purge_transactions: at {finalized_block:?} count:{count:?} purged:{:?} revalidated:{pending_revalidation_len:?} took {duration:?}", invalid_hashes.len(),
		);

		invalid_hashes
	}

	pub(super) async fn purge_finalized_transactions(
		&self,
		finalized_xts: &Vec<ExtrinsicHash<PoolApi>>,
	) {
		log::info!(target: LOG_TARGET, "purge_finalized_transactions count:{:?}", finalized_xts.len());
		log_xt_debug!(target: LOG_TARGET, finalized_xts, "[{:?}] purged finalized transactions");
		self.xts2.write().retain(|hash, _| !finalized_xts.contains(&hash));
	}

	pub async fn purge_transactions(&self, finalized_block: HashAndNumber<Block>) {
		let invalid_hashes = self.validate_array(finalized_block.clone()).await;

		self.xts2.write().retain(|hash, _| !invalid_hashes.contains(&hash));
		self.listener.invalidate_transactions(invalid_hashes).await;
	}
}
