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

//! Prometheus metrics for the HOP pool, RPC, and promotion task.

use crate::types::HopError;
use prometheus_endpoint::{
	exponential_buckets, register, Counter, CounterVec, Gauge, Histogram, HistogramOpts, Opts,
	PrometheusError, Registry, U64,
};
use std::time::Duration;

/// `reason` label values for `substrate_hop_pool_removed_total`.
pub mod removal_reasons {
	/// All recipients acknowledged; the entry served its purpose.
	pub const ACKED: &str = "acked";
	/// Expired after landing on-chain.
	pub const EXPIRED_PROMOTED: &str = "expired_promoted";
	/// Expired without ever landing on-chain; the data is lost.
	pub const EXPIRED_UNPROMOTED: &str = "expired_unpromoted";
	/// Blob failed its content-hash integrity check.
	pub const CORRUPT: &str = "corrupt";
}

/// `method` label values for the RPC metrics.
pub mod rpc_methods {
	pub const SUBMIT: &str = "submit";
	pub const CLAIM: &str = "claim";
	pub const ACK: &str = "ack";
	pub const POOL_STATUS: &str = "pool_status";
}

/// `outcome` label for successful operations.
pub const OUTCOME_OK: &str = "ok";

/// `outcome` label for the result of an operation returning [`HopError`].
pub fn outcome_label<T>(result: &Result<T, HopError>) -> &'static str {
	match result {
		Ok(_) => OUTCOME_OK,
		Err(e) => error_label(e),
	}
}

/// Stable low-cardinality label for a [`HopError`] variant.
pub fn error_label(err: &HopError) -> &'static str {
	match err {
		HopError::DataTooLarge(_, _) => "data_too_large",
		HopError::PoolFull(_, _) => "pool_full",
		HopError::DuplicateEntry => "duplicate_entry",
		HopError::NotFound => "not_found",
		HopError::EmptyData => "empty_data",
		HopError::InvalidSignature => "invalid_signature",
		HopError::NotRecipient => "not_recipient",
		HopError::NoRecipients => "no_recipients",
		HopError::InvalidRecipientKey => "invalid_recipient_key",
		HopError::UserQuotaExceeded { .. } => "user_quota_exceeded",
		HopError::NotAuthorized => "not_authorized",
		HopError::IoError(_) => "io_error",
		HopError::InvalidSigner => "invalid_signer",
		HopError::AlreadyClaimed => "already_claimed",
		HopError::InvalidHashLength(_) => "invalid_hash_length",
		HopError::RuntimeApiError(_) => "runtime_api_error",
		HopError::TooManyRecipients { .. } => "too_many_recipients",
		HopError::DuplicateRecipient => "duplicate_recipient",
		HopError::RateLimited { .. } => "rate_limited",
		HopError::MissingDataDir => "missing_data_dir",
	}
}

struct Inner {
	pool_entries: Gauge<U64>,
	pool_bytes: Gauge<U64>,
	pool_max_bytes: Gauge<U64>,
	pool_inserts_total: CounterVec<U64>,
	pool_inserted_bytes_total: Counter<U64>,
	pool_removed_total: CounterVec<U64>,
	rpc_requests_total: CounterVec<U64>,
	promotion_submissions_total: CounterVec<U64>,
	promotions_confirmed_total: Counter<U64>,
	promotions_abandoned_total: Counter<U64>,
	promotion_backlog: Gauge<U64>,
	promotion_enabled: Gauge<U64>,
	maintenance_tick_duration_seconds: Histogram,
}

impl Inner {
	fn register(registry: &Registry) -> Result<Self, PrometheusError> {
		Ok(Self {
			pool_entries: register(
				Gauge::new("substrate_hop_pool_entries", "Number of entries in the HOP pool")?,
				registry,
			)?,
			pool_bytes: register(
				Gauge::new(
					"substrate_hop_pool_bytes",
					"Accounted size of the HOP pool in bytes (data plus per-recipient overhead)",
				)?,
				registry,
			)?,
			pool_max_bytes: register(
				Gauge::new(
					"substrate_hop_pool_max_bytes",
					"Configured maximum HOP pool size in bytes",
				)?,
				registry,
			)?,
			pool_inserts_total: register(
				CounterVec::new(
					Opts::new(
						"substrate_hop_pool_inserts_total",
						"Total number of HOP pool insert attempts, by outcome",
					),
					&["outcome"],
				)?,
				registry,
			)?,
			pool_inserted_bytes_total: register(
				Counter::new(
					"substrate_hop_pool_inserted_bytes_total",
					"Total accounted bytes successfully inserted into the HOP pool",
				)?,
				registry,
			)?,
			pool_removed_total: register(
				CounterVec::new(
					Opts::new(
						"substrate_hop_pool_removed_total",
						"Total number of entries removed from the HOP pool, by reason",
					),
					&["reason"],
				)?,
				registry,
			)?,
			// Method-level call counts and durations are already covered for
			// every RPC method by the server middleware
			// (`substrate_rpc_calls_started/finished/time`); this counter only
			// adds the error-variant granularity the middleware cannot see.
			rpc_requests_total: register(
				CounterVec::new(
					Opts::new(
						"substrate_hop_rpc_requests_total",
						"Total number of HOP RPC requests, by method and outcome",
					),
					&["method", "outcome"],
				)?,
				registry,
			)?,
			promotion_submissions_total: register(
				CounterVec::new(
					Opts::new(
						"substrate_hop_promotion_submissions_total",
						"Total number of HOP promotion extrinsic submissions, by outcome",
					),
					&["outcome"],
				)?,
				registry,
			)?,
			promotions_confirmed_total: register(
				Counter::new(
					"substrate_hop_promotions_confirmed_total",
					"Total number of HOP entries confirmed as promoted on-chain",
				)?,
				registry,
			)?,
			promotions_abandoned_total: register(
				Counter::new(
					"substrate_hop_promotions_abandoned_total",
					"Total number of HOP entries that exhausted all promotion attempts",
				)?,
				registry,
			)?,
			promotion_backlog: register(
				Gauge::new(
					"substrate_hop_promotion_backlog",
					"Number of unpromoted HOP entries inside the promotion window",
				)?,
				registry,
			)?,
			promotion_enabled: register(
				Gauge::new(
					"substrate_hop_promotion_enabled",
					"Whether on-chain promotion is enabled on this node (0/1)",
				)?,
				registry,
			)?,
			maintenance_tick_duration_seconds: register(
				Histogram::with_opts(HistogramOpts {
					common_opts: Opts::new(
						"substrate_hop_maintenance_tick_duration_seconds",
						"Duration of a HOP maintenance cycle (promotion + cleanup), in seconds",
					),
					buckets: exponential_buckets(0.005, 2.0, 16)
						.expect("parameters are always valid values; qed"),
				})?,
				registry,
			)?,
		})
	}
}

/// Helper wrapper around the HOP metrics.
///
/// When constructed without a `Registry`, all recording methods become no-ops.
pub struct HopMetrics {
	inner: Option<Inner>,
}

impl HopMetrics {
	/// Register the metrics with the given Prometheus registry, if any.
	pub fn new(registry: Option<&Registry>) -> Result<Self, PrometheusError> {
		Ok(Self { inner: registry.map(Inner::register).transpose()? })
	}

	/// Create no-op metrics.
	pub fn disabled() -> Self {
		Self { inner: None }
	}

	/// Set the pool gauges to absolute values (used after disk recovery).
	pub(crate) fn set_pool_status(&self, entries: u64, bytes: u64, max_bytes: u64) {
		if let Some(inner) = &self.inner {
			inner.pool_entries.set(entries);
			inner.pool_bytes.set(bytes);
			inner.pool_max_bytes.set(max_bytes);
		}
	}

	/// Snapshot the pool size gauges. Gauges are set from the authoritative
	/// counts rather than inc/dec'd: a `Gauge<U64>` wraps on underflow, so any
	/// missed or double-counted delta would poison the gauge forever, while a
	/// snapshot self-corrects on the next update.
	pub(crate) fn set_pool_size(&self, entries: u64, bytes: u64) {
		if let Some(inner) = &self.inner {
			inner.pool_entries.set(entries);
			inner.pool_bytes.set(bytes);
		}
	}

	/// Record an insert attempt; on success also count `accounted` inserted bytes.
	pub(crate) fn record_insert<T>(&self, result: &Result<T, HopError>, accounted: u64) {
		if let Some(inner) = &self.inner {
			inner.pool_inserts_total.with_label_values(&[outcome_label(result)]).inc();
			if result.is_ok() {
				inner.pool_inserted_bytes_total.inc_by(accounted);
			}
		}
	}

	/// Record `entries` removals under `reason`.
	pub(crate) fn record_removed(&self, reason: &str, entries: u64) {
		if entries == 0 {
			return;
		}
		if let Some(inner) = &self.inner {
			inner.pool_removed_total.with_label_values(&[reason]).inc_by(entries);
		}
	}

	/// Record one RPC request with its outcome.
	pub(crate) fn record_rpc(&self, method: &str, outcome: &str) {
		if let Some(inner) = &self.inner {
			inner.rpc_requests_total.with_label_values(&[method, outcome]).inc();
		}
	}

	/// Record one promotion extrinsic submission. `submitted` means accepted by
	/// the local transaction pool; inclusion is confirmed separately.
	pub(crate) fn record_promotion_submission(&self, submitted: bool) {
		if let Some(inner) = &self.inner {
			let outcome = if submitted { "submitted" } else { "failed" };
			inner.promotion_submissions_total.with_label_values(&[outcome]).inc();
		}
	}

	/// Record one entry confirmed as promoted on-chain.
	pub(crate) fn record_promotion_confirmed(&self) {
		if let Some(inner) = &self.inner {
			inner.promotions_confirmed_total.inc();
		}
	}

	/// Record one entry that reached the promotion attempt cap and will expire
	/// without further attempts.
	pub(crate) fn record_promotion_abandoned(&self) {
		if let Some(inner) = &self.inner {
			inner.promotions_abandoned_total.inc();
		}
	}

	/// Set the promotion backlog gauge.
	pub(crate) fn set_promotion_backlog(&self, backlog: u64) {
		if let Some(inner) = &self.inner {
			inner.promotion_backlog.set(backlog);
		}
	}

	/// Set whether on-chain promotion is enabled.
	pub(crate) fn set_promotion_enabled(&self, enabled: bool) {
		if let Some(inner) = &self.inner {
			inner.promotion_enabled.set(enabled as u64);
		}
	}

	/// Observe the duration of one maintenance cycle.
	pub(crate) fn observe_tick_duration(&self, elapsed: Duration) {
		if let Some(inner) = &self.inner {
			inner.maintenance_tick_duration_seconds.observe(elapsed.as_secs_f64());
		}
	}

	/// Counter value of `substrate_hop_pool_removed_total{reason}`.
	#[cfg(test)]
	pub(crate) fn removed_count(&self, reason: &str) -> u64 {
		self.inner
			.as_ref()
			.map(|i| i.pool_removed_total.with_label_values(&[reason]).get())
			.unwrap_or(0)
	}

	/// Current value of the pool entries / bytes gauges.
	#[cfg(test)]
	pub(crate) fn pool_gauges(&self) -> (u64, u64) {
		self.inner
			.as_ref()
			.map(|i| (i.pool_entries.get(), i.pool_bytes.get()))
			.unwrap_or((0, 0))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn disabled_metrics_are_no_ops() {
		let metrics = HopMetrics::disabled();
		metrics.set_pool_status(1, 2, 3);
		metrics.set_pool_size(1, 2);
		metrics.record_insert::<()>(&Ok(()), 42);
		metrics.record_removed(removal_reasons::ACKED, 1);
		metrics.record_rpc(rpc_methods::SUBMIT, OUTCOME_OK);
		metrics.record_promotion_submission(true);
		metrics.record_promotion_confirmed();
		metrics.record_promotion_abandoned();
		metrics.set_promotion_backlog(7);
		metrics.set_promotion_enabled(true);
		metrics.observe_tick_duration(Duration::from_millis(5));
	}

	#[test]
	fn enabled_metrics_register_and_track_pool_size() {
		let registry = Registry::new();
		let metrics = HopMetrics::new(Some(&registry)).unwrap();

		metrics.set_pool_status(2, 200, 1000);
		assert_eq!(metrics.pool_gauges(), (2, 200));

		metrics.record_insert::<()>(&Ok(()), 100);
		metrics.record_insert::<()>(&Err(HopError::PoolFull(300, 1000)), 100);
		metrics.set_pool_size(3, 300);
		assert_eq!(metrics.pool_gauges(), (3, 300));

		metrics.record_removed(removal_reasons::EXPIRED_UNPROMOTED, 2);
		metrics.set_pool_size(1, 50);
		assert_eq!(metrics.pool_gauges(), (1, 50));
		assert_eq!(metrics.removed_count(removal_reasons::EXPIRED_UNPROMOTED), 2);
		assert_eq!(metrics.removed_count(removal_reasons::ACKED), 0);

		let inner = metrics.inner.as_ref().unwrap();
		assert_eq!(inner.pool_inserts_total.with_label_values(&[OUTCOME_OK]).get(), 1);
		assert_eq!(inner.pool_inserts_total.with_label_values(&["pool_full"]).get(), 1);
		assert_eq!(inner.pool_inserted_bytes_total.get(), 100);
	}

	#[test]
	fn rpc_and_promotion_metrics_increment() {
		let registry = Registry::new();
		let metrics = HopMetrics::new(Some(&registry)).unwrap();

		metrics.record_rpc(rpc_methods::SUBMIT, OUTCOME_OK);
		metrics.record_rpc(rpc_methods::CLAIM, error_label(&HopError::NotFound));
		metrics.record_promotion_submission(true);
		metrics.record_promotion_submission(false);
		metrics.record_promotion_confirmed();
		metrics.record_promotion_abandoned();
		metrics.set_promotion_backlog(4);
		metrics.set_promotion_enabled(true);
		metrics.observe_tick_duration(Duration::from_millis(10));

		let inner = metrics.inner.as_ref().unwrap();
		assert_eq!(
			inner
				.rpc_requests_total
				.with_label_values(&[rpc_methods::SUBMIT, OUTCOME_OK])
				.get(),
			1
		);
		assert_eq!(
			inner
				.rpc_requests_total
				.with_label_values(&[rpc_methods::CLAIM, "not_found"])
				.get(),
			1
		);
		assert_eq!(inner.promotion_submissions_total.with_label_values(&["submitted"]).get(), 1);
		assert_eq!(inner.promotion_submissions_total.with_label_values(&["failed"]).get(), 1);
		assert_eq!(inner.promotions_confirmed_total.get(), 1);
		assert_eq!(inner.promotions_abandoned_total.get(), 1);
		assert_eq!(inner.promotion_backlog.get(), 4);
		assert_eq!(inner.promotion_enabled.get(), 1);
		assert_eq!(inner.maintenance_tick_duration_seconds.get_sample_count(), 1);
	}
}
