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

//! Transaction pool view.
//!
//! The View represents the state of the transaction pool at given block. The view is created when
//! new block is notified to transaction pool. Views are removed on finalization.
//!
//! Refer to [*View*](../index.html#view) section for more details.

use super::metrics::MetricsLink as PrometheusMetrics;
use crate::{
	common::log_xt::log_xt_trace,
	graph::{
		self, watcher::Watcher, ExtrinsicFor, ExtrinsicHash, IsValidator, ValidatedTransaction,
		ValidatedTransactionFor,
	},
	LOG_TARGET,
};
use parking_lot::Mutex;
use sc_transaction_pool_api::{error::Error as TxPoolError, PoolStatus, TransactionSource};
use sp_blockchain::HashAndNumber;
use sp_runtime::{
	generic::BlockId, traits::Block as BlockT, transaction_validity::TransactionValidityError,
	SaturatedConversion,
};
use std::{collections::HashMap, sync::Arc, time::Instant};

pub(super) struct RevalidationResult<ChainApi: graph::ChainApi> {
	revalidated: HashMap<ExtrinsicHash<ChainApi>, ValidatedTransactionFor<ChainApi>>,
	invalid_hashes: Vec<ExtrinsicHash<ChainApi>>,
}

/// Used to obtain result from RevalidationWorker on View side.
pub(super) type RevalidationResultReceiver<ChainApi> =
	tokio::sync::mpsc::Receiver<RevalidationResult<ChainApi>>;

/// Used to send revalidation result from RevalidationWorker to View.
pub(super) type RevalidationResultSender<ChainApi> =
	tokio::sync::mpsc::Sender<RevalidationResult<ChainApi>>;

/// Used to receive finish-revalidation-request from View on RevalidationWorker side.
pub(super) type FinishRevalidationRequestReceiver = tokio::sync::mpsc::Receiver<()>;

/// Used to send finish-revalidation-request from View to RevalidationWorker.
pub(super) type FinishRevalidationRequestSender = tokio::sync::mpsc::Sender<()>;

/// Endpoints of channels used on View side (maintain thread)
pub(super) struct FinishRevalidationLocalChannels<ChainApi: graph::ChainApi> {
	/// Used to send finish revalidation request.
	finish_revalidation_request_tx: Option<FinishRevalidationRequestSender>,
	/// Used to receive revalidation results.
	revalidation_result_rx: RevalidationResultReceiver<ChainApi>,
}

impl<ChainApi: graph::ChainApi> FinishRevalidationLocalChannels<ChainApi> {
	/// Creates a new instance of endpoints for channels used on View side
	pub fn new(
		finish_revalidation_request_tx: FinishRevalidationRequestSender,
		revalidation_result_rx: RevalidationResultReceiver<ChainApi>,
	) -> Self {
		Self {
			finish_revalidation_request_tx: Some(finish_revalidation_request_tx),
			revalidation_result_rx,
		}
	}

	/// Removes a finish revalidation sender
	///
	/// Should be called when revalidation was already terminated and finish revalidation message is
	/// no longer expected.
	fn remove_sender(&mut self) {
		self.finish_revalidation_request_tx = None;
	}
}

/// Endpoints of channels used on `RevalidationWorker` side (background thread)
pub(super) struct FinishRevalidationWorkerChannels<ChainApi: graph::ChainApi> {
	/// Used to receive finish revalidation request.
	finish_revalidation_request_rx: FinishRevalidationRequestReceiver,
	/// Used to send revalidation results.
	revalidation_result_tx: RevalidationResultSender<ChainApi>,
}

impl<ChainApi: graph::ChainApi> FinishRevalidationWorkerChannels<ChainApi> {
	/// Creates a new instance of endpoints for channels used on `RevalidationWorker` side
	pub fn new(
		finish_revalidation_request_rx: FinishRevalidationRequestReceiver,
		revalidation_result_tx: RevalidationResultSender<ChainApi>,
	) -> Self {
		Self { finish_revalidation_request_rx, revalidation_result_tx }
	}
}

/// Represents the state of transaction pool for given block.
///
/// Refer to [*View*](../index.html#view) section for more details on the purpose and life cycle of
/// the `View`.
pub(super) struct View<ChainApi: graph::ChainApi> {
	/// The internal pool keeping the set of ready and future transaction at the given block.
	pub(super) pool: graph::Pool<ChainApi>,
	/// The hash and number of the block with which this view is associated.
	pub(super) at: HashAndNumber<ChainApi::Block>,
	/// Endpoints of communication channel with background worker.
	revalidation_worker_channels: Mutex<Option<FinishRevalidationLocalChannels<ChainApi>>>,
	/// Prometheus's metrics endpoint.
	metrics: PrometheusMetrics,
}

impl<ChainApi> View<ChainApi>
where
	ChainApi: graph::ChainApi,
	<ChainApi::Block as BlockT>::Hash: Unpin,
{
	/// Creates a new empty view.
	pub(super) fn new(
		api: Arc<ChainApi>,
		at: HashAndNumber<ChainApi::Block>,
		options: graph::Options,
		metrics: PrometheusMetrics,
		is_validator: IsValidator,
	) -> Self {
		metrics.report(|metrics| metrics.non_cloned_views.inc());
		Self {
			pool: graph::Pool::new(options, is_validator, api),
			at,
			revalidation_worker_channels: Mutex::from(None),
			metrics,
		}
	}

	/// Creates a copy of the other view.
	pub(super) fn new_from_other(&self, at: &HashAndNumber<ChainApi::Block>) -> Self {
		View {
			at: at.clone(),
			pool: self.pool.deep_clone(),
			revalidation_worker_channels: Mutex::from(None),
			metrics: self.metrics.clone(),
		}
	}

	/// Imports many unvalidated extrinsics into the view.
	pub(super) async fn submit_many(
		&self,
		source: TransactionSource,
		xts: impl IntoIterator<Item = ExtrinsicFor<ChainApi>>,
	) -> Vec<Result<ExtrinsicHash<ChainApi>, ChainApi::Error>> {
		if log::log_enabled!(target: LOG_TARGET, log::Level::Trace) {
			let xts = xts.into_iter().collect::<Vec<_>>();
			log_xt_trace!(target: LOG_TARGET, xts.iter().map(|xt| self.pool.validated_pool().api().hash_and_length(xt).0), "[{:?}] view::submit_many at:{}", self.at.hash);
			self.pool.submit_at(&self.at, source, xts).await
		} else {
			self.pool.submit_at(&self.at, source, xts).await
		}
	}

	/// Import a single extrinsic and starts to watch its progress in the view.
	pub(super) async fn submit_and_watch(
		&self,
		source: TransactionSource,
		xt: ExtrinsicFor<ChainApi>,
	) -> Result<Watcher<ExtrinsicHash<ChainApi>, ExtrinsicHash<ChainApi>>, ChainApi::Error> {
		log::trace!(target: LOG_TARGET, "[{:?}] view::submit_and_watch at:{}", self.pool.validated_pool().api().hash_and_length(&xt).0, self.at.hash);
		self.pool.submit_and_watch(&self.at, source, xt).await
	}

	/// Synchronously imports single unvalidated extrinsics into the view.
	pub(super) fn submit_local(
		&self,
		xt: ExtrinsicFor<ChainApi>,
	) -> Result<ExtrinsicHash<ChainApi>, ChainApi::Error> {
		let (hash, length) = self.pool.validated_pool().api().hash_and_length(&xt);
		log::trace!(target: LOG_TARGET, "[{:?}] view::submit_local at:{}", hash, self.at.hash);

		let validity = self
			.pool
			.validated_pool()
			.api()
			.validate_transaction_blocking(
				self.at.hash,
				TransactionSource::Local,
				Arc::from(xt.clone()),
			)?
			.map_err(|e| {
				match e {
					TransactionValidityError::Invalid(i) => TxPoolError::InvalidTransaction(i),
					TransactionValidityError::Unknown(u) => TxPoolError::UnknownTransaction(u),
				}
				.into()
			})?;

		let block_number = self
			.pool
			.validated_pool()
			.api()
			.block_id_to_number(&BlockId::hash(self.at.hash))?
			.ok_or_else(|| TxPoolError::InvalidBlockId(format!("{:?}", self.at.hash)))?;

		let validated = ValidatedTransaction::valid_at(
			block_number.saturated_into::<u64>(),
			hash,
			TransactionSource::Local,
			Arc::from(xt),
			length,
			validity,
		);

		self.pool.validated_pool().submit(vec![validated]).remove(0)
	}

	/// Status of the pool associated with the view.
	pub(super) fn status(&self) -> PoolStatus {
		self.pool.validated_pool().status()
	}

	/// Creates a watcher for given transaction.
	///
	/// Intended to be called for the transaction that already exists in the pool
	pub(super) fn create_watcher(
		&self,
		tx_hash: ExtrinsicHash<ChainApi>,
	) -> Watcher<ExtrinsicHash<ChainApi>, ExtrinsicHash<ChainApi>> {
		//todo(minor): some assert could be added here - to make sure that transaction actually
		// exists in the view.
		self.pool.validated_pool().create_watcher(tx_hash)
	}

	/// Revalidates some part of transaction from the internal pool.
	///
	/// Intended to be called from the revalidation worker. The revalidation process can be
	/// terminated by sending a message to the `rx` channel provided within
	/// `finish_revalidation_worker_channels`. Revalidation results are sent back over the `tx`
	/// channels and shall be applied in maintain thread.
	///
	/// View revalidation currently is not throttled, and until not terminated it will revalidate
	/// all the transactions. Note: this can be improved if CPU usage due to revalidation becomes a
	/// problem.
	pub(super) async fn revalidate(
		&self,
		finish_revalidation_worker_channels: FinishRevalidationWorkerChannels<ChainApi>,
	) {
		let FinishRevalidationWorkerChannels {
			mut finish_revalidation_request_rx,
			revalidation_result_tx,
		} = finish_revalidation_worker_channels;

		log::trace!(target:LOG_TARGET, "view::revalidate: at {} starting", self.at.hash);
		let start = Instant::now();
		let validated_pool = self.pool.validated_pool();
		let api = validated_pool.api();

		let batch: Vec<_> = validated_pool.ready().collect();
		let batch_len = batch.len();

		//todo: sort batch by revalidation timestamp | maybe not needed at all? xts will be getting
		//out of the view...
		//todo: revalidate future, remove if invalid [#5496]

		let mut invalid_hashes = Vec::new();
		let mut revalidated = HashMap::new();

		let mut validation_results = vec![];
		let mut batch_iter = batch.into_iter();
		loop {
			let mut should_break = false;
			tokio::select! {
				_ = finish_revalidation_request_rx.recv() => {
					log::trace!(target: LOG_TARGET, "view::revalidate: finish revalidation request received at {}.", self.at.hash);
					break
				}
				_ = async {
					if let Some(tx) = batch_iter.next() {
						let validation_result = (api.validate_transaction(self.at.hash, tx.source, tx.data.clone()).await, tx.hash, tx);
						validation_results.push(validation_result);
					} else {
						self.revalidation_worker_channels.lock().as_mut().map(|ch| ch.remove_sender());
						should_break = true;
					}
				} => {}
			}

			if should_break {
				break;
			}
		}

		let revalidation_duration = start.elapsed();
		self.metrics.report(|metrics| {
			metrics.view_revalidation_duration.observe(revalidation_duration.as_secs_f64());
		});
		log::debug!(
			target:LOG_TARGET,
			"view::revalidate: at {:?} count: {}/{} took {:?}",
			self.at.hash,
			validation_results.len(),
			batch_len,
			revalidation_duration
		);
		log_xt_trace!(data:tuple, target:LOG_TARGET, validation_results.iter().map(|x| (x.1, &x.0)), "[{:?}] view::revalidateresult: {:?}");

		for (validation_result, tx_hash, tx) in validation_results {
			match validation_result {
				Ok(Err(TransactionValidityError::Invalid(_))) => {
					invalid_hashes.push(tx_hash);
				},
				Ok(Ok(validity)) => {
					revalidated.insert(
						tx_hash,
						ValidatedTransaction::valid_at(
							self.at.number.saturated_into::<u64>(),
							tx_hash,
							tx.source,
							tx.data.clone(),
							api.hash_and_length(&tx.data).1,
							validity,
						),
					);
				},
				Ok(Err(TransactionValidityError::Unknown(e))) => {
					log::trace!(
						target: LOG_TARGET,
						"[{:?}]: Removing. Cannot determine transaction validity: {:?}",
						tx_hash,
						e
					);
					invalid_hashes.push(tx_hash);
				},
				Err(validation_err) => {
					log::trace!(
						target: LOG_TARGET,
						"[{:?}]: Removing due to error during revalidation: {}",
						tx_hash,
						validation_err
					);
					invalid_hashes.push(tx_hash);
				},
			}
		}

		log::trace!(target:LOG_TARGET, "view::revalidate: sending revalidation result at {}", self.at.hash);
		if let Err(e) = revalidation_result_tx
			.send(RevalidationResult { invalid_hashes, revalidated })
			.await
		{
			log::trace!(target:LOG_TARGET, "view::revalidate: sending revalidation_result at {} failed {:?}", self.at.hash, e);
		}
	}

	/// Sends revalidation request to the background worker.
	///
	/// Creates communication channels required to stop revalidation request and receive the
	/// revalidation results and sends the revalidation request to the background worker.
	///
	/// Intended to be called from maintain thread, at the very end of the maintain process.
	///
	/// Refer to [*View revalidation*](../index.html#view-revalidation) for more details.
	pub(super) async fn start_background_revalidation(
		view: Arc<Self>,
		revalidation_queue: Arc<
			super::revalidation_worker::RevalidationQueue<ChainApi, ChainApi::Block>,
		>,
	) {
		log::trace!(target:LOG_TARGET,"view::start_background_revalidation: at {}", view.at.hash);
		let (finish_revalidation_request_tx, finish_revalidation_request_rx) =
			tokio::sync::mpsc::channel(1);
		let (revalidation_result_tx, revalidation_result_rx) = tokio::sync::mpsc::channel(1);

		let finish_revalidation_worker_channels = FinishRevalidationWorkerChannels::new(
			finish_revalidation_request_rx,
			revalidation_result_tx,
		);

		let finish_revalidation_local_channels = FinishRevalidationLocalChannels::new(
			finish_revalidation_request_tx,
			revalidation_result_rx,
		);

		*view.revalidation_worker_channels.lock() = Some(finish_revalidation_local_channels);
		revalidation_queue
			.revalidate_view(view.clone(), finish_revalidation_worker_channels)
			.await;
	}

	/// Terminates a background view revalidation.
	///
	/// Receives the results from the background worker and applies them to the internal pool.
	/// Intended to be called from the maintain thread, at the very beginning of the maintain
	/// process, before the new view is cloned and updated. Applying results before cloning ensures
	/// that view contains up-to-date set of revalidated transactions.
	///
	/// Refer to [*View revalidation*](../index.html#view-revalidation) for more details.
	pub(super) async fn finish_revalidation(&self) {
		log::trace!(target:LOG_TARGET,"view::finish_revalidation: at {}", self.at.hash);
		let Some(revalidation_worker_channels) = self.revalidation_worker_channels.lock().take()
		else {
			log::trace!(target:LOG_TARGET, "view::finish_revalidation: no finish_revalidation_request_tx");
			return
		};

		let FinishRevalidationLocalChannels {
			finish_revalidation_request_tx,
			mut revalidation_result_rx,
		} = revalidation_worker_channels;

		if let Some(finish_revalidation_request_tx) = finish_revalidation_request_tx {
			if let Err(e) = finish_revalidation_request_tx.send(()).await {
				log::trace!(target:LOG_TARGET, "view::finish_revalidation: sending cancellation request at {} failed {:?}", self.at.hash, e);
			}
		}

		if let Some(revalidation_result) = revalidation_result_rx.recv().await {
			let start = Instant::now();
			let revalidated_len = revalidation_result.revalidated.len();
			let validated_pool = self.pool.validated_pool();
			validated_pool.remove_invalid(&revalidation_result.invalid_hashes);
			if revalidated_len > 0 {
				self.pool.resubmit(revalidation_result.revalidated);
			}

			self.metrics.report(|metrics| {
				let _ = (
					revalidation_result
						.invalid_hashes
						.len()
						.try_into()
						.map(|v| metrics.view_revalidation_invalid_txs.inc_by(v)),
					revalidated_len
						.try_into()
						.map(|v| metrics.view_revalidation_resubmitted_txs.inc_by(v)),
				);
			});

			log::debug!(
				target:LOG_TARGET,
				"view::finish_revalidation: applying revalidation result invalid: {} revalidated: {} at {:?} took {:?}",
				revalidation_result.invalid_hashes.len(),
				revalidated_len,
				self.at.hash,
				start.elapsed()
			);
		}
	}
}
