// This file is part of Substrate.

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

use crate::rc::mock::*;
use frame::testing_prelude::*;
use pallet_staking_async_ah_client::{self as ah_client, Mode, OperatingMode};
use pallet_staking_async_rc_client::{
	self as rc_client, Offence, SessionReport, ValidatorSetReport,
};

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
						validator_points: vec![(11, 580)],
						activation_timestamp: None,
						leftover: false
					})
				),
				(
					60,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 1,
						validator_points: vec![(11, 600)],
						activation_timestamp: None,
						leftover: false
					})
				),
				(
					90,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 2,
						validator_points: vec![(11, 600)],
						activation_timestamp: None,
						leftover: false
					})
				),
				(
					120,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 3,
						validator_points: vec![(11, 600)],
						activation_timestamp: None,
						leftover: false
					})
				),
				(
					150,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 4,
						validator_points: vec![(11, 600)],
						activation_timestamp: None,
						leftover: false
					})
				),
				(
					180,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 5,
						validator_points: vec![(11, 600)],
						activation_timestamp: None,
						leftover: false
					})
				),
				(
					210,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 6,
						validator_points: vec![(11, 600)],
						activation_timestamp: None,
						leftover: false
					})
				),
				(
					240,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 7,
						validator_points: vec![(11, 600)],
						activation_timestamp: None,
						leftover: false
					})
				),
				(
					270,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 8,
						validator_points: vec![(11, 600)],
						activation_timestamp: None,
						leftover: false
					})
				),
				(
					300,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 9,
						validator_points: vec![(11, 600)],
						activation_timestamp: None,
						leftover: false
					})
				)
			]
		);
	})
}

#[test]
fn upon_receiving_election_queue_and_activate_next_session() {
	ExtBuilder::default()
		.session_keys(vec![1, 2, 3, 4, 5])
		.local_queue()
		.no_default_author()
		.build()
		.execute_with(|| {
			// roll 3 sessions, and then validator set comes
			roll_until_matches(|| pallet_session::CurrentIndex::<Runtime>::get() == 3, false);

			assert_eq!(
				session_events_since_last_call(),
				vec![
					pallet_session::Event::<Runtime>::NewSession { session_index: 1 },
					pallet_session::Event::<Runtime>::NewSession { session_index: 2 },
					pallet_session::Event::<Runtime>::NewSession { session_index: 3 }
				]
			);

			assert_eq!(
				LocalQueue::get_since_last_call(),
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
				]
			);

			// current session validators are:
			assert!(pallet_session::Validators::<Runtime>::get().is_empty());
			assert_eq!(pallet_session::QueuedChanged::<Runtime>::get(), false);
			assert!(pallet_session::QueuedKeys::<Runtime>::get().is_empty());

			// new validator set comes in.
			let report = ValidatorSetReport {
				id: 1,
				prune_up_to: None,
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

			assert_eq!(
				session_events_since_last_call(),
				vec![
					pallet_session::Event::<Runtime>::NewQueued,
					pallet_session::Event::<Runtime>::NewSession { session_index: 4 },
				]
			);

			assert_eq!(
				LocalQueue::get_since_last_call(),
				vec![(
					120,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 3,
						validator_points: vec![],
						activation_timestamp: None,
						leftover: false
					})
				)]
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
			assert_eq!(
				session_events_since_last_call(),
				vec![pallet_session::Event::<Runtime>::NewSession { session_index: 5 },]
			);

			assert_eq!(
				LocalQueue::get_since_last_call(),
				vec![(
					150,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 4,
						validator_points: vec![],
						activation_timestamp: Some((150000, 1)),
						leftover: false
					})
				),]
			);

			// send another report, this time remove 4 and replace with 5
			// new validator set comes in.
			let report = ValidatorSetReport {
				id: 2,
				prune_up_to: None,
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
			assert_eq!(
				session_events_since_last_call(),
				vec![
					pallet_session::Event::<Runtime>::NewQueued,
					pallet_session::Event::<Runtime>::NewSession { session_index: 6 },
				]
			);

			assert_eq!(
				LocalQueue::get_since_last_call(),
				vec![(
					180,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 5,
						validator_points: vec![],
						activation_timestamp: None,
						leftover: false
					})
				)],
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
			assert_eq!(
				session_events_since_last_call(),
				vec![pallet_session::Event::<Runtime>::NewSession { session_index: 7 }]
			);

			assert_eq!(
				LocalQueue::get_since_last_call(),
				vec![(
					210,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 6,
						validator_points: vec![],
						activation_timestamp: Some((210000, 2)),
						leftover: false
					})
				)]
			);
		})
}

#[test]
fn cleans_validator_points_upon_session_report() {
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
					// first two are inserted by us, the other one by the test mock
					validator_points: vec![(1, 100), (2, 200), (11, 580)],
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
fn drops_too_small_validator_set() {
	ExtBuilder::default().local_queue().build().execute_with(|| {
		assert_eq!(MinimumValidatorSetSize::get(), 4);
		let report = ValidatorSetReport {
			id: 1,
			prune_up_to: None,
			leftover: false,
			new_validator_set: vec![1],
		};

		// This will raise okay, but nothing is queued, and event is emitted
		assert_ok!(ah_client::Pallet::<Runtime>::validator_set(RuntimeOrigin::root(), report),);
		assert_eq!(
			ah_client_events_since_last_call(),
			vec![ah_client::Event::SetTooSmallAndDropped,]
		);

		assert!(ah_client::ValidatorSet::<Runtime>::get().is_none());
		assert!(ah_client::IncompleteValidatorSetReport::<Runtime>::get().is_none());
	})
}

#[test]
fn splitted_drops_too_small_validator_set() {
	ExtBuilder::default().local_queue().build().execute_with(|| {
		let parts = ValidatorSetReport {
			id: 1,
			prune_up_to: None,
			leftover: true,
			new_validator_set: vec![1, 2],
		}
		.split(1);
		assert_eq!(parts.len(), 2);

		assert_ok!(ah_client::Pallet::<Runtime>::validator_set(
			RuntimeOrigin::root(),
			parts[0].clone()
		));

		assert_eq!(
			ah_client_events_since_last_call(),
			vec![ah_client::Event::ValidatorSetReceived {
				id: 1,
				new_validator_set_count: 1,
				prune_up_to: None,
				leftover: true
			}]
		);
		assert_eq!(ah_client::ValidatorSet::<Runtime>::get(), None);
		assert!(ah_client::IncompleteValidatorSetReport::<Runtime>::get().is_some());

		assert_ok!(ah_client::Pallet::<Runtime>::validator_set(
			RuntimeOrigin::root(),
			parts[1].clone()
		));

		assert_eq!(
			ah_client_events_since_last_call(),
			vec![ah_client::Event::SetTooSmallAndDropped]
		);
		assert_eq!(ah_client::ValidatorSet::<Runtime>::get(), None);
		assert!(ah_client::IncompleteValidatorSetReport::<Runtime>::get().is_none());
	})
}

#[test]
fn on_offence_non_validator() {
	ExtBuilder::default()
		.local_queue()
		.session_keys(vec![1, 2, 3, 4])
		.build()
		.execute_with(|| {
			receive_validator_set_at(3, 1, vec![1, 2, 3, 4], true);
			assert_eq!(pallet_session::CurrentIndex::<Runtime>::get(), 5);

			// flush some relevant data
			LocalQueue::flush();
			let _ = session_events_since_last_call();

			// submit an offence for validator 5 in current session, which is not a validator
			// really. Note that we have to provide a manual identification, as the default one
			// won't work here.
			assert_ok!(pallet_root_offences::Pallet::<Runtime>::create_offence(
				RuntimeOrigin::root(),
				vec![(5, Perbill::from_percent(50))],
				Some(vec![Default::default()]),
				None
			));

			// we nonetheless have sent the offence report to AH
			assert_eq!(
				LocalQueue::get_since_last_call(),
				vec![(
					150,
					OutgoingMessages::OffenceReport(
						5,
						vec![Offence {
							offender: 5,
							reporters: vec![],
							slash_fraction: Perbill::from_percent(50)
						}]
					)
				)]
			);

			// no disabling has happened in session
			assert_eq!(session_events_since_last_call(), vec![]);
		})
}

#[test]
fn on_offence_non_validator_and_active() {
	ExtBuilder::default()
		.local_queue()
		.session_keys(vec![1, 2, 3, 4])
		.build()
		.execute_with(|| {
			receive_validator_set_at(3, 1, vec![1, 2, 3, 4], true);
			assert_eq!(pallet_session::CurrentIndex::<Runtime>::get(), 5);

			// flush some relevant data
			LocalQueue::flush();
			let _ = session_events_since_last_call();

			// submit an offence for 5 and 4, first a non-validator and second an active one.
			assert_ok!(pallet_root_offences::Pallet::<Runtime>::create_offence(
				RuntimeOrigin::root(),
				vec![(4, Perbill::from_percent(50)), (5, Perbill::from_percent(50))],
				Some(vec![Default::default(), Default::default()]),
				None
			));

			// we nonetheless have sent the offence report to AH
			assert_eq!(
				LocalQueue::get_since_last_call(),
				vec![(
					150,
					OutgoingMessages::OffenceReport(
						5,
						vec![
							Offence {
								offender: 4,
								reporters: vec![],
								slash_fraction: Perbill::from_percent(50)
							},
							Offence {
								offender: 5,
								reporters: vec![],
								slash_fraction: Perbill::from_percent(50)
							}
						]
					)
				)]
			);

			// one validator has been disabled in session
			assert_eq!(
				session_events_since_last_call(),
				vec![pallet_session::Event::ValidatorDisabled { validator: 4 }]
			);
		})
}

#[test]
fn wont_disable_past_session_offence() {
	ExtBuilder::default()
		.local_queue()
		.session_keys(vec![1, 2, 3, 4])
		.minimum_validator_set_size(1)
		.build()
		.execute_with(|| {
			// receive 1, 2 at 3, activate them
			receive_validator_set_at(3, 1, vec![1, 2], true);
			assert_eq!(pallet_session::CurrentIndex::<Runtime>::get(), 5);

			// receive 3, 4 at 6, activate them
			receive_validator_set_at(6, 2, vec![3, 4], true);
			assert_eq!(pallet_session::CurrentIndex::<Runtime>::get(), 8);

			// flush some relevant data
			LocalQueue::flush();
			let _ = session_events_since_last_call();

			// submit an offence for 1, who is a past validator, in a past session.
			assert_ok!(pallet_root_offences::Pallet::<Runtime>::create_offence(
				RuntimeOrigin::root(),
				vec![(1, Perbill::from_percent(50))],
				Some(vec![Default::default()]),
				Some(5)
			));

			// we nonetheless have sent the offence report to AH
			assert_eq!(
				LocalQueue::get_since_last_call(),
				vec![(
					240,
					OutgoingMessages::OffenceReport(
						5,
						vec![Offence {
							offender: 1,
							reporters: vec![],
							slash_fraction: Perbill::from_percent(50)
						},]
					)
				)]
			);

			// no one disabled in session
			assert_eq!(session_events_since_last_call(), vec![]);
		})
}

#[test]
fn on_offence_disable_and_re_enabled_next_set() {
	ExtBuilder::default()
		.local_queue()
		.session_keys(vec![1, 2, 3, 4])
		.build()
		.execute_with(|| {
			receive_validator_set_at(3, 1, vec![1, 2, 3, 4], true);
			assert_eq!(pallet_session::CurrentIndex::<Runtime>::get(), 5);

			// flush some relevant data
			LocalQueue::flush();
			let _ = session_events_since_last_call();

			// submit an offence for 4 in the current session
			assert_ok!(pallet_root_offences::Pallet::<Runtime>::create_offence(
				RuntimeOrigin::root(),
				vec![(4, Perbill::from_percent(50))],
				Some(vec![Default::default()]),
				None
			));

			// offence dispatched to AH
			assert_eq!(
				LocalQueue::get_since_last_call(),
				vec![(
					150,
					OutgoingMessages::OffenceReport(
						5,
						vec![Offence {
							offender: 4,
							reporters: vec![],
							slash_fraction: Perbill::from_percent(50)
						},]
					)
				)]
			);

			// session disables 4
			assert_eq!(
				session_events_since_last_call(),
				vec![pallet_session::Event::ValidatorDisabled { validator: 4 }]
			);
			assert_eq!(
				pallet_session::DisabledValidators::<Runtime>::get()
					.into_iter()
					.map(|(x, _)| x)
					.collect::<Vec<_>>(),
				vec![3]
			);

			// now receive the same validator set, again
			receive_validator_set_at(6, 2, vec![1, 2, 3, 4], true);
			assert_eq!(pallet_session::CurrentIndex::<Runtime>::get(), 8);

			// events related to session rotation
			assert_eq!(
				session_events_since_last_call(),
				vec![
					pallet_session::Event::NewSession { session_index: 6 },
					pallet_session::Event::NewQueued,
					pallet_session::Event::NewSession { session_index: 7 },
					pallet_session::Event::NewSession { session_index: 8 }
				]
			);

			// disabled validators is now gone
			assert!(pallet_session::DisabledValidators::<Runtime>::get().is_empty());
		});
}

mod session_pruning {
	use super::*;

	#[test]
	fn stores_and_prunes_old_validator_set_trie() {
		ExtBuilder::default()
			.session_keys((1..100).collect::<Vec<_>>())
			.local_queue()
			.build()
			.execute_with(|| {
				// initially, no historical data
				assert_eq!(pallet_session::historical::StoredRange::<T>::get(), None);

				// forward 10 sessions, and each one set 10 different validators
				for i in 1..=10 {
					let session_validators =
						(i * 10..(i + 1) * 10).map(|x| x as AccountId).collect::<Vec<_>>();
					assert_ok!(ah_client::Pallet::<T>::validator_set(
						RuntimeOrigin::root(),
						ValidatorSetReport {
							id: i,
							prune_up_to: None,
							leftover: false,
							new_validator_set: session_validators.clone(),
						},
					));

					roll_until_matches(|| pallet_session::CurrentIndex::<T>::get() == i, false);
					assert_eq!(
						session_events_since_last_call(),
						vec![
							pallet_session::Event::<T>::NewQueued,
							pallet_session::Event::<T>::NewSession { session_index: i },
						]
					);
					assert_eq!(
						historical_events_since_last_call(),
						vec![pallet_session::historical::Event::<T>::RootStored { index: i + 1 }]
					)
				}

				// ensure that we have the root for these recorded in the historical session pallet
				assert_eq!(pallet_session::historical::StoredRange::<T>::get(), Some((2, 12)));

				// send back a new validator set, but with some pruning info.
				assert_ok!(ah_client::Pallet::<T>::validator_set(
					RuntimeOrigin::root(),
					ValidatorSetReport {
						id: 999,
						prune_up_to: Some(5),
						leftover: false,
						new_validator_set: vec![1, 2, 3, 4],
					},
				));

				assert_eq!(pallet_session::historical::StoredRange::<T>::get(), Some((5, 12)));
				assert_eq!(
					historical_events_since_last_call(),
					vec![pallet_session::historical::Event::<T>::RootsPruned { up_to: 5 }]
				);
			})
	}
}

mod blocking {
	use super::*;

	#[test]
	fn drops_incoming_if_passive_mode() {
		ExtBuilder::default().local_queue().pre_migration().build().execute_with(|| {
			// given
			let report = ValidatorSetReport {
				id: 1,
				prune_up_to: None,
				leftover: false,
				new_validator_set: vec![1, 2, 3, 4],
			};

			// when
			assert_noop!(
				ah_client::Pallet::<Runtime>::validator_set(RuntimeOrigin::root(), report),
				ah_client::Error::<Runtime>::Blocked,
			);

			// then
			assert_eq!(ah_client::ValidatorSet::<T>::get(), None);
		})
	}

	#[test]
	fn drops_outgoing_if_passive_mode() {
		ExtBuilder::default().local_queue().pre_migration().build().execute_with(|| {
			// roll 5 sessions
			roll_until_matches(|| pallet_session::CurrentIndex::<Runtime>::get() == 5, false);

			// nothing is queued; No outgoing messages expected in passive mode.
			assert_eq!(LocalQueue::get().unwrap(), vec![]);

			// make pallet active
			Mode::<T>::put(OperatingMode::Active);

			// roll another session
			roll_until_matches(|| pallet_session::CurrentIndex::<Runtime>::get() == 6, false);

			// now session report is queued.
			assert_eq!(
				LocalQueue::get().unwrap(),
				vec![(
					180,
					OutgoingMessages::SessionReport(SessionReport {
						end_index: 5,
						validator_points: vec![(11, 600)],
						activation_timestamp: None,
						leftover: false,
					})
				)]
			);
		})
	}
}

mod splitting {
	use super::*;

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
	fn splitting_and_merging_equal() {
		let full_report = ValidatorSetReport {
			new_validator_set: vec![1, 2, 3, 4, 5],
			id: 0,
			prune_up_to: None,
			leftover: false,
		};

		for c in 1..=6 {
			assert_eq!(
				full_report.clone().split(c).into_iter().reduce(|acc, x| acc.merge(x).unwrap()),
				Some(full_report.clone())
			);
		}
	}

	#[test]
	fn can_split_and_merge_validator_set_report() {
		ExtBuilder::default().local_queue().build().execute_with(|| {
			let full_report = ValidatorSetReport {
				new_validator_set: vec![1, 2, 3, 4, 5],
				id: 0,
				prune_up_to: None,
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
	fn can_handle_splitted_validator_set() {
		ExtBuilder::default().local_queue().build().execute_with(|| {
			let full_report = ValidatorSetReport {
				new_validator_set: vec![1, 2, 3, 4, 5, 6],
				id: 0,
				prune_up_to: None,
				leftover: false,
			};
			let splitted = full_report.split(2);
			let incomplete0 = splitted[0].clone();
			let incomplete1 = splitted[1].clone();
			let complete = splitted[2].clone();

			assert!(incomplete0.leftover);
			assert!(incomplete1.leftover);
			assert!(!complete.leftover);

			// nothing is queued.
			assert!(ah_client::IncompleteValidatorSetReport::<Runtime>::get().is_none());
			assert!(ah_client::ValidatorSet::<Runtime>::get().is_none());

			// when
			assert_ok!(StakingAhClient::validator_set(RuntimeOrigin::root(), incomplete0.clone()));
			assert_eq!(
				ah_client_events_since_last_call(),
				vec![ah_client::Event::<T>::ValidatorSetReceived {
					id: 0,
					new_validator_set_count: 2,
					prune_up_to: None,
					leftover: true
				}]
			);

			// then
			assert_eq!(
				ah_client::IncompleteValidatorSetReport::<Runtime>::get()
					.map(|r| r.new_validator_set),
				Some(vec![1, 2])
			);
			assert!(ah_client::ValidatorSet::<Runtime>::get().is_none());

			// when
			assert_ok!(StakingAhClient::validator_set(RuntimeOrigin::root(), incomplete1.clone()));
			assert_eq!(
				ah_client_events_since_last_call(),
				vec![ah_client::Event::<T>::ValidatorSetReceived {
					id: 0,
					new_validator_set_count: 4,
					prune_up_to: None,
					leftover: true
				}]
			);

			// then
			assert_eq!(
				ah_client::IncompleteValidatorSetReport::<Runtime>::get()
					.map(|r| r.new_validator_set),
				Some(vec![1, 2, 3, 4])
			);
			assert!(ah_client::ValidatorSet::<Runtime>::get().is_none());

			// when
			assert_ok!(StakingAhClient::validator_set(RuntimeOrigin::root(), complete.clone()));
			assert_eq!(
				ah_client_events_since_last_call(),
				vec![ah_client::Event::<T>::ValidatorSetReceived {
					id: 0,
					new_validator_set_count: 6,
					prune_up_to: None,
					leftover: false
				}]
			);

			// then
			assert_eq!(ah_client::IncompleteValidatorSetReport::<Runtime>::get(), None);
			assert_eq!(
				ah_client::ValidatorSet::<Runtime>::get(),
				Some((0, vec![1, 2, 3, 4, 5, 6]))
			);
		})
	}

	#[test]
	fn incomplete_wrong_id_dropped() {
		ExtBuilder::default().local_queue().build().execute_with(|| {
			let incomplete0 = rc_client::ValidatorSetReport {
				id: 0,
				new_validator_set: vec![1, 2],
				leftover: true,
				prune_up_to: None,
			};
			let broken = rc_client::ValidatorSetReport {
				id: 1,
				new_validator_set: vec![3, 4],
				leftover: true,
				prune_up_to: None,
			};

			// nothing is queued.
			assert!(ah_client::IncompleteValidatorSetReport::<Runtime>::get().is_none());
			assert!(ah_client::ValidatorSet::<Runtime>::get().is_none());

			// when
			assert_ok!(StakingAhClient::validator_set(RuntimeOrigin::root(), incomplete0.clone()));

			// then
			assert_eq!(
				ah_client::IncompleteValidatorSetReport::<Runtime>::get()
					.map(|r| r.new_validator_set),
				Some(vec![1, 2])
			);
			assert!(ah_client::ValidatorSet::<Runtime>::get().is_none());

			// when
			assert_ok!(StakingAhClient::validator_set(RuntimeOrigin::root(), broken.clone()));
			// then
			assert_eq!(ah_client::IncompleteValidatorSetReport::<Runtime>::get(), None);
			assert!(ah_client::ValidatorSet::<Runtime>::get().is_none());

			assert_eq!(
				frame_system::Pallet::<Runtime>::read_events_for_pallet::<ah_client::Event<Runtime>>(
				),
				vec![
					ah_client::Event::<T>::ValidatorSetReceived {
						id: 0,
						new_validator_set_count: 2,
						prune_up_to: None,
						leftover: true
					},
					ah_client::Event::<T>::CouldNotMergeAndDropped
				]
			);
		})
	}
}

#[cfg(test)]
mod key_proofs {
	use frame::traits::KeyOwnerProofSystem;
	use frame_support::sp_runtime;

	use super::*;

	#[test]
	#[ignore = "not complete yet"]
	fn can_generate_valid_latest_key_ownership_proof() {
		ExtBuilder::default()
			.local_queue()
			.session_keys(vec![1, 2, 3, 4])
			.build()
			.execute_with(|| {
				// no sessions exists, cannot generate any proofs
				assert_eq!(pallet_session::historical::StoredRange::<T>::get(), None);
				assert_eq!(pallet_session::CurrentIndex::<T>::get(), 0);

				// receive a validator set, and trigger a 3 new sessions, such that we store some
				// roots.
				assert_ok!(ah_client::Pallet::<T>::validator_set(
					RuntimeOrigin::root(),
					ValidatorSetReport {
						id: 0,
						prune_up_to: None,
						leftover: false,
						new_validator_set: vec![1, 2, 3, 4],
					},
				));
				roll_until_matches(|| pallet_session::CurrentIndex::<T>::get() == 3, false);

				assert_eq!(
					historical_events_since_last_call(),
					vec![
						pallet_session::historical::Event::RootStored { index: 2 },
						pallet_session::historical::Event::RootStored { index: 3 },
						pallet_session::historical::Event::RootStored { index: 4 }
					]
				);

				assert_eq!(pallet_session::CurrentIndex::<T>::get(), 3);
				assert_eq!(pallet_session::historical::StoredRange::<T>::get(), Some((2, 5)));

				// generate the proof for one of the validators
				use sp_runtime::{key_types::DUMMY, testing::UintAuthorityId, traits::OpaqueKeys};

				let key_ids = <SessionKeys as OpaqueKeys>::key_ids();
				assert_eq!(key_ids.len(), 1, "we have inserted only one key type in mock");

				let keys = pallet_session::Pallet::<T>::load_keys(&1).unwrap();
				let our_key = keys.get::<UintAuthorityId>(key_ids[0]);
				assert_eq!(key_ids[0], DUMMY);

				let proof =
					pallet_session::historical::Pallet::<T>::prove((DUMMY, &our_key.encode()[..]))
						.unwrap();

				assert_eq!(proof.session, 3);
				assert_eq!(proof.validator_count, 4);

				// proof is valid, and it results into a default exposure.
				assert_eq!(
					pallet_session::historical::Pallet::<T>::check_proof(
						(DUMMY, &our_key.encode()[..]),
						proof
					)
					.unwrap(),
					(1, sp_staking::Exposure::default())
				)
			})
	}
}
