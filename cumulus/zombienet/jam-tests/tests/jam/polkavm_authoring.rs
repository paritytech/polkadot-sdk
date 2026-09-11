// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! The task-3 gate: a real node executes the PolkaVM runtime blob and authors blocks.
//!
//! This is the dev-node half of the proof that the whole suite now runs the PolkaVM blob:
//! since task 4 the collator tests execute the very blob JAM validates with (the harness no
//! longer hands the collators a WASM override — see the crate README, "Why the runtime is
//! built once"). This test is the minimal version of that path: no chain, no JAM network, no
//! collator flags — just the node's only runtime, the PolkaVM blob the chain spec embeds as
//! `:code`. It must come up, serve
//! `author_rotateKeys` (which takes the keystore through `SessionKeys_generate_session_keys`
//! and the version-2 sr25519 crypto host calls) and author blocks on its own, proving the riscv
//! `sp_io::native::crypto` forwarding lands in the node's keystore instead of trapping.
//!
//! The node is a plain `polkadot-omni-node --dev --chain <spec>`: dev mode needs no relay and
//! produces a block every `dev_block_time` milliseconds with manual seal. `--chain` takes the
//! spec this test builds with `chain-spec-builder create ... -r <polkavm-blob> named-preset
//! development`, exactly the way `chain_spec.rs` builds specs for the collator tests, so the
//! `:code` the node boots from is the same PolkaVM blob every other test only validates with.

use super::network::polkavm_env;
use anyhow::Context;
use jsonrpsee::{
	core::client::ClientT,
	rpc_params,
	ws_client::{WsClient, WsClientBuilder},
};
use serde_json::Value;
use sp_core::bytes::from_hex;
use std::{
	fs::File,
	io::Read,
	path::{Path, PathBuf},
	process::{Child, Command, Stdio},
	sync::atomic::{AtomicU16, Ordering},
	time::Duration,
};
use tokio::time::{sleep, Instant};

/// The crate's binaries, restricted to the two a dev node on the PolkaVM blob needs.
///
/// Deliberately not [`env::Binaries`]: that set demands the polkajam binaries and the
/// parachain-service blobs, which a standalone authoring node never touches.
struct Artifacts {
	/// `polkadot-omni-node`, which is also the `chain-spec-builder` that makes the spec.
	omni_node: PathBuf,
	/// The PolkaVM build of the parachain runtime (`PVM\0` magic), embedded as the spec's
	/// `:code` — and the only runtime the node ever executes.
	runtime: PathBuf,
}

/// Dev-node ports sit above the collator range and away from the 9944/30333 defaults, so a node
/// the user is running themselves is never disturbed. The dev node uses three consecutive ports,
/// like a collator does.
const FIRST_PORT: u16 = 43100;
const PORTS_PER_NODE: u16 = 3;
static NEXT_PORT: AtomicU16 = AtomicU16::new(FIRST_PORT);

/// How long the node may take to come up and serve RPC.
const START_DEADLINE_SECS: u64 = 120;
/// How long the node may take to author the gate's three blocks after that. The dev block time
/// defaults to 3000 ms, so three blocks are nine seconds of production once it runs; the slack
/// covers the interpreter's startup cost and a loaded machine.
const AUTHOR_DEADLINE_SECS: u64 = 240;

/// The `:code` well-known key. `":" | "code"` in hex, as JSON-RPC wants it.
const CODE_KEY: &str = "0x3a636f6465";

/// The gate, as a `cargo test` filter:
/// `jam::dev_node_on_polkavm_blob_rotates_keys_and_authors_blocks`.
#[tokio::test(flavor = "multi_thread")]
async fn dev_node_on_polkavm_blob_rotates_keys_and_authors_blocks() -> Result<(), anyhow::Error> {
	let test = "dev_node_on_polkavm_blob_rotates_keys_and_authors_blocks";
	let Some(artifacts) = artifacts_or_skip(test) else {
		return Ok(());
	};

	let work_dir = WorkDir::create(&test)?;
	log::info!("work dir: {}", work_dir.path().display());

	// The chain spec: the PolkaVM blob as `:code`, built by the node binary itself, exactly as
	// `chain_spec::build` builds the collators' specs — the only difference is the relay-chain
	// id, which a dev node never connects to. The spec is built under the same PolkaVM env the
	// node runs under: chain-spec-builder has to execute the PVM blob to read its genesis preset.
	let spec_path = work_dir.path().join("dev-spec.json");
	let status = Command::new(&artifacts.omni_node)
		.envs(polkavm_env())
		.args(["chain-spec-builder", "--chain-spec-path"])
		.arg(&spec_path)
		.args(["create", "--relay-chain", "dev", "--para-id", "0", "-r"])
		.arg(&artifacts.runtime)
		.args(["named-preset", "development"])
		.status()
		.with_context(|| format!("running {} chain-spec-builder", artifacts.omni_node.display()))?;
	anyhow::ensure!(status.success(), "chain-spec-builder failed: {status}");
	log::info!(
		"chain spec {} embeds the PolkaVM blob as :code — the node will have no other runtime",
		spec_path.display()
	);

	// The node itself. No override is passed: the only runtime it can execute is the PolkaVM
	// blob the spec embeds, enabled by the SUBSTRATE_ENABLE_POLKAVM=1 in the environment.
	let base_path = work_dir.path().join("alice");
	let first_port = NEXT_PORT.fetch_add(PORTS_PER_NODE, Ordering::Relaxed);
	let p2p_port = first_port;
	let rpc_port = p2p_port + 1;
	let prometheus_port = p2p_port + 2;

	let log_path = work_dir.path().join("alice.log");
	let log_file = File::create(&log_path)?;

	let mut command = Command::new(&artifacts.omni_node);
	command
		.envs(polkavm_env())
		.arg("--dev")
		.arg("--chain")
		.arg(&spec_path)
		.arg("--base-path")
		.arg(&base_path)
		.args(["--port", &p2p_port.to_string()])
		.args(["--rpc-port", &rpc_port.to_string()])
		.args(["--prometheus-port", &prometheus_port.to_string()])
		.arg("--no-mdns")
		.args(["-l", "jam-collator=debug,jam-rpc-interface=debug"])
		.stdout(Stdio::from(log_file.try_clone()?))
		.stderr(Stdio::from(log_file));

	let mut child = KillChildOnDrop(
		command
			.spawn()
			.with_context(|| format!("spawning the dev node {}", artifacts.omni_node.display()))?,
	);
	log::info!(
		"dev node: rpc ws://127.0.0.1:{rpc_port}, p2p {p2p_port}, log {}, work dir {}",
		log_path.display(),
		work_dir.path().display()
	);

	// The single ws://127.0.0.1:<rpc_port> endpoint serves every method this gate needs.
	let deadline = Instant::now() + Duration::from_secs(START_DEADLINE_SECS);
	let url = format!("ws://127.0.0.1:{rpc_port}");
	let mut rpc = None;
	while Instant::now() < deadline {
		match WsClientBuilder::default()
			.max_response_size(128 * 1024 * 1024)
			.build(&url)
			.await
		{
			Ok(client) => {
				rpc = Some(Rpc { client });
				break;
			},
			Err(error) => {
				if let Some(status) = child.0.try_wait()? {
					let log_str = log_path.display();
					let tail = std::fs::read(&log_path)?;
					let tail_str = String::from_utf8_lossy(&tail);
					return Err(anyhow::anyhow!(
						"the dev node exited ({status}) before its RPC was up — see {log_str}: \
						 {tail_str}"
					));
				}
				sleep(Duration::from_millis(500)).await;
				if Instant::now() >= deadline {
					return Err(anyhow::anyhow!(
						"{url} never accepted a connection; last error: {error:?}"
					));
				}
			},
		}
	}

	let Some(rpc) = rpc else {
		return Err(anyhow::anyhow!("could not connect to {url}"));
	};

	// First adversarial class — the blob the node loaded: `:code` on chain must be byte-identical
	// to the freshly built blob file and start with the `PVM\0` magic. A stale or WASM `:code`
	// fails here with a named message instead of authoring for the wrong reason.
	let on_chain_code = rpc.storage(CODE_KEY).await?;
	let built_blob = std::fs::read(&artifacts.runtime)?;
	anyhow::ensure!(
		on_chain_code.bytes == built_blob,
		":code on chain is not the built PolkaVM blob: on-chain {} bytes, built {} bytes",
		on_chain_code.bytes.len(),
		built_blob.len()
	);
	let code_head = [
		on_chain_code.bytes[0],
		on_chain_code.bytes[1],
		on_chain_code.bytes[2],
		on_chain_code.bytes[3],
	];
	anyhow::ensure!(
		&code_head == b"PVM\0",
		":code on chain does not start with PVM\\0; the node is not running a PolkaVM runtime"
	);
	log::info!(
		"node's on-chain :code is the PolkaVM blob (PVM\\0, {} bytes) — the PVM executor is what \
		 runs it (SUBSTRATE_ENABLE_POLKAVM=1 is in the environment)",
		on_chain_code.bytes.len()
	);
	// Evidence artifacts for the gate record. These write the on-chain code, the rotateKeys
	// response and the reached height into the work dir, so the run's RPC responses can be
	// audited (and hashed against the built blob) after the node is gone.
	std::fs::write(work_dir.path().join("code-on-chain.polkavm"), on_chain_code.bytes)?;

	// The gate's first half: `author_rotateKeys` runs `SessionKeys_generate_session_keys`, which
	// on riscv forwards sr25519 generate/public_keys/sign to the node's keystore. This is the
	// call that trapped with `needs node-side state` before the forwarding.
	let rotated_hex = rpc.rotate_keys().await?;
	let rotated = from_hex(&rotated_hex[..])
		.with_context(|| format!("decoding the rotated keys {rotated_hex} as hex"))?;
	anyhow::ensure!(
		!rotated.is_empty(),
		"author_rotateKeys returned no keys — the SR25519 session key generation produced \
		 nothing"
	);
	log::info!("author_rotateKeys -> {} ({} bytes)", rotated_hex, rotated.len());
	std::fs::write(work_dir.path().join("author-rotateKeys.hex"), &rotated_hex[..])?;

	// The gate's second half: the dev node authors one block per 3 s; three blocks must appear.
	// Height is polled, never slept-to-a-race: the assertion is that the chain PROGRESSED.
	let author_deadline = Instant::now() + Duration::from_secs(AUTHOR_DEADLINE_SECS);
	let mut best = rpc.height().await?;
	loop {
		log::info!("best head height {best}");
		if best >= 3 {
			break;
		}
		if let Some(status) = child.0.try_wait()? {
			let log_str = log_path.display();
			let tail = std::fs::read(&log_path)?;
			let tail_str = String::from_utf8_lossy(&tail);
			return Err(anyhow::anyhow!(
				"the dev node exited ({status}) at height {best} — see {log_str}: {tail_str}"
			));
		}
		if Instant::now() >= author_deadline {
			let log_str = log_path.display();
			return Err(anyhow::anyhow!(
				"the dev node reached height {best}, not the gate's 3, in time — see {log_str}"
			));
		}
		sleep(Duration::from_secs(5)).await;
		best = rpc.height().await?;
	}
	log::info!("the dev node authored {best} blocks on the PolkaVM runtime");
	std::fs::write(work_dir.path().join("best-height.txt"), best.to_string())?;

	// No panic of any kind may cross the log: a `needs node-side state` trap or a missing
	// keystore panic is exactly what the plan re-scopes on, so the gate must fail with the
	// verbatim line when one appears.
	let log_bytes = std::fs::read(&log_path)?;
	let log_text = String::from_utf8_lossy(&log_bytes);
	let poison: Vec<&str> = vec![
		"needs node-side state",
		"No `keystore` associated",
		"explicit trap",
		"panicked at",
		"Trap at",
		"found a PolkaVM runtime blob; set the 'SUBSTRATE_ENABLE_POLKAVM'",
	];
	for marker in poison {
		anyhow::ensure!(
			!log_text.contains(marker),
			"the dev node's log contains a failure marker {marker:?}:\n{log_text}"
		);
	}

	Ok(())
}

/// Resolve the two artifacts, or print why the test is being skipped and return `None`.
fn artifacts_or_skip(test: &str) -> Option<Artifacts> {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../..")
		.canonicalize()
		.unwrap_or_default();

	let omni_node = std::env::var_os("OMNI_NODE_BIN")
		.map(PathBuf::from)
		.unwrap_or_else(|| root.join("target/release/polkadot-omni-node"));
	let runtime = std::env::var_os("RUNTIME_WASM").map(PathBuf::from).unwrap_or_else(|| {
		root.join(
			"target/release/rbuild/parachain-template-runtime/\
			 parachain-template-runtime-blob.polkavm",
		)
	});

	let wanted: Vec<(&str, &PathBuf)> = vec![
		("OMNI_NODE_BIN (the parachain node binary)", &omni_node),
		("RUNTIME_WASM (the PolkaVM runtime blob)", &runtime),
	];
	if let Some(reason) = missing(&wanted) {
		skip(test, &reason);
		return None;
	}
	if let Err(reason) = check_polkavm_format(&runtime) {
		skip(test, &reason);
		return None;
	}

	Some(Artifacts { omni_node, runtime })
}

/// The ones of `wanted` that are not on disk, as a skip message. `None` when they all are.
fn missing(wanted: &[(&str, &PathBuf)]) -> Option<String> {
	let missing: Vec<String> = wanted
		.iter()
		.filter(|(_, path)| !path.exists())
		.map(|(what, path)| format!("  {what}: {}", path.display()))
		.collect();
	(!missing.is_empty()).then(|| format!("missing artifacts:\n{}", missing.join("\n")))
}

/// Reject a runtime that is not a PolkaVM program — the whole point of this test is that the
/// node executes the PolkaVM build, so a WASM blob here would author fine for the wrong reason
/// and the gate would silently test nothing. Same guard `env.rs` runs on `RUNTIME_WASM`.
fn check_polkavm_format(path: &Path) -> Result<(), String> {
	let mut header = [0u8; 4];
	let n = std::fs::File::open(path)
		.and_then(|mut f| f.read(&mut header))
		.map_err(|e| format!("RUNTIME_WASM: cannot read {}: {e}", path.display()))?;
	if n >= 4 && &header == b"PVM\0" {
		return Ok(());
	}
	Err(format!(
		"RUNTIME_WASM: {} is not a PolkaVM blob; a PolkaVM blob is required (PVM\\0 magic) — \
		 build with: SUBSTRATE_RUNTIME_TARGET=riscv cargo build --release \
		 -p parachain-template-runtime",
		path.display(),
	))
}

/// Say the test is being skipped, and why.
///
/// Not `log::warn!`: this has to be readable without a logger, and both are only visible under
/// `--nocapture` anyway.
fn skip(test: &str, reason: &str) {
	eprintln!("SKIP {test}: {reason}");
}

/// One run's tree: kept under `JAM_TEST_BASE_DIR` when set (so a failed run keeps its logs),
/// else a temporary directory that is gone when the test ends. Same contract as `harness.rs`.
enum WorkDir {
	Temporary(tempfile::TempDir),
	Kept(PathBuf),
}

impl WorkDir {
	fn create(test: &str) -> anyhow::Result<Self> {
		let Some(base) = std::env::var_os("JAM_TEST_BASE_DIR") else {
			return Ok(WorkDir::Temporary(
				tempfile::Builder::new().prefix("polkavm-authoring.").tempdir()?,
			));
		};

		let started = chrono::Local::now().format("%Y%m%d-%H%M%S");
		let path = PathBuf::from(base).join(format!("polkavm-authoring-{test}-{started}"));
		std::fs::create_dir_all(&path)
			.with_context(|| format!("creating the work dir {}", path.display()))?;
		Ok(WorkDir::Kept(path))
	}

	fn path(&self) -> &Path {
		match self {
			WorkDir::Temporary(dir) => dir.path(),
			WorkDir::Kept(path) => path,
		}
	}
}

/// The RPC surface the gate needs. The node is a substrate client, so the methods are
/// substrate's: `state_getStorage` for the `:code` evidence, `author_rotateKeys` for the
/// session-key generation, and `chain_getHeader` for the authored height.
struct Rpc {
	client: WsClient,
}

/// The bytes `state_getStorage` returned.
struct StorageRead {
	bytes: Vec<u8>,
}

impl Rpc {
	/// `state_getStorage(":code", None)` at the best block — the node's actual runtime code.
	async fn storage(&self, key: &str) -> anyhow::Result<StorageRead> {
		let value: Value = self
			.client
			.request("state_getStorage", rpc_params![key, Value::Null])
			.await
			.context("state_getStorage")?;
		let hex = match value.as_str() {
			Some(h) => h,
			None => return Err(anyhow::anyhow!("state_getStorage({key}) returned {value:?}")),
		};
		let bytes = from_hex(hex).with_context(|| format!("decoding {hex} as hex"))?;
		Ok(StorageRead { bytes })
	}

	/// `author_rotateKeys` — run `SessionKeys_generate_session_keys` against the keystore and
	/// return the public session keys, as the hex the JSON-RPC `Bytes` encoding uses.
	async fn rotate_keys(&self) -> anyhow::Result<String> {
		let value: Value = self
			.client
			.request("author_rotateKeys", rpc_params![])
			.await
			.context("author_rotateKeys")?;
		value
			.as_str()
			.map(|hex| hex.to_owned())
			.with_context(|| format!("author_rotateKeys returned {value:?}"))
	}

	/// The best block header's `number` — the height the dev node has authored to.
	async fn height(&self) -> anyhow::Result<u64> {
		let header: Value = self
			.client
			.request("chain_getHeader", rpc_params![])
			.await
			.context("chain_getHeader")?;
		let number = header["number"]
			.as_str()
			.with_context(|| format!("header has no number: {header:?}"))?;
		u64::from_str_radix(number.trim_start_matches("0x"), 16)
			.with_context(|| format!("header number {number} is not hex"))
	}
}

/// Kill the child when the test drops (`Child::kill` on every exit path — panic, assertion or
/// clean end).
pub struct KillChildOnDrop(pub Child);

impl Drop for KillChildOnDrop {
	fn drop(&mut self) {
		let _ = self.0.kill();
	}
}
