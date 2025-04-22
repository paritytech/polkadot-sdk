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

use crate::ah::mock::*;
use frame::prelude::Perbill;
use frame_support::{assert_noop, assert_ok};
use pallet_election_provider_multi_block::{Event as ElectionEvent, Phase};
use pallet_staking_async::{
	session_rotation::Rotator, ActiveEra, ActiveEraInfo, CurrentEra, Event as StakingEvent,
};
use pallet_staking_async_rc_client as rc_client;
use pallet_staking_async_rc_client::ValidatorSetReport;

// Tests that are specific to Asset Hub.
#[test]
fn on_receive_session_report() {
	ExtBuilder::default().local_queue().build().execute_with(|| {
		// GIVEN genesis state of ah
		assert_eq!(System::block_number(), 1);
		assert_eq!(CurrentEra::<T>::get(), Some(0));
		assert_eq!(pallet_staking_async::ErasStartSessionIndex::<T>::get(0), Some(0));
		assert_eq!(ActiveEra::<T>::get(), Some(ActiveEraInfo { index: 0, start: Some(0) }));

		// WHEN session ends on RC and session report is received by AH.
		let session_report = rc_client::SessionReport {
			end_index: 0,
			validator_points: (1..9).into_iter().map(|v| (v as AccountId, v * 10)).collect(),
			activation_timestamp: None,
			leftover: false,
		};

		assert_ok!(rc_client::Pallet::<T>::relay_session_report(
			RuntimeOrigin::root(),
			session_report.clone(),
		));

		// THEN end 0, start 1, plan 2
		let era_points = pallet_staking_async::ErasRewardPoints::<T>::get(&0);
		assert_eq!(era_points.total, 360);
		assert_eq!(era_points.individual.get(&1), Some(&10));
		assert_eq!(era_points.individual.get(&4), Some(&40));
		assert_eq!(era_points.individual.get(&7), Some(&70));
		assert_eq!(era_points.individual.get(&8), Some(&80));
		assert_eq!(era_points.individual.get(&9), None);

		// assert no era changed yet.
		assert_eq!(CurrentEra::<T>::get(), Some(0));
		assert_eq!(ActiveEra::<T>::get(), Some(ActiveEraInfo { index: 0, start: Some(0) }));

		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::SessionRotated {
				starting_session: 1,
				active_era: 0,
				planned_era: 0
			}]
		);

		assert_eq!(election_events_since_last_call(), vec![]);

		// roll two more sessions...
		for i in 1..3 {
			// roll some random number of blocks.
			roll_many(10);

			// send the session report.
			assert_ok!(rc_client::Pallet::<T>::relay_session_report(
				RuntimeOrigin::root(),
				rc_client::SessionReport {
					end_index: i,
					validator_points: vec![(1, 10)],
					activation_timestamp: None,
					leftover: false,
				}
			));

			let era_points = pallet_staking_async::ErasRewardPoints::<T>::get(&0);
			assert_eq!(era_points.total, 360 + i * 10);
			assert_eq!(era_points.individual.get(&1), Some(&(10 + i * 10)));

			assert_eq!(
				staking_events_since_last_call(),
				vec![StakingEvent::SessionRotated {
					starting_session: i + 1,
					active_era: 0,
					planned_era: 0
				}]
			);
		}

		// Next session we will begin election.
		assert_ok!(rc_client::Pallet::<T>::relay_session_report(
			RuntimeOrigin::root(),
			rc_client::SessionReport {
				end_index: 3,
				validator_points: vec![(1, 10)],
				activation_timestamp: None,
				leftover: false,
			}
		));

		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::SessionRotated {
				starting_session: 4,
				active_era: 0,
				// planned era 1 indicates election start signal is sent.
				planned_era: 1
			}]
		);

		assert_eq!(
			election_events_since_last_call(),
			// Snapshot phase has started which will run for 3 blocks
			vec![ElectionEvent::PhaseTransitioned { from: Phase::Off, to: Phase::Snapshot(3) }]
		);

		// roll 3 blocks for signed phase, and one for the transition.
		roll_many(3 + 1);
		assert_eq!(
			election_events_since_last_call(),
			// Signed phase has started which will run for 3 blocks.
			vec![ElectionEvent::PhaseTransitioned {
				from: Phase::Snapshot(0),
				to: Phase::Signed(3)
			}]
		);

		// roll some blocks until election result is exported.
		roll_many(14);
		assert_eq!(
			election_events_since_last_call(),
			vec![
				ElectionEvent::PhaseTransitioned {
					from: Phase::Signed(0),
					to: Phase::SignedValidation(5)
				},
				ElectionEvent::PhaseTransitioned {
					from: Phase::SignedValidation(0),
					to: Phase::Unsigned(3)
				},
				ElectionEvent::PhaseTransitioned { from: Phase::Unsigned(0), to: Phase::Done },
			]
		);

		// no staking event while election ongoing.
		assert_eq!(staking_events_since_last_call(), vec![]);
		// no xcm message sent yet.
		assert_eq!(LocalQueue::get().unwrap(), vec![]);

		// next 3 block exports the election result to staking.
		roll_many(3);

		assert_eq!(
			staking_events_since_last_call(),
			vec![
				StakingEvent::PagedElectionProceeded { page: 2, result: Ok(4) },
				StakingEvent::PagedElectionProceeded { page: 1, result: Ok(0) },
				StakingEvent::PagedElectionProceeded { page: 0, result: Ok(0) }
			]
		);

		assert_eq!(
			election_events_since_last_call(),
			vec![
				ElectionEvent::PhaseTransitioned { from: Phase::Done, to: Phase::Export(2) },
				ElectionEvent::PhaseTransitioned { from: Phase::Export(0), to: Phase::Off }
			]
		);

		// New validator set xcm message is sent to RC.
		assert_eq!(
			LocalQueue::get().unwrap(),
			vec![(
				// this is the block number at which the message was sent.
				42,
				OutgoingMessages::ValidatorSet(ValidatorSetReport {
					new_validator_set: vec![3, 5, 6, 8],
					id: 1,
					prune_up_to: None,
					leftover: false
				})
			)]
		);
	})
}

#[test]
fn roll_many_eras() {
	// todo:
	// - Ensure rewards can be claimed at correct era.
	// - assert outgoing messages, including id and prune_up_to.
	ExtBuilder::default().local_queue().build().execute_with(|| {
		let mut session_counter: u32 = 0;

		let mut roll_session = |activate: bool| {
			let activation_timestamp = if activate {
				let current_era = CurrentEra::<T>::get().unwrap();
				Some((current_era as u64 * 1000, current_era as u32))
			} else {
				None
			};

			assert_ok!(rc_client::Pallet::<T>::relay_session_report(
				RuntimeOrigin::root(),
				rc_client::SessionReport {
					end_index: session_counter,
					validator_points: vec![(1, 10)],
					activation_timestamp,
					leftover: false,
				}
			));

			// increment session for the next iteration.
			session_counter += 1;

			// run session blocks.
			roll_many(60);
		};

		for era in 0..50 {
			// --- first 3 idle session
			for _ in 0..3 {
				roll_session(false);
				assert_eq!(ActiveEra::<T>::get().unwrap().index, era);
				assert_eq!(CurrentEra::<T>::get().unwrap(), era);
			}

			// ensure validator set not sent yet to RC.
			// queue size same as in last iteration.
			assert_eq!(LocalQueue::get().unwrap().len() as u32, era);

			// --- plan era session
			roll_session(false);
			assert_eq!(ActiveEra::<T>::get().unwrap().index, era);
			assert_eq!(CurrentEra::<T>::get().unwrap(), era + 1);

			// ensure new validator set sent to RC.
			// length increases by 1.
			assert_eq!(LocalQueue::get().unwrap().len() as u32, era + 1);

			// --- 5th starting session, idle
			roll_session(false);
			assert_eq!(ActiveEra::<T>::get().unwrap().index, era);
			assert_eq!(CurrentEra::<T>::get().unwrap(), era + 1);

			// --- 6th the era rotation session
			roll_session(true);
			assert_eq!(ActiveEra::<T>::get().unwrap().index, era + 1);
			assert_eq!(CurrentEra::<T>::get().unwrap(), era + 1);
		}
	});
}

#[test]
fn receives_old_session_report() {
	ExtBuilder::default().local_queue().build().execute_with(|| {
		// Initial state
		assert_eq!(CurrentEra::<T>::get(), Some(0));
		assert_eq!(pallet_staking_async::ErasStartSessionIndex::<T>::get(0), Some(0));
		assert_eq!(ActiveEra::<T>::get(), Some(ActiveEraInfo { index: 0, start: Some(0) }));
		assert_eq!(rc_client::LastSessionReportEndingIndex::<T>::get(), None);

		// Receive report for end of 1, start of 1 and plan 2.
		let session_report = rc_client::SessionReport {
			end_index: 0,
			validator_points: vec![(5, 50)],
			activation_timestamp: None,
			leftover: false,
		};

		assert_ok!(rc_client::Pallet::<T>::relay_session_report(
			RuntimeOrigin::root(),
			session_report.clone(),
		));

		// then
		assert_eq!(rc_client::LastSessionReportEndingIndex::<T>::get(), Some(0));
		assert_eq!(
			rc_client_events_since_last_call(),
			vec![rc_client::Event::SessionReportReceived {
				end_index: 0,
				activation_timestamp: None,
				validator_points_counts: 1,
				leftover: false
			}]
		);
		assert_eq!(
			staking_events_since_last_call(),
			vec![pallet_staking_async::Event::SessionRotated {
				starting_session: 1,
				active_era: 0,
				planned_era: 0
			}]
		);

		// reward points are added
		assert_eq!(pallet_staking_async::ErasRewardPoints::<T>::get(&0).total, 50);

		// this is ok, but no new session report is received in staking.
		assert_noop!(
			rc_client::Pallet::<T>::relay_session_report(
				RuntimeOrigin::root(),
				session_report.clone(),
			),
			rc_client::Error::<T>::SessionIndexNotValid
		);
	})
}

#[test]
fn receives_session_report_in_future() {
	ExtBuilder::default().local_queue().build().execute_with(|| {
		// Initial state
		assert_eq!(CurrentEra::<T>::get(), Some(0));
		assert_eq!(pallet_staking_async::ErasStartSessionIndex::<T>::get(0), Some(0));
		assert_eq!(ActiveEra::<T>::get(), Some(ActiveEraInfo { index: 0, start: Some(0) }));
		assert_eq!(rc_client::LastSessionReportEndingIndex::<T>::get(), None);

		// Receive report for end of 1, start of 1 and plan 2.

		assert_ok!(rc_client::Pallet::<T>::relay_session_report(
			RuntimeOrigin::root(),
			rc_client::SessionReport {
				end_index: 0,
				validator_points: vec![(5, 50)],
				activation_timestamp: None,
				leftover: false,
			},
		));

		// then
		assert_eq!(rc_client::LastSessionReportEndingIndex::<T>::get(), Some(0));
		assert_eq!(
			rc_client_events_since_last_call(),
			vec![rc_client::Event::SessionReportReceived {
				end_index: 0,
				activation_timestamp: None,
				validator_points_counts: 1,
				leftover: false
			}]
		);
		assert_eq!(
			staking_events_since_last_call(),
			vec![pallet_staking_async::Event::SessionRotated {
				starting_session: 1,
				active_era: 0,
				planned_era: 0
			}]
		);

		// reward points are added
		assert_eq!(pallet_staking_async::ErasRewardPoints::<T>::get(&0).total, 50);

		// skip end_index 1
		assert_noop!(
			rc_client::Pallet::<T>::relay_session_report(
				RuntimeOrigin::root(),
				rc_client::SessionReport {
					end_index: 2,
					validator_points: vec![(5, 50)],
					activation_timestamp: None,
					leftover: false,
				},
			),
			rc_client::Error::<T>::SessionIndexNotValid
		);
	})
}

#[test]
fn on_offence_current_era() {
	ExtBuilder::default().local_queue().build().execute_with(|| {
		let active_validators = roll_until_next_active(0);
		assert_eq!(pallet_staking_async::ErasStartSessionIndex::<Runtime>::get(1), Some(5));
		assert_eq!(active_validators, vec![3, 5, 6, 8]);

		// flush the events.
		let _ = staking_events_since_last_call();

		assert_ok!(rc_client::Pallet::<Runtime>::relay_new_offence(
			RuntimeOrigin::root(),
			5,
			vec![
				rc_client::Offence {
					offender: 5,
					reporters: vec![],
					slash_fraction: Perbill::from_percent(50),
				},
				rc_client::Offence {
					offender: 3,
					reporters: vec![],
					slash_fraction: Perbill::from_percent(50),
				}
			]
		));

		assert_eq!(
			staking_events_since_last_call(),
			vec![
				pallet_staking_async::Event::OffenceReported {
					offence_era: 1,
					validator: 5,
					fraction: Perbill::from_percent(50)
				},
				pallet_staking_async::Event::OffenceReported {
					offence_era: 1,
					validator: 3,
					fraction: Perbill::from_percent(50)
				}
			]
		);

		// 2 blocks to process these offences, and they are deferred.
		roll_next();
		assert_eq!(
			staking_events_since_last_call(),
			vec![pallet_staking_async::Event::SlashComputed {
				offence_era: 1,
				slash_era: 3,
				offender: 5,
				page: 0
			},]
		);
		roll_next();
		assert_eq!(
			staking_events_since_last_call(),
			vec![pallet_staking_async::Event::SlashComputed {
				offence_era: 1,
				slash_era: 3,
				offender: 3,
				page: 0
			}]
		);

		// skip two eras
		assert_eq!(SlashDeferredDuration::get(), 2);
		roll_until_next_active(5);
		roll_until_next_active(10);
		let _ = staking_events_since_last_call();

		// 2 blocks to apply the slashes
		roll_next();
		assert_eq!(
			staking_events_since_last_call(),
			vec![pallet_staking_async::Event::Slashed { staker: 3, amount: 50 },]
		);
		roll_next();
		assert_eq!(
			staking_events_since_last_call(),
			vec![
				pallet_staking_async::Event::Slashed { staker: 5, amount: 50 },
				pallet_staking_async::Event::Slashed { staker: 110, amount: 50 }
			]
		);
	});
}

#[test]
fn on_offence_current_era_instant_apply() {
	ExtBuilder::default()
		.local_queue()
		.slash_defer_duration(0)
		.build()
		.execute_with(|| {
			let active_validators = roll_until_next_active(0);
			assert_eq!(pallet_staking_async::ErasStartSessionIndex::<Runtime>::get(1), Some(5));
			assert_eq!(active_validators, vec![3, 5, 6, 8]);

			// flush the events.
			let _ = staking_events_since_last_call();

			assert_ok!(rc_client::Pallet::<Runtime>::relay_new_offence(
				RuntimeOrigin::root(),
				5,
				vec![
					rc_client::Offence {
						offender: 5,
						reporters: vec![],
						slash_fraction: Perbill::from_percent(50),
					},
					rc_client::Offence {
						offender: 3,
						reporters: vec![],
						slash_fraction: Perbill::from_percent(50),
					}
				]
			));

			assert_eq!(
				staking_events_since_last_call(),
				vec![
					pallet_staking_async::Event::OffenceReported {
						offence_era: 1,
						validator: 5,
						fraction: Perbill::from_percent(50)
					},
					pallet_staking_async::Event::OffenceReported {
						offence_era: 1,
						validator: 3,
						fraction: Perbill::from_percent(50)
					}
				]
			);

			// 2 blocks to process these offences, and they are applied on the spot.
			roll_next();
			assert_eq!(
				staking_events_since_last_call(),
				vec![
					pallet_staking_async::Event::SlashComputed {
						offence_era: 1,
						slash_era: 1,
						offender: 5,
						page: 0
					},
					pallet_staking_async::Event::Slashed { staker: 5, amount: 50 },
					pallet_staking_async::Event::Slashed { staker: 110, amount: 50 }
				]
			);
			roll_next();
			assert_eq!(
				staking_events_since_last_call(),
				vec![
					pallet_staking_async::Event::SlashComputed {
						offence_era: 1,
						slash_era: 1,
						offender: 3,
						page: 0
					},
					pallet_staking_async::Event::Slashed { staker: 3, amount: 50 }
				]
			);
		});
}

#[test]
fn on_offence_non_validator() {
	ExtBuilder::default()
		.slash_defer_duration(0)
		.local_queue()
		.build()
		.execute_with(|| {
			let active_validators = roll_until_next_active(0);
			assert_eq!(pallet_staking_async::ErasStartSessionIndex::<Runtime>::get(1), Some(5));
			assert_eq!(active_validators, vec![3, 5, 6, 8]);

			// flush the events.
			let _ = staking_events_since_last_call();

			assert_ok!(rc_client::Pallet::<Runtime>::relay_new_offence(
				RuntimeOrigin::root(),
				5,
				vec![rc_client::Offence {
					// this offender is unknown to the staking pallet.
					offender: 666,
					reporters: vec![],
					slash_fraction: Perbill::from_percent(50),
				}]
			));

			// nada
			assert_eq!(staking_events_since_last_call(), vec![]);
		});
}

#[test]
fn on_offence_previous_era() {
	ExtBuilder::default().local_queue().build().execute_with(|| {
		let _ = roll_until_next_active(0);
		let _ = roll_until_next_active(5);
		let active_validators = roll_until_next_active(10);

		assert_eq!(active_validators, vec![3, 5, 6, 8]);
		assert_eq!(Rotator::<Runtime>::active_era(), 3);

		// flush the events.
		let _ = staking_events_since_last_call();

		// report an offence for the session belonging to the previous era
		assert_eq!(pallet_staking_async::ErasStartSessionIndex::<Runtime>::get(1), Some(5));

		assert_ok!(rc_client::Pallet::<Runtime>::relay_new_offence(
			RuntimeOrigin::root(),
			// offence is in era 1
			5,
			vec![rc_client::Offence {
				offender: 3,
				reporters: vec![],
				slash_fraction: Perbill::from_percent(50),
			}]
		));

		// reported
		assert_eq!(
			staking_events_since_last_call(),
			vec![pallet_staking_async::Event::OffenceReported {
				offence_era: 1,
				validator: 3,
				fraction: Perbill::from_percent(50)
			}]
		);

		// computed, and instantly applied, as we are already on era 3 (slash era = 1, defer = 2)
		roll_next();
		assert_eq!(
			staking_events_since_last_call(),
			vec![
				pallet_staking_async::Event::SlashComputed {
					offence_era: 1,
					slash_era: 3,
					offender: 3,
					page: 0
				},
				pallet_staking_async::Event::Slashed { staker: 3, amount: 50 }
			]
		);

		// nothing left
		roll_next();
		assert_eq!(staking_events_since_last_call(), vec![]);
	});
}

#[test]
fn on_offence_previous_era_instant_apply() {
	ExtBuilder::default()
		.slash_defer_duration(0)
		.local_queue()
		.build()
		.execute_with(|| {
			let _ = roll_until_next_active(0);
			let _ = roll_until_next_active(5);
			let active_validators = roll_until_next_active(10);

			assert_eq!(active_validators, vec![3, 5, 6, 8]);
			assert_eq!(Rotator::<Runtime>::active_era(), 3);

			// flush the events.
			let _ = staking_events_since_last_call();

			// report an offence for the session belonging to the previous era
			assert_eq!(pallet_staking_async::ErasStartSessionIndex::<Runtime>::get(1), Some(5));

			assert_ok!(rc_client::Pallet::<Runtime>::relay_new_offence(
				RuntimeOrigin::root(),
				// offence is in era 1
				5,
				vec![rc_client::Offence {
					offender: 3,
					reporters: vec![],
					slash_fraction: Perbill::from_percent(50),
				}]
			));

			// reported
			assert_eq!(
				staking_events_since_last_call(),
				vec![pallet_staking_async::Event::OffenceReported {
					offence_era: 1,
					validator: 3,
					fraction: Perbill::from_percent(50)
				}]
			);

			// applied right away
			roll_next();
			assert_eq!(
				staking_events_since_last_call(),
				vec![
					pallet_staking_async::Event::SlashComputed {
						offence_era: 1,
						slash_era: 1,
						offender: 3,
						page: 0
					},
					pallet_staking_async::Event::Slashed { staker: 3, amount: 50 }
				]
			);

			// nothing left
			roll_next();
			assert_eq!(staking_events_since_last_call(), vec![]);
		});
}
