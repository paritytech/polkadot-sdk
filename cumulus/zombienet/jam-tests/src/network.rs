// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Pure helpers the JAM tests share while each declares its own zombienet network with the SDK's
//! [`zombienet_sdk::NetworkConfigBuilder`]. Nothing here touches the builder: these are the run's
//! work dir, the node names and counts, and the collator arguments, all computed the same way for
//! every test.

use crate::para::{Para, PARACHAIN_SERVICE_ID};
use anyhow::Context;
use std::{
	collections::HashMap,
	path::{Path, PathBuf},
	sync::atomic::{AtomicU64, Ordering},
	time::{SystemTime, UNIX_EPOCH},
};
use zombienet_sdk::Arg;

/// The ordinary JAM node every para's collators point their `--jam-rpc-urls` at by default.
pub const ORDINARY_NODE: &str = "jam-or";

/// The validators each JAM core carries; a network with `cores` cores has `cores * 3`.
pub const VALIDATORS_PER_CORE: usize = 3;

/// The default collator `--jam-rpc-urls`: the ordinary JAM node's RPC, resolved by zombienet at
/// spawn time.
pub const DEFAULT_JAM_RPC_URL: &str = "ws://{{ZOMBIE:jam-or:rpc_uri}}";

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
pub fn run_name(test_name: &str) -> String {
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
/// `authoring` is the one difference between a collator and a full node: only a collator gets
/// `--force-authoring`. A full node is not in the authority set, so it only syncs. A collator
/// named in `jam_rpc_url_overrides` gets that URL instead of [`DEFAULT_JAM_RPC_URL`].
pub fn collator_args(
	authorizer_blob: &str,
	jam_rpc_url_overrides: &HashMap<String, String>,
	name: &str,
	authoring: bool,
) -> Vec<Arg> {
	let mut args: Vec<Arg> = Vec::new();
	if authoring {
		args.push("--force-authoring".into());
	}
	let jam_rpc_url = jam_rpc_url_overrides
		.get(name)
		.map(String::as_str)
		.unwrap_or(DEFAULT_JAM_RPC_URL);
	args.extend([
		Arg::Option("--jam-rpc-urls".into(), jam_rpc_url.into()),
		Arg::Option("--jam-service-id".into(), PARACHAIN_SERVICE_ID.to_string()),
		Arg::Option("--jam-authorizer-blob".into(), authorizer_blob.to_string()),
		"--no-mdns".into(),
		// A 7 MB code upgrade hex-encodes to ~14 MB, against the 15 MiB default.
		Arg::Option("--rpc-max-request-size".into(), "32".into()),
		Arg::Option("--rpc-max-response-size".into(), "32".into()),
		"-ljam-collator=debug,jam-rpc-interface=debug".into(),
	]);
	args
}
