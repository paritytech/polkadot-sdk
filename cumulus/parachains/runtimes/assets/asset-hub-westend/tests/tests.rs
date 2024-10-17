// This file is part of Cumulus.

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

//! Tests for the Westmint (Westend Assets Hub) chain.

use asset_hub_westend_runtime::{
	xcm_config,
	xcm_config::{
		bridging, AssetFeeAsExistentialDepositMultiplierFeeCharger, CheckingAccount,
		ForeignAssetFeeAsExistentialDepositMultiplierFeeCharger, ForeignCreatorsSovereignAccountOf,
		LocationToAccountId, StakingPot, TrustBackedAssetsPalletLocation, WestendLocation,
		XcmConfig,
	},
	AllPalletsWithoutSystem, Assets, Balances, ExistentialDeposit, ForeignAssets,
	ForeignAssetsInstance, MetadataDepositBase, MetadataDepositPerByte, ParachainSystem,
	PolkadotXcm, Runtime, RuntimeCall, RuntimeEvent, RuntimeOrigin, SessionKeys,
	TrustBackedAssetsInstance, XcmpQueue,
};
pub use asset_hub_westend_runtime::{AssetConversion, AssetDeposit, CollatorSelection, System};
use asset_test_utils::{
	test_cases_over_bridge::TestBridgingConfig, CollatorSessionKey, CollatorSessionKeys,
	ExtBuilder, SlotDurations,
};
use codec::{Decode, Encode};
use cumulus_primitives_utility::ChargeWeightInFungibles;
use frame_support::{
	assert_noop, assert_ok,
	traits::{
		fungible::{Inspect, Mutate},
		fungibles::{
			Create, Inspect as FungiblesInspect, InspectEnumerable, Mutate as FungiblesMutate,
		},
	},
	weights::{Weight, WeightToFee as WeightToFeeT},
};
use parachains_common::{AccountId, AssetIdForTrustBackedAssets, AuraId, Balance};
use sp_consensus_aura::SlotDuration;
use sp_core::crypto::Ss58Codec;
use sp_runtime::traits::MaybeEquivalence;
use std::{convert::Into, ops::Mul};
use testnet_parachains_constants::westend::{consensus::*, currency::UNITS, fee::WeightToFee};
use xcm::latest::prelude::{Assets as XcmAssets, *};
use xcm_builder::WithLatestLocationConverter;
use xcm_executor::traits::{ConvertLocation, JustTry, WeightTrader};
use xcm_runtime_apis::conversions::LocationToAccountHelper;

const ALICE: [u8; 32] = [1u8; 32];
const SOME_ASSET_ADMIN: [u8; 32] = [5u8; 32];

type AssetIdForTrustBackedAssetsConvert =
	assets_common::AssetIdForTrustBackedAssetsConvert<TrustBackedAssetsPalletLocation>;

type RuntimeHelper = asset_test_utils::RuntimeHelper<Runtime, AllPalletsWithoutSystem>;

fn collator_session_key(account: [u8; 32]) -> CollatorSessionKey<Runtime> {
	CollatorSessionKey::new(
		AccountId::from(account),
		AccountId::from(account),
		SessionKeys { aura: AuraId::from(sp_core::sr25519::Public::from_raw(account)) },
	)
}

fn collator_session_keys() -> CollatorSessionKeys<Runtime> {
	CollatorSessionKeys::default().add(collator_session_key(ALICE))
}

fn slot_durations() -> SlotDurations {
	SlotDurations {
		relay: SlotDuration::from_millis(RELAY_CHAIN_SLOT_DURATION_MILLIS.into()),
		para: SlotDuration::from_millis(SLOT_DURATION),
	}
}

fn setup_pool_for_paying_fees_with_foreign_assets(
	(foreign_asset_owner, foreign_asset_id_location, foreign_asset_id_minimum_balance): (
		AccountId,
		xcm::v5::Location,
		Balance,
	),
) {
	let existential_deposit = ExistentialDeposit::get();

	// setup a pool to pay fees with `foreign_asset_id_location` tokens
	let pool_owner: AccountId = [14u8; 32].into();
	let native_asset = xcm::v5::Location::parent();
	let pool_liquidity: Balance =
		existential_deposit.max(foreign_asset_id_minimum_balance).mul(100_000);

	let _ = Balances::force_set_balance(
		RuntimeOrigin::root(),
		pool_owner.clone().into(),
		(existential_deposit + pool_liquidity).mul(2).into(),
	);

	assert_ok!(ForeignAssets::mint(
		RuntimeOrigin::signed(foreign_asset_owner),
		foreign_asset_id_location.clone().into(),
		pool_owner.clone().into(),
		(foreign_asset_id_minimum_balance + pool_liquidity).mul(2).into(),
	));

	assert_ok!(AssetConversion::create_pool(
		RuntimeOrigin::signed(pool_owner.clone()),
		Box::new(native_asset.clone().into()),
		Box::new(foreign_asset_id_location.clone().into())
	));

	assert_ok!(AssetConversion::add_liquidity(
		RuntimeOrigin::signed(pool_owner.clone()),
		Box::new(native_asset.into()),
		Box::new(foreign_asset_id_location.into()),
		pool_liquidity,
		pool_liquidity,
		1,
		1,
		pool_owner,
	));
}

#[test]
fn test_buy_and_refund_weight_in_native() {
	ExtBuilder::<Runtime>::default()
		.with_collators(vec![AccountId::from(ALICE)])
		.with_session_keys(vec![(
			AccountId::from(ALICE),
			AccountId::from(ALICE),
			SessionKeys { aura: AuraId::from(sp_core::sr25519::Public::from_raw(ALICE)) },
		)])
		.build()
		.execute_with(|| {
			let bob: AccountId = SOME_ASSET_ADMIN.into();
			let staking_pot = CollatorSelection::account_id();
			let native_location = WestendLocation::get();
			let initial_balance = 200 * UNITS;

			assert_ok!(Balances::mint_into(&bob, initial_balance));
			assert_ok!(Balances::mint_into(&staking_pot, initial_balance));

			// keep initial total issuance to assert later.
			let total_issuance = Balances::total_issuance();

			// prepare input to buy weight.
			let weight = Weight::from_parts(4_000_000_000, 0);
			let fee = WeightToFee::weight_to_fee(&weight);
			let extra_amount = 100;
			let ctx = XcmContext { origin: None, message_id: XcmHash::default(), topic: None };
			let payment: Asset = (native_location.clone(), fee + extra_amount).into();

			// init trader and buy weight.
			let mut trader = <XcmConfig as xcm_executor::Config>::Trader::new();
			let unused_asset =
				trader.buy_weight(weight, payment.into(), &ctx).expect("Expected Ok");

			// assert.
			let unused_amount =
				unused_asset.fungible.get(&native_location.clone().into()).map_or(0, |a| *a);
			assert_eq!(unused_amount, extra_amount);
			assert_eq!(Balances::total_issuance(), total_issuance);

			// prepare input to refund weight.
			let refund_weight = Weight::from_parts(1_000_000_000, 0);
			let refund = WeightToFee::weight_to_fee(&refund_weight);

			// refund.
			let actual_refund = trader.refund_weight(refund_weight, &ctx).unwrap();
			assert_eq!(actual_refund, (native_location, refund).into());

			// assert.
			assert_eq!(Balances::balance(&staking_pot), initial_balance);
			// only after `trader` is dropped we expect the fee to be resolved into the treasury
			// account.
			drop(trader);
			assert_eq!(Balances::balance(&staking_pot), initial_balance + fee - refund);
			assert_eq!(Balances::total_issuance(), total_issuance + fee - refund);
		})
}

#[test]
fn test_buy_and_refund_weight_with_swap_local_asset_xcm_trader() {
	ExtBuilder::<Runtime>::default()
		.with_collators(vec![AccountId::from(ALICE)])
		.with_session_keys(vec![(
			AccountId::from(ALICE),
			AccountId::from(ALICE),
			SessionKeys { aura: AuraId::from(sp_core::sr25519::Public::from_raw(ALICE)) },
		)])
		.build()
		.execute_with(|| {
			let bob: AccountId = SOME_ASSET_ADMIN.into();
			let staking_pot = CollatorSelection::account_id();
			let asset_1: u32 = 1;
			let native_location = WestendLocation::get();
			let asset_1_location =
				AssetIdForTrustBackedAssetsConvert::convert_back(&asset_1).unwrap();
			// bob's initial balance for native and `asset1` assets.
			let initial_balance = 200 * UNITS;
			// liquidity for both arms of (native, asset1) pool.
			let pool_liquidity = 100 * UNITS;

			// init asset, balances and pool.
			assert_ok!(<Assets as Create<_>>::create(asset_1, bob.clone(), true, 10));

			assert_ok!(Assets::mint_into(asset_1, &bob, initial_balance));
			assert_ok!(Balances::mint_into(&bob, initial_balance));
			assert_ok!(Balances::mint_into(&staking_pot, initial_balance));

			assert_ok!(AssetConversion::create_pool(
				RuntimeHelper::origin_of(bob.clone()),
				Box::new(
					xcm::v5::Location::try_from(native_location.clone()).expect("conversion works")
				),
				Box::new(
					xcm::v5::Location::try_from(asset_1_location.clone())
						.expect("conversion works")
				)
			));

			assert_ok!(AssetConversion::add_liquidity(
				RuntimeHelper::origin_of(bob.clone()),
				Box::new(
					xcm::v5::Location::try_from(native_location.clone()).expect("conversion works")
				),
				Box::new(
					xcm::v5::Location::try_from(asset_1_location.clone())
						.expect("conversion works")
				),
				pool_liquidity,
				pool_liquidity,
				1,
				1,
				bob,
			));

			// keep initial total issuance to assert later.
			let asset_total_issuance = Assets::total_issuance(asset_1);
			let native_total_issuance = Balances::total_issuance();

			// prepare input to buy weight.
			let weight = Weight::from_parts(4_000_000_000, 0);
			let fee = WeightToFee::weight_to_fee(&weight);
			let asset_fee =
				AssetConversion::get_amount_in(&fee, &pool_liquidity, &pool_liquidity).unwrap();
			let extra_amount = 100;
			let ctx = XcmContext { origin: None, message_id: XcmHash::default(), topic: None };
			let payment: Asset = (asset_1_location.clone(), asset_fee + extra_amount).into();

			// init trader and buy weight.
			let mut trader = <XcmConfig as xcm_executor::Config>::Trader::new();
			let unused_asset =
				trader.buy_weight(weight, payment.into(), &ctx).expect("Expected Ok");

			// assert.
			let unused_amount =
				unused_asset.fungible.get(&asset_1_location.clone().into()).map_or(0, |a| *a);
			assert_eq!(unused_amount, extra_amount);
			assert_eq!(Assets::total_issuance(asset_1), asset_total_issuance + asset_fee);

			// prepare input to refund weight.
			let refund_weight = Weight::from_parts(1_000_000_000, 0);
			let refund = WeightToFee::weight_to_fee(&refund_weight);
			let (reserve1, reserve2) = AssetConversion::get_reserves(
				xcm::v5::Location::try_from(native_location).expect("conversion works"),
				xcm::v5::Location::try_from(asset_1_location.clone()).expect("conversion works"),
			)
			.unwrap();
			let asset_refund =
				AssetConversion::get_amount_out(&refund, &reserve1, &reserve2).unwrap();

			// refund.
			let actual_refund = trader.refund_weight(refund_weight, &ctx).unwrap();
			assert_eq!(actual_refund, (asset_1_location, asset_refund).into());

			// assert.
			assert_eq!(Balances::balance(&staking_pot), initial_balance);
			// only after `trader` is dropped we expect the fee to be resolved into the treasury
			// account.
			drop(trader);
			assert_eq!(Balances::balance(&staking_pot), initial_balance + fee - refund);
			assert_eq!(
				Assets::total_issuance(asset_1),
				asset_total_issuance + asset_fee - asset_refund
			);
			assert_eq!(Balances::total_issuance(), native_total_issuance);
		})
}

#[test]
fn test_buy_and_refund_weight_with_swap_foreign_asset_xcm_trader() {
	ExtBuilder::<Runtime>::default()
		.with_collators(vec![AccountId::from(ALICE)])
		.with_session_keys(vec![(
			AccountId::from(ALICE),
			AccountId::from(ALICE),
			SessionKeys { aura: AuraId::from(sp_core::sr25519::Public::from_raw(ALICE)) },
		)])
		.build()
		.execute_with(|| {
			let bob: AccountId = SOME_ASSET_ADMIN.into();
			let staking_pot = CollatorSelection::account_id();
			let native_location =
				xcm::v5::Location::try_from(WestendLocation::get()).expect("conversion works");
			let foreign_location = xcm::v5::Location {
				parents: 1,
				interior: (
					xcm::v5::Junction::Parachain(1234),
					xcm::v5::Junction::GeneralIndex(12345),
				)
					.into(),
			};
			// bob's initial balance for native and `asset1` assets.
			let initial_balance = 200 * UNITS;
			// liquidity for both arms of (native, asset1) pool.
			let pool_liquidity = 100 * UNITS;

			// init asset, balances and pool.
			assert_ok!(<ForeignAssets as Create<_>>::create(
				foreign_location.clone(),
				bob.clone(),
				true,
				10
			));

			assert_ok!(ForeignAssets::mint_into(foreign_location.clone(), &bob, initial_balance));
			assert_ok!(Balances::mint_into(&bob, initial_balance));
			assert_ok!(Balances::mint_into(&staking_pot, initial_balance));

			assert_ok!(AssetConversion::create_pool(
				RuntimeHelper::origin_of(bob.clone()),
				Box::new(native_location.clone()),
				Box::new(foreign_location.clone())
			));

			assert_ok!(AssetConversion::add_liquidity(
				RuntimeHelper::origin_of(bob.clone()),
				Box::new(native_location.clone()),
				Box::new(foreign_location.clone()),
				pool_liquidity,
				pool_liquidity,
				1,
				1,
				bob,
			));

			// keep initial total issuance to assert later.
			let asset_total_issuance = ForeignAssets::total_issuance(foreign_location.clone());
			let native_total_issuance = Balances::total_issuance();

			// prepare input to buy weight.
			let weight = Weight::from_parts(4_000_000_000, 0);
			let fee = WeightToFee::weight_to_fee(&weight);
			let asset_fee =
				AssetConversion::get_amount_in(&fee, &pool_liquidity, &pool_liquidity).unwrap();
			let extra_amount = 100;
			let ctx = XcmContext { origin: None, message_id: XcmHash::default(), topic: None };
			let payment: Asset = (foreign_location.clone(), asset_fee + extra_amount).into();

			// init trader and buy weight.
			let mut trader = <XcmConfig as xcm_executor::Config>::Trader::new();
			let unused_asset =
				trader.buy_weight(weight, payment.into(), &ctx).expect("Expected Ok");

			// assert.
			let unused_amount =
				unused_asset.fungible.get(&foreign_location.clone().into()).map_or(0, |a| *a);
			assert_eq!(unused_amount, extra_amount);
			assert_eq!(
				ForeignAssets::total_issuance(foreign_location.clone()),
				asset_total_issuance + asset_fee
			);

			// prepare input to refund weight.
			let refund_weight = Weight::from_parts(1_000_000_000, 0);
			let refund = WeightToFee::weight_to_fee(&refund_weight);
			let (reserve1, reserve2) =
				AssetConversion::get_reserves(native_location, foreign_location.clone()).unwrap();
			let asset_refund =
				AssetConversion::get_amount_out(&refund, &reserve1, &reserve2).unwrap();

			// refund.
			let actual_refund = trader.refund_weight(refund_weight, &ctx).unwrap();
			assert_eq!(actual_refund, (foreign_location.clone(), asset_refund).into());

			// assert.
			assert_eq!(Balances::balance(&staking_pot), initial_balance);
			// only after `trader` is dropped we expect the fee to be resolved into the treasury
			// account.
			drop(trader);
			assert_eq!(Balances::balance(&staking_pot), initial_balance + fee - refund);
			assert_eq!(
				ForeignAssets::total_issuance(foreign_location),
				asset_total_issuance + asset_fee - asset_refund
			);
			assert_eq!(Balances::total_issuance(), native_total_issuance);
		})
}

#[test]
fn test_asset_xcm_take_first_trader() {
	ExtBuilder::<Runtime>::default()
		.with_collators(vec![AccountId::from(ALICE)])
		.with_session_keys(vec![(
			AccountId::from(ALICE),
			AccountId::from(ALICE),
			SessionKeys { aura: AuraId::from(sp_core::sr25519::Public::from_raw(ALICE)) },
		)])
		.build()
		.execute_with(|| {
			// We need root origin to create a sufficient asset
			let minimum_asset_balance = 3333333_u128;
			let local_asset_id = 1;
			assert_ok!(Assets::force_create(
				RuntimeHelper::root_origin(),
				local_asset_id.into(),
				AccountId::from(ALICE).into(),
				true,
				minimum_asset_balance
			));

			// We first mint enough asset for the account to exist for assets
			assert_ok!(Assets::mint(
				RuntimeHelper::origin_of(AccountId::from(ALICE)),
				local_asset_id.into(),
				AccountId::from(ALICE).into(),
				minimum_asset_balance
			));

			// get asset id as location
			let asset_location =
				AssetIdForTrustBackedAssetsConvert::convert_back(&local_asset_id).unwrap();

			// Set Alice as block author, who will receive fees
			RuntimeHelper::run_to_block(2, AccountId::from(ALICE));

			// We are going to buy 4e9 weight
			let bought = Weight::from_parts(4_000_000_000u64, 0);

			// Lets calculate amount needed
			let asset_amount_needed =
				AssetFeeAsExistentialDepositMultiplierFeeCharger::charge_weight_in_fungibles(
					local_asset_id,
					bought,
				)
				.expect("failed to compute");

			// Lets pay with: asset_amount_needed + asset_amount_extra
			let asset_amount_extra = 100_u128;
			let asset: Asset =
				(asset_location.clone(), asset_amount_needed + asset_amount_extra).into();

			let mut trader = <XcmConfig as xcm_executor::Config>::Trader::new();
			let ctx = XcmContext { origin: None, message_id: XcmHash::default(), topic: None };

			// Lets buy_weight and make sure buy_weight does not return an error
			let unused_assets = trader.buy_weight(bought, asset.into(), &ctx).expect("Expected Ok");
			// Check whether a correct amount of unused assets is returned
			assert_ok!(unused_assets.ensure_contains(&(asset_location, asset_amount_extra).into()));

			// Drop trader
			drop(trader);

			// Make sure author(Alice) has received the amount
			assert_eq!(
				Assets::balance(local_asset_id, AccountId::from(ALICE)),
				minimum_asset_balance + asset_amount_needed
			);

			// We also need to ensure the total supply increased
			assert_eq!(
				Assets::total_supply(local_asset_id),
				minimum_asset_balance + asset_amount_needed
			);
		});
}

#[test]
fn test_foreign_asset_xcm_take_first_trader() {
	ExtBuilder::<Runtime>::default()
		.with_collators(vec![AccountId::from(ALICE)])
		.with_session_keys(vec![(
			AccountId::from(ALICE),
			AccountId::from(ALICE),
			SessionKeys { aura: AuraId::from(sp_core::sr25519::Public::from_raw(ALICE)) },
		)])
		.build()
		.execute_with(|| {
			// We need root origin to create a sufficient asset
			let minimum_asset_balance = 3333333_u128;
			let foreign_location = xcm::v5::Location {
				parents: 1,
				interior: (
					xcm::v5::Junction::Parachain(1234),
					xcm::v5::Junction::GeneralIndex(12345),
				)
					.into(),
			};
			assert_ok!(ForeignAssets::force_create(
				RuntimeHelper::root_origin(),
				foreign_location.clone().into(),
				AccountId::from(ALICE).into(),
				true,
				minimum_asset_balance
			));

			// We first mint enough asset for the account to exist for assets
			assert_ok!(ForeignAssets::mint(
				RuntimeHelper::origin_of(AccountId::from(ALICE)),
				foreign_location.clone().into(),
				AccountId::from(ALICE).into(),
				minimum_asset_balance
			));

			let asset_location_v5: Location = foreign_location.clone().try_into().unwrap();

			// Set Alice as block author, who will receive fees
			RuntimeHelper::run_to_block(2, AccountId::from(ALICE));

			// We are going to buy 4e9 weight
			let bought = Weight::from_parts(4_000_000_000u64, 0);

			// Lets calculate amount needed
			let asset_amount_needed = ForeignAssetFeeAsExistentialDepositMultiplierFeeCharger::charge_weight_in_fungibles(foreign_location.clone(), bought)
			.expect("failed to compute");

			// Lets pay with: asset_amount_needed + asset_amount_extra
			let asset_amount_extra = 100_u128;
			let asset: Asset =
				(asset_location_v5.clone(), asset_amount_needed + asset_amount_extra).into();

			let mut trader = <XcmConfig as xcm_executor::Config>::Trader::new();
			let ctx = XcmContext { origin: None, message_id: XcmHash::default(), topic: None };

			// Lets buy_weight and make sure buy_weight does not return an error
			let unused_assets = trader.buy_weight(bought, asset.into(), &ctx).expect("Expected Ok");
			// Check whether a correct amount of unused assets is returned
			assert_ok!(
				unused_assets.ensure_contains(&(asset_location_v5, asset_amount_extra).into())
			);

			// Drop trader
			drop(trader);

			// Make sure author(Alice) has received the amount
			assert_eq!(
				ForeignAssets::balance(foreign_location.clone(), AccountId::from(ALICE)),
				minimum_asset_balance + asset_amount_needed
			);

			// We also need to ensure the total supply increased
			assert_eq!(
				ForeignAssets::total_supply(foreign_location),
				minimum_asset_balance + asset_amount_needed
			);
		});
}

#[test]
fn test_asset_xcm_take_first_trader_with_refund() {
	ExtBuilder::<Runtime>::default()
		.with_collators(vec![AccountId::from(ALICE)])
		.with_session_keys(vec![(
			AccountId::from(ALICE),
			AccountId::from(ALICE),
			SessionKeys { aura: AuraId::from(sp_core::sr25519::Public::from_raw(ALICE)) },
		)])
		.build()
		.execute_with(|| {
			// We need root origin to create a sufficient asset
			// We set existential deposit to be identical to the one for Balances first
			assert_ok!(Assets::force_create(
				RuntimeHelper::root_origin(),
				1.into(),
				AccountId::from(ALICE).into(),
				true,
				ExistentialDeposit::get()
			));

			// We first mint enough asset for the account to exist for assets
			assert_ok!(Assets::mint(
				RuntimeHelper::origin_of(AccountId::from(ALICE)),
				1.into(),
				AccountId::from(ALICE).into(),
				ExistentialDeposit::get()
			));

			let mut trader = <XcmConfig as xcm_executor::Config>::Trader::new();
			let ctx = XcmContext { origin: None, message_id: XcmHash::default(), topic: None };

			// Set Alice as block author, who will receive fees
			RuntimeHelper::run_to_block(2, AccountId::from(ALICE));

			// We are going to buy 4e9 weight
			let bought = Weight::from_parts(4_000_000_000u64, 0);
			let asset_location = AssetIdForTrustBackedAssetsConvert::convert_back(&1).unwrap();

			// lets calculate amount needed
			let amount_bought = WeightToFee::weight_to_fee(&bought);

			let asset: Asset = (asset_location.clone(), amount_bought).into();

			// Make sure buy_weight does not return an error
			assert_ok!(trader.buy_weight(bought, asset.clone().into(), &ctx));

			// Make sure again buy_weight does return an error
			// This assert relies on the fact, that we use `TakeFirstAssetTrader` in `WeightTrader`
			// tuple chain, which cannot be called twice
			assert_noop!(trader.buy_weight(bought, asset.into(), &ctx), XcmError::TooExpensive);

			// We actually use half of the weight
			let weight_used = bought / 2;

			// Make sure refund works.
			let amount_refunded = WeightToFee::weight_to_fee(&(bought - weight_used));

			assert_eq!(
				trader.refund_weight(bought - weight_used, &ctx),
				Some((asset_location, amount_refunded).into())
			);

			// Drop trader
			drop(trader);

			// We only should have paid for half of the bought weight
			let fees_paid = WeightToFee::weight_to_fee(&weight_used);

			assert_eq!(
				Assets::balance(1, AccountId::from(ALICE)),
				ExistentialDeposit::get() + fees_paid
			);

			// We also need to ensure the total supply increased
			assert_eq!(Assets::total_supply(1), ExistentialDeposit::get() + fees_paid);
		});
}

#[test]
fn test_asset_xcm_take_first_trader_refund_not_possible_since_amount_less_than_ed() {
	ExtBuilder::<Runtime>::default()
		.with_collators(vec![AccountId::from(ALICE)])
		.with_session_keys(vec![(
			AccountId::from(ALICE),
			AccountId::from(ALICE),
			SessionKeys { aura: AuraId::from(sp_core::sr25519::Public::from_raw(ALICE)) },
		)])
		.build()
		.execute_with(|| {
			// We need root origin to create a sufficient asset
			// We set existential deposit to be identical to the one for Balances first
			assert_ok!(Assets::force_create(
				RuntimeHelper::root_origin(),
				1.into(),
				AccountId::from(ALICE).into(),
				true,
				ExistentialDeposit::get()
			));

			let mut trader = <XcmConfig as xcm_executor::Config>::Trader::new();
			let ctx = XcmContext { origin: None, message_id: XcmHash::default(), topic: None };

			// Set Alice as block author, who will receive fees
			RuntimeHelper::run_to_block(2, AccountId::from(ALICE));

			// We are going to buy small amount
			let bought = Weight::from_parts(500_000_000u64, 0);

			let asset_location = AssetIdForTrustBackedAssetsConvert::convert_back(&1).unwrap();

			let amount_bought = WeightToFee::weight_to_fee(&bought);

			assert!(
				amount_bought < ExistentialDeposit::get(),
				"we are testing what happens when the amount does not exceed ED"
			);

			let asset: Asset = (asset_location, amount_bought).into();

			// Buy weight should return an error
			assert_noop!(trader.buy_weight(bought, asset.into(), &ctx), XcmError::TooExpensive);

			// not credited since the ED is higher than this value
			assert_eq!(Assets::balance(1, AccountId::from(ALICE)), 0);

			// We also need to ensure the total supply did not increase
			assert_eq!(Assets::total_supply(1), 0);
		});
}

#[test]
fn test_that_buying_ed_refund_does_not_refund_for_take_first_trader() {
	ExtBuilder::<Runtime>::default()
		.with_collators(vec![AccountId::from(ALICE)])
		.with_session_keys(vec![(
			AccountId::from(ALICE),
			AccountId::from(ALICE),
			SessionKeys { aura: AuraId::from(sp_core::sr25519::Public::from_raw(ALICE)) },
		)])
		.build()
		.execute_with(|| {
			// We need root origin to create a sufficient asset
			// We set existential deposit to be identical to the one for Balances first
			assert_ok!(Assets::force_create(
				RuntimeHelper::root_origin(),
				1.into(),
				AccountId::from(ALICE).into(),
				true,
				ExistentialDeposit::get()
			));

			let mut trader = <XcmConfig as xcm_executor::Config>::Trader::new();
			let ctx = XcmContext { origin: None, message_id: XcmHash::default(), topic: None };

			// Set Alice as block author, who will receive fees
			RuntimeHelper::run_to_block(2, AccountId::from(ALICE));

			let bought = Weight::from_parts(500_000_000u64, 0);

			let asset_location = AssetIdForTrustBackedAssetsConvert::convert_back(&1).unwrap();

			let amount_bought = WeightToFee::weight_to_fee(&bought);

			assert!(
				amount_bought < ExistentialDeposit::get(),
				"we are testing what happens when the amount does not exceed ED"
			);

			// We know we will have to buy at least ED, so lets make sure first it will
			// fail with a payment of less than ED
			let asset: Asset = (asset_location.clone(), amount_bought).into();
			assert_noop!(trader.buy_weight(bought, asset.into(), &ctx), XcmError::TooExpensive);

			// Now lets buy ED at least
			let asset: Asset = (asset_location.clone(), ExistentialDeposit::get()).into();

			// Buy weight should work
			assert_ok!(trader.buy_weight(bought, asset.into(), &ctx));

			// Should return None. We have a specific check making sure we don't go below ED for
			// drop payment
			assert_eq!(trader.refund_weight(bought, &ctx), None);

			// Drop trader
			drop(trader);

			// Make sure author(Alice) has received the amount
			assert_eq!(Assets::balance(1, AccountId::from(ALICE)), ExistentialDeposit::get());

			// We also need to ensure the total supply increased
			assert_eq!(Assets::total_supply(1), ExistentialDeposit::get());
		});
}

#[test]
fn test_asset_xcm_take_first_trader_not_possible_for_non_sufficient_assets() {
	ExtBuilder::<Runtime>::default()
		.with_collators(vec![AccountId::from(ALICE)])
		.with_session_keys(vec![(
			AccountId::from(ALICE),
			AccountId::from(ALICE),
			SessionKeys { aura: AuraId::from(sp_core::sr25519::Public::from_raw(ALICE)) },
		)])
		.build()
		.execute_with(|| {
			// Create a non-sufficient asset with specific existential deposit
			let minimum_asset_balance = 1_000_000_u128;
			assert_ok!(Assets::force_create(
				RuntimeHelper::root_origin(),
				1.into(),
				AccountId::from(ALICE).into(),
				false,
				minimum_asset_balance
			));

			// We first mint enough asset for the account to exist for assets
			assert_ok!(Assets::mint(
				RuntimeHelper::origin_of(AccountId::from(ALICE)),
				1.into(),
				AccountId::from(ALICE).into(),
				minimum_asset_balance
			));

			let mut trader = <XcmConfig as xcm_executor::Config>::Trader::new();
			let ctx = XcmContext { origin: None, message_id: XcmHash::default(), topic: None };

			// Set Alice as block author, who will receive fees
			RuntimeHelper::run_to_block(2, AccountId::from(ALICE));

			// We are going to buy 4e9 weight
			let bought = Weight::from_parts(4_000_000_000u64, 0);

			// lets calculate amount needed
			let asset_amount_needed = WeightToFee::weight_to_fee(&bought);

			let asset_location = AssetIdForTrustBackedAssetsConvert::convert_back(&1).unwrap();

			let asset: Asset = (asset_location, asset_amount_needed).into();

			// Make sure again buy_weight does return an error
			assert_noop!(trader.buy_weight(bought, asset.into(), &ctx), XcmError::TooExpensive);

			// Drop trader
			drop(trader);

			// Make sure author(Alice) has NOT received the amount
			assert_eq!(Assets::balance(1, AccountId::from(ALICE)), minimum_asset_balance);

			// We also need to ensure the total supply NOT increased
			assert_eq!(Assets::total_supply(1), minimum_asset_balance);
		});
}

#[test]
fn test_assets_balances_api_works() {
	use assets_common::runtime_api::runtime_decl_for_fungibles_api::FungiblesApi;

	ExtBuilder::<Runtime>::default()
		.with_collators(vec![AccountId::from(ALICE)])
		.with_session_keys(vec![(
			AccountId::from(ALICE),
			AccountId::from(ALICE),
			SessionKeys { aura: AuraId::from(sp_core::sr25519::Public::from_raw(ALICE)) },
		)])
		.build()
		.execute_with(|| {
			let local_asset_id = 1;
			let foreign_asset_id_location = xcm::v5::Location {
				parents: 1,
				interior: [
					xcm::v5::Junction::Parachain(1234),
					xcm::v5::Junction::GeneralIndex(12345),
				]
				.into(),
			};

			// check before
			assert_eq!(Assets::balance(local_asset_id, AccountId::from(ALICE)), 0);
			assert_eq!(
				ForeignAssets::balance(foreign_asset_id_location.clone(), AccountId::from(ALICE)),
				0
			);
			assert_eq!(Balances::free_balance(AccountId::from(ALICE)), 0);
			assert!(Runtime::query_account_balances(AccountId::from(ALICE))
				.unwrap()
				.try_as::<XcmAssets>()
				.unwrap()
				.is_none());

			// Drip some balance
			use frame_support::traits::fungible::Mutate;
			let some_currency = ExistentialDeposit::get();
			Balances::mint_into(&AccountId::from(ALICE), some_currency).unwrap();

			// We need root origin to create a sufficient asset
			let minimum_asset_balance = 3333333_u128;
			assert_ok!(Assets::force_create(
				RuntimeHelper::root_origin(),
				local_asset_id.into(),
				AccountId::from(ALICE).into(),
				true,
				minimum_asset_balance
			));

			// We first mint enough asset for the account to exist for assets
			assert_ok!(Assets::mint(
				RuntimeHelper::origin_of(AccountId::from(ALICE)),
				local_asset_id.into(),
				AccountId::from(ALICE).into(),
				minimum_asset_balance
			));

			// create foreign asset
			let foreign_asset_minimum_asset_balance = 3333333_u128;
			assert_ok!(ForeignAssets::force_create(
				RuntimeHelper::root_origin(),
				foreign_asset_id_location.clone(),
				AccountId::from(SOME_ASSET_ADMIN).into(),
				false,
				foreign_asset_minimum_asset_balance
			));

			// We first mint enough asset for the account to exist for assets
			assert_ok!(ForeignAssets::mint(
				RuntimeHelper::origin_of(AccountId::from(SOME_ASSET_ADMIN)),
				foreign_asset_id_location.clone(),
				AccountId::from(ALICE).into(),
				6 * foreign_asset_minimum_asset_balance
			));

			// check after
			assert_eq!(
				Assets::balance(local_asset_id, AccountId::from(ALICE)),
				minimum_asset_balance
			);
			assert_eq!(
				ForeignAssets::balance(foreign_asset_id_location.clone(), AccountId::from(ALICE)),
				6 * minimum_asset_balance
			);
			assert_eq!(Balances::free_balance(AccountId::from(ALICE)), some_currency);

			let result: XcmAssets = Runtime::query_account_balances(AccountId::from(ALICE))
				.unwrap()
				.try_into()
				.unwrap();
			assert_eq!(result.len(), 3);

			// check currency
			assert!(result.inner().iter().any(|asset| asset.eq(
				&assets_common::fungible_conversion::convert_balance::<WestendLocation, Balance>(
					some_currency
				)
				.unwrap()
			)));
			// check trusted asset
			assert!(result.inner().iter().any(|asset| asset.eq(&(
				AssetIdForTrustBackedAssetsConvert::convert_back(&local_asset_id).unwrap(),
				minimum_asset_balance
			)
				.into())));
			// check foreign asset
			assert!(result.inner().iter().any(|asset| asset.eq(&(
				WithLatestLocationConverter::<xcm::v5::Location>::convert_back(
					&foreign_asset_id_location
				)
				.unwrap(),
				6 * foreign_asset_minimum_asset_balance
			)
				.into())));
		});
}

asset_test_utils::include_teleports_for_native_asset_works!(
	Runtime,
	AllPalletsWithoutSystem,
	XcmConfig,
	CheckingAccount,
	WeightToFee,
	ParachainSystem,
	collator_session_keys(),
	slot_durations(),
	ExistentialDeposit::get(),
	Box::new(|runtime_event_encoded: Vec<u8>| {
		match RuntimeEvent::decode(&mut &runtime_event_encoded[..]) {
			Ok(RuntimeEvent::PolkadotXcm(event)) => Some(event),
			_ => None,
		}
	}),
	1000
);

asset_test_utils::include_teleports_for_foreign_assets_works!(
	Runtime,
	AllPalletsWithoutSystem,
	XcmConfig,
	CheckingAccount,
	WeightToFee,
	ParachainSystem,
	ForeignCreatorsSovereignAccountOf,
	ForeignAssetsInstance,
	collator_session_keys(),
	slot_durations(),
	ExistentialDeposit::get(),
	Box::new(|runtime_event_encoded: Vec<u8>| {
		match RuntimeEvent::decode(&mut &runtime_event_encoded[..]) {
			Ok(RuntimeEvent::PolkadotXcm(event)) => Some(event),
			_ => None,
		}
	}),
	Box::new(|runtime_event_encoded: Vec<u8>| {
		match RuntimeEvent::decode(&mut &runtime_event_encoded[..]) {
			Ok(RuntimeEvent::XcmpQueue(event)) => Some(event),
			_ => None,
		}
	})
);

asset_test_utils::include_asset_transactor_transfer_with_local_consensus_currency_works!(
	Runtime,
	XcmConfig,
	collator_session_keys(),
	ExistentialDeposit::get(),
	Box::new(|| {
		assert!(Assets::asset_ids().collect::<Vec<_>>().is_empty());
		assert!(ForeignAssets::asset_ids().collect::<Vec<_>>().is_empty());
	}),
	Box::new(|| {
		assert!(Assets::asset_ids().collect::<Vec<_>>().is_empty());
		assert!(ForeignAssets::asset_ids().collect::<Vec<_>>().is_empty());
	})
);

asset_test_utils::include_asset_transactor_transfer_with_pallet_assets_instance_works!(
	asset_transactor_transfer_with_trust_backed_assets_works,
	Runtime,
	XcmConfig,
	TrustBackedAssetsInstance,
	AssetIdForTrustBackedAssets,
	AssetIdForTrustBackedAssetsConvert,
	collator_session_keys(),
	ExistentialDeposit::get(),
	12345,
	Box::new(|| {
		assert!(ForeignAssets::asset_ids().collect::<Vec<_>>().is_empty());
	}),
	Box::new(|| {
		assert!(ForeignAssets::asset_ids().collect::<Vec<_>>().is_empty());
	})
);

asset_test_utils::include_asset_transactor_transfer_with_pallet_assets_instance_works!(
	asset_transactor_transfer_with_foreign_assets_works,
	Runtime,
	XcmConfig,
	ForeignAssetsInstance,
	xcm::v5::Location,
	JustTry,
	collator_session_keys(),
	ExistentialDeposit::get(),
	xcm::v5::Location {
		parents: 1,
		interior: [xcm::v5::Junction::Parachain(1313), xcm::v5::Junction::GeneralIndex(12345)]
			.into()
	},
	Box::new(|| {
		assert!(Assets::asset_ids().collect::<Vec<_>>().is_empty());
	}),
	Box::new(|| {
		assert!(Assets::asset_ids().collect::<Vec<_>>().is_empty());
	})
);

asset_test_utils::include_create_and_manage_foreign_assets_for_local_consensus_parachain_assets_works!(
	Runtime,
	XcmConfig,
	WeightToFee,
	ForeignCreatorsSovereignAccountOf,
	ForeignAssetsInstance,
	xcm::v5::Location,
	WithLatestLocationConverter<xcm::v5::Location>,
	collator_session_keys(),
	ExistentialDeposit::get(),
	AssetDeposit::get(),
	MetadataDepositBase::get(),
	MetadataDepositPerByte::get(),
	Box::new(|pallet_asset_call| RuntimeCall::ForeignAssets(pallet_asset_call).encode()),
	Box::new(|runtime_event_encoded: Vec<u8>| {
		match RuntimeEvent::decode(&mut &runtime_event_encoded[..]) {
			Ok(RuntimeEvent::ForeignAssets(pallet_asset_event)) => Some(pallet_asset_event),
			_ => None,
		}
	}),
	Box::new(|| {
		assert!(Assets::asset_ids().collect::<Vec<_>>().is_empty());
		assert!(ForeignAssets::asset_ids().collect::<Vec<_>>().is_empty());
	}),
	Box::new(|| {
		assert!(Assets::asset_ids().collect::<Vec<_>>().is_empty());
		assert_eq!(ForeignAssets::asset_ids().collect::<Vec<_>>().len(), 1);
	})
);

fn bridging_to_asset_hub_rococo() -> TestBridgingConfig {
	let _ = PolkadotXcm::force_xcm_version(
		RuntimeOrigin::root(),
		Box::new(bridging::to_rococo::AssetHubRococo::get()),
		XCM_VERSION,
	)
	.expect("version saved!");
	TestBridgingConfig {
		bridged_network: bridging::to_rococo::RococoNetwork::get(),
		local_bridge_hub_para_id: bridging::SiblingBridgeHubParaId::get(),
		local_bridge_hub_location: bridging::SiblingBridgeHub::get(),
		bridged_target_location: bridging::to_rococo::AssetHubRococo::get(),
	}
}

#[test]
fn limited_reserve_transfer_assets_for_native_asset_to_asset_hub_rococo_works() {
	asset_test_utils::test_cases_over_bridge::limited_reserve_transfer_assets_for_native_asset_works::<
		Runtime,
		AllPalletsWithoutSystem,
		XcmConfig,
		ParachainSystem,
		XcmpQueue,
		LocationToAccountId,
	>(
		collator_session_keys(),
		slot_durations(),
		ExistentialDeposit::get(),
		AccountId::from(ALICE),
		Box::new(|runtime_event_encoded: Vec<u8>| {
			match RuntimeEvent::decode(&mut &runtime_event_encoded[..]) {
				Ok(RuntimeEvent::PolkadotXcm(event)) => Some(event),
				_ => None,
			}
		}),
		Box::new(|runtime_event_encoded: Vec<u8>| {
			match RuntimeEvent::decode(&mut &runtime_event_encoded[..]) {
				Ok(RuntimeEvent::XcmpQueue(event)) => Some(event),
				_ => None,
			}
		}),
		bridging_to_asset_hub_rococo,
		WeightLimit::Unlimited,
		Some(xcm_config::bridging::XcmBridgeHubRouterFeeAssetId::get()),
		Some(xcm_config::TreasuryAccount::get()),
	)
}

#[test]
fn receive_reserve_asset_deposited_roc_from_asset_hub_rococo_fees_paid_by_pool_swap_works() {
	const BLOCK_AUTHOR_ACCOUNT: [u8; 32] = [13; 32];
	let block_author_account = AccountId::from(BLOCK_AUTHOR_ACCOUNT);
	let staking_pot = StakingPot::get();

	let foreign_asset_id_location =
		xcm::v5::Location::new(2, [xcm::v5::Junction::GlobalConsensus(xcm::v5::NetworkId::Rococo)]);
	let foreign_asset_id_minimum_balance = 1_000_000_000;
	// sovereign account as foreign asset owner (can be whoever for this scenario)
	let foreign_asset_owner = LocationToAccountId::convert_location(&Location::parent()).unwrap();
	let foreign_asset_create_params =
		(foreign_asset_owner, foreign_asset_id_location.clone(), foreign_asset_id_minimum_balance);

	asset_test_utils::test_cases_over_bridge::receive_reserve_asset_deposited_from_different_consensus_works::<
			Runtime,
			AllPalletsWithoutSystem,
			XcmConfig,
			ForeignAssetsInstance,
		>(
			collator_session_keys().add(collator_session_key(BLOCK_AUTHOR_ACCOUNT)),
			ExistentialDeposit::get(),
			AccountId::from([73; 32]),
			block_author_account.clone(),
			// receiving ROCs
			foreign_asset_create_params.clone(),
			1000000000000,
			|| {
				// setup pool for paying fees to touch `SwapFirstAssetTrader`
				setup_pool_for_paying_fees_with_foreign_assets(foreign_asset_create_params);
				// staking pot account for collecting local native fees from `BuyExecution`
				let _ = Balances::force_set_balance(RuntimeOrigin::root(), StakingPot::get().into(), ExistentialDeposit::get());
				// prepare bridge configuration
				bridging_to_asset_hub_rococo()
			},
			(
				[PalletInstance(bp_bridge_hub_westend::WITH_BRIDGE_WESTEND_TO_ROCOCO_MESSAGES_PALLET_INDEX)].into(),
				GlobalConsensus(Rococo),
				[Parachain(1000)].into()
			),
			|| {
				// check staking pot for ED
				assert_eq!(Balances::free_balance(&staking_pot), ExistentialDeposit::get());
				// check now foreign asset for staking pot
				assert_eq!(
					ForeignAssets::balance(
						foreign_asset_id_location.clone().into(),
						&staking_pot
					),
					0
				);
			},
			|| {
				// `SwapFirstAssetTrader` - staking pot receives xcm fees in ROCs
				assert!(
					Balances::free_balance(&staking_pot) > ExistentialDeposit::get()
				);
				// staking pot receives no foreign assets
				assert_eq!(
					ForeignAssets::balance(
						foreign_asset_id_location.clone().into(),
						&staking_pot
					),
					0
				);
			}
		)
}

#[test]
fn receive_reserve_asset_deposited_roc_from_asset_hub_rococo_fees_paid_by_sufficient_asset_works() {
	const BLOCK_AUTHOR_ACCOUNT: [u8; 32] = [13; 32];
	let block_author_account = AccountId::from(BLOCK_AUTHOR_ACCOUNT);
	let staking_pot = StakingPot::get();

	let foreign_asset_id_location =
		xcm::v5::Location::new(2, [xcm::v5::Junction::GlobalConsensus(xcm::v5::NetworkId::Rococo)]);
	let foreign_asset_id_minimum_balance = 1_000_000_000;
	// sovereign account as foreign asset owner (can be whoever for this scenario)
	let foreign_asset_owner = LocationToAccountId::convert_location(&Location::parent()).unwrap();
	let foreign_asset_create_params =
		(foreign_asset_owner, foreign_asset_id_location.clone(), foreign_asset_id_minimum_balance);

	asset_test_utils::test_cases_over_bridge::receive_reserve_asset_deposited_from_different_consensus_works::<
		Runtime,
		AllPalletsWithoutSystem,
		XcmConfig,
		ForeignAssetsInstance,
	>(
		collator_session_keys().add(collator_session_key(BLOCK_AUTHOR_ACCOUNT)),
		ExistentialDeposit::get(),
		AccountId::from([73; 32]),
		block_author_account.clone(),
		// receiving ROCs
		foreign_asset_create_params,
		1000000000000,
		bridging_to_asset_hub_rococo,
		(
			[PalletInstance(bp_bridge_hub_westend::WITH_BRIDGE_WESTEND_TO_ROCOCO_MESSAGES_PALLET_INDEX)].into(),
			GlobalConsensus(Rococo),
			[Parachain(1000)].into()
		),
		|| {
			// check block author before
			assert_eq!(
				ForeignAssets::balance(
					foreign_asset_id_location.clone().into(),
					&block_author_account
				),
				0
			);
		},
		|| {
			// `TakeFirstAssetTrader` puts fees to the block author
			assert!(
				ForeignAssets::balance(
					foreign_asset_id_location.clone().into(),
					&block_author_account
				) > 0
			);
			// `SwapFirstAssetTrader` did not work
			assert_eq!(Balances::free_balance(&staking_pot), 0);
		}
	)
}

#[test]
fn change_xcm_bridge_hub_router_byte_fee_by_governance_works() {
	asset_test_utils::test_cases::change_storage_constant_by_governance_works::<
		Runtime,
		bridging::XcmBridgeHubRouterByteFee,
		Balance,
	>(
		collator_session_keys(),
		1000,
		Box::new(|call| RuntimeCall::System(call).encode()),
		|| {
			(
				bridging::XcmBridgeHubRouterByteFee::key().to_vec(),
				bridging::XcmBridgeHubRouterByteFee::get(),
			)
		},
		|old_value| {
			if let Some(new_value) = old_value.checked_add(1) {
				new_value
			} else {
				old_value.checked_sub(1).unwrap()
			}
		},
	)
}

#[test]
fn change_xcm_bridge_hub_router_base_fee_by_governance_works() {
	asset_test_utils::test_cases::change_storage_constant_by_governance_works::<
		Runtime,
		bridging::XcmBridgeHubRouterBaseFee,
		Balance,
	>(
		collator_session_keys(),
		1000,
		Box::new(|call| RuntimeCall::System(call).encode()),
		|| {
			log::error!(
				target: "bridges::estimate",
				"`bridging::XcmBridgeHubRouterBaseFee` actual value: {} for runtime: {}",
				bridging::XcmBridgeHubRouterBaseFee::get(),
				<Runtime as frame_system::Config>::Version::get(),
			);
			(
				bridging::XcmBridgeHubRouterBaseFee::key().to_vec(),
				bridging::XcmBridgeHubRouterBaseFee::get(),
			)
		},
		|old_value| {
			if let Some(new_value) = old_value.checked_add(1) {
				new_value
			} else {
				old_value.checked_sub(1).unwrap()
			}
		},
	)
}

#[test]
fn reserve_transfer_native_asset_to_non_teleport_para_works() {
	asset_test_utils::test_cases::reserve_transfer_native_asset_to_non_teleport_para_works::<
		Runtime,
		AllPalletsWithoutSystem,
		XcmConfig,
		ParachainSystem,
		XcmpQueue,
		LocationToAccountId,
	>(
		collator_session_keys(),
		slot_durations(),
		ExistentialDeposit::get(),
		AccountId::from(ALICE),
		Box::new(|runtime_event_encoded: Vec<u8>| {
			match RuntimeEvent::decode(&mut &runtime_event_encoded[..]) {
				Ok(RuntimeEvent::PolkadotXcm(event)) => Some(event),
				_ => None,
			}
		}),
		Box::new(|runtime_event_encoded: Vec<u8>| {
			match RuntimeEvent::decode(&mut &runtime_event_encoded[..]) {
				Ok(RuntimeEvent::XcmpQueue(event)) => Some(event),
				_ => None,
			}
		}),
		WeightLimit::Unlimited,
	);
}

#[test]
fn location_conversion_works() {
	// the purpose of hardcoded values is to catch an unintended location conversion logic change.
	struct TestCase {
		description: &'static str,
		location: Location,
		expected_account_id_str: &'static str,
	}

	let test_cases = vec![
		// DescribeTerminus
		TestCase {
			description: "DescribeTerminus Parent",
			location: Location::new(1, Here),
			expected_account_id_str: "5Dt6dpkWPwLaH4BBCKJwjiWrFVAGyYk3tLUabvyn4v7KtESG",
		},
		TestCase {
			description: "DescribeTerminus Sibling",
			location: Location::new(1, [Parachain(1111)]),
			expected_account_id_str: "5Eg2fnssmmJnF3z1iZ1NouAuzciDaaDQH7qURAy3w15jULDk",
		},
		// DescribePalletTerminal
		TestCase {
			description: "DescribePalletTerminal Parent",
			location: Location::new(1, [PalletInstance(50)]),
			expected_account_id_str: "5CnwemvaAXkWFVwibiCvf2EjqwiqBi29S5cLLydZLEaEw6jZ",
		},
		TestCase {
			description: "DescribePalletTerminal Sibling",
			location: Location::new(1, [Parachain(1111), PalletInstance(50)]),
			expected_account_id_str: "5GFBgPjpEQPdaxEnFirUoa51u5erVx84twYxJVuBRAT2UP2g",
		},
		// DescribeAccountId32Terminal
		TestCase {
			description: "DescribeAccountId32Terminal Parent",
			location: Location::new(
				1,
				[AccountId32 { network: None, id: AccountId::from(ALICE).into() }],
			),
			expected_account_id_str: "5DN5SGsuUG7PAqFL47J9meViwdnk9AdeSWKFkcHC45hEzVz4",
		},
		TestCase {
			description: "DescribeAccountId32Terminal Sibling",
			location: Location::new(
				1,
				[
					Parachain(1111),
					Junction::AccountId32 { network: None, id: AccountId::from(ALICE).into() },
				],
			),
			expected_account_id_str: "5DGRXLYwWGce7wvm14vX1Ms4Vf118FSWQbJkyQigY2pfm6bg",
		},
		// DescribeAccountKey20Terminal
		TestCase {
			description: "DescribeAccountKey20Terminal Parent",
			location: Location::new(1, [AccountKey20 { network: None, key: [0u8; 20] }]),
			expected_account_id_str: "5F5Ec11567pa919wJkX6VHtv2ZXS5W698YCW35EdEbrg14cg",
		},
		TestCase {
			description: "DescribeAccountKey20Terminal Sibling",
			location: Location::new(
				1,
				[Parachain(1111), AccountKey20 { network: None, key: [0u8; 20] }],
			),
			expected_account_id_str: "5CB2FbUds2qvcJNhDiTbRZwiS3trAy6ydFGMSVutmYijpPAg",
		},
		// DescribeTreasuryVoiceTerminal
		TestCase {
			description: "DescribeTreasuryVoiceTerminal Parent",
			location: Location::new(1, [Plurality { id: BodyId::Treasury, part: BodyPart::Voice }]),
			expected_account_id_str: "5CUjnE2vgcUCuhxPwFoQ5r7p1DkhujgvMNDHaF2bLqRp4D5F",
		},
		TestCase {
			description: "DescribeTreasuryVoiceTerminal Sibling",
			location: Location::new(
				1,
				[Parachain(1111), Plurality { id: BodyId::Treasury, part: BodyPart::Voice }],
			),
			expected_account_id_str: "5G6TDwaVgbWmhqRUKjBhRRnH4ry9L9cjRymUEmiRsLbSE4gB",
		},
		// DescribeBodyTerminal
		TestCase {
			description: "DescribeBodyTerminal Parent",
			location: Location::new(1, [Plurality { id: BodyId::Unit, part: BodyPart::Voice }]),
			expected_account_id_str: "5EBRMTBkDisEXsaN283SRbzx9Xf2PXwUxxFCJohSGo4jYe6B",
		},
		TestCase {
			description: "DescribeBodyTerminal Sibling",
			location: Location::new(
				1,
				[Parachain(1111), Plurality { id: BodyId::Unit, part: BodyPart::Voice }],
			),
			expected_account_id_str: "5DBoExvojy8tYnHgLL97phNH975CyT45PWTZEeGoBZfAyRMH",
		},
	];

	for tc in test_cases {
		let expected =
			AccountId::from_string(tc.expected_account_id_str).expect("Invalid AccountId string");

		let got = LocationToAccountHelper::<AccountId, LocationToAccountId>::convert_location(
			tc.location.into(),
		)
		.unwrap();

		assert_eq!(got, expected, "{}", tc.description);
	}
}
