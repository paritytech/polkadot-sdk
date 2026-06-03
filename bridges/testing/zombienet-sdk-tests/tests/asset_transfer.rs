// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Port of the legacy `bridges/testing/tests/0001-asset-transfer` test.
//!
//! A user transfers ROC from Rococo Asset Hub to Westend Asset Hub and back, and WND from Westend
//! Asset Hub to Rococo Asset Hub and back. We assert that:
//!   * the wrapped assets arrive on the remote Asset Hub (at least 4.8 of 5 sent, after fees),
//!   * the message relayers (`//Eve` / `//Ferdie`) are rewarded,
//!   * the unwrapped (native) assets arrive back on the origin Asset Hub (at least 2.8 of 3), and
//!   * the finality/parachain relayers (`//Charlie` / `//Dave`) keep a constant balance, because
//!     their transactions are free.

use crate::environment::{
	asset_hub_rococo, asset_hub_westend, bridge_hub_rococo_relayer_reward,
	bridge_hub_westend_relayer_reward, dev_account, dev_public, retry_until, BridgeTestEnv,
	ENDOWMENT, ROCOCO_GENESIS_HASH, WESTEND_GENESIS_HASH,
};
use std::time::Duration;
use subxt::{OnlineClient, PolkadotConfig};
use subxt_signer::sr25519::dev;

const FIVE_UNITS: u128 = 5_000_000_000_000;
const THREE_UNITS: u128 = 3_000_000_000_000;
// Minimum amount expected to arrive after bridge/XCM fees (4.8 of 5, 2.8 of 3).
const MIN_WRAPPED_RECEIVED: u128 = 4_800_000_000_000;
const MIN_NATIVE_RECEIVED: u128 = 2_800_000_000_000;
const MIN_RELAYER_REWARD: u128 = 1;

#[tokio::test(flavor = "multi_thread")]
async fn asset_transfer_works() -> Result<(), anyhow::Error> {
	let env = BridgeTestEnv::spawn(true, true).await?;

	let ahr = env.asset_hub_rococo_client().await?;
	let ahw = env.asset_hub_westend_client().await?;
	let bhr = env.bridge_hub_rococo_client().await?;
	let bhw = env.bridge_hub_westend_client().await?;

	let alice = dev::alice();
	let alice_pub = dev_public(&alice);
	let alice_acc = dev_account(&alice);
	let charlie = dev_public(&dev::charlie());
	let dave = dev_public(&dev::dave());

	// Relayer balances are constant (before the transfers).
	assert_relayer_balances_unchanged(&bhr, &bhw, charlie, dave).await?;

	// --- roc-reaches-westend: send 5 ROC from Rococo AH to Westend AH. ---
	// ROC is native to Rococo AH, so Rococo AH is the reserve.
	asset_hub_rococo::transfer_assets(
		&ahr,
		&alice,
		WESTEND_GENESIS_HASH,
		alice_pub,
		asset_hub_rococo::native_asset,
		FIVE_UNITS,
		asset_hub_rococo::TransferType::LocalReserve,
	)
	.await?;
	// //Alice receives at least 4.8 wrapped ROC on Westend AH.
	retry_until(Duration::from_secs(600), || {
		let ahw = ahw.clone();
		let acc = alice_acc.clone();
		async move {
			let asset = asset_hub_westend::bridged_asset(ROCOCO_GENESIS_HASH);
			let balance = asset_hub_westend::foreign_asset_balance(&ahw, asset, acc).await?;
			Ok(balance.filter(|b| *b > MIN_WRAPPED_RECEIVED).map(|_| ()))
		}
	})
	.await?;
	// //Eve is rewarded on Westend BH for delivering messages from Rococo BH.
	let eve = dev_account(&dev::eve());
	retry_until(Duration::from_secs(300), || {
		let bhw = bhw.clone();
		let eve = eve.clone();
		async move {
			let reward = bridge_hub_westend_relayer_reward(&bhw, eve).await?;
			Ok(reward.filter(|r| *r > MIN_RELAYER_REWARD).map(|_| ()))
		}
	})
	.await?;

	// --- wnd-reaches-rococo: send 5 WND from Westend AH to Rococo AH. ---
	// WND is native to Westend AH, so Westend AH is the reserve.
	asset_hub_westend::transfer_assets(
		&ahw,
		&alice,
		ROCOCO_GENESIS_HASH,
		alice_pub,
		asset_hub_westend::native_asset,
		FIVE_UNITS,
		asset_hub_westend::TransferType::LocalReserve,
	)
	.await?;
	retry_until(Duration::from_secs(600), || {
		let ahr = ahr.clone();
		let acc = alice_acc.clone();
		async move {
			let asset = asset_hub_rococo::bridged_asset(WESTEND_GENESIS_HASH);
			let balance = asset_hub_rococo::foreign_asset_balance(&ahr, asset, acc).await?;
			Ok(balance.filter(|b| *b > MIN_WRAPPED_RECEIVED).map(|_| ()))
		}
	})
	.await?;
	// //Ferdie is rewarded on Rococo BH for delivering messages from Westend BH.
	let ferdie = dev_account(&dev::ferdie());
	retry_until(Duration::from_secs(300), || {
		let bhr = bhr.clone();
		let ferdie = ferdie.clone();
		async move {
			let reward = bridge_hub_rococo_relayer_reward(&bhr, ferdie).await?;
			Ok(reward.filter(|r| *r > MIN_RELAYER_REWARD).map(|_| ()))
		}
	})
	.await?;

	// --- wroc-reaches-rococo: send 3 wrapped ROC back from Westend AH to Rococo AH. ---
	// Wrapped ROC's reserve is Rococo AH (the destination), so use a destination reserve.
	let initial_roc = asset_hub_rococo::free_balance(&ahr, alice_pub).await?;
	asset_hub_westend::transfer_assets(
		&ahw,
		&alice,
		ROCOCO_GENESIS_HASH,
		alice_pub,
		|| asset_hub_westend::bridged_asset(ROCOCO_GENESIS_HASH),
		THREE_UNITS,
		asset_hub_westend::TransferType::DestinationReserve,
	)
	.await?;
	wait_for_native_increase(&ahr, alice_pub, initial_roc, MIN_NATIVE_RECEIVED).await?;

	// --- wwnd-reaches-westend: send 3 wrapped WND back from Rococo AH to Westend AH. ---
	// Wrapped WND's reserve is Westend AH (the destination), so use a destination reserve.
	let initial_wnd = asset_hub_westend::free_balance(&ahw, alice_pub).await?;
	asset_hub_rococo::transfer_assets(
		&ahr,
		&alice,
		WESTEND_GENESIS_HASH,
		alice_pub,
		|| asset_hub_rococo::bridged_asset(WESTEND_GENESIS_HASH),
		THREE_UNITS,
		asset_hub_rococo::TransferType::DestinationReserve,
	)
	.await?;
	wait_for_native_increase(&ahw, alice_pub, initial_wnd, MIN_NATIVE_RECEIVED).await?;

	// Relayer balances are still constant (after the transfers).
	assert_relayer_balances_unchanged(&bhr, &bhw, charlie, dave).await?;

	Ok(())
}

/// Waits (up to 10 minutes) until the native free balance of `account` on `client` exceeds
/// `initial + min_increase`. Mirrors `native-assets-balance-increased.js`.
async fn wait_for_native_increase(
	client: &OnlineClient<PolkadotConfig>,
	account: [u8; 32],
	initial: u128,
	min_increase: u128,
) -> Result<(), anyhow::Error> {
	retry_until(Duration::from_secs(600), || {
		let client = client.clone();
		async move {
			let balance = crate::environment::free_balance_at(&client, account).await?;
			Ok((balance > initial + min_increase).then_some(()))
		}
	})
	.await
}

/// Asserts that `//Charlie` and `//Dave` keep exactly the genesis endowment on both Bridge Hubs.
async fn assert_relayer_balances_unchanged(
	bhr: &OnlineClient<PolkadotConfig>,
	bhw: &OnlineClient<PolkadotConfig>,
	charlie: [u8; 32],
	dave: [u8; 32],
) -> Result<(), anyhow::Error> {
	use crate::environment::{bridge_hub_rococo, bridge_hub_westend};
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
