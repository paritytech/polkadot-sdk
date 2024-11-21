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
use hex_literal::hex;
use snowbridge_core::rewards::RewardLedger;
use snowbridge_router_primitives::inbound::{Command, Destination, MessageV1, VersionedMessage};
use testnet_parachains_constants::westend::snowbridge::EthereumNetwork;

const INITIAL_FUND: u128 = 5_000_000_000_000;
pub const ETH: u128 = 1_000_000_000_000_000_000;

#[test]
fn claim_rewards_works() {
	let weth: [u8; 20] = hex!("fff9976782d46cc05630d1f6ebab18b2324d6b14");
	let assethub_location = BridgeHubWestend::sibling_location_of(AssetHubWestend::para_id());
	let assethub_sovereign = BridgeHubWestend::sovereign_account_id_of(assethub_location);
	let weth_asset_location: Location =
		(Parent, Parent, EthereumNetwork::get(), AccountKey20 { network: None, key: weth }).into();

	let relayer = BridgeHubWestendSender::get();

	BridgeHubWestend::fund_accounts(vec![
		(assethub_sovereign.clone(), INITIAL_FUND),
		(relayer.clone(), INITIAL_FUND),
	]);

	AssetHubWestend::execute_with(|| {
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::force_create(
			RuntimeOrigin::root(),
			weth_asset_location.clone().try_into().unwrap(),
			assethub_sovereign.clone().into(),
			true,
			1,
		));

		assert!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::asset_exists(
			weth_asset_location.clone().try_into().unwrap(),
		));
	});

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		type RuntimeOrigin = <BridgeHubWestend as Chain>::RuntimeOrigin;

		let reward_address = AssetHubWestendReceiver::get();
		type BridgeRelayers = <BridgeHubWestend as BridgeHubWestendPallet>::BridgeRelayers;
		assert_ok!(BridgeRelayers::deposit(relayer.clone().into(), 2 * ETH));

		// Check that the message was sent
		assert_expected_events!(
			BridgeHubWestend,
			vec![
				RuntimeEvent::BridgeRelayers(pallet_bridge_relayers::Event::RewardDeposited { .. }) => {},
			]
		);

		let relayer_location = Location::new(
			1,
			[Parachain(1000), Junction::AccountId32 { id: reward_address.into(), network: None }],
		);
		let result =
			BridgeRelayers::claim(RuntimeOrigin::signed(relayer.clone()), relayer_location.clone());
		assert_ok!(result);

		let events = BridgeHubWestend::events();
		assert!(
			events.iter().any(|event| matches!(
				event,
				RuntimeEvent::BridgeRelayers(pallet_bridge_relayers::Event::RewardClaimed { account_id, deposit_location, value, })
					if *account_id == relayer && *deposit_location == relayer_location && *value > 1 *ETH,
			)),
			"RewardClaimed event with correct fields."
		);
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubWestend,
			vec![RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { .. }) => {},]
		);
	})
}
