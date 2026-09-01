// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! The JAM network the collators collate for: spawned by zombienet-sdk, then given the parasim
//! service the collators talk to.

use super::{collators::Para, env::Binaries, rpc::JamRpc};
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

/// How many cores a network of [`VALIDATORS`] validators has.
///
/// Not a knob: polkajam's `tiny` parameter set ties `core_count` to the validator count (six
/// validators, three per core), and the next step up is 78 validators. It matters here because
/// the bootstrap lane needs a core that still holds the genesis authorizer, so how many cores the
/// chain has decides how many can be handed to parasim.
const CORE_COUNT: usize = VALIDATORS / 3;

/// JAM RPC ports sit above the collator range and well away from the 19800 default, so a testnet
/// the user is running themselves is never disturbed.
static NEXT_JAM_RPC_PORT: AtomicU16 = AtomicU16::new(42000);

/// PolkaVM cannot use its recompiler in this sandbox (no userfaultfd), and the native provider
/// clears the environment before spawning, so every JAM node needs these explicitly.
fn polkavm_env() -> Vec<(&'static str, &'static str)> {
	vec![("POLKAVM_BACKEND", "interpreter"), ("POLKAVM_ALLOW_INSECURE", "1")]
}

/// A running JAM network with parasim registered on it and the AURA authorizer hosted on it.
pub struct JamNetwork {
	network: Network<LocalFileSystem>,
	pub rpc_url: String,
	pub service_id: u32,
	/// The copy of the authorizer blob whose hash went on chain. Everything that has to agree on
	/// the authorizer hash — the assigned cores, the collators — is pointed at this file.
	pub authorizer_blob: PathBuf,
}

impl JamNetwork {
	/// Spawn the network, wait for it to finalize a block, register parasim on it, and host the
	/// AURA authorizer blob so an assigned core's code hash resolves to something.
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

		let authorizer_blob = copy_aside(&binaries.authorizer_blob, work_dir)?;
		let jam_network =
			JamNetwork { network, rpc_url, service_id: PARASIM_SERVICE_ID, authorizer_blob };
		let rpc = JamRpc::wait_ready(&jam_network.rpc_url, deadline).await.with_context(|| {
			format!("JAM node log tail:\n{}", jam_network.ordinary_node_log_tail(60))
		})?;
		jam_network.register_parasim(binaries, work_dir)?;
		jam_network.wait_for_parasim(&rpc, deadline).await?;
		jam_network.deploy_authorizer(binaries)?;

		Ok(jam_network)
	}

	/// Register parasim, and hand `jamt` the core to submit on rather than let it pick.
	///
	/// This is the run's only `jamt` call, and it deliberately happens before any core leaves the
	/// genesis authorizer: `jamt` builds its packages under that authorizer, so a core already
	/// pointed at a para would refuse them. Anything added here later has to keep that order, and
	/// name a core that still holds the genesis authorizer.
	fn register_parasim(&self, binaries: &Binaries, work_dir: &Path) -> anyhow::Result<()> {
		let blob = copy_aside(&binaries.parasim_blob, work_dir)?;

		run_step(
			&format!("jamt create-service: parasim as service {}", self.service_id),
			Command::new(&binaries.jamt)
				.args(["--rpc", &self.rpc_url, "create-service"])
				.arg(&blob)
				.args([
					PARASIM_ENDOWMENT,
					"--register=parasim",
					"--raw",
					"--id",
					&self.service_id.to_string(),
					"--force-core",
					"0",
				])
				.envs(polkavm_env()),
		)
	}

	/// Wait until the service id `create-service` was told to use is actually on the chain.
	async fn wait_for_parasim(&self, rpc: &JamRpc, deadline: Instant) -> anyhow::Result<()> {
		let started = std::time::Instant::now();
		while Instant::now() < deadline {
			if rpc.services().await.unwrap_or_default().contains(&(self.service_id as u64)) {
				log::info!(
					"parasim registered as service {} after {:?}",
					self.service_id,
					started.elapsed()
				);
				return Ok(());
			}
			sleep(Duration::from_secs(2)).await;
		}
		Err(anyhow::anyhow!("service {} never appeared on the JAM chain", self.service_id))
	}

	/// Host the AURA authorizer blob in the bootstrap service: solicit it, then provide it.
	///
	/// Must finish before the first `assign-core`. Validators fetch an authorizer's code by
	/// preimage lookup, so a core pointed at a code hash nobody hosts authorizes nothing — and
	/// says nothing about why. `parasim-tool` waits for the solicit to accumulate and then for the
	/// preimage to be readable at a finalized block, so there is nothing left to poll here.
	fn deploy_authorizer(&self, binaries: &Binaries) -> anyhow::Result<()> {
		run_step(
			&format!("deploy-authorizer: {}", self.authorizer_blob.display()),
			self.parasim_tool(binaries).arg("deploy-authorizer"),
		)
	}

	/// Point every para's core at its AURA authorizer, and hand parasim the cores' assigner
	/// privilege where the chain still allows it.
	///
	/// The order per para is `assign-core` then `grant-assigner`, and it is forced by who may
	/// assign a core. Service 0 is the assigner of every core at genesis, and a bootstrap
	/// instruction only rides a core that still holds the genesis authorizer — so `assign-core`
	/// goes first, riding the very core it is about to assign. `grant-assigner` then moves the
	/// privilege to parasim, which is what lets a later `free-core` or re-assignment travel inside
	/// an AURA package's token instead of the bootstrap lane.
	///
	/// That grant is itself a bootstrap instruction, so it needs *another* core still holding the
	/// genesis authorizer. On a run that assigns every core the chain has, the last one therefore
	/// keeps service 0 as its assigner; nothing can free or re-assign it for the rest of the run.
	pub fn assign_cores(&self, binaries: &Binaries, paras: &[Para]) -> anyhow::Result<()> {
		anyhow::ensure!(
			paras.len() <= CORE_COUNT && paras.iter().all(|para| (para.core as usize) < CORE_COUNT),
			"this network has {CORE_COUNT} cores, which does not fit {:?}",
			paras.iter().map(|para| (para.id, para.core)).collect::<Vec<_>>()
		);

		for (assigned, para) in paras.iter().enumerate() {
			let names = para.collator_names();
			run_step(
				&format!("assign-core: para {} onto core {} for {names}", para.id, para.core),
				self.parasim_tool(binaries)
					.args(["--collators", &names])
					.args(["assign-core", &para.id.to_string(), &para.core.to_string()]),
			)?;

			if assigned + 1 == CORE_COUNT {
				log::warn!(
					"core {}: keeping service 0 as its assigner. All {CORE_COUNT} cores are now \
					 assigned, so no core is left holding the genesis authorizer to carry the \
					 grant. free-core and re-assignment are unavailable on this core.",
					para.core
				);
				continue;
			}
			run_step(
				&format!("grant-assigner: core {} to service {}", para.core, self.service_id),
				self.parasim_tool(binaries)
					.args(["--collators", &names])
					.args(["grant-assigner", &para.core.to_string()]),
			)?;
		}
		Ok(())
	}

	/// A `parasim-tool` invocation carrying the arguments every phase-6 command needs.
	///
	/// `--authorizer-blob` and `--collators` are what an AURA authorizer hash is built from, so
	/// they have to be exactly what the para's collators are started with. A mismatch installs a
	/// hash nobody will ever satisfy, and the only symptom is a core that authorizes nothing.
	fn parasim_tool(&self, binaries: &Binaries) -> Command {
		let mut command = Command::new(&binaries.parasim_tool);
		command
			.args(["--rpc", &self.rpc_url])
			.args(["--service", &self.service_id.to_string()])
			.arg("--authorizer-blob")
			.arg(&self.authorizer_blob)
			.envs(polkavm_env());
		command
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

/// Copy a blob into the run's work dir and return the copy, which is what everything else names.
///
/// PVM builds are not byte-deterministic, so a rebuild in the source tree while a run is going
/// would leave an on-chain hash — a service's code, an authorizer's code — without a resolvable
/// preimage. The copy is also the run's record of what was actually put on the chain.
fn copy_aside(blob: &Path, work_dir: &Path) -> anyhow::Result<PathBuf> {
	let name = blob.file_name().with_context(|| format!("{} is not a file", blob.display()))?;
	let copy = work_dir.join(name);
	std::fs::copy(blob, &copy)
		.with_context(|| format!("copying {} to {}", blob.display(), copy.display()))?;
	Ok(copy)
}

/// Run one setup step, saying what it is about to submit and what came back.
///
/// Every step here changes JAM state through a work package, and a package JAM refuses leaves no
/// trace on the chain — so the command line, the exit status, the output and the wall clock are
/// all part of the record. Both tools read the state back themselves and exit non-zero when the
/// change did not land, which is why a bad status is fatal rather than a warning.
fn run_step(what: &str, command: &mut Command) -> anyhow::Result<()> {
	log::info!("{what}: running {command:?}");
	let started = std::time::Instant::now();
	let output = command.output().with_context(|| format!("{what}: running {command:?}"))?;
	let elapsed = started.elapsed();
	let stdout = String::from_utf8_lossy(&output.stdout);
	let stderr = String::from_utf8_lossy(&output.stderr);

	anyhow::ensure!(
		output.status.success(),
		"{what} failed ({}) after {elapsed:?}:\n{stdout}{stderr}",
		output.status,
	);
	log::info!("{what}: ok in {elapsed:?}\n{stdout}{stderr}");
	Ok(())
}
