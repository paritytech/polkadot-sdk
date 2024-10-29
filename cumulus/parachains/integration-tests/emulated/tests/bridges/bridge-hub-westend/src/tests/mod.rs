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
mod claim_assets;
mod register_bridged_assets;
mod send_xcm;
mod snowbridge;
mod snowbridge_v2;
mod teleport;
mod transact;

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

// ROC and wROC
pub(crate) fn roc_at_ah_rococo() -> Location {
	Parent.into()
}
pub(crate) fn bridged_roc_at_ah_westend() -> Location {
	Location::new(2, [GlobalConsensus(Rococo)])
}

// USDT and wUSDT
pub(crate) fn usdt_at_ah_westend() -> Location {
	Location::new(0, [PalletInstance(ASSETS_PALLET_ID), GeneralIndex(USDT_ID.into())])
}
pub(crate) fn bridged_usdt_at_ah_rococo() -> Location {
	Location::new(
		2,
		[
			GlobalConsensus(Westend),
			Parachain(AssetHubWestend::para_id().into()),
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

pub(crate) fn create_foreign_on_ah_rococo(id: v5::Location, sufficient: bool) {
	let owner = AssetHubRococo::account_id_of(ALICE);
	AssetHubRococo::force_create_foreign_asset(id, owner, sufficient, ASSET_MIN_BALANCE, vec![]);
}

pub(crate) fn create_foreign_on_ah_westend(
	id: v5::Location,
	sufficient: bool,
	prefund_accounts: Vec<(AccountId, u128)>,
) {
	let owner = AssetHubWestend::account_id_of(ALICE);
	let min = ASSET_MIN_BALANCE;
	AssetHubWestend::force_create_foreign_asset(id, owner, sufficient, min, prefund_accounts);
}

pub(crate) fn foreign_balance_on_ah_rococo(id: v5::Location, who: &AccountId) -> u128 {
	AssetHubRococo::execute_with(|| {
		type Assets = <AssetHubRococo as AssetHubRococoPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(id, who)
	})
}
pub(crate) fn foreign_balance_on_ah_westend(id: v5::Location, who: &AccountId) -> u128 {
	AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(id, who)
	})
}

/// note: $asset needs to be prefunded outside this function
#[macro_export]
macro_rules! create_pool_with_native_on {
	( $chain:ident, $asset:expr, $is_foreign:expr, $asset_owner:expr ) => {
		emulated_integration_tests_common::impls::paste::paste! {
			<$chain>::execute_with(|| {
				type RuntimeEvent = <$chain as Chain>::RuntimeEvent;
				let owner = $asset_owner;
				let signed_owner = <$chain as Chain>::RuntimeOrigin::signed(owner.clone());
				let native_asset: Location = Parent.into();

				if $is_foreign {
					assert_ok!(<$chain as [<$chain Pallet>]>::ForeignAssets::mint(
						signed_owner.clone(),
						$asset.clone().into(),
						owner.clone().into(),
						10_000_000_000_000, // For it to have more than enough.
					));
				} else {
					let asset_id = match $asset.interior.last() {
						Some(GeneralIndex(id)) => *id as u32,
						_ => unreachable!(),
					};
					assert_ok!(<$chain as [<$chain Pallet>]>::Assets::mint(
						signed_owner.clone(),
						asset_id.into(),
						owner.clone().into(),
						10_000_000_000_000, // For it to have more than enough.
					));
				}

				assert_ok!(<$chain as [<$chain Pallet>]>::AssetConversion::create_pool(
					signed_owner.clone(),
					Box::new(native_asset.clone()),
					Box::new($asset.clone()),
				));

				assert_expected_events!(
					$chain,
					vec![
						RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::PoolCreated { .. }) => {},
					]
				);

				assert_ok!(<$chain as [<$chain Pallet>]>::AssetConversion::add_liquidity(
					signed_owner,
					Box::new(native_asset),
					Box::new($asset),
					1_000_000_000_000,
					2_000_000_000_000, // $asset is worth half of native_asset
					0,
					0,
					owner.into()
				));

				assert_expected_events!(
					$chain,
					vec![
						RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::LiquidityAdded { .. }) => {},
					]
				);
			});
		}
	};
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

pub(crate) fn open_bridge_between_asset_hub_rococo_and_asset_hub_westend() {
	use testnet_parachains_constants::{
		rococo::currency::UNITS as ROC, westend::currency::UNITS as WND,
	};

	// open AHR -> AHW
	BridgeHubRococo::fund_para_sovereign(AssetHubRococo::para_id(), ROC * 5);
	AssetHubRococo::open_bridge(
		AssetHubRococo::sibling_location_of(BridgeHubRococo::para_id()),
		[GlobalConsensus(Westend), Parachain(AssetHubWestend::para_id().into())].into(),
		Some((
			(roc_at_ah_rococo(), ROC * 1).into(),
			BridgeHubRococo::sovereign_account_id_of(BridgeHubRococo::sibling_location_of(
				AssetHubRococo::para_id(),
			)),
		)),
	);

	// open AHW -> AHR
	BridgeHubWestend::fund_para_sovereign(AssetHubWestend::para_id(), WND * 5);
	AssetHubWestend::open_bridge(
		AssetHubWestend::sibling_location_of(BridgeHubWestend::para_id()),
		[GlobalConsensus(Rococo), Parachain(AssetHubRococo::para_id().into())].into(),
		Some((
			(wnd_at_ah_westend(), WND * 1).into(),
			BridgeHubWestend::sovereign_account_id_of(BridgeHubWestend::sibling_location_of(
				AssetHubWestend::para_id(),
			)),
		)),
	);
}
