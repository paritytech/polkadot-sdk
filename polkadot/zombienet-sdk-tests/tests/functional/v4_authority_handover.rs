// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Authority handover across an aura set change on a V4 parachain: with backing paused across a
//! sudo rotation of the set, pre-rotation blocks must be resubmitted by the prior set's author.

use anyhow::anyhow;
use codec::{Decode, Encode};
use cumulus_zombienet_sdk_helpers::assign_cores;
use serde_json::json;
use sp_crypto_hashing::twox_128;
use std::time::{Duration, Instant};
use zombienet_sdk::{
	subxt::{
		self,
		config::polkadot::PolkadotExtrinsicParamsBuilder,
		dynamic::Value,
		ext::{scale_value::value, subxt_rpcs::client::rpc_params},
		tx::TxStatus,
		OnlineClient, PolkadotConfig,
	},
	subxt_signer::sr25519::dev,
	NetworkConfigBuilder,
};

const PARA_ID: u32 = 2900;
const NUM_COLLATORS: usize = 3;
const NUM_VALIDATORS: usize = 6;
const MIN_START_HEIGHT: u32 = 5;
const RELAY_ADVANCE: u32 = 3;
const FINALIZED_PAST_ROTATION: u32 = 10;

#[tokio::test(flavor = "multi_thread")]
async fn v4_authority_handover() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();
	let collator_names: Vec<String> =
		(0..NUM_COLLATORS).map(|i| format!("collator-2900-{i}")).collect();

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![
					("-lparachain=debug,runtime=debug").into(),
					"--experimental-collator-protocol".into(),
				])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"num_cores": 2,
								"max_validators_per_core": 2,
								"group_rotation_frequency": 4,
								"lookahead": 5
							},
							// V2 (bit 4) and V3 (bit 3) descriptors enabled.
							"node_features": {"bits": 8, "data": [0b00011000]},
							"max_relay_parent_session_age": 10,
						}
					}
				}))
				.with_validator(|node| node.with_name("validator-0"));

			(1..NUM_VALIDATORS).fold(r, |acc, i| {
				acc.with_validator(|node| node.with_name(&format!("validator-{i}")))
			})
		})
		.with_parachain(|p| {
			let p = p
				.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("elastic-scaling-v3")
				.with_default_args(vec![
					("-lparachain=debug,aura=debug,aura::cumulus=debug,aura::resubmission=trace")
						.into(),
					"--authoring=slot-based".into(),
					"--".into(),
					"--state-pruning=archive".into(),
					"--blocks-pruning=archive".into(),
					"--network-backend=libp2p".into(),
				])
				.with_collator(|n| n.with_name(collator_names[0].as_str()));
			collator_names
				.iter()
				.skip(1)
				.fold(p, |acc, name| acc.with_collator(|n| n.with_name(name.as_str())))
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
	let collator_node = network.get_node(collator_names[0].as_str())?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;
	let para_client: OnlineClient<PolkadotConfig> = collator_node.wait_client().await?;

	assign_cores(&relay_client, PARA_ID, vec![0, 1]).await?;
	log::info!("Para {PARA_ID} elastic-scaled to 2 cores (V3 descriptors, resubmission on)");

	// Sanity: the para is producing blocks before the validators are paused.
	wait_for_para_best(&para_client, MIN_START_HEIGHT, Duration::from_secs(300)).await?;
	log::info!("Para is producing blocks, pausing validator-1..{}", NUM_VALIDATORS - 1);

	// A lone validator never reaches the 2 backing votes but still authors relay blocks.
	let paused: Vec<_> = (1..NUM_VALIDATORS)
		.map(|i| network.get_node(format!("validator-{i}")))
		.collect::<Result<_, _>>()?;
	for node in &paused {
		node.pause().await?;
	}
	log::info!("Validators paused, rotating the aura authority set");

	// Rotate `pallet_aura::Authorities` left by one with sudo. The flip lands in the block's
	// post-state like a session rotation would, aura-ext mirrors it on finalize, and with three
	// distinct keys every slot's expected author changes.
	let authorities_key =
		[twox_128(b"Aura").as_slice(), twox_128(b"Authorities").as_slice()].concat();
	let raw = para_client
		.storage()
		.at_latest()
		.await?
		.fetch_raw(authorities_key.clone())
		.await?
		.ok_or_else(|| anyhow!("aura authorities not found in storage"))?;
	let mut authorities: Vec<[u8; 32]> = Decode::decode(&mut &raw[..])?;
	assert_eq!(authorities.len(), NUM_COLLATORS, "one genesis authority per collator expected");
	authorities.rotate_left(1);

	let items = vec![Value::unnamed_composite([
		Value::from_bytes(authorities_key),
		Value::from_bytes(authorities.encode()),
	])];
	let rotate_call = subxt::tx::dynamic(
		"Sudo",
		"sudo",
		vec![value! {
			System(set_storage { items: items })
		}],
	);
	let rotation_hash = tokio::time::timeout(Duration::from_secs(120), async {
		let extensions = PolkadotExtrinsicParamsBuilder::new().immortal().build();
		let tx = para_client.tx().create_signed(&rotate_call, &dev::alice(), extensions).await?;
		let mut progress = tx.submit_and_watch().await?;
		while let Some(status) = progress.next().await.transpose()? {
			match status {
				TxStatus::InBestBlock(in_block) => {
					in_block.wait_for_success().await?;
					return Ok(in_block.block_hash());
				},
				TxStatus::Error { message } |
				TxStatus::Invalid { message } |
				TxStatus::Dropped { message } => return Err(anyhow!("rotation tx: {message}")),
				_ => continue,
			}
		}
		Err(anyhow!("rotation tx stream ended before inclusion"))
	})
	.await
	.map_err(|_| anyhow!("timeout waiting for the rotation to be in a best block"))??;
	let rotation_number = para_client.blocks().at(rotation_hash).await?.number();
	// Best, not finalized: grandpa stalls while validators are paused.
	let relay_rpc = relay_node.rpc().await?;
	let relay_at_rotation = best_number(&relay_rpc).await?;
	log::info!("Rotation in para block #{rotation_number} {rotation_hash:?}");

	// Original submissions must be out of the allowed relay-parent window (genesis ancestry is
	// small), so only a resubmission can still back the rotation block.
	let target = relay_at_rotation + RELAY_ADVANCE;
	let start = Instant::now();
	loop {
		let best = best_number(&relay_rpc).await?;
		if best >= target {
			log::info!("Relay advanced to #{best} (rotation seen at #{relay_at_rotation})");
			break;
		}
		if start.elapsed() > Duration::from_secs(360) {
			return Err(anyhow!("relay stuck at #{best}, wanted #{target} while paused"));
		}
		tokio::time::sleep(Duration::from_secs(5)).await;
	}
	for node in &paused {
		node.resume().await?;
	}
	log::info!("Validators resumed, waiting for the rotation block to finalize");

	// Poll, never block on a stream: without the handover the para stalls and must time out.
	let rpc = collator_node.rpc().await?;
	let want = rotation_number + FINALIZED_PAST_ROTATION;
	let start = Instant::now();
	loop {
		let fin_hash: subxt::utils::H256 =
			rpc.request("chain_getFinalizedHead", rpc_params![]).await?;
		let fin_number = para_client.blocks().at(fin_hash).await?.number();
		let mut at_rotation = None;
		if fin_number >= want {
			let h: subxt::utils::H256 =
				rpc.request("chain_getBlockHash", rpc_params![rotation_number]).await?;
			if h == rotation_hash {
				log::info!("Rotation block #{rotation_number} finalized, head #{fin_number}");
				break;
			}
			at_rotation = Some(h);
		}
		if start.elapsed() > Duration::from_secs(300) {
			return Err(anyhow!(
				"rotation block #{rotation_number} {rotation_hash:?} not finalized with \
				 {FINALIZED_PAST_ROTATION} blocks on top: finalized head #{fin_number}, \
				 hash at rotation {at_rotation:?}"
			));
		}
		tokio::time::sleep(Duration::from_secs(5)).await;
	}

	log::info!("V4 authority handover test finished");
	Ok(())
}

async fn best_number(rpc: &subxt::backend::rpc::RpcClient) -> Result<u32, anyhow::Error> {
	let header: serde_json::Value = rpc.request("chain_getHeader", rpc_params![]).await?;
	let number = header["number"].as_str().ok_or_else(|| anyhow!("header without number"))?;
	Ok(u32::from_str_radix(number.trim_start_matches("0x"), 16)?)
}

async fn wait_for_para_best(
	client: &OnlineClient<PolkadotConfig>,
	height: u32,
	timeout: Duration,
) -> Result<(), anyhow::Error> {
	let start = Instant::now();
	loop {
		if client.blocks().at_latest().await?.number() >= height {
			return Ok(());
		}
		if start.elapsed() > timeout {
			return Err(anyhow!("para did not reach height {height} within {timeout:?}"));
		}
		tokio::time::sleep(Duration::from_secs(3)).await;
	}
}
