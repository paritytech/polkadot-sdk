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
use crate::{common::*, imports::*};
use codec::Encode;
use emulated_integration_tests_common::{
	assert_whitelisted,
	impls::RelayChain,
	xcm_emulator::{Chain, Parachain, TestExt},
	xcm_helpers::{
		build_xcm_send_authorize_upgrade_call, call_hash_of,
		dispatch_whitelisted_call_with_preimage,
	},
};
use frame_support::assert_err;
use sp_runtime::DispatchError;
use westend_runtime::governance::pallet_custom_origins::Origin;
use westend_system_emulated_network::{
	AssetHubWestendPara as AssetHubWestend, BridgeHubWestendPara as BridgeHubWestend,
	CoretimeWestendPara as CoretimeWestend, PeopleWestendPara as PeopleWestend,
	WestendRelay as Westend,
};

use westend_system_emulated_network::westend_emulated_chain::westend_runtime::Dmp;

#[test]
fn relaychain_can_authorize_upgrade_for_itself() {
	let code_hash = [1u8; 32].into();
	type WestendRuntime = <Westend as Chain>::Runtime;
	type WestendRuntimeCall = <Westend as Chain>::RuntimeCall;
	type WestendRuntimeOrigin = <Westend as Chain>::RuntimeOrigin;

	let authorize_upgrade =
		WestendRuntimeCall::Utility(pallet_utility::Call::<WestendRuntime>::force_batch {
			calls: vec![
				// upgrade the relaychain
				WestendRuntimeCall::System(frame_system::Call::authorize_upgrade { code_hash }),
			],
		});

	// bad origin
	let invalid_origin: WestendRuntimeOrigin = Origin::StakingAdmin.into();
	// ok origin
	let ok_origin: WestendRuntimeOrigin = Origin::WhitelistedCaller.into();

	let call_hash = call_hash_of::<Westend>(&authorize_upgrade);

	// Err - when dispatch non-whitelisted
	assert_err!(
		dispatch_whitelisted_call_with_preimage::<Westend>(
			authorize_upgrade.clone(),
			ok_origin.clone()
		),
		DispatchError::Module(sp_runtime::ModuleError {
			index: 36,
			error: [3, 0, 0, 0],
			message: Some("CallIsNotWhitelisted")
		})
	);

	// whitelist
	collectives_send_whitelist(Location::parent(), || {
		WestendRuntimeCall::Whitelist(pallet_whitelist::Call::<WestendRuntime>::whitelist_call {
			call_hash,
		})
		.encode()
	});

	// Err - when dispatch wrong origin
	assert_err!(
		dispatch_whitelisted_call_with_preimage::<Westend>(
			authorize_upgrade.clone(),
			invalid_origin
		),
		DispatchError::BadOrigin
	);

	// check before
	Westend::execute_with(|| assert!(<Westend as Chain>::System::authorized_upgrade().is_none()));

	// ok - authorized
	assert_ok!(dispatch_whitelisted_call_with_preimage::<Westend>(authorize_upgrade, ok_origin));

	// check after - authorized
	Westend::execute_with(|| {
		assert_eq!(
			<Westend as Chain>::System::authorized_upgrade().unwrap().code_hash(),
			&code_hash
		)
	});
}

#[test]
fn relaychain_can_authorize_upgrade_for_system_chains() {
	type WestendRuntime = <Westend as Chain>::Runtime;
	type WestendRuntimeCall = <Westend as Chain>::RuntimeCall;
	type WestendRuntimeOrigin = <Westend as Chain>::RuntimeOrigin;

	Westend::execute_with(|| {
		Dmp::make_parachain_reachable(AssetHubWestend::para_id());
		Dmp::make_parachain_reachable(BridgeHubWestend::para_id());
		Dmp::make_parachain_reachable(CollectivesWestend::para_id());
		Dmp::make_parachain_reachable(CoretimeWestend::para_id());
		Dmp::make_parachain_reachable(PeopleWestend::para_id());
	});

	let code_hash_asset_hub = [1u8; 32].into();
	let code_hash_bridge_hub = [2u8; 32].into();
	let code_hash_collectives = [3u8; 32].into();
	let code_hash_coretime = [4u8; 32].into();
	let code_hash_people = [5u8; 32].into();

	let authorize_upgrade =
		WestendRuntimeCall::Utility(pallet_utility::Call::<WestendRuntime>::force_batch {
			calls: vec![
				build_xcm_send_authorize_upgrade_call::<Westend, AssetHubWestend>(
					Westend::child_location_of(AssetHubWestend::para_id()),
					&code_hash_asset_hub,
					None,
				),
				build_xcm_send_authorize_upgrade_call::<Westend, BridgeHubWestend>(
					Westend::child_location_of(BridgeHubWestend::para_id()),
					&code_hash_bridge_hub,
					None,
				),
				build_xcm_send_authorize_upgrade_call::<Westend, CollectivesWestend>(
					Westend::child_location_of(CollectivesWestend::para_id()),
					&code_hash_collectives,
					None,
				),
				build_xcm_send_authorize_upgrade_call::<Westend, CoretimeWestend>(
					Westend::child_location_of(CoretimeWestend::para_id()),
					&code_hash_coretime,
					None,
				),
				build_xcm_send_authorize_upgrade_call::<Westend, PeopleWestend>(
					Westend::child_location_of(PeopleWestend::para_id()),
					&code_hash_people,
					None,
				),
			],
		});

	// bad origin
	let invalid_origin: WestendRuntimeOrigin = Origin::StakingAdmin.into();
	// ok origin
	let ok_origin: WestendRuntimeOrigin = Origin::WhitelistedCaller.into();

	let call_hash = call_hash_of::<Westend>(&authorize_upgrade);

	// Err - when dispatch non-whitelisted
	assert_err!(
		dispatch_whitelisted_call_with_preimage::<Westend>(
			authorize_upgrade.clone(),
			ok_origin.clone()
		),
		DispatchError::Module(sp_runtime::ModuleError {
			index: 36,
			error: [3, 0, 0, 0],
			message: Some("CallIsNotWhitelisted")
		})
	);

	// whitelist
	collectives_send_whitelist(Location::parent(), || {
		WestendRuntimeCall::Whitelist(pallet_whitelist::Call::<WestendRuntime>::whitelist_call {
			call_hash,
		})
		.encode()
	});

	Westend::execute_with(|| {
		assert_whitelisted!(Westend, call_hash);
	});

	// Err - when dispatch wrong origin
	assert_err!(
		dispatch_whitelisted_call_with_preimage::<Westend>(
			authorize_upgrade.clone(),
			invalid_origin
		),
		DispatchError::BadOrigin
	);

	// check before
	AssetHubWestend::execute_with(|| {
		assert!(<AssetHubWestend as Chain>::System::authorized_upgrade().is_none())
	});
	BridgeHubWestend::execute_with(|| {
		assert!(<BridgeHubWestend as Chain>::System::authorized_upgrade().is_none())
	});
	CollectivesWestend::execute_with(|| {
		assert!(<CollectivesWestend as Chain>::System::authorized_upgrade().is_none())
	});
	CoretimeWestend::execute_with(|| {
		assert!(<CoretimeWestend as Chain>::System::authorized_upgrade().is_none())
	});
	PeopleWestend::execute_with(|| {
		assert!(<PeopleWestend as Chain>::System::authorized_upgrade().is_none())
	});

	// ok - authorized
	assert_ok!(dispatch_whitelisted_call_with_preimage::<Westend>(authorize_upgrade, ok_origin));

	AssetHubWestend::execute_with(|| {
		assert_eq!(
			<AssetHubWestend as Chain>::System::authorized_upgrade().unwrap().code_hash(),
			&code_hash_asset_hub
		)
	});
	// check after - authorized
	BridgeHubWestend::execute_with(|| {
		assert_eq!(
			<BridgeHubWestend as Chain>::System::authorized_upgrade().unwrap().code_hash(),
			&code_hash_bridge_hub
		)
	});
	CollectivesWestend::execute_with(|| {
		assert_eq!(
			<CollectivesWestend as Chain>::System::authorized_upgrade().unwrap().code_hash(),
			&code_hash_collectives
		)
	});
	CoretimeWestend::execute_with(|| {
		assert_eq!(
			<CoretimeWestend as Chain>::System::authorized_upgrade().unwrap().code_hash(),
			&code_hash_coretime
		)
	});
	PeopleWestend::execute_with(|| {
		assert_eq!(
			<PeopleWestend as Chain>::System::authorized_upgrade().unwrap().code_hash(),
			&code_hash_people
		)
	});
}
