// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! The external artifacts a JAM collator test needs, and the gate that reports any of them
//! missing.

use std::path::{Path, PathBuf};

/// Install the process logger at the tests' `info` filter, ignoring a logger already set.
///
/// Every test entry point calls this before it does anything; a second call in the same process
/// is a no-op because `env_logger` refuses to install twice.
pub fn init_logger() {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
}

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
	/// every other test asserts on is read straight off the JAM node's RPC — so only those two
	/// tests need it and nothing else does.
	pub parasim_tool: Option<PathBuf>,
	/// The compiled real parachain-service blob, which genesis creates the service from.
	pub parachain_service_blob: PathBuf,
	/// The compiled AURA authorizer blob. Only its hash ever reaches the chain, but the collators
	/// and the genesis that queues their core have to hash the same bytes, so this one file is
	/// handed to both.
	pub authorizer_blob: PathBuf,
	/// The collator binary.
	pub omni_node: PathBuf,
	/// The parachain runtime blob the collators run **and** the para's JAM validation code.
	///
	/// Must be the **PolkaVM** build (`PVM\0` magic), not the WASM one the env var name and the
	/// default path imply. [`Binaries::from_env`] rejects a WASM blob with a named diagnostic so
	/// the failure appears at start-up, not eight minutes later as a para head that never moves.
	///
	/// Set `RUNTIME_WASM` to the PolkaVM output:
	/// `SUBSTRATE_RUNTIME_TARGET=riscv cargo build --release -p parachain-template-runtime`
	///
	/// TODO(T11): rename field and env var to `runtime_pvf`/`RUNTIME_PVF` once the README is
	/// updated — the current name misleads, but the rename touches the README.
	pub runtime_wasm: PathBuf,
}

/// Where this crate sits relative to the workspace root, so the defaults can find `target/`.
fn workspace_root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../..")
		.canonicalize()
		.unwrap_or_default()
}

fn from_env_or(var: &str, default: impl FnOnce() -> PathBuf) -> PathBuf {
	std::env::var_os(var).map(PathBuf::from).unwrap_or_else(default)
}

/// The two node-side artifacts a plain dev-node run needs: the node binary and the PolkaVM
/// runtime blob it embeds as `:code`.
#[derive(Clone, Debug)]
pub struct NodeArtifacts {
	/// `polkadot-omni-node`, which is also the `chain-spec-builder` that makes the spec.
	pub omni_node: PathBuf,
	/// The PolkaVM build of the runtime (`PVM\0` magic), the node's only runtime.
	pub runtime: PathBuf,
}

impl NodeArtifacts {
	/// Resolve from the environment without touching the disk: `OMNI_NODE_BIN` and `RUNTIME_WASM`,
	/// the latter falling back to `runtime_default`.
	fn resolve(runtime_default: PathBuf) -> Self {
		let root = workspace_root();
		NodeArtifacts {
			omni_node: from_env_or("OMNI_NODE_BIN", || {
				root.join("target/release/polkadot-omni-node")
			}),
			runtime: from_env_or("RUNTIME_WASM", || runtime_default),
		}
	}

	/// Resolve the artifacts a plain dev-node run needs, or report which are missing or not a
	/// PolkaVM program.
	pub fn from_env() -> Result<Self, String> {
		let artifacts = NodeArtifacts::resolve(workspace_root().join(
			"target/release/rbuild/parachain-template-runtime/\
			 parachain-template-runtime-blob.polkavm",
		));

		let wanted: Vec<(&str, &PathBuf)> = vec![
			("OMNI_NODE_BIN (cargo build --release -p polkadot-omni-node)", &artifacts.omni_node),
			(
				"RUNTIME_WASM (SUBSTRATE_RUNTIME_TARGET=riscv cargo build --release \
				 -p parachain-template-runtime — the PolkaVM blob, not the WASM one)",
				&artifacts.runtime,
			),
		];
		if let Some(reason) = missing(&wanted) {
			return Err(reason);
		}
		check_polkavm_format(&artifacts.runtime)?;
		Ok(artifacts)
	}
}

impl Binaries {
	/// Resolve every artifact from the environment, or return the human-readable list of what is
	/// missing so the caller can skip the test with an explanation.
	pub fn from_env() -> Result<Self, String> {
		let root = workspace_root();
		// The node binary and the runtime blob are resolved exactly as a plain dev-node run
		// resolves them; here they are only two more entries in the combined list below, so an
		// absent one never short-circuits the report of the others.
		let node = NodeArtifacts::resolve(root.join(
			"target/release/wbuild/parachain-template-runtime/\
			 parachain_template_runtime.compact.compressed.wasm",
		));
		let binaries = Binaries {
			jam_node: from_env_or("JAM_NODE_BIN", PathBuf::new),
			genspec_node: std::env::var_os("JAM_GENSPEC_BIN").map(PathBuf::from),
			parasim_tool: std::env::var_os("PARASIM_TOOL_BIN").map(PathBuf::from),
			parachain_service_blob: from_env_or("PARACHAIN_SERVICE_BLOB", PathBuf::new),
			authorizer_blob: from_env_or("AUTHORIZER_BLOB", PathBuf::new),
			omni_node: node.omni_node,
			runtime_wasm: node.runtime,
		};

		let mut wanted: Vec<(&str, &PathBuf)> = vec![
			("JAM_NODE_BIN (the polkajam node binary)", &binaries.jam_node),
			(
				"PARACHAIN_SERVICE_BLOB (the real parachain-service .jam blob, which genesis \
				 creates the service from)",
				&binaries.parachain_service_blob,
			),
			(
				"AUTHORIZER_BLOB (parachain-authorizer-sr25519.jam, the scheme the template \
				 runtime's AuraId asks for)",
				&binaries.authorizer_blob,
			),
			("OMNI_NODE_BIN (cargo build --release -p polkadot-omni-node)", &binaries.omni_node),
			(
				"RUNTIME_WASM (SUBSTRATE_RUNTIME_TARGET=riscv cargo build --release \
				 -p parachain-template-runtime — the PolkaVM blob, not the WASM one)",
				&binaries.runtime_wasm,
			),
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

		if let Some(reason) = missing(&wanted) {
			return Err(reason);
		}

		// Format guard: `runtime_wasm` is the para's JAM validation code AND the collators'
		// runtime, and must be the PolkaVM build. A WASM blob here records a WASM hash in
		// genesis and the real service refuses every candidate — silent 8-minute stall. The
		// WASM default path passes the `missing` check above; this catches it before the run
		// starts.
		check_polkavm_format(&binaries.runtime_wasm)?;

		Ok(binaries)
	}
}

/// Reject any runtime blob that is not a PolkaVM program (`PVM\0` magic).
///
/// A WASM blob at `RUNTIME_WASM` records a WASM hash in genesis; the real service refuses every
/// candidate and the run stalls for 8 minutes with no log. The WASM default path is on disk and
/// passes the missing-artifact check — this guard makes the failure immediate and named.
///
/// Also applied to every per-para runtime override, when a para chooses its own blob.
pub(crate) fn check_polkavm_format(path: &Path) -> Result<(), String> {
	use std::io::Read;
	let mut header = [0u8; 4];
	let n = std::fs::File::open(path)
		.and_then(|mut f| f.read(&mut header))
		.map_err(|e| format!("RUNTIME_WASM: cannot read {}: {e}", path.display()))?;
	if n >= 4 && &header == b"PVM\0" {
		return Ok(());
	}
	let found = if n >= 4 && &header == b"\0asm" {
		"a WASM blob (\\0asm magic)".to_string()
	} else if n < 4 {
		format!("only {n} bytes — too small to be a PolkaVM program")
	} else {
		format!("unrecognised format (first 4 bytes: {:02x?})", header)
	};
	Err(format!(
		"RUNTIME_WASM: {} is {found}; a PolkaVM blob is required (PVM\\0 magic) — \
		 build with: SUBSTRATE_RUNTIME_TARGET=riscv cargo build --release \
		 -p parachain-template-runtime",
		path.display(),
	))
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

/// Resolve the artifacts or fail, for callers that must NOT silently skip.
///
/// The error is [`Binaries::from_env`]'s own reason string, so it names exactly which env var or
/// path is missing.
pub fn binaries_or_err() -> anyhow::Result<Binaries> {
	Binaries::from_env().map_err(|reason| anyhow::anyhow!("{reason}"))
}

#[cfg(test)]
mod tests {
	use super::*;

	/// The PolkaVM format guard accepts a `PVM\0` file and rejects a `\0asm` file, with a message
	/// that names the file, says what was found, and gives the build command.
	///
	/// Adversarial: only the input file changes between the two halves — the predicate itself is
	/// not perturbed — so both a false accept and a false reject would surface as a test failure.
	#[test]
	fn polkavm_format_guard_accepts_polkavm_and_rejects_wasm() {
		let dir = tempfile::tempdir().expect("temp dir; qed");

		let pvf = dir.path().join("runtime.polkavm");
		std::fs::write(&pvf, b"PVM\0some-content").expect("write; qed");
		check_polkavm_format(&pvf).expect("a PVM\\0 blob must pass the format guard");

		let wasm = dir.path().join("runtime.wasm");
		std::fs::write(&wasm, b"\0asm\x01\0\0\0more-content").expect("write; qed");
		let err = check_polkavm_format(&wasm).expect_err("a WASM blob must be rejected");
		assert!(err.contains("WASM blob"), "error must say the blob is WASM: {err}");
		assert!(
			err.contains("PolkaVM blob is required"),
			"error must say PolkaVM is required: {err}",
		);
		assert!(
			err.contains("SUBSTRATE_RUNTIME_TARGET=riscv"),
			"error must give the build command: {err}",
		);
	}

	/// The strict resolver must fail loudly, naming the missing variable.
	#[test]
	fn binaries_or_err_names_the_missing_service_blob() {
		let saved = std::env::var_os("PARACHAIN_SERVICE_BLOB");
		std::env::remove_var("PARACHAIN_SERVICE_BLOB");

		let error =
			binaries_or_err().expect_err("an absent PARACHAIN_SERVICE_BLOB must be an error");
		let message = format!("{error}");

		if let Some(value) = saved {
			std::env::set_var("PARACHAIN_SERVICE_BLOB", value);
		}

		assert!(
			message.contains("PARACHAIN_SERVICE_BLOB"),
			"the error must name the missing variable: {message}",
		);
	}
}
