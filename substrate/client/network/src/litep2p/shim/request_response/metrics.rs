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

//! Metrics for [`RequestResponseProtocol`](super::RequestResponseProtocol).

use crate::{service::metrics::Metrics, types::ProtocolName};

use std::time::Duration;

/// Request-response metrics.
pub struct RequestResponseMetrics {
	/// Metrics.
	metrics: Option<Metrics>,

	/// Protocol name.
	protocol: ProtocolName,
}

impl RequestResponseMetrics {
	pub fn new(metrics: Option<Metrics>, protocol: ProtocolName) -> Self {
		Self { metrics, protocol }
	}

	/// Register inbound request failure to Prometheus
	pub fn register_inbound_request_failure(&self, reason: &str) {
		if let Some(metrics) = &self.metrics {
			metrics
				.requests_in_failure_total
				.with_label_values(&[&self.protocol, reason])
				.inc();
		}
	}

	/// Register inbound request success to Prometheus
	pub fn register_inbound_request_success(&self, serve_time: Duration) {
		if let Some(metrics) = &self.metrics {
			metrics
				.requests_in_success_total
				.with_label_values(&[&self.protocol])
				.observe(serve_time.as_secs_f64());
		}
	}

	/// Register inbound request failure to Prometheus
	pub fn register_outbound_request_failure(&self, reason: &str) {
		if let Some(metrics) = &self.metrics {
			metrics
				.requests_out_failure_total
				.with_label_values(&[&self.protocol, reason])
				.inc();
		}
	}

	/// Register inbound request success to Prometheus
	pub fn register_outbound_request_success(&self, duration: Duration) {
		if let Some(metrics) = &self.metrics {
			metrics
				.requests_out_success_total
				.with_label_values(&[&self.protocol])
				.observe(duration.as_secs_f64());
		}
	}
}
