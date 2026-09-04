// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! The external artifacts a JAM collator test needs, and the gate that skips cleanly when any
//! of them is missing.

use std::path::{Path, PathBuf};

/// Paths to everything the harness shells out to.
#[derive(Clone, Debug)]
pub struct Binaries {
	/// The polkajam node binary that zombienet-sdk spawns for every JAM node.
	pub jam_node: PathBuf,
	/// The polkajam build that generates the chain spec, when it is not [`Binaries::jam_node`].
	///
	/// Transitional, and it should be `None`: polkajam's genesis-config support and the JIP-2
	/// state RPCs the collator reads sit on two branches that have not been merged, so until they
	/// are, one build writes the genesis and another serves it.
	pub genspec_node: Option<PathBuf>,
	/// The `parasim-tool` CLI, used by the dynamic-core tests to point cores at paras mid-run.
	///
	/// `None` unless `PARASIM_TOOL_BIN` is set. Nothing else shells out to it — the para head
	/// every other test asserts on is read straight off the JAM node's RPC — so a run without it
	/// skips those two tests and nothing more.
	pub parasim_tool: Option<PathBuf>,
	/// The compiled parasim service blob, which genesis creates the service from.
	///
	/// The service blob, strictly speaking: pointing this at the real `parachain-service.jam`
	/// runs the real service instead — see [`Binaries::pvf_blob`], which that needs.
	pub parasim_blob: PathBuf,
	/// The paras' PVM validation code (a raw `.polkavm` blob), when the run targets the real
	/// parachain service. `None` — the default — is the parasim arrangement: no PVF exists,
	/// nothing is pre-registered, parasim registers paras on first sight and validates nothing.
	///
	/// Set (`JAM_PVF_BLOB`), the run seeds genesis for the real service: the blob becomes a
	/// service preimage, every para is pre-registered with its genesis head and this code's
	/// `(hash, len)`, and the collators stamp this file's hash into their candidates
	/// (`--jam-pvf-blob`).
	pub pvf_blob: Option<PathBuf>,
	/// The compiled AURA authorizer blob. Only its hash ever reaches the chain, but the collators
	/// and the genesis that queues their core have to hash the same bytes, so this one file is
	/// handed to both.
	pub authorizer_blob: PathBuf,
	/// The collator binary.
	pub omni_node: PathBuf,
	/// The parachain runtime the collators run.
	pub runtime_wasm: PathBuf,
	/// A relay chain node. zombienet-sdk cannot yet spawn a network without a relay chain, so one
	/// idle validator is started alongside the JAM nodes and otherwise ignored.
	pub relay_node: PathBuf,
}

/// Where this crate sits relative to the workspace root, so the defaults can find `target/`.
fn workspace_root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..").canonicalize().unwrap_or_default()
}

fn from_env_or(var: &str, default: impl FnOnce() -> PathBuf) -> PathBuf {
	std::env::var_os(var).map(PathBuf::from).unwrap_or_else(default)
}

/// A port base from the environment, for when a committed default block is already taken: a
/// user's own long-running network holds exactly these blocks, and the spawn's only symptom
/// would be zombienet's "Can't bind in socket".
pub fn port_base(var: &str, default: u16) -> u16 {
	match std::env::var(var) {
		Ok(value) => value.parse().unwrap_or_else(|_| panic!("{var} is not a port: {value}")),
		Err(_) => default,
	}
}

impl Binaries {
	/// Resolve every artifact from the environment, or return the human-readable list of what is
	/// missing so the caller can skip the test with an explanation.
	pub fn from_env() -> Result<Self, String> {
		let root = workspace_root();
		let binaries = Binaries {
			jam_node: from_env_or("JAM_NODE_BIN", PathBuf::new),
			genspec_node: std::env::var_os("JAM_GENSPEC_BIN").map(PathBuf::from),
			parasim_tool: std::env::var_os("PARASIM_TOOL_BIN").map(PathBuf::from),
			parasim_blob: from_env_or("PARASIM_BLOB", PathBuf::new),
			pvf_blob: std::env::var_os("JAM_PVF_BLOB").map(PathBuf::from),
			authorizer_blob: from_env_or("AUTHORIZER_BLOB", PathBuf::new),
			omni_node: from_env_or("OMNI_NODE_BIN", || {
				root.join("target/release/polkadot-omni-node")
			}),
			relay_node: from_env_or("RELAY_NODE_BIN", || root.join("target/release/polkadot")),
			runtime_wasm: from_env_or("RUNTIME_WASM", || {
				root.join(
					"target/release/wbuild/parachain-template-runtime/\
					 parachain_template_runtime.compact.compressed.wasm",
				)
			}),
		};

		// A relay validator refuses to start without its PVF workers, and zombienet reports that
		// only as a spawn timeout — so check for them here, where the message is useful.
		let workers = binaries.relay_node.parent().unwrap_or(Path::new(""));
		let prepare_worker = workers.join("polkadot-prepare-worker");
		let execute_worker = workers.join("polkadot-execute-worker");

		let mut wanted: Vec<(&str, &PathBuf)> = vec![
			("JAM_NODE_BIN (the polkajam node binary)", &binaries.jam_node),
			("PARASIM_BLOB (the parasim service .jam blob)", &binaries.parasim_blob),
			(
				"AUTHORIZER_BLOB (parachain-authorizer-sr25519.jam, the scheme the template \
				 runtime's AuraId asks for)",
				&binaries.authorizer_blob,
			),
			("OMNI_NODE_BIN (cargo build --release -p polkadot-omni-node)", &binaries.omni_node),
			(
				"RUNTIME_WASM (cargo build --release -p parachain-template-runtime)",
				&binaries.runtime_wasm,
			),
			("RELAY_NODE_BIN (cargo build --release --bin polkadot)", &binaries.relay_node),
			("the relay node's PVF workers (--bin polkadot-prepare-worker)", &prepare_worker),
			("the relay node's PVF workers (--bin polkadot-execute-worker)", &execute_worker),
		];
		// Only when it was asked for: an unset `JAM_GENSPEC_BIN` means the node binary generates
		// its own spec, which is the arrangement this should get back to.
		if let Some(genspec) = &binaries.genspec_node {
			wanted.push((
				"JAM_GENSPEC_BIN (a polkajam whose gen-spec reads the genesis keys)",
				genspec,
			));
		}
		// Likewise optional, but checked here rather than where it is used: a `PARASIM_TOOL_BIN`
		// that points at nothing is a typo, and every test should say so instead of two of them
		// skipping as though it had been left unset.
		if let Some(tool) = &binaries.parasim_tool {
			wanted.push((PARASIM_TOOL, tool));
		}
		if let Some(pvf) = &binaries.pvf_blob {
			wanted.push(("JAM_PVF_BLOB (the paras' PVM validation code)", pvf));
		}

		match missing(&wanted) {
			None => Ok(binaries),
			Some(reason) => Err(reason),
		}
	}
}

/// How `PARASIM_TOOL_BIN` is named in a skip message, wherever it is missed.
const PARASIM_TOOL: &str = "PARASIM_TOOL_BIN (the parasim-tool CLI)";

/// The ones of `wanted` that are not on disk, as a skip message. `None` when they all are.
fn missing(wanted: &[(&str, &PathBuf)]) -> Option<String> {
	let missing: Vec<String> = wanted
		.iter()
		.filter(|(_, path)| !path.exists())
		.map(|(what, path)| format!("  {what}: {}", path.display()))
		.collect();
	(!missing.is_empty()).then(|| format!("missing artifacts:\n{}", missing.join("\n")))
}

/// Say the test is being skipped, and why.
///
/// Not `log::warn!`: this has to be readable without a logger, and both are only visible under
/// `--nocapture` anyway.
fn skip(test: &str, reason: &str) {
	eprintln!("SKIP {test}: {reason}");
}

/// Resolve the artifacts, or print why the test is being skipped and return `None`.
pub fn binaries_or_skip(test: &str) -> Option<Binaries> {
	match Binaries::from_env() {
		Ok(binaries) => Some(binaries),
		Err(reason) => {
			skip(test, &reason);
			None
		},
	}
}

/// The `parasim-tool` CLI, or `None` after saying that this test is being skipped without it.
///
/// For the two dynamic-core tests, which are the only ones that move a core mid-run and so the
/// only ones that shell out to the tool at all.
pub fn parasim_tool_or_skip(test: &str, binaries: &Binaries) -> Option<PathBuf> {
	match &binaries.parasim_tool {
		Some(tool) => Some(tool.clone()),
		None => {
			skip(test, &format!("missing artifacts:\n  {PARASIM_TOOL}: unset"));
			None
		},
	}
}
