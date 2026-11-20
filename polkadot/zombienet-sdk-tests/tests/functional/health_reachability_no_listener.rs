// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Tests of the inbound reachability check of the health subsystem.
//!
//! The nodes must be built with the `short-health-tick` feature, which makes the check judge
//! every minute instead of every ten.
//!
//! In a static network on one host every connection is opened at startup, so a node sees inbound
//! connections only in its first window and is judged unreachable from the second one on. The
//! tests therefore assert on the presence or the absence of the reachable line, never on the
//! absence of the warning.
//!
//! The network has five validators. Alice is the bootnode: everybody dials it, so it is judged
//! reachable. Dave is the node under test. A node which is not a bootnode is dialed by nobody at
//! startup, so bob, charlie and eve get dave as an additional bootnode and start after it: dave
//! sees three distinct inbound peers in its first window, unless it does not listen.

use crate::utils::{
	env_or_default, initialize_network, BLOCK_HEIGHT_FINALIZED_METRIC, INTEGRATION_IMAGE_ENV,
};
use anyhow::anyhow;
use std::time::Duration;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{Arg, NetworkConfig, NetworkConfigBuilder};

const REACHABLE_PATTERN: &str = "*Peers connect to us: the node is reachable*";
const UNREACHABLE_PATTERN: &str = "*the node may be unreachable*";
const NO_LISTENER_PATTERN: &str = "*litep2p started with no listen addresses*";

/// Alice and dave are dialed by at least three nodes at startup, so each sees three distinct
/// inbound peers in its first window and is judged reachable. Dave is the control of the
/// no-listener test: the same node in the same topology, listening.
#[tokio::test(flavor = "multi_thread")]
async fn health_reachability_control_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let config = build_network_config(true)?;
	let network = initialize_network(config).await?;

	for name in ["alice", "dave"] {
		let result = network
			.get_node(name)?
			.wait_log_line_count_with_timeout(
				REACHABLE_PATTERN,
				true,
				LogLineCountOptions::new(|n| n >= 1, Duration::from_secs(180), false),
			)
			.await?;
		assert!(result.success(), "{name} was not judged reachable");
	}

	log::info!("Test finished successfully");
	Ok(())
}

/// Simulates blocked inbound connections: dave does not listen, its outbound connections work.
/// Dave follows the chain but is never judged reachable.
#[tokio::test(flavor = "multi_thread")]
async fn health_reachability_no_listener_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let config = build_network_config(false)?;
	let network = initialize_network(config).await?;

	let alice = network.get_node("alice")?;
	let dave = network.get_node("dave")?;

	// Dave has no listeners.
	let result = dave
		.wait_log_line_count_with_timeout(
			NO_LISTENER_PATTERN,
			true,
			LogLineCountOptions::new(|n| n >= 1, Duration::from_secs(10), false),
		)
		.await?;
	assert!(result.success(), "dave has a listener");

	// Alice is reachable.
	let result = alice
		.wait_log_line_count_with_timeout(
			REACHABLE_PATTERN,
			true,
			LogLineCountOptions::new(|n| n >= 1, Duration::from_secs(180), false),
		)
		.await?;
	assert!(result.success(), "alice was not judged reachable");

	// Dave's outbound connections work: it dialed the bootnode and follows the chain.
	dave.wait_metric_with_timeout(BLOCK_HEIGHT_FINALIZED_METRIC, |v| v > 2.0, 180u64)
		.await
		.map_err(|e| anyhow!("dave does not finalize blocks: {}", e))?;

	// Dave is never judged reachable over two full windows.
	let result = dave
		.wait_log_line_count_with_timeout(
			REACHABLE_PATTERN,
			true,
			LogLineCountOptions::new(|n| n == 0, Duration::from_secs(150), true),
		)
		.await?;
	assert!(result.success(), "dave was judged reachable");

	let result = dave
		.wait_log_line_count_with_timeout(
			UNREACHABLE_PATTERN,
			true,
			LogLineCountOptions::new(|n| n >= 1, Duration::from_secs(60), false),
		)
		.await?;
	assert!(result.success(), "dave was not judged unreachable");

	log::info!("Test finished successfully");
	Ok(())
}

fn build_network_config(dave_listens: bool) -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	let polkadot_image = env_or_default(INTEGRATION_IMAGE_ENV, images.polkadot.as_str());

	let args: Vec<Arg> = vec![
		// The reachable line is logged at debug level.
		"-lparachain::health=debug".into(),
		// All nodes are on 127.0.0.1; without the flag no inbound connection counts.
		"--allow-private-ip".into(),
		// With mDNS the nodes find each other on their own and who dials whom is a race: alice
		// and dave dial out and may be left with fewer than three inbound peers.
		"--no-mdns".into(),
	];
	let mut dave_args = args.clone();
	if !dave_listens {
		// The network backend listens only on ip addresses and ignores this one, so dave starts
		// without a listener. zombienet replaces the port and hands the address to the dialers,
		// which get their connections refused.
		dave_args.push(("--listen-addr", "/dns/localhost/tcp/0/ws").into());
	}
	// The node dials dave at startup. The token also makes zombienet start it after dave.
	let mut dialer_args = args.clone();
	dialer_args.push(("--bootnodes", "{{ZOMBIE:dave:multiaddr}}").into());

	NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(polkadot_image.as_str())
				.with_default_args(args)
				.with_validator(|node| node.with_name("alice"))
				.with_validator(|node| node.with_name("dave").with_args(dave_args))
				.with_validator(|node| node.with_name("bob").with_args(dialer_args.clone()))
				.with_validator(|node| node.with_name("charlie").with_args(dialer_args.clone()))
				.with_validator(|node| node.with_name("eve").with_args(dialer_args.clone()))
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
