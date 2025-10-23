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

//! Transaction metrics for various transaction states.
//!
//! The histograms are shared between the transaction pool and the RPC layer.
//!
//! # Note
//!
//! The RPC layer will utilize a subset of these metrics for now, since the
//! RPC emitted events do not have a direct mapping to all transaction pool events.
//!
//! Changing these metrics will impact the RPC layer as well.

use prometheus_endpoint::{
	exponential_buckets, histogram_opts, linear_buckets, Histogram, PrometheusError,
};

/// Histogram of timings for reporting `Ready`/`Future` events.
pub fn ready_future(name: &'static str, label: &'static str) -> Result<Histogram, PrometheusError> {
	Histogram::with_opts(histogram_opts!(name, label, exponential_buckets(0.01, 2.0, 16).unwrap()))
}

/// Histogram of timings for reporting `Broadcast` event.
pub fn broadcast(name: &'static str, label: &'static str) -> Result<Histogram, PrometheusError> {
	Histogram::with_opts(histogram_opts!(name, label, linear_buckets(0.01, 0.25, 16).unwrap()))
}

/// Histogram of timings for reporting `InBlock` event.
pub fn in_block(name: &'static str, label: &'static str) -> Result<Histogram, PrometheusError> {
	Histogram::with_opts(
		histogram_opts!(name, label).buckets(
			[
				linear_buckets(0.0, 3.0, 20).unwrap(),
				// requested in #9158
				vec![60.0, 75.0, 90.0, 120.0, 180.0],
			]
			.concat(),
		),
	)
}

/// Histogram of timings for reporting `Retracted` event.
pub fn retracted(name: &'static str, label: &'static str) -> Result<Histogram, PrometheusError> {
	Histogram::with_opts(histogram_opts!(name, label, linear_buckets(0.0, 3.0, 20).unwrap()))
}

/// Histogram of timings for reporting `FinalityTimeout` event.
pub fn finalized_timeout(
	name: &'static str,
	label: &'static str,
) -> Result<Histogram, PrometheusError> {
	Histogram::with_opts(histogram_opts!(name, label, linear_buckets(0.0, 40.0, 20).unwrap()))
}

/// Histogram of timings for reporting `Finalized` event.
pub fn finalized(name: &'static str, label: &'static str) -> Result<Histogram, PrometheusError> {
	Histogram::with_opts(
		histogram_opts!(name, label).buckets(
			[
				// requested in #9158
				linear_buckets(0.0, 5.0, 8).unwrap(),
				linear_buckets(40.0, 40.0, 19).unwrap(),
			]
			.concat(),
		),
	)
}

/// Histogram of timings for reporting `Invalid` / `Dropped` / `Usurped` events.
pub fn invalid(name: &'static str, label: &'static str) -> Result<Histogram, PrometheusError> {
	Histogram::with_opts(
		histogram_opts!(name, label).buckets(
			[
				linear_buckets(0.0, 3.0, 20).unwrap(),
				// requested in PR 9158
				vec![60.0, 75.0, 90.0, 120.0, 180.0],
			]
			.concat(),
		),
	)
}
