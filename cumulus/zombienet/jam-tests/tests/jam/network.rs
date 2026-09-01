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
				.args(["--rpc", &self.rpc_url, "--force-core", "0", "create-service"])
				.arg(&blob)
				.args([
					PARASIM_ENDOWMENT,
					"--register=parasim",
					"--raw",
					"--id",
					&self.service_id.to_string(),
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
		// A core each, and a real one. Two paras sharing a core would silently leave the first
		// one's authorizer overwritten, and the "no genesis-authorizer core left" reasoning below
		// counts paras as if each took its own.
		let mut cores: Vec<u32> = paras.iter().map(|para| para.core).collect();
		cores.sort_unstable();
		cores.dedup();
		anyhow::ensure!(
			cores.len() == paras.len() && cores.iter().all(|core| (*core as usize) < CORE_COUNT),
			"this network has {CORE_COUNT} cores, one each, which does not fit {:?}",
			paras.iter().map(|para| (para.id, para.core)).collect::<Vec<_>>()
		);

		for (assigned, para) in paras.iter().enumerate() {
			let names = para.collator_names();
			self.assign_core(binaries, para, para.core)?;

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

	/// Point `core`'s authorizer queue at `para`'s AURA authorizer.
	///
	/// Which lane the command travels down is not this caller's business: `parasim-tool` reads who
	/// holds the core's assigner privilege and either rides an unassigned core as a bootstrap
	/// instruction (service 0 still assigns it) or puts the command in an AURA package's token
	/// (parasim assigns it). It returns only once the core's *pool* holds the new authorizer, so
	/// afterwards the core really can carry the para's packages.
	pub fn assign_core(&self, binaries: &Binaries, para: &Para, core: u32) -> anyhow::Result<()> {
		let names = para.collator_names();
		run_step(
			&format!("assign-core: para {} onto core {core} for {names}", para.id),
			self.parasim_tool(binaries)
				.args(["--collators", &names])
				.args(["assign-core", &para.id.to_string(), &core.to_string()])
				.args(["--via-para", &para.id.to_string()]),
		)
	}

	/// Return `core` to the unassigned authorizer, so its pool drains over the next few blocks.
	///
	/// Only a core parasim was granted can be freed this way, and the command rides the core
	/// itself: it is a control package under the AURA authorizer that is about to go away, signed
	/// by `para`'s own collator set. Returns once the pool holds the unassigned authorizer, which
	/// is the moment the drain of the old one starts being visible.
	pub fn free_core(&self, binaries: &Binaries, para: &Para, core: u32) -> anyhow::Result<()> {
		let names = para.collator_names();
		run_step(
			&format!("free-core: core {core}, carried under para {}'s authorizer", para.id),
			self.parasim_tool(binaries)
				.args(["--collators", &names])
				.args(["free-core", &core.to_string()])
				.args(["--via-para", &para.id.to_string()]),
		)
	}

	/// The parachain head parasim has accumulated for `para`, or `None` while it has none.
	///
	/// This is the completion signal of the whole pipeline: JAM emits no "accumulated" event, so a
	/// para head that moves is the only proof that a work package was guaranteed, reported and
	/// accumulated. Reading it out of service storage rather than out of a collator's log is what
	/// makes an assertion about it an assertion about the chain.
	pub fn para_head(&self, binaries: &Binaries, para: u32) -> anyhow::Result<Option<ParaHead>> {
		let what = format!("display-key parahead: para {para}");
		let output = capture_step(
			&what,
			self.parasim_tool(binaries).args(["display-key", "parahead", &para.to_string()]),
		)?;
		parse_para_head(&output).with_context(|| format!("{what}: reading\n{output}"))
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
	let stdout = capture_step(what, command)?;
	log::info!("{what}: ok in {:?}\n{stdout}", started.elapsed());
	Ok(())
}

/// Run one step and hand back its stdout, for the reads whose value the caller parses.
///
/// Quieter than [`run_step`] because a state read happens on a poll loop: the caller logs the
/// value it extracted instead, and the full transcript stays at `debug`. A non-zero exit is still
/// fatal, and still carries the whole transcript.
fn capture_step(what: &str, command: &mut Command) -> anyhow::Result<String> {
	let started = std::time::Instant::now();
	let output = command.output().with_context(|| format!("{what}: running {command:?}"))?;
	let elapsed = started.elapsed();
	let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
	let stderr = String::from_utf8_lossy(&output.stderr);

	anyhow::ensure!(
		output.status.success(),
		"{what} failed ({}) after {elapsed:?}:\n{stdout}{stderr}",
		output.status,
	);
	if !stderr.trim().is_empty() {
		log::info!("{what}: stderr\n{stderr}");
	}
	log::debug!("{what}: ok in {elapsed:?}\n{stdout}");
	Ok(stdout)
}

/// A parachain head as JAM has accumulated it: the tip parasim believes the chain has reached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParaHead {
	pub number: u64,
	/// The block hash, `0x`-prefixed, in the same spelling a collator's RPC uses.
	pub hash: String,
}

impl std::fmt::Display for ParaHead {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(formatter, "#{} {}", self.number, self.hash)
	}
}

/// Pull the accumulated head out of `parasim-tool display-key parahead`'s report.
///
/// The head is a substrate header the tool decodes for us, so only its hash and number are read
/// back. An unrecognised report is an error rather than "no head": the two mean opposite things to
/// a stall assertion, and a tool whose output has moved on should say so loudly.
fn parse_para_head(output: &str) -> anyhow::Result<Option<ParaHead>> {
	if output.contains("no entry:") {
		return Ok(None);
	}
	let (_, header) = output
		.split_once("head (substrate header)")
		.context("no decoded header in the report")?;
	let field = |name: &str| {
		header
			.lines()
			.find_map(|line| line.trim().strip_prefix(name))
			.map(str::trim)
			.with_context(|| format!("the decoded header has no {name}"))
	};

	Ok(Some(ParaHead {
		number: field("number")?.parse().context("the header's number is not a number")?,
		hash: field("hash")?.to_string(),
	}))
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Verbatim output of `parasim-tool display-key parahead 0`, trimmed of the long hex.
	const REPORT: &str = "\
block       0xaaaa
service     5
para        0
service key 0xbbbb
state key   0xcccc

ParaInfo    123 bytes
  head_data           99 bytes
  validation_code     None
  pending_upgrade     None
  total_state_balance 0
  used_state_balance  0
  is_deregistering    false

head (substrate header)
  hash        0xdddd
  parent_hash 0xeeee
  number      17
  state_root  0xffff
  encoded     0x0102
";

	#[test]
	fn the_accumulated_head_is_read_out_of_the_report() {
		let head = parse_para_head(REPORT).expect("the report parses").expect("there is a head");
		// `parent_hash` and `state_root` sit either side of the two fields wanted, so a parser
		// matching on a prefix could pick up the wrong line without ever failing.
		assert_eq!(head, ParaHead { number: 17, hash: "0xdddd".to_string() });
	}

	#[test]
	fn a_para_with_no_head_yet_is_not_a_head_of_zero() {
		// Height zero is a real head — the genesis one — so "nothing accumulated yet" has to stay
		// distinguishable from it, or a stall would read as progress.
		let empty = "block       0xaaaa\n\nno entry: para 0 has no head at this block\n";
		assert_eq!(parse_para_head(empty).expect("the report parses"), None);
	}

	#[test]
	fn an_unrecognised_report_is_an_error() {
		assert!(parse_para_head("ParaInfo    123 bytes\n  (undecodable)\n").is_err());
	}
}
