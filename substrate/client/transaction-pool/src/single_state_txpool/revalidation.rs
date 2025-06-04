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

//! Pool periodic revalidation.

use crate::graph::{BlockHash, ChainApi, ExtrinsicHash, ValidatedTransaction};
use futures::prelude::*;
use indexmap::IndexMap;
use sc_utils::mpsc::{tracing_unbounded, TracingUnboundedReceiver, TracingUnboundedSender};
use sp_runtime::{
	generic::BlockId, traits::SaturatedConversion, transaction_validity::TransactionValidityError,
};
use std::{
	collections::{BTreeMap, HashMap, HashSet},
	pin::Pin,
	sync::Arc,
	time::Duration,
};
use tracing::{debug, trace, warn};

const BACKGROUND_REVALIDATION_INTERVAL: Duration = Duration::from_millis(200);

const MIN_BACKGROUND_REVALIDATION_BATCH_SIZE: usize = 20;

const LOG_TARGET: &str = "txpool::revalidation";

type Pool<Api> = crate::graph::Pool<Api, ()>;

/// Payload from queue to worker.
struct WorkerPayload<Api: ChainApi> {
	at: BlockHash<Api>,
	transactions: Vec<ExtrinsicHash<Api>>,
}

/// Async revalidation worker.
///
/// Implements future and can be spawned in place or in background.
struct RevalidationWorker<Api: ChainApi> {
	api: Arc<Api>,
	pool: Arc<Pool<Api>>,
	best_block: BlockHash<Api>,
	block_ordered: BTreeMap<BlockHash<Api>, HashSet<ExtrinsicHash<Api>>>,
	members: HashMap<ExtrinsicHash<Api>, BlockHash<Api>>,
}

impl<Api: ChainApi> Unpin for RevalidationWorker<Api> {}

/// Revalidate batch of transaction.
///
/// Each transaction is validated  against chain, and invalid are
/// removed from the `pool`, while valid are resubmitted.
async fn batch_revalidate<Api: ChainApi>(
	pool: Arc<Pool<Api>>,
	api: Arc<Api>,
	at: BlockHash<Api>,
	batch: impl IntoIterator<Item = ExtrinsicHash<Api>>,
) {
	// This conversion should work. Otherwise, for unknown block the revalidation shall be skipped,
	// all the transactions will be kept in the validated pool, and can be scheduled for
	// revalidation with the next request.
	let block_number = match api.block_id_to_number(&BlockId::Hash(at)) {
		Ok(Some(n)) => n,
		Ok(None) => {
			trace!(
				target: LOG_TARGET,
				?at,
				"Revalidation skipped: could not get block number"
			);
			return
		},
		Err(error) => {
			trace!(
				target: LOG_TARGET,
				?at,
				?error,
				"Revalidation skipped."
			);
			return
		},
	};

	let mut invalid_hashes = Vec::new();
	let mut revalidated = IndexMap::new();

	let validation_results = futures::future::join_all(batch.into_iter().filter_map(|ext_hash| {
		pool.validated_pool().ready_by_hash(&ext_hash).map(|ext| {
			api.validate_transaction(at, ext.source.clone().into(), ext.data.clone())
				.map(move |validation_result| (validation_result, ext_hash, ext))
		})
	}))
	.await;

	for (validation_result, tx_hash, ext) in validation_results {
		match validation_result {
			Ok(Err(TransactionValidityError::Invalid(error))) => {
				trace!(
					target: LOG_TARGET,
					?tx_hash,
					?error,
					"Revalidation: invalid."
				);
				invalid_hashes.push(tx_hash);
			},
			Ok(Err(TransactionValidityError::Unknown(error))) => {
				// skipping unknown, they might be pushed by valid or invalid transaction
				// when latter resubmitted.
				trace!(
					target: LOG_TARGET,
					?tx_hash,
					?error,
					"Unknown during revalidation."
				);
			},
			Ok(Ok(validity)) => {
				revalidated.insert(
					tx_hash,
					ValidatedTransaction::valid_at(
						block_number.saturated_into::<u64>(),
						tx_hash,
						ext.source.clone(),
						ext.data.clone(),
						api.hash_and_length(&ext.data).1,
						validity,
					),
				);
			},
			Err(error) => {
				trace!(
					target: LOG_TARGET,
					?tx_hash,
					?error,
					"Removing due to error during revalidation."
				);
				invalid_hashes.push(tx_hash);
			},
		}
	}

	pool.validated_pool().remove_invalid(&invalid_hashes);
	if revalidated.len() > 0 {
		pool.resubmit(revalidated);
	}
}

impl<Api: ChainApi> RevalidationWorker<Api> {
	fn new(api: Arc<Api>, pool: Arc<Pool<Api>>, best_block: BlockHash<Api>) -> Self {
		Self {
			api,
			pool,
			best_block,
			block_ordered: Default::default(),
			members: Default::default(),
		}
	}

	fn prepare_batch(&mut self) -> Vec<ExtrinsicHash<Api>> {
		let mut queued_exts = Vec::new();
		let mut left =
			std::cmp::max(MIN_BACKGROUND_REVALIDATION_BATCH_SIZE, self.members.len() / 4);

		// Take maximum of count transaction by order
		// which they got into the pool
		while left > 0 {
			let first_block = match self.block_ordered.keys().next().cloned() {
				Some(bn) => bn,
				None => break,
			};
			let mut block_drained = false;
			if let Some(extrinsics) = self.block_ordered.get_mut(&first_block) {
				let to_queue = extrinsics.iter().take(left).cloned().collect::<Vec<_>>();
				if to_queue.len() == extrinsics.len() {
					block_drained = true;
				} else {
					for xt in &to_queue {
						extrinsics.remove(xt);
					}
				}
				left -= to_queue.len();
				queued_exts.extend(to_queue);
			}

			if block_drained {
				self.block_ordered.remove(&first_block);
			}
		}

		for hash in queued_exts.iter() {
			self.members.remove(hash);
		}

		queued_exts
	}

	fn len(&self) -> usize {
		self.block_ordered.iter().map(|b| b.1.len()).sum()
	}

	fn push(&mut self, worker_payload: WorkerPayload<Api>) {
		// we don't add something that already scheduled for revalidation
		let transactions = worker_payload.transactions;
		let block_number = worker_payload.at;

		for tx_hash in transactions {
			// we don't add something that already scheduled for revalidation
			if self.members.contains_key(&tx_hash) {
				trace!(
					target: LOG_TARGET,
					?tx_hash,
					"Skipped adding for revalidation: Already there."
				);

				continue
			}

			self.block_ordered
				.entry(block_number)
				.and_modify(|value| {
					value.insert(tx_hash);
				})
				.or_insert_with(|| {
					let mut bt = HashSet::new();
					bt.insert(tx_hash);
					bt
				});
			self.members.insert(tx_hash, block_number);
		}
	}

	/// Background worker main loop.
	///
	/// It does two things: periodically tries to process some transactions
	/// from the queue and also accepts messages to enqueue some more
	/// transactions from the pool.
	pub async fn run(
		mut self,
		from_queue: TracingUnboundedReceiver<WorkerPayload<Api>>,
		interval: Duration,
	) {
		let interval_fut = futures_timer::Delay::new(interval);
		let from_queue = from_queue.fuse();
		futures::pin_mut!(interval_fut, from_queue);
		let this = &mut self;

		loop {
			futures::select! {
				// Using `fuse()` in here is okay, because we reset the interval when it has fired.
				_ = (&mut interval_fut).fuse() => {
					let next_batch = this.prepare_batch();
					let batch_len = next_batch.len();

					batch_revalidate(this.pool.clone(), this.api.clone(), this.best_block, next_batch).await;

					if batch_len > 0 || this.len() > 0 {
						trace!(
							target: LOG_TARGET,
							batch_len,
							queue_len = this.len(),
							"Revalidated transactions. Left in the queue for revalidation."
						);
					}

					interval_fut.reset(interval);
				},
				workload = from_queue.next() => {
					match workload {
						Some(worker_payload) => {
							this.best_block = worker_payload.at;
							this.push(worker_payload);

							if this.members.len() > 0 {
								trace!(
									target: LOG_TARGET,
									at = ?this.best_block,
									transactions = ?this.members,
									"Updated revalidation queue."
								);
							}

							continue;
						},
						// R.I.P. worker!
						None => break,
					}
				}
			}
		}
	}
}

/// Revalidation queue.
///
/// Can be configured background (`new_background`)
/// or immediate (just `new`).
pub struct RevalidationQueue<Api: ChainApi> {
	pool: Arc<Pool<Api>>,
	api: Arc<Api>,
	background: Option<TracingUnboundedSender<WorkerPayload<Api>>>,
}

impl<Api: ChainApi> RevalidationQueue<Api>
where
	Api: 'static,
{
	/// New revalidation queue without background worker.
	pub fn new(api: Arc<Api>, pool: Arc<Pool<Api>>) -> Self {
		Self { api, pool, background: None }
	}

	/// New revalidation queue with background worker.
	pub fn new_with_interval(
		api: Arc<Api>,
		pool: Arc<Pool<Api>>,
		interval: Duration,
		best_block: BlockHash<Api>,
	) -> (Self, Pin<Box<dyn Future<Output = ()> + Send>>) {
		let (to_worker, from_queue) = tracing_unbounded("mpsc_revalidation_queue", 100_000);

		let worker = RevalidationWorker::new(api.clone(), pool.clone(), best_block);

		let queue = Self { api, pool, background: Some(to_worker) };

		(queue, worker.run(from_queue, interval).boxed())
	}

	/// New revalidation queue with background worker.
	pub fn new_background(
		api: Arc<Api>,
		pool: Arc<Pool<Api>>,
		best_block: BlockHash<Api>,
	) -> (Self, Pin<Box<dyn Future<Output = ()> + Send>>) {
		Self::new_with_interval(api, pool, BACKGROUND_REVALIDATION_INTERVAL, best_block)
	}

	/// Queue some transaction for later revalidation.
	///
	/// If queue configured with background worker, this will return immediately.
	/// If queue configured without background worker, this will resolve after
	/// revalidation is actually done.
	pub async fn revalidate_later(
		&self,
		at: BlockHash<Api>,
		transactions: Vec<ExtrinsicHash<Api>>,
	) {
		if transactions.len() > 0 {
			debug!(
				target: LOG_TARGET,
				transaction_count = transactions.len(),
				"Sent transactions to revalidation queue."
			);
		}

		if let Some(ref to_worker) = self.background {
			if let Err(error) = to_worker.unbounded_send(WorkerPayload { at, transactions }) {
				warn!(
					target: LOG_TARGET,
					?error,
					"Failed to update background worker."
				);
			}
		} else {
			debug!(
				target: LOG_TARGET,
				"Batch revalidate direct call."
			);
			let pool = self.pool.clone();
			let api = self.api.clone();
			batch_revalidate(pool, api, at, transactions).await
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		common::tests::{uxt, TestApi},
		graph::Pool,
		TimedTransactionSource,
	};
	use futures::executor::block_on;
	use substrate_test_runtime::{AccountId, Transfer, H256};
	use substrate_test_runtime_client::Sr25519Keyring::{Alice, Bob};

	#[test]
	fn revalidation_queue_works() {
		let api = Arc::new(TestApi::default());
		let pool = Arc::new(Pool::new_with_staticly_sized_rotator(
			Default::default(),
			true.into(),
			api.clone(),
		));
		let queue = Arc::new(RevalidationQueue::new(api.clone(), pool.clone()));

		let uxt = uxt(Transfer {
			from: Alice.into(),
			to: AccountId::from_h256(H256::from_low_u64_be(2)),
			amount: 5,
			nonce: 0,
		});

		let han_of_block0 = api.expect_hash_and_number(0);

		let uxt_hash = block_on(pool.submit_one(
			&han_of_block0,
			TimedTransactionSource::new_external(false),
			uxt.clone().into(),
		))
		.expect("Should be valid")
		.hash();

		block_on(queue.revalidate_later(han_of_block0.hash, vec![uxt_hash]));

		// revalidated in sync offload 2nd time
		assert_eq!(api.validation_requests().len(), 2);
		// number of ready
		assert_eq!(pool.validated_pool().status().ready, 1);
	}

	#[test]
	fn revalidation_queue_skips_revalidation_for_unknown_block_hash() {
		let api = Arc::new(TestApi::default());
		let pool = Arc::new(Pool::new_with_staticly_sized_rotator(
			Default::default(),
			true.into(),
			api.clone(),
		));
		let queue = Arc::new(RevalidationQueue::new(api.clone(), pool.clone()));

		let uxt0 = uxt(Transfer {
			from: Alice.into(),
			to: AccountId::from_h256(H256::from_low_u64_be(2)),
			amount: 5,
			nonce: 0,
		});
		let uxt1 = uxt(Transfer {
			from: Bob.into(),
			to: AccountId::from_h256(H256::from_low_u64_be(2)),
			amount: 4,
			nonce: 1,
		});

		let han_of_block0 = api.expect_hash_and_number(0);
		let unknown_block = H256::repeat_byte(0x13);

		let source = TimedTransactionSource::new_external(false);
		let uxt_hashes =
			block_on(pool.submit_at(
				&han_of_block0,
				vec![(source.clone(), uxt0.into()), (source, uxt1.into())],
			))
			.into_iter()
			.map(|r| r.expect("Should be valid").hash())
			.collect::<Vec<_>>();

		assert_eq!(api.validation_requests().len(), 2);
		assert_eq!(pool.validated_pool().status().ready, 2);

		// revalidation works fine for block 0:
		block_on(queue.revalidate_later(han_of_block0.hash, uxt_hashes.clone()));
		assert_eq!(api.validation_requests().len(), 4);
		assert_eq!(pool.validated_pool().status().ready, 2);

		// revalidation shall be skipped for unknown block:
		block_on(queue.revalidate_later(unknown_block, uxt_hashes));
		// no revalidation shall be done
		assert_eq!(api.validation_requests().len(), 4);
		// number of ready shall not change
		assert_eq!(pool.validated_pool().status().ready, 2);
	}
}
