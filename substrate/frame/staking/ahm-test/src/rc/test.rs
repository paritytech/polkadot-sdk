use crate::rc::mock::*;
use frame::testing_prelude::*;
use pallet_staking_ah_client as ah_client;
use pallet_staking_rc_client::{self as rc_client, SessionReport, ValidatorSetReport};

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
fn prunes_old_validator_set_trie() {
	// roll forward and store the trie of a number of validators
	// receive a message that we must now prune a part of them
}

#[test]
fn upon_receiving_election_queue_and_activate_next_session() {
	ExtBuilder::default()
		.session_keys(vec![1, 2, 3, 4, 5])
		.local_queue()
		.build()
		.execute_with(|| {
			// roll 3 sessions, and then validator set comes
			roll_until_matches(|| pallet_session::CurrentIndex::<Runtime>::get() == 3, false);

			// current session validators are:
			assert!(pallet_session::Validators::<Runtime>::get().is_empty());
			assert_eq!(pallet_session::QueuedChanged::<Runtime>::get(), false);
			assert!(pallet_session::QueuedKeys::<Runtime>::get().is_empty());

			// new validator set comes in.
			let report = ValidatorSetReport {
				id: 1,
				prune_up_to: 0,
				leftover: false,
				new_validator_set: vec![1, 2, 3, 4],
			};

			assert_ok!(ah_client::Pallet::<Runtime>::validator_set(RuntimeOrigin::root(), report));

			// session validators are not set yet.
			assert!(pallet_session::Validators::<Runtime>::get().is_empty());
			assert_eq!(pallet_session::QueuedChanged::<Runtime>::get(), false);
			assert!(pallet_session::QueuedKeys::<Runtime>::get().is_empty());

			// rotate one more session
			roll_until_matches(|| pallet_session::CurrentIndex::<Runtime>::get() == 4, false);
			// current validators are still the same
			assert!(pallet_session::Validators::<Runtime>::get().is_empty());
			// queued has changed
			assert_eq!(pallet_session::QueuedChanged::<Runtime>::get(), true);
			assert_eq!(
				pallet_session::QueuedKeys::<Runtime>::get()
					.into_iter()
					.map(|(x, _)| x)
					.collect::<Vec<_>>(),
				vec![1, 2, 3, 4]
			);

			// rotate one more session
			roll_until_matches(|| pallet_session::CurrentIndex::<Runtime>::get() == 5, false);
			// current validators have changed
			assert_eq!(pallet_session::Validators::<Runtime>::get(), vec![1, 2, 3, 4]);
			// queued is back to normal
			assert_eq!(pallet_session::QueuedChanged::<Runtime>::get(), false);
			assert_eq!(
				pallet_session::QueuedKeys::<Runtime>::get()
					.into_iter()
					.map(|(x, _)| x)
					.collect::<Vec<_>>(),
				vec![1, 2, 3, 4]
			);

			// send another report, this time remove 4 and replace with 5
			// new validator set comes in.
			let report = ValidatorSetReport {
				id: 2,
				prune_up_to: 0,
				leftover: false,
				new_validator_set: vec![1, 2, 3, 5],
			};
			assert_ok!(ah_client::Pallet::<Runtime>::validator_set(RuntimeOrigin::root(), report));

			// rotate one more session
			roll_until_matches(|| pallet_session::CurrentIndex::<Runtime>::get() == 6, false);

			// current validators not changed
			assert_eq!(pallet_session::Validators::<Runtime>::get(), vec![1, 2, 3, 4]);
			// queued is set -- notice 5 is queued but not in the current set
			assert_eq!(pallet_session::QueuedChanged::<Runtime>::get(), true);
			assert_eq!(
				pallet_session::QueuedKeys::<Runtime>::get()
				.into_iter()
				.map(|(x, _)| x)
				.collect::<Vec<_>>(),
				vec![1, 2, 3, 5]
			);

			// rotate one more session
			roll_until_matches(|| pallet_session::CurrentIndex::<Runtime>::get() == 7, false);

			// current validators changed
			assert_eq!(pallet_session::Validators::<Runtime>::get(), vec![1, 2, 3, 5]);
			assert_eq!(pallet_session::QueuedChanged::<Runtime>::get(), false);
			assert_eq!(
				pallet_session::QueuedKeys::<Runtime>::get()
				.into_iter()
				.map(|(x, _)| x)
				.collect::<Vec<_>>(),
				vec![1, 2, 3, 5]
			);
		})
}

#[test]
fn drops_too_small_validator_set() {
	// when a splitted message is too small, we will not process it
}

#[test]
fn splitted_drops_too_small_validator_set() {
	// when a splitted message is too small, we will not process it, and clear any previous queue
}

#[test]
fn drops_incoming_if_blocked() {}

#[test]
fn drops_outgoing_if_blocked() {}

#[test]
fn can_split_and_merge_session_report() {
	ExtBuilder::default().local_queue().build().execute_with(|| {
		let full_report = SessionReport {
			activation_timestamp: None,
			end_index: 0,
			leftover: false,
			validator_points: vec![(1, 1), (2, 2), (3, 3), (4, 4), (5, 5)],
		};

		// Split by 0
		assert_eq!(
			full_report
				.clone()
				.split(0)
				.into_iter()
				.map(|r| r.validator_points)
				.collect::<Vec<_>>(),
			vec![vec![(1, 1)], vec![(2, 2)], vec![(3, 3)], vec![(4, 4)], vec![(5, 5)]]
		);

		// Split by 1 is noop
		assert_eq!(
			full_report
				.clone()
				.split(1)
				.into_iter()
				.map(|r| r.validator_points)
				.collect::<Vec<_>>(),
			vec![vec![(1, 1)], vec![(2, 2)], vec![(3, 3)], vec![(4, 4)], vec![(5, 5)]]
		);

		// split by 2
		assert_eq!(
			full_report
				.clone()
				.split(2)
				.into_iter()
				.map(|r| r.validator_points)
				.collect::<Vec<_>>(),
			vec![vec![(1, 1), (2, 2)], vec![(3, 3), (4, 4)], vec![(5, 5)]]
		);

		// split by 3
		assert_eq!(
			full_report
				.clone()
				.split(3)
				.into_iter()
				.map(|r| r.validator_points)
				.collect::<Vec<_>>(),
			vec![vec![(1, 1), (2, 2), (3, 3)], vec![(4, 4), (5, 5)]]
		);

		// split by 4
		assert_eq!(
			full_report
				.clone()
				.split(4)
				.into_iter()
				.map(|r| r.validator_points)
				.collect::<Vec<_>>(),
			vec![vec![(1, 1), (2, 2), (3, 3), (4, 4)], vec![(5, 5)]]
		);

		// split by 5
		assert_eq!(
			full_report
				.clone()
				.split(5)
				.into_iter()
				.map(|r| r.validator_points)
				.collect::<Vec<_>>(),
			vec![vec![(1, 1), (2, 2), (3, 3), (4, 4), (5, 5)]]
		);

		// split by 6
		assert_eq!(
			full_report
				.clone()
				.split(6)
				.into_iter()
				.map(|r| r.validator_points)
				.collect::<Vec<_>>(),
			vec![vec![(1, 1), (2, 2), (3, 3), (4, 4), (5, 5)]]
		);
	})
}

#[test]
fn can_split_and_merge_validator_set_report() {
	ExtBuilder::default().local_queue().build().execute_with(|| {
		let full_report = ValidatorSetReport {
			new_validator_set: vec![1, 2, 3, 4, 5],
			id: 0,
			prune_up_to: 0,
			leftover: false,
		};

		// Split by 0
		assert_eq!(
			full_report
				.clone()
				.split(0)
				.into_iter()
				.map(|r| r.new_validator_set)
				.collect::<Vec<_>>(),
			vec![vec![1], vec![2], vec![3], vec![4], vec![5]]
		);

		// Split by 1 is noop
		assert_eq!(
			full_report
				.clone()
				.split(1)
				.into_iter()
				.map(|r| r.new_validator_set)
				.collect::<Vec<_>>(),
			vec![vec![1], vec![2], vec![3], vec![4], vec![5]]
		);

		// split by 2
		assert_eq!(
			full_report
				.clone()
				.split(2)
				.into_iter()
				.map(|r| r.new_validator_set)
				.collect::<Vec<_>>(),
			vec![vec![1, 2], vec![3, 4], vec![5]]
		);

		// split by 3
		assert_eq!(
			full_report
				.clone()
				.split(3)
				.into_iter()
				.map(|r| r.new_validator_set)
				.collect::<Vec<_>>(),
			vec![vec![1, 2, 3], vec![4, 5]]
		);

		// split by 4
		assert_eq!(
			full_report
				.clone()
				.split(4)
				.into_iter()
				.map(|r| r.new_validator_set)
				.collect::<Vec<_>>(),
			vec![vec![1, 2, 3, 4], vec![5]]
		);

		// split by 5
		assert_eq!(
			full_report
				.clone()
				.split(5)
				.into_iter()
				.map(|r| r.new_validator_set)
				.collect::<Vec<_>>(),
			vec![vec![1, 2, 3, 4, 5]]
		);

		// split by 6
		assert_eq!(
			full_report
				.clone()
				.split(6)
				.into_iter()
				.map(|r| r.new_validator_set)
				.collect::<Vec<_>>(),
			vec![vec![1, 2, 3, 4, 5]]
		);
	})
}

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
			vec![(
				30,
				OutgoingMessages::SessionReport(SessionReport {
					end_index: 0,
					validator_points: vec![(1, 100), (2, 200)],
					activation_timestamp: None,
					leftover: false
				})
			),]
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
			vec![ah_client::Event::ValidatorSetDropped,]
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
