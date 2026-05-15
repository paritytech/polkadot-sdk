// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Snapshot builder for the storage-chain tip-sync test.
//!
//! Produces node databases plus metadata/chainspec sidecars. The companion
//! `generate-snapshots.sh` script archives the databases into tarballs.

use super::{
	common::{
		get_alice_nonce, initialize_network, verify_parachain_binaries, wait_for_block_height,
		wait_for_finalized_height, wait_for_in_best_block, wait_for_session_change_on_node,
		NETWORK_READY_TIMEOUT_SECS, NODE_LOG_CONFIG, PARACHAIN_BINARY, PARACHAIN_CHAIN_SPEC,
		PARA_ID, RELAY_BINARY, RELAY_CHAIN, SYNC_TIMEOUT_SECS,
	},
	fixture::{
		algorithm, content_hash, payload, snapshot_metadata_path, HashingAlgorithm,
		SnapshotMetadata, FIXTURE_RETENTION_PERIOD, N_STORES, PAYLOAD_SIZE_MAX, PAYLOAD_SIZE_MIN,
		TIP_SYNC_TARGET_BLOCKS,
	},
};
use anyhow::{anyhow, Context, Result};
use env_logger::Env;
use std::{collections::HashSet, path::PathBuf};
use zombienet_sdk::{
	subxt::{
		config::substrate::{SubstrateConfig, SubstrateExtrinsicParamsBuilder},
		dynamic::Value,
		ext::scale_value::value,
		OnlineClient,
	},
	subxt_signer::sr25519::dev,
	NetworkConfig, NetworkConfigBuilder,
};

const SESSION_CHANGE_TIMEOUT_SECS: u64 = 300;
const DB_OUTPUT_DIR_ENV: &str = "DB_OUTPUT_DIR";
const DEFAULT_DB_OUTPUT_DIR: &str = "./zombienet/test-databases";
const STORE_START_BLOCK: u64 = 50;
const GEN_TIMEOUT_SECS: u64 = 1800;

fn get_db_output_dir() -> PathBuf {
	std::env::var(DB_OUTPUT_DIR_ENV)
		.map(PathBuf::from)
		.unwrap_or_else(|_| DEFAULT_DB_OUTPUT_DIR.into())
}

fn build_gendb_network_config(pruning_blocks: u32) -> Result<NetworkConfig> {
	let relay_args: Vec<_> = vec!["-lruntime=debug"].into_iter().map(Into::into).collect();
	let collator_args: Vec<_> =
		vec!["--ipfs-server", NODE_LOG_CONFIG].into_iter().map(Into::into).collect();
	let pruning_flag = format!("--blocks-pruning={pruning_blocks}");
	let pruned_args: Vec<_> =
		vec!["--sync=full", "--ipfs-server", pruning_flag.as_str(), NODE_LOG_CONFIG]
			.into_iter()
			.map(Into::into)
			.collect();

	NetworkConfigBuilder::new()
		.with_relaychain(|relaychain| {
			relaychain
				.with_chain(RELAY_CHAIN)
				.with_default_command(RELAY_BINARY)
				.with_validator(|node| {
					node.with_name("alice").validator(true).with_args(relay_args.clone())
				})
				.with_validator(|node| node.with_name("bob").validator(true).with_args(relay_args))
		})
		.with_parachain(|parachain| {
			parachain
				.with_id(PARA_ID)
				.with_chain_spec_path(PARACHAIN_CHAIN_SPEC)
				.cumulus_based(true)
				.with_collator(|node| {
					node.with_name("collator-1")
						.validator(true)
						.with_command(PARACHAIN_BINARY)
						.with_args(collator_args)
				})
				.with_collator(|node| {
					node.with_name("pruned-node")
						.validator(false)
						.with_command(PARACHAIN_BINARY)
						.with_args(pruned_args)
				})
		})
		.with_global_settings(|settings| match std::env::var("ZOMBIENET_SDK_BASE_DIR") {
			Ok(base_dir) => settings.with_base_dir(base_dir),
			Err(_) => settings,
		})
		.build()
		.map_err(|errs| {
			let message = errs.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", ");
			anyhow!("config errs: {message}")
		})
}

async fn authorize_account(
	client: &OnlineClient<SubstrateConfig>,
	who: &zombienet_sdk::subxt_signer::sr25519::Keypair,
	transactions: u32,
	bytes: u64,
	nonce: u64,
	label: &str,
) -> Result<()> {
	let signer = dev::alice();
	let call = zombienet_sdk::subxt::tx::dynamic(
		"Sudo",
		"sudo",
		vec![value! {
			TransactionStorage(authorize_account {
				who: Value::from_bytes(who.public_key().0),
				transactions: transactions,
				bytes: bytes
			})
		}],
	);
	let params = SubstrateExtrinsicParamsBuilder::new().nonce(nonce).build();
	tokio::time::timeout(std::time::Duration::from_secs(60), async {
		let progress = client.tx().sign_and_submit_then_watch(&call, &signer, params).await?;
		wait_for_in_best_block(progress).await?;
		Ok::<_, anyhow::Error>(())
	})
	.await
	.map_err(|_| anyhow!("{label} authorization timed out"))??;
	Ok(())
}

async fn store_data(
	client: &OnlineClient<SubstrateConfig>,
	data: &[u8],
	nonce: u64,
	algo: HashingAlgorithm,
) -> Result<u64> {
	let algo_name = match algo {
		HashingAlgorithm::Blake2b256 => "Blake2b256",
		HashingAlgorithm::Sha2_256 => "Sha2_256",
		HashingAlgorithm::Keccak256 => "Keccak256",
	};
	let cid_config = Value::named_composite(vec![
		("codec", Value::u128(0x55)),
		("hashing", Value::unnamed_variant(algo_name, vec![])),
	]);
	let call = zombienet_sdk::subxt::tx::dynamic(
		"TransactionStorage",
		"store_with_cid_config",
		vec![cid_config, Value::from_bytes(data)],
	);
	let params = SubstrateExtrinsicParamsBuilder::new().nonce(nonce).build();
	let (block_hash, _) = tokio::time::timeout(std::time::Duration::from_secs(120), async {
		let progress = client.tx().sign_and_submit_then_watch(&call, &dev::alice(), params).await?;
		wait_for_in_best_block(progress).await
	})
	.await
	.map_err(|_| anyhow!("store transaction timed out (nonce={nonce})"))??;
	Ok(client.blocks().at(block_hash).await?.number() as u64)
}

fn payload_stats() -> Result<u64> {
	let mut hashes_seen = HashSet::new();
	let mut total_payload_bytes = 0;
	for i in 0..N_STORES {
		let bytes = payload(i);
		anyhow::ensure!((PAYLOAD_SIZE_MIN..=PAYLOAD_SIZE_MAX).contains(&bytes.len()));
		let hash = content_hash(i);
		anyhow::ensure!(hash == algorithm(i).hash(&bytes));
		anyhow::ensure!(hashes_seen.insert(hash), "duplicate content_hash at i={i}");
		total_payload_bytes += bytes.len() as u64;
	}
	Ok(total_payload_bytes)
}

#[tokio::test(flavor = "multi_thread")]
async fn parachain_generate_databases() -> Result<()> {
	let _ = env_logger::Builder::from_env(Env::default().default_filter_or("info")).try_init();
	verify_parachain_binaries()?;

	let total_payload_bytes = payload_stats()?;
	let output_dir = get_db_output_dir();
	std::fs::create_dir_all(&output_dir)
		.with_context(|| format!("Failed to create output dir {}", output_dir.display()))?;
	log::info!("Generating storage-chain fixture data into {}", output_dir.display());

	let config = build_gendb_network_config(FIXTURE_RETENTION_PERIOD)?;
	let network = initialize_network(config).await?;
	network.wait_until_is_up(NETWORK_READY_TIMEOUT_SECS).await?;

	let relay_alice = network.get_node("alice")?;
	wait_for_session_change_on_node(relay_alice, SESSION_CHANGE_TIMEOUT_SECS).await?;

	let collator = network.get_node("collator-1")?;
	let client: OnlineClient<SubstrateConfig> = collator.wait_client().await?;
	let authorize_transactions = (N_STORES + 10) as u32;
	let authorize_bytes = total_payload_bytes.saturating_mul(2).saturating_add(1024 * 1024);

	let mut nonce = get_alice_nonce(collator).await?;
	authorize_account(
		&client,
		&dev::alice(),
		authorize_transactions,
		authorize_bytes,
		nonce,
		"alice",
	)
	.await?;
	nonce += 1;
	authorize_account(&client, &dev::bob(), authorize_transactions, authorize_bytes, nonce, "bob")
		.await?;
	nonce += 1;

	wait_for_block_height(collator, STORE_START_BLOCK, GEN_TIMEOUT_SECS).await?;

	let mut first_store_block = u64::MAX;
	let mut last_store_block = 0;
	for i in 0..N_STORES {
		let data = payload(i);
		let included = store_data(&client, &data, nonce, algorithm(i)).await?;
		nonce += 1;
		first_store_block = first_store_block.min(included);
		last_store_block = last_store_block.max(included);
		log::info!("Store {}/{} included at block {}", i + 1, N_STORES, included);
	}

	anyhow::ensure!(
		last_store_block <= TIP_SYNC_TARGET_BLOCKS,
		"last store landed at block {last_store_block}, beyond target {TIP_SYNC_TARGET_BLOCKS}",
	);

	let finalize_target = TIP_SYNC_TARGET_BLOCKS.max(last_store_block);
	wait_for_finalized_height(collator, finalize_target, GEN_TIMEOUT_SECS).await?;
	let pruned_node = network.get_node("pruned-node")?;
	wait_for_block_height(pruned_node, finalize_target, SYNC_TIMEOUT_SECS).await?;
	wait_for_finalized_height(pruned_node, finalize_target, GEN_TIMEOUT_SECS).await?;

	let base_dir = network
		.base_dir()
		.ok_or_else(|| anyhow!("Failed to get network base directory"))?
		.to_string();
	let collator_base = PathBuf::from(&base_dir).join("collator-1");
	let relay_alice_base = PathBuf::from(&base_dir).join("alice");

	std::fs::copy(
		collator_base.join("cfg").join(format!("{}.json", PARA_ID)),
		output_dir.join("raw-chain-spec.json"),
	)
	.context("Failed to copy raw parachain chain spec")?;
	std::fs::copy(
		relay_alice_base.join("cfg").join("westend-local.json"),
		output_dir.join("raw-relay-chain-spec.json"),
	)
	.context("Failed to copy raw relay chain spec")?;

	let metadata = SnapshotMetadata {
		total_blocks: TIP_SYNC_TARGET_BLOCKS,
		retention_period: FIXTURE_RETENTION_PERIOD,
		n_stores: N_STORES,
		payload_size_min: PAYLOAD_SIZE_MIN,
		payload_size_max: PAYLOAD_SIZE_MAX,
		snapshot_height: finalize_target,
		first_store_block,
		last_store_block,
	};
	let metadata_path = snapshot_metadata_path(&output_dir);
	let metadata_file = std::fs::File::create(&metadata_path)
		.with_context(|| format!("Failed to create {}", metadata_path.display()))?;
	serde_json::to_writer_pretty(metadata_file, &metadata)
		.with_context(|| format!("Failed to write {}", metadata_path.display()))?;

	log::info!("Database generation complete. Base dir: {base_dir}. Archive with generate-snapshots.sh snapshots-archive.");
	Ok(())
}
