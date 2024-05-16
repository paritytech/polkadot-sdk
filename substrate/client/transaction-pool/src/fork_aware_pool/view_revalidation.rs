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

//! View background revalidation.

use std::{
	collections::{BTreeMap, HashMap, HashSet},
	pin::Pin,
	sync::Arc,
};

use crate::graph::{BlockHash, ChainApi, ExtrinsicHash, Pool, ValidatedTransaction};
use sc_utils::mpsc::{tracing_unbounded, TracingUnboundedReceiver, TracingUnboundedSender};
use sp_blockchain::HashAndNumber;
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, SaturatedConversion},
	transaction_validity::TransactionValidityError,
};

use super::{TxMemPool, View};
use futures::prelude::*;
use std::time::Duration;

const LOG_TARGET: &str = "txpool::v-revalidation";

// /// Payload from queue to worker.
// struct WorkerPayload<Api: ChainApi> {
// 	view: Arc<View<Api>>,
// }
/// Payload from queue to worker.
enum WorkerPayload<Api, Block>
where
	Block: BlockT,
	Api: ChainApi<Block = Block> + 'static,
{
	RevalidateMempool(Arc<TxMemPool<Api, Block>>, HashAndNumber<Block>),
}

/// Async revalidation worker.
///
/// Implements future and can be spawned in place or in background.
struct RevalidationWorker<Block: BlockT> {
	//what is already scheduled, so we don't need to duplicate work
	scheduled: HashSet<Block::Hash>,
}

// todo: ??? (remove?)
// impl<Block: BlockT> Unpin for RevalidationWorker<Block> {}

impl<Block> RevalidationWorker<Block>
where
	Block: BlockT,
	<Block as BlockT>::Hash: Unpin,
{
	fn new() -> Self {
		Self { scheduled: Default::default() }
	}

	/// Background worker main loop.
	///
	/// It does two things: periodically tries to process some transactions
	/// from the queue and also accepts messages to enqueue some more
	/// transactions from the pool.
	pub async fn run<Api: ChainApi<Block = Block> + 'static>(
		self,
		from_queue: TracingUnboundedReceiver<WorkerPayload<Api, Block>>,
	) {
		let mut from_queue = from_queue.fuse();

		loop {
			// Using `fuse()` in here is okay, because we reset the interval when it has fired.
			let Some(payload) = from_queue.next().await else {
				// R.I.P. worker!
				break;
			};
			match payload {
				WorkerPayload::RevalidateMempool(mempool, finalized_hash_and_number) =>
					(*mempool).purge_transactions(finalized_hash_and_number).await,
			};
		}
	}
}

/// Revalidation queue.
///
/// Can be configured background (`new_background`)
/// or immediate (just `new`).
pub struct RevalidationQueue<Api, Block>
where
	Api: ChainApi<Block = Block> + 'static,
	Block: BlockT,
{
	background: Option<TracingUnboundedSender<WorkerPayload<Api, Block>>>,
}

impl<Api, Block> RevalidationQueue<Api, Block>
where
	Api: ChainApi<Block = Block> + 'static,
	Block: BlockT,
	<Block as BlockT>::Hash: Unpin,
{
	/// New revalidation queue without background worker.
	pub fn new() -> Self {
		Self { background: None }
	}

	/// New revalidation queue with background worker.
	pub fn new_with_worker() -> (Self, Pin<Box<dyn Future<Output = ()> + Send>>) {
		let (to_worker, from_queue) = tracing_unbounded("mpsc_revalidation_queue", 100_000);
		(Self { background: Some(to_worker) }, RevalidationWorker::new().run(from_queue).boxed())
	}

	pub async fn purge_transactions_later(
		&self,
		mempool: Arc<TxMemPool<Api, Block>>,
		finalized_hash: HashAndNumber<Block>,
	) {
		log::info!(
			target: LOG_TARGET,
			"Sent mempool to revalidation queue at hash: {:?}",
			finalized_hash
		);

		if let Some(ref to_worker) = self.background {
			log::info!(
				target: LOG_TARGET,
				"revlidation send",
			);
			if let Err(e) =
				to_worker.unbounded_send(WorkerPayload::RevalidateMempool(mempool, finalized_hash))
			{
				log::warn!(target: LOG_TARGET, "Failed to update background worker: {:?}", e);
			}
		} else {
			mempool.purge_transactions(finalized_hash).await
		}
	}
}

#[cfg(test)]
//todo: add tests!
mod tests {
	use super::*;
	use crate::{
		graph::Pool,
		tests::{uxt, TestApi},
	};
	use futures::executor::block_on;
	use sc_transaction_pool_api::TransactionSource;
	use substrate_test_runtime::{AccountId, Transfer, H256};
	use substrate_test_runtime_client::AccountKeyring::{Alice, Bob};

	// #[test]
	// fn revalidation_queue_works() {
	// 	let api = Arc::new(TestApi::default());
	// 	let block0 = api.expect_hash_and_number(0);
	//
	// 	let view = Arc::new(View::new(api.clone(), block0));
	// 	let queue = Arc::new(RevalidationQueue::new());
	//
	// 	let uxt = uxt(Transfer {
	// 		from: Alice.into(),
	// 		to: AccountId::from_h256(H256::from_low_u64_be(2)),
	// 		amount: 5,
	// 		nonce: 0,
	// 	});
	//
	// 	let uxt_hash = block_on(view.submit_one(TransactionSource::External, uxt.clone()))
	// 		.expect("Should be valid");
	// 	assert_eq!(api.validation_requests().len(), 1);
	//
	// 	block_on(queue.revalidate_later(view.clone()));
	//
	// 	assert_eq!(api.validation_requests().len(), 2);
	// 	// number of ready
	// 	assert_eq!(view.status().ready, 1);
	// }

	// #[test]
	// fn revalidation_queue_skips_revalidation_for_unknown_block_hash() {
	// 	let api = Arc::new(TestApi::default());
	// 	let pool = Arc::new(Pool::new(Default::default(), true.into(), api.clone()));
	// 	let queue = Arc::new(RevalidationQueue::new(api.clone(), pool.clone()));
	//
	// 	let uxt0 = uxt(Transfer {
	// 		from: Alice.into(),
	// 		to: AccountId::from_h256(H256::from_low_u64_be(2)),
	// 		amount: 5,
	// 		nonce: 0,
	// 	});
	// 	let uxt1 = uxt(Transfer {
	// 		from: Bob.into(),
	// 		to: AccountId::from_h256(H256::from_low_u64_be(2)),
	// 		amount: 4,
	// 		nonce: 1,
	// 	});
	//
	// 	let han_of_block0 = api.expect_hash_and_number(0);
	// 	let unknown_block = H256::repeat_byte(0x13);
	//
	// 	let uxt_hashes =
	// 		block_on(pool.submit_at(&han_of_block0, TransactionSource::External, vec![uxt0, uxt1]))
	// 			.into_iter()
	// 			.map(|r| r.expect("Should be valid"))
	// 			.collect::<Vec<_>>();
	//
	// 	assert_eq!(api.validation_requests().len(), 2);
	// 	assert_eq!(pool.validated_pool().status().ready, 2);
	//
	// 	// revalidation works fine for block 0:
	// 	block_on(queue.revalidate_later(han_of_block0.hash, uxt_hashes.clone()));
	// 	assert_eq!(api.validation_requests().len(), 4);
	// 	assert_eq!(pool.validated_pool().status().ready, 2);
	//
	// 	// revalidation shall be skipped for unknown block:
	// 	block_on(queue.revalidate_later(unknown_block, uxt_hashes));
	// 	// no revalidation shall be done
	// 	assert_eq!(api.validation_requests().len(), 4);
	// 	// number of ready shall not change
	// 	assert_eq!(pool.validated_pool().status().ready, 2);
	// }
}
