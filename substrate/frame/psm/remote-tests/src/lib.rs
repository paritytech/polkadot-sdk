// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Remote integration tests for pallet-psm.
//!
//! These tests fetch live chain state (e.g., Asset Hub Westend) via RPC and execute
//! PSM operations against real asset data. Since the PSM pallet may not be deployed
//! on the live chain yet, the tests inject PSM configuration into the fetched state.

use frame_support::{
	assert_noop, assert_ok,
	traits::{
		fungible::{metadata::Inspect as FungibleMetadataInspect, Inspect as FungibleInspect},
		fungibles::{
			metadata::{Inspect as FungiblesMetadataInspect, Mutate as FungiblesMetadataMutate},
			Create as FungiblesCreate, Inspect as FungiblesInspect, Mutate as FungiblesMutate,
		},
		Get, UncheckedOnRuntimeUpgrade,
	},
};
use remote_externalities::{Builder, Mode, OfflineConfig, OnlineConfig, SnapshotConfig};
use sp_runtime::{
	traits::{AccountIdConversion, Block as BlockT, Zero},
	DeserializeOwned, Saturating,
};

pub const LOG_TARGET: &str = "runtime::psm::remote-tests";

/// Balance type used by the PSM pallet's fungibles.
type BalanceOf<Runtime> =
	<<Runtime as pallet_psm::Config>::Fungibles as frame_support::traits::fungibles::Inspect<
		<Runtime as frame_system::Config>::AccountId,
	>>::Balance;

/// Configuration for which asset to use as the external stablecoin in tests.
pub struct PsmTestConfig {
	/// The external stablecoin asset ID (e.g., USDT = 1984).
	pub external_asset_id: u32,
	/// The pUSD stable asset ID. Will be created if it doesn't exist.
	pub stable_asset_id: u32,
	/// The expected decimal precision for pUSD (e.g., 6).
	pub stable_asset_decimals: u8,
	/// The pallet name for the assets pallet on the target chain (e.g., "Assets").
	/// Used to determine which storage prefixes to fetch from the live chain.
	pub assets_pallet_name: String,
	/// Optional setup callback invoked before creating the stable asset.
	/// Use this to set `NextAssetId` so that the asset can be created with
	/// the desired ID on chains that use `AutoIncAssetId`.
	pub pre_create_hook: Option<Box<dyn Fn()>>,
}

/// Amount of external stablecoin to swap in tests (1000 units).
const SWAP_AMOUNT: u128 = 1_000;
/// Amount of external stablecoin to fund the test caller with (2000 units).
const FUND_AMOUNT: u128 = 2_000;
/// Amount for a small redeem in circuit breaker tests (100 units).
const SMALL_REDEEM: u128 = 100;

/// Common test state returned by [`setup`].
struct TestEnv<Runtime: pallet_psm::Config + frame_system::Config> {
	asset_id: Runtime::AssetId,
	caller: Runtime::AccountId,
	psm_account: Runtime::AccountId,
	swap_amount: BalanceOf<Runtime>,
}

/// Create pUSD if needed, configure PSM, and fund test accounts.
/// Must be called inside `execute_with`.
fn setup<Runtime, MigrationConfig>(config: &PsmTestConfig) -> TestEnv<Runtime>
where
	Runtime: pallet_psm::Config + frame_system::Config,
	Runtime::AssetId: From<u32>,
	BalanceOf<Runtime>: TryFrom<u128> + core::fmt::Debug,
	Runtime::Fungibles:
		FungiblesCreate<Runtime::AccountId> + FungiblesMetadataMutate<Runtime::AccountId>,
	MigrationConfig: pallet_psm::migrations::v1::InitialPsmConfig<Runtime>,
{
	let asset_id: Runtime::AssetId = config.external_asset_id.into();
	let stable_asset_id: Runtime::AssetId = config.stable_asset_id.into();
	let psm_account: Runtime::AccountId = Runtime::PalletId::get().into_account_truncating();

	// Check that the external asset actually exists on-chain.
	assert!(
		<Runtime::Fungibles as FungiblesInspect<Runtime::AccountId>>::asset_exists(asset_id),
		"External asset does not exist on the live chain. \
		 Make sure the asset ID is correct."
	);

	let decimals =
		<Runtime::Fungibles as FungiblesMetadataInspect<Runtime::AccountId>>::decimals(asset_id);
	log::info!(
		target: LOG_TARGET,
		"External asset found with {} decimals",
		decimals,
	);

	// Create the pUSD stable asset if it doesn't exist yet.
	if !<Runtime::Fungibles as FungiblesInspect<Runtime::AccountId>>::asset_exists(stable_asset_id)
	{
		// Run pre-create hook (e.g., set NextAssetId for AutoIncAssetId chains).
		if let Some(hook) = &config.pre_create_hook {
			hook();
		}

		let _ = frame_system::Pallet::<Runtime>::inc_providers(&psm_account);

		assert_ok!(<Runtime::Fungibles as FungiblesCreate<Runtime::AccountId>>::create(
			stable_asset_id,
			psm_account.clone(),
			true,
			10_000u128.try_into().unwrap_or_else(|_| panic!("balance conversion failed")),
		));

		// Set pUSD metadata using the configured decimals.
		assert_ok!(<Runtime::Fungibles as FungiblesMetadataMutate<Runtime::AccountId>>::set(
			stable_asset_id,
			&psm_account,
			b"pUSD".to_vec(),
			b"pUSD".to_vec(),
			config.stable_asset_decimals,
		));

		log::info!(
			target: LOG_TARGET,
			"Created pUSD stable asset (id={}) with {} decimals",
			config.stable_asset_id,
			config.stable_asset_decimals,
		);
	}

	// Verify the stable asset and external asset have matching decimals.
	let stable_decimals =
		<Runtime::StableAsset as FungibleMetadataInspect<Runtime::AccountId>>::decimals();
	let external_decimals =
		<Runtime::Fungibles as FungiblesMetadataInspect<Runtime::AccountId>>::decimals(asset_id);
	assert_eq!(
		stable_decimals, external_decimals,
		"Decimals mismatch: stable={} vs external={}",
		stable_decimals, external_decimals,
	);

	// Run the V1 migration to initialize PSM.
	pallet_psm::migrations::v1::UncheckedMigrateToV1::<Runtime, MigrationConfig>::on_runtime_upgrade();

	// Fund test account.
	let caller: Runtime::AccountId =
		frame_support::PalletId(*b"py/test!").into_account_truncating();
	let _ = frame_system::Pallet::<Runtime>::inc_providers(&caller);

	let unit = 10u128.pow(config.stable_asset_decimals as u32);

	let fund_amount: BalanceOf<Runtime> = (FUND_AMOUNT * unit)
		.try_into()
		.unwrap_or_else(|_| panic!("balance conversion failed"));
	assert_ok!(<Runtime::Fungibles as FungiblesMutate<Runtime::AccountId>>::mint_into(
		asset_id,
		&caller,
		fund_amount,
	));

	let swap_amount: BalanceOf<Runtime> = (SWAP_AMOUNT * unit)
		.try_into()
		.unwrap_or_else(|_| panic!("balance conversion failed"));

	TestEnv { asset_id, caller, psm_account, swap_amount }
}

const SNAPSHOT_PATH: &str = "psm_remote_test.snap";

/// Build remote externalities by fetching live chain state.
///
/// State is fetched from the RPC node and cached to a local snapshot file so
/// that multiple tests within the same run can reuse it without extra RPC calls.
///
/// Call [`clear_ext`] after all tests complete to remove the snapshot file.
pub async fn build_ext<Block>(
	ws_url: String,
	assets_pallet_name: String,
) -> remote_externalities::RemoteExternalities<Block>
where
	Block: BlockT + DeserializeOwned,
	Block::Header: DeserializeOwned,
{
	Builder::<Block>::new()
		.mode(Mode::OfflineOrElseOnline(
			OfflineConfig { state_snapshot: SnapshotConfig::new(SNAPSHOT_PATH) },
			OnlineConfig {
				transport_uris: vec![ws_url],
				pallets: vec![assets_pallet_name],
				state_snapshot: Some(SnapshotConfig::new(SNAPSHOT_PATH)),
				..Default::default()
			},
		))
		.build()
		.await
		.unwrap()
}

/// Remove the snapshot file so the next run fetches fresh state.
pub fn clear_ext() {
	let _ = std::fs::remove_file(SNAPSHOT_PATH);
}

/// Test that minting and redeeming through the PSM works against real on-chain
/// asset data.
///
/// This test:
/// 1. Sets up PSM with an approved external asset
/// 2. Mints pUSD by depositing the external stablecoin
/// 3. Redeems pUSD back for the external stablecoin
/// 4. Verifies balances, debt tracking, and fee accounting
pub fn mint_and_redeem<Runtime, Block, MigrationConfig>(
	ext: &mut remote_externalities::RemoteExternalities<Block>,
	config: &PsmTestConfig,
) where
	Runtime: pallet_psm::Config + frame_system::Config,
	Block: BlockT,
	Runtime::AssetId: From<u32>,
	BalanceOf<Runtime>: TryFrom<u128> + core::fmt::Debug,
	Runtime::Fungibles:
		FungiblesCreate<Runtime::AccountId> + FungiblesMetadataMutate<Runtime::AccountId>,
	MigrationConfig: pallet_psm::migrations::v1::InitialPsmConfig<Runtime>,
{
	ext.execute_with(|| {
		let TestEnv { asset_id, caller, psm_account, swap_amount } =
			setup::<Runtime, MigrationConfig>(config);

		let balance_before = <Runtime::Fungibles as FungiblesInspect<Runtime::AccountId>>::balance(
			asset_id, &caller,
		);

		log::info!(
			target: LOG_TARGET,
			"Test account external stablecoin balance: {:?}",
			balance_before,
		);

		// Test mint
		assert_ok!(pallet_psm::Pallet::<Runtime>::mint(
			frame_system::RawOrigin::Signed(caller.clone()).into(),
			asset_id,
			swap_amount,
		));

		let balance_after_mint =
			<Runtime::Fungibles as FungiblesInspect<Runtime::AccountId>>::balance(
				asset_id, &caller,
			);
		assert_eq!(
			balance_after_mint,
			balance_before - swap_amount,
			"Caller external balance should decrease by exactly swap_amount"
		);

		let total_debt = pallet_psm::PsmDebt::<Runtime>::iter_values()
			.fold(BalanceOf::<Runtime>::zero(), |acc, debt| acc.saturating_add(debt));
		assert_eq!(total_debt, swap_amount, "PSM total debt should equal the swap amount");

		// The PSM account should hold the external stablecoin.
		let psm_external = <Runtime::Fungibles as FungiblesInspect<Runtime::AccountId>>::balance(
			asset_id,
			&psm_account,
		);
		assert_eq!(psm_external, swap_amount, "PSM should hold the external stablecoin");

		log::info!(
			target: LOG_TARGET,
			"Mint successful: debt={:?}, PSM external balance={:?}",
			total_debt,
			psm_external,
		);

		// Redeem all pUSD the caller has.
		let pusd_balance = Runtime::StableAsset::balance(&caller);
		let redeem_amount = pusd_balance;

		assert_ok!(pallet_psm::Pallet::<Runtime>::redeem(
			frame_system::RawOrigin::Signed(caller.clone()).into(),
			asset_id,
			redeem_amount,
		));

		// Verify caller's pUSD was fully spent.
		let pusd_after = Runtime::StableAsset::balance(&caller);
		assert_eq!(pusd_after, Zero::zero(), "Caller should have no pUSD remaining");

		// Debt should decrease after redeem but not reach zero (fees keep some debt alive).
		let debt_after = pallet_psm::PsmDebt::<Runtime>::iter_values()
			.fold(BalanceOf::<Runtime>::zero(), |acc, debt| acc.saturating_add(debt));
		assert!(debt_after > Zero::zero(), "Some debt should remain (fee portion)");
		assert!(debt_after < total_debt, "Debt should decrease after redeem");

		// Fee destination should have received fees.
		let fee_dest = Runtime::FeeDestination::get();
		let fee_balance = Runtime::StableAsset::balance(&fee_dest);
		assert!(fee_balance > Zero::zero(), "Fee destination should have collected fees");

		log::info!(
			target: LOG_TARGET,
			"Redeem successful: debt_after={:?}, fee_balance={:?}",
			debt_after,
			fee_balance,
		);

		log::info!(target: LOG_TARGET, "mint_and_redeem passed.");
	});
}

/// Test the circuit breaker mechanism against live chain state.
///
/// This test:
/// 1. Sets up PSM with an approved external asset
/// 2. Activates circuit breaker to `MintingDisabled` — verifies mint fails, redeem works
/// 3. Activates circuit breaker to `AllDisabled` — verifies both mint and redeem fail
/// 4. Deactivates circuit breaker — verifies both operations resume
pub fn circuit_breaker<Runtime, Block, MigrationConfig>(
	ext: &mut remote_externalities::RemoteExternalities<Block>,
	config: &PsmTestConfig,
) where
	Runtime: pallet_psm::Config + frame_system::Config,
	Block: BlockT,
	Runtime::AssetId: From<u32>,
	BalanceOf<Runtime>: TryFrom<u128> + core::fmt::Debug,
	Runtime::Fungibles:
		FungiblesCreate<Runtime::AccountId> + FungiblesMetadataMutate<Runtime::AccountId>,
	MigrationConfig: pallet_psm::migrations::v1::InitialPsmConfig<Runtime>,
{
	ext.execute_with(|| {
		let TestEnv { asset_id, caller, swap_amount, .. } =
			setup::<Runtime, MigrationConfig>(config);

		// Mint some pUSD first so we have something to redeem later.
		assert_ok!(pallet_psm::Pallet::<Runtime>::mint(
			frame_system::RawOrigin::Signed(caller.clone()).into(),
			asset_id,
			swap_amount,
		));

		let unit = 10u128.pow(config.stable_asset_decimals as u32);
		let small_redeem: BalanceOf<Runtime> = (SMALL_REDEEM * unit)
			.try_into()
			.unwrap_or_else(|_| panic!("balance conversion failed"));

		// Test: MintingDisabled. Mint fails, redeem still works
		assert_ok!(pallet_psm::Pallet::<Runtime>::set_asset_status(
			frame_system::RawOrigin::Root.into(),
			asset_id,
			pallet_psm::CircuitBreakerLevel::MintingDisabled,
		));

		assert_noop!(
			pallet_psm::Pallet::<Runtime>::mint(
				frame_system::RawOrigin::Signed(caller.clone()).into(),
				asset_id,
				swap_amount,
			),
			pallet_psm::Error::<Runtime>::MintingStopped
		);

		assert_ok!(pallet_psm::Pallet::<Runtime>::redeem(
			frame_system::RawOrigin::Signed(caller.clone()).into(),
			asset_id,
			small_redeem,
		));

		log::info!(target: LOG_TARGET, "MintingDisabled: mint blocked, redeem allowed");

		// Test: AllDisabled. Both mint and redeem fail
		assert_ok!(pallet_psm::Pallet::<Runtime>::set_asset_status(
			frame_system::RawOrigin::Root.into(),
			asset_id,
			pallet_psm::CircuitBreakerLevel::AllDisabled,
		));

		assert_noop!(
			pallet_psm::Pallet::<Runtime>::mint(
				frame_system::RawOrigin::Signed(caller.clone()).into(),
				asset_id,
				swap_amount,
			),
			pallet_psm::Error::<Runtime>::MintingStopped
		);

		assert_noop!(
			pallet_psm::Pallet::<Runtime>::redeem(
				frame_system::RawOrigin::Signed(caller.clone()).into(),
				asset_id,
				small_redeem,
			),
			pallet_psm::Error::<Runtime>::AllSwapsStopped
		);

		log::info!(target: LOG_TARGET, "AllDisabled: both mint and redeem blocked");

		// Test: Re-enable. Both operations resume
		assert_ok!(pallet_psm::Pallet::<Runtime>::set_asset_status(
			frame_system::RawOrigin::Root.into(),
			asset_id,
			pallet_psm::CircuitBreakerLevel::AllEnabled,
		));

		assert_ok!(pallet_psm::Pallet::<Runtime>::mint(
			frame_system::RawOrigin::Signed(caller.clone()).into(),
			asset_id,
			swap_amount,
		));

		assert_ok!(pallet_psm::Pallet::<Runtime>::redeem(
			frame_system::RawOrigin::Signed(caller.clone()).into(),
			asset_id,
			small_redeem,
		));

		log::info!(target: LOG_TARGET, "AllEnabled: both mint and redeem resumed");

		log::info!(target: LOG_TARGET, "circuit_breaker passed.");
	});
}
