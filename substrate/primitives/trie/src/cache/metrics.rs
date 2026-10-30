// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Metrics for the trie cache.

use prometheus_endpoint::{
	exponential_buckets,
	prometheus::{core::Collector, HistogramTimer},
	CounterVec, GaugeVec, HistogramOpts, HistogramVec, Opts, PrometheusError, Registry, U64,
};

// Register a metric with the given registry.
fn register<T: Clone + Collector + 'static>(
	metric: T,
	registry: &Registry,
) -> Result<T, PrometheusError> {
	registry.register(Box::new(metric.clone()))?;
	Ok(metric)
}

/// Metrics for the trie cache.
/// This struct is used to track the performance of the trie cache.
/// It contains histograms and counters for the shared and local caches.
#[derive(Clone)]
pub struct Metrics {
	// The duration in seconds to update the shared trie caches from local to shared cache.
	shared_update_duration: HistogramVec,
	// Number of attempts hitting the shared trie caches.
	shared_hits: CounterVec<U64>,
	// Number of attempts to the shared trie caches.
	shared_fetch_attempts: CounterVec<U64>,
	// Number of attempts hitting the local trie caches.
	local_hits: CounterVec<U64>,
	// Number of attempts to the local caches.
	local_fetch_attempts: CounterVec<U64>,
	// Length of the local caches.
	local_cache_lengths: HistogramVec,
	// The inline size of the shared caches.
	shared_cache_inline_size: GaugeVec<U64>,
	// The heap size of the shared caches.
	shared_cache_heap_size: GaugeVec<U64>,
}

impl Metrics {
	/// Create a new instance of the metrics.
	pub(crate) fn register(registry: &Registry) -> Result<Self, PrometheusError> {
		Ok(Self {
			shared_update_duration: register(
				HistogramVec::new(
					HistogramOpts {
						common_opts: Opts::new(
							"trie_cache_shared_update_duration",
							"Duration in seconds to update the shared trie caches from local cache to shared cache",
						),
						buckets: exponential_buckets(0.001, 4.0, 9)
							.expect("function parameters are constant and always valid; qed"),
					},
					&["cache_type"], // node or value
				)?,
				registry,
			)?,
			shared_hits: register(
				CounterVec::new(
					Opts::new(
						"trie_cache_shared_hits",
						"Number of attempts hitting the shared trie cache",
					),
					&["cache_type"], // node or value
				)?,
				registry,
			)?,
			shared_fetch_attempts: register(
				CounterVec::new(
					Opts::new(
						"trie_cache_shared_fetch_attempts",
						"Number of attempts to the shared trie cache",
					),
					&["cache_type"],
				)?,
				registry,
			)?,
			local_hits: register(
				CounterVec::new(
					Opts::new(
						"trie_cache_local_hits",
						"Number of attempts hitting the local trie cache",
					),
					&["cache_type"],
				)?,
				registry,
			)?,
			local_fetch_attempts: register(
				CounterVec::new(
					Opts::new(
						"trie_cache_local_fetch_attempts",
						"Number of attempts to the local cache",
					),
					&["cache_type"],
				)?,
				registry,
			)?,
			local_cache_lengths: register(
				HistogramVec::new(
					HistogramOpts {
						common_opts: Opts::new(
							"trie_cache_local_cache_lengths",
							"Histogram of length of the local cache",
						),
						buckets: exponential_buckets(1.0, 4.0, 9)
							.expect("function parameters are constant and always valid; qed"),
					},
					&["cache_type"],
				)?,
				registry,
			)?,
			shared_cache_inline_size: register(
				GaugeVec::new(
					Opts::new(
						"trie_cache_shared_cache_inline_size",
						"The inline size of the shared caches",
					),
					&["cache_type"],
				)?,
				registry,
			)?,
			shared_cache_heap_size: register(
				GaugeVec::new(
					Opts::new(
						"trie_cache_shared_cache_heap_size",
						"The heap size of the shared caches",
					),
					&["cache_type"],
				)?,
				registry,
			)?,
		})
	}

	/// Start a timer for the shared node cache update duration.
	pub(crate) fn start_shared_node_update_timer(&self) -> HistogramTimer {
		self.shared_update_duration.with_label_values(&["node"]).start_timer()
	}

	/// Start a timer for the shared value cache update duration.
	pub(crate) fn start_shared_value_update_timer(&self) -> HistogramTimer {
		self.shared_update_duration.with_label_values(&["value"]).start_timer()
	}

	/// Observe the shared node cache length.
	pub(crate) fn observe_local_node_cache_length(&self, node_cache_len: usize) {
		self.local_cache_lengths
			.with_label_values(&["node"])
			.observe(node_cache_len as f64);
	}

	/// Observe the shared value cache length.
	pub(crate) fn observe_local_value_cache_length(&self, value_cache_len: usize) {
		self.local_cache_lengths
			.with_label_values(&["value"])
			.observe(value_cache_len as f64);
	}

	/// Observe the shared node cache inline size.
	pub(crate) fn observe_node_cache_inline_size(&self, cache_size: usize) {
		self.shared_cache_inline_size
			.with_label_values(&["node"])
			.set(cache_size as u64);
	}

	/// Observe the shared value cache inline size.
	pub(crate) fn observe_value_cache_inline_size(&self, cache_size: usize) {
		self.shared_cache_inline_size
			.with_label_values(&["value"])
			.set(cache_size as u64);
	}

	/// Observe the shared node cache heap size.
	pub(crate) fn observe_node_cache_heap_size(&self, cache_size: usize) {
		self.shared_cache_heap_size.with_label_values(&["node"]).set(cache_size as u64);
	}

	/// Observe the shared value cache heap size.
	pub(crate) fn observe_value_cache_heap_size(&self, cache_size: usize) {
		self.shared_cache_heap_size.with_label_values(&["value"]).set(cache_size as u64);
	}

	/// Observe the hit stats from an instance of a local cache.
	pub(crate) fn observe_hits_stats(&self, stats: &TrieHitStatsSnapshot) {
		self.shared_hits
			.with_label_values(&["node"])
			.inc_by(stats.node_cache.shared_hits);
		self.shared_fetch_attempts
			.with_label_values(&["node"])
			.inc_by(stats.node_cache.shared_fetch_attempts);
		self.local_hits.with_label_values(&["node"]).inc_by(stats.node_cache.local_hits);
		self.local_fetch_attempts
			.with_label_values(&["node"])
			.inc_by(stats.node_cache.local_fetch_attempts);

		self.shared_hits
			.with_label_values(&["value"])
			.inc_by(stats.value_cache.shared_hits);
		self.shared_fetch_attempts
			.with_label_values(&["value"])
			.inc_by(stats.value_cache.shared_fetch_attempts);
		self.local_hits
			.with_label_values(&["value"])
			.inc_by(stats.value_cache.local_hits);
		self.local_fetch_attempts
			.with_label_values(&["value"])
			.inc_by(stats.value_cache.local_fetch_attempts);
	}
}

/// A snapshot of the hit/miss stats.
#[derive(Default, Copy, Clone, Debug)]
pub(crate) struct HitStatsSnapshot {
	pub(crate) shared_hits: u64,
	pub(crate) shared_fetch_attempts: u64,
	pub(crate) local_hits: u64,
	pub(crate) local_fetch_attempts: u64,
}

impl std::fmt::Display for HitStatsSnapshot {
	fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
		let shared_hits = self.shared_hits;
		let shared_fetch_attempts = self.shared_fetch_attempts;
		let local_hits = self.local_hits;
		let local_fetch_attempts = self.local_fetch_attempts;

		if shared_fetch_attempts == 0 && local_hits == 0 {
			write!(fmt, "empty")
		} else {
			let percent_local = (local_hits as f32 / local_fetch_attempts as f32) * 100.0;
			let percent_shared = (shared_hits as f32 / shared_fetch_attempts as f32) * 100.0;
			write!(
				fmt,
				"local hit rate = {}% [{}/{}], shared hit rate = {}% [{}/{}]",
				percent_local as u32,
				local_hits,
				local_fetch_attempts,
				percent_shared as u32,
				shared_hits,
				shared_fetch_attempts
			)
		}
	}
}

/// Snapshot of the hit/miss stats for the node cache and the value cache.
#[derive(Default, Debug, Clone, Copy)]
pub(crate) struct TrieHitStatsSnapshot {
	pub(crate) node_cache: HitStatsSnapshot,
	pub(crate) value_cache: HitStatsSnapshot,
}
