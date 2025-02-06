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

use crate::common::metrics::{GenericMetricsLink, MetricsRegistrant};
use futures::FutureExt;
use prometheus_endpoint::{
	histogram_opts, linear_buckets, register, Counter, Gauge, Histogram, PrometheusError, Registry,
	U64,
};
use sc_transaction_pool_api::TransactionStatus;
use sc_utils::mpsc;
use std::{
	collections::HashMap,
	future::Future,
	pin::Pin,
	time::{Duration, Instant},
};

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
	pub removed_invalid_txs: Counter<U64>,
	/// Total number of finalized transactions.
	pub finalized_txs: Counter<U64>,
	/// Histogram of maintain durations.
	pub maintain_duration: Histogram,
	/// Total number of transactions resubmitted from retracted forks.
	pub resubmitted_retracted_txs: Counter<U64>,
	/// Total number of transactions submitted from mempool to views.
	pub submitted_from_mempool_txs: Counter<U64>,
	/// Total number of transactions found as invalid during mempool revalidation.
	pub mempool_revalidation_invalid_txs: Counter<U64>,
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
}

pub struct EventsHistograms {
	/// Histogram of timings for reporting `TransactionStatus::Future` event
	pub future: Histogram,
	/// Histogram of timings for reporting `TransactionStatus::Ready` event
	pub ready: Histogram,
	/// Histogram of timings for reporting `TransactionStatus::Broadcast` event
	pub broadcast: Histogram,
	/// Histogram of timings for reporting `TransactionStatus::InBlock` event
	pub in_block: Histogram,
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
					linear_buckets(0.0, 0.25, 13).unwrap()
				))?,
				registry,
			)?,
			ready: register(
				Histogram::with_opts(histogram_opts!(
					"substrate_sub_txpool_timing_event_ready",
					"Histogram of timings for reporting Ready event",
					linear_buckets(0.0, 0.25, 13).unwrap()
				))?,
				registry,
			)?,
			broadcast: register(
				Histogram::with_opts(histogram_opts!(
					"substrate_sub_txpool_timing_event_broadcast",
					"Histogram of timings for reporting Broadcast event",
					linear_buckets(0.0, 0.25, 13).unwrap()
				))?,
				registry,
			)?,
			in_block: register(
				Histogram::with_opts(histogram_opts!(
					"substrate_sub_txpool_timing_event_in_block",
					"Histogram of timings for reporting InBlock event",
					linear_buckets(0.0, 0.25, 13).unwrap()
				))?,
				registry,
			)?,
			retracted: register(
				Histogram::with_opts(histogram_opts!(
					"substrate_sub_txpool_timing_event_retracted",
					"Histogram of timings for reporting Retracted event",
					linear_buckets(0.0, 0.25, 13).unwrap()
				))?,
				registry,
			)?,
			finality_timeout: register(
				Histogram::with_opts(histogram_opts!(
					"substrate_sub_txpool_timing_event_finality_timeout",
					"Histogram of timings for reporting FinalityTimeout event",
					linear_buckets(0.0, 0.25, 13).unwrap()
				))?,
				registry,
			)?,
			finalized: register(
				Histogram::with_opts(histogram_opts!(
					"substrate_sub_txpool_timing_event_finalized",
					"Histogram of timings for reporting Finalized event",
					linear_buckets(0.0, 0.25, 13).unwrap()
				))?,
				registry,
			)?,
			usurped: register(
				Histogram::with_opts(histogram_opts!(
					"substrate_sub_txpool_timing_event_usurped",
					"Histogram of timings for reporting Usurped event",
					linear_buckets(0.0, 0.25, 13).unwrap()
				))?,
				registry,
			)?,
			dropped: register(
				Histogram::with_opts(histogram_opts!(
					"substrate_sub_txpool_timing_event_dropped",
					"Histogram of timings for reporting Dropped event",
					linear_buckets(0.0, 0.25, 13).unwrap()
				))?,
				registry,
			)?,
			invalid: register(
				Histogram::with_opts(histogram_opts!(
					"substrate_sub_txpool_timing_event_invalid",
					"Histogram of timings for reporting Invalid event",
					linear_buckets(0.0, 0.25, 13).unwrap()
				))?,
				registry,
			)?,
		})
	}

	fn observe<Hash, BlockHash>(
		&self,
		status: TransactionStatus<Hash, BlockHash>,
		duration: Duration,
	) {
		let duration = duration.as_secs_f64();
		match status {
			TransactionStatus::Future => self.future.observe(duration),
			TransactionStatus::Ready => self.ready.observe(duration),
			TransactionStatus::Broadcast(..) => self.broadcast.observe(duration),
			TransactionStatus::InBlock(..) => self.in_block.observe(duration),
			TransactionStatus::Retracted(..) => self.retracted.observe(duration),
			TransactionStatus::FinalityTimeout(..) => self.finality_timeout.observe(duration),
			TransactionStatus::Finalized(..) => self.finalized.observe(duration),
			TransactionStatus::Usurped(..) => self.usurped.observe(duration),
			TransactionStatus::Dropped => self.dropped.observe(duration),
			TransactionStatus::Invalid => self.invalid.observe(duration),
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
			removed_invalid_txs: register(
				Counter::new(
					"substrate_sub_txpool_removed_invalid_txs_total",
					"Total number of transactions reported as invalid.",
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
			mempool_revalidation_invalid_txs: register(
				Counter::new(
					"substrate_sub_txpool_mempool_revalidation_invalid_txs_total",
					"Total number of transactions found as invalid during mempool revalidation.",
				)?,
				registry,
			)?,
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
		}))
	}
}

enum EventMetricsMessage<Hash, BlockHash> {
	Submitted(Instant, Hash),
	Status(Instant, Hash, TransactionStatus<Hash, BlockHash>),
}

use crate::graph::{self, BlockHash, ExtrinsicHash};
use futures::StreamExt;

struct EventsMetricsCollector<ChainApi: graph::ChainApi> {
	metrics_message_sink: MessageSink<ExtrinsicHash<ChainApi>, BlockHash<ChainApi>>,
}

pub type EventsMetricsCollectorTask = Pin<Box<dyn Future<Output = ()> + Send>>;

type MessageSink<Hash, BlockHash> =
	mpsc::TracingUnboundedSender<EventMetricsMessage<Hash, BlockHash>>;
type MessageReceiver<Hash, BlockHash> =
	mpsc::TracingUnboundedReceiver<EventMetricsMessage<Hash, BlockHash>>;

impl<ChainApi> EventsMetricsCollector<ChainApi>
where
	ChainApi: graph::ChainApi + 'static,
{
	fn handle_status(
		hash: ExtrinsicHash<ChainApi>,
		status: TransactionStatus<ExtrinsicHash<ChainApi>, BlockHash<ChainApi>>,
		timestamp: Instant,
		submitted_timestamp_map: &HashMap<ExtrinsicHash<ChainApi>, Instant>,
		metrics: &MetricsLink,
	) {
		let Some(submitted_timestamp) = submitted_timestamp_map.get(&hash) else { return };
		metrics.report(|metrics| {
			metrics
				.events_histograms
				.observe(status, timestamp.duration_since(*submitted_timestamp))
		});
	}

	async fn task(
		mut rx: MessageReceiver<ExtrinsicHash<ChainApi>, BlockHash<ChainApi>>,
		metrics: MetricsLink,
	) {
		let mut submitted_timestamp = HashMap::<ExtrinsicHash<ChainApi>, Instant>::default();

		loop {
			tokio::select! {
				biased;
				cmd = rx.next() => {
					match cmd {
						Some(EventMetricsMessage::Submitted(timestamp, hash)) => {
							submitted_timestamp.insert(hash, timestamp);
						},
						Some(EventMetricsMessage::Status(timestamp,hash,status)) => {
							Self::handle_status(hash, status, timestamp, &submitted_timestamp, &metrics);
						},
						None => { return /* ? */ }
					}
				},
			};
		}
	}

	pub fn new_with_worker(metrics: MetricsLink) -> (Self, EventsMetricsCollectorTask) {
		const QUEUE_WARN_SIZE: usize = 100_000;
		let (metrics_message_sink, rx) =
			mpsc::tracing_unbounded("txpool-event-metrics-collector", QUEUE_WARN_SIZE);
		let task = Self::task(rx, metrics);

		(Self { metrics_message_sink }, task.boxed())
	}
}
