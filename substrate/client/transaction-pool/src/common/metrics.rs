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

//! Transaction pool Prometheus metrics for implementation of Chain API.

use prometheus_endpoint::{register, Counter, PrometheusError, Registry, U64};
use std::sync::Arc;

use crate::LOG_TARGET;

/// Provides interface to register the specific metrics in the Prometheus register.
pub(crate) trait MetricsRegistrant {
	/// Registers the metrics at given Prometheus registry.
	fn register(registry: &Registry) -> Result<Box<Self>, PrometheusError>;
}

/// Generic structure to keep a link to metrics register.
pub(crate) struct GenericMetricsLink<M: MetricsRegistrant>(Arc<Option<Box<M>>>);

impl<M: MetricsRegistrant> Default for GenericMetricsLink<M> {
	fn default() -> Self {
		Self(Arc::from(None))
	}
}

impl<M: MetricsRegistrant> Clone for GenericMetricsLink<M> {
	fn clone(&self) -> Self {
		Self(self.0.clone())
	}
}

impl<M: MetricsRegistrant> GenericMetricsLink<M> {
	pub fn new(registry: Option<&Registry>) -> Self {
		Self(Arc::new(registry.and_then(|registry| {
			M::register(registry)
				.map_err(|err| {
					log::warn!(target: LOG_TARGET, "Failed to register prometheus metrics: {}", err);
				})
				.ok()
		})))
	}

	pub fn report(&self, do_this: impl FnOnce(&M)) {
		if let Some(metrics) = self.0.as_ref() {
			do_this(&**metrics);
		}
	}
}

/// Transaction pool api Prometheus metrics.
pub struct ApiMetrics {
	pub validations_scheduled: Counter<U64>,
	pub validations_finished: Counter<U64>,
}

impl ApiMetrics {
	/// Register the metrics at the given Prometheus registry.
	pub fn register(registry: &Registry) -> Result<Self, PrometheusError> {
		Ok(Self {
			validations_scheduled: register(
				Counter::new(
					"substrate_sub_txpool_validations_scheduled",
					"Total number of transactions scheduled for validation",
				)?,
				registry,
			)?,
			validations_finished: register(
				Counter::new(
					"substrate_sub_txpool_validations_finished",
					"Total number of transactions that finished validation",
				)?,
				registry,
			)?,
		})
	}
}

/// An extension trait for [`ApiMetrics`].
pub trait ApiMetricsExt {
	/// Report an event to the metrics.
	fn report(&self, report: impl FnOnce(&ApiMetrics));
}

impl ApiMetricsExt for Option<Arc<ApiMetrics>> {
	fn report(&self, report: impl FnOnce(&ApiMetrics)) {
		if let Some(metrics) = self.as_ref() {
			report(metrics)
		}
	}
}
