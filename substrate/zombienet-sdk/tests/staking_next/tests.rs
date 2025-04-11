// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;
use subxt::{OnlineClient, PolkadotConfig};

use crate::staking_next::{activate_ah_client, build_network_config, set_validator_count};

#[tokio::test(flavor = "multi_thread")]
async fn happy_case() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let config = build_network_config().await?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	log::info!("Spawned");

	let rc_node = network.get_node("alice")?;
	let ah_next_node = network.get_node("charlie")?;

	let rc_client: OnlineClient<PolkadotConfig> = rc_node.wait_client().await?;
	let ah_next_client: OnlineClient<PolkadotConfig> = ah_next_node.wait_client().await?;

	log::info!("Set validator count to 500");
	set_validator_count(&ah_next_client, 500).await?;

	log::info!("Activate AH Client");
	activate_ah_client(&rc_client).await?;

	log::info!("Waiting for 30 minutes");

	tokio::time::sleep(Duration::from_secs(30 * 60)).await;
	Ok(())
}
