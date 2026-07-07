// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Shared network config + chain-spec resolution for the bitswap zombienet tests: the consumer
//! (`e2e`) and the snapshot generator (`generate_snapshot`). The single [`network_config`] is the
//! whole point of the snapshot API — the two tests spawn the *same* topology, differing only in
//! whether the nodes load DB snapshots (consumer) or start from genesis (generator).

#![allow(dead_code)] // A couple of items are used by only one of the two tests.

use super::payloads::{PARA_BINARY, PARA_ID, RELAY_BINARY, RELAY_CHAIN};
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder};

/// Bulletin parachain chain-spec. Not vendored in polkadot-sdk — referenced from smoldot's
/// checked-in copy, which is generated upstream by
/// `polkadot-bulletin-chain/scripts/create_bulletin_westend_spec.sh`. Pinned to a commit so the
/// URL is stable. Override with a local path (or newer URL) via [`ENV_CHAIN_SPEC`] when iterating
/// on a newer bulletin runtime.
pub const CHAIN_SPEC_BULLETIN: &str = "https://raw.githubusercontent.com/paritytech/smoldot/386a1f9a/e2e-tests/chain-specs/bulletin-westend-local-spec.json";

/// Env var overriding [`CHAIN_SPEC_BULLETIN`] with a local path or alternative URL.
pub const ENV_CHAIN_SPEC: &str = "BULLETIN_CHAIN_SPEC_OVERRIDE";

/// Log directives applied to every parachain node in both tests.
const NODE_LOG_CONFIG: &str =
	"-lbitswap=trace,sub-libp2p::bitswap=trace,rpc-spec-v2::bitswap=trace,sync=debug";

/// DB snapshot sources for the consumer. Each is a URL or local path accepted by
/// `with_db_snapshot`. Passing `None` to [`network_config`] instead spawns fresh-from-genesis for
/// the generator.
pub struct Snapshots {
	pub relay: String,
	pub bulletin_full: String,
}

/// Build the relay + 2-collator bulletin network config. With `snaps = Some(..)` the nodes load
/// the given DB snapshots (consumer); with `None` they start from genesis (generator). The relay
/// snapshot loads into both validators, `bulletin_full` into both collators.
pub fn network_config(chain_spec: &Path, snaps: Option<&Snapshots>) -> Result<NetworkConfig> {
	let chain_spec_str = chain_spec.to_str().ok_or_else(|| anyhow!("non-utf8 chain spec path"))?;
	// `Option<&str>` is `Copy`, so these can be reused across the node closures below.
	let relay = snaps.map(|s| s.relay.as_str());
	let full = snaps.map(|s| s.bulletin_full.as_str());

	NetworkConfigBuilder::new()
		.with_relaychain(|rc| {
			rc.with_chain(RELAY_CHAIN)
				.with_default_command(RELAY_BINARY)
				.with_validator(|n| {
					n.with_name("alice").bootnode(true).with_optional_db_snapshot(relay)
				})
				.with_validator(|n| {
					n.with_name("bob").bootnode(true).with_optional_db_snapshot(relay)
				})
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_chain_spec_path(chain_spec_str)
				.cumulus_based(true)
				.with_default_args(vec![
					"--ipfs-server".into(),
					NODE_LOG_CONFIG.into(),
					("--relay-chain-rpc-urls", "{{ZOMBIE:alice:ws_uri}}").into(),
				])
				.with_collator(|c| {
					c.with_name("collator-1")
						.validator(true)
						.bootnode(true)
						.with_command(PARA_BINARY)
						.with_optional_db_snapshot(full)
				})
				.with_collator(|c| {
					c.with_name("collator-2")
						.validator(true)
						.bootnode(true)
						.with_command(PARA_BINARY)
						.with_optional_db_snapshot(full)
				})
		})
		.with_global_settings(|settings| match std::env::var("ZOMBIENET_SDK_BASE_DIR") {
			Ok(base_dir) => settings.with_base_dir(base_dir),
			Err(_) => settings,
		})
		.build()
		.map_err(|e| anyhow!("network config errors: {e:?}"))
}

/// Resolve the chain-spec source to a local path. If the override env var holds a path that
/// exists on disk, use it directly. Otherwise treat the value (or the baked-in default) as an
/// HTTP(S) URL and shell out to `curl` to download it to a tempfile.
pub async fn resolve_chain_spec() -> Result<PathBuf> {
	let value = std::env::var(ENV_CHAIN_SPEC).unwrap_or_else(|_| CHAIN_SPEC_BULLETIN.to_string());
	let as_path = PathBuf::from(&value);
	if as_path.exists() {
		return Ok(as_path);
	}
	if !(value.starts_with("http://") || value.starts_with("https://")) {
		return Err(anyhow!(
			"chain-spec source {value:?} is neither an existing path nor an http(s) URL"
		));
	}

	let dest = std::env::temp_dir()
		.join(format!("bulletin-westend-local-spec-{}.json", std::process::id()));
	let status = std::process::Command::new("curl")
		.args(["--fail", "--silent", "--show-error", "--location", "--output"])
		.arg(&dest)
		.arg(&value)
		.status()
		.map_err(|e| anyhow!("failed to spawn curl for chain-spec download: {e}"))?;
	if !status.success() {
		return Err(anyhow!("curl exit {status} fetching chain-spec from {value}"));
	}
	Ok(dest)
}
