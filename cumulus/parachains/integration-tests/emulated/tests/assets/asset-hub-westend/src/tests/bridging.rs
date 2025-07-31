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

use crate::imports::assert_ok;
pub use codec::Encode;
use emulated_integration_tests_common::{
	xcm_emulator::{bx, Chain, Parachain as Para, TestExt},
	xcm_helpers::xcm_transact_paid_execution,
};
use xcm::{
	latest::{ROCOCO_GENESIS_HASH, WESTEND_GENESIS_HASH},
	prelude::*,
};

// For a bridging we need rococo_westend system and not separated ones (because different Penpal setups).
use rococo_westend_system_emulated_network::{
	asset_hub_rococo_emulated_chain::{
		asset_hub_rococo_runtime::ExistentialDeposit as AssetHubRococoExistentialDeposit,
		genesis::AssetHubRococoUniversalLocation, AssetHubRococoParaPallet as AssetHubRococoPallet,
		AssetHubRococoRuntimeOrigin,
	},
	asset_hub_westend_emulated_chain::{
		asset_hub_westend_runtime::ExistentialDeposit as AssetHubWestendExistentialDeposit,
		genesis::AssetHubWestendUniversalLocation,
		AssetHubWestendParaPallet as AssetHubWestendPallet, AssetHubWestendRuntimeOrigin,
	},
	penpal_emulated_chain::{
		PenpalAParaPallet as PenpalAPallet, PenpalBParaPallet as PenpalBPallet,
	},
	AssetHubRococoPara as AssetHubRococo, AssetHubWestendPara as AssetHubWestend,
	PenpalAPara as PenpalA, PenpalBPara as PenpalB,
};

#[test]
fn ah_to_ah_open_close_bridge_works() {
	// open bridges
	let westend_bridge_opened_lane_id = AssetHubWestend::execute_with(|| {
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::XcmOverAssetHubRococo::open_bridge(
			AssetHubWestendRuntimeOrigin::root(),
			Box::new(AssetHubRococoUniversalLocation::get().into()),
			None,
		));
		let events = AssetHubWestend::events();
		type RuntimeEventWestend = <AssetHubWestend as Chain>::RuntimeEvent;
		events.iter().find_map(|event| {
			if let RuntimeEventWestend::XcmOverAssetHubRococo(
				pallet_xcm_bridge::Event::BridgeOpened { lane_id, .. },
			) = event
			{
				Some(*lane_id)
			} else {
				None
			}
		})
	});
	assert!(westend_bridge_opened_lane_id.is_some(), "Westend BridgeOpened event not found");

	let rococo_bridge_opened_lane_id = AssetHubRococo::execute_with(|| {
		assert_ok!(<AssetHubRococo as AssetHubRococoPallet>::XcmOverAssetHubWestend::open_bridge(
			AssetHubRococoRuntimeOrigin::root(),
			Box::new(AssetHubWestendUniversalLocation::get().into()),
			None,
		));
		let events = AssetHubRococo::events();
		type RuntimeEventRococo = <AssetHubRococo as Chain>::RuntimeEvent;
		events.iter().find_map(|event| {
			if let RuntimeEventRococo::XcmOverAssetHubWestend(
				pallet_xcm_bridge::Event::BridgeOpened { lane_id, .. },
			) = event
			{
				Some(*lane_id)
			} else {
				None
			}
		})
	});
	assert!(rococo_bridge_opened_lane_id.is_some(), "Rococo BridgeOpened event not found");

	// check the same lane ID is generated
	assert_eq!(westend_bridge_opened_lane_id, rococo_bridge_opened_lane_id);

	// close bridges
	let westend_bridge_pruned_lane_id = AssetHubWestend::execute_with(|| {
		assert_ok!(
			<AssetHubWestend as AssetHubWestendPallet>::XcmOverAssetHubRococo::close_bridge(
				AssetHubWestendRuntimeOrigin::root(),
				Box::new(AssetHubRococoUniversalLocation::get().into()),
				1,
			)
		);
		let events = AssetHubWestend::events();
		type RuntimeEventWestend = <AssetHubWestend as Chain>::RuntimeEvent;
		events.iter().find_map(|event| {
			if let RuntimeEventWestend::XcmOverAssetHubRococo(
				pallet_xcm_bridge::Event::BridgePruned { lane_id, .. },
			) = event
			{
				Some(*lane_id)
			} else {
				None
			}
		})
	});
	assert!(westend_bridge_pruned_lane_id.is_some(), "Westend BridgePruned event not found");

	let rococo_bridge_pruned_lane_id = AssetHubRococo::execute_with(|| {
		assert_ok!(<AssetHubRococo as AssetHubRococoPallet>::XcmOverAssetHubWestend::close_bridge(
			AssetHubRococoRuntimeOrigin::root(),
			Box::new(AssetHubWestendUniversalLocation::get().into()),
			1,
		));
		let events = AssetHubRococo::events();
		type RuntimeEventRococo = <AssetHubRococo as Chain>::RuntimeEvent;
		events.iter().find_map(|event| {
			if let RuntimeEventRococo::XcmOverAssetHubWestend(
				pallet_xcm_bridge::Event::BridgePruned { lane_id, .. },
			) = event
			{
				Some(*lane_id)
			} else {
				None
			}
		})
	});
	assert!(rococo_bridge_pruned_lane_id.is_some(), "Rococo BridgePruned event not found");
}

#[test]
fn para_to_para_open_close_bridge_works() {
	// Ensure Penpal A/B locations (set `on_init` with `set_storage`).
	let penpal_a_on_rococo_universal_location = PenpalA::execute_with(|| {
		let loc = <<PenpalA as Chain>::Runtime as pallet_xcm::Config>::UniversalLocation::get();
		assert_eq!(loc.global_consensus(), Ok(ByGenesis(ROCOCO_GENESIS_HASH)));
		loc
	});
	let penpal_b_on_westend_universal_location = PenpalB::execute_with(|| {
		let loc = <<PenpalB as Chain>::Runtime as pallet_xcm::Config>::UniversalLocation::get();
		assert_eq!(loc.global_consensus(), Ok(ByGenesis(WESTEND_GENESIS_HASH)));
		loc
	});

	// 1. Open bridge from PenpalA(Rococo) to PenpalB(Westend)
	let penpal_a_para_sovereign_account = AssetHubRococo::sovereign_account_id_of(
		AssetHubRococo::sibling_location_of(PenpalA::para_id()),
	);
	let fee_amount = AssetHubRococoExistentialDeposit::get() * 1000;
	let bridge_deposit = AssetHubRococo::ext_wrapper(|| {
		<AssetHubRococo as AssetHubRococoPallet>::XcmOverAssetHubWestend::bridge_deposit()
	});
	AssetHubRococo::fund_accounts(vec![(
		penpal_a_para_sovereign_account.clone().into(),
		AssetHubRococoExistentialDeposit::get() + fee_amount + bridge_deposit,
	)]);

	// send paid XCM with `open_bridge` from PenpalA to the AssetHubRococo
	PenpalA::execute_with(|| {
		assert_ok!(<PenpalA as PenpalAPallet>::PolkadotXcm::send(
			<PenpalA as Chain>::RuntimeOrigin::root(),
			bx!(PenpalA::sibling_location_of(AssetHubRococo::para_id()).into()),
			bx!(xcm_transact_paid_execution(
				bp_asset_hub_rococo::Call::XcmOverAssetHubWestend(
					bp_xcm_bridge::XcmBridgeCall::open_bridge {
						bridge_destination_universal_location: Box::new(
							penpal_b_on_westend_universal_location.into()
						),
						maybe_notify: None,
					},
				)
				.encode()
				.into(),
				OriginKind::Xcm,
				(Parent, fee_amount).into(),
				penpal_a_para_sovereign_account,
			)),
		));

		PenpalA::assert_xcm_pallet_sent();
	});

	// check BridgeOpened event on AssetHubRococo
	let penpal_a_bridge_opened_lane_id = AssetHubRococo::execute_with(|| {
		let events = AssetHubRococo::events();
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;
		AssetHubRococo::assert_xcmp_queue_success(None);
		let e = events.iter().find_map(|event| {
			if let RuntimeEvent::XcmOverAssetHubWestend(pallet_xcm_bridge::Event::BridgeOpened {
				lane_id,
				..
			}) = event
			{
				Some(*lane_id)
			} else {
				None
			}
		});
		e
	});
	assert!(penpal_a_bridge_opened_lane_id.is_some(), "PenpalA BridgeOpened event not found");

	// 2. Open bridge from PenpalB(Westend) to PenpalA(Rococo)
	let penpal_b_para_sovereign_account = AssetHubWestend::sovereign_account_id_of(
		AssetHubWestend::sibling_location_of(PenpalB::para_id()),
	);
	let fee_amount = AssetHubWestendExistentialDeposit::get() * 1000;
	let bridge_deposit = AssetHubWestend::ext_wrapper(|| {
		<AssetHubWestend as AssetHubWestendPallet>::XcmOverAssetHubRococo::bridge_deposit()
	});
	AssetHubWestend::fund_accounts(vec![(
		penpal_b_para_sovereign_account.clone().into(),
		AssetHubWestendExistentialDeposit::get() + fee_amount + bridge_deposit,
	)]);

	// send paid XCM with `open_bridge` from PenpalB to the AssetHubWestend
	PenpalB::execute_with(|| {
		assert_ok!(<PenpalB as PenpalBPallet>::PolkadotXcm::send(
			<PenpalB as Chain>::RuntimeOrigin::root(),
			bx!(PenpalB::sibling_location_of(AssetHubWestend::para_id()).into()),
			bx!(xcm_transact_paid_execution(
				bp_asset_hub_westend::Call::XcmOverAssetHubRococo(
					bp_xcm_bridge::XcmBridgeCall::open_bridge {
						bridge_destination_universal_location: Box::new(
							penpal_a_on_rococo_universal_location.into()
						),
						maybe_notify: None,
					},
				)
				.encode()
				.into(),
				OriginKind::Xcm,
				(Parent, fee_amount).into(),
				penpal_b_para_sovereign_account,
			)),
		));

		PenpalB::assert_xcm_pallet_sent();
	});

	// check BridgeOpened event on AssetHubWestend
	let penpal_b_bridge_opened_lane_id = AssetHubWestend::execute_with(|| {
		let events = AssetHubWestend::events();
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		AssetHubWestend::assert_xcmp_queue_success(None);
		events.iter().find_map(|event| {
			if let RuntimeEvent::XcmOverAssetHubRococo(pallet_xcm_bridge::Event::BridgeOpened {
				lane_id,
				..
			}) = event
			{
				Some(*lane_id)
			} else {
				None
			}
		})
	});
	assert!(penpal_b_bridge_opened_lane_id.is_some(), "PenpalB BridgeOpened event not found");

	// check the same lane ID is generated
	assert_eq!(penpal_a_bridge_opened_lane_id, penpal_b_bridge_opened_lane_id);
}
