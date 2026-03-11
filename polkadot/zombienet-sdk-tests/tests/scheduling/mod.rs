// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

mod v3_descriptor_disabled;
mod v3_descriptor_enabled;
mod v3_elastic_scaling;

use anyhow::anyhow;
use codec::Decode;
use cumulus_zombienet_sdk_helpers::wait_for_first_session_change;
use polkadot_primitives::{CandidateDescriptorVersion, CandidateReceiptV2, Id as ParaId};
use zombienet_sdk::{
	subxt::{utils::H256, OnlineClient, PolkadotConfig},
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

/// Find CandidateBacked events and decode them.
fn find_candidate_backed_events(
	events: &zombienet_sdk::subxt::events::Events<PolkadotConfig>,
) -> Result<Vec<CandidateReceiptV2<H256>>, anyhow::Error> {
	let mut result = vec![];
	for event in events.iter() {
		let event = event?;
		if event.pallet_name() == "ParaInclusion" && event.variant_name() == "CandidateBacked" {
			result.push(CandidateReceiptV2::<H256>::decode(&mut &event.field_bytes()[..])?);
		}
	}
	Ok(result)
}

/// Asserts that candidates of the expected version are being backed for a given parachain.
///
/// Waits for the first session change (so that genesis configuration like `node_features` is
/// active), then checks that at least `min_candidates` candidates matching `expected_version`
/// are backed within `max_blocks` relay chain blocks.
async fn assert_candidates_version(
	relay_client: &OnlineClient<PolkadotConfig>,
	para_id: ParaId,
	expected_version: CandidateDescriptorVersion,
	v3_enabled: bool,
	min_candidates: u32,
	max_blocks: u32,
) -> Result<(), anyhow::Error> {
	let mut blocks_sub = relay_client.blocks().subscribe_finalized().await?;

	wait_for_first_session_change(&mut blocks_sub).await?;

	let mut matched = 0u32;
	let mut total = 0u32;
	let mut block_count = 0u32;

	while let Some(block) = blocks_sub.next().await {
		let block = block?;
		log::debug!("Finalized relay chain block {}", block.number());

		for receipt in find_candidate_backed_events(&block.events().await?)? {
			if receipt.descriptor.para_id() != para_id {
				continue;
			}

			total += 1;
			let version = receipt.descriptor.version(v3_enabled);
			log::info!(
				"Para {} candidate backed: version={:?}, relay_parent={:?}",
				para_id,
				version,
				receipt.descriptor.relay_parent(),
			);

			if version == expected_version {
				matched += 1;
			}
		}

		block_count += 1;

		if matched >= min_candidates {
			log::info!(
				"Found {matched}/{total} {:?} candidates for para {para_id} in {block_count} blocks",
				expected_version,
			);
			return Ok(());
		}

		if block_count >= max_blocks {
			break;
		}
	}

	Err(anyhow!(
		"Only found {matched} {:?} candidates (needed {min_candidates}) out of {total} total for para {para_id} in {block_count} blocks",
		expected_version,
	))
}
