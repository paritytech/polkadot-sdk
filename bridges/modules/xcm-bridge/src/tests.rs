// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

#![cfg(test)]

use super::*;
use bp_messages::LaneIdType;
use mock::*;

use frame_support::{assert_err, assert_noop, assert_ok, BoundedVec};
use frame_system::{EventRecord, Phase};
use sp_runtime::{traits::Zero, TryRuntimeError};

fn mock_open_bridge_from_with(
	origin: RuntimeOrigin,
	deposit: Option<Balance>,
	with: InteriorLocation,
) -> (BridgeOf<TestRuntime, ()>, BridgeLocations) {
	let locations =
		XcmOverBridge::bridge_locations_from_origin(origin, Box::new(with.into())).unwrap();
	let lane_id = locations.calculate_lane_id(xcm::latest::VERSION).unwrap();

	let deposit = deposit.map(|deposit| {
		let bridge_owner_account =
			fund_origin_sovereign_account(&locations, deposit + ExistentialDeposit::get());
		Balances::hold(&HoldReason::BridgeDeposit.into(), &bridge_owner_account, deposit).unwrap();
		Deposit::new(bridge_owner_account, deposit)
	});

	let bridge = Bridge {
		bridge_origin_relative_location: Box::new(
			locations.bridge_origin_relative_location().clone().into(),
		),
		bridge_origin_universal_location: Box::new(
			locations.bridge_origin_universal_location().clone().into(),
		),
		bridge_destination_universal_location: Box::new(
			locations.bridge_destination_universal_location().clone().into(),
		),
		state: BridgeState::Opened,
		deposit,
		lane_id,
		maybe_notify: None,
	};
	Bridges::<TestRuntime, ()>::insert(locations.bridge_id(), bridge.clone());
	LaneToBridge::<TestRuntime, ()>::insert(bridge.lane_id, locations.bridge_id());

	let lanes_manager = LanesManagerOf::<TestRuntime, ()>::new();
	lanes_manager.create_inbound_lane(bridge.lane_id).unwrap();
	lanes_manager.create_outbound_lane(bridge.lane_id).unwrap();

	assert_ok!(XcmOverBridge::do_try_state());

	(bridge, *locations)
}

fn mock_open_bridge_from(
	origin: RuntimeOrigin,
	deposit: Option<Balance>,
) -> (BridgeOf<TestRuntime, ()>, BridgeLocations) {
	mock_open_bridge_from_with(origin, deposit, bridged_asset_hub_universal_location())
}

fn enqueue_message(lane: TestLaneIdType) {
	let lanes_manager = LanesManagerOf::<TestRuntime, ()>::new();
	lanes_manager
		.active_outbound_lane(lane)
		.unwrap()
		.send_message(BoundedVec::try_from(vec![42]).expect("We craft valid messages"));
}

#[test]
fn open_bridge_fails_if_origin_is_not_allowed() {
	run_test(|| {
		assert_noop!(
			XcmOverBridge::open_bridge(
				OpenBridgeOrigin::disallowed_origin(),
				Box::new(bridged_asset_hub_universal_location().into()),
				None,
			),
			sp_runtime::DispatchError::BadOrigin,
		);
	})
}

#[test]
fn open_bridge_fails_if_origin_is_not_relative() {
	run_test(|| {
		assert_noop!(
			XcmOverBridge::open_bridge(
				OpenBridgeOrigin::parent_relay_chain_universal_origin(),
				Box::new(bridged_asset_hub_universal_location().into()),
				None,
			),
			Error::<TestRuntime, ()>::BridgeLocations(BridgeLocationsError::InvalidBridgeOrigin),
		);

		assert_noop!(
			XcmOverBridge::open_bridge(
				OpenBridgeOrigin::sibling_parachain_universal_origin(),
				Box::new(bridged_asset_hub_universal_location().into()),
				None,
			),
			Error::<TestRuntime, ()>::BridgeLocations(BridgeLocationsError::InvalidBridgeOrigin),
		);
	})
}

#[test]
fn open_bridge_fails_if_destination_is_not_remote() {
	run_test(|| {
		assert_noop!(
			XcmOverBridge::open_bridge(
				OpenBridgeOrigin::parent_relay_chain_origin(),
				Box::new(
					[GlobalConsensus(RelayNetwork::get()), Parachain(BRIDGED_ASSET_HUB_ID)].into()
				),
				None,
			),
			Error::<TestRuntime, ()>::BridgeLocations(BridgeLocationsError::DestinationIsLocal),
		);
	});
}

#[test]
fn open_bridge_fails_if_outside_of_bridged_consensus() {
	run_test(|| {
		assert_noop!(
			XcmOverBridge::open_bridge(
				OpenBridgeOrigin::parent_relay_chain_origin(),
				Box::new(
					[
						GlobalConsensus(NonBridgedRelayNetwork::get()),
						Parachain(BRIDGED_ASSET_HUB_ID)
					]
					.into()
				),
				None,
			),
			Error::<TestRuntime, ()>::BridgeLocations(BridgeLocationsError::UnreachableDestination),
		);
	});
}

#[test]
fn open_bridge_fails_if_origin_has_no_sovereign_account() {
	run_test(|| {
		assert_noop!(
			XcmOverBridge::open_bridge(
				OpenBridgeOrigin::origin_without_sovereign_account(),
				Box::new(bridged_asset_hub_universal_location().into()),
				None,
			),
			Error::<TestRuntime, ()>::InvalidBridgeOriginAccount,
		);
	});
}

#[test]
fn open_bridge_fails_if_origin_sovereign_account_has_no_enough_funds() {
	run_test(|| {
		assert_noop!(
			XcmOverBridge::open_bridge(
				OpenBridgeOrigin::sibling_parachain_origin(),
				Box::new(bridged_asset_hub_universal_location().into()),
				None,
			),
			Error::<TestRuntime, ()>::FailedToReserveBridgeDeposit,
		);
	});
}

#[test]
fn open_bridge_fails_if_it_already_exists() {
	run_test(|| {
		let origin = OpenBridgeOrigin::parent_relay_chain_origin();
		let locations = XcmOverBridge::bridge_locations_from_origin(
			origin.clone(),
			Box::new(bridged_asset_hub_universal_location().into()),
		)
		.unwrap();
		let lane_id = locations.calculate_lane_id(xcm::latest::VERSION).unwrap();

		Bridges::<TestRuntime, ()>::insert(
			locations.bridge_id(),
			Bridge {
				bridge_origin_relative_location: Box::new(
					locations.bridge_origin_relative_location().clone().into(),
				),
				bridge_origin_universal_location: Box::new(
					locations.bridge_origin_universal_location().clone().into(),
				),
				bridge_destination_universal_location: Box::new(
					locations.bridge_destination_universal_location().clone().into(),
				),
				state: BridgeState::Opened,
				deposit: None,
				lane_id,
				maybe_notify: None,
			},
		);

		assert_noop!(
			XcmOverBridge::open_bridge(
				origin,
				Box::new(bridged_asset_hub_universal_location().into()),
				None,
			),
			Error::<TestRuntime, ()>::BridgeAlreadyExists,
		);
	})
}

#[test]
fn open_bridge_fails_if_its_lanes_already_exists() {
	run_test(|| {
		let origin = OpenBridgeOrigin::parent_relay_chain_origin();
		let locations = XcmOverBridge::bridge_locations_from_origin(
			origin.clone(),
			Box::new(bridged_asset_hub_universal_location().into()),
		)
		.unwrap();
		let lane_id = locations.calculate_lane_id(xcm::latest::VERSION).unwrap();
		fund_origin_sovereign_account(&locations, BridgeDeposit::get() + ExistentialDeposit::get());

		let lanes_manager = LanesManagerOf::<TestRuntime, ()>::new();

		lanes_manager.create_inbound_lane(lane_id).unwrap();
		assert_noop!(
			XcmOverBridge::open_bridge(
				origin.clone(),
				Box::new(bridged_asset_hub_universal_location().into()),
				None,
			),
			Error::<TestRuntime, ()>::LanesManager(LanesManagerError::InboundLaneAlreadyExists),
		);

		lanes_manager.active_inbound_lane(lane_id).unwrap().purge();
		lanes_manager.create_outbound_lane(lane_id).unwrap();
		assert_noop!(
			XcmOverBridge::open_bridge(
				origin,
				Box::new(bridged_asset_hub_universal_location().into()),
				None,
			),
			Error::<TestRuntime, ()>::LanesManager(LanesManagerError::OutboundLaneAlreadyExists),
		);
	})
}

#[test]
fn open_bridge_works() {
	run_test(|| {
		// in our test runtime, we expect that bridge may be opened by parent relay chain,
		// any sibling parachain or local root
		let origins = [
			(OpenBridgeOrigin::parent_relay_chain_origin(), None),
			(OpenBridgeOrigin::sibling_parachain_origin(), Some(BridgeDeposit::get())),
			(RuntimeOrigin::root(), None),
		];

		// check that every origin may open the bridge
		let lanes_manager = LanesManagerOf::<TestRuntime, ()>::new();
		let existential_deposit = ExistentialDeposit::get();
		for (origin, expected_deposit_amount) in origins {
			// reset events
			System::set_block_number(1);
			System::reset_events();

			// compute all other locations
			let xcm_version = xcm::latest::VERSION;
			let locations = XcmOverBridge::bridge_locations_from_origin(
				origin.clone(),
				Box::new(
					VersionedInteriorLocation::from(bridged_asset_hub_universal_location())
						.into_version(xcm_version)
						.expect("valid conversion"),
				),
			)
			.unwrap();
			let lane_id = locations.calculate_lane_id(xcm_version).unwrap();

			// ensure that there's no bridge and lanes in the storage
			assert_eq!(Bridges::<TestRuntime, ()>::get(locations.bridge_id()), None);
			assert_eq!(
				lanes_manager.active_inbound_lane(lane_id).map(drop),
				Err(LanesManagerError::UnknownInboundLane)
			);
			assert_eq!(
				lanes_manager.active_outbound_lane(lane_id).map(drop),
				Err(LanesManagerError::UnknownOutboundLane)
			);
			assert_eq!(LaneToBridge::<TestRuntime, ()>::get(lane_id), None);

			// give enough funds to the sovereign account of the bridge origin
			let expected_deposit = expected_deposit_amount.map(|deposit_amount| {
				let bridge_owner_account =
					fund_origin_sovereign_account(&locations, deposit_amount + existential_deposit);
				assert_eq!(
					Balances::free_balance(&bridge_owner_account),
					deposit_amount + existential_deposit
				);
				assert_eq!(Balances::reserved_balance(&bridge_owner_account), 0);
				Deposit::new(bridge_owner_account, deposit_amount)
			});

			let maybe_notify = Some(Receiver::new(13, 15));

			// now open the bridge
			assert_ok!(XcmOverBridge::open_bridge(
				origin,
				Box::new(locations.bridge_destination_universal_location().clone().into()),
				maybe_notify.clone(),
			));

			// ensure that everything has been set up in the runtime storage
			assert_eq!(
				Bridges::<TestRuntime, ()>::get(locations.bridge_id()),
				Some(Bridge {
					bridge_origin_relative_location: Box::new(
						locations.bridge_origin_relative_location().clone().into()
					),
					bridge_origin_universal_location: Box::new(
						locations.bridge_origin_universal_location().clone().into(),
					),
					bridge_destination_universal_location: Box::new(
						locations.bridge_destination_universal_location().clone().into(),
					),
					state: BridgeState::Opened,
					deposit: expected_deposit.clone(),
					lane_id,
					maybe_notify,
				}),
			);
			assert_eq!(
				lanes_manager.active_inbound_lane(lane_id).map(|l| l.state()),
				Ok(LaneState::Opened)
			);
			assert_eq!(
				lanes_manager.active_outbound_lane(lane_id).map(|l| l.state()),
				Ok(LaneState::Opened)
			);
			assert_eq!(LaneToBridge::<TestRuntime, ()>::get(lane_id), Some(*locations.bridge_id()));
			if let Some(expected_deposit) = expected_deposit.as_ref() {
				assert_eq!(Balances::free_balance(&expected_deposit.account), existential_deposit);
				assert_eq!(
					Balances::reserved_balance(&expected_deposit.account),
					expected_deposit.amount
				);
			}

			// ensure that the proper event is deposited
			assert_eq!(
				System::events().last(),
				Some(&EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::XcmOverBridge(Event::BridgeOpened {
						bridge_id: *locations.bridge_id(),
						bridge_deposit: expected_deposit,
						local_endpoint: Box::new(
							locations.bridge_origin_universal_location().clone()
						),
						remote_endpoint: Box::new(
							locations.bridge_destination_universal_location().clone()
						),
						lane_id: lane_id.into()
					}),
					topics: vec![],
				}),
			);

			// check state
			assert_ok!(XcmOverBridge::do_try_state());
		}
	});
}

#[test]
fn close_bridge_fails_if_origin_is_not_allowed() {
	run_test(|| {
		assert_noop!(
			XcmOverBridge::close_bridge(
				OpenBridgeOrigin::disallowed_origin(),
				Box::new(bridged_asset_hub_universal_location().into()),
				0,
			),
			sp_runtime::DispatchError::BadOrigin,
		);
	})
}

#[test]
fn close_bridge_fails_if_origin_is_not_relative() {
	run_test(|| {
		assert_noop!(
			XcmOverBridge::close_bridge(
				OpenBridgeOrigin::parent_relay_chain_universal_origin(),
				Box::new(bridged_asset_hub_universal_location().into()),
				0,
			),
			Error::<TestRuntime, ()>::BridgeLocations(BridgeLocationsError::InvalidBridgeOrigin),
		);

		assert_noop!(
			XcmOverBridge::close_bridge(
				OpenBridgeOrigin::sibling_parachain_universal_origin(),
				Box::new(bridged_asset_hub_universal_location().into()),
				0,
			),
			Error::<TestRuntime, ()>::BridgeLocations(BridgeLocationsError::InvalidBridgeOrigin),
		);
	})
}

#[test]
fn close_bridge_fails_if_its_lanes_are_unknown() {
	run_test(|| {
		let origin = OpenBridgeOrigin::parent_relay_chain_origin();
		let (bridge, locations) = mock_open_bridge_from(origin.clone(), None);

		let lanes_manager = LanesManagerOf::<TestRuntime, ()>::new();
		lanes_manager.any_state_inbound_lane(bridge.lane_id).unwrap().purge();
		assert_noop!(
			XcmOverBridge::close_bridge(
				origin.clone(),
				Box::new(locations.bridge_destination_universal_location().clone().into()),
				0,
			),
			Error::<TestRuntime, ()>::LanesManager(LanesManagerError::UnknownInboundLane),
		);
		lanes_manager.any_state_outbound_lane(bridge.lane_id).unwrap().purge();

		let (_, locations) = mock_open_bridge_from(origin.clone(), None);
		lanes_manager.any_state_outbound_lane(bridge.lane_id).unwrap().purge();
		assert_noop!(
			XcmOverBridge::close_bridge(
				origin,
				Box::new(locations.bridge_destination_universal_location().clone().into()),
				0,
			),
			Error::<TestRuntime, ()>::LanesManager(LanesManagerError::UnknownOutboundLane),
		);
	});
}

#[test]
fn close_bridge_works() {
	run_test(|| {
		let origin = OpenBridgeOrigin::parent_relay_chain_origin();
		let expected_deposit = BridgeDeposit::get();
		let (bridge, locations) = mock_open_bridge_from(origin.clone(), Some(expected_deposit));
		System::set_block_number(1);
		let bridge_owner_account = bridge.deposit.unwrap().account;

		// remember owner balances
		let free_balance = Balances::free_balance(&bridge_owner_account);
		let reserved_balance = Balances::reserved_balance(&bridge_owner_account);

		// enqueue some messages
		for _ in 0..32 {
			enqueue_message(bridge.lane_id);
		}

		// now call the `close_bridge`, which will only partially prune messages
		assert_ok!(XcmOverBridge::close_bridge(
			origin.clone(),
			Box::new(locations.bridge_destination_universal_location().clone().into()),
			16,
		),);

		// as a result, the bridge and lanes are switched to the `Closed` state, some messages
		// are pruned, but funds are not unreserved
		let lanes_manager = LanesManagerOf::<TestRuntime, ()>::new();
		assert_eq!(
			Bridges::<TestRuntime, ()>::get(locations.bridge_id()).map(|b| b.state),
			Some(BridgeState::Closed)
		);
		assert_eq!(
			lanes_manager.any_state_inbound_lane(bridge.lane_id).unwrap().state(),
			LaneState::Closed
		);
		assert_eq!(
			lanes_manager.any_state_outbound_lane(bridge.lane_id).unwrap().state(),
			LaneState::Closed
		);
		assert_eq!(
			lanes_manager
				.any_state_outbound_lane(bridge.lane_id)
				.unwrap()
				.queued_messages()
				.checked_len(),
			Some(16)
		);
		assert_eq!(
			LaneToBridge::<TestRuntime, ()>::get(bridge.lane_id),
			Some(*locations.bridge_id())
		);
		assert_eq!(Balances::free_balance(&bridge_owner_account), free_balance);
		assert_eq!(Balances::reserved_balance(&bridge_owner_account), reserved_balance);
		assert_eq!(
			System::events().last(),
			Some(&EventRecord {
				phase: Phase::Initialization,
				event: RuntimeEvent::XcmOverBridge(Event::ClosingBridge {
					bridge_id: *locations.bridge_id(),
					lane_id: bridge.lane_id.into(),
					pruned_messages: 16,
					enqueued_messages: 16,
				}),
				topics: vec![],
			}),
		);

		// now call the `close_bridge` again, which will only partially prune messages
		assert_ok!(XcmOverBridge::close_bridge(
			origin.clone(),
			Box::new(locations.bridge_destination_universal_location().clone().into()),
			8,
		),);

		// nothing is changed (apart from the pruned messages)
		assert_eq!(
			Bridges::<TestRuntime, ()>::get(locations.bridge_id()).map(|b| b.state),
			Some(BridgeState::Closed)
		);
		assert_eq!(
			lanes_manager.any_state_inbound_lane(bridge.lane_id).unwrap().state(),
			LaneState::Closed
		);
		assert_eq!(
			lanes_manager.any_state_outbound_lane(bridge.lane_id).unwrap().state(),
			LaneState::Closed
		);
		assert_eq!(
			lanes_manager
				.any_state_outbound_lane(bridge.lane_id)
				.unwrap()
				.queued_messages()
				.checked_len(),
			Some(8)
		);
		assert_eq!(
			LaneToBridge::<TestRuntime, ()>::get(bridge.lane_id),
			Some(*locations.bridge_id())
		);
		assert_eq!(Balances::free_balance(&bridge_owner_account), free_balance);
		assert_eq!(Balances::reserved_balance(&bridge_owner_account), reserved_balance);
		assert_eq!(
			System::events().last(),
			Some(&EventRecord {
				phase: Phase::Initialization,
				event: RuntimeEvent::XcmOverBridge(Event::ClosingBridge {
					bridge_id: *locations.bridge_id(),
					lane_id: bridge.lane_id.into(),
					pruned_messages: 8,
					enqueued_messages: 8,
				}),
				topics: vec![],
			}),
		);

		// now call the `close_bridge` again that will prune all remaining messages and the
		// bridge
		assert_ok!(XcmOverBridge::close_bridge(
			origin,
			Box::new(locations.bridge_destination_universal_location().clone().into()),
			9,
		),);

		// there's no traces of bridge in the runtime storage and funds are unreserved
		assert_eq!(Bridges::<TestRuntime, ()>::get(locations.bridge_id()).map(|b| b.state), None);
		assert_eq!(
			lanes_manager.any_state_inbound_lane(bridge.lane_id).map(drop),
			Err(LanesManagerError::UnknownInboundLane)
		);
		assert_eq!(
			lanes_manager.any_state_outbound_lane(bridge.lane_id).map(drop),
			Err(LanesManagerError::UnknownOutboundLane)
		);
		assert_eq!(LaneToBridge::<TestRuntime, ()>::get(bridge.lane_id), None);
		assert_eq!(Balances::free_balance(&bridge_owner_account), free_balance + reserved_balance);
		assert_eq!(Balances::reserved_balance(&bridge_owner_account), 0);
		assert_eq!(
			System::events().last(),
			Some(&EventRecord {
				phase: Phase::Initialization,
				event: RuntimeEvent::XcmOverBridge(Event::BridgePruned {
					bridge_id: *locations.bridge_id(),
					lane_id: bridge.lane_id.into(),
					bridge_deposit: Some(Deposit::new(bridge_owner_account, expected_deposit)),
					pruned_messages: 8,
				}),
				topics: vec![],
			}),
		);
	});
}

#[test]
fn update_notification_receiver_works() {
	run_test(|| {
		let origin = OpenBridgeOrigin::parent_relay_chain_origin();
		let locations = XcmOverBridge::bridge_locations_from_origin(
			origin.clone(),
			Box::new(VersionedInteriorLocation::from(bridged_asset_hub_universal_location())),
		)
		.unwrap();

		// open the bridge
		assert_ok!(XcmOverBridge::open_bridge(
			origin.clone(),
			Box::new(locations.bridge_destination_universal_location().clone().into()),
			Some(Receiver::new(13, 15)),
		));
		assert_eq!(
			Bridges::<TestRuntime, ()>::get(locations.bridge_id())
				.map(|b| b.maybe_notify)
				.unwrap(),
			Some(Receiver::new(13, 15))
		);

		// update the notification receiver to `None`
		assert_ok!(XcmOverBridge::update_notification_receiver(
			origin.clone(),
			Box::new(locations.bridge_destination_universal_location().clone().into()),
			None,
		));
		assert_eq!(
			Bridges::<TestRuntime, ()>::get(locations.bridge_id())
				.map(|b| b.maybe_notify)
				.unwrap(),
			None,
		);

		// update the notification receiver to `Some(..)`
		assert_ok!(XcmOverBridge::update_notification_receiver(
			origin.clone(),
			Box::new(locations.bridge_destination_universal_location().clone().into()),
			Some(Receiver::new(29, 43)),
		));
		assert_eq!(
			Bridges::<TestRuntime, ()>::get(locations.bridge_id())
				.map(|b| b.maybe_notify)
				.unwrap(),
			Some(Receiver::new(29, 43))
		);
		// update the notification receiver to `Some(..)`
		assert_ok!(XcmOverBridge::update_notification_receiver(
			origin.clone(),
			Box::new(locations.bridge_destination_universal_location().clone().into()),
			Some(Receiver::new(29, 79)),
		));
		assert_eq!(
			Bridges::<TestRuntime, ()>::get(locations.bridge_id())
				.map(|b| b.maybe_notify)
				.unwrap(),
			Some(Receiver::new(29, 79))
		);
	});
}

#[test]
fn do_try_state_works() {
	let bridge_origin_relative_location = SiblingLocation::get();
	let bridge_origin_universal_location = SiblingUniversalLocation::get();
	let bridge_destination_universal_location = BridgedUniversalDestination::get();
	let bridge_owner_account =
		LocationToAccountId::convert_location(&bridge_origin_relative_location)
			.expect("valid accountId");
	let bridge_owner_account_mismatch =
		LocationToAccountId::convert_location(&Location::parent()).expect("valid accountId");
	let bridge_id =
		BridgeId::new(&bridge_origin_universal_location, &bridge_destination_universal_location);
	let bridge_id_mismatch = BridgeId::new(&InteriorLocation::Here, &InteriorLocation::Here);
	let lane_id = TestLaneIdType::try_new(1, 2).unwrap();
	let lane_id_mismatch = TestLaneIdType::try_new(3, 4).unwrap();

	let test_bridge_state = |id,
	                         bridge,
	                         (lane_id, bridge_id),
	                         (inbound_lane_id, outbound_lane_id),
	                         expected_error: Option<TryRuntimeError>| {
		Bridges::<TestRuntime, ()>::insert(id, bridge);
		LaneToBridge::<TestRuntime, ()>::insert(lane_id, bridge_id);

		let lanes_manager = LanesManagerOf::<TestRuntime, ()>::new();
		lanes_manager.create_inbound_lane(inbound_lane_id).unwrap();
		lanes_manager.create_outbound_lane(outbound_lane_id).unwrap();

		let result = XcmOverBridge::do_try_state();
		if let Some(e) = expected_error {
			assert_err!(result, e);
		} else {
			assert_ok!(result);
		}
	};
	let cleanup = |bridge_id, lane_ids| {
		Bridges::<TestRuntime, ()>::remove(bridge_id);
		for lane_id in lane_ids {
			LaneToBridge::<TestRuntime, ()>::remove(lane_id);
			let lanes_manager = LanesManagerOf::<TestRuntime, ()>::new();
			if let Ok(lane) = lanes_manager.any_state_inbound_lane(lane_id) {
				lane.purge();
			}
			if let Ok(lane) = lanes_manager.any_state_outbound_lane(lane_id) {
				lane.purge();
			}
		}
		assert_ok!(XcmOverBridge::do_try_state());
	};

	run_test(|| {
		// ok state
		test_bridge_state(
			bridge_id,
			Bridge {
				bridge_origin_relative_location: Box::new(VersionedLocation::from(
					bridge_origin_relative_location.clone(),
				)),
				bridge_origin_universal_location: Box::new(VersionedInteriorLocation::from(
					bridge_origin_universal_location.clone(),
				)),
				bridge_destination_universal_location: Box::new(VersionedInteriorLocation::from(
					bridge_destination_universal_location.clone(),
				)),
				state: BridgeState::Opened,
				deposit: Some(Deposit::new(bridge_owner_account.clone(), Zero::zero())),
				lane_id,
				maybe_notify: None,
			},
			(lane_id, bridge_id),
			(lane_id, lane_id),
			None,
		);
		cleanup(bridge_id, vec![lane_id]);

		// error - missing `LaneToBridge` mapping
		test_bridge_state(
			bridge_id,
			Bridge {
				bridge_origin_relative_location: Box::new(VersionedLocation::from(
					bridge_origin_relative_location.clone(),
				)),
				bridge_origin_universal_location: Box::new(VersionedInteriorLocation::from(
					bridge_origin_universal_location.clone(),
				)),
				bridge_destination_universal_location: Box::new(VersionedInteriorLocation::from(
					bridge_destination_universal_location.clone(),
				)),
				state: BridgeState::Opened,
				deposit: Some(Deposit::new(bridge_owner_account.clone(), Zero::zero())),
				lane_id,
				maybe_notify: None,
			},
			(lane_id, bridge_id_mismatch),
			(lane_id, lane_id),
			Some(TryRuntimeError::Other(
				"Found `LaneToBridge` inconsistency for bridge_id - missing mapping!",
			)),
		);
		cleanup(bridge_id, vec![lane_id]);

		// error bridge owner account cannot be calculated
		test_bridge_state(
			bridge_id,
			Bridge {
				bridge_origin_relative_location: Box::new(VersionedLocation::from(
					bridge_origin_relative_location.clone(),
				)),
				bridge_origin_universal_location: Box::new(VersionedInteriorLocation::from(
					bridge_origin_universal_location.clone(),
				)),
				bridge_destination_universal_location: Box::new(VersionedInteriorLocation::from(
					bridge_destination_universal_location.clone(),
				)),
				state: BridgeState::Opened,
				deposit: Some(Deposit::new(bridge_owner_account_mismatch.clone(), Zero::zero())),
				lane_id,
				maybe_notify: None,
			},
			(lane_id, bridge_id),
			(lane_id, lane_id),
			Some(TryRuntimeError::Other("`bridge.deposit.account` is different than calculated from `bridge.bridge_origin_relative_location`, needs migration!")),
		);
		cleanup(bridge_id, vec![lane_id]);

		// error when (bridge_origin_universal_location + bridge_destination_universal_location)
		// produces different `BridgeId`
		test_bridge_state(
			bridge_id_mismatch,
			Bridge {
				bridge_origin_relative_location: Box::new(VersionedLocation::from(
					bridge_origin_relative_location.clone(),
				)),
				bridge_origin_universal_location: Box::new(VersionedInteriorLocation::from(
					bridge_origin_universal_location.clone(),
				)),
				bridge_destination_universal_location: Box::new(VersionedInteriorLocation::from(
					bridge_destination_universal_location.clone(),
				)),
				state: BridgeState::Opened,
				deposit: Some(Deposit::new(bridge_owner_account_mismatch.clone(), Zero::zero())),
				lane_id,
				maybe_notify: None,
			},
			(lane_id, bridge_id_mismatch),
			(lane_id, lane_id),
			Some(TryRuntimeError::Other("`bridge_id` is different than calculated from `bridge_origin_universal_location_as_latest` and `bridge_destination_universal_location_as_latest`, needs migration!")),
		);
		cleanup(bridge_id_mismatch, vec![lane_id]);

		// missing inbound lane for a bridge
		test_bridge_state(
			bridge_id,
			Bridge {
				bridge_origin_relative_location: Box::new(VersionedLocation::from(
					bridge_origin_relative_location.clone(),
				)),
				bridge_origin_universal_location: Box::new(VersionedInteriorLocation::from(
					bridge_origin_universal_location.clone(),
				)),
				bridge_destination_universal_location: Box::new(VersionedInteriorLocation::from(
					bridge_destination_universal_location.clone(),
				)),
				state: BridgeState::Opened,
				deposit: Some(Deposit::new(bridge_owner_account.clone(), Zero::zero())),
				lane_id,
				maybe_notify: None,
			},
			(lane_id, bridge_id),
			(lane_id_mismatch, lane_id),
			Some(TryRuntimeError::Other("Inbound lane not found!")),
		);
		cleanup(bridge_id, vec![lane_id, lane_id_mismatch]);

		// missing outbound lane for a bridge
		test_bridge_state(
			bridge_id,
			Bridge {
				bridge_origin_relative_location: Box::new(VersionedLocation::from(
					bridge_origin_relative_location.clone(),
				)),
				bridge_origin_universal_location: Box::new(VersionedInteriorLocation::from(
					bridge_origin_universal_location.clone(),
				)),
				bridge_destination_universal_location: Box::new(VersionedInteriorLocation::from(
					bridge_destination_universal_location.clone(),
				)),
				state: BridgeState::Opened,
				deposit: Some(Deposit::new(bridge_owner_account.clone(), Zero::zero())),
				lane_id,
				maybe_notify: None,
			},
			(lane_id, bridge_id),
			(lane_id, lane_id_mismatch),
			Some(TryRuntimeError::Other("Outbound lane not found!")),
		);
		cleanup(bridge_id, vec![lane_id, lane_id_mismatch]);

		// ok state with old XCM version
		test_bridge_state(
			bridge_id,
			Bridge {
				bridge_origin_relative_location: Box::new(
					VersionedLocation::from(bridge_origin_relative_location.clone())
						.into_version(XCM_VERSION - 1)
						.unwrap(),
				),
				bridge_origin_universal_location: Box::new(
					VersionedInteriorLocation::from(bridge_origin_universal_location.clone())
						.into_version(XCM_VERSION - 1)
						.unwrap(),
				),
				bridge_destination_universal_location: Box::new(
					VersionedInteriorLocation::from(bridge_destination_universal_location.clone())
						.into_version(XCM_VERSION - 1)
						.unwrap(),
				),
				state: BridgeState::Opened,
				deposit: Some(Deposit::new(bridge_owner_account, Zero::zero())),
				lane_id,
				maybe_notify: None,
			},
			(lane_id, bridge_id),
			(lane_id, lane_id),
			None,
		);
		cleanup(bridge_id, vec![lane_id]);

		// missing bridge for inbound lane
		let lanes_manager = LanesManagerOf::<TestRuntime, ()>::new();
		assert!(lanes_manager.create_inbound_lane(lane_id).is_ok());
		assert_err!(XcmOverBridge::do_try_state(), TryRuntimeError::Other("Found `LaneToBridge` inconsistency for `InboundLanes`'s lane_id - missing mapping!"));
		cleanup(bridge_id, vec![lane_id]);

		// missing bridge for outbound lane
		let lanes_manager = LanesManagerOf::<TestRuntime, ()>::new();
		assert!(lanes_manager.create_outbound_lane(lane_id).is_ok());
		assert_err!(XcmOverBridge::do_try_state(), TryRuntimeError::Other("Found `LaneToBridge` inconsistency for `OutboundLanes`'s lane_id - missing mapping!"));
		cleanup(bridge_id, vec![lane_id]);
	});
}

#[test]
fn ensure_encoding_compatibility() {
	use codec::Encode;

	let bridge_destination_universal_location = BridgedUniversalDestination::get();
	let may_prune_messages = 13;
	let receiver = Receiver::new(13, 15);

	assert_eq!(
		bp_xcm_bridge::XcmBridgeCall::open_bridge {
			bridge_destination_universal_location: Box::new(
				bridge_destination_universal_location.clone().into()
			),
			maybe_notify: Some(receiver.clone()),
		}
		.encode(),
		Call::<TestRuntime, ()>::open_bridge {
			bridge_destination_universal_location: Box::new(
				bridge_destination_universal_location.clone().into()
			),
			maybe_notify: Some(receiver),
		}
		.encode()
	);
	assert_eq!(
		bp_xcm_bridge::XcmBridgeCall::close_bridge {
			bridge_destination_universal_location: Box::new(
				bridge_destination_universal_location.clone().into()
			),
			may_prune_messages,
		}
		.encode(),
		Call::<TestRuntime, ()>::close_bridge {
			bridge_destination_universal_location: Box::new(
				bridge_destination_universal_location.clone().into()
			),
			may_prune_messages,
		}
		.encode()
	);
}
