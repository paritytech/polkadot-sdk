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
	/// The `parasim-tool` CLI, used by the dynamic-core tests to point cores at paras mid-run.
	pub parasim_tool: PathBuf,
	/// The compiled parasim service blob, which genesis creates the service from.
	pub parasim_blob: PathBuf,
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

impl Binaries {
	/// Resolve every artifact from the environment, or return the human-readable list of what is
	/// missing so the caller can skip the test with an explanation.
	pub fn from_env() -> Result<Self, String> {
		let root = workspace_root();
		let binaries = Binaries {
			jam_node: from_env_or("JAM_NODE_BIN", PathBuf::new),
			parasim_tool: from_env_or("PARASIM_TOOL_BIN", PathBuf::new),
			parasim_blob: from_env_or("PARASIM_BLOB", PathBuf::new),
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

		let missing: Vec<String> = [
			("JAM_NODE_BIN (the polkajam node binary)", &binaries.jam_node),
			("PARASIM_TOOL_BIN (the parasim-tool CLI)", &binaries.parasim_tool),
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
		]
		.iter()
		.filter(|(_, path)| !path.exists())
		.map(|(what, path)| format!("  {what}: {}", path.display()))
		.collect();

		if missing.is_empty() {
			Ok(binaries)
		} else {
			Err(format!("missing artifacts:\n{}", missing.join("\n")))
		}
	}
}

/// Resolve the artifacts, or print why the test is being skipped and return `None`.
pub fn binaries_or_skip(test: &str) -> Option<Binaries> {
	match Binaries::from_env() {
		Ok(binaries) => Some(binaries),
		Err(reason) => {
			// Not `log::warn!`: this has to be readable without a logger, and both are only
			// visible under `--nocapture` anyway.
			eprintln!("SKIP {test}: {reason}");
			None
		},
	}
}
