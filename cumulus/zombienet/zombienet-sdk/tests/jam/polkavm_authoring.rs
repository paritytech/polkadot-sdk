// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

#![cfg(feature = "jam")]

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

use anyhow::Context;
use cumulus_jam_zombienet_tests::{
	chain_spec,
	env::{init_logger, NodeArtifacts},
	genesis_build::polkavm_env,
	network::{free_port, work_dir},
	rpc::CollatorRpc,
};
use std::{
	fs::File,
	process::{Child, Command, Stdio},
	time::Duration,
};
use tokio::time::{sleep, Instant};

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
	init_logger();

	let test = "dev_node_on_polkavm_blob_rotates_keys_and_authors_blocks";
	let artifacts = NodeArtifacts::from_env().map_err(|reason| anyhow::anyhow!("{reason}"))?;

	let (work_dir, _keep) = work_dir(test)?;
	log::info!("work dir: {}", work_dir.display());

	// The chain spec: the PolkaVM blob as `:code`, built by the node binary itself, exactly as
	// `chain_spec::build` builds the collators' specs — the only difference is the relay-chain
	// id, which a dev node never connects to. The spec is built under the same PolkaVM env the
	// node runs under: chain-spec-builder has to execute the PVM blob to read its genesis preset.
	let spec_path = work_dir.join("dev-spec.json");
	chain_spec::generate(&artifacts.omni_node, &artifacts.runtime, &spec_path, "dev", 0)?;
	log::info!(
		"chain spec {} embeds the PolkaVM blob as :code — the node will have no other runtime",
		spec_path.display()
	);

	// The node itself. No override is passed: the only runtime it can execute is the PolkaVM
	// blob the spec embeds, enabled by the SUBSTRATE_ENABLE_POLKAVM=1 in the environment.
	let base_path = work_dir.join("alice");
	let p2p_port = free_port()?;
	let rpc_port = free_port()?;
	let prometheus_port = free_port()?;

	let log_path = work_dir.join("alice.log");
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
		work_dir.display()
	);

	// The single ws://127.0.0.1:<rpc_port> endpoint serves every method this gate needs.
	let url = format!("ws://127.0.0.1:{rpc_port}");
	let deadline = Instant::now() + Duration::from_secs(START_DEADLINE_SECS);
	let rpc = match CollatorRpc::connect(&url, deadline).await {
		Ok(rpc) => rpc,
		Err(error) => {
			// Best effort: a node that already exited surfaces its status and log tail; a live
			// one that never answered surfaces the connect error itself.
			if let Some(status) = child.0.try_wait()? {
				let log_str = log_path.display();
				let tail = std::fs::read(&log_path)?;
				let tail_str = String::from_utf8_lossy(&tail);
				return Err(anyhow::anyhow!(
					"the dev node exited ({status}) before its RPC was up — see {log_str}: \
					 {tail_str}"
				));
			}
			return Err(anyhow::anyhow!("{error}"));
		},
	};

	// First adversarial class — the blob the node loaded: `:code` on chain must be byte-identical
	// to the freshly built blob file and start with the `PVM\0` magic. A stale or WASM `:code`
	// fails here with a named message instead of authoring for the wrong reason.
	let on_chain_code = rpc.storage(CODE_KEY).await?;
	let built_blob = std::fs::read(&artifacts.runtime)?;
	anyhow::ensure!(
		on_chain_code == built_blob,
		":code on chain is not the built PolkaVM blob: on-chain {} bytes, built {} bytes",
		on_chain_code.len(),
		built_blob.len()
	);
	let code_head = [on_chain_code[0], on_chain_code[1], on_chain_code[2], on_chain_code[3]];
	anyhow::ensure!(
		&code_head == b"PVM\0",
		":code on chain does not start with PVM\\0; the node is not running a PolkaVM runtime"
	);
	log::info!(
		"node's on-chain :code is the PolkaVM blob (PVM\\0, {} bytes) — the PVM executor is what \
		 runs it (SUBSTRATE_ENABLE_POLKAVM=1 is in the environment)",
		on_chain_code.len()
	);
	// Evidence artifacts for the gate record. These write the on-chain code, the rotateKeys
	// response and the reached height into the work dir, so the run's RPC responses can be
	// audited (and hashed against the built blob) after the node is gone.
	std::fs::write(work_dir.join("code-on-chain.polkavm"), &on_chain_code)?;

	// The gate's first half: `author_rotateKeys` runs `SessionKeys_generate_session_keys`, which
	// on riscv forwards sr25519 generate/public_keys/sign to the node's keystore. This is the
	// call that trapped with `needs node-side state` before the forwarding.
	let rotated = rpc.rotate_keys().await?;
	anyhow::ensure!(
		!rotated.is_empty(),
		"author_rotateKeys returned no keys — the SR25519 session key generation produced \
		 nothing"
	);
	log::info!("author_rotateKeys -> {rotated}");
	std::fs::write(work_dir.join("author-rotateKeys.hex"), &rotated[..])?;

	// The gate's second half: the dev node authors one block per 3 s; three blocks must appear.
	// Height is polled, never slept-to-a-race: the assertion is that the chain PROGRESSED.
	let author_deadline = Instant::now() + Duration::from_secs(AUTHOR_DEADLINE_SECS);
	let mut best = rpc.height().await?.best;
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
		best = rpc.height().await?.best;
	}
	log::info!("the dev node authored {best} blocks on the PolkaVM runtime");
	std::fs::write(work_dir.join("best-height.txt"), best.to_string())?;

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

/// Kill the child when the test drops (`Child::kill` on every exit path — panic, assertion or
/// clean end).
pub struct KillChildOnDrop(pub Child);

impl Drop for KillChildOnDrop {
	fn drop(&mut self) {
		let _ = self.0.kill();
	}
}
