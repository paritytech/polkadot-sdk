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

//! Prometheus metrics for the litep2p bitswap server.

use litep2p::protocol::libp2p::bitswap::{BlockPresenceType, ResponseType};
use prometheus_endpoint::{
	exponential_buckets, register, Counter, CounterVec, Histogram, HistogramOpts, Opts,
	PrometheusError, Registry, U64,
};
use std::time::Duration;

/// `outcome` label values for `substrate_sub_libp2p_bitswap_entries_total`.
pub mod outcomes {
	pub const BLOCK_SERVED: &str = "block_served";
	pub const HAVE: &str = "have";
	pub const DONT_HAVE: &str = "dont_have";
	pub const UNSUPPORTED_CID: &str = "unsupported_cid";
}

/// `reason` label values for `substrate_sub_libp2p_bitswap_request_errors_total`.
pub mod errors {
	pub const TOO_MANY_ENTRIES: &str = "too_many_entries";
	pub const CLIENT: &str = "client";
}

struct Inner {
	entries_total: CounterVec<U64>,
	request_errors_total: CounterVec<U64>,
	inbound_request_duration_seconds: Histogram,
	response_bytes_total: Counter<U64>,
}

impl Inner {
	fn register(registry: &Registry) -> Result<Self, PrometheusError> {
		Ok(Self {
			entries_total: register(
				CounterVec::new(
					Opts::new(
						"substrate_sub_libp2p_bitswap_entries_total",
						"Total number of bitswap wantlist entries processed, by outcome",
					),
					&["outcome"],
				)?,
				registry,
			)?,
			request_errors_total: register(
				CounterVec::new(
					Opts::new(
						"substrate_sub_libp2p_bitswap_request_errors_total",
						"Total number of bitswap inbound requests rejected, by reason",
					),
					&["reason"],
				)?,
				registry,
			)?,
			inbound_request_duration_seconds: register(
				Histogram::with_opts(HistogramOpts {
					common_opts: Opts::new(
						"substrate_sub_libp2p_bitswap_inbound_request_duration_seconds",
						"Duration of handling an inbound bitswap wantlist, in seconds",
					),
					buckets: exponential_buckets(0.001, 2.0, 16)
						.expect("parameters are always valid values; qed"),
				})?,
				registry,
			)?,
			response_bytes_total: register(
				Counter::new(
					"substrate_sub_libp2p_bitswap_response_bytes_total",
					"Total bytes sent in bitswap responses to inbound wantlists",
				)?,
				registry,
			)?,
		})
	}
}

/// Helper wrapper around the bitswap server metrics.
///
/// When constructed without a `Registry`, all recording methods become no-ops.
pub struct BitswapMetrics {
	inner: Option<Inner>,
}

impl BitswapMetrics {
	/// Register the metrics with the given Prometheus registry, if any.
	pub fn new(registry: Option<&Registry>) -> Result<Self, PrometheusError> {
		Ok(Self { inner: registry.map(Inner::register).transpose()? })
	}

	/// Record one wantlist entry processed with the given outcome.
	pub fn record_entry(&self, outcome: &str) {
		if let Some(inner) = &self.inner {
			inner.entries_total.with_label_values(&[outcome]).inc();
		}
	}

	/// Record one outbound response variant under the matching outcome label.
	pub fn record_response(&self, response: &ResponseType) {
		let outcome = match response {
			ResponseType::Block { .. } => outcomes::BLOCK_SERVED,
			ResponseType::Presence { presence: BlockPresenceType::Have, .. } => outcomes::HAVE,
			ResponseType::Presence { presence: BlockPresenceType::DontHave, .. } => {
				outcomes::DONT_HAVE
			},
		};
		self.record_entry(outcome);
	}

	/// Record one request-level error with the given reason.
	pub fn record_error(&self, reason: &str) {
		if let Some(inner) = &self.inner {
			inner.request_errors_total.with_label_values(&[reason]).inc();
		}
	}

	/// Observe the duration of an inbound wantlist handling.
	pub fn record_duration(&self, duration: Duration) {
		if let Some(inner) = &self.inner {
			inner.inbound_request_duration_seconds.observe(duration.as_secs_f64());
		}
	}

	/// Add to the running total of bitswap response bytes sent.
	pub fn add_response_bytes(&self, bytes: u64) {
		if let Some(inner) = &self.inner {
			inner.response_bytes_total.inc_by(bytes);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use cid::{multihash::Multihash as CidMultihash, Cid};

	fn make_cid() -> Cid {
		let mh = CidMultihash::<64>::wrap(0xb220, &[0u8; 32]).unwrap();
		Cid::new_v1(0x55, mh)
	}

	#[test]
	fn disabled_metrics_are_no_ops() {
		let metrics = BitswapMetrics::new(None).unwrap();
		metrics.record_entry(outcomes::BLOCK_SERVED);
		metrics.record_error(errors::CLIENT);
		metrics.record_duration(Duration::from_millis(1));
		metrics.add_response_bytes(42);
	}

	#[test]
	fn record_response_maps_variants_to_outcomes() {
		let registry = Registry::new();
		let metrics = BitswapMetrics::new(Some(&registry)).unwrap();
		let cid = make_cid();

		metrics.record_response(&ResponseType::Block { cid, block: vec![1, 2, 3] });
		metrics.record_response(&ResponseType::Block { cid, block: vec![4] });
		metrics.record_response(&ResponseType::Presence { cid, presence: BlockPresenceType::Have });
		metrics.record_response(&ResponseType::Presence {
			cid,
			presence: BlockPresenceType::DontHave,
		});
		metrics.record_response(&ResponseType::Presence {
			cid,
			presence: BlockPresenceType::DontHave,
		});
		metrics.record_response(&ResponseType::Presence {
			cid,
			presence: BlockPresenceType::DontHave,
		});

		let inner = metrics.inner.as_ref().expect("inner should be present");
		assert_eq!(inner.entries_total.with_label_values(&[outcomes::BLOCK_SERVED]).get(), 2);
		assert_eq!(inner.entries_total.with_label_values(&[outcomes::HAVE]).get(), 1);
		assert_eq!(inner.entries_total.with_label_values(&[outcomes::DONT_HAVE]).get(), 3);
		assert_eq!(inner.entries_total.with_label_values(&[outcomes::UNSUPPORTED_CID]).get(), 0);
	}

	#[test]
	fn enabled_metrics_register_and_increment() {
		let registry = Registry::new();
		let metrics = BitswapMetrics::new(Some(&registry)).unwrap();

		metrics.record_entry(outcomes::BLOCK_SERVED);
		metrics.record_entry(outcomes::BLOCK_SERVED);
		metrics.record_entry(outcomes::HAVE);
		metrics.record_error(errors::TOO_MANY_ENTRIES);
		metrics.record_duration(Duration::from_millis(5));
		metrics.add_response_bytes(1024);

		let inner = metrics.inner.as_ref().expect("inner should be present when registry given");
		assert_eq!(inner.entries_total.with_label_values(&[outcomes::BLOCK_SERVED]).get(), 2);
		assert_eq!(inner.entries_total.with_label_values(&[outcomes::HAVE]).get(), 1);
		assert_eq!(inner.entries_total.with_label_values(&[outcomes::DONT_HAVE]).get(), 0);
		assert_eq!(
			inner.request_errors_total.with_label_values(&[errors::TOO_MANY_ENTRIES]).get(),
			1
		);
		assert_eq!(inner.response_bytes_total.get(), 1024);
		assert_eq!(inner.inbound_request_duration_seconds.get_sample_count(), 1);
	}
}
