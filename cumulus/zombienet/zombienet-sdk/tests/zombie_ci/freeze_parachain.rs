// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Zombienet integration test for the freeze/unfreeze parachain feature.

use crate::utils::initialize_network;
use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{
    assert_blocks_are_being_finalized, assert_para_throughput,
    submit_extrinsic_and_wait_for_finalization_success,
};
use polkadot_primitives::Id as ParaId;
use zombienet_sdk::{
    subxt::{
        ext::scale_value::value,
        tx::DynamicPayload,
        OnlineClient, PolkadotConfig,
    },
    subxt_signer::sr25519::dev,
    NetworkConfig, NetworkConfigBuilder,
};

const PARA_ID: u32 = 2000;

/// Test that a parachain can be frozen and unfrozen by root, and that:
/// - While frozen: no parachain blocks are produced, relay chain still works.
/// - After unfreeze: parachain resumes block production.
#[tokio::test(flavor = "multi_thread")]
async fn freeze_parachain_test() -> Result<(), anyhow::Error> {
    let _ = env_logger::try_init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    log::info!("Spawning network for freeze_parachain test");
    let config = build_network_config().await?;
    let network = initialize_network(config).await?;

    let relay_node = network.get_node("alice")?;
    let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

    // Verify normal parachain operation
    log::info!("Step 1: Verifying parachain {} produces blocks normally...", PARA_ID);
    // Wait for at least 3 backed candidates within 6 relay chain blocks.
    assert_para_throughput(&relay_client, 6, [(ParaId::from(PARA_ID), 2..7)]).await?;
    log::info!("✓ Parachain {} is producing blocks normally", PARA_ID);

    // Freeze the parachain
    log::info!("Step 2: Freezing parachain {}...", PARA_ID);
    submit_extrinsic_and_wait_for_finalization_success(
        &relay_client,
        &create_sudo_call_freeze(PARA_ID),
        &dev::alice(),
    )
    .await
    .map_err(|e| anyhow!("Failed to submit freeze_parachain extrinsic: {e}"))?;
    log::info!("✓ Parachain {} frozen (extrinsic finalized)", PARA_ID);

    // Verify relay chain is unaffected
    log::info!("Step 3: Verifying relay chain continues finalizing after freeze...");
    assert_blocks_are_being_finalized(&relay_client).await?;
    log::info!("✓ Relay chain is still finalizing blocks");

    // Verify no parachain blocks are produced while frozen
    log::info!(
        "Step 4: Verifying parachain {} produces NO blocks while frozen (12 relay chain blocks)...",
        PARA_ID
    );
    assert_para_throughput(&relay_client, 12, [(ParaId::from(PARA_ID), 0..1)]).await?;
    log::info!("✓ Parachain {} produced no blocks while frozen", PARA_ID);

    // Unfreeze the parachain
    log::info!("Step 5: Unfreezing parachain {}...", PARA_ID);
    submit_extrinsic_and_wait_for_finalization_success(
        &relay_client,
        &create_sudo_call_unfreeze(PARA_ID),
        &dev::alice(),
    )
    .await
    .map_err(|e| anyhow!("Failed to submit unfreeze_parachain extrinsic: {e}"))?;
    log::info!("✓ Parachain {} unfrozen (extrinsic finalized)", PARA_ID);

    // Verify parachain resumes block production
    // After unfreezing we expect at least 3 backed candidates in 10 relay chain blocks.
    log::info!(
        "Step 6: Verifying parachain {} resumes block production after unfreeze...",
        PARA_ID
    );
    assert_para_throughput(&relay_client, 10, [(ParaId::from(PARA_ID), 3..11)]).await?;
    log::info!("✓ Parachain {} resumed producing blocks after unfreeze", PARA_ID);

    log::info!("freeze_parachain test PASSED ✓");
    Ok(())
}

/// Creates a `sudo(Paras::freeze_parachain(para))` call.
fn create_sudo_call_freeze(para_id: u32) -> DynamicPayload {
    zombienet_sdk::subxt::tx::dynamic(
        "Sudo",
        "sudo",
        vec![value! {
            Paras(freeze_parachain { para: para_id })
        }],
    )
}

/// Creates a `sudo(Paras::unfreeze_parachain(para))` call.
fn create_sudo_call_unfreeze(para_id: u32) -> DynamicPayload {
    zombienet_sdk::subxt::tx::dynamic(
        "Sudo",
        "sudo",
        vec![value! {
            Paras(unfreeze_parachain { para: para_id })
        }],
    )
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
    let images = zombienet_sdk::environment::get_images_from_env();
    log::info!("Using images: {images:?}");

    // Network setup:
    // - relaychain nodes:
    //   - alice: validator (also used to submit sudo extrinsics)
    //   - bob:   validator
    // - parachain nodes:
    //   - charlie: collator
    let config = NetworkConfigBuilder::new()
        .with_relaychain(|r| {
            r.with_chain("rococo-local")
                .with_default_command("polkadot")
                .with_default_image(images.polkadot.as_str())
                .with_default_args(vec![("-lparachain=debug").into()])
                .with_validator(|node| node.with_name("alice"))
                .with_validator(|node| node.with_name("bob"))
        })
        .with_parachain(|p| {
            p.with_id(PARA_ID)
                .with_default_command("test-parachain")
                .with_default_image(images.cumulus.as_str())
                .with_default_args(vec![("-lparachain=debug").into()])
                .with_collator(|n| n.with_name("charlie").validator(true))
        })
        .with_global_settings(|global_settings| {
            match std::env::var("ZOMBIENET_SDK_BASE_DIR") {
                Ok(val) => global_settings.with_base_dir(val),
                _ => global_settings,
            }
        })
        .build()
        .map_err(|e| {
            let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
            anyhow!("config errs: {errs}")
        })?;

    Ok(config)
}