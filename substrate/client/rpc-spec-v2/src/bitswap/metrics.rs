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

//! Prometheus metrics for the bitswap RPC namespace.
//!
//! Three counters, deliberately narrow scope:
//! - `rpc_bitswap_calls_total{method="get"|"stream"}` — RPC entry-point invocations.
//! - `rpc_bitswap_items_delivered_total` — chunks successfully delivered to clients.
//! - `rpc_bitswap_items_not_found_total` — chunks missing from the local indexed-transaction DB.
//!
//! Names follow Prometheus / OpenMetrics conventions: counters carry an explicit `_total` suffix
//! (the `prometheus` Rust crate does not auto-append it).
//!
//! [`Metrics`] is a thin wrapper around an `Option<Inner>`: when constructed via [`Metrics::new`]
//! it registers all three series on the given registry; when constructed via
//! [`Metrics::disabled`] (or `Default`) every recording method is a no-op. This keeps call sites
//! free of `if let Some(metrics) = ...` ceremony — mirrors the `InstanceMetrics` pattern in
//! `transaction/metrics.rs`.

use prometheus_endpoint::{register, Counter, CounterVec, Opts, PrometheusError, Registry, U64};

/// Entry-point method for the `method` label on the `rpc_bitswap_calls_total` counter.
#[derive(Debug, Clone, Copy)]
pub enum Method {
	/// `bitswap_unstable_get`.
	Get,
	/// `bitswap_unstable_stream`.
	Stream,
}

impl Method {
	fn as_str(self) -> &'static str {
		match self {
			Method::Get => "get",
			Method::Stream => "stream",
		}
	}
}

/// RPC layer metrics for the bitswap namespace.
///
/// Constructed via [`Metrics::new`] when a Prometheus registry is available, or
/// [`Metrics::disabled`] otherwise. All recording methods are safe to call in either state.
#[derive(Debug, Clone, Default)]
pub struct Metrics(Option<Inner>);

#[derive(Debug, Clone)]
struct Inner {
	/// Number of RPC invocations, labeled by entry method.
	calls: CounterVec<U64>,
	/// Number of chunks successfully delivered to clients.
	items_delivered: Counter<U64>,
	/// Number of chunks not found in the local indexed-transaction DB.
	items_not_found: Counter<U64>,
}

impl Metrics {
	/// A `Metrics` handle that records nothing. Cheap to clone and pass around.
	pub fn disabled() -> Self {
		Self(None)
	}

	/// Register the bitswap metrics on the given Prometheus registry.
	pub fn new(registry: &Registry) -> Result<Self, PrometheusError> {
		let calls = register(
			CounterVec::new(
				Opts::new(
					"rpc_bitswap_calls_total",
					"Total number of bitswap RPC method invocations",
				),
				&["method"],
			)?,
			registry,
		)?;

		let items_delivered = register(
			Counter::new(
				"rpc_bitswap_items_delivered_total",
				"Total number of chunks successfully delivered by the bitswap RPC namespace",
			)?,
			registry,
		)?;

		let items_not_found = register(
			Counter::new(
				"rpc_bitswap_items_not_found_total",
				"Total number of chunks not found in the local indexed-transaction DB",
			)?,
			registry,
		)?;

		Ok(Self(Some(Inner { calls, items_delivered, items_not_found })))
	}

	/// Record an RPC invocation for the given entry method. No-op when disabled.
	pub fn on_call(&self, method: Method) {
		let Some(inner) = &self.0 else { return };
		inner.calls.with_label_values(&[method.as_str()]).inc();
	}

	/// Record a successful chunk delivery to the client. No-op when disabled.
	pub fn on_delivered(&self) {
		let Some(inner) = &self.0 else { return };
		inner.items_delivered.inc();
	}

	/// Record a chunk missing from the local indexed-transaction DB. No-op when disabled.
	pub fn on_not_found(&self) {
		let Some(inner) = &self.0 else { return };
		inner.items_not_found.inc();
	}
}
