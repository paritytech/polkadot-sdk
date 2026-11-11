// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Generator for the bulletin DB snapshots consumed by the bitswap `e2e` test.
//!
//! Ported from smoldot's `e2e-tests/tests/bulletin_generate_snapshot.rs`, using the zombienet-sdk
//! 0.4.13 snapshot API (`Network::pause` / `NetworkNode::snapshot_db`) and its re-exported subxt.
//! Spawns a fresh westend-local relay + bulletin parachain (para id 2487), injects
//! [`super::payloads`] via `transactionStorage::store`, and captures loose per-node DB tarballs:
//!   1. authorise //Alice, submit every payload;
//!   2. wait for height ≥ `DEFAULT_SNAPSHOT_HEIGHT`;
//!   3. snapshot the relay (`relaychain.tgz`) and collator-1 (`bulletin-full.tgz`).
//!
//! `#[ignore]`d (like smoldot's) — it needs `polkadot` / `polkadot-parachain` on `$PATH`, network
//! access for the chain-spec, and takes a while:
//!
//! ```sh
//! cargo test -p cumulus-zombienet-sdk-tests --features zombie-ci --test tests \
//!     -- --ignored bitswap_generate_snapshot --nocapture
//! ```
//!
//! Outputs land in `$BITSWAP_SNAPSHOT_OUT_DIR` (default `target/snapshots/`). The run prints the
//! `gcloud storage cp` commands to upload them under `gs://zombienet-db-snaps/cumulus/bitswap/`;
//! bump `SNAPSHOT_REV` in `e2e.rs` to the uploaded git-revision afterwards. The bulletin runtime
//! is loaded from the chain-spec resolved by [`super::common::resolve_chain_spec`].

use anyhow::{anyhow, Context, Result};
use std::{
	path::{Path, PathBuf},
	time::Duration,
};
use zombienet_sdk::{
	subxt::{
		config::{substrate::SubstrateConfig, DefaultExtrinsicParamsBuilder},
		dynamic::Value,
		tx::{dynamic as dynamic_tx, Payload},
		OnlineClient,
	},
	subxt_signer::sr25519::{dev, Keypair},
	LocalFileSystem, Network,
};

use super::{
	common::{network_config, resolve_chain_spec},
	payloads::{payloads, Payload as BulletinPayload, DEFAULT_SNAPSHOT_HEIGHT},
};

/// How long to wait for the network to come up.
const SPAWN_TIMEOUT_SECS: u64 = 300;
/// Per-extrinsic submit/finalize timeout.
const EXTRINSIC_TIMEOUT_SECS: u64 = 60;
/// Time budget for the parachain to reach the snapshot height.
const HEIGHT_TIMEOUT_SECS: u64 = 7200;
/// Time budget for the parachain to onboard and start producing blocks from genesis. Onboarding
/// takes a couple of relay sessions, so this is generous.
const PARA_LIVENESS_TIMEOUT_SECS: u64 = 600;
/// Authorisation budget granted to //Alice — enough for the whole payload set.
const AUTH_TX_LIMIT: u128 = 1000;
const AUTH_BYTE_LIMIT: u128 = 100_000_000;
/// Prometheus metric for a node's best-block height.
const BEST_HEIGHT_METRIC: &str = "block_height{status=\"best\"}";

/// Bucket the operator uploads to (printed in the upload hint; keep in sync with
/// `e2e.rs::DB_SNAPSHOT_BASE`).
const UPLOAD_BUCKET: &str = "gs://zombienet-db-snaps/cumulus/bitswap";
/// Env: output directory for the tarballs. Default `target/snapshots` under the crate.
const ENV_OUT_DIR: &str = "BITSWAP_SNAPSHOT_OUT_DIR";
/// Env: git-revision suffix used only in the printed upload command.
const ENV_REV: &str = "BITSWAP_SNAPSHOT_REV";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires zombienet binaries + network access for the chain-spec; run manually"]
async fn bitswap_generate_snapshot() -> Result<()> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let out_dir = std::env::var(ENV_OUT_DIR)
		.map(PathBuf::from)
		.unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/snapshots"));
	std::fs::create_dir_all(&out_dir).with_context(|| format!("creating {}", out_dir.display()))?;

	let chain_spec = resolve_chain_spec().await?;
	let network = spawn_network(&chain_spec).await?;
	let collator = network.get_node("collator-1")?;

	// `wait_until_is_up` only means the RPC is answering — the parachain still has to onboard on
	// the relay and start authoring. Submitting extrinsics before then leaves them stuck in the
	// pool forever. Wait for real block production first.
	log::info!("waiting for the parachain to onboard and start producing blocks");
	collator
		.wait_metric_with_timeout(BEST_HEIGHT_METRIC, |h| h >= 2.0, PARA_LIVENESS_TIMEOUT_SECS)
		.await
		.context("parachain did not start producing blocks (onboarding failed?)")?;

	let client: OnlineClient<SubstrateConfig> = collator.wait_client().await?;

	let alice = dev::alice();
	log::info!("authorising //Alice");
	authorize_account(&client, &alice).await?;

	let all = payloads();
	log::info!("injecting {} payloads", all.len());
	for payload in &all {
		submit_store(&client, &alice, payload).await?;
	}

	log::info!("waiting for parachain best height >= {DEFAULT_SNAPSHOT_HEIGHT}");
	collator
		.wait_metric_with_timeout(
			BEST_HEIGHT_METRIC,
			|h| h >= DEFAULT_SNAPSHOT_HEIGHT as f64,
			HEIGHT_TIMEOUT_SECS,
		)
		.await?;

	// Full snapshot: relay (alice) + collator-1 with every payload.
	log::info!("snapshotting full state (relay + full bulletin)");
	network.pause().await?;
	network.get_node("alice")?.snapshot_db(out_dir.join("relaychain.tgz")).await?;
	collator.snapshot_db(out_dir.join("bulletin-full.tgz")).await?;
	network.resume().await?;

	print_upload_hint(&out_dir);
	Ok(())
}

/// Spawn a fresh relay + bulletin network with NO pre-loaded DB snapshots — the nodes build state
/// from genesis so we can snapshot them. Same topology as the consumer (see
/// [`super::common::network_config`]); the only difference is `snaps = None`.
async fn spawn_network(chain_spec_path: &Path) -> Result<Network<LocalFileSystem>> {
	let cfg = network_config(chain_spec_path, None)?;
	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(cfg).await?;
	network.wait_until_is_up(SPAWN_TIMEOUT_SECS).await?;
	Ok(network)
}

/// `transactionStorage::authorize_account(who, transactions, bytes)` signed by //Alice, who
/// authorises herself. The bulletin `local_testnet` preset grants //Alice the `Authorizer` origin,
/// so no sudo wrapping is needed.
async fn authorize_account(client: &OnlineClient<SubstrateConfig>, alice: &Keypair) -> Result<()> {
	let call = dynamic_tx(
		"TransactionStorage",
		"authorize_account",
		vec![
			Value::from_bytes(alice.public_key().0),
			Value::u128(AUTH_TX_LIMIT),
			Value::u128(AUTH_BYTE_LIMIT),
		],
	);
	submit(client, &call, alice, "authorize_account").await
}

/// Submit `transactionStorage::store(data)` and wait for finalized success.
async fn submit_store(
	client: &OnlineClient<SubstrateConfig>,
	signer: &Keypair,
	payload: &BulletinPayload,
) -> Result<()> {
	log::info!("store {} ({} bytes) {}", payload.label, payload.size(), payload.predicted_cid());
	let call = dynamic_tx("TransactionStorage", "store", vec![Value::from_bytes(payload.content)]);
	submit(client, &call, signer, payload.label).await
}

/// Sign, submit, and await finalized success of a dynamic call, with timeouts. Nonce is fetched
/// from chain per call — fine here because we finalize between submissions.
async fn submit<Call: Payload>(
	client: &OnlineClient<SubstrateConfig>,
	call: &Call,
	signer: &Keypair,
	what: &str,
) -> Result<()> {
	let params = DefaultExtrinsicParamsBuilder::<SubstrateConfig>::new().build();
	let progress = tokio::time::timeout(
		Duration::from_secs(EXTRINSIC_TIMEOUT_SECS),
		client.tx().sign_and_submit_then_watch(call, signer, params),
	)
	.await
	.map_err(|_| anyhow!("{what}: submit timed out"))??;

	tokio::time::timeout(
		Duration::from_secs(EXTRINSIC_TIMEOUT_SECS * 2),
		progress.wait_for_finalized_success(),
	)
	.await
	.map_err(|_| anyhow!("{what}: finalize timed out"))??;
	Ok(())
}

/// Print the manual `gcloud` upload commands and the follow-up `SNAPSHOT_REV` bump.
fn print_upload_hint(out_dir: &Path) {
	let rev = std::env::var(ENV_REV).unwrap_or_else(|_| "REPLACE_AFTER_FIRST_UPLOAD".to_string());
	let d = out_dir.display();
	log::info!(
		"snapshots written to {d}\nupload with:\n  \
		 gcloud storage cp {d}/relaychain.tgz    {UPLOAD_BUCKET}/relaychain-{rev}.tgz\n  \
		 gcloud storage cp {d}/bulletin-full.tgz {UPLOAD_BUCKET}/bulletin-full-{rev}.tgz\n\
		 then set SNAPSHOT_REV = \"{rev}\" in e2e.rs"
	);
}
