// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test that nodes fetch availability chunks early for scheduled cores and normally for occupied
// core.

use std::ops::Range;
use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{assert_para_throughput, wait_for_nth_session_change};
use polkadot_primitives::{Id as ParaId, SessionIndex};
use serde_json::json;
use subxt::{OnlineClient, PolkadotConfig};
use subxt::blocks::Block;
use zombienet_orchestrator::network::Network;
use zombienet_orchestrator::network::node::NetworkNode;
use zombienet_sdk::{LocalFileSystem, NetworkConfigBuilder};

#[tokio::test(flavor = "multi_thread")]
async fn rewards_statistics_collector_test() -> Result<(), anyhow::Error> {
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

            (1..12)
                .fold(r, |acc, i|
                    acc.with_node(|node| node.with_name(&format!("validator-{i}"))))
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

    assert_para_throughput(
        &relay_client,
        15,
        [(ParaId::from(2000), 11..16), (ParaId::from(2001), 11..16)]
            .into_iter()
            .collect(),
    )
        .await?;

    // wait for a session to be finalized
    wait_for_nth_session_change(&mut blocks_sub, 1).await;

    assert_approval_usages_medians(
        1,
        0..12,
        &network,
    ).await?;

    Ok(())
}

async fn assert_approval_usages_medians(
    session: SessionIndex,
    validators_range: Range<u32>,
    network: &Network<LocalFileSystem>,
) -> Result<(), anyhow::Error> {
    let mut medians = vec![];

    for idx in validators_range.clone() {
        let validator_identifier = format!("validator-{}", idx);
        let relay_node = network.get_node(validator_identifier.clone())?;

        let approvals_per_session =
            report_label_with_attributes(
                "polkadot_parachain_rewards_statistics_collector_approvals_per_session",
                vec![
                    ("session", session.to_string().as_str()),
                    ("chain", "rococo_local_testnet"),
                ],
            );

        let total_approvals = relay_node.reports(approvals_per_session.clone()).await?;

        let mut metrics = vec![];
        for validator_idx in validators_range.clone() {
            let approvals_per_session_per_validator =
                report_label_with_attributes(
                    "polkadot_parachain_rewards_statistics_collector_approvals_per_session_per_validator",
                    vec![
                        ("session", session.to_string().as_str()),
                        ("validator_idx", validator_idx.to_string().as_str()),
                        ("chain", "rococo_local_testnet"),
                    ],
                );
            metrics.push(approvals_per_session_per_validator);
        }

        let mut total_sum = 0;
        for metric_per_validator in metrics {
            let validator_approvals_usage = relay_node.reports(metric_per_validator.clone()).await?;
            total_sum += validator_approvals_usage as u32;
        }

        assert_eq!(total_sum, total_approvals as u32);
        medians.push(total_sum / validators_range.len() as u32);
    }

    log::info!("Collected medians for session {session} {:?}", medians);
    Ok(())
}

fn report_label_with_attributes(label: &str, attributes: Vec<(&str, &str)>) -> String {
    let mut attrs: Vec<String> = vec![];
    for (k, v) in attributes {
        attrs.push(format!("{}=\"{}\"", k, v));
    }
    let final_attrs = attrs.join(",");
    format!("{label}{{{final_attrs}}}")
}