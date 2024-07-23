// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::*;
use asset_hub_westend_runtime::{
	xcm_config::LocationToAccountId as AssetHubLocationToAccountId, ProxyType as AssetHubProxyType,
};
use codec::Encode;
use collectives_westend_runtime::ProxyType as CollectivesProxyType;
use frame_support::{
	assert_ok,
	traits::fungible::{Inspect, Mutate},
};
use xcm_executor::traits::ConvertLocation;

#[test]
fn cross_chain_pure_proxy() {
	// proxy of the pure account.
	let alice = Westend::account_id_of("Alice");
	// spawner of the pure account, delegates to the pure account to Alice.
	let bob = Westend::account_id_of("Bob");

	// Bob creates a pure account on Collectives Chain.
	let pure = CollectivesWestend::execute_with(|| {
		type Proxy = <CollectivesWestend as CollectivesWestendPallet>::Proxy;
		type RuntimeOrigin = <CollectivesWestend as Chain>::RuntimeOrigin;
		type RuntimeEvent = <CollectivesWestend as Chain>::RuntimeEvent;

		assert_ok!(Proxy::create_pure(
			RuntimeOrigin::signed(bob.clone()),
			CollectivesProxyType::Any,
			0,
			0
		));

		let events = CollectivesWestend::events();
		match events.iter().last() {
			Some(RuntimeEvent::Proxy(pallet_proxy::Event::PureCreated { pure, .. })) =>
				pure.clone(),
			_ => panic!("Expected PureCreated event"),
		}
	});

	// Bob adds Alice as a proxy to the pure account on Collectives Chain.
	CollectivesWestend::execute_with(|| {
		type Proxy = <CollectivesWestend as CollectivesWestendPallet>::Proxy;
		type Balances = <CollectivesWestend as CollectivesWestendPallet>::Balances;
		type Runtime = <CollectivesWestend as Chain>::Runtime;
		type RuntimeOrigin = <CollectivesWestend as Chain>::RuntimeOrigin;
		type RuntimeEvent = <CollectivesWestend as Chain>::RuntimeEvent;
		type RuntimeCall = <CollectivesWestend as Chain>::RuntimeCall;

		// fund pure account for a deposit required for adding Alice as a proxy.
		assert_ok!(<Balances as Mutate<_>>::mint_into(
			&pure,
			<Balances as Inspect<_>>::minimum_balance() * 100
		));

		let add_proxy_call = RuntimeCall::Proxy(pallet_proxy::Call::<Runtime>::add_proxy {
			delegate: alice.clone().into(),
			proxy_type: CollectivesProxyType::Any,
			delay: 0,
		});

		assert_ok!(Proxy::proxy(
			RuntimeOrigin::signed(bob.clone()),
			pure.clone().into(),
			None,
			bx!(add_proxy_call),
		));

		let events = CollectivesWestend::events();
		match events.iter().rev().nth(1) {
			Some(RuntimeEvent::Proxy(pallet_proxy::Event::ProxyAdded {
				delegator,
				delegatee,
				..
			})) => {
				assert_eq!(*delegator, pure);
				assert_eq!(*delegatee, alice);
			},
			_ => panic!("Expected PureCreated event"),
		}
	});

	// Alice sends the XCM program from Collectives to the Asset Hub on behalf of a pure account,
	// aiming to add herself as a proxy to that same pure account.
	//
	// Given that the XCM program on the Asset Hub will execute with the origin set as
	// location(1, [Parachain(1001), pure]) after descending the origin, it's necessary to
	// pre-fund that account to cover execution fees.
	// Additionally, the pure account on the Asset Hub requires funding for the deposit needed
	// to add Alice as a proxy.
	//
	// TODO: We can eliminate the need for additional funding on the Asset Hub by teleporting
	// some assets from the Collectives to the Asset Hub. These assets would be placed in a holding
	// registry for the actual XCM program execution and can be used for fee payments and deposit.
	// This could be achieved with a call like `xcm_pallet::transfer_assets_using_type_and_then`.
	// However, the current version of xcm-executor is not suitable for our case, as it requires to
	// clear the origin (using the ClearOrigin instruction) after receiving teleported/reserve
	// assets and before executing any following XCM program. A potential solution could involve
	// allowing the use of the `DescendOrigin` instruction in certain scenarios, instead of
	// `ClearOrigin`.

	AssetHubWestend::execute_with(|| {
		type Balances = <AssetHubWestend as AssetHubWestendPallet>::Balances;

		let ah_pure = AssetHubLocationToAccountId::convert_location(&Location::new(
			1,
			[
				Parachain(1001),
				AccountId32 { network: Some(NetworkId::Westend), id: pure.clone().into() },
			],
		))
		.unwrap();

		assert_ok!(<Balances as Mutate<_>>::mint_into(
			&ah_pure,
			<Balances as Inspect<_>>::minimum_balance() * 100
		));

		assert_ok!(<Balances as Mutate<_>>::mint_into(
			&pure,
			<Balances as Inspect<_>>::minimum_balance() * 100
		));
	});

	// Alice sends the XCM program from Collectives to the Asset Hub.
	CollectivesWestend::execute_with(|| {
		type Proxy = <CollectivesWestend as CollectivesWestendPallet>::Proxy;
		type Runtime = <CollectivesWestend as Chain>::Runtime;
		type RuntimeOrigin = <CollectivesWestend as Chain>::RuntimeOrigin;
		type RuntimeEvent = <CollectivesWestend as Chain>::RuntimeEvent;
		type RuntimeCall = <CollectivesWestend as Chain>::RuntimeCall;
		type AssetHubRuntime = <AssetHubWestend as Chain>::Runtime;
		type AssetHubRuntimeCall = <AssetHubWestend as Chain>::RuntimeCall;
		type AssetHubBalances = <AssetHubWestend as AssetHubWestendPallet>::Balances;

		// Add proxy call to be executed on the Asset Hub on behalf of the pure account.
		let add_proxy_call =
			AssetHubRuntimeCall::Proxy(pallet_proxy::Call::<AssetHubRuntime>::add_proxy {
				delegate: alice.clone().into(),
				proxy_type: AssetHubProxyType::Any,
				delay: 0,
			});

		let asset_hub_location = Location::new(1, Parachain(1000));
		let fee_asset = Asset {
			id: Location::parent().into(),
			fun: (<AssetHubBalances as Inspect<_>>::minimum_balance() * 20).into(),
		};

		// XCM send call to be commanded by pure account on Collectives.
		let send_xcm = RuntimeCall::PolkadotXcm(pallet_xcm::Call::<Runtime>::send {
			dest: bx!(VersionedLocation::V4(asset_hub_location)),
			message: bx!(VersionedXcm::V4(Xcm(vec![
				WithdrawAsset(fee_asset.clone().into()),
				BuyExecution { weight_limit: Unlimited, fees: fee_asset },
				// we replace `Location(1, [Parachain(1001), pure])` by Location(0, [pure])
				AliasOrigin(Location::new(
					0,
					AccountId32 { network: None, id: pure.clone().into() }
				)),
				Transact {
					origin_kind: OriginKind::SovereignAccount,
					require_weight_at_most: Weight::from_parts(1_000_000_000, 10_000),
					call: add_proxy_call.encode().into(),
				},
			]))),
		});

		// Alice proxies the XCM send call.
		assert_ok!(Proxy::proxy(
			RuntimeOrigin::signed(alice.clone()),
			pure.clone().into(),
			None,
			bx!(send_xcm),
		));

		let events = CollectivesWestend::events();
		match events.iter().last() {
			Some(RuntimeEvent::Proxy(pallet_proxy::Event::ProxyExecuted { result: Ok(()) })) => (),
			_ => panic!("Expected ProxyExecuted event"),
		}
	});

	AssetHubWestend::execute_with(|| {
		type Proxy = <AssetHubWestend as AssetHubWestendPallet>::Proxy;
		type Balances = <AssetHubWestend as AssetHubWestendPallet>::Balances;
		type Runtime = <AssetHubWestend as Chain>::Runtime;
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		type RuntimeCall = <AssetHubWestend as Chain>::RuntimeCall;

		let events = AssetHubWestend::events();
		match events.iter().last() {
			Some(RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {
				success: true,
				..
			})) => (),
			_ => panic!("Expected MessageQueue::Processed event"),
		}

		let bob_init_balance = <Balances as Inspect<_>>::balance(&bob);
		let transfer_amount = <Balances as Inspect<_>>::minimum_balance() * 10;

		// Alice transfers some funds from pure account on Asset Hub.

		let transfer_call =
			RuntimeCall::Balances(pallet_balances::Call::<Runtime>::transfer_keep_alive {
				dest: bob.clone().into(),
				value: transfer_amount,
			});

		assert_ok!(Proxy::proxy(
			RuntimeOrigin::signed(alice.clone()),
			pure.clone().into(),
			None,
			bx!(transfer_call),
		));

		assert_eq!(
			<Balances as Inspect<_>>::total_balance(&bob),
			bob_init_balance + transfer_amount,
		);
	});
}
