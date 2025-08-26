// Copyright (C) Parity Technologies (UK) Ltd.
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
use bp_xcm_bridge_router::MINIMAL_DELIVERY_FEE_FACTOR;
use frame_support::assert_ok;
use mock::*;

use frame_system::{EventRecord, Phase};
use sp_runtime::traits::Dispatchable;

#[test]
fn not_applicable_if_destination_is_within_other_network() {
	run_test(|| {
		// unroutable dest
		let dest = Location::new(2, [GlobalConsensus(ByGenesis([0; 32])), Parachain(1000)]);
		let xcm: Xcm<()> = vec![ClearOrigin].into();

		// check that router does not consume when `NotApplicable`
		let mut xcm_wrapper = Some(xcm.clone());
		assert_eq!(
			XcmBridgeHubRouter::validate(&mut Some(dest.clone()), &mut xcm_wrapper),
			Err(SendError::NotApplicable),
		);
		// XCM is NOT consumed and untouched
		assert_eq!(Some(xcm.clone()), xcm_wrapper);

		// check the full `send_xcm`
		assert_eq!(send_xcm::<XcmBridgeHubRouter>(dest, xcm,), Err(SendError::NotApplicable),);
	});
}

#[test]
fn exceeds_max_message_size_if_size_is_above_hard_limit() {
	run_test(|| {
		// routable dest with XCM version
		let dest = Location::new(2, [GlobalConsensus(BridgedNetworkId::get()), Parachain(1000)]);
		// oversized XCM
		let xcm: Xcm<()> = vec![ClearOrigin; HARD_MESSAGE_SIZE_LIMIT as usize].into();

		// dest is routable with the inner router
		assert_ok!(<TestRuntime as Config<()>>::MessageExporter::validate(
			&mut Some(dest.clone()),
			&mut Some(xcm.clone())
		));

		// check for oversized message
		let mut xcm_wrapper = Some(xcm.clone());
		assert_eq!(
			XcmBridgeHubRouter::validate(&mut Some(dest.clone()), &mut xcm_wrapper),
			Err(SendError::ExceedsMaxMessageSize),
		);
		// XCM is consumed by the inner router
		assert!(xcm_wrapper.is_none());

		// check the full `send_xcm`
		assert_eq!(
			send_xcm::<XcmBridgeHubRouter>(dest, xcm,),
			Err(SendError::ExceedsMaxMessageSize),
		);
	});
}

#[test]
fn destination_unsupported_if_wrap_version_fails() {
	run_test(|| {
		// routable dest but we don't know XCM version
		let dest = UnknownXcmVersionForRoutableLocation::get();
		let xcm: Xcm<()> = vec![ClearOrigin].into();

		// dest is routable with the inner router
		assert_ok!(<TestRuntime as Config<()>>::MessageExporter::validate(
			&mut Some(dest.clone()),
			&mut Some(xcm.clone())
		));

		// check that it does not pass XCM version check
		let mut xcm_wrapper = Some(xcm.clone());
		assert_eq!(
			XcmBridgeHubRouter::validate(&mut Some(dest.clone()), &mut xcm_wrapper),
			Err(SendError::DestinationUnsupported),
		);
		// XCM is consumed by the inner router
		assert!(xcm_wrapper.is_none());

		// check the full `send_xcm`
		assert_eq!(
			send_xcm::<XcmBridgeHubRouter>(dest, xcm,),
			Err(SendError::DestinationUnsupported),
		);
	});
}

#[test]
fn returns_proper_delivery_price() {
	run_test(|| {
		let dest = Location::new(2, [GlobalConsensus(BridgedNetworkId::get())]);
		let xcm: Xcm<()> = vec![ClearOrigin].into();
		let msg_size = xcm.encoded_size();

		// `BASE_FEE + BYTE_FEE * msg_size` (without `HRMP_FEE`)
		let base_cost_formula = || BASE_FEE + BYTE_FEE * (msg_size as u128);

		// initially the base fee is used
		let expected_fee = base_cost_formula() + HRMP_FEE;
		assert_eq!(
			XcmBridgeHubRouter::validate(&mut Some(dest.clone()), &mut Some(xcm.clone()))
				.unwrap()
				.1
				.get(0),
			Some(&(BridgeFeeAsset::get(), expected_fee).into()),
		);

		// but when factor is larger than one, it increases the fee, so it becomes:
		// `base_cost_formula() * F`
		let factor = FixedU128::from_rational(125, 100);

		// make bridge congested + update fee factor
		set_bridge_state_for::<TestRuntime, ()>(
			&dest,
			Some(BridgeState { delivery_fee_factor: factor, is_congested: true }),
		);

		let expected_fee = (FixedU128::saturating_from_integer(base_cost_formula()) * factor)
			.into_inner() /
			FixedU128::DIV +
			HRMP_FEE;
		assert_eq!(
			XcmBridgeHubRouter::validate(&mut Some(dest), &mut Some(xcm)).unwrap().1.get(0),
			Some(&(BridgeFeeAsset::get(), expected_fee).into()),
		);
	});
}

#[test]
fn sent_message_doesnt_increase_factor_if_bridge_is_uncongested() {
	run_test(|| {
		let dest = Location::new(2, [GlobalConsensus(BridgedNetworkId::get()), Parachain(1000)]);

		// bridge not congested
		let old_delivery_fee_factor = FixedU128::from_rational(125, 100);
		set_bridge_state_for::<TestRuntime, ()>(
			&dest,
			Some(BridgeState { delivery_fee_factor: old_delivery_fee_factor, is_congested: false }),
		);

		assert_eq!(
			send_xcm::<XcmBridgeHubRouter>(dest.clone(), vec![ClearOrigin].into(),).map(drop),
			Ok(()),
		);

		assert!(TestXcmRouter::is_message_sent());
		assert_eq!(
			old_delivery_fee_factor,
			get_bridge_state_for::<TestRuntime, ()>(&dest).delivery_fee_factor
		);

		assert_eq!(System::events(), vec![]);
	});
}

#[test]
fn sent_message_increases_factor_if_bridge_is_congested() {
	run_test(|| {
		let dest = Location::new(2, [GlobalConsensus(BridgedNetworkId::get()), Parachain(1000)]);

		// make bridge congested + update fee factor
		let old_delivery_fee_factor = FixedU128::from_rational(125, 100);
		set_bridge_state_for::<TestRuntime, ()>(
			&dest,
			Some(BridgeState { delivery_fee_factor: old_delivery_fee_factor, is_congested: true }),
		);

		assert_ok!(
			send_xcm::<XcmBridgeHubRouter>(dest.clone(), vec![ClearOrigin].into(),).map(drop)
		);

		assert!(TestXcmRouter::is_message_sent());
		let _delivery_fee_factor =
			get_bridge_state_for::<TestRuntime, ()>(&dest).delivery_fee_factor;
		assert!(old_delivery_fee_factor < _delivery_fee_factor);

		// check emitted event
		let first_system_event = System::events().first().cloned();
		let _previous_value_ = old_delivery_fee_factor;
		assert!(matches!(
			first_system_event,
			Some(EventRecord {
				phase: Phase::Initialization,
				event: RuntimeEvent::XcmBridgeHubRouter(Event::DeliveryFeeFactorUpdated {
					previous_value: _previous_value,
					new_value: _delivery_fee_factor,
					..
				}),
				..
			})
		));
	});
}

#[test]
fn get_messages_does_not_return_anything() {
	run_test(|| {
		assert_ok!(send_xcm::<XcmBridgeHubRouter>(
			(Parent, Parent, GlobalConsensus(BridgedNetworkId::get()), Parachain(1000)).into(),
			vec![ClearOrigin].into()
		));
		assert_eq!(XcmBridgeHubRouter::get_messages(), vec![]);
	});
}

#[test]
fn update_bridge_status_works() {
	run_test(|| {
		let dest = Location::new(2, [GlobalConsensus(BridgedNetworkId::get()), Parachain(1000)]);
		let bridge_id = bp_xcm_bridge::BridgeId::new(&UniversalLocation::get(), dest.interior());
		let update_bridge_status = |bridge_id, is_congested| {
			let call = RuntimeCall::XcmBridgeHubRouter(Call::update_bridge_status {
				bridge_id,
				is_congested,
			});
			assert_ok!(call.dispatch(RuntimeOrigin::root()));
		};

		assert_eq!(
			get_bridge_state_for::<TestRuntime, ()>(&dest),
			BridgeState { delivery_fee_factor: MINIMAL_DELIVERY_FEE_FACTOR, is_congested: false }
		);

		// make congested
		update_bridge_status(bridge_id, true);
		assert_eq!(
			get_bridge_state_for::<TestRuntime, ()>(&dest),
			BridgeState { delivery_fee_factor: MINIMAL_DELIVERY_FEE_FACTOR, is_congested: true }
		);

		// make uncongested
		update_bridge_status(bridge_id, false);
		assert_eq!(
			get_bridge_state_for::<TestRuntime, ()>(&dest),
			BridgeState { delivery_fee_factor: MINIMAL_DELIVERY_FEE_FACTOR, is_congested: false }
		);
	});
}

#[test]
fn do_update_bridge_status_works() {
	run_test(|| {
		let dest = Location::new(2, [GlobalConsensus(BridgedNetworkId::get())]);
		let bridge_id = bp_xcm_bridge::BridgeId::new(&UniversalLocation::get(), dest.interior());
		// by default is_congested is false
		assert_eq!(
			get_bridge_state_for::<TestRuntime, ()>(&dest),
			BridgeState { delivery_fee_factor: MINIMAL_DELIVERY_FEE_FACTOR, is_congested: false }
		);

		// update as is_congested=true
		Pallet::<TestRuntime, ()>::do_update_bridge_status(bridge_id, true);
		assert_eq!(
			get_bridge_state_for::<TestRuntime, ()>(&dest),
			BridgeState { delivery_fee_factor: MINIMAL_DELIVERY_FEE_FACTOR, is_congested: true }
		);

		// increase fee factor when congested
		Pallet::<TestRuntime, ()>::on_message_sent_to(5, dest.clone());
		assert!(
			get_bridge_state_for::<TestRuntime, ()>(&dest).delivery_fee_factor >
				MINIMAL_DELIVERY_FEE_FACTOR
		);
		// update as is_congested=true - should not reset fee factor
		Pallet::<TestRuntime, ()>::do_update_bridge_status(bridge_id, true);
		assert!(
			get_bridge_state_for::<TestRuntime, ()>(&dest).delivery_fee_factor >
				MINIMAL_DELIVERY_FEE_FACTOR
		);

		// update as is_congested=false when `Some(..)`
		Pallet::<TestRuntime, ()>::do_update_bridge_status(bridge_id, false);
		assert_eq!(
			get_bridge_state_for::<TestRuntime, ()>(&dest),
			BridgeState { delivery_fee_factor: MINIMAL_DELIVERY_FEE_FACTOR, is_congested: false }
		);
	})
}
