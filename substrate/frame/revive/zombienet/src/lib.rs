// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Zombienet integration tests for pallet-revive.
//!
//! This crate contains integration tests that use Zombienet to test
//! pallet-revive functionality in a realistic multi-node environment.
use anyhow::anyhow;
use serde_json::json;
use std::{
	fs::File,
	io::{BufRead, BufReader, Write},
	process::{Child, Command, Stdio},
	thread,
};

use cumulus_zombienet_sdk_helpers::assert_para_throughput;
use polkadot_primitives::Id as ParaId;
use zombienet_sdk::{LocalFileSystem, Network, NetworkConfig, NetworkConfigBuilder};

const PARA_ID: u32 = 1000;
pub const BEST_BLOCK_METRIC: &str = "block_height{status=\"best\"}";

pub struct ZombienetNetwork {
	pub network: Network<LocalFileSystem>,
}

impl ZombienetNetwork {
	/// Create zombienet config.
	/// Using below approach instead of '*.toml' because toml does not allow to
	/// unset some fields when patching ("devStakers" in this particular case).
	fn build_config(collator_rpc_port: u16) -> Result<NetworkConfig, anyhow::Error> {
		// images are not relevant for `native`, but we leave it here in case we use `k8s` some day
		let images = zombienet_sdk::environment::get_images_from_env();
		log::info!("Using images: {images:?}");

		NetworkConfigBuilder::new()
			.with_relaychain(|r| {
				r.with_chain("westend-local")
					.with_default_command("polkadot")
					.with_default_image(images.polkadot.as_str())
					.with_default_args(vec![("-lparachain=debug,xcm=trace").into()])
					.with_node(|node| {
						node.with_name("alice-westend-validator")
							.with_initial_balance(2000000000000)
					})
					.with_node(|node| {
						node.with_name("bob-westend-validator").with_initial_balance(2000000000000)
					})
			})
			.with_parachain(|p| {
				p.with_id(PARA_ID)
					.with_chain("asset-hub-westend-local")
					.cumulus_based(true)
					.with_default_command("polkadot-parachain")
					.with_default_image(images.cumulus.as_str())
					.with_default_args(vec![("-lparachain=debug,runtime::revive=debug").into()])
					// Unset devStakers
					// https://matrix.to/#/!fBnLYzzPKYqhYEWXWL:parity.io/$YJwq5jVe22ehGBukwwt2eYYICJFc_6bqoMpwbzlU4lU?via=parity.io&via=matrix.org&via=web3.foundation
					// https://matrix.to/#/!fBnLYzzPKYqhYEWXWL:parity.io/$9scJvupMsoM837t-L6Ww-anY-L40dUipPrncbmDkjbI?via=parity.io&via=matrix.org&via=web3.foundation
					.with_genesis_overrides(json!({
						"staking": {
							"devStakers": null,
						}
					}))
					.with_collator(|n| {
						n.with_name("asset-hub-westend-collator1")
							// eth-rpc will connect to this port
							.with_rpc_port(collator_rpc_port)
					})
					.with_collator(|n| n.with_name("asset-hub-westend-collator2"))
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

	/// Launch zombienet network and ensure it is running as expected.
	pub async fn launch(collator_rpc_port: u16) -> Result<ZombienetNetwork, anyhow::Error> {
		let config = Self::build_config(collator_rpc_port)?;

		log::info!("Launching network");
		let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
		let network = spawn_fn(config).await?;

		// Do not terminate network after the test is finished.
		// This is needed for CI to get logs from k8s.
		// Network shall be terminated from CI after logs are downloaded.
		// NOTE! For local execution (native provider) below call has no effect.
		network.detach().await;

		log::info!("Checking if network nodes are up");
		let result = network.wait_until_is_up(200u64).await;
		assert!(result.is_ok(), "Network is not up: {:?}", result.unwrap_err());

		let alice = network.get_node("alice-westend-validator")?.wait_client().await?;

		log::info!("Ensuring parachain making progress");
		assert_para_throughput(&alice, 5, [(ParaId::from(PARA_ID), 2..8)].into_iter().collect())
			.await?;

		Ok(Self { network })
	}
}

pub struct EthRpcServer {
	child: Child,
}

impl EthRpcServer {
	/// Launch eth-rpc server, which will connect to the parachain's collator.
	pub fn launch(
		node_rpc_ip: &str,
		node_rpc_port: u16,
		log_path: &str,
	) -> Result<Self, anyhow::Error> {
		let node_rpc_url = format!("ws://{node_rpc_ip}:{node_rpc_port}");
		log::info!("Launching eth-rpc server with node RPC URL: {}", node_rpc_url);

		// Assuming eth-rpc is available in the PATH
		let mut child = Command::new("eth-rpc")
			.arg("--node-rpc-url")
			.arg(node_rpc_url)
			.arg("--dev")
			.arg("-leth-rpc=debug")
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.spawn()
			.map_err(|e| anyhow!("Failed to spawn eth-rpc process: {}", e))?;

		let log_file_path = format!("{}/eth-rpc.log", log_path);

		// Handle stdout
		if let Some(stdout) = child.stdout.take() {
			let log_file_path_clone = log_file_path.clone();
			thread::spawn(move || {
				let reader = BufReader::new(stdout);
				let mut log_file = File::create(log_file_path_clone).unwrap();

				for line in reader.lines() {
					if let Ok(line) = line {
						println!("[eth-rpc] {}", line); // Print to stdout
						writeln!(log_file, "{}", line).unwrap(); // Write to file
						log_file.flush().unwrap();
					}
				}
			});
		}

		// Handle stderr similarly
		if let Some(stderr) = child.stderr.take() {
			let log_file_path_clone = log_file_path.clone();
			thread::spawn(move || {
				let reader = BufReader::new(stderr);
				let mut log_file = std::fs::OpenOptions::new()
					.create(true)
					.append(true)
					.open(log_file_path_clone)
					.unwrap();

				for line in reader.lines() {
					if let Ok(line) = line {
						eprintln!("[eth-rpc] {}", line); // Print to stderr
						writeln!(log_file, "{}", line).unwrap(); // Write to file
						log_file.flush().unwrap();
					}
				}
			});
		}

		log::info!("eth-rpc server launched with PID: {}", child.id());

		Ok(Self { child })
	}

	fn kill(&mut self) -> Result<(), anyhow::Error> {
		log::info!("Terminating eth-rpc server");
		self.child
			.kill()
			.map_err(|e| anyhow!("Failed to kill eth-rpc process: {}", e))?;
		self.child
			.wait()
			.map_err(|e| anyhow!("Failed to wait for eth-rpc process: {}", e))?;
		Ok(())
	}
}

impl Drop for EthRpcServer {
	fn drop(&mut self) {
		if let Err(e) = self.kill() {
			log::error!("Failed to terminate eth-rpc server in drop: {}", e);
		}
	}
}
