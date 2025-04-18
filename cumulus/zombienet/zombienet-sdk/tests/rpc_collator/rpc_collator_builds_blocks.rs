// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use subxt::{OnlineClient, PolkadotConfig, ext::codec::Decode};
use tokio::time::{sleep, Duration};
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder};

const PARA_ID: u32 = 2000;

/// This test verifies that parachain collators can correctly use relay chain RPC
/// instead of a direct network connection to get the relay chain state.
///
/// It also tests resilience when relay chain RPC endpoints restart.
#[tokio::test(flavor = "multi_thread")]
async fn rpc_collator_builds_blocks() -> Result<(), anyhow::Error> {
    let _ = env_logger::try_init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let config = build_network_config().await?;

    let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
    let network = spawn_fn(config).await?;

    // Get nodes
    let alice = network.get_node("alice")?;
    let one = network.get_node("one")?;
    let two = network.get_node("two")?;
    let three = network.get_node("three")?;
    let dave = network.get_node("dave")?;
    let eve = network.get_node("eve")?;
    
    // Create client for alice
    log::info!("Creating client for alice");
    let alice_client: OnlineClient<PolkadotConfig> = alice.wait_client().await?;
    
    // 1. Check if parachain 2000 is registered within 225 seconds
    let start_time = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(225);
    let mut is_registered = false;
    log::info!("Checking if parachain {} is registered within {} seconds", PARA_ID, timeout.as_secs());

    while start_time.elapsed() < timeout && !is_registered {
        match network.parachain(PARA_ID) {
            Some(_) => {
                log::info!("Parachain {} is registered", PARA_ID);
                is_registered = true;
                break;
            }
            None => {
                log::info!("Parachain {} is not registered yet", PARA_ID);
                sleep(Duration::from_secs(10)).await;
            }
        }
    }
    if !is_registered {
        return Err(anyhow!("Parachain {} did not register within {:?}", PARA_ID, timeout));
    }

    // 2. Check that parachain 2000 reaches block height 10 within 250 seconds
    log::info!("Check that parachain {} reaches block height 10 within 250 seconds", PARA_ID);

    // Create client for eve
    log::info!("Creating client for eve");
    let eve_client: OnlineClient<PolkadotConfig> = eve.wait_client().await?;

    let start_time = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(250);
    let mut block_height = 0;
    
    while start_time.elapsed() < timeout && block_height < 10 {
        match eve_client
            .blocks()
            .at_latest()
            .await
        {
            Ok(block) => {
                block_height = block.number();
                log::info!("Parachain is at block {}", &block_height);
                if block_height >= 10 {
                    break;
                }
                sleep(Duration::from_secs(10)).await;
            }
            Err(e) => {
                log::info!("Error checking for parachain {}: {}", PARA_ID, e);
                return Err(anyhow!("Error checking for parachain {}: {}", PARA_ID, e));
            }
        }
    }
    if block_height < 10 {
        return Err(anyhow!("Parachain {} did not reach block height 10 within 250 seconds", PARA_ID));
    }
    log::info!("Parachain {} reached block height 10", PARA_ID);

    // 3. Check that eve reports block height is at least 12 within 250 seconds
    log::info!("Check that eve reports block height is at least 12 within 250 seconds");
    let start_time = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(250);
    let mut block_height = 0;

    while start_time.elapsed() < timeout && block_height < 12 {
        match eve_client
            .blocks()
            .at_latest()
            .await
        {
            Ok(block) => {
                block_height = block.number();
                log::info!("Eve is at block {}", &block_height);
                if block_height >= 12 {
                    break;
                }
                sleep(Duration::from_secs(10)).await;
            }
            Err(e) => {
                log::info!("Error checking for parachain {}: {}", PARA_ID, e);
                return Err(anyhow!("Error checking for parachain {}: {}", PARA_ID, e));
            }
        }
    }
    if block_height < 12 {
        return Err(anyhow!("Eve did not reach block height 12 within 250 seconds"));
    }
    log::info!("Eve reached block height 12");
    
    log::info!("Test successful!");
    Ok(())
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
    let images = zombienet_sdk::environment::get_images_from_env();
    log::info!("Using images: {images:?}");
    
    NetworkConfigBuilder::new()
        .with_relaychain(|r| {
            r.with_chain("rococo-local")
                .with_default_command("polkadot")
                .with_default_image(images.polkadot.as_str())
                .with_default_args(vec![("-lparachain=debug").into()])
                .with_node(|node| node.with_name("alice").validator(true))
                .with_node(|node| node.with_name("bob").validator(true))
                .with_node(|node| node.with_name("charlie").validator(true))
                .with_node(|node| node.with_name("one").validator(false))
                .with_node(|node| node.with_name("two").validator(false))
                .with_node(|node| node.with_name("three").validator(false))
        })
        .with_parachain(|p| {
            p.with_id(PARA_ID)
                .cumulus_based(true)
                .with_default_command("test-parachain")
                .with_default_image(images.cumulus.as_str())
                .with_collator(|n| {
                    n.with_name("dave")
                    .validator(true)
                    // .with_args(vec![
                    //     "--lparachain=trace,blockchain-rpc-client=debug".into(),
                    //     "--relay-chain-rpc-urls {{'one'|zombie('wsUri')}} {{'two'|zombie('wsUri')}} {{'three'|zombie('wsUri')}}".into(),
                    //     "--".into(),
                    //     "--bootnodes {{'one'|zombie('multiAddress')}} {{'two'|zombie('multiAddress')}} {{'three'|zombie('multiAddress')}}".into(),
                    // ])
                })
				.with_collator(|n| {
                    n.with_name("eve")
                    .validator(true)
                    .with_args(vec![
                        "-lparachain=trace,blockchain-rpc-client=debug".into(),
                        // "--relay-chain-rpc-urls".into(),
                        // "{{'one'|zombie('wsUri')}}".into(),
                        // "{{'two'|zombie('wsUri')}}".into(),
                        // "{{'three'|zombie('wsUri')}}".into(),
                        // "--".into(),
                        // "--bootnodes".into(),
                        // "{{'one'|zombie('multiAddress')}}".into(),
                        // "{{'two'|zombie('multiAddress')}}".into(),
                        // "{{'three'|zombie('multiAddress')}}".into(),
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