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

use crate::*;
use codec::Encode;
use collectives_fellowship::pallet_fellowship_origins::Origin::Fellows as FellowsOrigin;
use frame_support::{assert_ok, sp_runtime::traits::Dispatchable};

#[test]
fn fellows_whitelist_call() {
	CollectivesWestend::execute_with(|| {
		type RuntimeEvent = <CollectivesWestend as Chain>::RuntimeEvent;
		type RuntimeCall = <CollectivesWestend as Chain>::RuntimeCall;
		type RuntimeOrigin = <CollectivesWestend as Chain>::RuntimeOrigin;
		type Runtime = <CollectivesWestend as Chain>::Runtime;
		type WestendCall = <Westend as Chain>::RuntimeCall;
		type WestendRuntime = <Westend as Chain>::Runtime;

		let call_hash = [1u8; 32].into();

		let whitelist_call = RuntimeCall::PolkadotXcm(pallet_xcm::Call::<Runtime>::send {
			dest: bx!(VersionedLocation::from(Location::parent())),
			message: bx!(VersionedXcm::from(Xcm(vec![
				UnpaidExecution { weight_limit: Unlimited, check_origin: None },
				Transact {
					origin_kind: OriginKind::Xcm,
					call: WestendCall::Whitelist(
						pallet_whitelist::Call::<WestendRuntime>::whitelist_call { call_hash }
					)
					.encode()
					.into(),
				}
			]))),
		});

		let fellows_origin: RuntimeOrigin = FellowsOrigin.into();

		assert_ok!(whitelist_call.dispatch(fellows_origin));

		assert_expected_events!(
			CollectivesWestend,
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
				RuntimeEvent::Whitelist(pallet_whitelist::Event::CallWhitelisted { .. }) => {},
				RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { success: true, .. }) => {},
			]
		);
	});
}
