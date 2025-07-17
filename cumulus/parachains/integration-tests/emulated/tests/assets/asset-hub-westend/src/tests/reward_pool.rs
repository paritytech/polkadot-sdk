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
use codec::Encode;
use frame_support::{assert_ok, sp_runtime::traits::Dispatchable, traits::schedule::DispatchTime};
use xcm_executor::traits::ConvertLocation;

#[test]
fn treasury_creates_asset_reward_pool() {
	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		type Balances = <AssetHubWestend as AssetHubWestendPallet>::Balances;

		let treasurer =
			Location::new(1, [Plurality { id: BodyId::Treasury, part: BodyPart::Voice }]);
		let treasurer_account =
			ahw_xcm_config::LocationToAccountId::convert_location(&treasurer).unwrap();

		assert_ok!(Balances::force_set_balance(
			<AssetHubWestend as Chain>::RuntimeOrigin::root(),
			treasurer_account.clone().into(),
			ASSET_HUB_WESTEND_ED * 100_000,
		));

		let events = AssetHubWestend::events();
		match events.iter().last() {
			Some(RuntimeEvent::Balances(pallet_balances::Event::BalanceSet { who, .. })) =>
				assert_eq!(*who, treasurer_account),
			_ => panic!("Expected Balances::BalanceSet event"),
		}
	});
	Westend::execute_with(|| {
		type AssetHubWestendRuntimeCall = <AssetHubWestend as Chain>::RuntimeCall;
		type AssetHubWestendRuntime = <AssetHubWestend as Chain>::Runtime;
		type WestendRuntimeCall = <Westend as Chain>::RuntimeCall;
		type WestendRuntime = <Westend as Chain>::Runtime;
		type WestendRuntimeEvent = <Westend as Chain>::RuntimeEvent;
		type WestendRuntimeOrigin = <Westend as Chain>::RuntimeOrigin;

		Dmp::make_parachain_reachable(AssetHubWestend::para_id());

		let staked_asset_id = bx!(RelayLocation::get());
		let reward_asset_id = bx!(RelayLocation::get());

		let reward_rate_per_block = 1_000_000_000;
		let lifetime = 1_000_000_000;
		let admin = None;

		let create_pool_call =
			WestendRuntimeCall::XcmPallet(pallet_xcm::Call::<WestendRuntime>::send {
				dest: bx!(VersionedLocation::V4(
					xcm::v4::Junction::Parachain(AssetHubWestend::para_id().into()).into()
				)),
				message: bx!(VersionedXcm::V5(Xcm(vec![
					UnpaidExecution { weight_limit: Unlimited, check_origin: None },
					Transact {
						origin_kind: OriginKind::SovereignAccount,
						fallback_max_weight: None,
						call: AssetHubWestendRuntimeCall::AssetRewards(
							pallet_asset_rewards::Call::<AssetHubWestendRuntime>::create_pool {
								staked_asset_id,
								reward_asset_id,
								reward_rate_per_block,
								expiry: DispatchTime::After(lifetime),
								admin
							}
						)
						.encode()
						.into(),
					}
				]))),
			});

		let treasury_origin: WestendRuntimeOrigin = Treasurer.into();
		assert_ok!(create_pool_call.dispatch(treasury_origin));

		assert_expected_events!(
			Westend,
			vec![
				WestendRuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { .. }) => {},
			]
		);
	});

	AssetHubWestend::execute_with(|| {
		type Runtime = <AssetHubWestend as Chain>::Runtime;
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		assert_eq!(1, pallet_asset_rewards::Pools::<Runtime>::iter().count());

		let events = AssetHubWestend::events();
		match events.iter().last() {
			Some(RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {
				success: true,
				..
			})) => (),
			_ => panic!("Expected MessageQueue::Processed event"),
		}
	});
}
