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
use asset_hub_westend_runtime::xcm_config::bridging::to_ethereum::DefaultBridgeHubEthereumBaseFee;
use bridge_hub_westend_runtime::EthereumInboundQueue;
use codec::{Decode, Encode};
use emulated_integration_tests_common::RESERVABLE_ASSET_ID;
use frame_support::pallet_prelude::TypeInfo;
use hex_literal::hex;
use rococo_westend_system_emulated_network::asset_hub_westend_emulated_chain::genesis::AssetHubWestendAssetOwner;
use snowbridge_core::{outbound::OperatingMode, AssetMetadata, TokenIdOf};
use snowbridge_router_primitives::inbound::{
	v1::{Command, Destination, MessageV1, VersionedMessage},
	GlobalConsensusEthereumConvertsFor,
};
use sp_core::H256;
use testnet_parachains_constants::westend::snowbridge::EthereumNetwork;
use xcm_executor::traits::ConvertLocation;

const INITIAL_FUND: u128 = 5_000_000_000_000;
pub const CHAIN_ID: u64 = 11155111;
pub const WETH: [u8; 20] = hex!("87d1f7fdfEe7f651FaBc8bFCB6E086C278b77A7d");
const ETHEREUM_DESTINATION_ADDRESS: [u8; 20] = hex!("44a57ee2f2FCcb85FDa2B0B18EBD0D8D2333700e");
const XCM_FEE: u128 = 100_000_000_000;
const TOKEN_AMOUNT: u128 = 100_000_000_000;

#[test]
fn send_weth_from_asset_hub_to_ethereum_by_executing_raw_xcm() {
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

		let local_fee_amount = 80_000_000_000;
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

		// Internal xcm of InitiateReserveWithdraw, WithdrawAssets + ClearOrigin instructions will
		// be appended to the front of the list by the xcm executor
		let xcm_on_bh = Xcm(vec![
			BuyExecution { fees: remote_fee_asset.clone(), weight_limit: Unlimited },
			// ExpectAsset as a workaround before XCMv5 to differ Route V1 and V2
			ExpectAsset(vec![remote_fee_asset.clone()].into()),
			DepositAsset { assets: Wild(AllCounted(2)), beneficiary },
		]);

		let xcms = VersionedXcm::from(Xcm(vec![
			WithdrawAsset(assets.clone().into()),
			// BuyExecution { fees: local_fee_asset.clone(), weight_limit: Unlimited },
			SetFeesMode { jit_withdraw: true },
			InitiateReserveWithdraw {
				assets: Definite(reserve_asset.clone().into()),
				// with reserve set to Ethereum destination, the ExportMessage will
				// be appended to the front of the list by the SovereignPaidRemoteExporter
				reserve: destination,
				xcm: xcm_on_bh,
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
		use bridge_hub_westend_runtime::xcm_config::TreasuryAccount;
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
