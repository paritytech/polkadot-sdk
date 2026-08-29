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
const PORTS_PER_COLLATOR: u16 = 2;
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
	pub core: u32,
}

impl Collators {
	/// Build the chain spec and start `count` collators in `work_dir`.
	pub fn spawn(
		binaries: &Binaries,
		work_dir: &Path,
		count: usize,
		jam: &JamTarget,
	) -> anyhow::Result<Self> {
		let spec = work_dir.join("jam-parachain-spec.json");
		chain_spec::build(&binaries.omni_node, &binaries.runtime_wasm, &spec, count)?;

		let first_port = NEXT_PORT.fetch_add(count as u16 * PORTS_PER_COLLATOR, Ordering::Relaxed);
		let mut collators = Vec::with_capacity(count);
		let mut bootnode: Option<String> = None;

		for index in 0..count {
			let name = chain_spec::DEV_ACCOUNTS[index].to_string().to_lowercase();
			let base_path = work_dir.join(&name);
			let p2p_port = first_port + index as u16 * PORTS_PER_COLLATOR;
			let rpc_port = p2p_port + 1;
			let peer_id = node_key(&binaries.omni_node, &base_path, &spec)?;

			let log_path = work_dir.join(format!("{name}.log"));
			let log = File::create(&log_path)?;

			let mut command = Command::new(&binaries.omni_node);
			command
				.arg("--chain")
				.arg(&spec)
				.arg("--base-path")
				.arg(&base_path)
				.args(["--collator", &chain_spec::dev_account_flag(index), "--force-authoring"])
				.args(["--port", &p2p_port.to_string()])
				.args(["--rpc-port", &rpc_port.to_string()])
				.args(["--jam-rpc-urls", &jam.rpc_url])
				.args(["--jam-service-id", &jam.service_id.to_string()])
				.args(["--jam-core", &jam.core.to_string()])
				// Discovery is explicit: without this the collators would find, and try to sync
				// with, any other parachain node running on this machine.
				.arg("--no-mdns")
				.args(["-l", "jam-collator=info,jam-rpc-interface=info"])
				.stdout(Stdio::from(log.try_clone()?))
				.stderr(Stdio::from(log));

			if let Some(bootnode) = &bootnode {
				command.args(["--bootnodes", bootnode]);
			}

			let process = command
				.spawn()
				.with_context(|| format!("spawning collator {name} from {}", binaries.omni_node.display()))?;
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
/// work dir (the demo's restart path) must tolerate.
fn node_key(omni_node: &Path, base_path: &Path, spec: &Path) -> anyhow::Result<String> {
	let existing = glob_secret(base_path);
	let output = match &existing {
		Some(key_file) => Command::new(omni_node)
			.args(["key", "inspect-node-key", "--file"])
			.arg(key_file)
			.output()?,
		None => Command::new(omni_node)
			.args(["key", "generate-node-key", "--base-path"])
			.arg(base_path)
			.arg("--chain")
			.arg(spec)
			.output()?,
	};
	anyhow::ensure!(
		output.status.success(),
		"node key for {} failed: {}",
		base_path.display(),
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
