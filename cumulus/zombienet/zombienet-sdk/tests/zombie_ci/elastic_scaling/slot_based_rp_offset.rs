// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test that parachains that use a single slot-based collator with elastic scaling MVP and with
// elastic scaling with RFC103 can achieve full throughput of 3 candidates per block.

use anyhow::anyhow;
use codec::Decode;
use cumulus_primitives_core::relay_chain;
use cumulus_zombienet_sdk_helpers::{
	assert_relay_parent_offset, assign_cores, extract_relay_parent_storage_root,
};
use futures::StreamExt;
use serde_json::json;
use sp_consensus_babe::{ConsensusLog, BABE_ENGINE_ID};
use std::{
	cmp::max,
	collections::{HashMap, HashSet},
};
use subxt::config::substrate::Digest;
use zombienet_sdk::{
	subxt::{config::substrate::DigestItem, OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};
type Block = subxt::blocks::Block<PolkadotConfig, OnlineClient<PolkadotConfig>>;

fn extract_relay_parent_state_root_from_digest(
	digest: &Digest,
) -> Result<relay_chain::Hash, anyhow::Error> {
	for log in digest.logs.iter() {
		if let Some((h, _)) = extract_relay_parent_storage_root(log) {
			return Ok(h);
		}
	}
	Err(anyhow!("No RPSR digest found"))
}

fn does_rc_block_contain_session_change(relay_block: &Block) -> bool {
	for log in relay_block.header().digest.logs.iter() {
		match log {
			DigestItem::Consensus(id, val) if id == &BABE_ENGINE_ID => {
				let consensus_log = ConsensusLog::decode(&mut &val[..]);
				match consensus_log {
					Ok(ConsensusLog::NextEpochData(_)) => return true,
					_ => continue,
				}
			},
			_ => continue,
		}
	}
	return false
}
pub type BlockNumber = u32;

pub struct ParentFinder {
	rc_block_by_number: HashMap<BlockNumber, Block>,
	rc_height_by_hash: HashMap<relay_chain::Hash, BlockNumber>,
}
impl ParentFinder {
	pub fn new() -> Self {
		Self {
			rc_block_by_number: HashMap::<BlockNumber, Block>::new(),
			rc_height_by_hash: HashMap::<relay_chain::Hash, BlockNumber>::new(),
		}
	}
}

pub async fn assert_parablocks_are_built_on_rc_or_parent_of_rc_which_contains_session_change(
	relay_client: &OnlineClient<PolkadotConfig>,
	para_client: &OnlineClient<PolkadotConfig>,
	rc_offset_before_abort: u32,
) -> Result<bool, anyhow::Error> {
	let mut relay_block_stream = relay_client.blocks().subscribe_all().await?;

	// First parachain header #0 does not contains RSPR digest item.
	let mut para_block_stream = para_client.blocks().subscribe_all().await?.skip(1);

	let mut relay_blocks_with_session_change = HashSet::new();

	let mut highest_relay_block_seen = rc_offset_before_abort;
	let mut num_para_blocks_seen = 0;
	let rc_block_limit = 40;
	let mut parent_finder = ParentFinder::new();
	let para_block_limit = 100; // this must be larger than amount of parablocks in a session, such that we span across sessions

	let is_built_on_parent_of_relay_chain_block_with_session_change =
		|key: &relay_chain::Hash,
		 relay_blocks_with_session_change: &HashSet<relay_chain::Hash>,
		 parent_finder: &mut ParentFinder|
		 -> Result<bool, anyhow::Error> {
			let Some(height) = parent_finder.rc_height_by_hash.get(&key) else { return Ok(false) };
			let height_of_parent = height - 1;
			let Some(rc_parent_block) = parent_finder.rc_block_by_number.get(&height_of_parent)
			else {
				return Ok(false)
			};
			let parent_key = rc_parent_block.header().state_root;
			let parent_contains_session_change =
				relay_blocks_with_session_change.contains(&parent_key);
			if parent_contains_session_change {
				log::info!("Found session change in parent");
				return Ok(true);
			} else {
				return Ok(false)
			}
		};

	loop {
		tokio::select! {
			Some(Ok(rc_block)) = relay_block_stream.next() => {
				highest_relay_block_seen = max(rc_block.number(), highest_relay_block_seen);
				let has_progressed_passed_limit  = highest_relay_block_seen > (rc_offset_before_abort + rc_block_limit);
				let has_not_seen_any_para_blocks = num_para_blocks_seen == 0;
				if has_progressed_passed_limit && has_not_seen_any_para_blocks {
					return Err(anyhow!("No parachain blocks produced!"))
				}

				let key = rc_block.header().state_root;

				if does_rc_block_contain_session_change(&rc_block) {
					log::info!("Relay chain block #{} contains session change, key: {:?}", rc_block.number(), key);
					relay_blocks_with_session_change.insert(key);
				}

				log::trace!("Inserting relay block number {} key {:?}", rc_block.number(), key);
				parent_finder.rc_height_by_hash.insert(key, rc_block.number());
				log::trace!("Inserting rc block by number {}", rc_block.number());
				parent_finder.rc_block_by_number.insert(rc_block.number(), rc_block);
			},
			Some(Ok(para_block)) = para_block_stream.next() => {

				let key = extract_relay_parent_state_root_from_digest(&para_block.header().digest)?;

				let is_built_on_parent_of_rc = is_built_on_parent_of_relay_chain_block_with_session_change(
					&key,
					&relay_blocks_with_session_change,
					&mut parent_finder,
				)?;

				if is_built_on_parent_of_rc {
					log::info!("Crosses session boundary");
					break;
				}

				num_para_blocks_seen += 1;
				if num_para_blocks_seen >= para_block_limit {
					return Err(anyhow!("Did not build on relay chain block with session change after {para_block_limit} parachain blocks"));
				}
			}
		}
	}
	Ok(true)
}

#[tokio::test(flavor = "multi_thread")]
async fn elastic_scaling_slot_based_relay_parent_offset_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	// images are not relevant for `native`, but we leave it here in case we use `k8s` some day
	let images = zombienet_sdk::environment::get_images_from_env();

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								// Num cores is 4, because 2 extra will be added automatically when registering the paras.
								"num_cores": 4,
								// "lookahead": 8,
								"max_validators_per_core": 1
							}
						}
					}
				}))
				// Have to set a `with_node` outside of the loop below, so that `r` has the right
				// type.
				.with_node(|node| node.with_name("validator-0"));

			(1..6).fold(r, |acc, i| acc.with_node(|node| node.with_name(&format!("validator-{i}"))))
		})
		.with_parachain(|p| {
			p.with_id(2400)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("relay-parent-offset")
				.with_default_args(vec![
					"--authoring=slot-based".into(),
					("-lparachain=debug,aura=debug,parachain::collator-protocol=debug").into(),
				])
				.with_collator(|n| n.with_name("collator-rp-offset"))
		})
		.with_global_settings(|global_settings| match std::env::var("ZOMBIENET_SDK_BASE_DIR") {
			Ok(val) => global_settings.with_base_dir(val),
			_ => global_settings,
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	let para_node_rp_offset = network.get_node("collator-rp-offset")?;

	let para_client = para_node_rp_offset.wait_client().await?;

	assign_cores(relay_node, 2400, vec![0, 1]).await?;

	let highest_block_seen = assert_relay_parent_offset(&relay_client, &para_client, 2, 30).await?;

	// // Count parablocks to ensure that we ARE building on old session relay parents
	// assert_parablocks_are_built_on_rc_or_parent_of_rc_which_contains_session_change(
	// 	&relay_client,
	// 	&para_client,
	// 	highest_block_seen,
	// )
	// .await?;

	log::info!("Test finished successfully");

	Ok(())
}
