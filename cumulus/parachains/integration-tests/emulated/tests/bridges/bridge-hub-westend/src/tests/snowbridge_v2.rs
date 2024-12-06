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
use crate::imports::*;
use asset_hub_westend_runtime::ForeignAssets;
use bridge_hub_westend_runtime::{
	bridge_to_ethereum_config::{CreateAssetCall, CreateAssetDeposit, EthereumGatewayAddress},
	EthereumInboundQueueV2,
};
use codec::Encode;
use hex_literal::hex;
use snowbridge_router_primitives::inbound::{
	v2::{Asset::NativeTokenERC20, Message},
	EthereumLocationsConverterFor,
};
use sp_core::{H160, H256};
use sp_runtime::MultiAddress;

/// Calculates the XCM prologue fee for sending an XCM to AH.
const INITIAL_FUND: u128 = 5_000_000_000_000;
use testnet_parachains_constants::westend::snowbridge::EthereumNetwork;
const WETH: [u8; 20] = hex!("fff9976782d46cc05630d1f6ebab18b2324d6b14");
/// An ERC-20 token to be registered and sent.
const TOKEN_ID: [u8; 20] = hex!("c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2");
const CHAIN_ID: u64 = 11155111u64;

pub fn weth_location() -> Location {
	erc20_token_location(WETH.into())
}

pub fn erc20_token_location(token_id: H160) -> Location {
	Location::new(
		2,
		[
			GlobalConsensus(EthereumNetwork::get().into()),
			AccountKey20 { network: None, key: token_id.into() },
		],
	)
}

#[test]
fn register_token_v2() {
	let relayer = BridgeHubWestendSender::get();
	let receiver = AssetHubWestendReceiver::get();
	BridgeHubWestend::fund_accounts(vec![(relayer.clone(), INITIAL_FUND)]);

	register_foreign_asset(weth_location());

	set_up_weth_and_dot_pool(weth_location());

	let claimer = AccountId32 { network: None, id: receiver.clone().into() };
	let claimer_bytes = claimer.encode();

	let relayer_location =
		Location::new(0, AccountId32 { network: None, id: relayer.clone().into() });

	let bridge_owner = EthereumLocationsConverterFor::<[u8; 32]>::from_chain_id(&CHAIN_ID);

	let token: H160 = TOKEN_ID.into();
	let asset_id = erc20_token_location(token.into());

	let dot_asset = Location::new(1, Here);
	let dot_fee: xcm::prelude::Asset = (dot_asset, CreateAssetDeposit::get()).into();

	// Used to pay the asset creation deposit.
	let weth_asset_value = 9_000_000_000_000u128;
	let assets = vec![NativeTokenERC20 { token_id: WETH.into(), value: weth_asset_value }];
	let asset_deposit: xcm::prelude::Asset = (weth_location(), weth_asset_value).into();

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		let instructions = vec![
			// Exchange weth for dot to pay the asset creation deposit
			ExchangeAsset {
				give: asset_deposit.clone().into(),
				want: dot_fee.clone().into(),
				maximal: false,
			},
			// Deposit the dot deposit into the bridge sovereign account (where the asset creation
			// fee will be deducted from)
			DepositAsset { assets: dot_fee.into(), beneficiary: bridge_owner.into() },
			// Call to create the asset.
			Transact {
				origin_kind: OriginKind::Xcm,
				call: (
					CreateAssetCall::get(),
					asset_id,
					MultiAddress::<[u8; 32], ()>::Id(bridge_owner.into()),
					1u128,
				)
					.encode()
					.into(),
			},
			ExpectTransactStatus(MaybeErrorCode::Success),
		];
		let xcm: Xcm<()> = instructions.into();
		let versioned_message_xcm = VersionedXcm::V5(xcm);
		let origin = EthereumGatewayAddress::get();

		let message = Message {
			origin,
			fee: 1_500_000_000_000u128,
			assets,
			xcm: versioned_message_xcm.encode(),
			claimer: Some(claimer_bytes),
		};
		let xcm = EthereumInboundQueueV2::do_convert(message, relayer_location).unwrap();
		let _ = EthereumInboundQueueV2::send_xcm(xcm, AssetHubWestend::para_id().into()).unwrap();

		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) => {},]
		);
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		assert_expected_events!(
			AssetHubWestend,
			vec![RuntimeEvent::ForeignAssets(pallet_assets::Event::Created { .. }) => {},]
		);
	});
}

#[test]
fn send_token_v2() {
	let relayer = BridgeHubWestendSender::get();
	let relayer_location =
		Location::new(0, AccountId32 { network: None, id: relayer.clone().into() });

	let token: H160 = TOKEN_ID.into();
	let token_location = erc20_token_location(token);

	let beneficiary_acc_id: H256 = H256::random();
	let beneficiary_acc_bytes: [u8; 32] = beneficiary_acc_id.into();
	let beneficiary =
		Location::new(0, AccountId32 { network: None, id: beneficiary_acc_id.into() });

	let claimer_acc_id = H256::random();
	let claimer_acc_id_bytes: [u8; 32] = claimer_acc_id.into();
	let claimer = AccountId32 { network: None, id: claimer_acc_id.into() };
	let claimer_bytes = claimer.encode();

	register_foreign_asset(weth_location());
	register_foreign_asset(token_location.clone());

	let token_transfer_value = 2_000_000_000_000u128;

	let assets = vec![
		// to pay fees
		NativeTokenERC20 { token_id: WETH.into(), value: 1_500_000_000_000u128 },
		// the token being transferred
		NativeTokenERC20 { token_id: token.into(), value: token_transfer_value },
	];

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		let instructions = vec![DepositAsset {
			assets: Wild(AllOf {
				id: AssetId(token_location.clone()),
				fun: WildFungibility::Fungible,
			}),
			beneficiary,
		}];
		let xcm: Xcm<()> = instructions.into();
		let versioned_message_xcm = VersionedXcm::V5(xcm);
		let origin = EthereumGatewayAddress::get();

		let message = Message {
			origin,
			fee: 1_500_000_000_000u128,
			assets,
			xcm: versioned_message_xcm.encode(),
			claimer: Some(claimer_bytes),
		};

		let xcm = EthereumInboundQueueV2::do_convert(message, relayer_location).unwrap();
		let _ = EthereumInboundQueueV2::send_xcm(xcm, AssetHubWestend::para_id().into()).unwrap();

		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) => {},]
		);
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		// Check that the token was received and issued as a foreign asset on AssetHub
		assert_expected_events!(
			AssetHubWestend,
			vec![RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { .. }) => {},]
		);

		// Beneficiary received the token transfer value
		assert_eq!(
			ForeignAssets::balance(token_location, AccountId::from(beneficiary_acc_bytes)),
			token_transfer_value
		);

		// Claimer received weth refund for fees paid
		assert!(ForeignAssets::balance(weth_location(), AccountId::from(claimer_acc_id_bytes)) > 0);
	});
}

#[test]
fn send_weth_v2() {
	let relayer = BridgeHubWestendSender::get();
	let relayer_location =
		Location::new(0, AccountId32 { network: None, id: relayer.clone().into() });

	let beneficiary_acc_id: H256 = H256::random();
	let beneficiary_acc_bytes: [u8; 32] = beneficiary_acc_id.into();
	let beneficiary =
		Location::new(0, AccountId32 { network: None, id: beneficiary_acc_id.into() });

	let claimer_acc_id = H256::random();
	let claimer_acc_id_bytes: [u8; 32] = claimer_acc_id.into();
	let claimer = AccountId32 { network: None, id: claimer_acc_id.into() };
	let claimer_bytes = claimer.encode();

	register_foreign_asset(weth_location());

	let token_transfer_value = 2_000_000_000_000u128;

	let assets = vec![
		// to pay fees
		NativeTokenERC20 { token_id: WETH.into(), value: token_transfer_value },
		// the token being transferred
	];

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		let instructions = vec![DepositAsset {
			assets: Wild(AllOf {
				id: AssetId(weth_location().clone()),
				fun: WildFungibility::Fungible,
			}),
			beneficiary,
		}];
		let xcm: Xcm<()> = instructions.into();
		let versioned_message_xcm = VersionedXcm::V5(xcm);
		let origin = EthereumGatewayAddress::get();

		let message = Message {
			origin,
			fee: 1_500_000_000_000u128,
			assets,
			xcm: versioned_message_xcm.encode(),
			claimer: Some(claimer_bytes),
		};

		let xcm = EthereumInboundQueueV2::do_convert(message, relayer_location).unwrap();
		let _ = EthereumInboundQueueV2::send_xcm(xcm, AssetHubWestend::para_id().into()).unwrap();

		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) => {},]
		);
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		// Check that the token was received and issued as a foreign asset on AssetHub
		assert_expected_events!(
			AssetHubWestend,
			vec![RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { .. }) => {},]
		);

		// Beneficiary received the token transfer value
		assert_eq!(
			ForeignAssets::balance(weth_location(), AccountId::from(beneficiary_acc_bytes)),
			token_transfer_value
		);

		// Claimer received weth refund for fees paid
		assert!(ForeignAssets::balance(weth_location(), AccountId::from(claimer_acc_id_bytes)) > 0);
	});
}

#[test]
fn register_and_send_multiple_tokens_v2() {
	let relayer = BridgeHubWestendSender::get();
	let relayer_location =
		Location::new(0, AccountId32 { network: None, id: relayer.clone().into() });

	let token: H160 = TOKEN_ID.into();
	let token_location = erc20_token_location(token);

	let bridge_owner = EthereumLocationsConverterFor::<[u8; 32]>::from_chain_id(&CHAIN_ID);

	let beneficiary_acc_id: H256 = H256::random();
	let beneficiary_acc_bytes: [u8; 32] = beneficiary_acc_id.into();
	let beneficiary =
		Location::new(0, AccountId32 { network: None, id: beneficiary_acc_id.clone().into() });

	// To satisfy ED
	AssetHubWestend::fund_accounts(vec![(
		sp_runtime::AccountId32::from(beneficiary_acc_bytes),
		3_000_000_000_000,
	)]);

	let claimer_acc_id = H256::random();
	let claimer_acc_id_bytes: [u8; 32] = claimer_acc_id.into();
	let claimer = AccountId32 { network: None, id: claimer_acc_id.into() };
	let claimer_bytes = claimer.encode();

	register_foreign_asset(weth_location());

	set_up_weth_and_dot_pool(weth_location());

	let token_transfer_value = 2_000_000_000_000u128;
	let weth_transfer_value = 2_500_000_000_000u128;

	let dot_asset = Location::new(1, Here);
	let dot_fee: xcm::prelude::Asset = (dot_asset, CreateAssetDeposit::get()).into();

	// Used to pay the asset creation deposit.
	let weth_asset_value = 9_000_000_000_000u128;
	let asset_deposit: xcm::prelude::Asset = (weth_location(), weth_asset_value).into();

	let assets = vec![
		// to pay fees and transfer assets
		NativeTokenERC20 { token_id: WETH.into(), value: 2_800_000_000_000u128 },
		// the token being transferred
		NativeTokenERC20 { token_id: token.into(), value: token_transfer_value },
	];

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		let instructions = vec![
			ExchangeAsset {
				give: asset_deposit.clone().into(),
				want: dot_fee.clone().into(),
				maximal: false,
			},
			DepositAsset { assets: dot_fee.into(), beneficiary: bridge_owner.into() },
			// register new token
			Transact {
				origin_kind: OriginKind::Xcm,
				call: (
					CreateAssetCall::get(),
					token_location.clone(),
					MultiAddress::<[u8; 32], ()>::Id(bridge_owner.into()),
					1u128,
				)
					.encode()
					.into(),
			},
			ExpectTransactStatus(MaybeErrorCode::Success),
			// deposit new token to beneficiary
			DepositAsset {
				assets: Wild(AllOf {
					id: AssetId(token_location.clone()),
					fun: WildFungibility::Fungible,
				}),
				beneficiary: beneficiary.clone(),
			},
			// deposit weth to beneficiary
			DepositAsset {
				assets: Wild(AllOf {
					id: AssetId(weth_location()),
					fun: WildFungibility::Fungible,
				}),
				beneficiary: beneficiary.clone(),
			},
		];
		let xcm: Xcm<()> = instructions.into();
		let versioned_message_xcm = VersionedXcm::V5(xcm);
		let origin = EthereumGatewayAddress::get();

		let message = Message {
			origin,
			fee: 1_500_000_000_000u128,
			assets,
			xcm: versioned_message_xcm.encode(),
			claimer: Some(claimer_bytes),
		};

		let xcm = EthereumInboundQueueV2::do_convert(message, relayer_location).unwrap();
		let _ = EthereumInboundQueueV2::send_xcm(xcm, AssetHubWestend::para_id().into()).unwrap();

		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) => {},]
		);
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		// The token was created
		assert_expected_events!(
			AssetHubWestend,
			vec![RuntimeEvent::ForeignAssets(pallet_assets::Event::Created { .. }) => {},]
		);

		// Check that the token was received and issued as a foreign asset on AssetHub
		assert_expected_events!(
			AssetHubWestend,
			vec![RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { .. }) => {},]
		);

		// Beneficiary received the token transfer value
		assert_eq!(
			ForeignAssets::balance(token_location, AccountId::from(beneficiary_acc_bytes)),
			token_transfer_value
		);

		// Beneficiary received the weth transfer value
		assert!(
			ForeignAssets::balance(weth_location(), AccountId::from(beneficiary_acc_bytes)) >
				weth_transfer_value
		);

		// Claimer received weth refund for fees paid
		assert!(ForeignAssets::balance(weth_location(), AccountId::from(claimer_acc_id_bytes)) > 0);
	});
}

#[test]
fn invalid_xcm_traps_funds_on_ah() {
	let relayer = BridgeHubWestendSender::get();
	let relayer_location =
		Location::new(0, AccountId32 { network: None, id: relayer.clone().into() });

	let token: H160 = TOKEN_ID.into();
	let claimer = AccountId32 { network: None, id: H256::random().into() };
	let claimer_bytes = claimer.encode();
	let beneficiary_acc_bytes: [u8; 32] = H256::random().into();

	AssetHubWestend::fund_accounts(vec![(
		sp_runtime::AccountId32::from(beneficiary_acc_bytes),
		3_000_000_000_000,
	)]);

	register_foreign_asset(weth_location());

	set_up_weth_and_dot_pool(weth_location());

	let assets = vec![
		// to pay fees and transfer assets
		NativeTokenERC20 { token_id: WETH.into(), value: 2_800_000_000_000u128 },
		// the token being transferred
		NativeTokenERC20 { token_id: token.into(), value: 2_000_000_000_000u128 },
	];

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		// invalid xcm
		let instructions = hex!("02806c072d50e2c7cd6821d1f084cbb4");
		let origin = EthereumGatewayAddress::get();

		let message = Message {
			origin,
			fee: 1_500_000_000_000u128,
			assets,
			xcm: instructions.to_vec(),
			claimer: Some(claimer_bytes),
		};

		let xcm = EthereumInboundQueueV2::do_convert(message, relayer_location).unwrap();
		let _ = EthereumInboundQueueV2::send_xcm(xcm, AssetHubWestend::para_id().into()).unwrap();

		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) => {},]
		);
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		// Assets are trapped
		assert_expected_events!(
			AssetHubWestend,
			vec![RuntimeEvent::PolkadotXcm(pallet_xcm::Event::AssetsTrapped { .. }) => {},]
		);
	});
}

#[test]
fn invalid_claimer_does_not_fail_the_message() {
	let relayer = BridgeHubWestendSender::get();
	let relayer_location =
		Location::new(0, AccountId32 { network: None, id: relayer.clone().into() });

	let beneficiary_acc: [u8; 32] = H256::random().into();
	let beneficiary = Location::new(0, AccountId32 { network: None, id: beneficiary_acc.into() });

	register_foreign_asset(weth_location());

	let token_transfer_value = 2_000_000_000_000u128;

	let assets = vec![
		// to pay fees
		NativeTokenERC20 { token_id: WETH.into(), value: token_transfer_value },
		// the token being transferred
	];

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		let instructions = vec![DepositAsset {
			assets: Wild(AllOf {
				id: AssetId(weth_location().clone()),
				fun: WildFungibility::Fungible,
			}),
			beneficiary,
		}];
		let xcm: Xcm<()> = instructions.into();
		let versioned_message_xcm = VersionedXcm::V5(xcm);
		let origin = EthereumGatewayAddress::get();

		let message = Message {
			origin,
			fee: 1_500_000_000_000u128,
			assets,
			xcm: versioned_message_xcm.encode(),
			// Set an invalid claimer
			claimer: Some(hex!("2b7ce7bc7e87e4d6619da21487c7a53f").to_vec()),
		};

		let xcm = EthereumInboundQueueV2::do_convert(message, relayer_location).unwrap();
		let _ = EthereumInboundQueueV2::send_xcm(xcm, AssetHubWestend::para_id().into()).unwrap();

		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) => {},]
		);
	});

	// Message still processes successfully
	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		// Check that the token was received and issued as a foreign asset on AssetHub
		assert_expected_events!(
			AssetHubWestend,
			vec![RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { .. }) => {},]
		);

		// Beneficiary received the token transfer value
		assert_eq!(
			ForeignAssets::balance(weth_location(), AccountId::from(beneficiary_acc)),
			token_transfer_value
		);

		// Relayer (instead of claimer) received weth refund for fees paid
		assert!(ForeignAssets::balance(weth_location(), AccountId::from(relayer)) > 0);
	});
}

pub fn register_foreign_asset(token_location: Location) {
	let assethub_location = BridgeHubWestend::sibling_location_of(AssetHubWestend::para_id());
	let assethub_sovereign = BridgeHubWestend::sovereign_account_id_of(assethub_location);
	AssetHubWestend::execute_with(|| {
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::force_create(
			RuntimeOrigin::root(),
			token_location.clone().try_into().unwrap(),
			assethub_sovereign.clone().into(),
			true,
			1000,
		));

		assert!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::asset_exists(
			token_location.clone().try_into().unwrap(),
		));
	});
}

pub(crate) fn set_up_weth_and_dot_pool(asset: v5::Location) {
	let wnd: v5::Location = v5::Parent.into();
	let assethub_location = BridgeHubWestend::sibling_location_of(AssetHubWestend::para_id());
	let owner = AssetHubWestendSender::get();
	let bh_sovereign = BridgeHubWestend::sovereign_account_id_of(assethub_location);

	AssetHubWestend::fund_accounts(vec![(owner.clone(), 3_000_000_000_000)]);

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		let signed_owner = <AssetHubWestend as Chain>::RuntimeOrigin::signed(owner.clone());
		let signed_bh_sovereign =
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(bh_sovereign.clone());

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::mint(
			signed_bh_sovereign.clone(),
			asset.clone().into(),
			bh_sovereign.clone().into(),
			3_500_000_000_000,
		));

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::transfer(
			signed_bh_sovereign.clone(),
			asset.clone().into(),
			owner.clone().into(),
			3_000_000_000_000,
		));

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::create_pool(
			signed_owner.clone(),
			Box::new(wnd.clone()),
			Box::new(asset.clone()),
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::PoolCreated { .. }) => {},
			]
		);

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::add_liquidity(
			signed_owner.clone(),
			Box::new(wnd),
			Box::new(asset),
			1_000_000_000_000,
			2_000_000_000_000,
			1,
			1,
			owner.into()
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::LiquidityAdded {..}) => {},
			]
		);
	});
}
