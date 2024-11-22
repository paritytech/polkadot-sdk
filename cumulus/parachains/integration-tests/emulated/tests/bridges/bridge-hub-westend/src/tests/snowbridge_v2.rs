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
use bridge_hub_westend_runtime::EthereumInboundQueue;
use frame_support::traits::fungibles::Mutate;
use hex_literal::hex;
use snowbridge_core::{
	transact::{TransactInfo, TransactKind},
	AssetMetadata,
};
use snowbridge_router_primitives::inbound::{
	v1::{Command, Destination, MessageV1, VersionedMessage},
	EthereumLocationsConverterFor,
};
use sp_runtime::MultiAddress;
use testnet_parachains_constants::westend::snowbridge::EthereumNetwork;
use xcm::v5::AssetTransferFilter;
use xcm_executor::traits::ConvertLocation;

const INITIAL_FUND: u128 = 5_000_000_000_000;
pub const CHAIN_ID: u64 = 11155111;
pub const WETH: [u8; 20] = hex!("87d1f7fdfEe7f651FaBc8bFCB6E086C278b77A7d");
const ETHEREUM_DESTINATION_ADDRESS: [u8; 20] = hex!("44a57ee2f2FCcb85FDa2B0B18EBD0D8D2333700e");
const XCM_FEE: u128 = 100_000_000_000;
const TOKEN_AMOUNT: u128 = 100_000_000_000;

pub fn fund_sovereign() {
	let assethub_location = BridgeHubWestend::sibling_location_of(AssetHubWestend::para_id());
	let assethub_sovereign = BridgeHubWestend::sovereign_account_id_of(assethub_location);
	BridgeHubWestend::fund_accounts(vec![(assethub_sovereign.clone(), INITIAL_FUND)]);
}

pub fn register_weth() {
	let assethub_location = BridgeHubWestend::sibling_location_of(AssetHubWestend::para_id());
	let assethub_sovereign = BridgeHubWestend::sovereign_account_id_of(assethub_location);
	let weth_asset_location: Location =
		(Parent, Parent, EthereumNetwork::get(), AccountKey20 { network: None, key: WETH }).into();
	AssetHubWestend::execute_with(|| {
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::force_create(
			RuntimeOrigin::root(),
			weth_asset_location.clone().try_into().unwrap(),
			assethub_sovereign.clone().into(),
			false,
			1,
		));

		assert!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::asset_exists(
			weth_asset_location.clone().try_into().unwrap(),
		));

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::mint_into(
			weth_asset_location.clone().try_into().unwrap(),
			&AssetHubWestendReceiver::get(),
			TOKEN_AMOUNT,
		));
	});
}

#[test]
fn send_weth_from_asset_hub_to_ethereum() {
	let assethub_location = BridgeHubWestend::sibling_location_of(AssetHubWestend::para_id());
	let assethub_sovereign = BridgeHubWestend::sovereign_account_id_of(assethub_location);
	let weth_asset_location: Location =
		(Parent, Parent, EthereumNetwork::get(), AccountKey20 { network: None, key: WETH }).into();

	BridgeHubWestend::fund_accounts(vec![(assethub_sovereign.clone(), INITIAL_FUND)]);

	AssetHubWestend::execute_with(|| {
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::force_create(
			RuntimeOrigin::root(),
			weth_asset_location.clone().try_into().unwrap(),
			assethub_sovereign.clone().into(),
			false,
			1,
		));

		assert!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::asset_exists(
			weth_asset_location.clone().try_into().unwrap(),
		));
	});

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;

		let message = VersionedMessage::V1(MessageV1 {
			chain_id: CHAIN_ID,
			command: Command::SendToken {
				token: WETH.into(),
				destination: Destination::AccountId32 { id: AssetHubWestendReceiver::get().into() },
				amount: TOKEN_AMOUNT,
				fee: XCM_FEE,
			},
		});
		let (xcm, _) = EthereumInboundQueue::do_convert([0; 32].into(), message).unwrap();
		let _ = EthereumInboundQueue::send_xcm(xcm, AssetHubWestend::para_id().into()).unwrap();

		// Check that the send token message was sent using xcm
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) =>{},]
		);
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		// Check that AssetHub has issued the foreign asset
		assert_expected_events!(
			AssetHubWestend,
			vec![RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { .. }) => {},]
		);

		// Local fee amount(in DOT) should cover
		// 1. execution cost on AH
		// 2. delivery cost to BH
		// 3. execution cost on BH
		let local_fee_amount = 200_000_000_000;
		// Remote fee amount(in WETH) should cover execution cost on Ethereum
		let remote_fee_amount = 4_000_000_000;
		let local_fee_asset =
			Asset { id: AssetId(Location::parent()), fun: Fungible(local_fee_amount) };
		let remote_fee_asset =
			Asset { id: AssetId(weth_asset_location.clone()), fun: Fungible(remote_fee_amount) };
		let reserve_asset = Asset {
			id: AssetId(weth_asset_location.clone()),
			fun: Fungible(TOKEN_AMOUNT - remote_fee_amount),
		};
		let assets = vec![
			Asset { id: weth_asset_location.clone().into(), fun: Fungible(TOKEN_AMOUNT) },
			local_fee_asset.clone(),
		];
		let destination = Location::new(2, [GlobalConsensus(Ethereum { chain_id: CHAIN_ID })]);

		let beneficiary = Location::new(
			0,
			[AccountKey20 { network: None, key: ETHEREUM_DESTINATION_ADDRESS.into() }],
		);

		let xcm_on_bh = Xcm(vec![DepositAsset { assets: Wild(AllCounted(2)), beneficiary }]);

		let xcms = VersionedXcm::from(Xcm(vec![
			WithdrawAsset(assets.clone().into()),
			PayFees { asset: local_fee_asset.clone() },
			InitiateTransfer {
				destination,
				remote_fees: Some(AssetTransferFilter::ReserveWithdraw(Definite(
					remote_fee_asset.clone().into(),
				))),
				preserve_origin: true,
				assets: vec![AssetTransferFilter::ReserveWithdraw(Definite(
					reserve_asset.clone().into(),
				))],
				remote_xcm: xcm_on_bh,
			},
		]));

		// Send the Weth back to Ethereum
		<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::execute(
			RuntimeOrigin::signed(AssetHubWestendReceiver::get()),
			bx!(xcms),
			Weight::from(8_000_000_000),
		)
		.unwrap();
	});

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		// Check that the transfer token back to Ethereum message was queue in the Ethereum
		// Outbound Queue
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::EthereumOutboundQueueV2(snowbridge_pallet_outbound_queue_v2::Event::MessageQueued{ .. }) => {},]
		);
		let events = BridgeHubWestend::events();
		// Check that the remote fee was credited to the AssetHub sovereign account
		assert!(
			events.iter().any(|event| matches!(
				event,
				RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. })
					if *who == assethub_sovereign
			)),
			"AssetHub sovereign takes remote fee."
		);
	});
}

#[test]
fn transfer_relay_token() {
	let assethub_sovereign = BridgeHubWestend::sovereign_account_id_of(
		BridgeHubWestend::sibling_location_of(AssetHubWestend::para_id()),
	);
	BridgeHubWestend::fund_accounts(vec![(assethub_sovereign.clone(), INITIAL_FUND)]);

	let asset_id: Location = Location { parents: 1, interior: [].into() };
	let _expected_asset_id: Location = Location {
		parents: 1,
		interior: [GlobalConsensus(ByGenesis(WESTEND_GENESIS_HASH))].into(),
	};

	let ethereum_sovereign: AccountId =
		EthereumLocationsConverterFor::<[u8; 32]>::convert_location(&Location::new(
			2,
			[GlobalConsensus(EthereumNetwork::get())],
		))
		.unwrap()
		.into();

	// Register token
	BridgeHubWestend::execute_with(|| {
		type RuntimeOrigin = <BridgeHubWestend as Chain>::RuntimeOrigin;
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;

		assert_ok!(<BridgeHubWestend as BridgeHubWestendPallet>::Balances::force_set_balance(
			RuntimeOrigin::root(),
			sp_runtime::MultiAddress::Id(BridgeHubWestendSender::get()),
			INITIAL_FUND * 10,
		));

		assert_ok!(<BridgeHubWestend as BridgeHubWestendPallet>::EthereumSystem::register_token(
			RuntimeOrigin::root(),
			Box::new(VersionedLocation::from(asset_id.clone())),
			AssetMetadata {
				name: "wnd".as_bytes().to_vec().try_into().unwrap(),
				symbol: "wnd".as_bytes().to_vec().try_into().unwrap(),
				decimals: 12,
			},
		));
		// Check that a message was sent to Ethereum to create the agent
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::EthereumSystem(snowbridge_pallet_system::Event::RegisterToken { .. }) => {},]
		);
	});

	// Send token to Ethereum
	AssetHubWestend::execute_with(|| {
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		let weth_asset_location: Location =
			(Parent, Parent, EthereumNetwork::get(), AccountKey20 { network: None, key: WETH })
				.into();

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::force_create(
			RuntimeOrigin::root(),
			weth_asset_location.clone().try_into().unwrap(),
			assethub_sovereign.clone().into(),
			false,
			1,
		));

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::mint(
			RuntimeOrigin::signed(assethub_sovereign.clone().into()),
			weth_asset_location.clone().try_into().unwrap(),
			MultiAddress::Id(AssetHubWestendSender::get()),
			TOKEN_AMOUNT,
		));

		// Local fee amount(in DOT) should cover
		// 1. execution cost on AH
		// 2. delivery cost to BH
		// 3. execution cost on BH
		let local_fee_amount = 200_000_000_000;
		// Remote fee amount(in WETH) should cover execution cost on Ethereum
		let remote_fee_amount = 4_000_000_000;

		let local_fee_asset =
			Asset { id: AssetId(Location::parent()), fun: Fungible(local_fee_amount) };
		let remote_fee_asset =
			Asset { id: AssetId(weth_asset_location.clone()), fun: Fungible(remote_fee_amount) };

		let assets = vec![
			Asset {
				id: AssetId(Location::parent()),
				fun: Fungible(TOKEN_AMOUNT + local_fee_amount),
			},
			remote_fee_asset.clone(),
		];

		let destination = Location::new(2, [GlobalConsensus(Ethereum { chain_id: CHAIN_ID })]);

		let beneficiary = Location::new(
			0,
			[AccountKey20 { network: None, key: ETHEREUM_DESTINATION_ADDRESS.into() }],
		);

		let xcm_on_bh = Xcm(vec![DepositAsset { assets: Wild(AllCounted(2)), beneficiary }]);

		let xcms = VersionedXcm::from(Xcm(vec![
			WithdrawAsset(assets.clone().into()),
			PayFees { asset: local_fee_asset.clone() },
			InitiateTransfer {
				destination,
				remote_fees: Some(AssetTransferFilter::ReserveWithdraw(Definite(
					remote_fee_asset.clone().into(),
				))),
				preserve_origin: true,
				assets: vec![AssetTransferFilter::ReserveDeposit(Definite(
					Asset { id: AssetId(Location::parent()), fun: Fungible(TOKEN_AMOUNT) }.into(),
				))],
				remote_xcm: xcm_on_bh,
			},
		]));

		// Send DOT to Ethereum
		<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::execute(
			RuntimeOrigin::signed(AssetHubWestendSender::get()),
			bx!(xcms),
			Weight::from(8_000_000_000),
		)
		.unwrap();

		// Check that the native asset transferred to some reserved account(sovereign of Ethereum)
		let events = AssetHubWestend::events();
		assert!(
			events.iter().any(|event| matches!(
				event,
				RuntimeEvent::Balances(pallet_balances::Event::Minted { who, amount})
					if *who == ethereum_sovereign.clone() && *amount == TOKEN_AMOUNT,
			)),
			"native token reserved to Ethereum sovereign account."
		);
	});

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;

		// Check that the transfer token back to Ethereum message was queue in the Ethereum
		// Outbound Queue
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::EthereumOutboundQueueV2(snowbridge_pallet_outbound_queue_v2::Event::MessageQueued{ .. }) => {},]
		);
	});
}

#[test]
fn send_weth_and_dot_from_asset_hub_to_ethereum() {
	let assethub_location = BridgeHubWestend::sibling_location_of(AssetHubWestend::para_id());
	let assethub_sovereign = BridgeHubWestend::sovereign_account_id_of(assethub_location);
	let weth_asset_location: Location =
		(Parent, Parent, EthereumNetwork::get(), AccountKey20 { network: None, key: WETH }).into();

	BridgeHubWestend::fund_accounts(vec![(assethub_sovereign.clone(), INITIAL_FUND)]);

	AssetHubWestend::execute_with(|| {
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		// Register WETH on AH
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::force_create(
			RuntimeOrigin::root(),
			weth_asset_location.clone().try_into().unwrap(),
			assethub_sovereign.clone().into(),
			false,
			1,
		));

		assert!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::asset_exists(
			weth_asset_location.clone().try_into().unwrap(),
		));
	});

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		type RuntimeOrigin = <BridgeHubWestend as Chain>::RuntimeOrigin;

		// Register WND on BH
		assert_ok!(<BridgeHubWestend as BridgeHubWestendPallet>::Balances::force_set_balance(
			RuntimeOrigin::root(),
			MultiAddress::Id(BridgeHubWestendSender::get()),
			INITIAL_FUND * 10,
		));
		assert_ok!(<BridgeHubWestend as BridgeHubWestendPallet>::EthereumSystem::register_token(
			RuntimeOrigin::root(),
			Box::new(VersionedLocation::from(Location::parent())),
			AssetMetadata {
				name: "wnd".as_bytes().to_vec().try_into().unwrap(),
				symbol: "wnd".as_bytes().to_vec().try_into().unwrap(),
				decimals: 12,
			},
		));
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::EthereumSystem(snowbridge_pallet_system::Event::RegisterToken { .. }) => {},]
		);

		// Transfer some WETH to AH
		let message = VersionedMessage::V1(MessageV1 {
			chain_id: CHAIN_ID,
			command: Command::SendToken {
				token: WETH.into(),
				destination: Destination::AccountId32 { id: AssetHubWestendReceiver::get().into() },
				amount: TOKEN_AMOUNT,
				fee: XCM_FEE,
			},
		});
		let (xcm, _) = EthereumInboundQueue::do_convert([0; 32].into(), message).unwrap();
		let _ = EthereumInboundQueue::send_xcm(xcm, AssetHubWestend::para_id().into()).unwrap();
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) =>{},]
		);
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		// Check that AssetHub has issued the foreign asset
		assert_expected_events!(
			AssetHubWestend,
			vec![RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { .. }) => {},]
		);

		// Local fee amount(in DOT) should cover
		// 1. execution cost on AH
		// 2. delivery cost to BH
		// 3. execution cost on BH
		let local_fee_amount = 200_000_000_000;
		// Remote fee amount(in WETH) should cover execution cost on Ethereum
		let remote_fee_amount = 4_000_000_000;

		let local_fee_asset =
			Asset { id: AssetId(Location::parent()), fun: Fungible(local_fee_amount) };
		let remote_fee_asset =
			Asset { id: AssetId(weth_asset_location.clone()), fun: Fungible(remote_fee_amount) };
		let reserve_asset = Asset {
			id: AssetId(weth_asset_location.clone()),
			fun: Fungible(TOKEN_AMOUNT - remote_fee_amount),
		};

		let weth_asset =
			Asset { id: weth_asset_location.clone().into(), fun: Fungible(TOKEN_AMOUNT) };
		let dot_asset = Asset { id: AssetId(Location::parent()), fun: Fungible(TOKEN_AMOUNT) };

		let assets = vec![weth_asset, dot_asset.clone(), local_fee_asset.clone()];
		let destination = Location::new(2, [GlobalConsensus(Ethereum { chain_id: CHAIN_ID })]);

		let beneficiary = Location::new(
			0,
			[AccountKey20 { network: None, key: ETHEREUM_DESTINATION_ADDRESS.into() }],
		);

		let xcm_on_bh = Xcm(vec![DepositAsset { assets: Wild(All), beneficiary }]);

		let xcms = VersionedXcm::from(Xcm(vec![
			WithdrawAsset(assets.clone().into()),
			PayFees { asset: local_fee_asset.clone() },
			InitiateTransfer {
				destination,
				remote_fees: Some(AssetTransferFilter::ReserveWithdraw(Definite(
					remote_fee_asset.clone().into(),
				))),
				preserve_origin: true,
				assets: vec![
					AssetTransferFilter::ReserveWithdraw(Definite(reserve_asset.clone().into())),
					AssetTransferFilter::ReserveDeposit(Definite(dot_asset.into())),
				],
				remote_xcm: xcm_on_bh,
			},
		]));

		// Send the Weth back to Ethereum
		<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::execute(
			RuntimeOrigin::signed(AssetHubWestendReceiver::get()),
			bx!(xcms),
			Weight::from(8_000_000_000),
		)
		.unwrap();
	});

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		// Check that the transfer token back to Ethereum message was queue in the Ethereum
		// Outbound Queue
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::EthereumOutboundQueueV2(snowbridge_pallet_outbound_queue_v2::Event::MessageQueued{ .. }) => {},]
		);
		let events = BridgeHubWestend::events();
		// Check that the remote fee was credited to the AssetHub sovereign account
		assert!(
			events.iter().any(|event| matches!(
				event,
				RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. })
					if *who == assethub_sovereign
			)),
			"AssetHub sovereign takes remote fee."
		);
	});
}

#[test]
fn create_agent() {
	let weth_asset_location: Location =
		(Parent, Parent, EthereumNetwork::get(), AccountKey20 { network: None, key: WETH }).into();

	fund_sovereign();

	register_weth();

	BridgeHubWestend::execute_with(|| {});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		let local_fee_amount = 200_000_000_000;

		let remote_fee_amount = 4_000_000_000;

		let local_fee_asset =
			Asset { id: AssetId(Location::parent()), fun: Fungible(local_fee_amount) };
		let remote_fee_asset =
			Asset { id: AssetId(weth_asset_location.clone()), fun: Fungible(remote_fee_amount) };
		let reserve_asset = Asset {
			id: AssetId(weth_asset_location.clone()),
			fun: Fungible(TOKEN_AMOUNT - remote_fee_amount),
		};
		let assets = vec![
			Asset { id: weth_asset_location.clone().into(), fun: Fungible(TOKEN_AMOUNT) },
			local_fee_asset.clone(),
		];
		let destination = Location::new(2, [GlobalConsensus(Ethereum { chain_id: CHAIN_ID })]);

		let beneficiary = Location::new(
			0,
			[AccountKey20 { network: None, key: ETHEREUM_DESTINATION_ADDRESS.into() }],
		);

		let transact_info = TransactInfo { kind: TransactKind::RegisterAgent, params: vec![] };

		let xcms = VersionedXcm::from(Xcm(vec![
			WithdrawAsset(assets.clone().into()),
			PayFees { asset: local_fee_asset.clone() },
			InitiateTransfer {
				destination,
				remote_fees: Some(AssetTransferFilter::ReserveWithdraw(Definite(
					remote_fee_asset.clone().into(),
				))),
				preserve_origin: true,
				assets: vec![AssetTransferFilter::ReserveWithdraw(Definite(
					reserve_asset.clone().into(),
				))],
				remote_xcm: Xcm(vec![
					DepositAsset { assets: Wild(AllCounted(2)), beneficiary },
					Transact {
						origin_kind: OriginKind::SovereignAccount,
						call: transact_info.encode().into(),
					},
				]),
			},
		]));

		// Send the Weth back to Ethereum
		<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::execute(
			RuntimeOrigin::signed(AssetHubWestendReceiver::get()),
			bx!(xcms),
			Weight::from(8_000_000_000),
		)
		.unwrap();
	});

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		// Check that the transfer token back to Ethereum message was queue in the Ethereum
		// Outbound Queue
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::EthereumOutboundQueueV2(snowbridge_pallet_outbound_queue_v2::Event::MessageQueued{ .. }) => {},]
		);
	});
}

#[test]
fn transact_with_agent() {}
