// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! Zombienet e2e coverage of the `bitswap_unstable_*` JSON-RPC namespace.
//!
//! Brings up a 4-node network (2 westend-local validators + 2 Bulletin parachain collators) from
//! pre-built DB snapshots, then issues `bitswap_unstable_get` / `bitswap_unstable_stream` calls
//! over a direct WebSocket connection to the full-snapshot collator and asserts on responses.
//!
//! Mirrors the network shape of `smoldot/e2e-tests/tests/bulletin_batch.rs` (same snapshots, same
//! parachain id, same `polkadot-parachain` binary) — the only thing we don't share is the JS
//! light-client driver: this test talks to the node's RPC directly in Rust.
//!
//! All artifact URLs are overridable via env vars so a developer can point at local files for
//! iteration. The chain-spec is downloaded with `curl` to a tempfile when the value is an HTTP
//! URL; bare paths are passed through to zombienet as-is.

use anyhow::{anyhow, Result};
use cid::{multihash::Multihash, Cid};
use jsonrpsee::{
	core::{
		client::{ClientT, Subscription, SubscriptionClientT},
		ClientError,
	},
	rpc_params,
	ws_client::WsClientBuilder,
};
use sc_rpc_spec_v2::bitswap::api::StreamEvent;
use std::{
	path::{Path, PathBuf},
	time::Duration,
};
use zombienet_sdk::{LocalFileSystem, Network, NetworkConfigBuilder};

/// Para id of the Bulletin chain in the smoldot snapshots.
const PARA_ID: u32 = 2487;

/// Relay chain name embedded in the snapshot's chain-spec.
const RELAY_CHAIN: &str = "westend-local";
const RELAY_BINARY: &str = "polkadot";
const PARA_BINARY: &str = "polkadot-parachain";

/// Default snapshot artifacts shared with the smoldot `bulletin_batch` test. Each can be
/// overridden by the matching env var below — the override value may be either an HTTP(S) URL or
/// a local filesystem path; both are accepted by zombienet's `with_db_snapshot`.
const DB_SNAPSHOT_RELAY: &str =
	"https://storage.googleapis.com/zombienet-db-snaps/smoldot/bulletin_fetch/relay-2026-05-04.tgz";
const DB_SNAPSHOT_BULLETIN_FULL: &str = "https://storage.googleapis.com/zombienet-db-snaps/smoldot/bulletin_fetch/bulletin-full-2026-05-04.tgz";
const DB_SNAPSHOT_BULLETIN_PARTIAL: &str = "https://storage.googleapis.com/zombienet-db-snaps/smoldot/bulletin_fetch/bulletin-partial-2026-05-04.tgz";

/// Bulletin parachain chain-spec. The smoldot repo keeps a copy in `e2e-tests/chain-specs/`; for
/// polkadot-sdk we expect it hosted alongside the snapshot tarballs. Override with a local path
/// (e.g. pointing at the sibling smoldot checkout) for offline iteration.
const CHAIN_SPEC_BULLETIN: &str = "https://storage.googleapis.com/zombienet-db-snaps/smoldot/bulletin_fetch/bulletin-westend-local-spec-2026-05-04.json";

const ENV_DB_RELAY: &str = "DB_SNAPSHOT_RELAY_OVERRIDE";
const ENV_DB_FULL: &str = "DB_SNAPSHOT_BULLETIN_FULL_OVERRIDE";
const ENV_DB_PARTIAL: &str = "DB_SNAPSHOT_BULLETIN_PARTIAL_OVERRIDE";
const ENV_CHAIN_SPEC: &str = "BULLETIN_CHAIN_SPEC_OVERRIDE";

/// CIDs hardcoded from `smoldot/e2e-tests/src/bulletin.rs::payloads()` + `tmp/snapshots/manifest.json`.
/// All four are stored in the `full` snapshot. Only the first two also live on the `partial`
/// snapshot (`on_partial = true` in the manifest).
const CID_26B: &str = "bafk2bzacec6y4g7jkuw4a56nhgwujo64ajczzr6eijlsjb47ydcmoit4qcwqc";
const CID_4KIB: &str = "bafk2bzacebtgbe4obl6uzfoykcsigmounzfvycajptfeqjasfyukzjzxp5nli";
const CID_31B_FULL_ONLY: &str = "bafk2bzaceakzpr62fygyiyigr3thmkgfeyh5l3dlotse7pmwhbvtapx6yp4ow";

/// Expected payload sizes (bytes) for each CID. Matches the four `Payload` entries in
/// `smoldot/e2e-tests/src/bulletin.rs`. Hex envelope on the wire is `0x` + 2 chars / byte.
const CID_26B_BYTES: usize = 26;
const CID_4KIB_BYTES: usize = 4 * 1024;

/// `MAX_CIDS_PER_REQUEST` in the bitswap RPC impl. Anything beyond this triggers a top-level
/// `-32801 TooManyCids`.
const MAX_CIDS: usize = 64;

/// Per-CID error codes from `bitswap_unstable_get` / `streamItemError`.
const ERR_INVALID_PARAMS: i32 = -32602;
const ERR_FAIL: i32 = -32810;

/// Top-level subscription-rejection codes.
const ERR_TOO_MANY_CIDS: i32 = -32801;
const ERR_EMPTY_CIDS: i32 = -32802;
const ERR_DUPLICATE_CIDS: i32 = -32803;

/// Spawn the network from snapshots and return the running handle. Caller is responsible for
/// keeping it alive until the test finishes (`detach()` is called internally so cleanup is
/// driven by harness teardown).
async fn spawn_network(chain_spec_path: &Path) -> Result<Network<LocalFileSystem>> {
	let chain_spec_str = chain_spec_path
		.to_str()
		.ok_or_else(|| anyhow!("non-utf8 chain spec path"))?
		.to_string();
	let relay = std::env::var(ENV_DB_RELAY).unwrap_or_else(|_| DB_SNAPSHOT_RELAY.to_string());
	let bulletin_full =
		std::env::var(ENV_DB_FULL).unwrap_or_else(|_| DB_SNAPSHOT_BULLETIN_FULL.to_string());
	let bulletin_partial =
		std::env::var(ENV_DB_PARTIAL).unwrap_or_else(|_| DB_SNAPSHOT_BULLETIN_PARTIAL.to_string());

	let cfg = NetworkConfigBuilder::new()
		.with_relaychain(|rc| {
			rc.with_chain(RELAY_CHAIN)
				.with_default_command(RELAY_BINARY)
				.with_validator(|n| {
					n.with_name("alice").bootnode(true).with_db_snapshot(relay.as_str())
				})
				.with_validator(|n| {
					n.with_name("bob").bootnode(true).with_db_snapshot(relay.as_str())
				})
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_chain_spec_path(chain_spec_str.as_str())
				.cumulus_based(true)
				.with_default_args(vec![
					"--ipfs-server".into(),
					// `bitswap` is the substrate network-side bitswap handler
					// (`substrate/client/network/src/bitswap/mod.rs`); `sub-libp2p::bitswap` is
					// the lower-level libp2p protocol layer. Both at trace level for inspection.
					// `rpc-spec-v2::bitswap` is the RPC layer itself — fires on every get / stream
					// invocation against the local DB.
					"-lbitswap=trace,sub-libp2p::bitswap=trace,rpc-spec-v2::bitswap=trace,sync=debug".into(),
					("--relay-chain-rpc-urls", "{{ZOMBIE:alice:ws_uri}}").into(),
				])
				.with_collator(|c| {
					c.with_name("collator-1")
						.validator(true)
						.bootnode(true)
						.with_command(PARA_BINARY)
						.with_db_snapshot(bulletin_full.as_str())
				})
				.with_collator(|c| {
					c.with_name("collator-2")
						.validator(true)
						.bootnode(true)
						.with_command(PARA_BINARY)
						.with_db_snapshot(bulletin_partial.as_str())
				})
		})
		.build()
		.map_err(|e| anyhow!("network config errors: {e:?}"))?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(cfg).await?;
	network.detach().await;
	network.wait_until_is_up(180).await?;
	Ok(network)
}

/// Resolve the chain-spec source to a local path. If the override env var holds a path that
/// exists on disk, use it directly. Otherwise treat the value (or the baked-in default) as an
/// HTTP(S) URL and shell out to `curl` to download it to a tempfile.
async fn resolve_chain_spec() -> Result<PathBuf> {
	let value =
		std::env::var(ENV_CHAIN_SPEC).unwrap_or_else(|_| CHAIN_SPEC_BULLETIN.to_string());
	let as_path = PathBuf::from(&value);
	if as_path.exists() {
		return Ok(as_path);
	}
	if !(value.starts_with("http://") || value.starts_with("https://")) {
		return Err(anyhow!(
			"chain-spec source {value:?} is neither an existing path nor an http(s) URL"
		));
	}

	let dest = std::env::temp_dir().join(format!(
		"bulletin-westend-local-spec-{}.json",
		std::process::id()
	));
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

/// Build a `WsClient` against the collator's WS endpoint. We connect with `jsonrpsee` directly
/// (rather than via `node.rpc()` which returns subxt's `RpcClient`) so we can extract JSON-RPC
/// error codes through `ClientError::Call(ErrorObject)` — mirrors the in-process bitswap tests.
async fn ws_client_for(
	node: &zombienet_sdk::NetworkNode,
) -> Result<jsonrpsee::ws_client::WsClient> {
	let url = node.ws_uri();
	WsClientBuilder::default()
		.build(url)
		.await
		.map_err(|e| anyhow!("connecting to {url}: {e}"))
}

/// Pull the JSON-RPC error code out of a `ClientError`. Panics on shape mismatch — fine for tests.
fn expect_call_error_code(err: ClientError) -> i32 {
	match err {
		ClientError::Call(obj) => obj.code(),
		other => panic!("expected JSON-RPC Call error, got {other:?}"),
	}
}

/// Construct a valid CIDv1 for a payload that is NOT present on any collator. Uses
/// blake2b-256 (multihash code 0xb220) over a fixed seed so the test stays deterministic.
fn make_unknown_cid(seed: u8) -> String {
	const BLAKE2B_256: u64 = 0xb220;
	const DAG_PB: u64 = 0x70;
	let digest = [seed; 32];
	let mh = Multihash::<64>::wrap(BLAKE2B_256, &digest).expect("32-byte digest fits");
	Cid::new_v1(DAG_PB, mh).to_string()
}

/// Drain a stream subscription, collecting events until `StreamDone` arrives.
async fn drain_stream(sub: &mut Subscription<StreamEvent>) -> Result<Vec<StreamEvent>> {
	let mut out = Vec::new();
	loop {
		let next = tokio::time::timeout(Duration::from_secs(30), sub.next())
			.await
			.map_err(|_| anyhow!("timed out waiting for next stream event"))?
			.ok_or_else(|| anyhow!("subscription closed before streamDone"))?
			.map_err(|e| anyhow!("subscription decode error: {e}"))?;
		let is_done = matches!(next, StreamEvent::StreamDone);
		out.push(next);
		if is_done {
			return Ok(out);
		}
	}
}

/// Assert that no further events arrive within a short timeout — quiescence after `StreamDone`.
async fn expect_no_more_events(sub: &mut Subscription<StreamEvent>) {
	match tokio::time::timeout(Duration::from_millis(500), sub.next()).await {
		Err(_) | Ok(None) => {},
		Ok(Some(Ok(ev))) => panic!("unexpected extra event after streamDone: {ev:?}"),
		Ok(Some(Err(e))) => panic!("decode error after streamDone: {e}"),
	}
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires zombienet binary, network access for snapshots, and the published bulletin chain-spec; run manually or via the zombie-ci pipeline"]
async fn bitswap_unstable_e2e() -> Result<()> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let chain_spec = resolve_chain_spec().await?;
	let network = spawn_network(&chain_spec).await?;
	let collator_full = network.get_node("collator-1")?;
	let client = ws_client_for(collator_full).await?;

	// --- 1. `bitswap_unstable_get` happy path --------------------------------------------------
	let value_26b: String = client
		.request("bitswap_unstable_get", rpc_params![CID_26B])
		.await
		.map_err(|e| anyhow!("get(CID_26B) failed: {e}"))?;
	assert!(value_26b.starts_with("0x"), "value must be 0x-prefixed hex: {value_26b}");
	assert_eq!(
		value_26b.len(),
		2 + 2 * CID_26B_BYTES,
		"expected 0x + {CID_26B_BYTES} bytes hex, got {} chars",
		value_26b.len()
	);

	// --- 2. `bitswap_unstable_get` not-found ---------------------------------------------------
	let unknown = make_unknown_cid(0xAA);
	let err = client
		.request::<String, _>("bitswap_unstable_get", rpc_params![unknown.clone()])
		.await
		.expect_err("expected -32810 for unknown CID");
	assert_eq!(expect_call_error_code(err), ERR_FAIL);

	// --- 3. `bitswap_unstable_get` invalid CID -------------------------------------------------
	let err = client
		.request::<String, _>("bitswap_unstable_get", rpc_params!["not-a-valid-cid"])
		.await
		.expect_err("expected -32602 for malformed CID");
	assert_eq!(expect_call_error_code(err), ERR_INVALID_PARAMS);

	// --- 4. `bitswap_v1_get` legacy alias ------------------------------------------------------
	let via_alias: String = client
		.request("bitswap_v1_get", rpc_params![CID_26B])
		.await
		.map_err(|e| anyhow!("v1 alias call failed: {e}"))?;
	assert_eq!(via_alias, value_26b, "alias must return the same payload as unstable_get");

	// --- 5. `bitswap_unstable_get` error envelope has no `data` field --------------------------
	let err = client
		.request::<String, _>("bitswap_unstable_get", rpc_params![unknown.clone()])
		.await
		.expect_err("expected error response");
	if let ClientError::Call(obj) = &err {
		assert_eq!(obj.code(), ERR_FAIL);
		assert!(
			obj.data().is_none(),
			"bitswap error envelope must not carry a `data` field, got {:?}",
			obj.data()
		);
	} else {
		panic!("expected Call error, got {err:?}");
	}

	// --- 6. `bitswap_unstable_stream` happy path -----------------------------------------------
	let mut sub: Subscription<StreamEvent> = client
		.subscribe(
			"bitswap_unstable_stream",
			rpc_params![vec![CID_26B, CID_4KIB]],
			"bitswap_unstable_unstream",
		)
		.await
		.map_err(|e| anyhow!("stream subscribe failed: {e}"))?;
	let events = drain_stream(&mut sub).await?;
	assert_eq!(events.len(), 3, "expected 2 items + streamDone, got {events:?}");
	let mut by_cid: std::collections::HashMap<&str, &StreamEvent> =
		std::collections::HashMap::new();
	for ev in &events[..2] {
		match ev {
			StreamEvent::StreamItem { cid, .. } => {
				by_cid.insert(cid.as_str(), ev);
			},
			other => panic!("expected StreamItem in first two events, got {other:?}"),
		}
	}
	assert!(by_cid.contains_key(CID_26B), "missing StreamItem for CID_26B");
	assert!(by_cid.contains_key(CID_4KIB), "missing StreamItem for CID_4KIB");
	if let StreamEvent::StreamItem { value, .. } = by_cid[CID_26B] {
		assert_eq!(value.len(), 2 + 2 * CID_26B_BYTES);
	}
	if let StreamEvent::StreamItem { value, .. } = by_cid[CID_4KIB] {
		assert_eq!(value.len(), 2 + 2 * CID_4KIB_BYTES);
	}
	assert!(matches!(events[2], StreamEvent::StreamDone), "third event must be streamDone");
	expect_no_more_events(&mut sub).await;

	// --- 7. `bitswap_unstable_stream` mixed batch ----------------------------------------------
	let unknown_b = make_unknown_cid(0xBB);
	let mut sub: Subscription<StreamEvent> = client
		.subscribe(
			"bitswap_unstable_stream",
			rpc_params![vec![CID_26B, unknown_b.as_str(), "not-a-cid"]],
			"bitswap_unstable_unstream",
		)
		.await
		.map_err(|e| anyhow!("mixed stream subscribe failed: {e}"))?;
	let events = drain_stream(&mut sub).await?;
	assert_eq!(events.len(), 4, "expected 1 item + 2 errors + streamDone, got {events:?}");
	let mut hits = 0;
	let mut fail_count = 0;
	let mut invalid_count = 0;
	for ev in &events[..3] {
		match ev {
			StreamEvent::StreamItem { cid, .. } if cid == CID_26B => hits += 1,
			StreamEvent::StreamItemError { cid, code, .. } if cid == &unknown_b =>
				if *code == ERR_FAIL {
					fail_count += 1;
				},
			StreamEvent::StreamItemError { cid, code, .. } if cid == "not-a-cid" =>
				if *code == ERR_INVALID_PARAMS {
					invalid_count += 1;
				},
			other => panic!("unexpected event in mixed batch: {other:?}"),
		}
	}
	assert_eq!((hits, fail_count, invalid_count), (1, 1, 1), "mixed batch composition mismatch");
	assert!(matches!(events[3], StreamEvent::StreamDone));
	expect_no_more_events(&mut sub).await;

	// --- 8. Top-level rejections ---------------------------------------------------------------
	// 8a. empty
	let err = client
		.subscribe::<StreamEvent, _>(
			"bitswap_unstable_stream",
			rpc_params![Vec::<String>::new()],
			"bitswap_unstable_unstream",
		)
		.await
		.expect_err("expected -32802 for empty input");
	assert_eq!(expect_call_error_code(err), ERR_EMPTY_CIDS);

	// 8b. over-limit
	let too_many: Vec<String> = (0..(MAX_CIDS as u8 + 1)).map(make_unknown_cid).collect();
	let err = client
		.subscribe::<StreamEvent, _>(
			"bitswap_unstable_stream",
			rpc_params![too_many],
			"bitswap_unstable_unstream",
		)
		.await
		.expect_err("expected -32801 for over-limit input");
	assert_eq!(expect_call_error_code(err), ERR_TOO_MANY_CIDS);

	// 8c. duplicates
	let err = client
		.subscribe::<StreamEvent, _>(
			"bitswap_unstable_stream",
			rpc_params![vec![CID_26B, CID_26B]],
			"bitswap_unstable_unstream",
		)
		.await
		.expect_err("expected -32803 for duplicate input");
	assert_eq!(expect_call_error_code(err), ERR_DUPLICATE_CIDS);

	// --- 9. Cross-collator: partial-snapshot node sees no peer-fetch ---------------------------
	// `collator-2` has the `partial` snapshot. `CID_31B_FULL_ONLY` is absent from its DB. The
	// current bitswap RPC has no peer-side fetch yet (see TODO in `bitswap_unstable_stream`), so
	// this MUST return -32810. If/when peer-fetch lands and the data arrives via gossip, this
	// assertion will flip — keep the test gated so the failure points at the right TODO.
	let collator_partial = network.get_node("collator-2")?;
	let partial_client = ws_client_for(collator_partial).await?;
	let err = partial_client
		.request::<String, _>("bitswap_unstable_get", rpc_params![CID_31B_FULL_ONLY])
		.await
		.expect_err("expected -32810 on partial snapshot before peer-fetch lands");
	assert_eq!(expect_call_error_code(err), ERR_FAIL);

	keep_alive_if_requested().await;

	Ok(())
}

/// If `KEEP_ALIVE_SECS` is set in the environment, park the test for that many seconds before
/// returning. This keeps zombienet's child processes alive (the native provider tears them down
/// the moment the test function returns, since `network.detach()` is a no-op there), so a
/// developer can `tail -f` the node logs, hit the RPC by hand, etc. while iterating.
async fn keep_alive_if_requested() {
	let Ok(raw) = std::env::var("KEEP_ALIVE_SECS") else { return };
	let secs: u64 = match raw.parse() {
		Ok(n) => n,
		Err(e) => {
			eprintln!("KEEP_ALIVE_SECS={raw:?} is not a u64: {e} — skipping keep-alive");
			return;
		},
	};
	eprintln!(
		"KEEP_ALIVE_SECS={secs}: assertions passed; keeping the network alive for {secs}s. \
		 Ctrl-C to exit early."
	);
	tokio::time::sleep(Duration::from_secs(secs)).await;
}
