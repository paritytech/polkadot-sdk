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

//! The background worker for the [`View`] and [`TxMemPool`] revalidation.
//!
//! The [*Background tasks*](../index.html#background-tasks) section provides some extra details on
//! revalidation process.

use std::{marker::PhantomData, pin::Pin, sync::Arc};

use crate::{graph::ChainApi, LOG_TARGET};
use sc_utils::mpsc::{tracing_unbounded, TracingUnboundedReceiver, TracingUnboundedSender};
use sp_blockchain::HashAndNumber;
use sp_runtime::traits::Block as BlockT;

use super::tx_mem_pool::TxMemPool;
use futures::prelude::*;

use super::view::{FinishRevalidationWorkerChannels, View};

/// Revalidation request payload sent from the queue to the worker.
enum WorkerPayload<Api, Block>
where
	Block: BlockT,
	Api: ChainApi<Block = Block> + 'static,
{
	/// Request to revalidated the given instance of the [`View`]
	///
	/// Communication channels with maintain thread are also provided.
	RevalidateView(Arc<View<Api>>, FinishRevalidationWorkerChannels<Api>),
	/// Request to revalidated the given instance of the [`TxMemPool`] at provided block hash.
	RevalidateMempool(Arc<TxMemPool<Api, Block>>, HashAndNumber<Block>),
}

/// The background revalidation worker.
struct RevalidationWorker<Block: BlockT> {
	_phantom: PhantomData<Block>,
}

impl<Block> RevalidationWorker<Block>
where
	Block: BlockT,
	<Block as BlockT>::Hash: Unpin,
{
	/// Create a new instance of the background worker.
	fn new() -> Self {
		Self { _phantom: Default::default() }
	}

	/// A background worker main loop.
	///
	/// Waits for and dispatches the [`WorkerPayload`] messages sent from the
	/// [`RevalidationQueue`].
	pub async fn run<Api: ChainApi<Block = Block> + 'static>(
		self,
		from_queue: TracingUnboundedReceiver<WorkerPayload<Api, Block>>,
	) {
		let mut from_queue = from_queue.fuse();

		loop {
			let Some(payload) = from_queue.next().await else {
				// R.I.P. worker!
				break;
			};
			match payload {
				WorkerPayload::RevalidateView(view, worker_channels) =>
					view.revalidate(worker_channels).await,
				WorkerPayload::RevalidateMempool(mempool, finalized_hash_and_number) =>
					mempool.revalidate(finalized_hash_and_number).await,
			};
		}
	}
}

/// A Revalidation queue.
///
/// Allows to send the revalidation requests to the [`RevalidationWorker`].
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
	///
	/// All validation requests will be blocking.
	pub fn new() -> Self {
		Self { background: None }
	}

	/// New revalidation queue with background worker.
	///
	/// All validation requests will be executed in the background.
	pub fn new_with_worker() -> (Self, Pin<Box<dyn Future<Output = ()> + Send>>) {
		let (to_worker, from_queue) = tracing_unbounded("mpsc_revalidation_queue", 100_000);
		(Self { background: Some(to_worker) }, RevalidationWorker::new().run(from_queue).boxed())
	}

	/// Queue the view for later revalidation.
	///
	/// If the queue is configured with background worker, this will return immediately.
	/// If the queue is configured without background worker, this will resolve after
	/// revalidation is actually done.
	///
	/// Schedules execution of the [`View::revalidate`].
	pub async fn revalidate_view(
		&self,
		view: Arc<View<Api>>,
		finish_revalidation_worker_channels: FinishRevalidationWorkerChannels<Api>,
	) {
		log::trace!(
			target: LOG_TARGET,
			"revalidation_queue::revalidate_view: Sending view to revalidation queue at {}",
			view.at.hash
		);

		if let Some(ref to_worker) = self.background {
			if let Err(e) = to_worker.unbounded_send(WorkerPayload::RevalidateView(
				view,
				finish_revalidation_worker_channels,
			)) {
				log::warn!(target: LOG_TARGET, "revalidation_queue::revalidate_view: Failed to update background worker: {:?}", e);
			}
		} else {
			view.revalidate(finish_revalidation_worker_channels).await
		}
	}

	/// Revalidates the given mempool instance.
	///
	/// If queue configured with background worker, this will return immediately.
	/// If queue configured without background worker, this will resolve after
	/// revalidation is actually done.
	///
	/// Schedules execution of the [`TxMemPool::revalidate`].
	pub async fn revalidate_mempool(
		&self,
		mempool: Arc<TxMemPool<Api, Block>>,
		finalized_hash: HashAndNumber<Block>,
	) {
		log::trace!(
			target: LOG_TARGET,
			"Sent mempool to revalidation queue at hash: {:?}",
			finalized_hash
		);

		if let Some(ref to_worker) = self.background {
			if let Err(e) =
				to_worker.unbounded_send(WorkerPayload::RevalidateMempool(mempool, finalized_hash))
			{
				log::warn!(target: LOG_TARGET, "Failed to update background worker: {:?}", e);
			}
		} else {
			mempool.revalidate(finalized_hash).await
		}
	}
}

#[cfg(test)]
//todo: add more tests [#5480]
mod tests {
	use super::*;
	use crate::{
		common::tests::{uxt, TestApi},
		fork_aware_txpool::view::FinishRevalidationLocalChannels,
	};
	use futures::executor::block_on;
	use sc_transaction_pool_api::TransactionSource;
	use substrate_test_runtime::{AccountId, Transfer, H256};
	use substrate_test_runtime_client::AccountKeyring::Alice;
	#[test]
	fn revalidation_queue_works() {
		let api = Arc::new(TestApi::default());
		let block0 = api.expect_hash_and_number(0);

		let view = Arc::new(View::new(
			api.clone(),
			block0,
			Default::default(),
			Default::default(),
			false.into(),
		));
		let queue = Arc::new(RevalidationQueue::new());

		let uxt = uxt(Transfer {
			from: Alice.into(),
			to: AccountId::from_h256(H256::from_low_u64_be(2)),
			amount: 5,
			nonce: 0,
		});

		let _ = block_on(
			view.submit_many(TransactionSource::External, std::iter::once(uxt.clone().into())),
		);
		assert_eq!(api.validation_requests().len(), 1);

		let (finish_revalidation_request_tx, finish_revalidation_request_rx) =
			tokio::sync::mpsc::channel(1);
		let (revalidation_result_tx, revalidation_result_rx) = tokio::sync::mpsc::channel(1);

		let finish_revalidation_worker_channels = FinishRevalidationWorkerChannels::new(
			finish_revalidation_request_rx,
			revalidation_result_tx,
		);

		let _finish_revalidation_local_channels = FinishRevalidationLocalChannels::new(
			finish_revalidation_request_tx,
			revalidation_result_rx,
		);

		block_on(queue.revalidate_view(view.clone(), finish_revalidation_worker_channels));

		assert_eq!(api.validation_requests().len(), 2);
		// number of ready
		assert_eq!(view.status().ready, 1);
	}
}
