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
use frame_support::traits::fungibles::Mutate;
use hex_literal::hex;
use snowbridge_core::AssetMetadata;
use snowbridge_outbound_primitives::TransactInfo;
use snowbridge_router_primitives::inbound::EthereumLocationsConverterFor;
use testnet_parachains_constants::westend::snowbridge::EthereumNetwork;
use xcm::v5::AssetTransferFilter;
use xcm_executor::traits::ConvertLocation;

const INITIAL_FUND: u128 = 50_000_000_000_000;
pub const CHAIN_ID: u64 = 11155111;
pub const WETH: [u8; 20] = hex!("c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2");
const ETHEREUM_DESTINATION_ADDRESS: [u8; 20] = hex!("44a57ee2f2FCcb85FDa2B0B18EBD0D8D2333700e");
const AGENT_ADDRESS: [u8; 20] = hex!("90A987B944Cb1dCcE5564e5FDeCD7a54D3de27Fe");
const TOKEN_AMOUNT: u128 = 100_000_000_000;
const REMOTE_FEE_AMOUNT_IN_WETH: u128 = 4_000_000_000;
const LOCAL_FEE_AMOUNT_IN_DOT: u128 = 200_000_000_000;

const EXECUTION_WEIGHT: u64 = 8_000_000_000;

pub fn weth_location() -> Location {
	Location::new(
		2,
		[
			GlobalConsensus(Ethereum { chain_id: CHAIN_ID }),
			AccountKey20 { network: None, key: WETH },
		],
	)
}

pub fn destination() -> Location {
	Location::new(2, [GlobalConsensus(Ethereum { chain_id: CHAIN_ID })])
}

pub fn beneficiary() -> Location {
	Location::new(0, [AccountKey20 { network: None, key: ETHEREUM_DESTINATION_ADDRESS.into() }])
}

pub fn fund_sovereign() {
	let assethub_location = BridgeHubWestend::sibling_location_of(AssetHubWestend::para_id());
	let assethub_sovereign = BridgeHubWestend::sovereign_account_id_of(assethub_location);
	BridgeHubWestend::fund_accounts(vec![(assethub_sovereign.clone(), INITIAL_FUND)]);
}

pub fn register_weth() {
	let assethub_location = BridgeHubWestend::sibling_location_of(AssetHubWestend::para_id());
	let assethub_sovereign = BridgeHubWestend::sovereign_account_id_of(assethub_location);
	AssetHubWestend::execute_with(|| {
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::force_create(
			RuntimeOrigin::root(),
			weth_location().try_into().unwrap(),
			assethub_sovereign.clone().into(),
			false,
			1,
		));

		assert!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::asset_exists(
			weth_location().try_into().unwrap(),
		));

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::mint_into(
			weth_location().try_into().unwrap(),
			&AssetHubWestendReceiver::get(),
			TOKEN_AMOUNT,
		));

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::mint_into(
			weth_location().try_into().unwrap(),
			&AssetHubWestendSender::get(),
			TOKEN_AMOUNT,
		));
	});
}
pub fn register_relay_token() {
	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		type RuntimeOrigin = <BridgeHubWestend as Chain>::RuntimeOrigin;

		// Register WND on BH
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
	});
}

#[test]
fn send_weth_from_asset_hub_to_ethereum() {
	fund_sovereign();

	register_weth();

	AssetHubWestend::execute_with(|| {
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		let local_fee_asset =
			Asset { id: AssetId(Location::parent()), fun: Fungible(LOCAL_FEE_AMOUNT_IN_DOT) };

		let remote_fee_asset =
			Asset { id: AssetId(weth_location()), fun: Fungible(REMOTE_FEE_AMOUNT_IN_WETH) };

		let reserve_asset = Asset {
			id: AssetId(weth_location()),
			fun: Fungible(TOKEN_AMOUNT - REMOTE_FEE_AMOUNT_IN_WETH),
		};

		let assets = vec![
			Asset { id: weth_location().into(), fun: Fungible(TOKEN_AMOUNT) },
			local_fee_asset.clone(),
		];

		let xcm = VersionedXcm::from(Xcm(vec![
			WithdrawAsset(assets.clone().into()),
			PayFees { asset: local_fee_asset.clone() },
			InitiateTransfer {
				destination: destination(),
				remote_fees: Some(AssetTransferFilter::ReserveWithdraw(Definite(
					remote_fee_asset.clone().into(),
				))),
				preserve_origin: true,
				assets: vec![AssetTransferFilter::ReserveWithdraw(Definite(
					reserve_asset.clone().into(),
				))],
				remote_xcm: Xcm(vec![DepositAsset {
					assets: Wild(AllCounted(2)),
					beneficiary: beneficiary(),
				}]),
			},
		]));

		// Send the Weth back to Ethereum
		<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::execute(
			RuntimeOrigin::signed(AssetHubWestendReceiver::get()),
			bx!(xcm),
			Weight::from(EXECUTION_WEIGHT),
		)
		.unwrap();
	});

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		// Check that the Ethereum message was queue in the Outbound Queue
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::EthereumOutboundQueueV2(snowbridge_pallet_outbound_queue_v2::Event::MessageQueued{ .. }) => {},]
		);
	});
}

#[test]
fn transfer_relay_token() {
	let ethereum_sovereign: AccountId =
		EthereumLocationsConverterFor::<[u8; 32]>::convert_location(&destination())
			.unwrap()
			.into();

	fund_sovereign();

	register_weth();

	register_relay_token();

	// Send token to Ethereum
	AssetHubWestend::execute_with(|| {
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		let local_fee_asset =
			Asset { id: AssetId(Location::parent()), fun: Fungible(LOCAL_FEE_AMOUNT_IN_DOT) };
		let remote_fee_asset =
			Asset { id: AssetId(weth_location()), fun: Fungible(REMOTE_FEE_AMOUNT_IN_WETH) };

		let assets = vec![
			Asset {
				id: AssetId(Location::parent()),
				fun: Fungible(TOKEN_AMOUNT + LOCAL_FEE_AMOUNT_IN_DOT),
			},
			remote_fee_asset.clone(),
		];

		let xcm = VersionedXcm::from(Xcm(vec![
			WithdrawAsset(assets.clone().into()),
			PayFees { asset: local_fee_asset.clone() },
			InitiateTransfer {
				destination: destination(),
				remote_fees: Some(AssetTransferFilter::ReserveWithdraw(Definite(
					remote_fee_asset.clone().into(),
				))),
				preserve_origin: true,
				assets: vec![AssetTransferFilter::ReserveDeposit(Definite(
					Asset { id: AssetId(Location::parent()), fun: Fungible(TOKEN_AMOUNT) }.into(),
				))],
				remote_xcm: Xcm(vec![DepositAsset {
					assets: Wild(AllCounted(2)),
					beneficiary: beneficiary(),
				}]),
			},
		]));

		// Send DOT to Ethereum
		<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::execute(
			RuntimeOrigin::signed(AssetHubWestendSender::get()),
			bx!(xcm),
			Weight::from(EXECUTION_WEIGHT),
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

		// Check that the Ethereum message was queue in the Outbound Queue
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::EthereumOutboundQueueV2(snowbridge_pallet_outbound_queue_v2::Event::MessageQueued{ .. }) => {},]
		);
	});
}

#[test]
fn send_weth_and_dot_from_asset_hub_to_ethereum() {
	fund_sovereign();

	register_weth();

	register_relay_token();

	AssetHubWestend::execute_with(|| {
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		let local_fee_asset =
			Asset { id: AssetId(Location::parent()), fun: Fungible(LOCAL_FEE_AMOUNT_IN_DOT) };
		let remote_fee_asset =
			Asset { id: AssetId(weth_location()), fun: Fungible(REMOTE_FEE_AMOUNT_IN_WETH) };

		let reserve_asset = Asset {
			id: AssetId(weth_location()),
			fun: Fungible(TOKEN_AMOUNT - REMOTE_FEE_AMOUNT_IN_WETH),
		};

		let weth_asset = Asset { id: weth_location().into(), fun: Fungible(TOKEN_AMOUNT) };

		let dot_asset = Asset { id: AssetId(Location::parent()), fun: Fungible(TOKEN_AMOUNT) };

		let assets = vec![weth_asset, dot_asset.clone(), local_fee_asset.clone()];

		let xcms = VersionedXcm::from(Xcm(vec![
			WithdrawAsset(assets.clone().into()),
			PayFees { asset: local_fee_asset.clone() },
			InitiateTransfer {
				destination: destination(),
				remote_fees: Some(AssetTransferFilter::ReserveWithdraw(Definite(
					remote_fee_asset.clone().into(),
				))),
				preserve_origin: true,
				assets: vec![
					AssetTransferFilter::ReserveWithdraw(Definite(reserve_asset.clone().into())),
					AssetTransferFilter::ReserveDeposit(Definite(dot_asset.into())),
				],
				remote_xcm: Xcm(vec![DepositAsset {
					assets: Wild(All),
					beneficiary: beneficiary(),
				}]),
			},
		]));

		<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::execute(
			RuntimeOrigin::signed(AssetHubWestendReceiver::get()),
			bx!(xcms),
			Weight::from(EXECUTION_WEIGHT),
		)
		.unwrap();
	});

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		// Check that Ethereum message was queue in the Outbound Queue
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::EthereumOutboundQueueV2(snowbridge_pallet_outbound_queue_v2::Event::MessageQueued{ .. }) => {},]
		);
	});
}

#[test]
fn create_agent() {
	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		type RuntimeOrigin = <BridgeHubWestend as Chain>::RuntimeOrigin;

		let location = Location::new(
			1,
			[
				Parachain(AssetHubWestend::para_id().into()),
				AccountId32 { network: None, id: AssetHubWestendSender::get().into() },
			],
		);

		<BridgeHubWestend as BridgeHubWestendPallet>::EthereumSystem::force_create_agent(
			RuntimeOrigin::root(),
			bx!(location.into()),
		);
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::EthereumSystem(snowbridge_pallet_system::Event::CreateAgent{ .. }) => {},]
		);
	});
}

#[test]
fn transact_with_agent() {
	let weth_asset_location: Location =
		(Parent, Parent, EthereumNetwork::get(), AccountKey20 { network: None, key: WETH }).into();

	fund_sovereign();

	register_weth();

	BridgeHubWestend::execute_with(|| {});

	AssetHubWestend::execute_with(|| {
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		let local_fee_asset =
			Asset { id: AssetId(Location::parent()), fun: Fungible(LOCAL_FEE_AMOUNT_IN_DOT) };

		let remote_fee_asset = Asset {
			id: AssetId(weth_asset_location.clone()),
			fun: Fungible(REMOTE_FEE_AMOUNT_IN_WETH),
		};
		let reserve_asset = Asset {
			id: AssetId(weth_asset_location.clone()),
			fun: Fungible(TOKEN_AMOUNT - REMOTE_FEE_AMOUNT_IN_WETH),
		};

		let assets = vec![
			Asset { id: weth_asset_location.clone().into(), fun: Fungible(TOKEN_AMOUNT) },
			local_fee_asset.clone(),
		];

		let beneficiary =
			Location::new(0, [AccountKey20 { network: None, key: AGENT_ADDRESS.into() }]);

		let transact_info = TransactInfo {
			target: Default::default(),
			data: vec![],
			gas_limit: 40000,
			// value should be less than the transfer amount, require validation on BH Exporter
			value: 4 * (TOKEN_AMOUNT - REMOTE_FEE_AMOUNT_IN_WETH) / 5,
		};

		let xcms = VersionedXcm::from(Xcm(vec![
			WithdrawAsset(assets.clone().into()),
			PayFees { asset: local_fee_asset.clone() },
			InitiateTransfer {
				destination: destination(),
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

		<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::execute(
			RuntimeOrigin::signed(AssetHubWestendReceiver::get()),
			bx!(xcms),
			Weight::from(EXECUTION_WEIGHT),
		)
		.unwrap();
	});

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		// Check that Ethereum message was queue in the Outbound Queue
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::EthereumOutboundQueueV2(snowbridge_pallet_outbound_queue_v2::Event::MessageQueued{ .. }) => {},]
		);
	});
}
