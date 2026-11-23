// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! The JAM counterpart of counting `ParaInclusion::CandidateBacked` events on a relay chain.
//!
//! JAM emits no events. A work report the guarantees of a JAM block accepted is placed in the
//! availability assignments (state key C(10)) of that block's posterior state, registered at the
//! block's slot, and stays there until its data is available. One work report of the Parachain
//! Service is one candidate; its refine output names the para, the head it produced and the
//! parent head it was built on.

use crate::jam::JamConfig;
use anyhow::anyhow;
use codec::{Decode, DecodeAll};
use cumulus_jam_interface::{
	HeaderHash, JamChainSource, JamStateSource, ServiceId, Slot, WorkReport,
};
use cumulus_jam_rpc_interface::JamRpcInterface;
use futures::StreamExt;
use parachain_service_core::PARACHAIN_SERVICE_ID;
use polkadot_primitives::Id as ParaId;
use sp_crypto_hashing::blake2_256;
use std::{
	collections::{HashMap, HashSet},
	ops::Range,
};
use zombienet_configuration::types::JamNodeMode;
use zombienet_sdk::{
	subxt::{utils::H256, OnlineClient},
	LocalFileSystem, Network,
};

/// How many para blocks one candidate may carry before the walk to its parent is given up.
const MAX_BLOCKS_PER_CANDIDATE: u32 = 256;

/// A candidate a JAM block accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct JamCandidate {
	pub para_id: ParaId,
	/// The hash of the last para block the candidate carries.
	pub head: H256,
	/// `blake2_256` of the hash of the para block the candidate builds on.
	pub parent_head_hash: [u8; 32],
}

/// The candidates of `service` accepted in the JAM block authored at `slot`.
///
/// `assignments` are that block's availability assignments, each with the slot it was registered
/// at. One registered earlier is a report still waiting for availability, counted in the block
/// that accepted it.
pub(crate) fn accepted_candidates<'a>(
	assignments: impl IntoIterator<Item = (Slot, &'a WorkReport)>,
	slot: Slot,
	service: ServiceId,
) -> Result<Vec<JamCandidate>, anyhow::Error> {
	let mut candidates = Vec::new();
	for (_, report) in assignments.into_iter().filter(|(registered, _)| *registered == slot) {
		for digest in report.results.iter().filter(|digest| digest.service == service) {
			// A refine that failed did not produce a candidate.
			let Ok(output) = &digest.result else { continue };
			if let Some(candidate) = decode_candidate(&output.0)? {
				candidates.push(candidate);
			}
		}
	}
	Ok(candidates)
}

/// Decodes the Parachain Service's refine output, `ParachainWorkDigest`.
///
/// `Ok { para_id, validation_code, parent_head_hash, head_data, .. }` is a candidate whose head
/// data is the last block's hash; `Err { .. }` is a candidate the PVF rejected.
fn decode_candidate(output: &[u8]) -> Result<Option<JamCandidate>, anyhow::Error> {
	let mut input = output;
	let decoded = (|| {
		if u8::decode(&mut input)? != 0 {
			return Ok(None);
		}
		let para_id = u32::decode(&mut input)?;
		let _validation_code = <[u8; 32]>::decode(&mut input)?;
		let parent_head_hash = <[u8; 32]>::decode(&mut input)?;
		let head = H256::decode_all(&mut &Vec::<u8>::decode(&mut input)?[..])?;
		Ok::<_, codec::Error>(Some(JamCandidate {
			para_id: para_id.into(),
			head,
			parent_head_hash,
		}))
	})();
	decoded.map_err(|e| anyhow!("Undecodable Parachain Service refine output {output:?}: {e}"))
}

/// Asserts the throughput of the paras in `expected_candidate_ranges` the way the relay chain
/// variant does, with JAM blocks in place of relay blocks.
///
/// The candidates are counted over `stop_after` finalized JAM blocks, starting with the first one
/// by which every tracked para has had a candidate accepted. `expected_number_of_blocks` checks
/// how many para blocks the counted candidates carry.
pub(crate) async fn assert_para_throughput(
	network: &Network<LocalFileSystem>,
	stop_after: u32,
	expected_candidate_ranges: HashMap<ParaId, Range<u32>>,
	expected_number_of_blocks: HashMap<ParaId, (OnlineClient<JamConfig>, Range<u32>)>,
) -> Result<(), anyhow::Error> {
	let tracked: HashSet<ParaId> = expected_candidate_ranges.keys().cloned().collect();
	let candidates = collect_para_throughput(network, stop_after, &tracked).await?;

	for (para_id, expected_candidate_range) in expected_candidate_ranges {
		let actual = candidates
			.get(&para_id)
			.ok_or_else(|| anyhow!("ParaId {para_id} did not have any accepted candidates"))?
			.len() as u32;

		if !expected_candidate_range.contains(&actual) {
			return Err(anyhow!(
				"ParaId {para_id}: candidate count {actual} not within expected range {expected_candidate_range:?}"
			));
		}
	}

	for (para_id, (para_client, expected_number_of_blocks)) in expected_number_of_blocks {
		let para_candidates = candidates
			.get(&para_id)
			.ok_or_else(|| anyhow!("ParaId did not have any accepted candidates"))?;

		let mut num_blocks = 0;
		for candidate in para_candidates {
			num_blocks += blocks_in_candidate(&para_client, candidate).await?;
		}

		if !expected_number_of_blocks.contains(&num_blocks) {
			return Err(anyhow!(
				"Block number count {num_blocks} not within range {expected_number_of_blocks:?}",
			));
		}
	}

	Ok(())
}

/// The accepted candidates of the `tracked` paras over `stop_after` counted JAM blocks.
async fn collect_para_throughput(
	network: &Network<LocalFileSystem>,
	stop_after: u32,
	tracked: &HashSet<ParaId>,
) -> Result<HashMap<ParaId, Vec<JamCandidate>>, anyhow::Error> {
	let jam_url = ordinary_jam_node_url(network)?;
	let url = url::Url::parse(&jam_url)?;
	let (jam, worker) = JamRpcInterface::new(vec![url])
		.await
		.map_err(|e| anyhow!("Connecting to JAM node {jam_url}: {e}"))?;
	let worker = tokio::spawn(worker);

	let result = count_candidates(&jam, stop_after, tracked).await;
	worker.abort();
	result
}

/// The RPC URL of the network's ordinary JAM node, the one the harness reads the JAM chain through.
pub(crate) fn ordinary_jam_node_url(
	network: &Network<LocalFileSystem>,
) -> Result<String, anyhow::Error> {
	let jam_node = network
		.jamchain()
		.ok_or_else(|| anyhow!("The network has no JAM chain"))?
		.nodes()
		.into_iter()
		.find(|node| matches!(node.mode(), JamNodeMode::Ordinary))
		.ok_or_else(|| anyhow!("The JAM chain has no ordinary node to read from"))?;
	Ok(jam_node.ws_uri().to_string())
}

async fn count_candidates(
	jam: &JamRpcInterface,
	stop_after: u32,
	tracked: &HashSet<ParaId>,
) -> Result<HashMap<ParaId, Vec<JamCandidate>>, anyhow::Error> {
	let mut finalized = jam.finalized_block_stream().await?;
	let mut candidates: HashMap<ParaId, Vec<JamCandidate>> = HashMap::new();
	let mut paras_seen = HashSet::new();
	let mut current_block_count = 0;
	let mut last_seen: Option<HeaderHash> = None;

	log::info!(
		"Asserting parachain throughput for para_ids: {tracked:?}. Waiting for accepted candidates from all tracked paras before counting."
	);

	while let Some(tip) = finalized.next().await {
		// The stream may skip blocks; every finalized block since the last one is counted.
		for (header_hash, slot) in blocks_since(jam, last_seen, (tip.header_hash, tip.slot)).await?
		{
			let availability = jam.availability(header_hash).await?;
			let accepted: Vec<JamCandidate> = accepted_candidates(
				availability
					.iter()
					.flatten()
					.map(|assignment| (assignment.registered_slot, assignment.report())),
				slot,
				PARACHAIN_SERVICE_ID,
			)?
			.into_iter()
			.filter(|candidate| tracked.contains(&candidate.para_id))
			.collect();

			// Skip JAM blocks until every tracked para has had at least one accepted candidate.
			// This avoids counting the warm-up period before every collator reached steady state.
			paras_seen.extend(accepted.iter().map(|candidate| candidate.para_id));
			if paras_seen.len() != tracked.len() {
				log::info!(
					"Not all tracked paras have had candidates accepted by the JAM block at slot {slot}. Not counting blocks yet."
				);
				continue;
			}

			current_block_count += 1;
			for candidate in accepted {
				log::debug!("Candidate accepted at slot {slot} for para_id {}", candidate.para_id);
				candidates.entry(candidate.para_id).or_default().push(candidate);
			}

			if current_block_count == stop_after {
				log::info!(
					"Reached {stop_after} finalized JAM blocks that contain accepted candidates. The per-parachain distribution is: {:#?}",
					candidates
						.iter()
						.map(|(para_id, candidates)| format!(
							"{para_id} has {} accepted candidates",
							candidates.len()
						))
						.collect::<Vec<_>>()
				);
				return Ok(candidates);
			}
		}
		last_seen = Some(tip.header_hash);
	}

	Err(anyhow!("The JAM finalized block stream ended"))
}

/// The finalized blocks after `last_seen` up to and including `tip`, oldest first.
async fn blocks_since(
	jam: &JamRpcInterface,
	last_seen: Option<HeaderHash>,
	tip: (HeaderHash, Slot),
) -> Result<Vec<(HeaderHash, Slot)>, anyhow::Error> {
	let Some(last_seen) = last_seen else { return Ok(vec![tip]) };

	let mut blocks = Vec::new();
	let mut current = tip;
	while current.0 != last_seen {
		blocks.push(current);
		let parent = jam.parent(current.0).await?;
		current = (parent.header_hash, parent.slot);
	}
	blocks.reverse();
	Ok(blocks)
}

/// How many para blocks `candidate` carries: its head and the ancestors down to its parent.
async fn blocks_in_candidate(
	para_client: &OnlineClient<JamConfig>,
	candidate: &JamCandidate,
) -> Result<u32, anyhow::Error> {
	let mut hash = candidate.head;
	for count in 1..=MAX_BLOCKS_PER_CANDIDATE {
		let parent = para_client.blocks().at(hash).await?.header().parent_hash;
		if blake2_256(parent.as_bytes()) == candidate.parent_head_hash {
			return Ok(count);
		}
		hash = parent;
	}
	Err(anyhow!(
		"Para {} candidate with head {:?} does not reach its parent within {MAX_BLOCKS_PER_CANDIDATE} blocks",
		candidate.para_id,
		candidate.head
	))
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;
	use jam_types::{RefineContext, WorkDigest, WorkError, WorkOutput};

	const SLOT: Slot = 42;
	const OTHER_SERVICE: ServiceId = 7;

	/// `ParachainWorkDigest::Ok` as the Parachain Service encodes it; the fields after
	/// `head_data` are irrelevant to a candidate.
	fn ok_output(para_id: u32, head: H256, parent_head_hash: [u8; 32]) -> Vec<u8> {
		(0u8, para_id, [0u8; 32], parent_head_hash, head.encode()).encode()
	}

	fn report(results: Vec<(ServiceId, Result<Vec<u8>, WorkError>)>) -> WorkReport {
		let results: Vec<WorkDigest> = results
			.into_iter()
			.map(|(service, result)| WorkDigest {
				service,
				code_hash: Default::default(),
				payload_hash: Default::default(),
				accumulate_gas: 0,
				result: result.map(WorkOutput),
				refine_load: Default::default(),
			})
			.collect();
		WorkReport {
			package_spec: jam_std_common::WorkPackageSpec {
				hash: Default::default(),
				len: 0,
				erasure_root: Default::default(),
				erasure_shards: Default::default(),
				exports_root: Default::default(),
				exports_count: 0,
			},
			context: RefineContext {
				anchor: Default::default(),
				anchor_slot: 0,
				state_root: Default::default(),
				beefy_root: Default::default(),
				lookup_anchor: Default::default(),
				lookup_anchor_slot: 0,
				lookup_anchor_state_root: Default::default(),
				prerequisites: Default::default(),
			},
			core_index: 0,
			authorizer_hash: Default::default(),
			auth_gas_used: 0,
			auth_output: Default::default(),
			sr_lookup: Default::default(),
			results: results.try_into().expect("a few digests always fit; qed"),
		}
	}

	fn candidate(para_id: u32, head: u8) -> (JamCandidate, Vec<u8>) {
		let candidate = JamCandidate {
			para_id: para_id.into(),
			head: H256::repeat_byte(head),
			parent_head_hash: [head.wrapping_sub(1); 32],
		};
		let output = ok_output(para_id, candidate.head, candidate.parent_head_hash);
		(candidate, output)
	}

	#[test]
	fn counts_only_reports_registered_in_the_block() {
		let (accepted, accepted_output) = candidate(2000, 1);
		let (_, pending_output) = candidate(2000, 2);
		let this_block = report(vec![(PARACHAIN_SERVICE_ID, Ok(accepted_output))]);
		let earlier_block = report(vec![(PARACHAIN_SERVICE_ID, Ok(pending_output))]);

		let candidates = accepted_candidates(
			[(SLOT, &this_block), (SLOT - 1, &earlier_block)],
			SLOT,
			PARACHAIN_SERVICE_ID,
		)
		.unwrap();

		assert_eq!(candidates, vec![accepted]);
	}

	#[test]
	fn attributes_each_report_to_its_para() {
		let (para_a, output_a) = candidate(2000, 1);
		let (para_b, output_b) = candidate(2001, 5);
		let core_0 = report(vec![(PARACHAIN_SERVICE_ID, Ok(output_a))]);
		let core_1 = report(vec![(PARACHAIN_SERVICE_ID, Ok(output_b))]);

		let candidates =
			accepted_candidates([(SLOT, &core_0), (SLOT, &core_1)], SLOT, PARACHAIN_SERVICE_ID)
				.unwrap();

		assert_eq!(candidates, vec![para_a, para_b]);
	}

	#[test]
	fn ignores_other_services_and_failed_refines() {
		let (accepted, accepted_output) = candidate(2000, 1);
		let (_, other_service_output) = candidate(2000, 2);
		let rejected_by_pvf = (1u8, 2000u32, [0u8; 32]).encode();
		let block = report(vec![
			(OTHER_SERVICE, Ok(other_service_output)),
			(PARACHAIN_SERVICE_ID, Err(WorkError::Panic)),
			(PARACHAIN_SERVICE_ID, Ok(rejected_by_pvf)),
			(PARACHAIN_SERVICE_ID, Ok(accepted_output)),
		]);

		let candidates = accepted_candidates([(SLOT, &block)], SLOT, PARACHAIN_SERVICE_ID).unwrap();

		assert_eq!(candidates, vec![accepted]);
	}

	#[test]
	fn an_undecodable_output_is_an_error() {
		let block = report(vec![(PARACHAIN_SERVICE_ID, Ok(vec![0, 1, 2]))]);

		assert!(accepted_candidates([(SLOT, &block)], SLOT, PARACHAIN_SERVICE_ID).is_err());
	}
}
