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
use bridge_hub_westend_runtime::xcm_config::LocationToAccountId;
use frame_support::traits::fungibles::Mutate;
use snowbridge_router_primitives::inbound::EthereumLocationsConverterFor;
use testnet_parachains_constants::westend::snowbridge::EthereumNetwork;
use xcm_executor::traits::ConvertLocation;

use crate::tests::snowbridge::{CHAIN_ID, ETHEREUM_DESTINATION_ADDRESS, WETH};

const INITIAL_FUND: u128 = 5_000_000_000_000;
const TOKEN_AMOUNT: u128 = 100_000_000_000;

pub fn bridge_hub() -> Location {
	Location::new(1, Parachain(BridgeHubWestend::para_id().into()))
}

pub fn beneficiary() -> Location {
	Location::new(0, [AccountKey20 { network: None, key: ETHEREUM_DESTINATION_ADDRESS.into() }])
}

pub fn ethereum() -> Location {
	Location::new(2, [GlobalConsensus(EthereumNetwork::get())])
}

pub fn weth_location() -> Location {
	Location::new(
		2,
		[GlobalConsensus(EthereumNetwork::get()), AccountKey20 { network: None, key: WETH }],
	)
}

pub fn fund_on_bh() {
	let asset_hub_sovereign = BridgeHubWestend::sovereign_account_id_of(Location::new(
		1,
		[Parachain(AssetHubWestend::para_id().into())],
	));
	BridgeHubWestend::fund_accounts(vec![(asset_hub_sovereign.clone(), INITIAL_FUND)]);
}

pub fn fund_on_ah() {
	AssetHubWestend::fund_accounts(vec![
	  (AssetHubWestendSender::get(), INITIAL_FUND),
	  (AssetHubWestendReceiver::get(), INITIAL_FUND),
	]);

	AssetHubWestend::execute_with(|| {
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::mint_into(
			weth_location().try_into().unwrap(),
			&AssetHubWestendReceiver::get(),
			INITIAL_FUND,
		));
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::mint_into(
			weth_location().try_into().unwrap(),
			&AssetHubWestendSender::get(),
			INITIAL_FUND,
		));
	});

	let ethereum_sovereign: AccountId =
		EthereumLocationsConverterFor::<[u8; 32]>::convert_location(&Location::new(
			2,
			[GlobalConsensus(EthereumNetwork::get())],
		))
		.unwrap()
		.into();
	AssetHubWestend::fund_accounts(vec![(ethereum_sovereign.clone(), INITIAL_FUND)]);
}

pub fn register_weth_on_ah() {
	let ethereum_sovereign: AccountId =
		EthereumLocationsConverterFor::<[u8; 32]>::convert_location(&Location::new(
			2,
			[GlobalConsensus(EthereumNetwork::get())],
		))
		.unwrap()
		.into();

	AssetHubWestend::execute_with(|| {
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::force_create(
			RuntimeOrigin::root(),
			weth_location().try_into().unwrap(),
			ethereum_sovereign.clone().into(),
			true,
			1,
		));

		assert!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::asset_exists(
			weth_location().try_into().unwrap(),
		));
	});
}

#[test]
fn user_send_message_bypass_exporter_on_ah_will_fail() {
	let sov_account_for_assethub_sender = LocationToAccountId::convert_location(&Location::new(
		1,
		[
			Parachain(AssetHubWestend::para_id().into()),
			AccountId32 {
				network: Some(ByGenesis(WESTEND_GENESIS_HASH)),
				id: AssetHubWestendSender::get().into(),
			},
		],
	))
	.unwrap();
	BridgeHubWestend::fund_accounts(vec![(sov_account_for_assethub_sender, INITIAL_FUND)]);

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		let local_fee_asset =
			Asset { id: AssetId(Location::parent()), fun: Fungible(1_000_000_000_000) };

		let weth_location_reanchored =
			Location::new(0, [AccountKey20 { network: None, key: WETH.into() }]);

		let weth_asset = Asset {
			id: AssetId(weth_location_reanchored.clone()),
			fun: Fungible(TOKEN_AMOUNT * 1_000_000_000),
		};

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::send(
			RuntimeOrigin::signed(AssetHubWestendSender::get()),
			bx!(VersionedLocation::from(bridge_hub())),
			bx!(VersionedXcm::from(Xcm(vec![
				WithdrawAsset(local_fee_asset.clone().into()),
				BuyExecution { fees: local_fee_asset.clone(), weight_limit: Unlimited },
				ExportMessage {
					network: Ethereum { chain_id: CHAIN_ID },
					destination: Here,
					xcm: Xcm(vec![
						WithdrawAsset(weth_asset.clone().into()),
						DepositAsset { assets: Wild(All), beneficiary: beneficiary() },
						SetTopic([0; 32]),
					]),
				},
			]))),
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![RuntimeEvent::PolkadotXcm(pallet_xcm::Event::Sent{ .. }) => {},]
		);
	});

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed{ success:false, .. }) => {},]
		);
	});
}

#[test]
fn user_exploit_with_arbitrary_message_will_fail() {
	fund_on_bh();
	register_weth_on_ah();
	fund_on_ah();
	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		let remote_fee_asset_location: Location = Location::new(
			2,
			[EthereumNetwork::get().into(), AccountKey20 { network: None, key: WETH }],
		)
		.into();

		let remote_fee_asset: Asset = (remote_fee_asset_location.clone(), 1).into();

		let assets = VersionedAssets::from(vec![remote_fee_asset]);

		let exploited_weth = Asset {
			id: AssetId(Location::new(0, [AccountKey20 { network: None, key: WETH.into() }])),
			// A big amount without burning
			fun: Fungible(TOKEN_AMOUNT * 1_000_000_000),
		};

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::transfer_assets_using_type_and_then(
			RuntimeOrigin::signed(AssetHubWestendSender::get()),
			bx!(VersionedLocation::from(ethereum())),
			bx!(assets),
			bx!(TransferType::DestinationReserve),
			bx!(VersionedAssetId::from(remote_fee_asset_location.clone())),
			bx!(TransferType::DestinationReserve),
			bx!(VersionedXcm::from(Xcm(vec![
			// Instructions inner are user provided and untrustworthy/dangerous!
			// Currently it depends on EthereumBlobExporter on BH to check the message is legal
			// and convert to Ethereum command, should be very careful to handle that.
			// Or we may move the security check ahead to AH to fail earlier if possible.
				WithdrawAsset(exploited_weth.clone().into()),
				DepositAsset { assets: Wild(All), beneficiary: beneficiary() },
				SetTopic([0; 32]),
			]))),
			Unlimited
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![RuntimeEvent::PolkadotXcm(pallet_xcm::Event::Sent{ .. }) => {},]
		);
	});

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed{ success:false, .. }) => {},]
		);
	});
}
