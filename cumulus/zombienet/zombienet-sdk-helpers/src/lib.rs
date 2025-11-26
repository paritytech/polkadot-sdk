// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use codec::{Decode, Encode};
use cumulus_primitives_core::{BundleInfo, CoreInfo, CumulusDigestItem, RelayBlockIdentifier};
use futures::{pin_mut, select, stream::StreamExt, TryStreamExt};
use polkadot_primitives::{BlakeTwo256, CandidateReceiptV2, HashT, Id as ParaId};
use sp_runtime::traits::Zero;
use std::{cmp::max, collections::HashMap, ops::Range, sync::Arc};
use tokio::{
	join,
	time::{sleep, Duration},
};
use zombienet_sdk::subxt::{
	self,
	backend::legacy::LegacyRpcMethods,
	blocks::Block,
	config::{polkadot::PolkadotExtrinsicParamsBuilder, substrate::DigestItem, Header},
	dynamic::Value,
	events::Events,
	ext::scale_value::value,
	tx::{signer::Signer, DynamicPayload, SubmittableTransaction, TxStatus},
	utils::H256,
	OnlineClient, PolkadotConfig,
};

/// Specifies which block should occupy a full core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockToCheck {
	/// The exact block hash provided should occupy a full core.
	Exact(H256),
	/// Wait for the next first bundle block.
	NextFirstBundleBlock(H256),
}

use zombienet_sdk::{
	tx_helper::{ChainUpgrade, RuntimeUpgradeOptions},
	LocalFileSystem, Network, NetworkNode,
};

use zombienet_configuration::types::AssetLocation;

// Maximum number of blocks to wait for a session change.
// If it does not arrive for whatever reason, we should not wait forever.
const WAIT_MAX_BLOCKS_FOR_SESSION: u32 = 50;

/// Create a batch call to assign cores to a parachain.
///
/// Zombienet by default adds extra core for each registered parachain additionally to the one
/// requested by `num_cores`. It then assigns the parachains to the extra cores allocated at the
/// end. So, the passed core indices should be counted from zero.
///
/// # Example
///
/// Genesis patch:
/// ```json
/// "configuration": {
/// 		"config": {
/// 			"scheduler_params": {
/// 				"num_cores": 2,
/// 			}
/// 		}
/// 	}
/// ```
///
/// Runs the relay chain with `2` cores and we also add two parachains.
/// To assign these extra `2` cores, the call would look like this:
///
/// ```rust
/// create_assign_core_call(&[(0, 2400), (1, 2400)])
/// ```
///
/// The cores `2` and `3` are assigned to the parachains by zombienet.
pub fn create_assign_core_call(core_and_para: &[(u32, u32)]) -> DynamicPayload {
	let mut assign_cores = vec![];
	for (core, para_id) in core_and_para.iter() {
		assign_cores.push(value! {
			Coretime(assign_core { core : *core, begin: 0, assignment: ((Task(*para_id), 57600)), end_hint: None() })
		});
	}

	zombienet_sdk::subxt::tx::dynamic(
		"Sudo",
		"sudo",
		vec![value! {
			Utility(batch { calls: assign_cores })
		}],
	)
}

/// Find an event in subxt `Events` and attempt to decode the fields fo the event.
fn find_event_and_decode_fields<T: Decode>(
	events: &Events<PolkadotConfig>,
	pallet: &str,
	variant: &str,
) -> Result<Vec<T>, anyhow::Error> {
	let mut result = vec![];
	for event in events.iter() {
		let event = event?;
		if event.pallet_name() == pallet && event.variant_name() == variant {
			result.push(T::decode(&mut &event.field_bytes()[..])?);
		}
	}
	Ok(result)
}

// Helper function for asserting the throughput of parachains, after the first session change.
//
// The throughput is measured as total number of backed candidates in a window of relay chain
// blocks. Relay chain blocks with session changes are generally ignores.
pub async fn assert_para_throughput(
	relay_client: &OnlineClient<PolkadotConfig>,
	stop_after: u32,
	expected_candidate_ranges: impl Into<HashMap<ParaId, Range<u32>>>,
	expected_number_of_blocks: impl Into<HashMap<ParaId, (OnlineClient<PolkadotConfig>, Range<u32>)>>,
) -> Result<(), anyhow::Error> {
	let mut blocks_sub = relay_client.blocks().subscribe_finalized().await?;
	let mut candidate_count: HashMap<ParaId, Vec<CandidateReceiptV2<H256>>> = HashMap::new();
	let mut current_block_count = 0;

	let expected_candidate_ranges = expected_candidate_ranges.into();
	let expected_number_of_blocks = expected_number_of_blocks.into();
	let valid_para_ids: Vec<ParaId> = expected_candidate_ranges.keys().cloned().collect();

	// Wait for the first session, block production on the parachain will start after that.
	wait_for_first_session_change(&mut blocks_sub).await?;

	while let Some(block) = blocks_sub.next().await {
		let block = block?;
		log::debug!("Finalized relay chain block {}", block.number());
		let events = block.events().await?;

		// Do not count blocks with session changes, no backed blocks there.
		if is_session_change(&block).await? {
			continue
		}

		current_block_count += 1;

		let receipts = find_event_and_decode_fields::<CandidateReceiptV2<H256>>(
			&events,
			"ParaInclusion",
			"CandidateBacked",
		)?;

		for receipt in receipts {
			let para_id = receipt.descriptor.para_id();
			log::debug!("Block backed for para_id {para_id}");

			if !valid_para_ids.contains(&para_id) {
				return Err(anyhow!("Invalid ParaId detected: {}", para_id));
			};

			candidate_count.entry(para_id).or_default().push(receipt);
		}

		if current_block_count == stop_after {
			break;
		}
	}

	log::info!(
		"Reached {stop_after} finalized relay chain blocks that contain backed candidates. The per-parachain distribution is: {:#?}",
		candidate_count.iter().map(|(para_id, receipts)| format!("{para_id} has {} backed candidates", receipts.len())).collect::<Vec<_>>()
	);

	for (para_id, expected_candidate_range) in expected_candidate_ranges {
		let receipts = candidate_count
			.get(&para_id)
			.ok_or_else(|| anyhow!("ParaId did not have any backed candidates"))?;

		if !expected_candidate_range.contains(&(receipts.len() as u32)) {
			return Err(anyhow!(
				"Candidate count {} not within range {expected_candidate_range:?}",
				receipts.len()
			))
		}
	}

	for (para_id, (para_client, expected_number_of_blocks)) in expected_number_of_blocks {
		let receipts = candidate_count
			.get(&para_id)
			.ok_or_else(|| anyhow!("ParaId did not have any backed candidates"))?;

		let mut num_blocks = 0;

		for receipt in receipts {
			// We "abuse" the fact that the parachain is using `BlakeTwo256` as hash and thus, the
			// `para_head` hash and the hash of the `header` should be equal.
			let mut next_para_block_hash = receipt.descriptor().para_head();

			let mut relay_identifier = None;
			let mut core_info = None;

			loop {
				let block = para_client.blocks().at(next_para_block_hash).await?;

				// Genesis block is not part of a candidate :)
				if block.number() == 0 {
					break
				}

				let ri = find_relay_block_identifier(&block)?;
				let ci = find_core_info(&block)?;

				// If the core changes or the relay identifier, we found all blocks for the
				// candidate.
				if *relay_identifier.get_or_insert(ri.clone()) != ri ||
					*core_info.get_or_insert(ci.clone()) != ci
				{
					break
				}

				num_blocks += 1;
				next_para_block_hash = block.header().parent_hash;
			}
		}

		if !expected_number_of_blocks.contains(&num_blocks) {
			return Err(anyhow!(
				"Block number count {num_blocks} not within range {expected_number_of_blocks:?}",
			))
		}
	}

	Ok(())
}

/// Returns `true` if the `block` is a session change.
async fn is_session_change(
	block: &Block<PolkadotConfig, OnlineClient<PolkadotConfig>>,
) -> Result<bool, anyhow::Error> {
	let events = block.events().await?;
	Ok(events.iter().any(|event| {
		event.as_ref().is_ok_and(|event| {
			event.pallet_name() == "Session" && event.variant_name() == "NewSession"
		})
	}))
}

/// Returns [`CoreInfo`] for the given parachain block.
pub fn find_core_info(
	block: &Block<PolkadotConfig, OnlineClient<PolkadotConfig>>,
) -> Result<CoreInfo, anyhow::Error> {
	let substrate_digest =
		sp_runtime::generic::Digest::decode(&mut &block.header().digest.encode()[..])
			.expect("`subxt::Digest` and `substrate::Digest` should encode and decode; qed");

	CumulusDigestItem::find_core_info(&substrate_digest)
		.ok_or_else(|| anyhow!("Failed to find `CoreInfo` digest"))
}

/// Returns [`RelayBlockIdentifier`] for the given parachain block.
fn find_relay_block_identifier(
	block: &Block<PolkadotConfig, OnlineClient<PolkadotConfig>>,
) -> Result<RelayBlockIdentifier, anyhow::Error> {
	let substrate_digest =
		sp_runtime::generic::Digest::decode(&mut &block.header().digest.encode()[..])
			.expect("`subxt::Digest` and `substrate::Digest` should encode and decode; qed");

	CumulusDigestItem::find_relay_block_identifier(&substrate_digest)
		.ok_or_else(|| anyhow!("Failed to find `RelayBlockIdentifier` digest"))
}

/// Find the `CandidateIncluded` events for the given `para_id`.
async fn find_candidate_included_events(
	para_id: ParaId,
	block: &Block<PolkadotConfig, OnlineClient<PolkadotConfig>>,
) -> Result<Vec<CandidateReceiptV2<H256>>, anyhow::Error> {
	let events = block.events().await?;

	find_event_and_decode_fields::<CandidateReceiptV2<H256>>(
		&events,
		"ParaInclusion",
		"CandidateIncluded",
	)
	.map(|events| events.into_iter().filter(|e| e.descriptor.para_id() == para_id).collect())
}

/// Assert that `stop_after` parachain blocks are included via `expected_relay_blocks`.
///
/// It waits for `stop_after` parachain blocks to be finalized. Then it ensures that these parachain
/// blocks are included on the relay chain using the given number of `expected_relay_blocks`.
pub async fn assert_para_blocks_throughput(
	para_id: ParaId,
	para_client: &OnlineClient<PolkadotConfig>,
	stop_after: usize,
	relay_rpc_client: &LegacyRpcMethods<PolkadotConfig>,
	relay_client: &OnlineClient<PolkadotConfig>,
	expected_relay_blocks: Range<u32>,
	expected_candidates_per_relay_block: Range<usize>,
) -> Result<(), anyhow::Error> {
	// Wait for the first session, block production on the parachain will start after that.
	wait_for_first_session_change(&mut relay_client.blocks().subscribe_best().await?).await?;

	para_client
		.blocks()
		.subscribe_finalized()
		.await?
		.try_filter(|b| {
			futures::future::ready(find_core_info(b).map_or(false, |info| {
				expected_candidates_per_relay_block.contains(&(info.number_of_cores.0 as usize))
			}))
		})
		.next()
		.await
		.transpose()?;

	let finalized_stream = para_client.blocks().subscribe_finalized().await?.fuse();
	let finalized_relay_blocks = relay_client.blocks().subscribe_finalized().await?.fuse();
	let start_relay_block = relay_client
		.blocks()
		.subscribe_best()
		.await?
		.next()
		.await
		.ok_or_else(|| anyhow!("Could not get a best block from the relay chain"))??;

	let mut finalized_parachain_blocks = Vec::new();

	pin_mut!(finalized_stream);
	pin_mut!(finalized_relay_blocks);

	let last_finalized_relay_block = loop {
		select! {
			finalized = finalized_stream.select_next_some() => {
				let finalized = finalized?;
				if !finalized.number().is_zero() && finalized_parachain_blocks.len() < stop_after {
					finalized_parachain_blocks.push(finalized);
				}
			},
			finalized = finalized_relay_blocks.select_next_some() => {
				let finalized = finalized?;
				let num_relay_chain_blocks = finalized.number().saturating_sub(start_relay_block.number());

				// If we have recorded enough parachain blocks
				if finalized_parachain_blocks.len() >= stop_after {
					break finalized
				}

				// `start_relay_block` maybe not being finalized at the beginning, but we just
				// need some good estimation to ensure the tests ends at some point if there is some issue.
				if num_relay_chain_blocks >= expected_relay_blocks.end {
					return Err(anyhow!("Already processed more relay chain blocks ({num_relay_chain_blocks}) \
						than allowed in the range ({expected_relay_blocks:?})."))
				}
			},
			complete => { panic!("Both streams should not finish"); }
		}
	};

	// The number of cores occupied by the parachain candidates, ignoring session changes.
	let mut occupied_relay_chain_blocks = 0;
	// Did we found the first candidate matching one of our expected parachain blocks?
	let mut found_first_candidate = false;
	let mut current_relay_header = last_finalized_relay_block.header().clone();
	loop {
		if current_relay_header.number().is_zero() {
			return Err(anyhow!(
				"Reached relay genesis block without finding all parachain blocks?"
			));
		}

		let block = relay_rpc_client
			.chain_get_block(Some(current_relay_header.hash_with(relay_client.hasher())))
			.await?
			.ok_or_else(|| {
				anyhow!(
					"Could not fetch relay block: {:?}",
					current_relay_header.hash_with(relay_client.hasher())
				)
			})?
			.block;

		let block = relay_client.blocks().at(block.header.hash_with(relay_client.hasher())).await?;

		let included_events = find_candidate_included_events(para_id, &block).await?;

		let included_parachain_block_identifiers = included_events
			.iter()
			.filter_map(|i| {
				finalized_parachain_blocks.iter().rev().find_map(|p| {
					(BlakeTwo256::hash_of(p.header()) == i.descriptor.para_head()).then(|| {
						find_core_info(&p)
							.and_then(|c| find_relay_block_identifier(&p).map(|rbi| (c, rbi)))
					})
				})
			})
			.collect::<Result<Vec<_>, _>>()?;

		finalized_parachain_blocks.retain(|b| {
			let core_info = find_core_info(b).unwrap();
			let rbi = find_relay_block_identifier(b).unwrap();

			!included_parachain_block_identifiers.contains(&(core_info, rbi))
		});

		if !is_session_change(&block).await? {
			found_first_candidate |= !included_parachain_block_identifiers.is_empty();

			if found_first_candidate {
				occupied_relay_chain_blocks += 1;
			}

			if !included_parachain_block_identifiers.is_empty() &&
				!expected_candidates_per_relay_block
					.contains(&included_parachain_block_identifiers.len())
			{
				return Err(anyhow!(
					"{} candidates did not match the expected {expected_candidates_per_relay_block:?} \
					candidates per relay chain block", included_parachain_block_identifiers.len()
				))
			}
		}

		if finalized_parachain_blocks.is_empty() {
			break
		}

		current_relay_header = relay_rpc_client
			.chain_get_header(Some(current_relay_header.parent_hash))
			.await?
			.ok_or_else(|| {
				anyhow!(
					"Could not fetch relay chain header: {:?}",
					current_relay_header.parent_hash
				)
			})?;
	}

	if !expected_relay_blocks.contains(&occupied_relay_chain_blocks) {
		return Err(anyhow!("{occupied_relay_chain_blocks} did not match the expected {expected_candidates_per_relay_block:?} relay chain blocks"))
	}

	Ok(())
}

/// Wait for the first block with a session change.
///
/// The session change is detected by inspecting the events in the block.
pub async fn wait_for_first_session_change(
	blocks_sub: &mut zombienet_sdk::subxt::backend::StreamOfResults<
		Block<PolkadotConfig, OnlineClient<PolkadotConfig>>,
	>,
) -> Result<(), anyhow::Error> {
	wait_for_nth_session_change(blocks_sub, 1).await
}

/// Wait for the first block with the Nth session change.
///
/// The session change is detected by inspecting the events in the block.
pub async fn wait_for_nth_session_change(
	blocks_sub: &mut zombienet_sdk::subxt::backend::StreamOfResults<
		Block<PolkadotConfig, OnlineClient<PolkadotConfig>>,
	>,
	mut sessions_to_wait: u32,
) -> Result<(), anyhow::Error> {
	let mut waited_block_num = 0;
	while let Some(block) = blocks_sub.next().await {
		let block = block?;
		log::debug!("Finalized relay chain block {}", block.number());

		if is_session_change(&block).await? {
			sessions_to_wait -= 1;
			if sessions_to_wait == 0 {
				return Ok(());
			}

			waited_block_num = 0;
		} else {
			if waited_block_num >= WAIT_MAX_BLOCKS_FOR_SESSION {
				return Err(anyhow::format_err!("Waited for {WAIT_MAX_BLOCKS_FOR_SESSION}, a new session should have been arrived by now."));
			}

			waited_block_num += 1;
		}
	}
	Ok(())
}

// Helper function that asserts the maximum finality lag.
pub async fn assert_finality_lag(
	client: &OnlineClient<PolkadotConfig>,
	maximum_lag: u32,
) -> Result<(), anyhow::Error> {
	let mut best_stream = client.blocks().subscribe_best().await?;
	let mut fut_stream = client.blocks().subscribe_finalized().await?;
	let (Some(Ok(best)), Some(Ok(finalized))) = join!(best_stream.next(), fut_stream.next()) else {
		return Err(anyhow::format_err!("Unable to fetch best an finalized block!"));
	};
	let finality_lag = best.number() - finalized.number();

	log::info!(
		"Finality lagged by {finality_lag} blocks, maximum expected was {maximum_lag} blocks"
	);

	assert!(finality_lag <= maximum_lag, "Expected finality to lag by a maximum of {maximum_lag} blocks, but was lagging by {finality_lag} blocks.");
	Ok(())
}

/// Assert that finality has not stalled.
pub async fn assert_blocks_are_being_finalized(
	client: &OnlineClient<PolkadotConfig>,
) -> Result<(), anyhow::Error> {
	let sleep_duration = Duration::from_secs(12);
	let mut finalized_blocks = client.blocks().subscribe_finalized().await?;
	let first_measurement = finalized_blocks
		.next()
		.await
		.ok_or(anyhow::anyhow!("Can't get finalized block from stream"))??
		.number();
	sleep(sleep_duration).await;
	let second_measurement = finalized_blocks
		.next()
		.await
		.ok_or(anyhow::anyhow!("Can't get finalized block from stream"))??
		.number();

	log::info!(
		"Finalized {} blocks within {sleep_duration:?}",
		second_measurement - first_measurement
	);
	assert!(second_measurement > first_measurement);

	Ok(())
}

/// Asserts that parachain blocks have the correct relay parent offset.
///
/// # Arguments
///
/// * `relay_client` - Client connected to a relay chain node
/// * `para_client` - Client connected to a parachain node
/// * `offset` - Expected minimum offset between relay parent and highest seen relay block
/// * `block_limit` - Number of parachain blocks to verify before completing
pub async fn assert_relay_parent_offset(
	relay_client: &OnlineClient<PolkadotConfig>,
	para_client: &OnlineClient<PolkadotConfig>,
	offset: u32,
	block_limit: u32,
) -> Result<(), anyhow::Error> {
	let mut relay_block_stream = relay_client.blocks().subscribe_all().await?;

	// First parachain header #0 does not contains RSPR digest item.
	let mut para_block_stream = para_client.blocks().subscribe_all().await?.skip(1);
	let mut highest_relay_block_seen = 0;
	let mut num_para_blocks_seen = 0;
	loop {
		tokio::select! {
			Some(Ok(relay_block)) = relay_block_stream.next() => {
				highest_relay_block_seen = max(relay_block.number(), highest_relay_block_seen);
				if highest_relay_block_seen > 15 && num_para_blocks_seen == 0 {
					return Err(anyhow!("No parachain blocks produced!"))
				}
			},
			Some(Ok(para_block)) = para_block_stream.next() => {
				let relay_block_identifier = find_relay_block_identifier(&para_block)?;

				let relay_parent_number = match relay_block_identifier {
					RelayBlockIdentifier::ByHash(block_hash) => relay_client.blocks().at(block_hash).await?.number(),
					RelayBlockIdentifier::ByStorageRoot { block_number, .. } => block_number,
				};

				log::debug!("Parachain block #{} was built on relay parent #{relay_parent_number}, highest seen was {highest_relay_block_seen}", para_block.number());

				assert!(
					highest_relay_block_seen < offset ||
					relay_parent_number <= highest_relay_block_seen.saturating_sub(offset),
					"Relay parent is not at the correct offset! relay_parent: #{relay_parent_number} highest_seen_relay_block: #{highest_relay_block_seen}",
				);
				num_para_blocks_seen += 1;
				if num_para_blocks_seen >= block_limit {
					log::info!("Successfully verified relay parent offset of {offset} for {num_para_blocks_seen} parachain blocks.");
					break;
				}
			}
		}
	}

	Ok(())
}

/// Submits the given `call` as signed transaction and waits for it successful finalization.
///
/// The transaction is send as immortal transaction.
pub async fn submit_extrinsic_and_wait_for_finalization_success<S: Signer<PolkadotConfig>>(
	client: &OnlineClient<PolkadotConfig>,
	call: &DynamicPayload,
	signer: &S,
) -> Result<H256, anyhow::Error> {
	let extensions = PolkadotExtrinsicParamsBuilder::new().immortal().build();

	let tx = client.tx().create_signed(call, signer, extensions).await?;

	submit_tx_and_wait_for_finalization(tx).await
}

/// Submits the given `call` as unsigned transaction and waits for it successful finalization.
pub async fn submit_unsigned_extrinsic_and_wait_for_finalization_success(
	client: &OnlineClient<PolkadotConfig>,
	call: &DynamicPayload,
) -> Result<H256, anyhow::Error> {
	let tx = client.tx().create_unsigned(call)?;

	submit_tx_and_wait_for_finalization(tx).await
}

/// Submit the given transaction and wait for its finalization.
async fn submit_tx_and_wait_for_finalization(
	tx: SubmittableTransaction<PolkadotConfig, OnlineClient<PolkadotConfig>>,
) -> Result<H256, anyhow::Error> {
	log::info!("Submitting transaction: {:?}", tx.hash());

	let mut tx = tx.submit_and_watch().await?;

	// Below we use the low level API to replicate the `wait_for_in_block` behavior
	// which was removed in subxt 0.33.0. See https://github.com/paritytech/subxt/pull/1237.
	while let Some(status) = tx.next().await.transpose()? {
		match status {
			TxStatus::InBestBlock(tx_in_block) => {
				tx_in_block.wait_for_success().await?;

				log::info!("[Best] In block: {:#?}", tx_in_block.block_hash());
			},
			TxStatus::InFinalizedBlock(ref tx_in_block) => {
				tx_in_block.wait_for_success().await?;
				log::info!("[Finalized] In block: {:#?}", tx_in_block.block_hash());
				return Ok(tx_in_block.block_hash())
			},
			TxStatus::Error { message } |
			TxStatus::Invalid { message } |
			TxStatus::Dropped { message } => {
				return Err(anyhow::anyhow!("Error submitting tx: {message}"));
			},
			_ => continue,
		}
	}

	Err(anyhow::anyhow!("Transaction event stream ended without reaching the finalized state"))
}

/// Submits the given `call` as transaction and waits `timeout_secs` for it successful finalization.
///
/// If the transaction does not reach the finalized state in `timeout_secs` an error is returned.
/// The transaction is send as immortal transaction.
pub async fn submit_extrinsic_and_wait_for_finalization_success_with_timeout<
	S: Signer<PolkadotConfig>,
>(
	client: &OnlineClient<PolkadotConfig>,
	call: &DynamicPayload,
	signer: &S,
	timeout_secs: impl Into<u64>,
) -> Result<(), anyhow::Error> {
	let secs = timeout_secs.into();
	let res = tokio::time::timeout(
		Duration::from_secs(secs),
		submit_extrinsic_and_wait_for_finalization_success(client, call, signer),
	)
	.await;

	match res {
		Ok(Ok(_)) => Ok(()),
		Ok(Err(e)) => Err(anyhow!("Error waiting for metric: {}", e)),
		// timeout
		Err(_) => Err(anyhow!("Timeout ({secs}), waiting for extrinsic finalization")),
	}
}

/// Asserts that the given `para_id` is registered at the relay chain.
pub async fn assert_para_is_registered(
	relay_client: &OnlineClient<PolkadotConfig>,
	para_id: ParaId,
	blocks_to_wait: u32,
) -> Result<(), anyhow::Error> {
	let mut blocks_sub = relay_client.blocks().subscribe_all().await?;
	let para_id: u32 = para_id.into();

	let keys: Vec<Value> = vec![];
	let query = subxt::dynamic::storage("Paras", "Parachains", keys);

	let mut blocks_cnt = 0;
	while let Some(block) = blocks_sub.next().await {
		let block = block?;
		log::debug!("Relay block #{}, checking if para_id {para_id} is registered", block.number(),);
		let parachains = block.storage().fetch(&query).await?;

		let parachains: Vec<u32> = match parachains {
			Some(parachains) => parachains.as_type()?,
			None => vec![],
		};

		log::debug!("Registered para_ids: {:?}", parachains);

		if parachains.iter().any(|p| para_id.eq(p)) {
			log::debug!("para_id {para_id} registered");
			return Ok(());
		}
		if blocks_cnt >= blocks_to_wait {
			return Err(anyhow!(
				"Parachain {para_id} not registered within {blocks_to_wait} blocks"
			));
		}
		blocks_cnt += 1;
	}

	Err(anyhow!("No more blocks to check"))
}

/// Returns [`BundleInfo`] for the given parachain block.
fn find_bundle_info(
	block: &Block<PolkadotConfig, OnlineClient<PolkadotConfig>>,
) -> Result<BundleInfo, anyhow::Error> {
	let substrate_digest =
		sp_runtime::generic::Digest::decode(&mut &block.header().digest.encode()[..])
			.expect("`subxt::Digest` and `substrate::Digest` should encode and decode; qed");

	CumulusDigestItem::find_bundle_info(&substrate_digest)
		.ok_or_else(|| anyhow!("Failed to find `BundleInfo` digest"))
}

/// Validates that the given block is a "special" block in the core.
///
/// If `is_only_block_in_core` is true, it checks if the given block is the first block in the core
/// and the only one. If this is `false`, it only checks if the block is the last block in the core.
async fn ensure_is_block_in_core_impl(
	para_client: &OnlineClient<PolkadotConfig>,
	block_hash: H256,
	is_only_block_in_core: bool,
) -> Result<(), anyhow::Error> {
	let blocks = para_client.blocks();
	let block = blocks.at(block_hash).await?;
	let block_core_info = find_core_info(&block)?;

	if is_only_block_in_core {
		let parent = blocks.at(block.header().parent_hash).await?;

		// Genesis is for sure on a different core :)
		if parent.number() != 0 {
			let parent_core_info = find_core_info(&parent)?;

			if parent_core_info == block_core_info {
				return Err(anyhow::anyhow!(
					"Not first block ({}) in core, at least the parent block is on the same core.",
					block.header().number
				));
			}
		}
	}

	let next_block = loop {
		// Start with the latest best block.
		let mut current_block = Arc::new(blocks.subscribe_best().await?.next().await.unwrap()?);

		let mut next_block = None;

		while current_block.hash() != block_hash {
			next_block = Some(current_block.clone());
			current_block = Arc::new(blocks.at(current_block.header().parent_hash).await?);

			if current_block.number() == 0 {
				return Err(anyhow::anyhow!(
					"Did not found block while going backwards from the best block"
				))
			}
		}

		// It possible that the first block we got is the same as the transaction got finalized.
		// So, we just retry again until we found some more blocks.
		if let Some(next_block) = next_block {
			break next_block
		}
	};

	let next_block_core_info = find_core_info(&next_block)?;

	if next_block_core_info == block_core_info {
		return Err(anyhow::anyhow!(
			"Not {} block ({}) in core, at least the following block is on the same core.",
			if is_only_block_in_core { "first" } else { "last" },
			block.header().number
		));
	}

	Ok(())
}

/// Checks if the specified block occupies a full core.
pub async fn ensure_is_only_block_in_core(
	para_client: &OnlineClient<PolkadotConfig>,
	block_to_check: BlockToCheck,
) -> Result<(), anyhow::Error> {
	let blocks = para_client.blocks();

	match block_to_check {
		BlockToCheck::Exact(block_hash) =>
			ensure_is_block_in_core_impl(para_client, block_hash, true).await,
		BlockToCheck::NextFirstBundleBlock(start_block_hash) => {
			let start_block = blocks.at(start_block_hash).await?;

			let mut best_block_stream = blocks.subscribe_best().await?;

			let mut next_first_bundle_block = None;
			while let Some(mut block) = best_block_stream.next().await.transpose()? {
				while block.number() > start_block.number() {
					if find_bundle_info(&block)?.index == 0 {
						next_first_bundle_block = Some(block.hash());
					}

					block = blocks.at(block.header().parent_hash).await?;
				}

				if next_first_bundle_block.is_some() {
					break;
				}
			}

			if let Some(block) = next_first_bundle_block {
				ensure_is_block_in_core_impl(para_client, block, true).await
			} else {
				Err(anyhow!("Could not find the next bundle after {}", start_block.number()))
			}
		},
	}
}

/// Checks if the specified block is the last block in a core.
///
/// Also ensures that the last block is NOT the first block.
pub async fn ensure_is_last_block_in_core(
	para_client: &OnlineClient<PolkadotConfig>,
	block_to_check: H256,
) -> Result<(), anyhow::Error> {
	ensure_is_block_in_core_impl(para_client, block_to_check, false).await?;

	let blocks = para_client.blocks();
	let block = blocks.at(block_to_check).await?;
	let bundle_info = find_bundle_info(&block)?;

	// Above we ensure it is the last block in the core and now we want to ensure it isn't the first
	// block.
	if bundle_info.index == 0 {
		Err(anyhow!("`{block_to_check:?}` is the first block of a core and not the last"))
	} else {
		Ok(())
	}
}

pub async fn runtime_upgrade(
	network: &Network<LocalFileSystem>,
	node: &NetworkNode,
	para_id: u32,
	wasm_path: &str,
) -> Result<(), anyhow::Error> {
	log::info!("Performing runtime upgrade for parachain {}, wasm: {}", para_id, wasm_path);
	let para = network.parachain(para_id).unwrap();

	para.perform_runtime_upgrade(node, RuntimeUpgradeOptions::new(AssetLocation::from(wasm_path)))
		.await
}

/// Assigns the given `cores` to the given `para_id`.
///
/// Zombienet by default adds extra core for each registered parachain additionally to the one
/// requested by `num_cores`. It then assigns the parachains to the extra cores allocated at the
/// end. So, the passed core indices should be counted from zero.
///
/// # Example
///
/// Genesis patch:
/// ```json
/// "configuration": {
/// 		"config": {
/// 			"scheduler_params": {
/// 				"num_cores": 2,
/// 			}
/// 		}
/// 	}
/// ```
///
/// Runs the relay chain with `2` cores and we also add two parachains.
/// To assign these extra `2` cores, the call would look like this:
///
/// ```rust
/// assign_core(&relay_node, PARA_ID, vec![0, 1])
/// ```
///
/// The cores `2` and `3` are assigned to the parachains by Zombienet.
pub async fn assign_cores(
	relay_node: &NetworkNode,
	para_id: u32,
	cores: Vec<u32>,
) -> Result<(), anyhow::Error> {
	log::info!("Assigning {:?} cores to parachain {}", cores, para_id);

	let assign_cores_call =
		create_assign_core_call(&cores.into_iter().map(|core| (core, para_id)).collect::<Vec<_>>());

	let client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;
	let res = submit_extrinsic_and_wait_for_finalization_success_with_timeout(
		&client,
		&assign_cores_call,
		&zombienet_sdk::subxt_signer::sr25519::dev::alice(),
		60u64,
	)
	.await;
	assert!(res.is_ok(), "Extrinsic failed to finalize: {:?}", res.unwrap_err());
	log::info!("Cores assigned to the parachain");

	Ok(())
}

/// Wait until a runtime upgrade has happened.
///
/// This checks all finalized blocks until it finds a block that sets the
/// `RuntimeEnvironmentUpdated` digest.
///
/// Returns the hash of the block at which the runtime upgrade was applied.
pub async fn wait_for_runtime_upgrade(
	client: &OnlineClient<PolkadotConfig>,
) -> Result<H256, anyhow::Error> {
	let mut finalized_blocks = client.blocks().subscribe_finalized().await?;

	while let Some(Ok(block)) = finalized_blocks.next().await {
		if block
			.header()
			.digest
			.logs
			.iter()
			.any(|d| matches!(d, DigestItem::RuntimeEnvironmentUpdated))
		{
			log::info!("Runtime upgraded in block {:?}", block.hash());

			return Ok(block.hash())
		}
	}

	Err(anyhow!("Did not find a runtime upgrade"))
}
