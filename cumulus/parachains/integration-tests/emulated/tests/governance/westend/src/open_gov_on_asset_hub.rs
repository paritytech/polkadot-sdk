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
use asset_hub_westend_runtime::governance::pallet_custom_origins::Origin;

#[test]
fn assethub_can_authorize_upgrade_for_itself() {
	let code_hash = [1u8; 32].into();
	type AssetHubRuntime = <AssetHubWestend as Chain>::Runtime;
	type AssetHubRuntimeCall = <AssetHubWestend as Chain>::RuntimeCall;
	type AssetHubRuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

	let authorize_upgrade =
		AssetHubRuntimeCall::Utility(pallet_utility::Call::<AssetHubRuntime>::force_batch {
			calls: vec![AssetHubRuntimeCall::System(frame_system::Call::authorize_upgrade {
				code_hash,
			})],
		});

	// bad origin
	let invalid_origin: AssetHubRuntimeOrigin = Origin::StakingAdmin.into();
	// ok origin
	let ok_origin: AssetHubRuntimeOrigin = Origin::WhitelistedCaller.into();

	// store preimage
	let call_hash = dispatch_note_preimage_call::<AssetHubWestend>(authorize_upgrade.clone());

	// Err - when dispatch non-whitelisted
	assert_err!(
		dispatch_whitelisted_call_with_preimage::<AssetHubWestend>(
			authorize_upgrade.clone(),
			ok_origin.clone()
		),
		DispatchError::Module(sp_runtime::ModuleError {
			index: 93,
			error: [3, 0, 0, 0],
			message: Some("CallIsNotWhitelisted")
		})
	);

	// whitelist
	collectives_send_whitelist(
		CollectivesWestend::sibling_location_of(<AssetHubWestend as Parachain>::para_id()),
		|| {
			AssetHubRuntimeCall::Whitelist(
				pallet_whitelist::Call::<AssetHubRuntime>::whitelist_call { call_hash },
			)
			.encode()
		},
	);

	// Err - when dispatch wrong origin
	assert_err!(
		dispatch_whitelisted_call_with_preimage::<AssetHubWestend>(
			authorize_upgrade.clone(),
			invalid_origin
		),
		DispatchError::BadOrigin
	);

	// check before
	AssetHubWestend::execute_with(|| {
		assert!(<AssetHubWestend as Chain>::System::authorized_upgrade().is_none())
	});

	// ok - authorized
	assert_ok!(dispatch_whitelisted_call_with_preimage::<AssetHubWestend>(
		authorize_upgrade,
		ok_origin
	));

	// check after - authorized
	AssetHubWestend::execute_with(|| {
		assert!(<AssetHubWestend as Chain>::System::authorized_upgrade().is_some())
	});
}

#[test]
fn assethub_can_authorize_upgrade_for_relay_chain() {
	type AssetHubRuntime = <AssetHubWestend as Chain>::Runtime;
	type AssetHubRuntimeCall = <AssetHubWestend as Chain>::RuntimeCall;
	type AssetHubRuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

	let authorize_upgrade =
		AssetHubRuntimeCall::Utility(pallet_utility::Call::<AssetHubRuntime>::force_batch {
			calls: vec![build_xcm_send_authorize_upgrade_call::<AssetHubWestend, Westend>(
				AssetHubWestend::parent_location(),
			)],
		});

	// bad origin
	let invalid_origin: AssetHubRuntimeOrigin = Origin::StakingAdmin.into();
	// ok origin
	let ok_origin: AssetHubRuntimeOrigin = Origin::WhitelistedCaller.into();

	// store preimage
	let call_hash = dispatch_note_preimage_call::<AssetHubWestend>(authorize_upgrade.clone());

	// Err - when dispatch non-whitelisted
	assert_err!(
		dispatch_whitelisted_call_with_preimage::<AssetHubWestend>(
			authorize_upgrade.clone(),
			ok_origin.clone()
		),
		DispatchError::Module(sp_runtime::ModuleError {
			index: 93,
			error: [3, 0, 0, 0],
			message: Some("CallIsNotWhitelisted")
		})
	);

	// whitelist
	collectives_send_whitelist(
		CollectivesWestend::sibling_location_of(<AssetHubWestend as Parachain>::para_id()),
		|| {
			AssetHubRuntimeCall::Whitelist(
				pallet_whitelist::Call::<AssetHubRuntime>::whitelist_call { call_hash },
			)
			.encode()
		},
	);

	// Err - when dispatch wrong origin
	assert_err!(
		dispatch_whitelisted_call_with_preimage::<AssetHubWestend>(
			authorize_upgrade.clone(),
			invalid_origin
		),
		DispatchError::BadOrigin
	);

	// check before
	Westend::execute_with(|| assert!(<Westend as Chain>::System::authorized_upgrade().is_none()));

	// ok - authorized
	assert_ok!(dispatch_whitelisted_call_with_preimage::<AssetHubWestend>(
		authorize_upgrade,
		ok_origin
	));

	// check after - authorized
	Westend::execute_with(|| assert!(<Westend as Chain>::System::authorized_upgrade().is_some()));
}

#[test]
fn assethub_can_authorize_upgrade_for_system_chains() {
	type AssetHubRuntime = <AssetHubWestend as Chain>::Runtime;
	type AssetHubRuntimeCall = <AssetHubWestend as Chain>::RuntimeCall;
	type AssetHubRuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

	let authorize_upgrade =
		AssetHubRuntimeCall::Utility(pallet_utility::Call::<AssetHubRuntime>::force_batch {
			calls: vec![
				build_xcm_send_authorize_upgrade_call::<AssetHubWestend, BridgeHubWestend>(
					AssetHubWestend::sibling_location_of(BridgeHubWestend::para_id()),
				),
				build_xcm_send_authorize_upgrade_call::<AssetHubWestend, CollectivesWestend>(
					AssetHubWestend::sibling_location_of(CollectivesWestend::para_id()),
				),
				build_xcm_send_authorize_upgrade_call::<AssetHubWestend, CoretimeWestend>(
					AssetHubWestend::sibling_location_of(CoretimeWestend::para_id()),
				),
				build_xcm_send_authorize_upgrade_call::<AssetHubWestend, PeopleWestend>(
					AssetHubWestend::sibling_location_of(PeopleWestend::para_id()),
				),
			],
		});

	// bad origin
	let invalid_origin: AssetHubRuntimeOrigin = Origin::StakingAdmin.into();
	// ok origin
	let ok_origin: AssetHubRuntimeOrigin = Origin::WhitelistedCaller.into();

	// store preimage
	let call_hash = dispatch_note_preimage_call::<AssetHubWestend>(authorize_upgrade.clone());

	// Err - when dispatch non-whitelisted
	assert_err!(
		dispatch_whitelisted_call_with_preimage::<AssetHubWestend>(
			authorize_upgrade.clone(),
			ok_origin.clone()
		),
		DispatchError::Module(sp_runtime::ModuleError {
			index: 93,
			error: [3, 0, 0, 0],
			message: Some("CallIsNotWhitelisted")
		})
	);

	// whitelist
	collectives_send_whitelist(
		CollectivesWestend::sibling_location_of(<AssetHubWestend as Parachain>::para_id()),
		|| {
			AssetHubRuntimeCall::Whitelist(
				pallet_whitelist::Call::<AssetHubRuntime>::whitelist_call { call_hash },
			)
			.encode()
		},
	);

	// Err - when dispatch wrong origin
	assert_err!(
		dispatch_whitelisted_call_with_preimage::<AssetHubWestend>(
			authorize_upgrade.clone(),
			invalid_origin
		),
		DispatchError::BadOrigin
	);

	// check before
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
	assert_ok!(dispatch_whitelisted_call_with_preimage::<AssetHubWestend>(
		authorize_upgrade,
		ok_origin
	));

	// check after - authorized
	BridgeHubWestend::execute_with(|| {
		assert!(<BridgeHubWestend as Chain>::System::authorized_upgrade().is_some())
	});
	CollectivesWestend::execute_with(|| {
		assert!(<CollectivesWestend as Chain>::System::authorized_upgrade().is_some())
	});
	CoretimeWestend::execute_with(|| {
		assert!(<CoretimeWestend as Chain>::System::authorized_upgrade().is_some())
	});
	PeopleWestend::execute_with(|| {
		assert!(<PeopleWestend as Chain>::System::authorized_upgrade().is_some())
	});
}