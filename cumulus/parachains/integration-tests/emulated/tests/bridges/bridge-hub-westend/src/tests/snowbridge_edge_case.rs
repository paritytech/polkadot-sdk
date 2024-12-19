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
		snowbridge::{CHAIN_ID, WETH},
		snowbridge_common::*,
	},
};
use bridge_hub_westend_runtime::xcm_config::LocationToAccountId;
use snowbridge_core::AssetMetadata;
use snowbridge_pallet_system::Error;
use xcm_executor::traits::ConvertLocation;

// The user origin should be banned in ethereum_blob_exporter with error logs
// xcm::ethereum_blob_exporter: could not get parachain id from universal source
// 'X2([Parachain(1000), AccountId32 {...}])'
#[test]
fn user_export_message_from_ah_directly_will_fail() {
	fund_on_bh();
	register_weth_on_ah();
	fund_on_ah();
	create_pools_on_ah();

	let sov_account_for_sender = LocationToAccountId::convert_location(&Location::new(
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
	BridgeHubWestend::fund_accounts(vec![(sov_account_for_sender, INITIAL_FUND)]);

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

// ENA is not allowed to be registered as PNA
#[test]
fn test_register_ena_on_bh_will_fail() {
	BridgeHubWestend::execute_with(|| {
		type RuntimeOrigin = <BridgeHubWestend as Chain>::RuntimeOrigin;
		type Runtime = <BridgeHubWestend as Chain>::Runtime;

		assert_ok!(<BridgeHubWestend as BridgeHubWestendPallet>::Balances::force_set_balance(
			RuntimeOrigin::root(),
			sp_runtime::MultiAddress::Id(BridgeHubWestendSender::get()),
			INITIAL_FUND * 10,
		));

		assert_err!(
			<BridgeHubWestend as BridgeHubWestendPallet>::EthereumSystem::register_token(
				RuntimeOrigin::root(),
				Box::new(VersionedLocation::from(weth_location())),
				AssetMetadata {
					name: "weth".as_bytes().to_vec().try_into().unwrap(),
					symbol: "weth".as_bytes().to_vec().try_into().unwrap(),
					decimals: 18,
				},
			),
			Error::<Runtime>::LocationConversionFailed
		);
	});
}
