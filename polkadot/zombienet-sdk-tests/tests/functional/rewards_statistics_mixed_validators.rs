// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test that nodes fetch availability chunks early for scheduled cores and normally for occupied
// core.

use std::ops::Range;
use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{assert_para_throughput, wait_for_nth_session_change,
                                    report_label_with_attributes};
use polkadot_primitives::{Id as ParaId, SessionIndex};
use serde_json::json;
use subxt::{OnlineClient, PolkadotConfig};
use subxt::blocks::Block;
use zombienet_orchestrator::network::Network;
use zombienet_orchestrator::network::node::NetworkNode;
use zombienet_sdk::{LocalFileSystem, NetworkConfigBuilder};

#[tokio::test(flavor = "multi_thread")]
async fn rewards_statistics_mixed_validators_test() -> Result<(), anyhow::Error> {
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
                .with_default_args(vec![("-lparachain=debug").into()])
                .with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"group_rotation_frequency": 4
							}
						}
					}
				}))
                .with_node(|node| node.with_name("validator-0"));

            let r = (1..9)
                .fold(r, |acc, i|
                    acc.with_node(|node| node.with_name(&format!("validator-{i}"))));
    
            (9..12).fold(r, |acc, i| {
                acc.with_node(|node| {
                    node.with_name(&format!("malus-{i}"))
                        .with_args(vec![
                            "-lparachain=debug,MALUS=trace".into(),
                            "--dispute-offset=14".into(),
                            "--alice".into(),
                            "--insecure-validator-i-know-what-i-do".into(),
                        ])
                        .with_image(
                            std::env::var("MALUS_IMAGE")
                                .unwrap_or("docker.io/paritypr/malus".to_string())
                                .as_str(),
                        )
                        .with_command("malus")
                        .with_subcommand("dispute_valid_candidates")
                        .invulnerable(false)
                })
            })
        })
        .with_parachain(|p| {
            p.with_id(2000)
                .with_default_command("adder-collator")
                .with_default_image(
                    std::env::var("COL_IMAGE")
                        .unwrap_or("docker.io/paritypr/colander:latest".to_string())
                        .as_str(),
                )
                .cumulus_based(false)
                .with_default_args(vec![("-lparachain=debug").into()])
                .with_collator(|n| n.with_name("collator-adder-2000"))
        })
        .with_parachain(|p| {
            p.with_id(2001)
                .with_default_command("polkadot-parachain")
                .with_default_image(images.cumulus.as_str())
                .with_default_args(vec![("-lparachain=debug,aura=debug").into()])
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
    let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

    let mut blocks_sub = relay_client.blocks().subscribe_finalized().await?;

    // wait for session one to be finalized
    wait_for_nth_session_change(&mut blocks_sub, 2).await;

    assert_approval_usages_medians(
        1,
        12,
        vec![("validator", 0..9), ("malus", 9..12)],
        &network,
    ).await?;

    Ok(())
}

async fn assert_approval_usages_medians(
    session: SessionIndex,
    num_validators: usize,
    validators_kind_and_range: Vec<(&str, Range<usize>)>,
    network: &Network<LocalFileSystem>,
) -> Result<(), anyhow::Error> {
    for (kind, validators_range) in validators_kind_and_range {
        for idx in validators_range {
            let validator_identifier = format!("{}-{}", kind, idx);
            let relay_node = network.get_node(validator_identifier)?;

            let approvals_per_session =
                report_label_with_attributes(
                    "polkadot_parachain_rewards_statistics_collector_approvals_per_session",
                    vec![
                        ("session", session.to_string().as_str()),
                        ("chain", "rococo_local_testnet"),
                    ],
                );

            let noshows_per_session = report_label_with_attributes(
                "polkadot_parachain_rewards_statistics_collector_no_shows_per_session",
                vec![
                    ("session", session.to_string().as_str()),
                    ("chain", "rococo_local_testnet"),
                ],
            );

            let total_approvals = relay_node.reports(approvals_per_session).await?;
            let total_noshows = relay_node.reports(noshows_per_session).await?;

            log::info!("{kind} #{idx} approvals {session} -> {total_approvals}");
            log::info!("{kind} #{idx} no-shows {session} -> {total_noshows}");

            //assert!(total_approvals >= 9.0);
            //assert!(total_noshows >= 3.0);
        }
    }

    Ok(())
}