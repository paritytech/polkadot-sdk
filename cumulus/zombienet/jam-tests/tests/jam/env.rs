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
	/// The compiled parasim service blob. Retained for the parasim tooling and the README's
	/// toy runs; the genesis no longer creates the service from it.
	///
	/// `None` unless `PARASIM_BLOB` is set: the real-service path does not need it, so an absent
	/// blob does not skip the target test. A set-but-missing path is still a typo and every test
	/// says so, same as `PARASIM_TOOL_BIN`.
	pub parasim_blob: Option<PathBuf>,
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
	/// updated — the current name misleads, but the rename touches network.rs and the README.
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

impl Binaries {
	/// Resolve every artifact from the environment, or return the human-readable list of what is
	/// missing so the caller can skip the test with an explanation.
	pub fn from_env() -> Result<Self, String> {
		let root = workspace_root();
		let binaries = Binaries {
			jam_node: from_env_or("JAM_NODE_BIN", PathBuf::new),
			genspec_node: std::env::var_os("JAM_GENSPEC_BIN").map(PathBuf::from),
			parasim_tool: std::env::var_os("PARASIM_TOOL_BIN").map(PathBuf::from),
			parasim_blob: std::env::var_os("PARASIM_BLOB").map(PathBuf::from),
			parachain_service_blob: from_env_or("PARACHAIN_SERVICE_BLOB", PathBuf::new),
			authorizer_blob: from_env_or("AUTHORIZER_BLOB", PathBuf::new),
			omni_node: from_env_or("OMNI_NODE_BIN", || {
				root.join("target/release/polkadot-omni-node")
			}),
			runtime_wasm: from_env_or("RUNTIME_WASM", || {
				root.join(
					"target/release/wbuild/parachain-template-runtime/\
					 parachain_template_runtime.compact.compressed.wasm",
				)
			}),
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
		// Optional: the real-service path does not need parasim's blob. A set-but-missing path is
		// still a typo — every test says so, same as PARASIM_TOOL_BIN.
		if let Some(blob) = &binaries.parasim_blob {
			wanted.push(("PARASIM_BLOB (the parasim service .jam blob)", blob));
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
fn check_polkavm_format(path: &Path) -> Result<(), String> {
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

	/// The mandatory set for the real-service path resolves when all required files are present
	/// and `PARASIM_BLOB` is absent — the target test must not skip merely because the mock blob
	/// is missing.
	///
	/// Adversarial probe: one mandatory artifact is then removed from disk and `missing()` fires,
	/// proving the happy-path assertion is load-bearing rather than vacuous.
	#[test]
	fn mandatory_set_resolves_with_parasim_blob_absent() {
		let dir = tempfile::tempdir().expect("temp dir; qed");

		let jam_node = dir.path().join("polkajam");
		let service_blob = dir.path().join("parachain-service.jam");
		let auth_blob = dir.path().join("authorizer.jam");
		let omni_node = dir.path().join("polkadot-omni-node");
		let runtime = dir.path().join("runtime.polkavm");

		for path in [&jam_node, &service_blob, &auth_blob, &omni_node] {
			std::fs::write(path, b"placeholder").expect("write; qed");
		}
		std::fs::write(&runtime, b"PVM\0placeholder").expect("write; qed");

		// Happy path: the real-service mandatory set, with PARASIM_BLOB absent.
		let wanted: Vec<(&str, &PathBuf)> = vec![
			("JAM_NODE_BIN", &jam_node),
			("PARACHAIN_SERVICE_BLOB", &service_blob),
			("AUTHORIZER_BLOB", &auth_blob),
			("OMNI_NODE_BIN", &omni_node),
			("RUNTIME_WASM", &runtime),
			// PARASIM_BLOB is deliberately not in this list.
		];
		assert!(
			missing(&wanted).is_none(),
			"mandatory set must resolve when all required files exist and PARASIM_BLOB is absent",
		);

		// Adversarial: delete one mandatory artifact — missing() must fire.
		// Proves the assertion above is load-bearing, not a vacuous pass.
		std::fs::remove_file(&service_blob).expect("remove; qed");
		assert!(
			missing(&wanted).is_some(),
			"missing() must fire when PARACHAIN_SERVICE_BLOB is absent — \
			 the no-skip assertion is load-bearing, not vacuous",
		);
	}
}
