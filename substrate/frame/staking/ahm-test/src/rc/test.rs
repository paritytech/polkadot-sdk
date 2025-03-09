use crate::rc::mock::*;
use frame::testing_prelude::*;
use pallet_staking_ah_client as ah_client;
use pallet_staking_rc_client::{self as rc_client, SessionReport};

// Tests that are specific to Relay Chain.
#[test]
fn send_session_report_no_election_comes_in() {
	ExtBuilder::default().local_queue().build().execute_with(|| {
		roll_until_matches(|| pallet_session::CurrentIndex::<Runtime>::get() == 10, false);
		assert_eq!(
			LocalQueue::get().unwrap(),
			vec![
				(
					30,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 0,
						validator_points: vec![],
						activation_timestamp: None,
						leftover: false
					})
				),
				(
					60,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 1,
						validator_points: vec![],
						activation_timestamp: None,
						leftover: false
					})
				),
				(
					90,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 2,
						validator_points: vec![],
						activation_timestamp: None,
						leftover: false
					})
				),
				(
					120,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 3,
						validator_points: vec![],
						activation_timestamp: None,
						leftover: false
					})
				),
				(
					150,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 4,
						validator_points: vec![],
						activation_timestamp: None,
						leftover: false
					})
				),
				(
					180,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 5,
						validator_points: vec![],
						activation_timestamp: None,
						leftover: false
					})
				),
				(
					210,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 6,
						validator_points: vec![],
						activation_timestamp: None,
						leftover: false
					})
				),
				(
					240,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 7,
						validator_points: vec![],
						activation_timestamp: None,
						leftover: false
					})
				),
				(
					270,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 8,
						validator_points: vec![],
						activation_timestamp: None,
						leftover: false
					})
				),
				(
					300,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 9,
						validator_points: vec![],
						activation_timestamp: None,
						leftover: false
					})
				)
			]
		);
	})
}

#[test]
fn upon_receiving_election_queue_and_activate_next_session() {}

#[test]
fn prunes_validator_points_upon_session_report() {
	ExtBuilder::default().local_queue().build().execute_with(|| {
		// given
		ah_client::ValidatorPoints::<Runtime>::insert(1, 100);
		ah_client::ValidatorPoints::<Runtime>::insert(2, 200);

		// when
		roll_until_matches(|| pallet_session::CurrentIndex::<Runtime>::get() == 1, false);

		// then
		assert_eq!(
			LocalQueue::get().unwrap(),
			vec![
				(
					30,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 0,
						validator_points: vec![(1, 100), (2, 200)],
						activation_timestamp: None,
						leftover: false
					})
				),
			]
		);

		// then it is drained.
		assert!(!ah_client::ValidatorPoints::<Runtime>::contains_key(1));
		assert!(!ah_client::ValidatorPoints::<Runtime>::contains_key(2));
	})
}

#[test]
fn can_handle_queued_validator_set() {
	ExtBuilder::default().local_queue().build().execute_with(|| {
		let incomplete0 = rc_client::ValidatorSetReport {
			id: 0,
			new_validator_set: vec![1, 2],
			leftover: true,
			prune_up_to: 0,
		};
		let incomplete1 = rc_client::ValidatorSetReport {
			id: 0,
			new_validator_set: vec![3, 4],
			leftover: true,
			prune_up_to: 0,
		};
		let complete = rc_client::ValidatorSetReport {
			id: 0,
			new_validator_set: vec![5, 6],
			leftover: false,
			prune_up_to: 0,
		};

		// nothing is queued.
		assert!(ah_client::IncompleteValidatorSetReport::<Runtime>::get().is_none());
		assert!(ah_client::ValidatorSet::<Runtime>::get().is_none());

		// when
		assert_ok!(StakingAhClient::validator_set(RuntimeOrigin::root(), incomplete0.clone()));

		// then
		assert_eq!(
			ah_client::IncompleteValidatorSetReport::<Runtime>::get().map(|r| r.new_validator_set),
			Some(vec![1, 2])
		);
		assert!(ah_client::ValidatorSet::<Runtime>::get().is_none());

		// when
		assert_ok!(StakingAhClient::validator_set(RuntimeOrigin::root(), incomplete1.clone()));

		// then
		assert_eq!(
			ah_client::IncompleteValidatorSetReport::<Runtime>::get().map(|r| r.new_validator_set),
			Some(vec![1, 2, 3, 4])
		);
		assert!(ah_client::ValidatorSet::<Runtime>::get().is_none());

		// when
		assert_ok!(StakingAhClient::validator_set(RuntimeOrigin::root(), complete.clone()));

		// then
		assert_eq!(ah_client::IncompleteValidatorSetReport::<Runtime>::get(), None);
		assert_eq!(ah_client::ValidatorSet::<Runtime>::get(), Some((0, vec![1, 2, 3, 4, 5, 6])));
	})
}

#[test]
fn incomplete_wrong_id_dropped() {
	ExtBuilder::default().local_queue().build().execute_with(|| {
		let incomplete0 = rc_client::ValidatorSetReport {
			id: 0,
			new_validator_set: vec![1, 2],
			leftover: true,
			prune_up_to: 0,
		};
		let broken = rc_client::ValidatorSetReport {
			id: 1,
			new_validator_set: vec![3, 4],
			leftover: true,
			prune_up_to: 0,
		};

		// nothing is queued.
		assert!(ah_client::IncompleteValidatorSetReport::<Runtime>::get().is_none());
		assert!(ah_client::ValidatorSet::<Runtime>::get().is_none());

		// when
		assert_ok!(StakingAhClient::validator_set(RuntimeOrigin::root(), incomplete0.clone()));

		// then
		assert_eq!(
			ah_client::IncompleteValidatorSetReport::<Runtime>::get().map(|r| r.new_validator_set),
			Some(vec![1, 2])
		);
		assert!(ah_client::ValidatorSet::<Runtime>::get().is_none());

		// when
		assert_ok!(StakingAhClient::validator_set(RuntimeOrigin::root(), broken.clone()));
		// then
		assert_eq!(ah_client::IncompleteValidatorSetReport::<Runtime>::get(), None);
		assert!(ah_client::ValidatorSet::<Runtime>::get().is_none());

		assert_eq!(
			frame_system::Pallet::<Runtime>::read_events_for_pallet::<ah_client::Event<Runtime>>(),
			vec![
				ah_client::Event::ValidatorSetDropped,
			]
		);
	})
}


#[test]

fn sends_offence_report() {
	// todo(ank4n):
	// Test
	// - pre-verification of offence on RC.
	// - disabling of validator in active era.
	// - Dispatch validator offence to AH.
	ExtBuilder::default().local_queue().build().execute_with(|| {});
}
