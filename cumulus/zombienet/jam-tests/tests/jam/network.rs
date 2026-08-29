// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! The JAM network the collators collate for: spawned by zombienet-sdk, then given the parasim
//! service the collators talk to.

use super::{env::Binaries, rpc::JamRpc};
use anyhow::Context;
use std::{
	path::{Path, PathBuf},
	process::Command,
	sync::atomic::{AtomicU16, Ordering},
	time::Duration,
};
use tokio::time::{sleep, Instant};
use zombienet_sdk::{LocalFileSystem, Network, NetworkConfigBuilder, NetworkConfigExt};

/// The tiny JAM protocol shape: six validators, and one ordinary node to serve RPC.
const VALIDATORS: usize = 6;
const ORDINARY_NODE: &str = "jam-or";

/// The service id parasim is registered under. The network is freshly spawned and private to one
/// test, so a fixed id is always free — and choosing it means never parsing `jamt --raw`, which
/// prints ids in hex where `--jam-service-id` wants decimal.
pub const PARASIM_SERVICE_ID: u32 = 5;

/// The endowment parasim is registered with.
const PARASIM_ENDOWMENT: &str = "1000000000000000";

/// JAM RPC ports sit above the collator range and well away from the 19800 default, so a testnet
/// the user is running themselves is never disturbed.
static NEXT_JAM_RPC_PORT: AtomicU16 = AtomicU16::new(42000);

/// PolkaVM cannot use its recompiler in this sandbox (no userfaultfd), and the native provider
/// clears the environment before spawning, so every JAM node needs these explicitly.
fn polkavm_env() -> Vec<(&'static str, &'static str)> {
	vec![("POLKAVM_BACKEND", "interpreter"), ("POLKAVM_ALLOW_INSECURE", "1")]
}

/// A running JAM network with parasim registered on it.
pub struct JamNetwork {
	network: Network<LocalFileSystem>,
	pub rpc_url: String,
	pub service_id: u32,
}

impl JamNetwork {
	/// Spawn the network, wait for it to finalize a block, and register parasim on it.
	pub async fn spawn(
		binaries: &Binaries,
		work_dir: &Path,
		deadline: Instant,
	) -> anyhow::Result<Self> {
		let rpc_port = NEXT_JAM_RPC_PORT.fetch_add(1, Ordering::Relaxed);
		let base_dir = work_dir.join("zombienet");
		std::fs::create_dir_all(&base_dir)?;

		let jam_node = path_str(&binaries.jam_node)?;
		let relay_node = path_str(&binaries.relay_node)?;

		let config = NetworkConfigBuilder::new()
			// zombienet-sdk cannot yet express a network without a relay chain: NetworkSpec
			// unconditionally unwraps the relaychain config. One idle validator is the cheapest
			// way to satisfy it; nothing in the test uses it.
			.with_relaychain(|relay| {
				relay
					.with_chain("rococo-local")
					.with_default_command(relay_node.as_str())
					.with_validator(|node| node.with_name("relay-filler"))
			})
			.with_jamchain(|jam| {
				let jam = jam
					.with_id("dev")
					.with_default_command(jam_node.as_str())
					.with_validator(|node| node.with_name("jam0").with_env(polkavm_env()));
				let jam = (1..VALIDATORS).fold(jam, |jam, index| {
					jam.with_validator(|node| {
						node.with_name(&format!("jam{index}")).with_env(polkavm_env())
					})
				});
				// The RPC port is pinned because jam nodes never enter the `Network` handle, so
				// there is no `get_node("jam-or").ws_uri()` to read it back from.
				jam.with_ordinary(|node| {
					node.with_name(ORDINARY_NODE).with_rpc_port(rpc_port).with_env(polkavm_env())
				})
			})
			.with_global_settings(|settings| settings.with_base_dir(base_dir.clone()))
			.build()
			.map_err(|errors| {
				anyhow::anyhow!(
					"network config: {}",
					errors.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("; ")
				)
			})?;

		let network = config.spawn_native().await.context("spawning the zombienet network")?;
		let rpc_url = format!("ws://127.0.0.1:{rpc_port}");
		log::info!("JAM network up, ordinary node RPC at {rpc_url}");

		let jam_network = JamNetwork { network, rpc_url, service_id: PARASIM_SERVICE_ID };
		let rpc = JamRpc::wait_ready(&jam_network.rpc_url, deadline).await.with_context(|| {
			format!("JAM node log tail:\n{}", jam_network.ordinary_node_log_tail(60))
		})?;
		jam_network.register_parasim(binaries, work_dir)?;

		while Instant::now() < deadline {
			if rpc.services().await.unwrap_or_default().contains(&(PARASIM_SERVICE_ID as u64)) {
				log::info!("parasim registered as service {PARASIM_SERVICE_ID}");
				return Ok(jam_network);
			}
			sleep(Duration::from_secs(2)).await;
		}
		Err(anyhow::anyhow!("service {PARASIM_SERVICE_ID} never appeared on the JAM chain"))
	}

	/// Register parasim from a COPY of the blob: PVM builds are not byte-deterministic, and a
	/// rebuild in the source tree would leave the on-chain code hash without a resolvable preimage.
	fn register_parasim(&self, binaries: &Binaries, work_dir: &Path) -> anyhow::Result<()> {
		let blob = work_dir.join("parasim-service.jam");
		std::fs::copy(&binaries.parasim_blob, &blob).with_context(|| {
			format!("copying {} to {}", binaries.parasim_blob.display(), blob.display())
		})?;

		let output = Command::new(&binaries.jamt)
			.args(["--rpc", &self.rpc_url, "create-service"])
			.arg(&blob)
			.args([
				PARASIM_ENDOWMENT,
				"--register=parasim",
				"--raw",
				"--id",
				&self.service_id.to_string(),
			])
			.envs(polkavm_env())
			.output()
			.with_context(|| format!("running {}", binaries.jamt.display()))?;

		anyhow::ensure!(
			output.status.success(),
			"jamt create-service failed ({}):\n{}",
			output.status,
			String::from_utf8_lossy(&output.stderr)
		);
		Ok(())
	}

	/// Where the ordinary node writes its log, for failure diagnostics.
	fn ordinary_node_log(&self) -> Option<PathBuf> {
		let base = self.network.base_dir()?;
		Some(Path::new(base).join(ORDINARY_NODE).join(format!("{ORDINARY_NODE}.log")))
	}

	pub fn ordinary_node_log_tail(&self, lines: usize) -> String {
		match self.ordinary_node_log() {
			Some(path) => format!(
				"----- JAM node {ORDINARY_NODE} ({}) -----\n{}",
				path.display(),
				super::collators::tail(&path, lines)
			),
			None => "(the network has no base dir, so its logs cannot be located)".to_string(),
		}
	}

	/// Stop every JAM node. Dropping the network does the same via `kill_on_drop`, which is what
	/// covers a panicking test; this is the tidy path.
	pub async fn shutdown(self) {
		if let Err(error) = self.network.destroy().await {
			log::warn!("tearing down the JAM network failed: {error}");
		}
	}
}

fn path_str(path: &Path) -> anyhow::Result<String> {
	path.to_str().map(str::to_string).with_context(|| format!("{} is not utf-8", path.display()))
}
