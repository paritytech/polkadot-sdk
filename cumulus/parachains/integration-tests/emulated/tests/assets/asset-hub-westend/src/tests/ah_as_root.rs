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

/// System pallet on the Relay Chain for constructing remote calls.
#[derive(Encode, Decode)]
enum RelayPallets {
	#[codec(index = 50)]
	System(SystemCalls),
}

/// Call encoding for the calls needed from the Broker pallet.
#[derive(Encode, Decode)]
enum SystemCalls {
	#[codec(index = 1)]
	Remark(pallet_broker::Schedule),
}

#[test]
fn ah_can_transact_as_root_on_relay_chain() {
	// Encoded `set_storage` call to be executed in Relay Chain
	type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
	let root_origin = <AssetHubWestend as Chain>::RuntimeOrigin::root();
	let relay_location = Parent.into();

	let xcm = VersionedXcm::from(Xcm(vec![
		UnpaidExecution { weight_limit: Unlimited, check_origin: None },
		Transact {
			OriginKind::Superuser,
			fallback_max_weight: None,
			call: call.into(),
		},
	]));

	AssetHubWestend::execute_with(|| {
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::send(
			root_origin,
			relay_location,
			xcm,
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
