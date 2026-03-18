// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

mod v3_dynamic_enablement;
mod v3_rolling_upgrade;

use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::assert_para_throughput_with;
use polkadot_primitives::{CandidateDescriptorVersion, Id as ParaId};
use std::{collections::HashMap, ops::Range};
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkNode,
};

/// Metric name for the total number of backing statements signed by a validator.
const SIGNED_STATEMENTS_METRIC: &str =
	"polkadot_parachain_candidate_backing_signed_statements_total";

/// Asserts that a validator node has signed at least one backing statement.
pub async fn assert_validator_backed_candidates(
	node: &NetworkNode,
	timeout_secs: u64,
) -> Result<(), anyhow::Error> {
	node.wait_metric_with_timeout(SIGNED_STATEMENTS_METRIC, |v| v >= 1.0, timeout_secs)
		.await
		.map_err(|e| {
			anyhow!(
				"Validator {} did not sign any backing statements within {timeout_secs}s: {e}",
				node.name()
			)
		})
}

/// Asserts that candidates of the expected version are being backed for the given parachains.
///
/// Waits for the first session change (so that genesis configuration like `node_features` is
/// active), then checks that the number of candidates matching `expected_version` falls within
/// `expected_range` after `max_blocks` relay chain blocks for each para ID.
pub async fn assert_candidates_version(
	relay_client: &OnlineClient<PolkadotConfig>,
	para_ids: &[ParaId],
	expected_version: CandidateDescriptorVersion,
	v3_enabled: bool,
	expected_range: Range<u32>,
	max_blocks: u32,
) -> Result<(), anyhow::Error> {
	let expected_ranges: HashMap<ParaId, _> =
		para_ids.iter().map(|&id| (id, expected_range.clone())).collect();

	assert_para_throughput_with(relay_client, max_blocks, expected_ranges, |receipt| {
		let para_id = receipt.descriptor.para_id();
		let version = receipt.descriptor.version(v3_enabled);
		log::info!(
			"Para {} candidate backed: version={:?}, \
			 relay_parent={:?}, \
			 session_index={:?}, \
			 scheduling_parent={:?}",
			para_id,
			version,
			receipt.descriptor.relay_parent(),
			receipt.descriptor.session_index(v3_enabled),
			receipt.descriptor.scheduling_parent(v3_enabled),
		);

		if version != expected_version {
			return Err(anyhow!(
				"Para {para_id} candidate has version {:?}, expected {:?}",
				version,
				expected_version,
			));
		}

		if expected_version == CandidateDescriptorVersion::V2 {
			if receipt.descriptor.session_index(v3_enabled).is_none() {
				return Err(anyhow!("Para {para_id} V2 candidate has session_index=None",));
			}
			if receipt.descriptor.relay_parent() != receipt.descriptor.scheduling_parent(v3_enabled)
			{
				return Err(anyhow!(
					"Para {para_id} V2 candidate has scheduling_parent={:?} \
					 != relay_parent={:?}",
					receipt.descriptor.scheduling_parent(v3_enabled),
					receipt.descriptor.relay_parent(),
				));
			}
		}

		Ok(true)
	})
	.await
}
