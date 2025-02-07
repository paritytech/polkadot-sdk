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

use rococo_westend_system_emulated_network::asset_hub_rococo_emulated_chain::asset_hub_rococo_runtime::xcm_config::bridging::to_westend::EthereumNetwork;
use crate::{imports::*, tests::snowbridge_common::*};
use snowbridge_core::AssetMetadata;
use snowbridge_inbound_queue_primitives::EthereumLocationsConverterFor;
use xcm::latest::AssetTransferFilter;
use xcm_executor::traits::ConvertLocation;

pub(crate) fn asset_hub_westend_location() -> Location {
	Location::new(
		2,
		[
			GlobalConsensus(ByGenesis(WESTEND_GENESIS_HASH)),
			Parachain(AssetHubWestend::para_id().into()),
		],
	)
}
pub(crate) fn bridge_hub_westend_location() -> Location {
	Location::new(
		2,
		[
			GlobalConsensus(ByGenesis(WESTEND_GENESIS_HASH)),
			Parachain(BridgeHubWestend::para_id().into()),
		],
	)
}

// ROC and wROC
pub(crate) fn roc_at_ah_rococo() -> Location {
	Parent.into()
}
pub(crate) fn bridged_roc_at_ah_westend() -> Location {
	Location::new(2, [GlobalConsensus(ByGenesis(ROCOCO_GENESIS_HASH))])
}

pub(crate) fn create_foreign_on_ah_westend(id: xcm::opaque::v5::Location, sufficient: bool) {
	let owner = AssetHubWestend::account_id_of(ALICE);
	AssetHubWestend::force_create_foreign_asset(id, owner, sufficient, ASSET_MIN_BALANCE, vec![]);
}

// set up pool
pub(crate) fn set_up_pool_with_wnd_on_ah_westend(asset: Location, is_foreign: bool) {
	let wnd: Location = Parent.into();
	AssetHubWestend::fund_accounts(vec![(AssetHubWestendSender::get(), INITIAL_FUND)]);
	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		let owner = AssetHubWestendSender::get();
		let signed_owner = <AssetHubWestend as Chain>::RuntimeOrigin::signed(owner.clone());

		if is_foreign {
			assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::mint(
				signed_owner.clone(),
				asset.clone().into(),
				owner.clone().into(),
				8_000_000_000_000,
			));
		} else {
			let asset_id = match asset.interior.last() {
				Some(GeneralIndex(id)) => *id as u32,
				_ => unreachable!(),
			};
			assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::Assets::mint(
				signed_owner.clone(),
				asset_id.into(),
				owner.clone().into(),
				8_000_000_000_000,
			));
		}
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
			6_000_000_000_000,
			6_000_000_000_000,
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

pub(crate) fn assert_bridge_hub_rococo_message_accepted(expected_processed: bool) {
	BridgeHubRococo::execute_with(|| {
		type RuntimeEvent = <BridgeHubRococo as Chain>::RuntimeEvent;

		if expected_processed {
			assert_expected_events!(
				BridgeHubRococo,
				vec![
					// pay for bridge fees
					RuntimeEvent::Balances(pallet_balances::Event::Burned { .. }) => {},
					// message exported
					RuntimeEvent::BridgeWestendMessages(
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
				BridgeHubRococo,
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

pub(crate) fn assert_bridge_hub_westend_message_received() {
	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			BridgeHubWestend,
			vec![
				// message sent to destination
				RuntimeEvent::XcmpQueue(
					cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }
				) => {},
			]
		);
	})
}

pub fn register_roc_on_bh() {
	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		type RuntimeOrigin = <BridgeHubWestend as Chain>::RuntimeOrigin;

		// Register ROC on BH
		assert_ok!(<BridgeHubWestend as BridgeHubWestendPallet>::EthereumSystem::register_token(
			RuntimeOrigin::root(),
			Box::new(VersionedLocation::from(bridged_roc_at_ah_westend())),
			AssetMetadata {
				name: "roc".as_bytes().to_vec().try_into().unwrap(),
				symbol: "roc".as_bytes().to_vec().try_into().unwrap(),
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
fn send_roc_from_asset_hub_rococo_to_ethereum() {
	let amount: u128 = 1_000_000_000_000_000;
	let fee_amount: u128 = 80_000_000_000_000;
	let sender = AssetHubRococoSender::get();
	let roc_at_asset_hub_rococo = roc_at_ah_rococo();
	let bridged_roc_at_asset_hub_westend = bridged_roc_at_ah_westend();

	create_foreign_on_ah_westend(bridged_roc_at_asset_hub_westend.clone(), true);
	set_up_pool_with_wnd_on_ah_westend(bridged_roc_at_asset_hub_westend.clone(), true);
	AssetHubWestend::execute_with(|| {
		let previous_owner = EthereumLocationsConverterFor::<[u8; 32]>::convert_location(
			&Location::new(2, [GlobalConsensus(EthereumNetwork::get())]),
		)
		.unwrap()
		.into();
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::start_destroy(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(previous_owner),
			ethereum()
		));
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::finish_destroy(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestend::account_id_of(
				ALICE
			)),
			ethereum()
		));
	});
	create_foreign_on_ah_westend(ethereum(), true);
	set_up_pool_with_wnd_on_ah_westend(ethereum(), true);
	BridgeHubRococo::fund_para_sovereign(AssetHubRococo::para_id(), 50_000_000_000_000_000);
	AssetHubRococo::fund_accounts(vec![(AssetHubRococoSender::get(), 50_000_000_000_000_000)]);
	fund_on_bh();
	register_roc_on_bh();

	// set XCM versions
	AssetHubRococo::force_xcm_version(asset_hub_westend_location(), XCM_VERSION);
	BridgeHubRococo::force_xcm_version(bridge_hub_westend_location(), XCM_VERSION);

	// send ROCs, use them for fees
	let local_fee_asset: Asset = (roc_at_asset_hub_rococo.clone(), fee_amount).into();
	let remote_fee_on_westend: Asset = (roc_at_asset_hub_rococo.clone(), fee_amount).into();
	let assets: Assets = (roc_at_asset_hub_rococo.clone(), amount).into();
	let reserved_asset_on_westend: Asset =
		(roc_at_asset_hub_rococo.clone(), amount - fee_amount * 2).into();
	let reserved_asset_on_westend_reanchored: Asset =
		(bridged_roc_at_asset_hub_westend.clone(), (amount - fee_amount * 2) / 2).into();

	let ether_fee_amount: u128 = 4_000_000;

	let xcm = VersionedXcm::from(Xcm(vec![
		WithdrawAsset(assets.clone().into()),
		PayFees { asset: local_fee_asset.clone() },
		InitiateTransfer {
			destination: asset_hub_westend_location(),
			remote_fees: Some(AssetTransferFilter::ReserveDeposit(Definite(
				remote_fee_on_westend.clone().into(),
			))),
			preserve_origin: true,
			assets: vec![AssetTransferFilter::ReserveDeposit(Definite(
				reserved_asset_on_westend.clone().into(),
			))],
			remote_xcm: Xcm(vec![
				// swap from roc to wnd
				ExchangeAsset {
					give: Definite(reserved_asset_on_westend_reanchored.clone().into()),
					want: (Parent, 4_000_000_000_000_u128).into(),
					maximal: true,
				},
				// swap some wnd to ether
				ExchangeAsset {
					give: Definite((Parent, 40_000_000_000_u128).into()),
					want: (ethereum(), ether_fee_amount).into(),
					maximal: true,
				},
				PayFees { asset: (Parent, 400_000_000_000_u128).into() },
				InitiateTransfer {
					destination: ethereum(),
					remote_fees: Some(AssetTransferFilter::ReserveWithdraw(Definite(
						Asset { id: AssetId(ethereum()), fun: Fungible(ether_fee_amount) }.into(),
					))),
					preserve_origin: true,
					assets: vec![AssetTransferFilter::ReserveDeposit(Definite(
						reserved_asset_on_westend_reanchored.clone().into(),
					))],
					remote_xcm: Xcm(vec![DepositAsset {
						assets: Wild(All),
						beneficiary: beneficiary(),
					}]),
				},
			]),
		},
	]));

	let _ = AssetHubRococo::execute_with(|| {
		<AssetHubRococo as AssetHubRococoPallet>::PolkadotXcm::execute(
			<AssetHubRococo as Chain>::RuntimeOrigin::signed(sender),
			bx!(xcm),
			Weight::from(EXECUTION_WEIGHT),
		)
	});

	assert_bridge_hub_rococo_message_accepted(true);
	assert_bridge_hub_westend_message_received();

	// verify expected events on final destination
	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubWestend,
			vec![
				// message processed successfully
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
			]
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
