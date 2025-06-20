// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test systematic availability chunk recovery is used if the chunk mapping feature is enabled.

use anyhow::anyhow;

use cumulus_zombienet_sdk_helpers::{
	assert_blocks_are_being_finalized, assert_finality_lag, assert_finalized_para_throughput,
	wait_for_nth_session_change,
};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use std::time::Duration;
use subxt::{self, ext::scale_value::value, OnlineClient, PolkadotConfig};
use subxt_signer::sr25519::dev;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{NetworkConfigBuilder, NetworkNode};

#[tokio::test(flavor = "multi_thread")]
async fn systematic_chunk_recovery_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug,parachain::availability-recovery=trace,parachain::availability-distribution=trace").into()])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"max_validators_per_core": 2,
							},
						}
					}
				}))
				.with_node(|node| node.with_name("validator-0"));

			(1..5).fold(r, |acc, i| acc.with_node(|node| node.with_name(&format!("validator-{i}"))))
		})
		.with_parachain(|p| {
			p.with_id(2000)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("glutton-westend-local-2000")
				.with_default_args(vec![("-lparachain=debug,aura=debug").into()])
                // We need to configure the glutton so that the PoV is larger than 4Mib (the FETCH_CHUNKS_THRESHOLD)
                .with_genesis_overrides(json!({
					"glutton": {
						"compute": "50000000",
                        "storage": "2500000000",
                        "trashDataCount": 5120
					}
				}))
				.with_collator(|n| n.with_name("collator-2000"))
		})
        .with_parachain(|p| {
			p.with_id(2001)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("glutton-westend-local-2001")
				.with_default_args(vec![("-lparachain=debug,aura=debug").into()])
                // We need to configure the glutton so that the PoV is larger than 4Mib (the FETCH_CHUNKS_THRESHOLD)
                .with_genesis_overrides(json!({
					"glutton": {
						"compute": "50000000",
                        "storage": "2500000000",
                        "trashDataCount": 5120
					}
				}))
				.with_collator(|n| n.with_name("collator-2001"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let para_node_2000 = network.get_node("collator-2000")?;
	let para_node_2001 = network.get_node("collator-2001")?;

	let para_client_2000 = para_node_2000.wait_client().await?;
	let para_client_2001 = para_node_2001.wait_client().await?;

	let alice = dev::alice();

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	assert_finalized_para_throughput(
		&relay_client,
		15,
		[(ParaId::from(2000), 11..16), (ParaId::from(2001), 11..16)]
			.into_iter()
			.collect(),
	)
	.await?;

	assert_blocks_are_being_finalized(&relay_client).await?;

	// Assert the parachain finalized block height is progressing
	assert_finality_lag(&para_client_2000, 5).await?;
	assert_finality_lag(&para_client_2001, 5).await?;

	assert!(relay_node
		.wait_log_line_count_with_timeout(
			"Data recovery from chunks complete",
			false,
			LogLineCountOptions::new(|n| n >= 8, Duration::from_secs(1), false),
		)
		.await?
		.success());

	assert!(relay_node
		.wait_log_line_count_with_timeout(
			"Data recovery from systematic chunks complete",
			false,
			LogLineCountOptions::new(|n| n == 0, Duration::from_secs(1), false),
		)
		.await?
		.success());

	assert_no_failed_recoveries(&relay_node).await?;

	relay_client
		.tx()
		.sign_and_submit_then_watch_default(
			&subxt::tx::dynamic(
				"Sudo",
				"sudo",
				vec![value! {
					Configuration(set_node_feature { index: 2, value: true })
				}],
			),
			&alice,
		)
		.await?
		.wait_for_finalized_success()
		.await?;

	log::info!("Submitted transaction for enabling the node feature, waiting for 2 session changes for the new config to come into effect");

	// Wait just for 1 session change, since assert_finalized_para_throughput waits for one more.
	wait_for_nth_session_change(&mut relay_client.blocks().subscribe_finalized().await?, 1).await?;

	assert_finalized_para_throughput(
		&relay_client,
		15,
		[(ParaId::from(2000), 11..16), (ParaId::from(2001), 11..16)]
			.into_iter()
			.collect(),
	)
	.await?;

	assert_blocks_are_being_finalized(&relay_client).await?;

	// Assert the parachain finalized block height is progressing
	assert_finality_lag(&para_client_2000, 5).await?;
	assert_finality_lag(&para_client_2001, 5).await?;

	assert!(relay_node
		.wait_log_line_count_with_timeout(
			"Data recovery from systematic chunks complete",
			false,
			LogLineCountOptions::new(|n| n >= 8, Duration::from_secs(1), false),
		)
		.await?
		.success());

	assert_no_failed_recoveries(&relay_node).await?;

	log::info!("Test finished successfully");

	Ok(())
}

async fn assert_no_failed_recoveries(node: &NetworkNode) -> Result<(), anyhow::Error> {
	assert!(node
		.wait_log_line_count_with_timeout(
			"Data recovery from systematic chunks is not possible",
			false,
			LogLineCountOptions::new(|n| n == 0, Duration::from_secs(1), false),
		)
		.await?
		.success());

	assert!(node
		.wait_log_line_count_with_timeout(
			"Data recovery from chunks is not possible",
			false,
			LogLineCountOptions::new(|n| n == 0, Duration::from_secs(1), false),
		)
		.await?
		.success());

	node.wait_metric_with_timeout(
		"polkadot_parachain_availability_recovery_recoveries_finished{result=\"failure\"}",
		|n| n < 1.0,
		1u64,
	)
	.await?;

	Ok(())
}
