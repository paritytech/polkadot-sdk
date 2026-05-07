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

//! Prometheus's metrics for a fork-aware transaction pool.

use super::tx_mem_pool::{InsertionInfo, InvalidTxReason, TxInMemPool};
use crate::{
	common::metrics::{GenericMetricsLink, MetricsRegistrant},
	graph::{
		self,
		base_pool::{TimedTransactionSource, Transaction},
		BlockHash, ExtrinsicHash,
	},
	LOG_TARGET,
};
use futures::{FutureExt, StreamExt};
use prometheus_endpoint::{
	exponential_buckets, histogram_opts, linear_buckets, register, Counter, CounterVec, Gauge,
	Histogram, HistogramOpts, HistogramVec, Opts, PrometheusError, Registry, U64,
};
#[cfg(doc)]
use sc_transaction_pool_api::TransactionPool;
use sc_transaction_pool_api::TransactionStatus;
use sc_utils::mpsc;
use sp_runtime::traits::Block as BlockT;
use std::{
	collections::{hash_map::Entry, HashMap},
	future::Future,
	pin::Pin,
	sync::Arc,
	time::{Duration, Instant},
};
use tracing::trace;

/// A helper alias for the Prometheus's metrics endpoint.
pub type MetricsLink = GenericMetricsLink<Metrics>;

/// Transaction pool Prometheus metrics.
pub struct Metrics {
	/// Total number of transactions submitted.
	pub submitted_transactions: Counter<U64>,
	/// Total number of currently maintained views.
	pub active_views: Gauge<U64>,
	/// Total number of current inactive views.
	pub inactive_views: Gauge<U64>,
	/// Total number of watched transactions in txpool.
	pub watched_txs: Gauge<U64>,
	/// Total number of unwatched transactions in txpool.
	pub unwatched_txs: Gauge<U64>,
	/// Total number of transactions reported as invalid.
	///
	/// This only includes transaction reported as invalid by the
	/// [`TransactionPool::report_invalid`] method.
	pub reported_invalid_txs: Counter<U64>,
	/// Total number of transactions removed as invalid.
	pub removed_invalid_txs: Counter<U64>,
	/// Total number of transactions from imported blocks that are unknown to the pool.
	pub unknown_from_block_import_txs: Counter<U64>,
	/// Total number of finalized transactions.
	pub finalized_txs: Counter<U64>,
	/// Histogram of maintain durations.
	pub maintain_duration: Histogram,
	/// Total number of transactions resubmitted from retracted forks.
	pub resubmitted_retracted_txs: Counter<U64>,
	/// Total number of transactions submitted from mempool to views.
	pub submitted_from_mempool_txs: Counter<U64>,
	/// Total number of transactions found as invalid during mempool revalidation.
	pub mempool_revalidation_invalid_txs: MempoolInvalidTxReasonCounter,
	/// Total number of transactions found as invalid during view revalidation.
	pub view_revalidation_invalid_txs: Counter<U64>,
	/// Total number of valid transactions processed during view revalidation.
	pub view_revalidation_resubmitted_txs: Counter<U64>,
	/// Histogram of view revalidation durations.
	pub view_revalidation_duration: Histogram,
	/// Total number of the views created w/o cloning existing view.
	pub non_cloned_views: Counter<U64>,
	/// Histograms to track the timing distribution of individual transaction pool events.
	pub events_histograms: EventsHistograms,
	/// Histogram of transaction age in the pool at the moment of removal, labeled by
	/// `reason` and `source`.
	pub tx_age_at_removal: TxAgeAtRemovalHistogram,
}

/// Represents a collection of histogram timings for different transaction statuses.
pub struct EventsHistograms {
	/// Histogram of timings for reporting `TransactionStatus::Future` event
	pub future: Histogram,
	/// Histogram of timings for reporting `TransactionStatus::Ready` event
	pub ready: Histogram,
	/// Histogram of timings for reporting `TransactionStatus::Broadcast` event
	pub broadcast: Histogram,
	/// Histogram of timings for reporting `TransactionStatus::InBlock` event
	pub in_block: Histogram,
	/// Histogram of timings for reporting `TransactionStatus::InBlock` event (unfiltered).
	///
	/// This is an experimental feature for reliability dashboard.
	/// Unlike `in_block`, this metric records every InBlock event, including duplicates
	/// when the same transaction appears in multiple blocks (e.g., during chain reorgs).
	pub in_block_forks: Histogram,
	/// Histogram of timings for reporting `TransactionStatus::Retracted` event
	pub retracted: Histogram,
	/// Histogram of timings for reporting `TransactionStatus::FinalityTimeout` event
	pub finality_timeout: Histogram,
	/// Histogram of timings for reporting `TransactionStatus::Finalized` event
	pub finalized: Histogram,
	/// Histogram of timings for reporting `TransactionStatus::Usurped(Hash)` event
	pub usurped: Histogram,
	/// Histogram of timings for reporting `TransactionStatus::Dropped` event
	pub dropped: Histogram,
	/// Histogram of timings for reporting `TransactionStatus::Invalid` event
	pub invalid: Histogram,
}

impl EventsHistograms {
	fn register(registry: &Registry) -> Result<Self, PrometheusError> {
		Ok(Self {
			future: register(
				Histogram::with_opts(histogram_opts!(
					"substrate_sub_txpool_timing_event_future",
					"Histogram of timings for reporting Future event",
					exponential_buckets(0.01, 2.0, 16).unwrap()
				))?,
				registry,
			)?,
			ready: register(
				Histogram::with_opts(histogram_opts!(
					"substrate_sub_txpool_timing_event_ready",
					"Histogram of timings for reporting Ready event",
					exponential_buckets(0.01, 2.0, 16).unwrap()
				))?,
				registry,
			)?,
			broadcast: register(
				Histogram::with_opts(histogram_opts!(
					"substrate_sub_txpool_timing_event_broadcast",
					"Histogram of timings for reporting Broadcast event",
					linear_buckets(0.01, 0.25, 16).unwrap()
				))?,
				registry,
			)?,
			in_block: register(
				Histogram::with_opts(
					histogram_opts!(
						"substrate_sub_txpool_timing_event_in_block",
						"Histogram of timings for reporting InBlock event"
					)
					.buckets(
						[
							linear_buckets(0.0, 3.0, 20).unwrap(),
							// requested in #9158
							vec![60.0, 75.0, 90.0, 120.0, 180.0],
						]
						.concat(),
					),
				)?,
				registry,
			)?,
			in_block_forks: register(
				Histogram::with_opts(
					histogram_opts!(
						"substrate_sub_txpool_timing_event_in_block_forks",
						"Histogram of timings for reporting unfiltered InBlock event (experimental feature for reliability dashboard)",
					)
					.buckets(
						[
							linear_buckets(0.0, 3.0, 20).unwrap(),
							// requested in #9158
							vec![60.0, 75.0, 90.0, 120.0, 180.0],
						]
						.concat(),
					),
				)?,
				registry,
			)?,
			retracted: register(
				Histogram::with_opts(histogram_opts!(
					"substrate_sub_txpool_timing_event_retracted",
					"Histogram of timings for reporting Retracted event",
					linear_buckets(0.0, 3.0, 20).unwrap()
				))?,
				registry,
			)?,
			finality_timeout: register(
				Histogram::with_opts(histogram_opts!(
					"substrate_sub_txpool_timing_event_finality_timeout",
					"Histogram of timings for reporting FinalityTimeout event",
					linear_buckets(0.0, 40.0, 20).unwrap()
				))?,
				registry,
			)?,
			finalized: register(
				Histogram::with_opts(
					histogram_opts!(
						"substrate_sub_txpool_timing_event_finalized",
						"Histogram of timings for reporting Finalized event"
					)
					.buckets(
						[
							// requested in #9158
							linear_buckets(0.0, 5.0, 8).unwrap(),
							linear_buckets(40.0, 40.0, 19).unwrap(),
						]
						.concat(),
					),
				)?,
				registry,
			)?,
			usurped: register(
				Histogram::with_opts(
					histogram_opts!(
						"substrate_sub_txpool_timing_event_usurped",
						"Histogram of timings for reporting Usurped event"
					)
					.buckets(
						[
							linear_buckets(0.0, 3.0, 20).unwrap(),
							// requested in #9158
							vec![60.0, 75.0, 90.0, 120.0, 180.0],
						]
						.concat(),
					),
				)?,
				registry,
			)?,
			dropped: register(
				Histogram::with_opts(
					histogram_opts!(
						"substrate_sub_txpool_timing_event_dropped",
						"Histogram of timings for reporting Dropped event"
					)
					.buckets(
						[
							linear_buckets(0.0, 3.0, 20).unwrap(),
							// requested in #9158
							vec![60.0, 75.0, 90.0, 120.0, 180.0],
						]
						.concat(),
					),
				)?,
				registry,
			)?,
			invalid: register(
				Histogram::with_opts(
					histogram_opts!(
						"substrate_sub_txpool_timing_event_invalid",
						"Histogram of timings for reporting Invalid event"
					)
					.buckets(
						[
							linear_buckets(0.0, 3.0, 20).unwrap(),
							// requested in #9158
							vec![60.0, 75.0, 90.0, 120.0, 180.0],
						]
						.concat(),
					),
				)?,
				registry,
			)?,
		})
	}

	/// Records the timing for a given transaction status.
	///
	/// This method records the duration, representing the time elapsed since the
	/// transaction was submitted until the event was reported. Based on the
	/// transaction status, it utilizes the appropriate histogram to log this duration.
	pub fn observe<Hash, BlockHash>(
		&self,
		status: TransactionStatus<Hash, BlockHash>,
		duration: Duration,
	) {
		let duration = duration.as_secs_f64();
		let histogram = match status {
			TransactionStatus::Future => &self.future,
			TransactionStatus::Ready => &self.ready,
			TransactionStatus::Broadcast(..) => &self.broadcast,
			TransactionStatus::InBlock(..) => &self.in_block,
			TransactionStatus::Retracted(..) => &self.retracted,
			TransactionStatus::FinalityTimeout(..) => &self.finality_timeout,
			TransactionStatus::Finalized(..) => &self.finalized,
			TransactionStatus::Usurped(..) => &self.usurped,
			TransactionStatus::Dropped => &self.dropped,
			TransactionStatus::Invalid => &self.invalid,
		};
		histogram.observe(duration);
	}
}

/// Represents a labeled counter of invalid tx reasons.
pub struct MempoolInvalidTxReasonCounter {
	inner: CounterVec<U64>,
}

impl MempoolInvalidTxReasonCounter {
	fn register(registry: &Registry) -> Result<Self, PrometheusError> {
		Ok(Self {
			inner: register(
				CounterVec::new(
					Opts::new(
						"substrate_sub_txpool_mempool_revalidation_invalid_txs_total",
						r#"Total number of transactions found as invalid during mempool revalidation.
						They are broken down into `category` and `reason` labels.
						- `category` can be `invalid`, `unknown`, `subtree` or `validation_failed`.
						- `reason` is more nuanced, but is worth mentioning that for `subtree` category,
						   the underlying reason can be one of the other categories."#,
					),
					&["category", "reason"],
				)?,
				registry,
			)?,
		})
	}

	// Increments the mempool invalid txs metrics based on a custom category & reason.
	pub fn observe(&self, label: &InvalidTxReason, count: u64) {
		self.inner
			.with_label_values(&[label.category().as_str(), label.reason().as_str()])
			.inc_by(count)
	}
}

/// Reason why a transaction was removed from the pool.
///
/// Used as a Prometheus label on the [`TxAgeAtRemovalHistogram`] metric. The
/// variants are mapped to short snake_case strings via `strum::AsRefStr`.
#[derive(Clone, Copy, Debug, strum::AsRefStr)]
#[strum(serialize_all = "snake_case")]
pub enum RemovalReason {
	/// Transaction was finalized in a block.
	Finalized,
	/// Transaction was found invalid during mempool revalidation.
	InvalidRevalidationMempool,
	/// Transaction was found invalid during view revalidation.
	InvalidRevalidationView,
	/// Transaction was reported as invalid via the `report_invalid` API.
	InvalidReported,
	/// Transaction was dropped because it was usurped by another (higher-priority) one.
	DroppedUsurped,
	/// Transaction was dropped because the pool's size limits were enforced.
	DroppedLimits,
	/// Transaction was dropped because the view marked it as invalid.
	DroppedInvalid,
	/// Transaction reached the configured finality timeout without being finalized.
	FinalityTimeout,
}

/// Provides access to the [`TimedTransactionSource`] of an object held in the
/// fork-aware transaction pool.
///
/// Used by [`TxAgeAtRemovalHistogram::observe_batch`] to record the residence
/// time of a batch of removed transactions without forcing each call site to
/// thread `source` and `timestamp` manually.
pub(super) trait HasSource {
	/// Returns a reference to the source of the transaction.
	fn timed_source(&self) -> &TimedTransactionSource;
}

/// Auto-impl so any `Arc<T: HasSource>` works without boilerplate.
impl<T: HasSource + ?Sized> HasSource for Arc<T> {
	fn timed_source(&self) -> &TimedTransactionSource {
		(**self).timed_source()
	}
}

/// Auto-impl so iterators over references (`(&items).into_iter()`) work
/// without forcing call sites to clone or move ownership.
impl<T: HasSource + ?Sized> HasSource for &T {
	fn timed_source(&self) -> &TimedTransactionSource {
		(**self).timed_source()
	}
}

impl<ChainApi, Block> HasSource for TxInMemPool<ChainApi, Block>
where
	Block: BlockT,
	ChainApi: graph::ChainApi<Block = Block> + 'static,
{
	fn timed_source(&self) -> &TimedTransactionSource {
		self.source_ref()
	}
}

impl<Hash, Extrinsic> HasSource for Transaction<Hash, Extrinsic> {
	fn timed_source(&self) -> &TimedTransactionSource {
		&self.source
	}
}

/// Histogram of transaction age in the pool at the moment of removal,
/// labeled by `reason` and `source`.
pub struct TxAgeAtRemovalHistogram {
	inner: HistogramVec,
}

impl TxAgeAtRemovalHistogram {
	fn register(registry: &Registry) -> Result<Self, PrometheusError> {
		// Exponential buckets covering ~1s up to ~24h:
		// 1s, 2.5s, 6.25s, 15.6s, 39s, 1.6m, 4m, 10.2m, 25.4m, 1.06h, 2.65h, 6.6h, 16.5h, 41.4h.
		let buckets = exponential_buckets(1.0, 2.5, 14)
			.expect("exponential_buckets parameters are valid; qed");
		Ok(Self {
			inner: register(
				HistogramVec::new(
					HistogramOpts::new(
						"substrate_sub_txpool_tx_age_at_removal_seconds",
						"Transaction age (in seconds) in the fork-aware pool at the moment of \
						removal, labeled by `reason` and `source`. Used to detect stuck \
						transactions (see issue #7791).",
					)
					.buckets(buckets),
					&["reason", "source"],
				)?,
				registry,
			)?,
		})
	}

	/// Records the residence time of a transaction leaving the pool.
	///
	/// `age` is `None` when the original submission timestamp is unavailable, which
	/// in production should never happen (timestamps are always set in the fork-aware
	/// pool). Such observations are routed to the highest bucket so they remain
	/// visible in dashboards rather than being silently dropped.
	pub fn observe(
		&self,
		reason: RemovalReason,
		source: sp_runtime::transaction_validity::TransactionSource,
		age: Option<Duration>,
	) {
		let value = age.map(|a| a.as_secs_f64()).unwrap_or(f64::INFINITY);
		self.inner.with_label_values(&[reason.as_ref(), source.as_ref()]).observe(value)
	}

	/// Records one observation per item, computing each transaction's age as
	/// `removed_at - timestamp`.
	///
	/// `removed_at` is taken once at the call site so every item in the batch
	/// is anchored against the same instant — this matches the existing manual
	/// loops the helper replaces.
	pub(super) fn observe_batch<I>(&self, reason: RemovalReason, removed_at: Instant, items: I)
	where
		I: IntoIterator,
		I::Item: HasSource,
	{
		for item in items {
			let ts = item.timed_source();
			let age = ts.timestamp.map(|t| removed_at.saturating_duration_since(t));
			self.observe(reason, ts.source, age);
		}
	}
}

impl MetricsRegistrant for Metrics {
	fn register(registry: &Registry) -> Result<Box<Self>, PrometheusError> {
		Ok(Box::from(Self {
			submitted_transactions: register(
				Counter::new(
					"substrate_sub_txpool_submitted_txs_total",
					"Total number of transactions submitted",
				)?,
				registry,
			)?,
			active_views: register(
				Gauge::new(
					"substrate_sub_txpool_active_views",
					"Total number of currently maintained views.",
				)?,
				registry,
			)?,
			inactive_views: register(
				Gauge::new(
					"substrate_sub_txpool_inactive_views",
					"Total number of current inactive views.",
				)?,
				registry,
			)?,
			watched_txs: register(
				Gauge::new(
					"substrate_sub_txpool_watched_txs",
					"Total number of watched transactions in txpool.",
				)?,
				registry,
			)?,
			unwatched_txs: register(
				Gauge::new(
					"substrate_sub_txpool_unwatched_txs",
					"Total number of unwatched transactions in txpool.",
				)?,
				registry,
			)?,
			reported_invalid_txs: register(
				Counter::new(
					"substrate_sub_txpool_reported_invalid_txs_total",
					"Total number of transactions reported as invalid by external entities using TxPool API.",
				)?,
				registry,
			)?,
			removed_invalid_txs: register(
				Counter::new(
					"substrate_sub_txpool_removed_invalid_txs_total",
					"Total number of transactions removed as invalid.",
				)?,
				registry,
			)?,
			unknown_from_block_import_txs: register(
				Counter::new(
					"substrate_sub_txpool_unknown_from_block_import_txs_total",
					"Total number of transactions from imported blocks that are unknown to the pool.",
				)?,
				registry,
			)?,
			finalized_txs: register(
				Counter::new(
					"substrate_sub_txpool_finalized_txs_total",
					"Total number of finalized transactions.",
				)?,
				registry,
			)?,
			maintain_duration: register(
				Histogram::with_opts(histogram_opts!(
					"substrate_sub_txpool_maintain_duration_seconds",
					"Histogram of maintain durations.",
					linear_buckets(0.0, 0.25, 13).unwrap()
				))?,
				registry,
			)?,
			resubmitted_retracted_txs: register(
				Counter::new(
					"substrate_sub_txpool_resubmitted_retracted_txs_total",
					"Total number of transactions resubmitted from retracted forks.",
				)?,
				registry,
			)?,
			submitted_from_mempool_txs: register(
				Counter::new(
					"substrate_sub_txpool_submitted_from_mempool_txs_total",
					"Total number of transactions submitted from mempool to views.",
				)?,
				registry,
			)?,
			mempool_revalidation_invalid_txs: MempoolInvalidTxReasonCounter::register(registry)?,
			view_revalidation_invalid_txs: register(
				Counter::new(
					"substrate_sub_txpool_view_revalidation_invalid_txs_total",
					"Total number of transactions found as invalid during view revalidation.",
				)?,
				registry,
			)?,
			view_revalidation_resubmitted_txs: register(
				Counter::new(
					"substrate_sub_txpool_view_revalidation_resubmitted_txs_total",
					"Total number of valid transactions processed during view revalidation.",
				)?,
				registry,
			)?,
			view_revalidation_duration: register(
				Histogram::with_opts(histogram_opts!(
					"substrate_sub_txpool_view_revalidation_duration_seconds",
					"Histogram of view revalidation durations.",
					linear_buckets(0.0, 0.25, 13).unwrap()
				))?,
				registry,
			)?,
			non_cloned_views: register(
				Counter::new(
					"substrate_sub_txpool_non_cloned_views_total",
					"Total number of the views created w/o cloning existing view.",
				)?,
				registry,
			)?,
			events_histograms: EventsHistograms::register(registry)?,
			tx_age_at_removal: TxAgeAtRemovalHistogram::register(registry)?,
		}))
	}
}

/// Messages used to report and compute event metrics.
enum EventMetricsMessage<Hash, BlockHash> {
	/// Message indicating a transaction has been submitted, including the timestamp
	/// and its hash.
	Submitted(Instant, Hash),
	/// Message indicating the new status of a transaction, including the timestamp and transaction
	/// hash.
	Status(Instant, Hash, TransactionStatus<Hash, BlockHash>),
}

/// Collects metrics related to transaction events.
pub struct EventsMetricsCollector<ChainApi: graph::ChainApi> {
	/// Optional channel for sending event metrics messages.
	///
	/// If `None` no event metrics are collected (e.g. in tests).
	metrics_message_sink: Option<MessageSink<ExtrinsicHash<ChainApi>, BlockHash<ChainApi>>>,
}

impl<ChainApi: graph::ChainApi> Default for EventsMetricsCollector<ChainApi> {
	fn default() -> Self {
		Self { metrics_message_sink: None }
	}
}

impl<ChainApi: graph::ChainApi> Clone for EventsMetricsCollector<ChainApi> {
	fn clone(&self) -> Self {
		Self { metrics_message_sink: self.metrics_message_sink.clone() }
	}
}

impl<ChainApi: graph::ChainApi> EventsMetricsCollector<ChainApi> {
	/// Reports the status of a transaction.
	///
	/// Takes a transaction hash and status, and attempts to send a status
	/// message to the metrics messages processing task.
	pub fn report_status(
		&self,
		tx_hash: ExtrinsicHash<ChainApi>,
		status: TransactionStatus<BlockHash<ChainApi>, ExtrinsicHash<ChainApi>>,
	) {
		self.metrics_message_sink.as_ref().map(|sink| {
			if let Err(error) =
				sink.unbounded_send(EventMetricsMessage::Status(Instant::now(), tx_hash, status))
			{
				trace!(target: LOG_TARGET, %error, "tx status metrics message send failed")
			}
		});
	}

	/// Reports that a transaction has been submitted.
	///
	/// Takes a transaction hash and its submission timestamp, and attempts to
	/// send a submission message to the metrics messages processing task.
	pub fn report_submitted(&self, insertion_info: &InsertionInfo<ExtrinsicHash<ChainApi>>) {
		self.metrics_message_sink.as_ref().map(|sink| {
			if let Err(error) = sink.unbounded_send(EventMetricsMessage::Submitted(
				insertion_info
					.source
					.timestamp
					.expect("timestamp is set in fork-aware pool. qed"),
				insertion_info.hash,
			)) {
				trace!(target: LOG_TARGET, %error, "tx status metrics message send failed")
			}
		});
	}
}

/// A type alias for a asynchronous task that collects metrics related to events.
pub type EventsMetricsCollectorTask = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Sink type for sending event metrics messages.
type MessageSink<Hash, BlockHash> =
	mpsc::TracingUnboundedSender<EventMetricsMessage<Hash, BlockHash>>;

/// Receiver type for receiving event metrics messages.
type MessageReceiver<Hash, BlockHash> =
	mpsc::TracingUnboundedReceiver<EventMetricsMessage<Hash, BlockHash>>;

/// Holds data relevant to transaction event metrics, allowing de-duplication
/// of certain transaction statuses, and compute the timings of events.
struct TransactionEventMetricsData {
	/// Flag indicating if the transaction was seen as `Ready`.
	ready_seen: bool,
	/// Flag indicating if the transaction was seen as `Broadcast`.
	broadcast_seen: bool,
	/// Flag indicating if the transaction was seen as `Future`.
	future_seen: bool,
	/// Flag indicating if the transaction was seen as `InBlock`.
	in_block_seen: bool,
	/// Flag indicating if the transaction was seen as `Retracted`.
	retracted_seen: bool,
	/// Timestamp when the transaction was submitted.
	///
	/// Used to compute a time elapsed until events are reported.
	submit_timestamp: Instant,
}

impl TransactionEventMetricsData {
	/// Creates a new `TransactionEventMetricsData` with the given timestamp.
	fn new(submit_timestamp: Instant) -> Self {
		Self {
			submit_timestamp,
			future_seen: false,
			ready_seen: false,
			broadcast_seen: false,
			in_block_seen: false,
			retracted_seen: false,
		}
	}

	/// Sets flag to true once.
	///
	/// Return true if flag was toggled.
	fn set_true_once(flag: &mut bool) -> bool {
		if *flag {
			false
		} else {
			*flag = true;
			true
		}
	}

	/// Updates the status flags based on the given transaction status.
	///
	/// Returns the submit timestamp if given status was not seen yet, `None` otherwise.
	fn update<Hash, BlockHash>(
		&mut self,
		status: &TransactionStatus<Hash, BlockHash>,
	) -> Option<Instant> {
		let flag = match *status {
			TransactionStatus::Ready => &mut self.ready_seen,
			TransactionStatus::Future => &mut self.future_seen,
			TransactionStatus::Broadcast(..) => &mut self.broadcast_seen,
			TransactionStatus::InBlock(..) => &mut self.in_block_seen,
			TransactionStatus::Retracted(..) => &mut self.retracted_seen,
			_ => return Some(self.submit_timestamp),
		};
		Self::set_true_once(flag).then_some(self.submit_timestamp)
	}
}

impl<ChainApi> EventsMetricsCollector<ChainApi>
where
	ChainApi: graph::ChainApi + 'static,
{
	/// Handles the status event.
	///
	/// Updates the metrics by observing the time taken for a transaction's status update
	/// from its submission time.
	fn handle_status(
		hash: ExtrinsicHash<ChainApi>,
		status: TransactionStatus<ExtrinsicHash<ChainApi>, BlockHash<ChainApi>>,
		timestamp: Instant,
		submitted_timestamp_map: &mut HashMap<ExtrinsicHash<ChainApi>, TransactionEventMetricsData>,
		metrics: &MetricsLink,
	) {
		let Entry::Occupied(mut entry) = submitted_timestamp_map.entry(hash) else { return };
		let remove = status.is_final();

		// Record unfiltered in_block_forks metric for EVERY InBlock event
		if matches!(status, TransactionStatus::InBlock(..)) {
			let duration = timestamp.duration_since(entry.get().submit_timestamp);
			metrics.report(|metrics| {
				metrics.events_histograms.in_block_forks.observe(duration.as_secs_f64())
			});
		}

		if let Some(submit_timestamp) = entry.get_mut().update(&status) {
			metrics.report(|metrics| {
				metrics
					.events_histograms
					.observe(status, timestamp.duration_since(submit_timestamp))
			});
		}
		remove.then(|| entry.remove());
	}

	/// Asynchronous task to process received messages and compute relevant event metrics.
	///
	/// Runs indefinitely, handling arriving messages and updating metrics
	/// based on the recorded submission times and timestamps of current event statuses.
	async fn task(
		mut rx: MessageReceiver<ExtrinsicHash<ChainApi>, BlockHash<ChainApi>>,
		metrics: MetricsLink,
	) {
		let mut submitted_timestamp_map =
			HashMap::<ExtrinsicHash<ChainApi>, TransactionEventMetricsData>::default();

		loop {
			match rx.next().await {
				Some(EventMetricsMessage::Submitted(timestamp, hash)) => {
					submitted_timestamp_map
						.insert(hash, TransactionEventMetricsData::new(timestamp));
				},
				Some(EventMetricsMessage::Status(timestamp, hash, status)) => {
					Self::handle_status(
						hash,
						status,
						timestamp,
						&mut submitted_timestamp_map,
						&metrics,
					);
				},
				None => {
					return; // ?
				},
			};
		}
	}

	/// Constructs a new `EventsMetricsCollector` and its associated worker task.
	///
	/// Returns the collector alongside an asynchronous task. The task shall be polled by caller.
	pub fn new_with_worker(metrics: MetricsLink) -> (Self, EventsMetricsCollectorTask) {
		const QUEUE_WARN_SIZE: usize = 100_000;
		let (metrics_message_sink, rx) =
			mpsc::tracing_unbounded("txpool-event-metrics-collector", QUEUE_WARN_SIZE);
		let task = Self::task(rx, metrics);

		(Self { metrics_message_sink: Some(metrics_message_sink) }, task.boxed())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_runtime::transaction_validity::TransactionSource;

	#[test]
	fn removal_reason_label_strings_are_stable() {
		// The metric labels are part of the public observability surface
		// and dashboards depend on them — keep this test in sync if you
		// rename a variant.
		assert_eq!(RemovalReason::Finalized.as_ref(), "finalized");
		assert_eq!(
			RemovalReason::InvalidRevalidationMempool.as_ref(),
			"invalid_revalidation_mempool",
		);
		assert_eq!(RemovalReason::InvalidRevalidationView.as_ref(), "invalid_revalidation_view");
		assert_eq!(RemovalReason::InvalidReported.as_ref(), "invalid_reported");
		assert_eq!(RemovalReason::DroppedUsurped.as_ref(), "dropped_usurped");
		assert_eq!(RemovalReason::DroppedLimits.as_ref(), "dropped_limits");
		assert_eq!(RemovalReason::DroppedInvalid.as_ref(), "dropped_invalid");
		assert_eq!(RemovalReason::FinalityTimeout.as_ref(), "finality_timeout");
	}

	#[test]
	fn source_label_maps_each_variant() {
		assert_eq!(TransactionSource::Local.as_ref(), "local");
		assert_eq!(TransactionSource::External.as_ref(), "external");
		assert_eq!(TransactionSource::InBlock.as_ref(), "in_block");
	}

	/// Returns the sample count of the histogram for the given (reason, source) pair,
	/// or `None` if the metric family is not present yet.
	fn sample_count(registry: &Registry, reason: &str, source: &str) -> Option<u64> {
		registry
			.gather()
			.into_iter()
			.find(|m| m.get_name() == "substrate_sub_txpool_tx_age_at_removal_seconds")?
			.take_metric()
			.into_iter()
			.find(|m| {
				let labels = m.get_label();
				labels.iter().any(|l| l.get_name() == "reason" && l.get_value() == reason) &&
					labels.iter().any(|l| l.get_name() == "source" && l.get_value() == source)
			})
			.map(|m| m.get_histogram().get_sample_count())
	}

	#[test]
	fn observe_records_one_sample_with_expected_labels() {
		let registry = Registry::new();
		let histogram = TxAgeAtRemovalHistogram::register(&registry).unwrap();

		histogram.observe(
			RemovalReason::Finalized,
			TransactionSource::External,
			Some(Duration::from_secs(42)),
		);

		assert_eq!(sample_count(&registry, "finalized", "external"), Some(1));
		// Other label combinations must not have been touched.
		assert_eq!(sample_count(&registry, "finalized", "local"), None);
		assert_eq!(sample_count(&registry, "dropped_usurped", "external"), None);
	}

	#[test]
	fn observe_groups_samples_with_matching_labels() {
		let registry = Registry::new();
		let histogram = TxAgeAtRemovalHistogram::register(&registry).unwrap();

		histogram.observe(
			RemovalReason::DroppedUsurped,
			TransactionSource::Local,
			Some(Duration::from_millis(100)),
		);
		histogram.observe(
			RemovalReason::DroppedUsurped,
			TransactionSource::Local,
			Some(Duration::from_secs(3600)),
		);
		histogram.observe(
			RemovalReason::DroppedUsurped,
			TransactionSource::External,
			Some(Duration::from_secs(5)),
		);

		assert_eq!(sample_count(&registry, "dropped_usurped", "local"), Some(2));
		assert_eq!(sample_count(&registry, "dropped_usurped", "external"), Some(1));
	}

	#[test]
	fn observe_with_none_age_records_in_highest_bucket() {
		let registry = Registry::new();
		let histogram = TxAgeAtRemovalHistogram::register(&registry).unwrap();

		histogram.observe(RemovalReason::Finalized, TransactionSource::External, None);

		// Sample is recorded (count = 1) but it lands above all finite buckets,
		// so it's only counted in the implicit `+Inf` bucket.
		assert_eq!(sample_count(&registry, "finalized", "external"), Some(1));
	}

	/// Test fixture implementing [`HasSource`] without depending on the full
	/// `TxInMemPool` / `Transaction` machinery.
	struct MockTimedSource(TimedTransactionSource);

	impl HasSource for MockTimedSource {
		fn timed_source(&self) -> &TimedTransactionSource {
			&self.0
		}
	}

	fn mock_item(source: TransactionSource, timestamp: Option<Instant>) -> MockTimedSource {
		MockTimedSource(TimedTransactionSource { source, timestamp })
	}

	#[test]
	fn observe_batch_records_one_sample_per_item() {
		let registry = Registry::new();
		let histogram = TxAgeAtRemovalHistogram::register(&registry).unwrap();
		let removed_at = Instant::now();
		let now = Some(removed_at);

		let items = vec![
			mock_item(TransactionSource::External, now),
			mock_item(TransactionSource::External, now),
			mock_item(TransactionSource::External, now),
		];

		histogram.observe_batch(RemovalReason::Finalized, removed_at, &items);

		assert_eq!(sample_count(&registry, "finalized", "external"), Some(3));
	}

	#[test]
	fn observe_batch_with_empty_iterator_is_noop() {
		let registry = Registry::new();
		let histogram = TxAgeAtRemovalHistogram::register(&registry).unwrap();

		let items: Vec<MockTimedSource> = Vec::new();
		histogram.observe_batch(RemovalReason::Finalized, Instant::now(), &items);

		assert_eq!(sample_count(&registry, "finalized", "external"), None);
		assert_eq!(sample_count(&registry, "finalized", "local"), None);
	}

	#[test]
	fn observe_batch_routes_per_source_label() {
		let registry = Registry::new();
		let histogram = TxAgeAtRemovalHistogram::register(&registry).unwrap();
		let removed_at = Instant::now();
		let now = Some(removed_at);

		let items = vec![
			mock_item(TransactionSource::Local, now),
			mock_item(TransactionSource::External, now),
			mock_item(TransactionSource::External, now),
			mock_item(TransactionSource::InBlock, now),
		];

		histogram.observe_batch(RemovalReason::DroppedUsurped, removed_at, &items);

		assert_eq!(sample_count(&registry, "dropped_usurped", "local"), Some(1));
		assert_eq!(sample_count(&registry, "dropped_usurped", "external"), Some(2));
		assert_eq!(sample_count(&registry, "dropped_usurped", "in_block"), Some(1));
	}

	#[test]
	fn observe_batch_routes_missing_timestamp_to_infinity_bucket() {
		let registry = Registry::new();
		let histogram = TxAgeAtRemovalHistogram::register(&registry).unwrap();

		let items = vec![
			mock_item(TransactionSource::External, None),
			mock_item(TransactionSource::External, None),
		];

		histogram.observe_batch(RemovalReason::FinalityTimeout, Instant::now(), &items);

		// Both samples are recorded; lacking a timestamp they land in the
		// implicit `+Inf` bucket per the same contract as `observe`.
		assert_eq!(sample_count(&registry, "finality_timeout", "external"), Some(2));
	}
}
