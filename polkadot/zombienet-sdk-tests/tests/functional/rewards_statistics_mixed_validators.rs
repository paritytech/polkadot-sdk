// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test that nodes fetch availability chunks early for scheduled cores and normally for occupied
// core.

use std::collections::HashMap;
use std::ops::Range;
use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{assert_para_throughput, wait_for_nth_session_change, report_label_with_attributes, assert_finality_lag, wait_for_first_session_change, find_event_and_decode_fields};
use polkadot_primitives::{CandidateReceiptV2, Id as ParaId, SessionIndex};
use serde_json::json;
use subxt::{OnlineClient, PolkadotConfig};
use subxt::blocks::Block;
use zombienet_orchestrator::network::Network;
use zombienet_orchestrator::network::node::NetworkNode;
use zombienet_sdk::{LocalFileSystem, NetworkConfigBuilder};
use pallet_revive::H256;

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
                                "num_cores": 1,
                                    "group_rotation_frequency": 4
							},
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
                            "--no-hardware-benchmarks".into(),
                            "--insecure-validator-i-know-what-i-do".into(),
                        ])
                        .with_command("malus")
                        .with_subcommand("dispute-ancestor")
                        .invulnerable(false)
                })
            })
        })
        .with_parachain(|p| {
            p.with_id(1000)
                .with_default_command("adder-collator")
                .with_default_image(
                    std::env::var("COL_IMAGE")
                        .unwrap_or("docker.io/paritypr/colander:latest".to_string())
                        .as_str(),
                )
                .cumulus_based(false)
                .with_default_args(vec![("-lparachain=debug").into()])
                .with_collator(|n| n.with_name("adder-collator-1000"))
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

    assert_para_throughput_for_included_parablocks(
        &relay_client,
        20,
        [(polkadot_primitives::Id::from(1000), (10..30, 8..14))].into_iter().collect(),
    ).await?;

    let mut blocks_sub = relay_client.blocks().subscribe_finalized().await?;

    //wait_for_nth_session_change(&mut blocks_sub, 1).await?;

    // Assert the parachain finalized block height is also on par with the number of backed
    // candidates. We can only do this for the collator based on cumulus.
    assert_finality_lag(&relay_client, 6).await?;

    assert_approval_usages_medians(
        1,
        12,
        [("validator", 0..9), ("malus", 9..12)].into_iter().collect(),
        &network,
    ).await?;

    Ok(())
}

pub async fn assert_para_throughput_for_included_parablocks(
    relay_client: &OnlineClient<PolkadotConfig>,
    stop_after: u32,
    expected_candidate_ranges: HashMap<ParaId, (Range<u32>, Range<u32>)>,
) -> Result<(), anyhow::Error> {
    let mut blocks_sub = relay_client.blocks().subscribe_finalized().await?;
    let mut candidate_backed_count: HashMap<ParaId, u32> = HashMap::new();
    let mut candidate_included_count: HashMap<ParaId, u32> = HashMap::new();
    let mut current_block_count = 0;

    let valid_para_ids: Vec<ParaId> = expected_candidate_ranges.keys().cloned().collect();

    // Wait for the first session, block production on the parachain will start after that.
    wait_for_first_session_change(&mut blocks_sub).await?;

    while let Some(block) = blocks_sub.next().await {
        let block = block?;
        log::debug!("Finalized relay chain block {}", block.number());
        let events = block.events().await?;
        let is_session_change = events.iter().any(|event| {
            event.as_ref().is_ok_and(|event| {
                event.pallet_name() == "Session" && event.variant_name() == "NewSession"
            })
        });

        // Do not count blocks with session changes, no backed blocks there.
        if is_session_change {
            continue;
        }

        current_block_count += 1;

        let receipts_for_backed = find_event_and_decode_fields::<CandidateReceiptV2<H256>>(
            &events,
            "ParaInclusion",
            "CandidateBacked",
        )?;

        for receipt in receipts_for_backed {
            let para_id = receipt.descriptor.para_id();
            log::debug!("Block backed for para_id {para_id}");
            if !valid_para_ids.contains(&para_id) {
                return Err(anyhow!("Invalid ParaId detected: {}", para_id));
            };
            *(candidate_backed_count.entry(para_id).or_default()) += 1;
        }

        let receipts_for_included = find_event_and_decode_fields::<CandidateReceiptV2<H256>>(
            &events,
            "ParaInclusion",
            "CandidateIncluded",
        )?;

        for receipt in receipts_for_included {
            let para_id = receipt.descriptor.para_id();
            log::debug!("Block included for para_id {para_id}");
            if !valid_para_ids.contains(&para_id) {
                return Err(anyhow!("Invalid ParaId detected: {}", para_id));
            };
            *(candidate_included_count.entry(para_id).or_default()) += 1;
        }

        if current_block_count == stop_after {
            break;
        }
    }

    log::info!(
		"Reached {stop_after} finalized relay chain blocks that contain backed/included candidates. The per-parachain distribution is: {:#?} {:#?}",
		candidate_backed_count.iter().map(|(para_id, count)| format!("{para_id} has {count} backed candidates"),).collect::<Vec<_>>(),
        candidate_included_count.iter().map(|(para_id, count)| format!("{para_id} has {count} included candidates"),).collect::<Vec<_>>()
	);

    for (para_id, expected_candidate_range) in expected_candidate_ranges {
        let actual_backed = candidate_backed_count
            .get(&para_id)
            .ok_or_else(|| anyhow!("ParaId did not have any backed candidates"))?;

        let actual_included = candidate_included_count
            .get(&para_id)
            .ok_or_else(|| anyhow!("ParaId did not have any included candidates"))?;

        if !expected_candidate_range.0.contains(actual_backed) {
            let range = expected_candidate_range.0;
            return Err(anyhow!(
				"Candidate Backed count {actual_backed} not within range {range:?}"
			))
        }

        if !expected_candidate_range.1.contains(actual_included) {
            let range = expected_candidate_range.1;
            return Err(anyhow!(
				"Candidate Included count {actual_included} not within range {range:?}"
			))
        }
    }

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

            log::info!("Session {session}: {kind} #{idx} (Approvals: {total_approvals}, Noshows: {total_noshows}) ");

            assert!(total_approvals >= 9.0);
            assert!(total_noshows >= 3.0);
        }
    }

    Ok(())
}