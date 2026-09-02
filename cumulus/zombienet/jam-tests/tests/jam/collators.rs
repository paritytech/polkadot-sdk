// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! The collators: `polkadot-omni-node` processes run against a JAM network.
//!
//! zombienet-sdk has no notion of a parachain on JAM yet, so these are plain child processes the
//! harness owns. [`Collators`] kills them all on drop, which covers a panicking or timing-out test
//! as well as a clean one.

use super::{chain_spec, env::Binaries, rpc::CollatorRpc};
use anyhow::Context;
use std::{
	fs::File,
	path::{Path, PathBuf},
	process::{Child, Command, Stdio},
	sync::atomic::{AtomicU16, Ordering},
	time::Duration,
};
use tokio::time::Instant;

/// Collator ports start well above the omni-node defaults (9944/30333) so a hand-run demo, or the
/// user's own network, keeps working while the tests run. Each set reserves its own block, because
/// the previous set's sockets can still be in `TIME_WAIT` when the next one binds.
const FIRST_PORT: u16 = 41000;
const PORTS_PER_COLLATOR: u16 = 3;
static NEXT_PORT: AtomicU16 = AtomicU16::new(FIRST_PORT);

struct Collator {
	name: String,
	process: Child,
	rpc_url: String,
	log_path: PathBuf,
}

/// A set of running collators, all authoring on the same parachain against the same JAM network.
pub struct Collators {
	collators: Vec<Collator>,
}

/// Everything the collators need to know about the JAM network they collate for.
pub struct JamTarget {
	pub rpc_url: String,
	pub service_id: u32,
	/// The copy of the authorizer blob whose hash the paras' cores were assigned from. The
	/// collators must hash the very same bytes — PVM builds are not byte-deterministic, so the
	/// build output is not a safe substitute.
	pub authorizer_blob: PathBuf,
}

/// One parachain of a run: the id it collates under, the core its work packages are authorized
/// on, and the dev accounts that collate for it.
#[derive(Clone, Debug)]
pub struct Para {
	pub id: u32,
	pub core: u32,
	/// Indices into [`chain_spec::DEV_ACCOUNTS`], in the order the AURA round-robin walks them.
	pub collators: Vec<usize>,
}

impl Para {
	/// The single para the collator-progress tests run: para 0 on core 0, collated by the first
	/// `count` dev accounts.
	pub fn single(count: usize) -> Self {
		Para { id: 0, core: 0, collators: (0..count).collect() }
	}

	/// The collator set as `parasim-tool --collators` and `--jam-collators` spell it.
	///
	/// Every collator of the para is started with this exact string, and the core is assigned with
	/// it too: a name's position in the list is the collator index the authorizer hash commits to,
	/// so a list that differs anywhere is a different hash.
	pub fn collator_names(&self) -> String {
		let names: Vec<String> =
			self.collators.iter().map(|index| chain_spec::dev_name(*index)).collect();
		names.join(",")
	}
}

impl Collators {
	/// Build `para`'s chain spec and start one collator per dev account in its set.
	pub fn spawn(
		binaries: &Binaries,
		work_dir: &Path,
		para: &Para,
		jam: &JamTarget,
	) -> anyhow::Result<Self> {
		let spec = work_dir.join(format!("jam-parachain-{}-spec.json", para.id));
		chain_spec::build(
			&binaries.omni_node,
			&binaries.runtime_wasm,
			&spec,
			para.id,
			&para.collators,
		)?;

		let count = para.collators.len();
		let collator_names = para.collator_names();
		let first_port = NEXT_PORT.fetch_add(count as u16 * PORTS_PER_COLLATOR, Ordering::Relaxed);
		let mut collators = Vec::with_capacity(count);
		let mut bootnode: Option<String> = None;

		for (index, account) in para.collators.iter().copied().enumerate() {
			let name = chain_spec::dev_name(account);
			let base_path = work_dir.join(&name);
			let p2p_port = first_port + index as u16 * PORTS_PER_COLLATOR;
			let rpc_port = p2p_port + 1;
			let prometheus_port = p2p_port + 2;
			let peer_id = node_key(&binaries.omni_node, &base_path, &spec)?;
			insert_collator_key(&binaries.omni_node, &base_path, &spec, account)?;

			let log_path = work_dir.join(format!("{name}.log"));
			let log = File::create(&log_path)?;

			let mut command = Command::new(&binaries.omni_node);
			command
				.arg("--chain")
				.arg(&spec)
				.arg("--base-path")
				.arg(&base_path)
				.args(["--collator", &chain_spec::dev_account_flag(account), "--force-authoring"])
				.args(["--port", &p2p_port.to_string()])
				.args(["--rpc-port", &rpc_port.to_string()])
				// Every collator needs its own metrics port; they would otherwise all try to bind
				// the 9615 default.
				.args(["--prometheus-port", &prometheus_port.to_string()])
				.args(["--jam-rpc-urls", &jam.rpc_url])
				.args(["--jam-service-id", &jam.service_id.to_string()])
				// The core is not named: the collator finds it by scanning the authorizer pools
				// for the hash these two arguments produce.
				.args(["--jam-collators", &collator_names])
				.arg("--jam-authorizer-blob")
				.arg(&jam.authorizer_blob)
				// Discovery is explicit: without this the collators would find, and try to sync
				// with, any other parachain node running on this machine.
				.arg("--no-mdns")
				.args(["-l", "jam-collator=debug,jam-rpc-interface=debug"])
				.stdout(Stdio::from(log.try_clone()?))
				.stderr(Stdio::from(log));

			if let Some(bootnode) = &bootnode {
				command.args(["--bootnodes", bootnode]);
			}

			let process = command
				.spawn()
				.with_context(|| format!("spawning collator {name}"))?;
			log::info!("collator {name}: rpc 127.0.0.1:{rpc_port}, log {}", log_path.display());

			bootnode.get_or_insert(format!("/ip4/127.0.0.1/tcp/{p2p_port}/p2p/{peer_id}"));
			collators.push(Collator {
				name,
				process,
				rpc_url: format!("ws://127.0.0.1:{rpc_port}"),
				log_path,
			});
		}

		Ok(Collators { collators })
	}

	/// An RPC client for the first collator, which is the one the assertions read.
	pub async fn rpc(&self, deadline: Instant) -> anyhow::Result<CollatorRpc> {
		let collator = self.collators.first().context("no collators were started")?;
		CollatorRpc::connect(&collator.rpc_url, deadline).await
	}

	/// Fail if any collator has exited — otherwise a dead collator only shows up as a timeout.
	pub fn check_all_running(&mut self) -> anyhow::Result<()> {
		for collator in &mut self.collators {
			if let Some(status) = collator.process.try_wait()? {
				anyhow::bail!("collator {} exited with {status}", collator.name);
			}
		}
		Ok(())
	}

	/// Every line of every collator's log that contains all of `needles`.
	///
	/// For the few things no state read can show — which core a submission went to, that the
	/// builder re-rooted onto a stuck head — the collator's own log is the only witness.
	pub fn log_lines_with(&self, needles: &[&str]) -> Vec<String> {
		self.collators
			.iter()
			.flat_map(|collator| {
				let contents = std::fs::read_to_string(&collator.log_path).unwrap_or_default();
				contents
					.lines()
					.filter(|line| needles.iter().all(|needle| line.contains(needle)))
					.map(|line| format!("{}: {line}", collator.name))
					.collect::<Vec<_>>()
			})
			.collect()
	}

	/// The tail of every collator log, for a failure message.
	pub fn log_tails(&self, lines: usize) -> String {
		self.collators
			.iter()
			.map(|collator| {
				format!(
					"----- collator {} ({}) -----\n{}",
					collator.name,
					collator.log_path.display(),
					tail(&collator.log_path, lines)
				)
			})
			.collect::<Vec<_>>()
			.join("\n")
	}
}

impl Drop for Collators {
	fn drop(&mut self) {
		for collator in &mut self.collators {
			let _ = collator.process.kill();
			let _ = collator.process.wait();
			log::info!("collator {} stopped", collator.name);
		}
	}
}

/// The last `lines` lines of a log file, or an explanation of why they are not available.
pub fn tail(path: &Path, lines: usize) -> String {
	match std::fs::read_to_string(path) {
		Ok(contents) => {
			let all: Vec<&str> = contents.lines().collect();
			all[all.len().saturating_sub(lines)..].join("\n")
		},
		Err(error) => format!("(could not read {}: {error})", path.display()),
	}
}

/// The collator's libp2p key, and the peer id derived from it.
///
/// A collator is an authority and refuses to author with a throwaway network key, so the key is
/// generated up front. Generation fails if the key already exists, which a re-run against the same
/// work dir (the demo's restart path) must tolerate — and it prints the peer id on stderr, so the
/// id is always read back with `inspect-node-key`, which prints it on stdout.
fn node_key(omni_node: &Path, base_path: &Path, spec: &Path) -> anyhow::Result<String> {
	if glob_secret(base_path).is_none() {
		run(Command::new(omni_node)
			.args(["key", "generate-node-key", "--base-path"])
			.arg(base_path)
			.arg("--chain")
			.arg(spec))?;
	}
	let key_file = glob_secret(base_path)
		.with_context(|| format!("no node key under {}", base_path.display()))?;

	let peer_id = run(Command::new(omni_node)
		.args(["key", "inspect-node-key", "--file"])
		.arg(&key_file))?;
	anyhow::ensure!(!peer_id.is_empty(), "no peer id for the key at {}", key_file.display());
	Ok(peer_id)
}

/// Give the collator the ed25519 key it signs work packages with: key type `coll`, derived from
/// the same `//<Name>` seed as everything else it runs as.
///
/// `--alice` only ever produces an in-memory sr25519 aura key, so without this the on-disk
/// keystore holds nothing the AURA authorizer would accept and the collator refuses to start.
/// Re-inserting the same suri rewrites the same file, which is what makes a re-run against an
/// existing work dir safe.
fn insert_collator_key(
	omni_node: &Path,
	base_path: &Path,
	spec: &Path,
	account: usize,
) -> anyhow::Result<()> {
	let name = chain_spec::dev_name(account);
	let suri = chain_spec::dev_suri(account);
	let started = std::time::Instant::now();

	run(Command::new(omni_node)
		.args(["key", "insert", "--base-path"])
		.arg(base_path)
		.arg("--chain")
		.arg(spec)
		.args(["--scheme", "ed25519", "--key-type", "coll", "--suri", &suri]))?;

	// `key insert` prints nothing on success, and a keystore the node then finds empty is a
	// collator that signs nothing — so read the file back rather than trust the exit code.
	let key_file = collator_key_file(base_path)
		.with_context(|| format!("`key insert` left no coll key under {}", base_path.display()))?;
	log::info!(
		"collator {name}: coll key {suri} -> {} in {:?}",
		key_file.display(),
		started.elapsed()
	);
	Ok(())
}

/// The `coll` keystore file, if there is one. File names are the key type id followed by the
/// public key, and `636f6c6c` is `"coll"` in hex.
fn collator_key_file(base_path: &Path) -> Option<PathBuf> {
	let chains = std::fs::read_dir(base_path.join("chains")).ok()?;
	chains
		.flatten()
		.filter_map(|chain| std::fs::read_dir(chain.path().join("keystore")).ok())
		.flatten()
		.flatten()
		.map(|entry| entry.path())
		.find(|path| {
			path.file_name()
				.and_then(|name| name.to_str())
				.is_some_and(|name| name.starts_with("636f6c6c"))
		})
}

/// Run a command to completion and return its trimmed stdout.
fn run(command: &mut Command) -> anyhow::Result<String> {
	let output = command.output().with_context(|| format!("running {command:?}"))?;
	anyhow::ensure!(
		output.status.success(),
		"{command:?} failed ({}): {}",
		output.status,
		String::from_utf8_lossy(&output.stderr)
	);
	Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

fn glob_secret(base_path: &Path) -> Option<PathBuf> {
	let chains = std::fs::read_dir(base_path.join("chains")).ok()?;
	chains
		.flatten()
		.map(|chain| chain.path().join("network/secret_ed25519"))
		.find(|secret| secret.exists())
}

/// How often the harness re-reads a collator's height while waiting.
pub const POLL_INTERVAL: Duration = Duration::from_secs(3);
