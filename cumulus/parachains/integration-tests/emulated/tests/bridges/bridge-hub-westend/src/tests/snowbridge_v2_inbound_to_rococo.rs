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
use crate::{
	imports::*,
	tests::{
		assert_bridge_hub_rococo_message_received, assert_bridge_hub_westend_message_accepted,
		asset_hub_rococo_location,
		snowbridge_common::{
			asset_hub_westend_global_location, bridged_roc_at_ah_westend,
			create_foreign_on_ah_westend, erc20_token_location, eth_location,
			register_foreign_asset, register_roc_on_bh, set_up_eth_and_dot_pool,
			set_up_eth_and_dot_pool_on_rococo, set_up_pool_with_wnd_on_ah_westend,
			snowbridge_sovereign, TOKEN_AMOUNT,
		},
	},
};
use asset_hub_westend_runtime::ForeignAssets;
use bridge_hub_westend_runtime::{
	bridge_common_config::BridgeReward, bridge_to_ethereum_config::EthereumGatewayAddress,
	EthereumInboundQueueV2,
};
use codec::Encode;
use hex_literal::hex;
use snowbridge_core::TokenIdOf;
use snowbridge_inbound_queue_primitives::v2::{
	EthereumAsset::{ForeignTokenERC20, NativeTokenERC20},
	Message, Payload,
};
use sp_core::{H160, H256};
use xcm::opaque::latest::AssetTransferFilter::{ReserveDeposit, ReserveWithdraw};
use xcm_executor::traits::ConvertLocation;

/// Calculates the XCM prologue fee for sending an XCM to AH.
const INITIAL_FUND: u128 = 500_000_000_000_000;

/// An ERC-20 token to be registered and sent.
const TOKEN_ID: [u8; 20] = hex!("c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2");

#[test]
fn send_token_to_rococo_v2() {
	let relayer_account = BridgeHubWestendSender::get();
	let relayer_reward = 1_500_000_000_000u128;

	let token: H160 = TOKEN_ID.into();
	let token_location = erc20_token_location(token);

	let beneficiary_acc_id: H256 = H256::random();
	let beneficiary_acc_bytes: [u8; 32] = beneficiary_acc_id.into();
	let beneficiary =
		Location::new(0, AccountId32 { network: None, id: beneficiary_acc_id.into() });

	let claimer_acc_id = H256::random();
	let claimer = AccountId32 { network: None, id: claimer_acc_id.into() };
	let claimer_bytes = claimer.encode();

	// set XCM versions
	BridgeHubWestend::force_xcm_version(asset_hub_westend_global_location(), XCM_VERSION);
	BridgeHubWestend::force_xcm_version(asset_hub_rococo_location(), XCM_VERSION);
	AssetHubWestend::force_xcm_version(asset_hub_rococo_location(), XCM_VERSION);

	// To pay fees on Rococo.
	let eth_fee_rococo_ah: xcm::prelude::Asset = (eth_location(), 3_000_000_000_000u128).into();

	// To satisfy ED
	AssetHubRococo::fund_accounts(vec![(
		sp_runtime::AccountId32::from(beneficiary_acc_bytes),
		3_000_000_000_000,
	)]);
	BridgeHubWestend::fund_para_sovereign(AssetHubWestend::para_id(), INITIAL_FUND);

	// Register the token on AH Westend and Rococo
	let snowbridge_sovereign = snowbridge_sovereign();
	AssetHubRococo::execute_with(|| {
		type RuntimeOrigin = <AssetHubRococo as Chain>::RuntimeOrigin;

		assert_ok!(<AssetHubRococo as AssetHubRococoPallet>::ForeignAssets::force_create(
			RuntimeOrigin::root(),
			token_location.clone().try_into().unwrap(),
			snowbridge_sovereign.clone().into(),
			true,
			1000,
		));

		assert!(<AssetHubRococo as AssetHubRococoPallet>::ForeignAssets::asset_exists(
			token_location.clone().try_into().unwrap(),
		));
	});
	register_foreign_asset(token_location.clone());

	set_up_eth_and_dot_pool();
	set_up_eth_and_dot_pool_on_rococo();

	let token_transfer_value = 2_000_000_000_000u128;

	let assets = vec![
		// the token being transferred
		NativeTokenERC20 { token_id: token.into(), value: token_transfer_value },
	];

	let token_asset_ah: xcm::prelude::Asset = (token_location.clone(), token_transfer_value).into();
	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		let instructions = vec![
			// Send message to Rococo AH
			InitiateTransfer {
				// Rococo
				destination: Location::new(
					2,
					[
						GlobalConsensus(ByGenesis(xcm::latest::ROCOCO_GENESIS_HASH)),
						Parachain(1000u32),
					],
				),
				remote_fees: Some(ReserveDeposit(Definite(vec![eth_fee_rococo_ah.clone()].into()))),
				preserve_origin: false,
				assets: BoundedVec::truncate_from(vec![ReserveDeposit(Definite(
					vec![token_asset_ah.clone()].into(),
				))]),
				remote_xcm: vec![
					// Refund unspent fees
					RefundSurplus,
					// Deposit assets to beneficiary.
					DepositAsset { assets: Wild(AllCounted(3)), beneficiary: beneficiary.clone() },
					SetTopic(H256::random().into()),
				]
				.into(),
			},
			RefundSurplus,
			DepositAsset {
				assets: Wild(AllOf { id: AssetId(eth_location()), fun: WildFungibility::Fungible }),
				beneficiary,
			},
		];
		let xcm: Xcm<()> = instructions.into();
		let versioned_message_xcm = VersionedXcm::V5(xcm);
		let origin = EthereumGatewayAddress::get();

		let message = Message {
			gateway: origin,
			nonce: 1,
			origin,
			assets,
			payload: Payload::Raw(versioned_message_xcm.encode()),
			claimer: Some(claimer_bytes),
			value: 3_500_000_000_000u128,
			execution_fee: 1_500_000_000_000u128,
			relayer_fee: relayer_reward,
		};

		EthereumInboundQueueV2::process_message(relayer_account.clone(), message).unwrap();

		assert_expected_events!(
			BridgeHubWestend,
			vec![
				RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) => {},
				// Check that the relayer reward was registered.
				RuntimeEvent::BridgeRelayers(pallet_bridge_relayers::Event::RewardRegistered { relayer, reward_kind, reward_balance }) => {
					relayer: *relayer == relayer_account,
					reward_kind: *reward_kind == BridgeReward::Snowbridge,
					reward_balance: *reward_balance == relayer_reward,
				},
			]
		);
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		// Check that the assets were issued on AssetHub
		assert_expected_events!(
			AssetHubWestend,
			vec![
				// Message processed successfully
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
				RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) => {},
			]
		);

		let events = AssetHubWestend::events();
		// Check that no assets were trapped
		assert!(
			!events.iter().any(|event| matches!(
				event,
				RuntimeEvent::PolkadotXcm(pallet_xcm::Event::AssetsTrapped { .. })
			)),
			"Assets were trapped, should not happen."
		);
	});

	assert_bridge_hub_westend_message_accepted(true);

	assert_bridge_hub_rococo_message_received();

	AssetHubRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;

		assert_expected_events!(
			AssetHubRococo,
			vec![
				// Message processed successfully
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
				// Token was issued to beneficiary
				RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, .. }) => {
					asset_id: *asset_id == token_location,
					owner: *owner == beneficiary_acc_bytes.into(),
				},
				// Leftover fees was deposited to beneficiary
				RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, .. }) => {
					asset_id: *asset_id == eth_location(),
					owner: *owner == beneficiary_acc_bytes.into(),
				},
			]
		);

		// Beneficiary received the token transfer value
		assert_eq!(
			ForeignAssets::balance(token_location, AccountId::from(beneficiary_acc_bytes)),
			token_transfer_value
		);

		let events = AssetHubRococo::events();
		// Check that no assets were trapped
		assert!(
			!events.iter().any(|event| matches!(
				event,
				RuntimeEvent::PolkadotXcm(pallet_xcm::Event::AssetsTrapped { .. })
			)),
			"Assets were trapped on Rococo AssetHub, should not happen."
		);
	});
}

#[test]
fn send_ether_to_rococo_v2() {
	let relayer_account = BridgeHubWestendSender::get();
	let relayer_reward = 1_500_000_000_000u128;

	let beneficiary_acc_id: H256 = H256::random();
	let beneficiary_acc_bytes: [u8; 32] = beneficiary_acc_id.into();
	let beneficiary =
		Location::new(0, AccountId32 { network: None, id: beneficiary_acc_id.into() });

	let claimer_acc_id = H256::random();
	let claimer = AccountId32 { network: None, id: claimer_acc_id.into() };
	let claimer_bytes = claimer.encode();

	// set XCM versions
	BridgeHubWestend::force_xcm_version(asset_hub_westend_global_location(), XCM_VERSION);
	BridgeHubWestend::force_xcm_version(asset_hub_rococo_location(), XCM_VERSION);
	AssetHubWestend::force_xcm_version(asset_hub_rococo_location(), XCM_VERSION);

	// To pay fees on Rococo.
	let eth_fee_rococo_ah: xcm::prelude::Asset = (eth_location(), 2_000_000_000_000u128).into();
	let ether_asset_ah: xcm::prelude::Asset = (eth_location(), 4_000_000_000_000u128).into();

	BridgeHubWestend::fund_para_sovereign(AssetHubWestend::para_id(), INITIAL_FUND);

	set_up_eth_and_dot_pool();
	set_up_eth_and_dot_pool_on_rococo();
	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		let instructions = vec![
			// Send message to Rococo AH
			InitiateTransfer {
				// Rococo
				destination: Location::new(
					2,
					[
						GlobalConsensus(ByGenesis(xcm::latest::ROCOCO_GENESIS_HASH)),
						Parachain(1000u32),
					],
				),
				remote_fees: Some(ReserveDeposit(Definite(vec![eth_fee_rococo_ah.clone()].into()))),
				preserve_origin: false,
				assets: BoundedVec::truncate_from(vec![ReserveDeposit(Definite(
					vec![ether_asset_ah.clone()].into(),
				))]),
				remote_xcm: vec![
					// Refund unspent fees
					RefundSurplus,
					// Deposit assets to beneficiary.
					DepositAsset { assets: Wild(AllCounted(3)), beneficiary: beneficiary.clone() },
					SetTopic(H256::random().into()),
				]
				.into(),
			},
			RefundSurplus,
			DepositAsset {
				assets: Wild(AllOf { id: AssetId(eth_location()), fun: WildFungibility::Fungible }),
				beneficiary,
			},
		];
		let xcm: Xcm<()> = instructions.into();
		let versioned_message_xcm = VersionedXcm::V5(xcm);
		let origin = EthereumGatewayAddress::get();

		let message = Message {
			gateway: origin,
			nonce: 1,
			origin,
			assets: vec![],
			payload: Payload::Raw(versioned_message_xcm.encode()),
			claimer: Some(claimer_bytes),
			value: 6_500_000_000_000u128,
			execution_fee: 1_500_000_000_000u128,
			relayer_fee: relayer_reward,
		};

		EthereumInboundQueueV2::process_message(relayer_account.clone(), message).unwrap();

		assert_expected_events!(
			BridgeHubWestend,
			vec![
				RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) => {},
				// Check that the relayer reward was registered.
				RuntimeEvent::BridgeRelayers(pallet_bridge_relayers::Event::RewardRegistered { relayer, reward_kind, reward_balance }) => {
					relayer: *relayer == relayer_account,
					reward_kind: *reward_kind == BridgeReward::Snowbridge,
					reward_balance: *reward_balance == relayer_reward,
				},
			]
		);
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		// Check that the assets were issued on AssetHub
		assert_expected_events!(
			AssetHubWestend,
			vec![
				// Message processed successfully
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
				RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) => {},
			]
		);

		let events = AssetHubWestend::events();
		// Check that no assets were trapped
		assert!(
			!events.iter().any(|event| matches!(
				event,
				RuntimeEvent::PolkadotXcm(pallet_xcm::Event::AssetsTrapped { .. })
			)),
			"Assets were trapped, should not happen."
		);
	});

	assert_bridge_hub_westend_message_accepted(true);

	assert_bridge_hub_rococo_message_received();

	AssetHubRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;

		assert_expected_events!(
			AssetHubRococo,
			vec![
				// Message processed successfully
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
				// Ether was deposited to beneficiary
				RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, .. }) => {
					asset_id: *asset_id == eth_location(),
					owner: *owner == beneficiary_acc_bytes.into(),
				},
			]
		);

		let events = AssetHubRococo::events();
		// Check that no assets were trapped
		assert!(
			!events.iter().any(|event| matches!(
				event,
				RuntimeEvent::PolkadotXcm(pallet_xcm::Event::AssetsTrapped { .. })
			)),
			"Assets were trapped on Rococo AssetHub, should not happen."
		);
	});
}

#[test]
fn send_roc_from_ethereum_to_rococo() {
	let initial_fund: u128 = 200_000_000_000_000;
	let initial_liquidity: u128 = initial_fund / 2;

	let relayer_account = BridgeHubWestendSender::get();
	let relayer_reward = 1_500_000_000_000u128;

	let claimer = AccountId32 { network: None, id: H256::random().into() };
	let claimer_bytes = claimer.encode();
	let beneficiary =
		Location::new(0, AccountId32 { network: None, id: AssetHubRococoReceiver::get().into() });

	BridgeHubWestend::fund_para_sovereign(AssetHubWestend::para_id(), INITIAL_FUND);

	let ethereum_sovereign: AccountId = snowbridge_sovereign();
	let bridged_roc_at_asset_hub_westend = bridged_roc_at_ah_westend();
	create_foreign_on_ah_westend(bridged_roc_at_asset_hub_westend.clone(), true);
	set_up_pool_with_wnd_on_ah_westend(
		bridged_roc_at_asset_hub_westend.clone(),
		true,
		initial_fund,
		initial_liquidity,
	);

	BridgeHubRococo::fund_para_sovereign(AssetHubRococo::para_id(), initial_fund);
	AssetHubRococo::fund_accounts(vec![(AssetHubRococoSender::get(), initial_fund)]);
	register_roc_on_bh();

	set_up_eth_and_dot_pool();
	set_up_eth_and_dot_pool_on_rococo();

	// set XCM versions
	BridgeHubWestend::force_xcm_version(asset_hub_westend_global_location(), XCM_VERSION);
	BridgeHubWestend::force_xcm_version(asset_hub_rococo_location(), XCM_VERSION);
	AssetHubWestend::force_xcm_version(asset_hub_rococo_location(), XCM_VERSION);

	let eth_fee_rococo_ah: xcm::prelude::Asset = (eth_location(), 2_000_000_000_000u128).into();

	let roc = Location::new(1, [GlobalConsensus(ByGenesis(ROCOCO_GENESIS_HASH))]);
	let token_id = TokenIdOf::convert_location(&roc).unwrap();

	let roc_reachored: xcm::prelude::Asset =
		(Location::new(2, [GlobalConsensus(ByGenesis(ROCOCO_GENESIS_HASH))]), TOKEN_AMOUNT).into();

	let assets = vec![
		// the token being transferred
		ForeignTokenERC20 { token_id: token_id.into(), value: TOKEN_AMOUNT },
	];

	AssetHubWestend::execute_with(|| {
		// Mint the asset into the bridge sovereign account, to mimic locked funds
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::mint(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendAssetOwner::get()),
			bridged_roc_at_asset_hub_westend.clone().into(),
			ethereum_sovereign.clone().into(),
			TOKEN_AMOUNT,
		));
	});

	// fund the AHW's SA on AHR with the ROC tokens held in reserve
	let sov_ahw_on_ahr = AssetHubRococo::sovereign_account_of_parachain_on_other_global_consensus(
		ByGenesis(WESTEND_GENESIS_HASH),
		AssetHubWestend::para_id(),
	);
	AssetHubRococo::fund_accounts(vec![(sov_ahw_on_ahr.clone(), INITIAL_FUND)]);

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		let instructions = vec![
			// Send message to Rococo AH
			InitiateTransfer {
				// Rococo
				destination: Location::new(
					2,
					[
						GlobalConsensus(ByGenesis(xcm::latest::ROCOCO_GENESIS_HASH)),
						Parachain(1000u32),
					],
				),
				remote_fees: Some(ReserveDeposit(Definite(vec![eth_fee_rococo_ah.clone()].into()))),
				preserve_origin: false,
				assets: BoundedVec::truncate_from(vec![ReserveWithdraw(Definite(
					vec![roc_reachored.clone()].into(),
				))]),
				remote_xcm: vec![
					// Refund unspent fees
					RefundSurplus,
					// Deposit assets and leftover fees to beneficiary.
					DepositAsset { assets: Wild(AllCounted(2)), beneficiary: beneficiary.clone() },
					SetTopic(H256::random().into()),
				]
				.into(),
			},
			RefundSurplus,
			DepositAsset {
				assets: Wild(AllOf { id: AssetId(eth_location()), fun: WildFungibility::Fungible }),
				beneficiary,
			},
		];
		let xcm: Xcm<()> = instructions.into();
		let versioned_message_xcm = VersionedXcm::V5(xcm);
		let origin = EthereumGatewayAddress::get();

		let message = Message {
			gateway: origin,
			nonce: 1,
			origin,
			assets,
			payload: Payload::Raw(versioned_message_xcm.encode()),
			claimer: Some(claimer_bytes),
			value: 9_500_000_000_000u128,
			execution_fee: 3_500_000_000_000u128,
			relayer_fee: relayer_reward,
		};

		EthereumInboundQueueV2::process_message(relayer_account.clone(), message).unwrap();

		assert_expected_events!(
			BridgeHubWestend,
			vec![
				RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) => {},
				// Check that the relayer reward was registered.
				RuntimeEvent::BridgeRelayers(pallet_bridge_relayers::Event::RewardRegistered { relayer, reward_kind, reward_balance }) => {
					relayer: *relayer == relayer_account,
					reward_kind: *reward_kind == BridgeReward::Snowbridge,
					reward_balance: *reward_balance == relayer_reward,
				},
			]
		);
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		assert_expected_events!(
			AssetHubWestend,
			vec![
				// Message processed successfully
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
			]
		);

		let events = AssetHubWestend::events();
		// Check that no assets were trapped
		assert!(
			!events.iter().any(|event| matches!(
				event,
				RuntimeEvent::PolkadotXcm(pallet_xcm::Event::AssetsTrapped { .. })
			)),
			"Assets were trapped, should not happen."
		);
	});

	assert_bridge_hub_westend_message_accepted(true);

	assert_bridge_hub_rococo_message_received();

	AssetHubRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;

		assert_expected_events!(
			AssetHubRococo,
			vec![
				// ROC is withdrawn from AHW's SA on AHR
				RuntimeEvent::Balances(
					pallet_balances::Event::Burned { who, amount }
				) => {
					who: *who == sov_ahw_on_ahr,
					amount: *amount == TOKEN_AMOUNT,
				},
				// ROCs deposited to beneficiary
				RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. }) => {
					who: *who == AssetHubRococoReceiver::get(),
				},
				// message processed successfully
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
			]
		);

		let events = AssetHubRococo::events();
		// Check that no assets were trapped
		assert!(
			!events.iter().any(|event| matches!(
				event,
				RuntimeEvent::PolkadotXcm(pallet_xcm::Event::AssetsTrapped { .. })
			)),
			"Assets were trapped on Rococo AssetHub, should not happen."
		);
	});
}
