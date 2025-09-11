// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Zombienet integration tests for pallet-revive.
//!
//! This crate contains integration tests that use Zombienet to test
//! pallet-revive functionality in a realistic multi-node environment.
use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::assert_para_throughput;
use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use std::{
	fs::File,
	io::{BufRead, BufReader, Write},
	process::{Child, Command, Stdio},
	sync::Arc,
	thread,
	time::Duration,
};
use subxt::{self, backend::rpc::RpcClient, OnlineClient, PolkadotConfig};
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
	pub fn launch(node_rpc_url: &str, log_path: &str) -> Result<Self, anyhow::Error> {
		log::info!("Launching eth-rpc server with node RPC URL: {}", node_rpc_url);

		// Assuming eth-rpc is available in the PATH
		let mut child = Command::new("eth-rpc")
			.arg("--node-rpc-url")
			.arg(node_rpc_url)
			.arg("--dev")
			.arg("-leth-rpc=trace")
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

		// Sleep couple of seconds until eth-rpc server is up
		std::thread::sleep(Duration::from_secs(2));
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

pub struct TestEnvironment {
	pub collator_rpc_client: RpcClient,
	pub collator_client: OnlineClient<PolkadotConfig>,
	pub eth_rpc_client: Arc<HttpClient>,
	pub zombienet: Option<ZombienetNetwork>,
	pub eth_rpc_server: Option<EthRpcServer>,
}

impl TestEnvironment {
	/// Create a test environment with spawned zombienet and eth-rpc server
	pub async fn with_zombienet(
		collator_rpc_port: u16,
		eth_rpc_url: &str,
	) -> Result<Self, anyhow::Error> {
		let zn = ZombienetNetwork::launch(collator_rpc_port)
			.await
			.map_err(|err| anyhow!("Failed to spawn zombienet: {err:?}"))?;
		let base_dir = zn.network.base_dir().unwrap();

		let collator_name = "asset-hub-westend-collator1";
		let collator = zn
			.network
			.get_node(collator_name)
			.map_err(|err| anyhow!("Failed to get collator node: {err:?}"))?;

		let eth_rpc = EthRpcServer::launch(collator.ws_uri(), base_dir)
			.map_err(|err| anyhow!("Failed to spawn ETH-RPC server: {err:?}"))?;

		// TODO: use below approach once subxt versions used here and in zombienet-sdk match
		// let collator_rpc_client = collator.rpc().await.unwrap_or_else(|err| {
		//     panic!("Failed to get the RPC client for the collator {collator_name}: {err:?}")
		// });
		let collator_rpc_client = RpcClient::from_insecure_url(collator.ws_uri())
			.await
			.map_err(|err| anyhow!("Failed to create RPC client: {err:?}"))?;

		let collator_client = OnlineClient::from_rpc_client(collator_rpc_client.clone())
			.await
			.map_err(|err| anyhow!("Failed to create client from RPC client: {err:?}"))?;

		let eth_rpc_client = Arc::new(
			HttpClientBuilder::default()
				.build(eth_rpc_url)
				.map_err(|err| anyhow!("Failed to connect to eth-rpc server: {err:?}"))?,
		);

		Ok(Self {
			collator_rpc_client,
			collator_client,
			eth_rpc_client,
			zombienet: Some(zn),
			eth_rpc_server: Some(eth_rpc),
		})
	}

	/// Create a test environment that connects to external test network and eth-rpc server
	pub async fn without_zombienet(
		collator_rpc_port: u16,
		eth_rpc_url: &str,
	) -> Result<Self, anyhow::Error> {
		let collator_ws_uri = format!("ws://127.0.0.1:{collator_rpc_port}");
		let collator_rpc_client = RpcClient::from_insecure_url(collator_ws_uri)
			.await
			.map_err(|err| anyhow!("Failed to create RPC client: {err:?}"))?;
		let collator_client = OnlineClient::from_rpc_client(collator_rpc_client.clone())
			.await
			.map_err(|err| anyhow!("Failed to create client from RPC client: {err:?}"))?;

		let eth_rpc_client = Arc::new(
			HttpClientBuilder::default()
				.build(eth_rpc_url)
				.map_err(|err| anyhow!("Failed to connect to eth-rpc server: {err:?}"))?,
		);

		Ok(Self {
			collator_rpc_client,
			collator_client,
			eth_rpc_client,
			zombienet: None,
			eth_rpc_server: None,
		})
	}
}
