// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Port of the legacy `bridges/testing/tests/0002-free-headers-synced-while-idle` test.
//!
//! The networks are spawned without any bridge initialization and the relayer is started only
//! after a delay. While the relayer is otherwise idle (no messages to deliver) it should still
//! sync multiple *free* relay-chain headers (and a single bridged parachain header) to the remote
//! Bridge Hub, and the finality/parachain relayers (`//Charlie` / `//Dave`) should keep a constant
//! balance because their transactions are free.

use crate::environment::{
	bridge_hub_rococo, bridge_hub_westend, count_synced_headers, dev_public, BridgeTestEnv,
	ENDOWMENT,
};
use std::time::Duration;
use subxt::{OnlineClient, PolkadotConfig};
use subxt_signer::sr25519::dev;

// We sleep for at least one session (60s for the test environment) before starting the relayer,
// so that a backlog of free headers accumulates while it is offline.
const RELAYER_START_DELAY: Duration = Duration::from_secs(90);
// Window during which we observe header imports (matches the legacy `multiple-headers-synced.js`).
const OBSERVE_WINDOW: Duration = Duration::from_secs(300);

#[tokio::test(flavor = "multi_thread")]
async fn free_headers_synced_while_idle() -> Result<(), anyhow::Error> {
	// Spawn without `--init` and without starting the relayer.
	let mut env = BridgeTestEnv::spawn(false, false).await?;

	// Give the chains time to produce a backlog of (free) headers, then start the relayer.
	tokio::time::sleep(RELAYER_START_DELAY).await;
	env.start_relayer().await?;

	let bhr = env.bridge_hub_rococo_client().await?;
	let bhw = env.bridge_hub_westend_client().await?;

	let charlie = dev_public(&dev::charlie());
	let dave = dev_public(&dev::dave());

	// Relayer balances are constant (before the observation window).
	assert_relayer_balances_unchanged(&bhr, &bhw, charlie, dave).await?;

	// Observe both directions concurrently for the same window:
	//   * on Westend BH we expect free Rococo (relay) + a single Rococo BH (parachain) header,
	//   * on Rococo BH we expect free Westend (relay) + a single Westend BH (parachain) header.
	let rococo_to_westend =
		count_synced_headers(&bhw, "BridgeRococoGrandpa", "BridgeRococoParachains", OBSERVE_WINDOW);
	let westend_to_rococo = count_synced_headers(
		&bhr,
		"BridgeWestendGrandpa",
		"BridgeWestendParachains",
		OBSERVE_WINDOW,
	);
	let ((r2w_grandpa, r2w_para), (w2r_grandpa, w2r_para)) =
		tokio::try_join!(rococo_to_westend, westend_to_rococo)?;

	anyhow::ensure!(
		r2w_grandpa > 1,
		"expected multiple Rococo relay headers synced to Westend BH, got {r2w_grandpa}"
	);
	anyhow::ensure!(
		r2w_para > 1,
		"expected Rococo parachain headers synced to Westend BH, got {r2w_para}"
	);
	anyhow::ensure!(
		w2r_grandpa > 1,
		"expected multiple Westend relay headers synced to Rococo BH, got {w2r_grandpa}"
	);
	anyhow::ensure!(
		w2r_para > 1,
		"expected Westend parachain headers synced to Rococo BH, got {w2r_para}"
	);

	// Relayer balances are still constant (after the observation window).
	assert_relayer_balances_unchanged(&bhr, &bhw, charlie, dave).await?;

	Ok(())
}

/// Asserts that `//Charlie` and `//Dave` keep exactly the genesis endowment on both Bridge Hubs.
async fn assert_relayer_balances_unchanged(
	bhr: &OnlineClient<PolkadotConfig>,
	bhw: &OnlineClient<PolkadotConfig>,
	charlie: [u8; 32],
	dave: [u8; 32],
) -> Result<(), anyhow::Error> {
	for (name, balance) in [
		("Charlie@RococoBH", bridge_hub_rococo::free_balance(bhr, charlie).await?),
		("Dave@RococoBH", bridge_hub_rococo::free_balance(bhr, dave).await?),
		("Charlie@WestendBH", bridge_hub_westend::free_balance(bhw, charlie).await?),
		("Dave@WestendBH", bridge_hub_westend::free_balance(bhw, dave).await?),
	] {
		anyhow::ensure!(
			balance == ENDOWMENT,
			"relayer {name} balance changed: {balance} != {ENDOWMENT}"
		);
	}
	Ok(())
}
