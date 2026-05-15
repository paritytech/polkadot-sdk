// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Proves a warp-synced node fetches pre-warp transaction bytes on renewal and
//! serves them through `bitswap_v1_get`.

use super::{
	common::{
		bitswap_v1_get, build_parachain_network_config, expect_dont_have, expect_no_log_line,
		get_best_block_height, initialize_network, renew_data_with_content_hash,
		verify_parachain_binaries, verify_warp_sync_completed, wait_for_block_height,
		wait_for_finalized_height, wait_for_fullnode, wait_for_new_block_beyond,
		wait_for_relay_chain_to_sync, wait_for_session_change_on_node,
		BLOCK_PRODUCTION_TIMEOUT_SECS, NETWORK_READY_TIMEOUT_SECS, NODE_LOG_CONFIG,
		PARACHAIN_BINARY, PARA_ID, SYNC_TIMEOUT_SECS,
	},
	fixture::{
		algorithm, content_hash, hash_to_cid, payload, HashingAlgorithm, ResolvedSnapshots,
		FIXTURE_RETENTION_PERIOD, N_STORES, TIP_SYNC_TARGET_BLOCKS,
	},
};
use anyhow::{anyhow, Context, Result};
use env_logger::Env;
use futures::future::try_join_all;
use std::time::Duration;
use zombienet_orchestrator::AddCollatorOptions;
use zombienet_sdk::{
	subxt::{config::substrate::SubstrateConfig, OnlineClient},
	NetworkNode,
};

const N_RENEW_EXERCISES: u32 = N_STORES;
const WARP_PRUNING_BLOCKS: u32 = 500;
const SESSION_CHANGE_TIMEOUT_SECS: u64 = 300;
const BITSWAP_RPC_POLL_TIMEOUT_SECS: u64 = 600;
const RENEW_BLOCK_SYNC_TIMEOUT_SECS: u64 = 600;
const RENEW_BATCH_SIZE: usize = 5;

type Entry = ([u8; 32], HashingAlgorithm);

fn fixture_entries() -> Vec<Entry> {
	(0..N_RENEW_EXERCISES).map(|i| (content_hash(i), algorithm(i))).collect()
}

fn verify_metadata(metadata: &super::fixture::SnapshotMetadata) -> Result<()> {
	anyhow::ensure!(metadata.total_blocks == TIP_SYNC_TARGET_BLOCKS);
	anyhow::ensure!(metadata.retention_period == FIXTURE_RETENTION_PERIOD);
	anyhow::ensure!(metadata.n_stores == N_STORES);
	anyhow::ensure!(N_RENEW_EXERCISES <= metadata.n_stores);
	Ok(())
}

async fn add_sync_node(
	network: &mut zombienet_sdk::Network<zombienet_sdk::LocalFileSystem>,
) -> Result<()> {
	network
		.add_collator(
			"sync-node",
			AddCollatorOptions {
				command: Some(PARACHAIN_BINARY.try_into()?),
				args: vec![
					"--sync=warp".into(),
					"--ipfs-server".into(),
					format!("--blocks-pruning={WARP_PRUNING_BLOCKS}").as_str().into(),
					NODE_LOG_CONFIG.into(),
				],
				is_validator: false,
				..Default::default()
			},
			PARA_ID,
		)
		.await?;
	Ok(())
}

async fn assert_missing_before_renewal(sync_node: &NetworkNode, entries: &[Entry]) -> Result<()> {
	for (i, (hash, algo)) in entries.iter().enumerate() {
		let cid = hash_to_cid(hash, *algo);
		expect_dont_have(sync_node, &cid, Duration::from_secs(BITSWAP_RPC_POLL_TIMEOUT_SECS))
			.await
			.with_context(|| format!("pre-renewal: sync-node should not have entry {i} ({cid})"))?;
	}
	Ok(())
}

async fn renew_entries(
	collator_client: &OnlineClient<SubstrateConfig>,
	collator: &NetworkNode,
	sync_node: &NetworkNode,
	entries: &[Entry],
) -> Result<Vec<Entry>> {
	let nonce = collator_client
		.tx()
		.account_nonce(
			&zombienet_sdk::subxt_signer::sr25519::dev::bob().public_key().to_account_id(),
		)
		.await?;
	let mut renewed = Vec::with_capacity(entries.len());

	for (batch_idx, chunk) in entries.chunks(RENEW_BATCH_SIZE).enumerate() {
		let batch_start = batch_idx * RENEW_BATCH_SIZE;
		let batch_outcomes = try_join_all(chunk.iter().copied().enumerate().map(
			|(local_offset, (hash, algo))| async move {
				let global_idx = batch_start + local_offset;
				let batch_nonce = nonce + global_idx as u64;
				let outcome = renew_data_with_content_hash(collator_client, hash, batch_nonce)
					.await
					.with_context(|| {
						format!("renewing entry {global_idx} (hash={})", hex::encode(hash))
					})?;
				Ok::<_, anyhow::Error>((global_idx, hash, algo, batch_nonce, outcome))
			},
		))
		.await?;

		let max_renewed_block = batch_outcomes
			.iter()
			.map(|(_, _, _, _, outcome)| outcome.renewed_at_block)
			.max()
			.context("renew batch produced no outcomes")?;

		for (global_idx, hash, algo, batch_nonce, outcome) in batch_outcomes {
			log::info!(
				"Renew batch {} entry {}/{}: algo={:?}, nonce={}, block={}, index={}",
				batch_idx + 1,
				global_idx + 1,
				entries.len(),
				algo,
				batch_nonce,
				outcome.renewed_at_block,
				outcome.renewed_index,
			);
			renewed.push((hash, algo));
		}

		wait_for_finalized_height(collator, max_renewed_block, BLOCK_PRODUCTION_TIMEOUT_SECS)
			.await?;
		wait_for_block_height(sync_node, max_renewed_block, RENEW_BLOCK_SYNC_TIMEOUT_SECS).await?;
	}

	Ok(renewed)
}

async fn assert_served_after_renewal(sync_node: &NetworkNode, renewed: &[Entry]) -> Result<()> {
	let deadline = std::time::Instant::now() + Duration::from_secs(BITSWAP_RPC_POLL_TIMEOUT_SECS);
	loop {
		let mut served = 0;
		for (hash, algo) in renewed {
			let cid = hash_to_cid(hash, *algo);
			if matches!(bitswap_v1_get(sync_node, &cid).await, Ok(Some(bytes)) if algo.hash(&bytes) == *hash)
			{
				served += 1;
			}
		}

		if served == renewed.len() {
			break;
		}
		if std::time::Instant::now() >= deadline {
			return Err(anyhow!(
				"post-renewal: sync-node served only {served} of {} entries",
				renewed.len()
			));
		}
		tokio::time::sleep(Duration::from_secs(1)).await;
	}

	for i in 0..N_RENEW_EXERCISES {
		let cid = hash_to_cid(&content_hash(i), algorithm(i));
		let Some(bytes) = bitswap_v1_get(sync_node, &cid).await? else {
			anyhow::bail!("bitswap_v1_get returned None for entry {i} after successful poll loop");
		};
		anyhow::ensure!(bytes == payload(i), "bitswap returned bytes do not match payload({i})");
	}

	Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn parachain_tip_sync_with_renewals_test() -> Result<()> {
	let _ = env_logger::Builder::from_env(Env::default().default_filter_or("info")).try_init();
	verify_parachain_binaries()?;

	let snapshots = ResolvedSnapshots::load()?;
	let metadata = snapshots.load_metadata()?;
	verify_metadata(&metadata)?;
	log::info!(
		"Loaded snapshot metadata: target={}, snapshot={}, stores={} ({}..{})",
		metadata.total_blocks,
		metadata.snapshot_height,
		metadata.n_stores,
		metadata.first_store_block,
		metadata.last_store_block,
	);

	let config = build_parachain_network_config(
		vec!["--ipfs-server".into(), NODE_LOG_CONFIG.into()],
		Some(snapshots.as_parachain_snapshots()),
	)?;
	let mut network = initialize_network(config).await?;
	network.wait_until_is_up(NETWORK_READY_TIMEOUT_SECS).await?;

	let alice = network.get_node("alice")?;
	wait_for_session_change_on_node(alice, SESSION_CHANGE_TIMEOUT_SECS).await?;

	{
		let collator = network.get_node("collator-1")?;
		let snapshot_height = get_best_block_height(collator).await?;
		wait_for_new_block_beyond(collator, snapshot_height, BLOCK_PRODUCTION_TIMEOUT_SECS).await?;
	}

	add_sync_node(&mut network).await?;
	let collator = network.get_node("collator-1")?;
	let sync_node = network.get_node("sync-node")?;
	wait_for_fullnode(sync_node).await?;
	wait_for_relay_chain_to_sync(sync_node, SYNC_TIMEOUT_SECS).await?;

	let warp_target = get_best_block_height(collator).await?;
	wait_for_block_height(sync_node, warp_target, SYNC_TIMEOUT_SECS).await?;
	verify_warp_sync_completed(sync_node).await?;

	let entries = fixture_entries();
	assert_missing_before_renewal(sync_node, &entries).await?;

	let collator_client: OnlineClient<SubstrateConfig> = collator.wait_client().await?;
	let renewed = renew_entries(&collator_client, collator, sync_node, &entries).await?;
	assert_served_after_renewal(sync_node, &renewed).await?;

	expect_no_log_line(collator, "(?i)bitswap.*hash.mismatch", 10, "collator hash mismatch")
		.await?;
	expect_no_log_line(sync_node, "(?i)bitswap.*hash.mismatch", 10, "sync-node hash mismatch")
		.await?;

	network.destroy().await?;
	Ok(())
}
