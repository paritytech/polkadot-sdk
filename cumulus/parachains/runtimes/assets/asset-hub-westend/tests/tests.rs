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

use alloy_core::{
	primitives::U256,
	sol_types::{sol_data, SolType},
};
use asset_hub_westend_runtime::{
	governance, xcm_config,
	xcm_config::{
		bridging, CheckingAccount, LocationToAccountId, StakingPot,
		TrustBackedAssetsPalletLocation, UniquesConvertedConcreteId, UniquesPalletLocation,
		WestendLocation, XcmConfig,
	},
	AllPalletsWithoutSystem, Assets, Balances, Block, ExistentialDeposit, ForeignAssets,
	ForeignAssetsInstance, MetadataDepositBase, MetadataDepositPerByte, ParachainSystem,
	PolkadotXcm, Revive, Runtime, RuntimeCall, RuntimeEvent, RuntimeOrigin, SessionKeys,
	ToRococoXcmRouterInstance, TrustBackedAssetsInstance, Uniques, WeightToFee, XcmpQueue,
};
pub use asset_hub_westend_runtime::{AssetConversion, AssetDeposit, CollatorSelection, System};
use asset_test_utils::{
	test_cases_over_bridge::TestBridgingConfig, CollatorSessionKey, CollatorSessionKeys,
	ExtBuilder, GovernanceOrigin, SlotDurations,
};
use codec::{Decode, Encode};
use frame_support::{
	assert_err, assert_noop, assert_ok, parameter_types,
	traits::{
		fungible::{self, Inspect, Mutate},
		fungibles::{
			self, Create, Inspect as FungiblesInspect, InspectEnumerable, Mutate as FungiblesMutate,
		},
		tokens::asset_ops::{
			common_strategies::{Bytes, Owner},
			Inspect as InspectUniqueAsset,
		},
		ContainsPair,
	},
	weights::{Weight, WeightToFee as WeightToFeeT},
};
use hex_literal::hex;
use pallet_revive::{
	test_utils::builder::{BareInstantiateBuilder, Contract},
	Code,
};
use pallet_revive_fixtures::compile_module;
use pallet_uniques::{asset_ops::Item, asset_strategies::Attribute};
use parachains_common::{AccountId, AssetIdForTrustBackedAssets, AuraId, Balance};
use sp_consensus_aura::SlotDuration;
use sp_core::crypto::Ss58Codec;
use sp_runtime::{traits::MaybeEquivalence, Either, MultiAddress};
use std::convert::Into;
use testnet_parachains_constants::westend::{consensus::*, currency::UNITS};
use xcm::{
	latest::{
		prelude::{Assets as XcmAssets, *},
		ROCOCO_GENESIS_HASH,
	},
	VersionedXcm,
};
use xcm_builder::{
	unique_instances::UniqueInstancesAdapter as NewNftAdapter, MatchInClassInstances, NoChecking,
	NonFungiblesAdapter as OldNftAdapter, WithLatestLocationConverter,
};
use xcm_executor::traits::{ConvertLocation, JustTry, TransactAsset, WeightTrader};
use xcm_runtime_apis::conversions::LocationToAccountHelper;

const ALICE: [u8; 32] = [1u8; 32];
const BOB: [u8; 32] = [2u8; 32];
const SOME_ASSET_ADMIN: [u8; 32] = [5u8; 32];

const ERC20_PVM: &[u8] =
	include_bytes!("../../../../../../substrate/frame/revive/fixtures/erc20/erc20.polkavm");

const FAKE_ERC20_PVM: &[u8] =
	include_bytes!("../../../../../../substrate/frame/revive/fixtures/erc20/fake_erc20.polkavm");

const EXPENSIVE_ERC20_PVM: &[u8] = include_bytes!(
	"../../../../../../substrate/frame/revive/fixtures/erc20/expensive_erc20.polkavm"
);

parameter_types! {
	pub Governance: GovernanceOrigin<RuntimeOrigin> = GovernanceOrigin::Origin(RuntimeOrigin::root());
}

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

/// Build a bare_instantiate call.
fn bare_instantiate(origin: &AccountId, code: Vec<u8>) -> BareInstantiateBuilder<Runtime> {
	let origin = RuntimeOrigin::signed(origin.clone());
	BareInstantiateBuilder::<Runtime>::bare_instantiate(origin, Code::Upload(code))
}

#[test]
fn test_buy_and_refund_weight_in_native() {
	ExtBuilder::<Runtime>::default()
		.with_tracing()
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
		.with_tracing()
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
		.with_tracing()
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
fn test_asset_xcm_take_first_trader_refund_not_possible_since_amount_less_than_ed() {
	ExtBuilder::<Runtime>::default()
		.with_tracing()
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
fn test_asset_xcm_take_first_trader_not_possible_for_non_sufficient_assets() {
	ExtBuilder::<Runtime>::default()
		.with_tracing()
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

fn test_nft_asset_transactor_works<T: TransactAsset>() {
	ExtBuilder::<Runtime>::default()
		.with_tracing()
		.with_collators(vec![AccountId::from(ALICE)])
		.with_session_keys(vec![(
			AccountId::from(ALICE),
			AccountId::from(ALICE),
			SessionKeys { aura: AuraId::from(sp_core::sr25519::Public::from_raw(ALICE)) },
		)])
		.build()
		.execute_with(|| {
			let collection_id = 42;
			let item_id = 101;

			let alice = AccountId::from(ALICE);
			let bob = AccountId::from(BOB);
			let ctx = XcmContext { origin: None, message_id: XcmHash::default(), topic: None };

			assert_ok!(Balances::mint_into(&alice, 2 * UNITS));

			assert_ok!(Uniques::create(
				RuntimeHelper::origin_of(alice.clone()),
				collection_id,
				MultiAddress::Id(alice.clone()),
			));

			assert_ok!(Uniques::mint(
				RuntimeHelper::origin_of(alice.clone()),
				collection_id,
				item_id,
				MultiAddress::Id(bob.clone()),
			));

			let attr_key = vec![0xA, 0xA, 0xB, 0xB];
			let attr_value = vec![0xC, 0x0, 0x0, 0x1, 0xF, 0x0, 0x0, 0xD];

			assert_ok!(Uniques::set_attribute(
				RuntimeHelper::origin_of(alice.clone()),
				collection_id,
				Some(item_id),
				attr_key.clone().try_into().unwrap(),
				attr_value.clone().try_into().unwrap(),
			));

			let collection_location = UniquesPalletLocation::get()
				.appended_with(GeneralIndex(collection_id.into()))
				.unwrap();
			let item_asset: Asset =
				(collection_location, AssetInstance::Index(item_id.into())).into();

			let alice_account_location: Location = alice.clone().into();
			let bob_account_location: Location = bob.clone().into();

			// Can't deposit the token that isn't withdrawn
			assert_err!(
				T::deposit_asset(&item_asset, &alice_account_location, Some(&ctx),),
				XcmError::FailedToTransactAsset("AlreadyExists")
			);

			// Alice isn't the owner, she can't withdraw the token
			assert_noop!(
				T::withdraw_asset(&item_asset, &alice_account_location, Some(&ctx),),
				XcmError::FailedToTransactAsset("NoPermission")
			);

			// Bob, the owner, can withdraw the token
			assert_ok!(T::withdraw_asset(&item_asset, &bob_account_location, Some(&ctx),));

			// The token is withdrawn
			assert_eq!(
				Item::<Uniques>::inspect(&(collection_id, item_id), Owner::default()),
				Err(pallet_uniques::Error::<Runtime>::UnknownItem.into()),
			);

			// But the attribute data is preserved as the pallet-uniques works that way.
			assert_eq!(
				Item::<Uniques>::inspect(
					&(collection_id, item_id),
					Bytes(Attribute(attr_key.as_slice()))
				),
				Ok(attr_value.clone()),
			);

			// Can't withdraw the already withdrawn token
			assert_err!(
				T::withdraw_asset(&item_asset, &bob_account_location, Some(&ctx),),
				XcmError::FailedToTransactAsset("UnknownCollection")
			);

			// Deposit the token to alice
			assert_ok!(T::deposit_asset(&item_asset, &alice_account_location, Some(&ctx),));

			// The token is deposited
			assert_eq!(
				Item::<Uniques>::inspect(&(collection_id, item_id), Owner::default()),
				Ok(alice.clone()),
			);

			// The attribute data is the same
			assert_eq!(
				Item::<Uniques>::inspect(
					&(collection_id, item_id),
					Bytes(Attribute(attr_key.as_slice()))
				),
				Ok(attr_value.clone()),
			);

			// Can't deposit the token twice
			assert_err!(
				T::deposit_asset(&item_asset, &alice_account_location, Some(&ctx),),
				XcmError::FailedToTransactAsset("AlreadyExists")
			);

			// Transfer the token directly
			assert_ok!(T::transfer_asset(
				&item_asset,
				&alice_account_location,
				&bob_account_location,
				&ctx,
			));

			// The token's owner has changed
			assert_eq!(
				Item::<Uniques>::inspect(&(collection_id, item_id), Owner::default()),
				Ok(bob.clone()),
			);

			// The attribute data is the same
			assert_eq!(
				Item::<Uniques>::inspect(
					&(collection_id, item_id),
					Bytes(Attribute(attr_key.as_slice()))
				),
				Ok(attr_value.clone()),
			);
		});
}

#[test]
fn test_new_nft_config_works_as_the_old_one() {
	type OldNftTransactor = OldNftAdapter<
		Uniques,
		UniquesConvertedConcreteId,
		LocationToAccountId,
		AccountId,
		NoChecking,
		CheckingAccount,
	>;

	type NewNftTransactor = NewNftAdapter<
		AccountId,
		LocationToAccountId,
		MatchInClassInstances<UniquesConvertedConcreteId>,
		Item<Uniques>,
	>;

	test_nft_asset_transactor_works::<OldNftTransactor>();
	test_nft_asset_transactor_works::<NewNftTransactor>();
}

#[test]
fn test_assets_balances_api_works() {
	use assets_common::runtime_api::runtime_decl_for_fungibles_api::FungiblesApi;

	ExtBuilder::<Runtime>::default()
		.with_tracing()
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

#[test]
fn authorized_aliases_work() {
	ExtBuilder::<Runtime>::default()
		.with_tracing()
		.with_collators(vec![AccountId::from(ALICE)])
		.with_session_keys(vec![(
			AccountId::from(ALICE),
			AccountId::from(ALICE),
			SessionKeys { aura: AuraId::from(sp_core::sr25519::Public::from_raw(ALICE)) },
		)])
		.build()
		.execute_with(|| {
			let alice: AccountId = ALICE.into();
			let local_alice = Location::new(0, AccountId32 { network: None, id: ALICE });
			let alice_on_sibling_para =
				Location::new(1, [Parachain(42), AccountId32 { network: None, id: ALICE }]);
			let alice_on_relay = Location::new(1, AccountId32 { network: None, id: ALICE });
			let bob_on_relay = Location::new(1, AccountId32 { network: None, id: [42_u8; 32] });

			assert_ok!(Balances::mint_into(&alice, 2 * UNITS));

			// neither `alice_on_sibling_para`, `alice_on_relay`, `bob_on_relay` are allowed to
			// alias into `local_alice`
			for aliaser in [&alice_on_sibling_para, &alice_on_relay, &bob_on_relay] {
				assert!(!<XcmConfig as xcm_executor::Config>::Aliasers::contains(
					aliaser,
					&local_alice
				));
			}

			// Alice explicitly authorizes `alice_on_sibling_para` to alias her local account
			assert_ok!(PolkadotXcm::add_authorized_alias(
				RuntimeHelper::origin_of(alice.clone()),
				Box::new(alice_on_sibling_para.clone().into()),
				None
			));

			// `alice_on_sibling_para` now explicitly allowed to alias into `local_alice`
			assert!(<XcmConfig as xcm_executor::Config>::Aliasers::contains(
				&alice_on_sibling_para,
				&local_alice
			));
			// as expected, `alice_on_relay` and `bob_on_relay` still can't alias into `local_alice`
			for aliaser in [&alice_on_relay, &bob_on_relay] {
				assert!(!<XcmConfig as xcm_executor::Config>::Aliasers::contains(
					aliaser,
					&local_alice
				));
			}

			// Alice explicitly authorizes `alice_on_relay` to alias her local account
			assert_ok!(PolkadotXcm::add_authorized_alias(
				RuntimeHelper::origin_of(alice.clone()),
				Box::new(alice_on_relay.clone().into()),
				None
			));
			// Now both `alice_on_relay` and `alice_on_sibling_para` can alias into her local
			// account
			for aliaser in [&alice_on_relay, &alice_on_sibling_para] {
				assert!(<XcmConfig as xcm_executor::Config>::Aliasers::contains(
					aliaser,
					&local_alice
				));
			}

			// Alice removes authorization for `alice_on_relay` to alias her local account
			assert_ok!(PolkadotXcm::remove_authorized_alias(
				RuntimeHelper::origin_of(alice.clone()),
				Box::new(alice_on_relay.clone().into())
			));

			// `alice_on_relay` no longer allowed to alias into `local_alice`
			assert!(!<XcmConfig as xcm_executor::Config>::Aliasers::contains(
				&alice_on_relay,
				&local_alice
			));

			// `alice_on_sibling_para` still allowed to alias into `local_alice`
			assert!(<XcmConfig as xcm_executor::Config>::Aliasers::contains(
				&alice_on_sibling_para,
				&local_alice
			));
		})
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
	LocationToAccountId,
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
	LocationToAccountId,
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
		Some(governance::TreasuryAccount::get()),
	)
}

#[test]
fn receive_reserve_asset_deposited_roc_from_asset_hub_rococo_fees_paid_by_pool_swap_works() {
	const BLOCK_AUTHOR_ACCOUNT: [u8; 32] = [13; 32];
	let block_author_account = AccountId::from(BLOCK_AUTHOR_ACCOUNT);
	let staking_pot = StakingPot::get();

	let foreign_asset_id_location = xcm::v5::Location::new(
		2,
		[xcm::v5::Junction::GlobalConsensus(xcm::v5::NetworkId::ByGenesis(ROCOCO_GENESIS_HASH))],
	);
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
				asset_test_utils::test_cases::setup_pool_for_paying_fees_with_foreign_assets::<Runtime, RuntimeOrigin>(ExistentialDeposit::get(), foreign_asset_create_params);
				// staking pot account for collecting local native fees from `BuyExecution`
				let _ = Balances::force_set_balance(RuntimeOrigin::root(), StakingPot::get().into(), ExistentialDeposit::get());
				// prepare bridge configuration
				bridging_to_asset_hub_rococo()
			},
			(
				[PalletInstance(bp_bridge_hub_westend::WITH_BRIDGE_WESTEND_TO_ROCOCO_MESSAGES_PALLET_INDEX)].into(),
				GlobalConsensus(ByGenesis(ROCOCO_GENESIS_HASH)),
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

	let foreign_asset_id_location = xcm::v5::Location::new(
		2,
		[xcm::v5::Junction::GlobalConsensus(xcm::v5::NetworkId::ByGenesis(ROCOCO_GENESIS_HASH))],
	);
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
			asset_test_utils::test_cases::setup_pool_for_paying_fees_with_foreign_assets::<Runtime, RuntimeOrigin>(ExistentialDeposit::get(), foreign_asset_create_params);
			bridging_to_asset_hub_rococo()
		},
		(
			[PalletInstance(bp_bridge_hub_westend::WITH_BRIDGE_WESTEND_TO_ROCOCO_MESSAGES_PALLET_INDEX)].into(),
			GlobalConsensus(ByGenesis(ROCOCO_GENESIS_HASH)),
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
			// check staking pot has at least ED
			assert!(Balances::free_balance(&staking_pot) >= ExistentialDeposit::get());
			// check now foreign asset for staking pot
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
fn report_bridge_status_from_xcm_bridge_router_for_rococo_works() {
	asset_test_utils::test_cases_over_bridge::report_bridge_status_from_xcm_bridge_router_works::<
		Runtime,
		AllPalletsWithoutSystem,
		XcmConfig,
		LocationToAccountId,
		ToRococoXcmRouterInstance,
	>(
		collator_session_keys(),
		bridging_to_asset_hub_rococo,
		|| bp_asset_hub_westend::build_congestion_message(Default::default(), true).into(),
		|| bp_asset_hub_westend::build_congestion_message(Default::default(), false).into(),
	)
}

#[test]
fn test_report_bridge_status_call_compatibility() {
	// if this test fails, make sure `bp_asset_hub_rococo` has valid encoding
	assert_eq!(
		RuntimeCall::ToRococoXcmRouter(pallet_xcm_bridge_hub_router::Call::report_bridge_status {
			bridge_id: Default::default(),
			is_congested: true,
		})
		.encode(),
		bp_asset_hub_westend::Call::ToRococoXcmRouter(
			bp_asset_hub_westend::XcmBridgeHubRouterCall::report_bridge_status {
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
	let max_weight = bp_asset_hub_westend::XcmBridgeHubRouterTransactCallMaxWeight::get();
	assert!(
		actual.all_lte(max_weight),
		"max_weight: {:?} should be adjusted to actual {:?}",
		max_weight,
		actual
	);
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
		Governance::get(),
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
		Governance::get(),
		|| {
			tracing::error!(
				target: "bridges::estimate",
				actual_value=%bridging::XcmBridgeHubRouterBaseFee::get(),
				runtime=%<Runtime as frame_system::Config>::Version::get(),
				"`bridging::XcmBridgeHubRouterBaseFee`"
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
		// ExternalConsensusLocationsConverterFor
		TestCase {
			description: "Describe Ethereum Location",
			location: Location::new(2, [GlobalConsensus(Ethereum { chain_id: 11155111 })]),
			expected_account_id_str: "5GjRnmh5o3usSYzVmsxBWzHEpvJyHK4tKNPhjpUR3ASrruBy",
		},
		TestCase {
			description: "Describe Ethereum AccountKey",
			location: Location::new(
				2,
				[
					GlobalConsensus(Ethereum { chain_id: 11155111 }),
					AccountKey20 {
						network: None,
						key: hex!("87d1f7fdfEe7f651FaBc8bFCB6E086C278b77A7d"),
					},
				],
			),
			expected_account_id_str: "5HV4j4AsqT349oLRZmTjhGKDofPBWmWaPUfWGaRkuvzkjW9i",
		},
		TestCase {
			description: "Describe Rococo Location",
			location: Location::new(2, [GlobalConsensus(ByGenesis(ROCOCO_GENESIS_HASH))]),
			expected_account_id_str: "5FfpYGrFybJXFsQk7dabr1vEbQ5ycBBu85vrDjPJsF3q4A8P",
		},
		TestCase {
			description: "Describe Rococo AccountID",
			location: Location::new(
				2,
				[
					GlobalConsensus(ByGenesis(ROCOCO_GENESIS_HASH)),
					AccountId32 { network: None, id: AccountId::from(ALICE).into() },
				],
			),
			expected_account_id_str: "5CXVYinTeQKQGWAP9RqaPhitk7ybrqBZf66kCJmtAjV4Xwbg",
		},
		TestCase {
			description: "Describe Rococo AccountKey",
			location: Location::new(
				2,
				[
					GlobalConsensus(ByGenesis(ROCOCO_GENESIS_HASH)),
					AccountKey20 { network: None, key: [0u8; 20] },
				],
			),
			expected_account_id_str: "5GbRhbJWb2hZY7TCeNvTqZXaP3x3UY5xt4ccxpV1ZtJS1gFL",
		},
		TestCase {
			description: "Describe Rococo Treasury Plurality",
			location: Location::new(
				2,
				[
					GlobalConsensus(ByGenesis(ROCOCO_GENESIS_HASH)),
					Plurality { id: BodyId::Treasury, part: BodyPart::Voice },
				],
			),
			expected_account_id_str: "5EGi9NgJNGoMawY8ubnCDLmbdEW6nt2W2U2G3j9E3jXmspT7",
		},
		TestCase {
			description: "Describe Rococo Parachain Location",
			location: Location::new(
				2,
				[GlobalConsensus(ByGenesis(ROCOCO_GENESIS_HASH)), Parachain(1000)],
			),
			expected_account_id_str: "5CQeLKM7XC1xNBiQLp26Wa948cudjYRD5VzvaTG3BjnmUvLL",
		},
		TestCase {
			description: "Describe Rococo Parachain AccountID",
			location: Location::new(
				2,
				[
					GlobalConsensus(ByGenesis(ROCOCO_GENESIS_HASH)),
					Parachain(1000),
					AccountId32 { network: None, id: AccountId::from(ALICE).into() },
				],
			),
			expected_account_id_str: "5H8HsK17dV7i7J8fZBNd438rvwd7rHviZxJqyZpLEGJn6vb6",
		},
		TestCase {
			description: "Describe Rococo Parachain AccountKey",
			location: Location::new(
				2,
				[
					GlobalConsensus(ByGenesis(ROCOCO_GENESIS_HASH)),
					Parachain(1000),
					AccountKey20 { network: None, key: [0u8; 20] },
				],
			),
			expected_account_id_str: "5G121Rtddxn6zwMD2rZZGXxFHZ2xAgzFUgM9ki4A8wMGo4e2",
		},
		TestCase {
			description: "Describe Rococo Parachain Treasury Plurality",
			location: Location::new(
				2,
				[
					GlobalConsensus(ByGenesis(ROCOCO_GENESIS_HASH)),
					Parachain(1000),
					Plurality { id: BodyId::Treasury, part: BodyPart::Voice },
				],
			),
			expected_account_id_str: "5FNk7za2pQ71NHnN1jA63hJxJwdQywiVGnK6RL3nYjCdkWDF",
		},
		TestCase {
			description: "Describe Rococo USDT Location",
			location: Location::new(
				2,
				[
					GlobalConsensus(ByGenesis(ROCOCO_GENESIS_HASH)),
					Parachain(1000),
					PalletInstance(50),
					GeneralIndex(1984),
				],
			),
			expected_account_id_str: "5HNfT779KHeAL7PaVBTQDVxrT6dfJZJoQMTScxLSahBc9kxF",
		},
	];

	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_keys().collators())
		.with_session_keys(collator_session_keys().session_keys())
		.with_para_id(1000.into())
		.build()
		.execute_with(|| {
			for tc in test_cases {
				let expected = AccountId::from_string(tc.expected_account_id_str)
					.expect("Invalid AccountId string");
				let got =
					LocationToAccountHelper::<AccountId, LocationToAccountId>::convert_location(
						tc.location.into(),
					)
					.unwrap();

				assert_eq!(got, expected, "{}", tc.description);
			}
		});
}

#[test]
fn xcm_payment_api_works() {
	parachains_runtimes_test_utils::test_cases::xcm_payment_api_with_native_token_works::<
		Runtime,
		RuntimeCall,
		RuntimeOrigin,
		Block,
		WeightToFee,
	>();
	asset_test_utils::test_cases::xcm_payment_api_with_pools_works::<
		Runtime,
		RuntimeCall,
		RuntimeOrigin,
		Block,
		WeightToFee,
	>();

	asset_test_utils::test_cases::xcm_payment_api_foreign_asset_pool_works::<
		Runtime,
		RuntimeCall,
		RuntimeOrigin,
		LocationToAccountId,
		Block,
		WeightToFee,
	>(ExistentialDeposit::get(), ROCOCO_GENESIS_HASH);
}

#[test]
fn governance_authorize_upgrade_works() {
	use westend_runtime_constants::system_parachain::COLLECTIVES_ID;

	// no - random para
	assert_err!(
		parachains_runtimes_test_utils::test_cases::can_governance_authorize_upgrade::<
			Runtime,
			RuntimeOrigin,
		>(GovernanceOrigin::Location(Location::new(1, Parachain(12334)))),
		Either::Right(InstructionError { index: 0, error: XcmError::Barrier })
	);
	// ok - AssetHub (itself)
	assert_ok!(parachains_runtimes_test_utils::test_cases::can_governance_authorize_upgrade::<
		Runtime,
		RuntimeOrigin,
	>(GovernanceOrigin::Origin(RuntimeOrigin::root())));
	// no - Collectives
	assert_err!(
		parachains_runtimes_test_utils::test_cases::can_governance_authorize_upgrade::<
			Runtime,
			RuntimeOrigin,
		>(GovernanceOrigin::Location(Location::new(1, Parachain(COLLECTIVES_ID)))),
		Either::Right(InstructionError { index: 1, error: XcmError::BadOrigin })
	);
	// no - Collectives Voice of Fellows plurality
	assert_err!(
		parachains_runtimes_test_utils::test_cases::can_governance_authorize_upgrade::<
			Runtime,
			RuntimeOrigin,
		>(GovernanceOrigin::LocationAndDescendOrigin(
			Location::new(1, Parachain(COLLECTIVES_ID)),
			Plurality { id: BodyId::Technical, part: BodyPart::Voice }.into()
		)),
		Either::Right(InstructionError { index: 2, error: XcmError::BadOrigin })
	);

	// ok - relaychain
	assert_ok!(parachains_runtimes_test_utils::test_cases::can_governance_authorize_upgrade::<
		Runtime,
		RuntimeOrigin,
	>(GovernanceOrigin::Location(Location::parent())));
}

#[test]
fn weight_of_message_increases_when_dealing_with_erc20s() {
	use xcm::VersionedXcm;
	use xcm_runtime_apis::fees::runtime_decl_for_xcm_payment_api::XcmPaymentApiV1;
	let message = Xcm::<()>::builder_unsafe().withdraw_asset((Parent, 100u128)).build();
	let versioned = VersionedXcm::<()>::V5(message);
	let regular_asset_weight = Runtime::query_xcm_weight(versioned).unwrap();

	let message = Xcm::<()>::builder_unsafe()
		.withdraw_asset((AccountKey20 { network: None, key: [1u8; 20] }, 100u128))
		.build();
	let versioned = VersionedXcm::<()>::V5(message);
	let weight = Runtime::query_xcm_weight(versioned).unwrap();
	assert!(
		weight.ref_time() > regular_asset_weight.ref_time()
			// The proof size really blows up.
			&& weight.proof_size() > 10 * regular_asset_weight.proof_size()
	);
	assert_eq!(weight, crate::xcm_config::ERC20TransferGasLimit::get());
}

#[test]
fn withdraw_and_deposit_erc20s() {
	let sender: AccountId = ALICE.into();
	let beneficiary: AccountId = BOB.into();
	let revive_account = pallet_revive::Pallet::<Runtime>::account_id();
	let checking_account =
		asset_hub_westend_runtime::xcm_config::ERC20TransfersCheckingAccount::get();
	let initial_wnd_amount = 100_000_000_000_000_000u128;
	sp_tracing::init_for_tests();

	ExtBuilder::<Runtime>::default().build().execute_with(|| {
		// Bring the revive account to life.
		assert_ok!(Balances::mint_into(&revive_account, initial_wnd_amount));
		// We need to give enough funds for every account involved so they
		// can call `Revive::map_account`.
		assert_ok!(Balances::mint_into(&sender, initial_wnd_amount));
		assert_ok!(Balances::mint_into(&beneficiary, initial_wnd_amount));
		assert_ok!(Balances::mint_into(&checking_account, initial_wnd_amount));

		// We need to map all accounts.
		assert_ok!(Revive::map_account(RuntimeOrigin::signed(checking_account.clone())));
		assert_ok!(Revive::map_account(RuntimeOrigin::signed(sender.clone())));
		assert_ok!(Revive::map_account(RuntimeOrigin::signed(beneficiary.clone())));

		let code = ERC20_PVM.to_vec();

		let initial_amount_u256 = U256::from(1_000_000_000_000u128);
		let constructor_data = sol_data::Uint::<256>::abi_encode(&initial_amount_u256);
		let Contract { addr: erc20_address, .. } = bare_instantiate(&sender, code)
			.gas_limit(Weight::from_parts(500_000_000_000, 10 * 1024 * 1024))
			.storage_deposit_limit(Balance::MAX)
			.data(constructor_data)
			.build_and_unwrap_contract();

		let sender_balance_before = <Balances as fungible::Inspect<_>>::balance(&sender);

		let erc20_transfer_amount = 100u128;
		let wnd_amount_for_fees = 10_000_000_000_000u128;
		// Actual XCM to execute locally.
		let message = Xcm::<RuntimeCall>::builder()
			.withdraw_asset((Parent, wnd_amount_for_fees))
			.pay_fees((Parent, wnd_amount_for_fees))
			.withdraw_asset((
				AccountKey20 { key: erc20_address.into(), network: None },
				erc20_transfer_amount,
			))
			.deposit_asset(AllCounted(1), beneficiary.clone())
			.refund_surplus()
			.deposit_asset(AllCounted(1), sender.clone())
			.build();
		assert_ok!(PolkadotXcm::execute(
			RuntimeOrigin::signed(sender.clone()),
			Box::new(VersionedXcm::V5(message)),
			Weight::from_parts(600_000_000_000, 15 * 1024 * 1024),
		));

		// Revive is not taking any fees.
		let sender_balance_after = <Balances as fungible::Inspect<_>>::balance(&sender);
		// Balance after is larger than the difference between balance before and transferred
		// amount because of the refund.
		assert!(sender_balance_after > sender_balance_before - wnd_amount_for_fees);

		// Beneficiary receives the ERC20.
		let beneficiary_amount =
			<Revive as fungibles::Inspect<_>>::balance(erc20_address, &beneficiary);
		assert_eq!(beneficiary_amount, erc20_transfer_amount);
	});
}

#[test]
fn non_existent_erc20_will_error() {
	let sender: AccountId = ALICE.into();
	let beneficiary: AccountId = BOB.into();
	let revive_account = pallet_revive::Pallet::<Runtime>::account_id();
	let checking_account =
		asset_hub_westend_runtime::xcm_config::ERC20TransfersCheckingAccount::get();
	let initial_wnd_amount = 10_000_000_000_000u128;
	// We try to withdraw an ERC20 token but the address doesn't exist.
	let non_existent_contract_address = [1u8; 20];

	ExtBuilder::<Runtime>::default().build().execute_with(|| {
		// Bring the revive account to life.
		assert_ok!(Balances::mint_into(&revive_account, initial_wnd_amount));
		// We need to give enough funds for every account involved so they
		// can call `Revive::map_account`.
		assert_ok!(Balances::mint_into(&sender, initial_wnd_amount));
		assert_ok!(Balances::mint_into(&beneficiary, initial_wnd_amount));
		assert_ok!(Balances::mint_into(&checking_account, initial_wnd_amount));

		// We need to map all accounts.
		assert_ok!(Revive::map_account(RuntimeOrigin::signed(checking_account.clone())));
		assert_ok!(Revive::map_account(RuntimeOrigin::signed(sender.clone())));
		assert_ok!(Revive::map_account(RuntimeOrigin::signed(beneficiary.clone())));

		let wnd_amount_for_fees = 1_000_000_000_000u128;
		let erc20_transfer_amount = 100u128;
		let message = Xcm::<RuntimeCall>::builder()
			.withdraw_asset((Parent, wnd_amount_for_fees))
			.pay_fees((Parent, wnd_amount_for_fees))
			.withdraw_asset((
				AccountKey20 { key: non_existent_contract_address, network: None },
				erc20_transfer_amount,
			))
			.deposit_asset(AllCounted(1), beneficiary.clone())
			.build();
		// Execution fails but doesn't panic.
		assert!(PolkadotXcm::execute(
			RuntimeOrigin::signed(sender.clone()),
			Box::new(VersionedXcm::V5(message)),
			Weight::from_parts(2_500_000_000, 120_000),
		)
		.is_err());
	});
}

#[test]
fn smart_contract_not_erc20_will_error() {
	let sender: AccountId = ALICE.into();
	let beneficiary: AccountId = BOB.into();
	let revive_account = pallet_revive::Pallet::<Runtime>::account_id();
	let checking_account =
		asset_hub_westend_runtime::xcm_config::ERC20TransfersCheckingAccount::get();
	let initial_wnd_amount = 10_000_000_000_000u128;

	ExtBuilder::<Runtime>::default().build().execute_with(|| {
		// Bring the revive account to life.
		assert_ok!(Balances::mint_into(&revive_account, initial_wnd_amount));

		// We need to give enough funds for every account involved so they
		// can call `Revive::map_account`.
		assert_ok!(Balances::mint_into(&sender, initial_wnd_amount));
		assert_ok!(Balances::mint_into(&beneficiary, initial_wnd_amount));
		assert_ok!(Balances::mint_into(&checking_account, initial_wnd_amount));

		// We need to map all accounts.
		assert_ok!(Revive::map_account(RuntimeOrigin::signed(checking_account.clone())));
		assert_ok!(Revive::map_account(RuntimeOrigin::signed(sender.clone())));
		assert_ok!(Revive::map_account(RuntimeOrigin::signed(beneficiary.clone())));

		let (code, _) = compile_module("dummy").unwrap();

		let Contract { addr: non_erc20_address, .. } = bare_instantiate(&sender, code)
			.gas_limit(Weight::from_parts(500_000_000_000, 10 * 1024 * 1024))
			.storage_deposit_limit(Balance::MAX)
			.build_and_unwrap_contract();

		let wnd_amount_for_fees = 1_000_000_000_000u128;
		let erc20_transfer_amount = 100u128;
		let message = Xcm::<RuntimeCall>::builder()
			.withdraw_asset((Parent, wnd_amount_for_fees))
			.pay_fees((Parent, wnd_amount_for_fees))
			.withdraw_asset((
				AccountKey20 { key: non_erc20_address.into(), network: None },
				erc20_transfer_amount,
			))
			.deposit_asset(AllCounted(1), beneficiary.clone())
			.build();
		// Execution fails but doesn't panic.
		assert!(PolkadotXcm::execute(
			RuntimeOrigin::signed(sender.clone()),
			Box::new(VersionedXcm::V5(message)),
			Weight::from_parts(2_500_000_000, 120_000),
		)
		.is_err());
	});
}

// Here the contract returns a number but because it can be cast to true
// it still succeeds.
#[test]
fn smart_contract_does_not_return_bool_fails() {
	let sender: AccountId = ALICE.into();
	let beneficiary: AccountId = BOB.into();
	let revive_account = pallet_revive::Pallet::<Runtime>::account_id();
	let checking_account =
		asset_hub_westend_runtime::xcm_config::ERC20TransfersCheckingAccount::get();
	let initial_wnd_amount = 10_000_000_000_000u128;

	ExtBuilder::<Runtime>::default().build().execute_with(|| {
		// Bring the revive account to life.
		assert_ok!(Balances::mint_into(&revive_account, initial_wnd_amount));

		// We need to give enough funds for every account involved so they
		// can call `Revive::map_account`.
		assert_ok!(Balances::mint_into(&sender, initial_wnd_amount));
		assert_ok!(Balances::mint_into(&beneficiary, initial_wnd_amount));
		assert_ok!(Balances::mint_into(&checking_account, initial_wnd_amount));

		// We need to map all accounts.
		assert_ok!(Revive::map_account(RuntimeOrigin::signed(checking_account.clone())));
		assert_ok!(Revive::map_account(RuntimeOrigin::signed(sender.clone())));
		assert_ok!(Revive::map_account(RuntimeOrigin::signed(beneficiary.clone())));

		// This contract implements the ERC20 interface for `transfer` except it returns a uint256.
		let code = FAKE_ERC20_PVM.to_vec();

		let initial_amount_u256 = U256::from(1_000_000_000_000u128);
		let constructor_data = sol_data::Uint::<256>::abi_encode(&initial_amount_u256);

		let Contract { addr: non_erc20_address, .. } = bare_instantiate(&sender, code)
			.gas_limit(Weight::from_parts(500_000_000_000, 10 * 1024 * 1024))
			.storage_deposit_limit(Balance::MAX)
			.data(constructor_data)
			.build_and_unwrap_contract();

		let wnd_amount_for_fees = 1_000_000_000_000u128;
		let erc20_transfer_amount = 100u128;
		let message = Xcm::<RuntimeCall>::builder()
			.withdraw_asset((Parent, wnd_amount_for_fees))
			.pay_fees((Parent, wnd_amount_for_fees))
			.withdraw_asset((
				AccountKey20 { key: non_erc20_address.into(), network: None },
				erc20_transfer_amount,
			))
			.deposit_asset(AllCounted(1), beneficiary.clone())
			.build();
		// Execution fails but doesn't panic.
		assert!(PolkadotXcm::execute(
			RuntimeOrigin::signed(sender.clone()),
			Box::new(VersionedXcm::V5(message)),
			Weight::from_parts(2_500_000_000, 220_000),
		)
		.is_err());
	});
}

#[test]
fn expensive_erc20_runs_out_of_gas() {
	let sender: AccountId = ALICE.into();
	let beneficiary: AccountId = BOB.into();
	let revive_account = pallet_revive::Pallet::<Runtime>::account_id();
	let checking_account =
		asset_hub_westend_runtime::xcm_config::ERC20TransfersCheckingAccount::get();
	let initial_wnd_amount = 10_000_000_000_000u128;

	ExtBuilder::<Runtime>::default().build().execute_with(|| {
		// Bring the revive account to life.
		assert_ok!(Balances::mint_into(&revive_account, initial_wnd_amount));

		// We need to give enough funds for every account involved so they
		// can call `Revive::map_account`.
		assert_ok!(Balances::mint_into(&sender, initial_wnd_amount));
		assert_ok!(Balances::mint_into(&beneficiary, initial_wnd_amount));
		assert_ok!(Balances::mint_into(&checking_account, initial_wnd_amount));

		// We need to map all accounts.
		assert_ok!(Revive::map_account(RuntimeOrigin::signed(checking_account.clone())));
		assert_ok!(Revive::map_account(RuntimeOrigin::signed(sender.clone())));
		assert_ok!(Revive::map_account(RuntimeOrigin::signed(beneficiary.clone())));

		// This contract does a lot more storage writes in `transfer`.
		let code = EXPENSIVE_ERC20_PVM.to_vec();

		let initial_amount_u256 = U256::from(1_000_000_000_000u128);
		let constructor_data = sol_data::Uint::<256>::abi_encode(&initial_amount_u256);
		let Contract { addr: non_erc20_address, .. } = bare_instantiate(&sender, code)
			.gas_limit(Weight::from_parts(500_000_000_000, 10 * 1024 * 1024))
			.storage_deposit_limit(Balance::MAX)
			.data(constructor_data)
			.build_and_unwrap_contract();

		let wnd_amount_for_fees = 1_000_000_000_000u128;
		let erc20_transfer_amount = 100u128;
		let message = Xcm::<RuntimeCall>::builder()
			.withdraw_asset((Parent, wnd_amount_for_fees))
			.pay_fees((Parent, wnd_amount_for_fees))
			.withdraw_asset((
				AccountKey20 { key: non_erc20_address.into(), network: None },
				erc20_transfer_amount,
			))
			.deposit_asset(AllCounted(1), beneficiary.clone())
			.build();
		// Execution fails but doesn't panic.
		assert!(PolkadotXcm::execute(
			RuntimeOrigin::signed(sender.clone()),
			Box::new(VersionedXcm::V5(message)),
			Weight::from_parts(2_500_000_000, 120_000),
		)
		.is_err());
	});
}
