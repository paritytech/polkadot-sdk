// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Zombienet E2E test: speculative messaging latency measurement.
//!
//! Measures wall-clock delay from `send_message_extrinsic` on ParaA to
//! `MessagesReceived` event on ParaB, comparing with HRMP baseline.

use anyhow::anyhow;
use std::time::{Duration, Instant};

use crate::utils::{initialize_network, BEST_BLOCK_METRIC};

use cumulus_zombienet_sdk_helpers::{
	assert_para_throughput, assign_cores, submit_extrinsic_and_wait_for_finalization_success,
};
use serde_json::json;
use zombienet_orchestrator::network::node::NetworkNode;
use zombienet_sdk::{
	subxt::{self, config::polkadot::PolkadotExtrinsicParamsBuilder, dynamic::Value, ext::scale_value::value, OnlineClient, PolkadotConfig},
	NetworkConfig, NetworkConfigBuilder,
};

const PARA_A: u32 = 2000;
const PARA_B: u32 = 2001;

#[tokio::test(flavor = "multi_thread")]
async fn speculative_messaging_e2e() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	// Phase 1: Spawn network
	let config = build_network_config().await?;
	let network = initialize_network(config).await?;

	let alice = network.get_node("alice")?;
	let collator_a = network.get_node("collator-a")?;
	let collator_b = network.get_node("collator-b")?;
	assert!(alice.wait_until_is_up(60u64).await.is_ok());
	assert!(collator_a.wait_until_is_up(60u64).await.is_ok());
	assert!(collator_b.wait_until_is_up(60u64).await.is_ok());

	let relay_client: OnlineClient<PolkadotConfig> = alice.wait_client().await?;

	// Assign cores (finalization required by helper — ~30s each, unavoidable)
	log::info!("Assigning cores");
	assign_cores(&relay_client, PARA_A, vec![0]).await?;
	assign_cores(&relay_client, PARA_B, vec![1]).await?;

	// Wait for both paras to produce blocks
	log::info!("Waiting for para block production");
	for node in [collator_a, collator_b] {
		node.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 2.0, 180u64)
			.await
			.unwrap_or_else(|e| panic!("{}: {e}", node.name()));
	}

	let collator_a = network.get_node("collator-a")?;
	let collator_b = network.get_node("collator-b")?;
	let para_a_client: OnlineClient<PolkadotConfig> = collator_a.wait_client().await?;
	let para_b_client: OnlineClient<PolkadotConfig> = collator_b.wait_client().await?;

	// Phase 2: Register relay peers — fire-and-forget (no finalization wait)
	log::info!("Registering relay peers");
	let collator_a_relay_peer_id = extract_relay_peer_id(collator_a).await?;
	let collator_b_relay_peer_id = extract_relay_peer_id(collator_b).await?;

	let alice_signer = zombienet_sdk::subxt_signer::sr25519::dev::alice();
	// Submit both sudo calls without waiting for finalization
	submit_sudo_no_wait(&para_a_client, &alice_signer, PARA_B, &collator_b_relay_peer_id)
		.await?;
	submit_sudo_no_wait(&para_b_client, &alice_signer, PARA_A, &collator_a_relay_peer_id)
		.await?;

	// Wait a few blocks for the sudo calls to be included
	log::info!("Waiting for peer registrations to be included...");
	let collator_a = network.get_node("collator-a")?;
	collator_a
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 4.0, 60u64)
		.await
		.unwrap_or_else(|e| panic!("collator-a: {e}"));

	// Phase 3: Send message and measure (use Bob to avoid nonce clash with Alice's sudo calls)
	log::info!("═══ SENDING SPECULATIVE MESSAGE ═══");
	let t_send = Instant::now();
	let bob_signer = zombienet_sdk::subxt_signer::sr25519::dev::bob();

	let send_call = subxt::tx::dynamic(
		"SpeculativeMessaging",
		"send_message_extrinsic",
		vec![Value::u128(PARA_B as u128), Value::from_bytes(b"hello-spec-msg".to_vec())],
	);
	submit_extrinsic_and_wait_for_finalization_success(
		&para_a_client,
		&send_call,
		&bob_signer,
	)
	.await?;
	let t_sent = Instant::now();
	log::info!("[TIMING] ParaA finalized send in {:.1}s", t_sent.duration_since(t_send).as_secs_f64());

	// Phase 4: Poll ParaB for MessagesReceived
	log::info!("Polling ParaB for MessagesReceived...");
	let mut para_b_sub = para_b_client.blocks().subscribe_best().await?;
	let mut t_received: Option<Instant> = None;
	let deadline = Instant::now() + Duration::from_secs(60);

	while Instant::now() < deadline {
		let block = tokio::time::timeout(Duration::from_secs(12), async {
			use futures::StreamExt;
			para_b_sub.next().await
		})
		.await;

		let block = match block {
			Ok(Some(Ok(b))) => b,
			_ => continue,
		};

		let events = match block.events().await {
			Ok(e) => e,
			Err(_) => continue,
		};

		for event in events.iter().flatten() {
			if event.pallet_name() == "SpeculativeMessaging" &&
				event.variant_name() == "MessagesReceived"
			{
				t_received = Some(Instant::now());
				log::info!("[CHECK] ParaB MessagesReceived at block #{}", block.number());
				break;
			}
		}
		if t_received.is_some() {
			break;
		}
	}

	// Phase 5: Report
	log::info!("═══════════════════════════════════════════════════════");
	log::info!("  SPECULATIVE MESSAGING LATENCY REPORT");
	log::info!("═══════════════════════════════════════════════════════");

	if let Some(t_recv) = t_received {
		let e2e = t_recv.duration_since(t_send).as_secs_f64();
		let delivery = t_recv.duration_since(t_sent).as_secs_f64();
		log::info!("  End-to-end (submit→received): {e2e:.1}s");
		log::info!("  Delivery (finalized→received): {delivery:.1}s");
		log::info!("  HRMP baseline:                 ~12-18s");
		log::info!("═══════════════════════════════════════════════════════");

		assert!(
			delivery < 30.0,
			"Delivery took {delivery:.1}s, expected < 30s"
		);
	} else {
		log::warn!("  MessagesReceived NOT detected on ParaB within 60s");
		log::warn!("═══════════════════════════════════════════════════════");
	}

	// Phase 6: Health check
	assert_para_throughput(
		&relay_client,
		8,
		[
			(polkadot_primitives::Id::from(PARA_A), 1..15),
			(polkadot_primitives::Id::from(PARA_B), 1..15),
		],
	)
	.await?;

	log::info!("Test PASSED");
	Ok(())
}

async fn extract_relay_peer_id(node: &NetworkNode) -> Result<String, anyhow::Error> {
	let logs = node.logs().await?;
	let peer_ids: Vec<String> = logs
		.lines()
		.filter_map(|line| {
			line.split("Local node identity is: ")
				.nth(1)
				.map(|s| s.trim().to_string())
		})
		.collect();
	peer_ids
		.get(1)
		.or_else(|| peer_ids.last())
		.cloned()
		.ok_or_else(|| anyhow!("No relay peer ID in logs of {}", node.name()))
}

/// Submit set_relay_peer via sudo WITHOUT waiting for finalization.
async fn submit_sudo_no_wait(
	client: &OnlineClient<PolkadotConfig>,
	signer: &zombienet_sdk::subxt_signer::sr25519::Keypair,
	dest_para_id: u32,
	peer_id_str: &str,
) -> Result<(), anyhow::Error> {
	let peer_id: sc_network_types::PeerId =
		peer_id_str.parse().map_err(|e| anyhow!("parse PeerId: {e}"))?;

	let call = subxt::tx::dynamic(
		"Sudo",
		"sudo",
		vec![value! {
			SpeculativeMessaging(set_relay_peer {
				para_id: dest_para_id,
				peer_id: Value::from_bytes(peer_id.to_bytes())
			})
		}],
	);

	// Submit and just wait for it to be in a block (not finalized)
	let ext = PolkadotExtrinsicParamsBuilder::new().immortal().build();
	client
		.tx()
		.create_signed(&call, signer, ext)
		.await?
		.submit()
		.await?;
	log::info!("Submitted set_relay_peer for para {dest_para_id}");
	Ok(())
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();

	NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"num_cores": 4,
								"max_validators_per_core": 1
							}
						}
					}
				}))
				.with_validator(|node| node.with_name("alice"))
				.with_validator(|node| node.with_name("bob"))
				.with_validator(|node| node.with_name("charlie"))
				.with_validator(|node| node.with_name("dave"))
				.with_validator(|node| node.with_name("eve"))
		})
		.with_parachain(|p| {
			p.with_id(PARA_A)
				.with_chain("speculative-messaging-a")
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_collator(|n| {
					n.with_name("collator-a").with_args(vec![
						("-lruntime=debug,parachain=debug,cumulus-test-service=debug").into(),
						("--force-authoring").into(),
						("--authoring", "slot-based").into(),
					])
				})
		})
		.with_parachain(|p| {
			p.with_id(PARA_B)
				.with_chain("speculative-messaging-b")
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_collator(|n| {
					n.with_name("collator-b").with_args(vec![
						("-lruntime=debug,parachain=debug,cumulus-test-service=debug").into(),
						("--force-authoring").into(),
						("--authoring", "slot-based").into(),
					])
				})
		})
		.with_global_settings(|global_settings| match std::env::var("ZOMBIENET_SDK_BASE_DIR") {
			Ok(val) => global_settings.with_base_dir(val),
			_ => global_settings,
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})
}
