// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Pure helpers [`crate::spawn`] composes into a JAM network: the run's work dir, the node names
//! and counts, the collator arguments, the free-port reservation. Nothing here touches the
//! [`zombienet_sdk::NetworkConfigBuilder`] — `spawn` is the one place that does.

use crate::para::{Para, PARACHAIN_SERVICE_ID};
use anyhow::{anyhow, Context};
use cumulus_zombienet_sdk_helpers::{PARA_BLOCK_METRIC, PARA_FINALIZED_METRIC};
use std::{
	collections::HashMap,
	path::{Path, PathBuf},
	sync::atomic::{AtomicU64, Ordering},
	time::{SystemTime, UNIX_EPOCH},
};
use zombienet_sdk::{Arg, LocalFileSystem, Network};

/// The ordinary JAM node every para's collators point their `--jam-rpc-urls` at by default.
pub const ORDINARY_NODE: &str = "jam-or";

/// The validators each JAM core carries; a network with `cores` cores has `cores * 3`.
pub const VALIDATORS_PER_CORE: usize = 3;

/// The default collator `--jam-rpc-urls`: the ordinary JAM node's RPC, resolved by zombienet at
/// spawn time.
const DEFAULT_JAM_RPC_URL: &str = "ws://{{ZOMBIE:jam-or:rpc_uri}}";

/// The run's work dir: `JAM_TEST_BASE_DIR` when set, so the logs survive, a temp dir otherwise.
///
/// The returned [`tempfile::TempDir`] holds the directory alive for the run; it is `None` when the
/// run keeps its work dir under `JAM_TEST_BASE_DIR`.
pub fn work_dir(test_name: &str) -> anyhow::Result<(PathBuf, Option<tempfile::TempDir>)> {
	let name = run_name(test_name);
	match std::env::var_os("JAM_TEST_BASE_DIR") {
		Some(base) => {
			let dir = PathBuf::from(base).join(&name);
			std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
			Ok((dir, None))
		},
		None => {
			let temp = tempfile::Builder::new()
				.prefix(&format!("{name}."))
				.tempdir()
				.context("creating the run's temporary work dir")?;
			let dir = temp.path().to_path_buf();
			Ok((dir, Some(temp)))
		},
	}
}

/// A run dir named after the test, with a stamp and a counter so parallel runs do not collide.
fn run_name(test_name: &str) -> String {
	static NEXT_RUN: AtomicU64 = AtomicU64::new(0);
	let stamp = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map(|elapsed| elapsed.as_secs())
		.unwrap_or_default();
	let seq = NEXT_RUN.fetch_add(1, Ordering::Relaxed);
	format!("jam-collator-test-{test_name}-{stamp}-{seq}")
}

/// The run's zombienet base dir, a `zombienet/` subdir of the work dir, created if needed.
pub fn base_dir(work_dir: &Path) -> anyhow::Result<PathBuf> {
	let base = work_dir.join("zombienet");
	std::fs::create_dir_all(&base).with_context(|| format!("creating {}", base.display()))?;
	Ok(base)
}

/// Copy each para's spec into the work dir, in para order, and return the copies.
///
/// `spawn_parachains` writes bootnodes into the spec it is handed, so the file a para's genesis
/// head was derived from must never be that file: the tests hand zombienet these copies and leave
/// `genesis.para_specs` — the files the heads were exported from — untouched.
pub fn copy_para_specs(
	work_dir: &Path,
	para_specs: &[PathBuf],
	paras: &[Para],
) -> anyhow::Result<Vec<PathBuf>> {
	para_specs
		.iter()
		.enumerate()
		.map(|(index, original)| {
			let copy =
				work_dir.join(format!("jam-parachain-{}-spec.zombienet.json", paras[index].id));
			std::fs::copy(original, &copy)
				.with_context(|| format!("copying {} to {}", original.display(), copy.display()))?;
			Ok(copy)
		})
		.collect()
}

/// A path as a UTF-8 string, for the SDK builders that take a command or a path by string.
pub fn path_str(path: &Path) -> anyhow::Result<String> {
	path.to_str()
		.map(str::to_string)
		.with_context(|| format!("{} is not utf-8", path.display()))
}

/// The arguments every para node is started with: where the JAM node's RPC is, which service
/// hosts the authorizer, and the blob whose hash the para's core was assigned from.
///
/// A collator named in `jam_rpc_url_overrides` gets that URL instead of `DEFAULT_JAM_RPC_URL`.
pub fn collator_args(
	authorizer_blob: &str,
	jam_rpc_url_overrides: &HashMap<String, String>,
	name: &str,
) -> Vec<Arg> {
	let mut args: Vec<Arg> = Vec::new();
	args.push("--force-authoring".into());
	let jam_rpc_url = jam_rpc_url_overrides
		.get(name)
		.map(String::as_str)
		.unwrap_or(DEFAULT_JAM_RPC_URL);
	args.extend([
		Arg::Option("--jam-rpc-urls".into(), jam_rpc_url.into()),
		Arg::Option("--jam-service-id".into(), PARACHAIN_SERVICE_ID.to_string()),
		Arg::Option("--jam-authorizer-blob".into(), authorizer_blob.to_string()),
		"--no-mdns".into(),
		// Authority discovery drops loopback/private addresses unless this is set.
		"--allow-private-ip".into(),
		// A 7 MB code upgrade hex-encodes to ~14 MB, against the 15 MiB default.
		Arg::Option("--rpc-max-request-size".into(), "32".into()),
		Arg::Option("--rpc-max-response-size".into(), "32".into()),
		"-ljam-collator=debug,jam-rpc-interface=debug,jam-package-sync=debug".into(),
	]);
	args
}

/// Reserve a free loopback port by binding and immediately dropping the listener.
pub fn free_port() -> anyhow::Result<u16> {
	let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
	Ok(listener.local_addr()?.port())
}

/// Wait, per collator of every para, for the best metric to pass `blocks` and then the finalized
/// metric to pass `finalized`. Every collator is waited on, because a set where only one collator
/// authors still has to keep the chain producing and finalizing.
pub async fn wait_for_collators(
	network: &Network<LocalFileSystem>,
	paras: &[Para],
	blocks: u64,
	finalized: u64,
	timeout_secs: u64,
) -> anyhow::Result<()> {
	for para in paras {
		for name in &para.collators {
			let node = network.get_node(name.as_str())?;

			log::info!("Waiting for collator {name} to reach best block #{blocks}");
			node.wait_metric_with_timeout(
				PARA_BLOCK_METRIC,
				|best| best >= blocks as f64,
				timeout_secs,
			)
			.await
			.map_err(|error| {
				anyhow!(
					"collator {name} did not reach best block #{blocks} in {timeout_secs}s: {error}"
				)
			})?;

			log::info!("Waiting for collator {name} to finalize block #{finalized}");
			node.wait_metric_with_timeout(
				PARA_FINALIZED_METRIC,
				|metric| metric >= finalized as f64,
				timeout_secs,
			)
			.await
			.map_err(|error| {
				anyhow!(
					"collator {name} did not finalize block #{finalized} in {timeout_secs}s: {error}"
				)
			})?;
		}
	}
	Ok(())
}
