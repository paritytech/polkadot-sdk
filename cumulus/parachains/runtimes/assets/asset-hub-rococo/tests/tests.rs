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

//! Tests for the Rococo Assets Hub chain.

use asset_hub_rococo_runtime::xcm_config::{
	AssetFeeAsExistentialDepositMultiplierFeeCharger, TokenLocation,
	TrustBackedAssetsPalletLocation,
};
pub use asset_hub_rococo_runtime::{
	xcm_config::{
		self, bridging, CheckingAccount, ForeignCreatorsSovereignAccountOf, LocationToAccountId,
		XcmConfig,
	},
	AllPalletsWithoutSystem, AssetDeposit, Assets, Balances, ExistentialDeposit, ForeignAssets,
	ForeignAssetsInstance, MetadataDepositBase, MetadataDepositPerByte, ParachainSystem, Runtime,
	RuntimeCall, RuntimeEvent, RuntimeFlavor, SessionKeys, System, ToRococoXcmRouterInstance,
	ToWococoXcmRouterInstance, TrustBackedAssetsInstance, XcmpQueue,
};
use asset_test_utils::{CollatorSessionKey, CollatorSessionKeys, ExtBuilder};
use codec::{Decode, Encode};
use cumulus_primitives_utility::ChargeWeightInFungibles;
use frame_support::{
	assert_noop, assert_ok,
	traits::{fungibles::InspectEnumerable, Contains},
	weights::{Weight, WeightToFee as WeightToFeeT},
};
use parachains_common::{
	rococo::fee::WeightToFee, AccountId, AssetIdForTrustBackedAssets, AuraId, Balance,
};
use sp_runtime::traits::MaybeEquivalence;
use xcm::latest::prelude::*;
use xcm_executor::traits::{Identity, JustTry, WeightTrader};

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

#[test]
fn test_asset_xcm_trader() {
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

			// get asset id as multilocation
			let asset_multilocation =
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
			let asset: MultiAsset =
				(asset_multilocation, asset_amount_needed + asset_amount_extra).into();

			let mut trader = <XcmConfig as xcm_executor::Config>::Trader::new();
			let ctx = XcmContext { origin: None, message_id: XcmHash::default(), topic: None };

			// Lets buy_weight and make sure buy_weight does not return an error
			let unused_assets = trader.buy_weight(bought, asset.into(), &ctx).expect("Expected Ok");
			// Check whether a correct amount of unused assets is returned
			assert_ok!(
				unused_assets.ensure_contains(&(asset_multilocation, asset_amount_extra).into())
			);

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
fn test_asset_xcm_trader_with_refund() {
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

			let asset_multilocation = AssetIdForTrustBackedAssetsConvert::convert_back(&1).unwrap();

			// lets calculate amount needed
			let amount_bought = WeightToFee::weight_to_fee(&bought);

			let asset: MultiAsset = (asset_multilocation, amount_bought).into();

			// Make sure buy_weight does not return an error
			assert_ok!(trader.buy_weight(bought, asset.clone().into(), &ctx));

			// Make sure again buy_weight does return an error
			// This assert relies on the fact, that we use `TakeFirstAssetTrader` in `WeightTrader`
			// tuple chain, which cannot be called twice
			assert_noop!(trader.buy_weight(bought, asset.into(), &ctx), XcmError::TooExpensive);

			// We actually use half of the weight
			let weight_used = bought / 2;

			// Make sure refurnd works.
			let amount_refunded = WeightToFee::weight_to_fee(&(bought - weight_used));

			assert_eq!(
				trader.refund_weight(bought - weight_used, &ctx),
				Some((asset_multilocation, amount_refunded).into())
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
fn test_asset_xcm_trader_refund_not_possible_since_amount_less_than_ed() {
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

			let asset_multilocation = AssetIdForTrustBackedAssetsConvert::convert_back(&1).unwrap();

			let amount_bought = WeightToFee::weight_to_fee(&bought);

			assert!(
				amount_bought < ExistentialDeposit::get(),
				"we are testing what happens when the amount does not exceed ED"
			);

			let asset: MultiAsset = (asset_multilocation, amount_bought).into();

			// Buy weight should return an error
			assert_noop!(trader.buy_weight(bought, asset.into(), &ctx), XcmError::TooExpensive);

			// not credited since the ED is higher than this value
			assert_eq!(Assets::balance(1, AccountId::from(ALICE)), 0);

			// We also need to ensure the total supply did not increase
			assert_eq!(Assets::total_supply(1), 0);
		});
}

#[test]
fn test_that_buying_ed_refund_does_not_refund() {
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

			// We are gonna buy ED
			let bought = Weight::from_parts(ExistentialDeposit::get().try_into().unwrap(), 0);

			let asset_multilocation = AssetIdForTrustBackedAssetsConvert::convert_back(&1).unwrap();

			let amount_bought = WeightToFee::weight_to_fee(&bought);

			assert!(
				amount_bought < ExistentialDeposit::get(),
				"we are testing what happens when the amount does not exceed ED"
			);

			// We know we will have to buy at least ED, so lets make sure first it will
			// fail with a payment of less than ED
			let asset: MultiAsset = (asset_multilocation, amount_bought).into();
			assert_noop!(trader.buy_weight(bought, asset.into(), &ctx), XcmError::TooExpensive);

			// Now lets buy ED at least
			let asset: MultiAsset = (asset_multilocation, ExistentialDeposit::get()).into();

			// Buy weight should work
			assert_ok!(trader.buy_weight(bought, asset.into(), &ctx));

			// Should return None. We have a specific check making sure we dont go below ED for
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
fn test_asset_xcm_trader_not_possible_for_non_sufficient_assets() {
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

			let asset_multilocation = AssetIdForTrustBackedAssetsConvert::convert_back(&1).unwrap();

			let asset: MultiAsset = (asset_multilocation, asset_amount_needed).into();

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
			let foreign_asset_id_multilocation =
				MultiLocation { parents: 1, interior: X2(Parachain(1234), GeneralIndex(12345)) };

			// check before
			assert_eq!(Assets::balance(local_asset_id, AccountId::from(ALICE)), 0);
			assert_eq!(
				ForeignAssets::balance(foreign_asset_id_multilocation, AccountId::from(ALICE)),
				0
			);
			assert_eq!(Balances::free_balance(AccountId::from(ALICE)), 0);
			assert!(Runtime::query_account_balances(AccountId::from(ALICE))
				.unwrap()
				.try_as::<MultiAssets>()
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
				foreign_asset_id_multilocation,
				AccountId::from(SOME_ASSET_ADMIN).into(),
				false,
				foreign_asset_minimum_asset_balance
			));

			// We first mint enough asset for the account to exist for assets
			assert_ok!(ForeignAssets::mint(
				RuntimeHelper::origin_of(AccountId::from(SOME_ASSET_ADMIN)),
				foreign_asset_id_multilocation,
				AccountId::from(ALICE).into(),
				6 * foreign_asset_minimum_asset_balance
			));

			// check after
			assert_eq!(
				Assets::balance(local_asset_id, AccountId::from(ALICE)),
				minimum_asset_balance
			);
			assert_eq!(
				ForeignAssets::balance(foreign_asset_id_multilocation, AccountId::from(ALICE)),
				6 * minimum_asset_balance
			);
			assert_eq!(Balances::free_balance(AccountId::from(ALICE)), some_currency);

			let result: MultiAssets = Runtime::query_account_balances(AccountId::from(ALICE))
				.unwrap()
				.try_into()
				.unwrap();
			assert_eq!(result.len(), 3);

			// check currency
			assert!(result.inner().iter().any(|asset| asset.eq(
				&assets_common::fungible_conversion::convert_balance::<TokenLocation, Balance>(
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
				Identity::convert_back(&foreign_asset_id_multilocation).unwrap(),
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
	MultiLocation,
	JustTry,
	collator_session_keys(),
	ExistentialDeposit::get(),
	MultiLocation { parents: 1, interior: X2(Parachain(1313), GeneralIndex(12345)) },
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
	MultiLocation,
	JustTry,
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

mod asset_hub_rococo_tests {
	use super::*;

	fn bridging_to_asset_hub_wococo() -> asset_test_utils::test_cases_over_bridge::TestBridgingConfig
	{
		asset_test_utils::test_cases_over_bridge::TestBridgingConfig {
			bridged_network: bridging::to_wococo::WococoNetwork::get(),
			local_bridge_hub_para_id: bridging::SiblingBridgeHubParaId::get(),
			local_bridge_hub_location: bridging::SiblingBridgeHub::get(),
			bridged_target_location: bridging::to_wococo::AssetHubWococo::get(),
		}
	}

	#[test]
	fn limited_reserve_transfer_assets_for_native_asset_over_bridge_works() {
		asset_test_utils::test_cases_over_bridge::limited_reserve_transfer_assets_for_native_asset_works::<
			Runtime,
			AllPalletsWithoutSystem,
			XcmConfig,
			ParachainSystem,
			XcmpQueue,
			LocationToAccountId,
		>(
			collator_session_keys(),
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
			bridging_to_asset_hub_wococo,
			WeightLimit::Unlimited,
			Some(xcm_config::bridging::XcmBridgeHubRouterFeeAssetId::get()),
		)
	}

	#[test]
	fn receive_reserve_asset_deposited_woc_from_asset_hub_wococo_works() {
		const BLOCK_AUTHOR_ACCOUNT: [u8; 32] = [13; 32];
		asset_test_utils::test_cases_over_bridge::receive_reserve_asset_deposited_from_different_consensus_works::<
			Runtime,
			AllPalletsWithoutSystem,
			XcmConfig,
			LocationToAccountId,
			ForeignAssetsInstance,
		>(
			collator_session_keys().add(collator_session_key(BLOCK_AUTHOR_ACCOUNT)),
			ExistentialDeposit::get(),
			AccountId::from([73; 32]),
			AccountId::from(BLOCK_AUTHOR_ACCOUNT),
			// receiving WOCs
			(MultiLocation { parents: 2, interior: X1(GlobalConsensus(Wococo)) }, 1000000000000, 1_000_000_000),
			bridging_to_asset_hub_wococo,
			(
				X1(PalletInstance(bp_bridge_hub_rococo::WITH_BRIDGE_ROCOCO_TO_WOCOCO_MESSAGES_PALLET_INDEX)),
				GlobalConsensus(Wococo),
				X1(Parachain(1000))
			)
		)
	}

	#[test]
	fn report_bridge_status_from_xcm_bridge_router_works() {
		asset_test_utils::test_cases_over_bridge::report_bridge_status_from_xcm_bridge_router_works::<
			Runtime,
			AllPalletsWithoutSystem,
			XcmConfig,
			ParachainSystem,
			XcmpQueue,
			LocationToAccountId,
			ToWococoXcmRouterInstance,
		>(
			collator_session_keys(),
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
			bridging_to_asset_hub_wococo,
			WeightLimit::Unlimited,
			Some(xcm_config::bridging::XcmBridgeHubRouterFeeAssetId::get()),
			|| {
				sp_std::vec![
					UnpaidExecution { weight_limit: Unlimited, check_origin: None },
					Transact {
						origin_kind: OriginKind::Xcm,
						require_weight_at_most:
							bp_asset_hub_rococo::XcmBridgeHubRouterTransactCallMaxWeight::get(),
						call: bp_asset_hub_rococo::Call::ToWococoXcmRouter(
							bp_asset_hub_rococo::XcmBridgeHubRouterCall::report_bridge_status {
								bridge_id: Default::default(),
								is_congested: true,
							}
						)
						.encode()
						.into(),
					}
				]
				.into()
			},
			|| {
				sp_std::vec![
					UnpaidExecution { weight_limit: Unlimited, check_origin: None },
					Transact {
						origin_kind: OriginKind::Xcm,
						require_weight_at_most:
							bp_asset_hub_rococo::XcmBridgeHubRouterTransactCallMaxWeight::get(),
						call: bp_asset_hub_rococo::Call::ToWococoXcmRouter(
							bp_asset_hub_rococo::XcmBridgeHubRouterCall::report_bridge_status {
								bridge_id: Default::default(),
								is_congested: false,
							}
						)
						.encode()
						.into(),
					}
				]
				.into()
			},
		)
	}

	#[test]
	fn test_report_bridge_status_call_compatibility() {
		// if this test fails, make sure `bp_asset_hub_rococo` has valid encoding
		assert_eq!(
			RuntimeCall::ToWococoXcmRouter(
				pallet_xcm_bridge_hub_router::Call::report_bridge_status {
					bridge_id: Default::default(),
					is_congested: true,
				}
			)
			.encode(),
			bp_asset_hub_rococo::Call::ToWococoXcmRouter(
				bp_asset_hub_rococo::XcmBridgeHubRouterCall::report_bridge_status {
					bridge_id: Default::default(),
					is_congested: true,
				}
			)
			.encode()
		)
	}

	#[test]
	fn check_sane_weight_report_bridge_status() {
		use pallet_xcm_bridge_hub_router::WeightInfo;
		let actual = <Runtime as pallet_xcm_bridge_hub_router::Config<
			ToWococoXcmRouterInstance,
		>>::WeightInfo::report_bridge_status();
		let max_weight = bp_asset_hub_rococo::XcmBridgeHubRouterTransactCallMaxWeight::get();
		assert!(
			actual.all_lte(max_weight),
			"max_weight: {:?} should be adjusted to actual {:?}",
			max_weight,
			actual
		);
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
}

mod asset_hub_wococo_tests {
	use super::*;

	fn bridging_to_asset_hub_rococo() -> asset_test_utils::test_cases_over_bridge::TestBridgingConfig
	{
		asset_test_utils::test_cases_over_bridge::TestBridgingConfig {
			bridged_network: bridging::to_rococo::RococoNetwork::get(),
			local_bridge_hub_para_id: bridging::SiblingBridgeHubParaId::get(),
			local_bridge_hub_location: bridging::SiblingBridgeHub::get(),
			bridged_target_location: bridging::to_rococo::AssetHubRococo::get(),
		}
	}

	pub(crate) fn set_wococo_flavor() {
		let flavor_key = xcm_config::Flavor::key().to_vec();
		let flavor = RuntimeFlavor::Wococo;

		// encode `set_storage` call
		let set_storage_call = RuntimeCall::System(frame_system::Call::<Runtime>::set_storage {
			items: vec![(flavor_key, flavor.encode())],
		})
		.encode();

		// estimate - storing just 1 value
		use frame_system::WeightInfo;
		let require_weight_at_most =
			<Runtime as frame_system::Config>::SystemWeightInfo::set_storage(1);

		// execute XCM with Transact to `set_storage` as governance does
		assert_ok!(RuntimeHelper::execute_as_governance(set_storage_call, require_weight_at_most)
			.ensure_complete());

		// check if stored
		assert_eq!(flavor, xcm_config::Flavor::get());
	}

	fn with_wococo_flavor_bridging_to_asset_hub_rococo(
	) -> asset_test_utils::test_cases_over_bridge::TestBridgingConfig {
		set_wococo_flavor();
		bridging_to_asset_hub_rococo()
	}

	#[test]
	fn limited_reserve_transfer_assets_for_native_asset_over_bridge_works() {
		asset_test_utils::test_cases_over_bridge::limited_reserve_transfer_assets_for_native_asset_works::<
			Runtime,
			AllPalletsWithoutSystem,
			XcmConfig,
			ParachainSystem,
			XcmpQueue,
			LocationToAccountId,
		>(
			collator_session_keys(),
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
			with_wococo_flavor_bridging_to_asset_hub_rococo,
			WeightLimit::Unlimited,
			Some(xcm_config::bridging::XcmBridgeHubRouterFeeAssetId::get()),
		)
	}

	#[test]
	fn receive_reserve_asset_deposited_roc_from_asset_hub_rococo_works() {
		const BLOCK_AUTHOR_ACCOUNT: [u8; 32] = [13; 32];
		asset_test_utils::test_cases_over_bridge::receive_reserve_asset_deposited_from_different_consensus_works::<
			Runtime,
			AllPalletsWithoutSystem,
			XcmConfig,
			LocationToAccountId,
			ForeignAssetsInstance,
		>(
			collator_session_keys().add(collator_session_key(BLOCK_AUTHOR_ACCOUNT)),
			ExistentialDeposit::get(),
			AccountId::from([73; 32]),
			AccountId::from(BLOCK_AUTHOR_ACCOUNT),
			// receiving ROCs
			(MultiLocation { parents: 2, interior: X1(GlobalConsensus(Rococo)) }, 1000000000000, 1_000_000_000),
			with_wococo_flavor_bridging_to_asset_hub_rococo,
			(
				X1(PalletInstance(bp_bridge_hub_wococo::WITH_BRIDGE_WOCOCO_TO_ROCOCO_MESSAGES_PALLET_INDEX)),
				GlobalConsensus(Rococo),
				X1(Parachain(1000))
			)
		)
	}

	#[test]
	fn report_bridge_status_from_xcm_bridge_router_works() {
		asset_test_utils::test_cases_over_bridge::report_bridge_status_from_xcm_bridge_router_works::<
			Runtime,
			AllPalletsWithoutSystem,
			XcmConfig,
			ParachainSystem,
			XcmpQueue,
			LocationToAccountId,
			ToRococoXcmRouterInstance,
		>(
			collator_session_keys(),
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
			with_wococo_flavor_bridging_to_asset_hub_rococo,
			WeightLimit::Unlimited,
			Some(xcm_config::bridging::XcmBridgeHubRouterFeeAssetId::get()),
			|| {
				sp_std::vec![
					UnpaidExecution { weight_limit: Unlimited, check_origin: None },
					Transact {
						origin_kind: OriginKind::Xcm,
						require_weight_at_most:
							bp_asset_hub_wococo::XcmBridgeHubRouterTransactCallMaxWeight::get(),
						call: bp_asset_hub_wococo::Call::ToRococoXcmRouter(
							bp_asset_hub_wococo::XcmBridgeHubRouterCall::report_bridge_status {
								bridge_id: Default::default(),
								is_congested: true,
							}
						)
						.encode()
						.into(),
					}
				]
				.into()
			},
			|| {
				sp_std::vec![
					UnpaidExecution { weight_limit: Unlimited, check_origin: None },
					Transact {
						origin_kind: OriginKind::Xcm,
						require_weight_at_most:
							bp_asset_hub_wococo::XcmBridgeHubRouterTransactCallMaxWeight::get(),
						call: bp_asset_hub_wococo::Call::ToRococoXcmRouter(
							bp_asset_hub_wococo::XcmBridgeHubRouterCall::report_bridge_status {
								bridge_id: Default::default(),
								is_congested: false,
							}
						)
						.encode()
						.into(),
					}
				]
				.into()
			},
		)
	}

	#[test]
	fn test_report_bridge_status_call_compatibility() {
		// if this test fails, make sure `bp_asset_hub_rococo` has valid encoding
		assert_eq!(
			RuntimeCall::ToRococoXcmRouter(
				pallet_xcm_bridge_hub_router::Call::report_bridge_status {
					bridge_id: Default::default(),
					is_congested: true,
				}
			)
			.encode(),
			bp_asset_hub_wococo::Call::ToRococoXcmRouter(
				bp_asset_hub_wococo::XcmBridgeHubRouterCall::report_bridge_status {
					bridge_id: Default::default(),
					is_congested: true,
				}
			)
			.encode()
		)
	}

	#[test]
	fn check_sane_weight_report_bridge_status() {
		use pallet_xcm_bridge_hub_router::WeightInfo;
		let actual = <Runtime as pallet_xcm_bridge_hub_router::Config<
			ToRococoXcmRouterInstance,
		>>::WeightInfo::report_bridge_status();
		let max_weight = bp_asset_hub_wococo::XcmBridgeHubRouterTransactCallMaxWeight::get();
		assert!(
			actual.all_lte(max_weight),
			"max_weight: {:?} should be adjusted to actual {:?}",
			max_weight,
			actual
		);
	}
}

/// Tests expected configuration of isolated `pallet_xcm::Config::XcmReserveTransferFilter`.
#[test]
fn xcm_reserve_transfer_filter_works() {
	// prepare assets
	let only_native_assets = || vec![MultiAsset::from((TokenLocation::get(), 1000))];
	let only_trust_backed_assets = || {
		vec![MultiAsset::from((
			AssetIdForTrustBackedAssetsConvert::convert_back(&12345).unwrap(),
			2000,
		))]
	};
	let only_sibling_foreign_assets =
		|| vec![MultiAsset::from((MultiLocation::new(1, X1(Parachain(12345))), 3000))];
	let only_different_global_consensus_foreign_assets = || {
		vec![MultiAsset::from((
			MultiLocation::new(2, X2(GlobalConsensus(Wococo), Parachain(12345))),
			4000,
		))]
	};

	// prepare destinations
	let relaychain = MultiLocation::parent();
	let sibling_parachain = MultiLocation::new(1, X1(Parachain(54321)));
	let different_global_consensus_parachain_other_than_asset_hub_wococo =
		MultiLocation::new(2, X2(GlobalConsensus(Kusama), Parachain(12345)));
	let bridged_asset_hub_wococo = bridging::to_wococo::AssetHubWococo::get();
	let bridged_asset_hub_rococo = bridging::to_rococo::AssetHubRococo::get();

	// prepare expected test data sets: (destination, assets, expected_result)
	let test_data = vec![
		(relaychain, only_native_assets(), true),
		(relaychain, only_trust_backed_assets(), true),
		(relaychain, only_sibling_foreign_assets(), true),
		(relaychain, only_different_global_consensus_foreign_assets(), true),
		(sibling_parachain, only_native_assets(), true),
		(sibling_parachain, only_trust_backed_assets(), true),
		(sibling_parachain, only_sibling_foreign_assets(), true),
		(sibling_parachain, only_different_global_consensus_foreign_assets(), true),
		(
			different_global_consensus_parachain_other_than_asset_hub_wococo,
			only_native_assets(),
			false,
		),
		(
			different_global_consensus_parachain_other_than_asset_hub_wococo,
			only_trust_backed_assets(),
			false,
		),
		(
			different_global_consensus_parachain_other_than_asset_hub_wococo,
			only_sibling_foreign_assets(),
			false,
		),
		(
			different_global_consensus_parachain_other_than_asset_hub_wococo,
			only_different_global_consensus_foreign_assets(),
			false,
		),
	];

	let additional_test_data_for_rococo_flavor = vec![
		(bridged_asset_hub_wococo, only_native_assets(), true),
		(bridged_asset_hub_wococo, only_trust_backed_assets(), false),
		(bridged_asset_hub_wococo, only_sibling_foreign_assets(), false),
		(bridged_asset_hub_wococo, only_different_global_consensus_foreign_assets(), false),
	];
	let additional_test_data_for_wococo_flavor = vec![
		(bridged_asset_hub_rococo, only_native_assets(), true),
		(bridged_asset_hub_rococo, only_trust_backed_assets(), false),
		(bridged_asset_hub_rococo, only_sibling_foreign_assets(), false),
		(bridged_asset_hub_rococo, only_different_global_consensus_foreign_assets(), false),
	];

	// lets test filter with test data
	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_keys().collators())
		.with_session_keys(collator_session_keys().session_keys())
		.with_tracing()
		.build()
		.execute_with(|| {
			type XcmReserveTransferFilter =
				<Runtime as pallet_xcm::Config>::XcmReserveTransferFilter;

			fn do_test(data: Vec<(MultiLocation, Vec<MultiAsset>, bool)>) {
				for (dest, assets, expected_result) in data {
					assert_eq!(
						expected_result,
						XcmReserveTransferFilter::contains(&(dest, assets.clone())),
						"expected_result: {} for dest: {:?} and assets: {:?}",
						expected_result,
						dest,
						assets
					);
				}
			}

			// check for Rococo flavor
			do_test(test_data.clone());
			do_test(additional_test_data_for_rococo_flavor);

			// check for Wococo flavor
			asset_hub_wococo_tests::set_wococo_flavor();
			do_test(test_data);
			do_test(additional_test_data_for_wococo_flavor);
		})
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
