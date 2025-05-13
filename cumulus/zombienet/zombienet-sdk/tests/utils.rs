// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use zombienet_sdk::NetworkNode;

pub const BEST_BLOCK_METRIC: &str = "block_height{status=\"best\"}";

pub async fn wait_node_is_up(
	node: &NetworkNode,
	timeout_secs: impl Into<u64>,
) -> Result<(), anyhow::Error> {
	node.wait_metric_with_timeout("process_start_time_seconds", |b| b >= 1.0, timeout_secs)
		.await
}
