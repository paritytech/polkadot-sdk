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

use crate::{service::metrics::NotificationMetrics, types::ProtocolName};

/// Register opened substream to Prometheus.
pub fn register_substream_opened(metrics: &Option<NotificationMetrics>, protocol: &ProtocolName) {
	if let Some(metrics) = metrics {
		metrics.register_substream_opened(&protocol);
	}
}

/// Register closed substream to Prometheus.
pub fn register_substream_closed(metrics: &Option<NotificationMetrics>, protocol: &ProtocolName) {
	if let Some(metrics) = metrics {
		metrics.register_substream_closed(&protocol);
	}
}

/// Register sent notification to Prometheus.
pub fn register_notification_sent(
	metrics: &Option<std::sync::Arc<NotificationMetrics>>,
	protocol: &ProtocolName,
	size: usize,
) {
	if let Some(metrics) = metrics {
		metrics.register_notification_sent(protocol, size);
	}
}

/// Register received notification to Prometheus.
pub fn register_notification_received(
	metrics: &Option<NotificationMetrics>,
	protocol: &ProtocolName,
	size: usize,
) {
	if let Some(metrics) = metrics {
		metrics.register_notification_received(protocol, size);
	}
}
