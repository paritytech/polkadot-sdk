// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Tests of the validator connectivity check of the health subsystem.
//!
//! The nodes must be built with the `short-health-tick` feature, which makes the check judge
//! every minute instead of every ten.
//!
//! The network has five validators, so each of them has four others to be connected to. A node
//! which misses one of them is connected to 75% of the validators, which is low connectivity.
//!
//! The first windows of a node may be judged as low connectivity, while the addresses of the
//! other validators are still being resolved. The tests therefore assert on the presence or the
//! absence of the connected line, never on the absence of the warning.

use crate::utils::{
	env_or_default, initialize_network, BLOCK_HEIGHT_FINALIZED_METRIC, INTEGRATION_IMAGE_ENV,
};
use anyhow::anyhow;
use std::time::Duration;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{Arg, NetworkConfig, NetworkConfigBuilder};

const CONNECTED_PATTERN: &str = "*Connected to enough validators of the session*";
const LOW_CONNECTIVITY_PATTERN: &str = "*means low connectivity*";

/// Every validator listens, so everybody gets connected to everybody. Dave and eve are the
/// control of the no-listener test: the same nodes in the same network, listening.
#[tokio::test(flavor = "multi_thread")]
async fn health_validator_connectivity_control_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let config = build_network_config(true)?;
	let network = initialize_network(config).await?;

	for name in ["alice", "dave", "eve"] {
		let result = network
			.get_node(name)?
			.wait_log_line_count_with_timeout(
				CONNECTED_PATTERN,
				true,
				LogLineCountOptions::new(|n| n >= 1, Duration::from_secs(300), false),
			)
			.await?;
		assert!(result.success(), "{name} was not judged connected");
	}

	log::info!("Test finished successfully");
	Ok(())
}

/// Simulates two validators which can't connect to each other: dave and eve do not listen, their
/// outbound connections work. Each of them dials the three listening validators, but neither can
/// dial the other, so they stay connected to three validators out of four.
#[tokio::test(flavor = "multi_thread")]
async fn health_validator_connectivity_no_listener_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let config = build_network_config(false)?;
	let network = initialize_network(config).await?;

	let dave = network.get_node("dave")?;
	let eve = network.get_node("eve")?;

	for node in [dave, eve] {
		let name = node.name();

		// The outbound connections work: the node dialed the bootnode and follows the chain.
		node.wait_metric_with_timeout(BLOCK_HEIGHT_FINALIZED_METRIC, |v| v > 2.0, 180u64)
			.await
			.map_err(|e| anyhow!("{name} does not finalize blocks: {}", e))?;

		let result = node
			.wait_log_line_count_with_timeout(
				LOW_CONNECTIVITY_PATTERN,
				true,
				LogLineCountOptions::new(|n| n >= 1, Duration::from_secs(300), false),
			)
			.await?;
		assert!(result.success(), "{name} was not judged as having low connectivity");
	}

	// Neither of them is ever judged connected, over three more windows.
	let result = dave
		.wait_log_line_count_with_timeout(
			CONNECTED_PATTERN,
			true,
			LogLineCountOptions::new(|n| n == 0, Duration::from_secs(200), true),
		)
		.await?;
	assert!(result.success(), "dave was judged connected");

	let result = eve
		.wait_log_line_count_with_timeout(
			CONNECTED_PATTERN,
			true,
			LogLineCountOptions::new(|n| n == 0, Duration::from_secs(5), true),
		)
		.await?;
	assert!(result.success(), "eve was judged connected");

	log::info!("Test finished successfully");
	Ok(())
}

fn build_network_config(dave_and_eve_listen: bool) -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	let polkadot_image = env_or_default(INTEGRATION_IMAGE_ENV, images.polkadot.as_str());

	// The connected line is logged at debug level.
	let args: Vec<Arg> = vec!["-lparachain::health=debug".into()];
	let mut no_listener_args = args.clone();
	if !dave_and_eve_listen {
		// The network backend listens only on ip addresses and ignores this one, so the node
		// starts without a listener and nobody can dial it.
		no_listener_args.push(("--listen-addr", "/dns/localhost/tcp/0/ws").into());
	}

	NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(polkadot_image.as_str())
				.with_default_args(args)
				.with_validator(|node| node.with_name("alice"))
				.with_validator(|node| node.with_name("bob"))
				.with_validator(|node| node.with_name("charlie"))
				.with_validator(|node| node.with_name("dave").with_args(no_listener_args.clone()))
				.with_validator(|node| node.with_name("eve").with_args(no_listener_args.clone()))
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
