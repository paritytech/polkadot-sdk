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

mod asset_transfers;
mod send_xcm;
mod teleport;

mod snowbridge {
	pub const CHAIN_ID: u64 = 11155111;
	pub const WETH: [u8; 20] = hex_literal::hex!("87d1f7fdfEe7f651FaBc8bFCB6E086C278b77A7d");
}

pub(crate) fn asset_hub_rococo_location() -> Location {
	Location::new(2, [GlobalConsensus(Rococo), Parachain(AssetHubRococo::para_id().into())])
}

pub(crate) fn bridge_hub_rococo_location() -> Location {
	Location::new(2, [GlobalConsensus(Rococo), Parachain(BridgeHubRococo::para_id().into())])
}

// WND and wWND
pub(crate) fn wnd_at_ah_westend() -> Location {
	Parent.into()
}
pub(crate) fn bridged_wnd_at_ah_rococo() -> Location {
	Location::new(2, [GlobalConsensus(Westend)])
}

// wROC
pub(crate) fn bridged_roc_at_ah_westend() -> Location {
	Location::new(2, [GlobalConsensus(Rococo)])
}

// USDT and wUSDT
pub(crate) fn usdt_at_ah_rococo() -> Location {
	Location::new(0, [PalletInstance(ASSETS_PALLET_ID), GeneralIndex(USDT_ID.into())])
}
pub(crate) fn bridged_usdt_at_ah_westend() -> Location {
	Location::new(
		2,
		[
			GlobalConsensus(Rococo),
			Parachain(AssetHubRococo::para_id().into()),
			PalletInstance(ASSETS_PALLET_ID),
			GeneralIndex(USDT_ID.into()),
		],
	)
}

// wETH has same relative location on both Rococo and Westend AssetHubs
pub(crate) fn weth_at_asset_hubs() -> Location {
	Location::new(
		2,
		[
			GlobalConsensus(Ethereum { chain_id: snowbridge::CHAIN_ID }),
			AccountKey20 { network: None, key: snowbridge::WETH },
		],
	)
}

pub(crate) fn create_foreign_on_ah_rococo(id: v3::Location, sufficient: bool) {
	let owner = AssetHubRococo::account_id_of(ALICE);
	AssetHubRococo::force_create_foreign_asset(id, owner, sufficient, ASSET_MIN_BALANCE, vec![]);
}

pub(crate) fn create_foreign_on_ah_westend(
	id: v3::Location,
	sufficient: bool,
	prefund_accounts: Vec<(AccountId, u128)>,
) {
	let owner = AssetHubWestend::account_id_of(ALICE);
	let min = ASSET_MIN_BALANCE;
	AssetHubWestend::force_create_foreign_asset(id, owner, sufficient, min, prefund_accounts);
}

pub(crate) fn foreign_balance_on_ah_rococo(id: v3::Location, who: &AccountId) -> u128 {
	AssetHubRococo::execute_with(|| {
		type Assets = <AssetHubRococo as AssetHubRococoPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(id, who)
	})
}
pub(crate) fn foreign_balance_on_ah_westend(id: v3::Location, who: &AccountId) -> u128 {
	AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(id, who)
	})
}

// set up pool
pub(crate) fn set_up_pool_with_roc_on_ah_rococo(asset: v3::Location, is_foreign: bool) {
	let roc: v3::Location = v3::Parent.into();
	AssetHubRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;
		let owner = AssetHubRococoSender::get();
		let signed_owner = <AssetHubRococo as Chain>::RuntimeOrigin::signed(owner.clone());

		if is_foreign {
			assert_ok!(<AssetHubRococo as AssetHubRococoPallet>::ForeignAssets::mint(
				signed_owner.clone(),
				asset.into(),
				owner.clone().into(),
				3_000_000_000_000,
			));
		} else {
			let asset_id = match asset.interior.split_last() {
				(_, Some(v3::Junction::GeneralIndex(id))) => id as u32,
				_ => unreachable!(),
			};
			assert_ok!(<AssetHubRococo as AssetHubRococoPallet>::Assets::mint(
				signed_owner.clone(),
				asset_id.into(),
				owner.clone().into(),
				3_000_000_000_000,
			));
		}
		assert_ok!(<AssetHubRococo as AssetHubRococoPallet>::AssetConversion::create_pool(
			signed_owner.clone(),
			Box::new(roc),
			Box::new(asset),
		));
		assert_expected_events!(
			AssetHubRococo,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::PoolCreated { .. }) => {},
			]
		);
		assert_ok!(<AssetHubRococo as AssetHubRococoPallet>::AssetConversion::add_liquidity(
			signed_owner.clone(),
			Box::new(roc),
			Box::new(asset),
			1_000_000_000_000,
			2_000_000_000_000,
			1,
			1,
			owner.into()
		));
		assert_expected_events!(
			AssetHubRococo,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::LiquidityAdded {..}) => {},
			]
		);
	});
}

pub(crate) fn send_assets_from_asset_hub_westend(
	destination: Location,
	assets: Assets,
	fee_idx: u32,
) -> DispatchResult {
	let signed_origin =
		<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get().into());
	let beneficiary: Location =
		AccountId32Junction { network: None, id: AssetHubRococoReceiver::get().into() }.into();

	AssetHubWestend::execute_with(|| {
		<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::limited_reserve_transfer_assets(
			signed_origin,
			bx!(destination.into()),
			bx!(beneficiary.into()),
			bx!(assets.into()),
			fee_idx,
			WeightLimit::Unlimited,
		)
	})
}

pub(crate) fn assert_bridge_hub_westend_message_accepted(expected_processed: bool) {
	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;

		if expected_processed {
			assert_expected_events!(
				BridgeHubWestend,
				vec![
					// pay for bridge fees
					RuntimeEvent::Balances(pallet_balances::Event::Burned { .. }) => {},
					// message exported
					RuntimeEvent::BridgeRococoMessages(
						pallet_bridge_messages::Event::MessageAccepted { .. }
					) => {},
					// message processed successfully
					RuntimeEvent::MessageQueue(
						pallet_message_queue::Event::Processed { success: true, .. }
					) => {},
				]
			);
		} else {
			assert_expected_events!(
				BridgeHubWestend,
				vec![
					RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {
						success: false,
						..
					}) => {},
				]
			);
		}
	});
}

pub(crate) fn assert_bridge_hub_rococo_message_received() {
	BridgeHubRococo::execute_with(|| {
		type RuntimeEvent = <BridgeHubRococo as Chain>::RuntimeEvent;
		assert_expected_events!(
			BridgeHubRococo,
			vec![
				// message sent to destination
				RuntimeEvent::XcmpQueue(
					cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }
				) => {},
			]
		);
	})
}
