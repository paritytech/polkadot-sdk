// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Zombienet E2E test: speculative messaging latency measurement.

use anyhow::anyhow;
use std::time::{Duration, Instant};

use crate::utils::{initialize_network, BEST_BLOCK_METRIC};

use cumulus_zombienet_sdk_helpers::{
	assert_para_throughput, assign_cores, submit_extrinsic_and_wait_for_finalization_success,
};
use serde_json::json;
use zombienet_orchestrator::network::node::NetworkNode;
use zombienet_sdk::{
	subxt::{
		self, config::polkadot::PolkadotExtrinsicParamsBuilder, dynamic::Value,
		ext::scale_value::value, OnlineClient, PolkadotConfig,
	},
	NetworkConfig, NetworkConfigBuilder,
};

const PARA_A: u32 = 2000;
const PARA_B: u32 = 2100;

#[tokio::test(flavor = "multi_thread")]
async fn speculative_messaging_e2e() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let config = build_network_config().await?;
	let network = initialize_network(config).await?;

	let alice = network.get_node("alice")?;
	let collator_a = network.get_node("collator-a")?;
	let collator_b = network.get_node("collator-b")?;
	assert!(alice.wait_until_is_up(60u64).await.is_ok());
	assert!(collator_a.wait_until_is_up(60u64).await.is_ok());
	assert!(collator_b.wait_until_is_up(60u64).await.is_ok());

	let relay_client: OnlineClient<PolkadotConfig> = alice.wait_client().await?;

	// Assign cores — same pattern as elastic_scaling test
	log::info!("Assigning cores");
	assign_cores(&relay_client, PARA_B, vec![0, 1]).await?;

	// Wait for both paras producing blocks (225s like elastic_scaling)
	log::info!("Waiting for block production");
	for (node, cnt) in [(collator_a, 4.0), (collator_b, 4.0)] {
		node.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= cnt, 225u64)
			.await
			.unwrap_or_else(|e| panic!("{}: {e}", node.name()));
	}

	let collator_a = network.get_node("collator-a")?;
	let collator_b = network.get_node("collator-b")?;
	let para_a_client: OnlineClient<PolkadotConfig> = collator_a.wait_client().await?;
	let para_b_client: OnlineClient<PolkadotConfig> = collator_b.wait_client().await?;

	// Register relay peers (fire-and-forget)
	log::info!("Registering relay peers");
	let collator_a_relay_peer = extract_relay_peer_id(collator_a).await?;
	let collator_b_relay_peer = extract_relay_peer_id(collator_b).await?;
	let alice_signer = zombienet_sdk::subxt_signer::sr25519::dev::alice();
	submit_sudo_no_wait(&para_a_client, &alice_signer, PARA_B, &collator_b_relay_peer).await?;
	submit_sudo_no_wait(&para_b_client, &alice_signer, PARA_A, &collator_a_relay_peer).await?;

	// Wait a couple blocks for inclusion
	let collator_a = network.get_node("collator-a")?;
	collator_a
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 8.0, 60u64)
		.await
		.unwrap_or_else(|e| panic!("collator-a: {e}"));

	// Send message and measure
	log::info!("═══ SENDING SPECULATIVE MESSAGE ═══");
	let t_send = Instant::now();
	let bob_signer = zombienet_sdk::subxt_signer::sr25519::dev::bob();
	let send_call = subxt::tx::dynamic(
		"SpeculativeMessaging",
		"send_message_extrinsic",
		vec![Value::u128(PARA_B as u128), Value::from_bytes(b"hello-spec-msg".to_vec())],
	);
	submit_extrinsic_and_wait_for_finalization_success(&para_a_client, &send_call, &bob_signer)
		.await?;
	let t_sent = Instant::now();
	log::info!(
		"[TIMING] ParaA finalized send: {:.1}s",
		t_sent.duration_since(t_send).as_secs_f64()
	);

	// Check MessageSent on ParaA
	assert!(
		check_for_event(&para_a_client, "SpeculativeMessaging", "MessageSent").await,
		"ParaA must emit MessageSent"
	);
	log::info!("[CHECK] MessageSent confirmed on ParaA");

	// Poll ParaB best blocks for MessagesReceived
	log::info!("Polling ParaB for MessagesReceived...");
	let mut sub = para_b_client.blocks().subscribe_best().await?;
	let mut t_received: Option<Instant> = None;
	let deadline = Instant::now() + Duration::from_secs(60);
	while Instant::now() < deadline {
		let block = tokio::time::timeout(Duration::from_secs(12), async {
			use futures::StreamExt;
			sub.next().await
		})
		.await;
		let block = match block {
			Ok(Some(Ok(b))) => b,
			_ => continue,
		};
		if let Ok(events) = block.events().await {
			for event in events.iter().flatten() {
				if event.pallet_name() == "SpeculativeMessaging"
					&& event.variant_name() == "MessagesReceived"
				{
					t_received = Some(Instant::now());
					log::info!("[CHECK] MessagesReceived on ParaB block #{}", block.number());
				}
			}
		}
		if t_received.is_some() {
			break;
		}
	}

	// Report
	log::info!("═══════════════════════════════════════════════════════");
	log::info!("  SPECULATIVE MESSAGING LATENCY REPORT");
	log::info!("═══════════════════════════════════════════════════════");
	if let Some(t_recv) = t_received {
		let delivery = t_recv.duration_since(t_sent).as_secs_f64();
		log::info!("  Delivery (finalized→received): {delivery:.1}s");
		log::info!("  HRMP baseline:                 ~12-18s");
		log::info!("═══════════════════════════════════════════════════════");
		assert!(delivery < 30.0, "Delivery took {delivery:.1}s, expected < 30s");
	} else {
		log::warn!("  MessagesReceived NOT detected on ParaB");
		log::warn!("═══════════════════════════════════════════════════════");
	}

	// Health check
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
	logs.lines()
		.filter_map(|l| {
			l.split("Local node identity is: ").nth(1).map(|s| s.trim().to_string())
		})
		.nth(0) // first = relay chain peer (relay network is built before parachain network)
		.ok_or_else(|| anyhow!("No relay peer ID in logs of {}", node.name()))
}

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
	let ext = PolkadotExtrinsicParamsBuilder::new().immortal().build();
	client.tx().create_signed(&call, signer, ext).await?.submit().await?;
	log::info!("Submitted set_relay_peer for para {dest_para_id}");
	Ok(())
}

async fn check_for_event(
	client: &OnlineClient<PolkadotConfig>,
	pallet: &str,
	variant: &str,
) -> bool {
	let block = match client.blocks().at_latest().await {
		Ok(b) => b,
		Err(_) => return false,
	};
	let events = match block.events().await {
		Ok(e) => e,
		Err(_) => return false,
	};
	events
		.iter()
		.flatten()
		.any(|e| e.pallet_name() == pallet && e.variant_name() == variant)
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();

	// Mirrors elastic_scaling/slot_based_authoring.rs pattern:
	// 6 validators, genesis overrides for num_cores, assign_cores for PARA_B
	NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"num_cores": 4,
								"max_validators_per_core": 1
							},
							"approval_voting_params": {
								"max_approval_coalesce_count": 5
							}
						}
					}
				}))
				.with_validator(|node| node.with_name("alice").with_args(vec![]));
			(0..5).fold(r, |acc, i| {
				acc.with_validator(|node| {
					node.with_name(&format!("validator-{i}"))
						.with_args(vec![("-lparachain=debug").into()])
				})
			})
		})
		// ParaA (2000): default chain spec — auto-assigned core by rococo-local
		.with_parachain(|p| {
			p.with_id(PARA_A)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_collator(|n| {
					n.with_name("collator-a").with_args(vec![
						("-lruntime=debug,parachain=debug,cumulus-test-service=debug")
							.into(),
						("--force-authoring").into(),
						("--authoring", "slot-based").into(),
					])
				})
		})
		// ParaB (2100): speculative-messaging chain spec, cores assigned via assign_cores
		.with_parachain(|p| {
			p.with_id(PARA_B)
				.with_chain("speculative-messaging")
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_collator(|n| {
					n.with_name("collator-b").with_args(vec![
						("-lruntime=debug,parachain=debug,cumulus-test-service=debug")
							.into(),
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
