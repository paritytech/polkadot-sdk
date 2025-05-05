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

//! Ensure that AH can execute as root on the Relay Chain via XCM.

use crate::imports::*;

#[test]
fn ah_can_transact_as_root_on_relay_chain() {
	// Encoded `set_storage` call to be executed in Relay Chain
	let call: <Westend as Chain>::RuntimeCall =
		frame_system::Call::<<Westend as Chain>::Runtime>::set_storage { items: vec![] }.into();

	type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
	let root_origin = <AssetHubWestend as Chain>::RuntimeOrigin::root();
	let relay_location: Location = Parent.into();

	let xcm = VersionedXcm::from(Xcm(vec![
		UnpaidExecution { weight_limit: Unlimited, check_origin: None },
		Transact {
			origin_kind: OriginKind::Superuser,
			fallback_max_weight: None,
			call: call.encode().into(),
		},
	]));

	AssetHubWestend::execute_with(|| {
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::send(
			root_origin,
			Box::new(VersionedLocation::from(relay_location)),
			Box::new(xcm),
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::PolkadotXcm(pallet_xcm::Event::Sent { .. }) => {},
			]
		);
	});

	Westend::execute_with(|| {
		type RuntimeEvent = <Westend as Chain>::RuntimeEvent;
		assert_expected_events!(
			Westend,
			vec![
				RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { success: true, .. }) => {},
			]
		);
	});
}
