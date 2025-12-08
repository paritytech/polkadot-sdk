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
use frame_election_provider_support::Weight;
use frame_support::assert_ok;
use pallet_election_provider_multi_block::{
	unsigned::miner::OffchainWorkerMiner, verifier::Event as VerifierEvent, CurrentPhase,
	ElectionScore, Event as ElectionEvent, Phase,
};
use pallet_staking_async::{
	self as staking_async, session_rotation::Rotator, ActiveEra, ActiveEraInfo, CurrentEra,
	Event as StakingEvent,
};
use pallet_staking_async_rc_client::{
	self as rc_client, OutgoingValidatorSet, UnexpectedKind, ValidatorSetReport,
};

// Tests that are specific to Asset Hub.
#[test]
fn on_receive_session_report() {
	ExtBuilder::default().local_queue().build().execute_with(|| {
		// GIVEN genesis state of ah
		assert_eq!(System::block_number(), 1);
		assert_eq!(CurrentEra::<T>::get(), Some(0));
		assert_eq!(Rotator::<Runtime>::active_era_start_session_index(), 0);
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
		let era_points = staking_async::ErasRewardPoints::<T>::get(&0);
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

			let era_points = staking_async::ErasRewardPoints::<T>::get(&0);
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
		roll_many(15);
		assert_eq!(
			election_events_since_last_call(),
			vec![
				ElectionEvent::PhaseTransitioned {
					from: Phase::Signed(0),
					to: Phase::SignedValidation(6)
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

		// normal conditions, validator set can be sent.
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
				ElectionEvent::PhaseTransitioned { from: Phase::Done, to: Phase::Export(1) },
				ElectionEvent::PhaseTransitioned { from: Phase::Export(0), to: Phase::Off }
			]
		);

		// outgoing set is queued, sent in the next block.
		assert!(pallet_staking_async_rc_client::OutgoingValidatorSet::<T>::get().is_some());
		roll_next();
		assert!(pallet_staking_async_rc_client::OutgoingValidatorSet::<T>::get().is_none());

		// New validator set xcm message is sent to RC.
		assert_eq!(
			LocalQueue::get().unwrap(),
			vec![(
				// this is the block number at which the message was sent.
				44,
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
fn validator_set_send_fail_retries() {
	ExtBuilder::default().local_queue().build().execute_with(|| {
		// GIVEN genesis state of ah
		assert_eq!(System::block_number(), 1);
		assert_eq!(CurrentEra::<T>::get(), Some(0));
		assert_eq!(Rotator::<Runtime>::active_era_start_session_index(), 0);
		assert_eq!(ActiveEra::<T>::get(), Some(ActiveEraInfo { index: 0, start: Some(0) }));

		// first session comes in.
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

		// flush some events.
		let _ = staking_events_since_last_call();

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

			let era_points = staking_async::ErasRewardPoints::<T>::get(&0);
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
		roll_many(15);
		assert_eq!(
			election_events_since_last_call(),
			vec![
				ElectionEvent::PhaseTransitioned {
					from: Phase::Signed(0),
					to: Phase::SignedValidation(6)
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

		// bad condition -- validator set cannot be sent.
		// assume the next validator set cannot be sent.
		NextRelayDeliveryFails::set(true);
		let _ = rc_client_events_since_last_call();

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
				ElectionEvent::PhaseTransitioned { from: Phase::Done, to: Phase::Export(1) },
				ElectionEvent::PhaseTransitioned { from: Phase::Export(0), to: Phase::Off }
			]
		);

		// outgoing set is queued, sent in the next block.
		assert!(matches!(OutgoingValidatorSet::<T>::get(), Some((_, 3))));
		roll_next();

		// but..

		// nothing is queued
		assert!(LocalQueue::get().unwrap().is_empty());

		// rc-client has an event
		assert_eq!(
			rc_client_events_since_last_call(),
			vec![rc_client::Event::Unexpected(UnexpectedKind::ValidatorSetSendFailed)]
		);

		// the buffer is set
		assert!(matches!(OutgoingValidatorSet::<T>::get(), Some((_, 2))));

		// next block it is retried and sent fine
		roll_next();
		assert_eq!(
			LocalQueue::get().unwrap(),
			vec![(
				// this is the block number at which the message was sent.
				45,
				OutgoingMessages::ValidatorSet(ValidatorSetReport {
					new_validator_set: vec![3, 5, 6, 8],
					id: 1,
					prune_up_to: None,
					leftover: false
				})
			)]
		);

		// buffer is clear
		assert!(OutgoingValidatorSet::<T>::get().is_none());
	});
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
		assert_eq!(Rotator::<Runtime>::active_era_start_session_index(), 0);
		assert_eq!(ActiveEra::<T>::get(), Some(ActiveEraInfo { index: 0, start: Some(0) }));
		assert_eq!(rc_client::LastSessionReportEndingIndex::<T>::get(), None);

		// Receive report for end of 0, start of 1 and plan 2.
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
			vec![staking_async::Event::SessionRotated {
				starting_session: 1,
				active_era: 0,
				planned_era: 0
			}]
		);

		// reward points are not added
		assert_eq!(staking_async::ErasRewardPoints::<T>::get(&0).total, 50);
		assert_eq!(rc_client::LastSessionReportEndingIndex::<T>::get(), Some(0));

		// then send it again, this is basically dropped, although it returns `Ok()`
		assert_ok!(rc_client::Pallet::<T>::relay_session_report(
			RuntimeOrigin::root(),
			session_report
		));

		// no storage is changed
		assert_eq!(staking_async::ErasRewardPoints::<T>::get(&0).total, 50);
		assert_eq!(rc_client::LastSessionReportEndingIndex::<T>::get(), Some(0));
	})
}

#[test]
fn receives_session_report_in_future() {
	ExtBuilder::default().local_queue().build().execute_with(|| {
		// Initial state
		assert_eq!(CurrentEra::<T>::get(), Some(0));
		assert_eq!(Rotator::<Runtime>::active_era_start_session_index(), 0);
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
			vec![staking_async::Event::SessionRotated {
				starting_session: 1,
				active_era: 0,
				planned_era: 0
			}]
		);

		// reward points are added
		assert_eq!(staking_async::ErasRewardPoints::<T>::get(&0).total, 50);

		// skip end_index 1, send 2
		assert_ok!(rc_client::Pallet::<T>::relay_session_report(
			RuntimeOrigin::root(),
			rc_client::SessionReport {
				end_index: 2,
				validator_points: vec![(5, 50)],
				activation_timestamp: None,
				leftover: false,
			},
		));

		// our tracker of last session is updated (and has skipped `1`)
		assert_eq!(rc_client::LastSessionReportEndingIndex::<T>::get(), Some(2));

		assert_eq!(
			rc_client_events_since_last_call(),
			vec![
				rc_client::Event::Unexpected(UnexpectedKind::SessionSkipped),
				rc_client::Event::SessionReportReceived {
					end_index: 2,
					activation_timestamp: None,
					validator_points_counts: 1,
					leftover: false
				}
			]
		);
		assert_eq!(
			staking_events_since_last_call(),
			vec![staking_async::Event::SessionRotated {
				starting_session: 3,
				active_era: 0,
				planned_era: 0
			}]
		);

		// reward points are accumulated
		assert_eq!(staking_async::ErasRewardPoints::<T>::get(&0).total, 100);
	})
}

#[test]
fn session_report_burst() {
	// note: there is also an e2e `session_report_burst` test
	ExtBuilder::default().local_queue().build().execute_with(|| {
		// Initial state
		assert_eq!(CurrentEra::<T>::get(), Some(0));
		assert_eq!(Rotator::<Runtime>::active_era_start_session_index(), 0);
		assert_eq!(ActiveEra::<T>::get(), Some(ActiveEraInfo { index: 0, start: Some(0) }));
		assert_eq!(rc_client::LastSessionReportEndingIndex::<T>::get(), None);

		// then send 20 sessions all at once. This is enough to schedule multiple elections, but we
		// only schedule one.
		for s in 1..=20 {
			assert_ok!(rc_client::Pallet::<T>::relay_session_report(
				RuntimeOrigin::root(),
				rc_client::SessionReport {
					end_index: s,
					validator_points: vec![(5, 50)],
					activation_timestamp: None,
					leftover: false,
				},
			));
			// all are processed fine, in one go
			assert_eq!(
				rc_client_events_since_last_call(),
				vec![rc_client::Event::SessionReportReceived {
					end_index: s,
					activation_timestamp: None,
					validator_points_counts: 1,
					leftover: false
				}]
			);
			// and we have collected reward points.
			assert_eq!(staking_async::ErasRewardPoints::<T>::get(&0).total, 50 * s);
		}

		// crucially, election has started, but we have not done anything else.
		Rotator::<T>::assert_election_ongoing();

		assert!(matches!(
			&staking_events_since_last_call()[..],
			&[
				staking_async::Event::SessionRotated {
					starting_session: 2,
					active_era: 0,
					planned_era: 0
				},
				..,
				staking_async::Event::SessionRotated {
					starting_session: 21,
					active_era: 0,
					planned_era: 1
				}
			]
		));
	})
}

#[test]
fn on_offence_current_era() {
	ExtBuilder::default().local_queue().build().execute_with(|| {
		let active_validators = roll_until_next_active(0);
		assert_eq!(Rotator::<Runtime>::active_era_start_session_index(), 5);
		assert_eq!(active_validators, vec![3, 5, 6, 8]);

		// flush the events.
		let _ = staking_events_since_last_call();

		assert_ok!(rc_client::Pallet::<Runtime>::relay_new_offence_paged(
			RuntimeOrigin::root(),
			vec![
				(
					5,
					rc_client::Offence {
						offender: 5,
						reporters: vec![],
						slash_fraction: Perbill::from_percent(50),
					}
				),
				(
					5,
					rc_client::Offence {
						offender: 3,
						reporters: vec![],
						slash_fraction: Perbill::from_percent(50),
					}
				)
			]
		));

		assert_eq!(
			staking_events_since_last_call(),
			vec![
				staking_async::Event::OffenceReported {
					offence_era: 1,
					validator: 5,
					fraction: Perbill::from_percent(50)
				},
				staking_async::Event::OffenceReported {
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
			vec![staking_async::Event::SlashComputed {
				offence_era: 1,
				slash_era: 3,
				offender: 5,
				page: 0
			},]
		);
		roll_next();
		assert_eq!(
			staking_events_since_last_call(),
			vec![staking_async::Event::SlashComputed {
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
			vec![staking_async::Event::Slashed { staker: 3, amount: 50 },]
		);
		roll_next();
		assert_eq!(
			staking_events_since_last_call(),
			vec![
				staking_async::Event::Slashed { staker: 5, amount: 50 },
				staking_async::Event::Slashed { staker: 110, amount: 50 }
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
			assert_eq!(Rotator::<Runtime>::era_start_session_index(1), Some(5));
			assert_eq!(active_validators, vec![3, 5, 6, 8]);

			// flush the events.
			let _ = staking_events_since_last_call();

			assert_ok!(rc_client::Pallet::<Runtime>::relay_new_offence_paged(
				RuntimeOrigin::root(),
				vec![
					(
						5,
						rc_client::Offence {
							offender: 5,
							reporters: vec![],
							slash_fraction: Perbill::from_percent(50),
						}
					),
					(
						5,
						rc_client::Offence {
							offender: 3,
							reporters: vec![],
							slash_fraction: Perbill::from_percent(50),
						}
					)
				]
			));

			assert_eq!(
				staking_events_since_last_call(),
				vec![
					staking_async::Event::OffenceReported {
						offence_era: 1,
						validator: 5,
						fraction: Perbill::from_percent(50)
					},
					staking_async::Event::OffenceReported {
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
					staking_async::Event::SlashComputed {
						offence_era: 1,
						slash_era: 1,
						offender: 5,
						page: 0
					},
					staking_async::Event::Slashed { staker: 5, amount: 50 },
					staking_async::Event::Slashed { staker: 110, amount: 50 }
				]
			);
			roll_next();
			assert_eq!(
				staking_events_since_last_call(),
				vec![
					staking_async::Event::SlashComputed {
						offence_era: 1,
						slash_era: 1,
						offender: 3,
						page: 0
					},
					staking_async::Event::Slashed { staker: 3, amount: 50 }
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
			assert_eq!(Rotator::<Runtime>::era_start_session_index(1), Some(5));
			assert_eq!(active_validators, vec![3, 5, 6, 8]);

			// flush the events.
			let _ = staking_events_since_last_call();

			assert_ok!(rc_client::Pallet::<Runtime>::relay_new_offence_paged(
				RuntimeOrigin::root(),
				vec![(
					5,
					rc_client::Offence {
						// this offender is unknown to the staking pallet.
						offender: 666,
						reporters: vec![],
						slash_fraction: Perbill::from_percent(50),
					}
				)]
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

		// GIVEN slash defer duration of 2 eras, active era = 3.
		assert_eq!(SlashDeferredDuration::get(), 2);
		assert_eq!(Rotator::<Runtime>::era_start_session_index(1), Some(5));
		// 1 era is reserved for the application of slashes.
		let oldest_reportable_era =
			Rotator::<Runtime>::active_era() - (SlashDeferredDuration::get() - 1);
		assert_eq!(oldest_reportable_era, 2);

		// WHEN we report an offence older than Era 2 (oldest reportable era).
		assert_ok!(rc_client::Pallet::<Runtime>::relay_new_offence_paged(
			RuntimeOrigin::root(),
			// offence is in era 1
			vec![(
				5,
				rc_client::Offence {
					offender: 3,
					reporters: vec![],
					slash_fraction: Perbill::from_percent(30),
				}
			)]
		));

		// THEN offence is ignored.
		assert_eq!(
			staking_events_since_last_call(),
			vec![staking_async::Event::OffenceTooOld {
				offence_era: 1,
				validator: 3,
				fraction: Perbill::from_percent(30)
			}]
		);

		// WHEN: report an offence for the session belonging to the previous era
		assert_eq!(Rotator::<Runtime>::era_start_session_index(2), Some(10));
		assert_ok!(rc_client::Pallet::<Runtime>::relay_new_offence_paged(
			RuntimeOrigin::root(),
			// offence is in era 2
			vec![(
				10,
				rc_client::Offence {
					offender: 3,
					reporters: vec![],
					slash_fraction: Perbill::from_percent(50),
				}
			)]
		));

		// THEN: offence is reported.
		assert_eq!(
			staking_events_since_last_call(),
			vec![staking_async::Event::OffenceReported {
				offence_era: 2,
				validator: 3,
				fraction: Perbill::from_percent(50)
			}]
		);

		// computed in the next block (will be applied in era 4)
		roll_next();
		assert_eq!(
			staking_events_since_last_call(),
			vec![staking_async::Event::SlashComputed {
				offence_era: 2,
				slash_era: 4,
				offender: 3,
				page: 0
			},]
		);

		// roll to the next era.
		roll_until_next_active(15);
		// ensure we are in era 4.
		assert_eq!(Rotator::<Runtime>::active_era(), 4);
		// clear staking events.
		let _ = staking_events_since_last_call();

		// the next block applies the slashes.
		roll_next();
		assert_eq!(
			staking_events_since_last_call(),
			vec![staking_async::Event::Slashed { staker: 3, amount: 50 }]
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
			assert_eq!(Rotator::<Runtime>::era_start_session_index(1), Some(5));

			assert_ok!(rc_client::Pallet::<Runtime>::relay_new_offence_paged(
				RuntimeOrigin::root(),
				// offence is in era 1
				vec![(
					5,
					rc_client::Offence {
						offender: 3,
						reporters: vec![],
						slash_fraction: Perbill::from_percent(50),
					}
				)]
			));

			// reported
			assert_eq!(
				staking_events_since_last_call(),
				vec![staking_async::Event::OffenceReported {
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
					staking_async::Event::SlashComputed {
						offence_era: 1,
						slash_era: 1,
						offender: 3,
						page: 0
					},
					staking_async::Event::Slashed { staker: 3, amount: 50 }
				]
			);

			// nothing left
			roll_next();
			assert_eq!(staking_events_since_last_call(), vec![]);
		});
}

mod poll_operations {
	use super::*;
	use pallet_election_provider_multi_block::verifier::{Status, Verifier};

	#[test]
	fn full_election_cycle_with_occasional_out_of_weight_completes() {
		ExtBuilder::default().local_queue().build().execute_with(|| {
			// given initial state of AH
			assert_eq!(System::block_number(), 1);
			assert_eq!(CurrentEra::<T>::get(), Some(0));
			assert_eq!(Rotator::<Runtime>::active_era_start_session_index(), 0);
			assert_eq!(ActiveEra::<T>::get(), Some(ActiveEraInfo { index: 0, start: Some(0) }));
			assert!(pallet_staking_async_rc_client::OutgoingValidatorSet::<T>::get().is_none());

			// receive first 3 session reports that don't trigger election
			for i in 0..3 {
				assert_ok!(rc_client::Pallet::<T>::relay_session_report(
					RuntimeOrigin::root(),
					rc_client::SessionReport {
						end_index: i,
						validator_points: vec![(1, 10)],
						activation_timestamp: None,
						leftover: false,
					}
				));

				assert_eq!(
					staking_events_since_last_call(),
					vec![StakingEvent::SessionRotated {
						starting_session: i + 1,
						active_era: 0,
						planned_era: 0
					}]
				);
			}

			// receive session 4 which causes election to start
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
			assert_eq!(CurrentPhase::<T>::get(), Phase::Snapshot(3));

			// create 1 snapshot page normally
			roll_next();
			assert_eq!(election_events_since_last_call(), vec![]);
			assert_eq!(CurrentPhase::<T>::get(), Phase::Snapshot(2));

			// next block won't have enough weight
			NextPollWeight::set(Some(crate::ah::weights::SMALL));
			roll_next();
			assert_eq!(
				election_events_since_last_call(),
				vec![ElectionEvent::UnexpectedPhaseTransitionOutOfWeight {
					from: Phase::Snapshot(2),
					to: Phase::Snapshot(1),
					required: Weight::from_parts(100, 0),
					had: Weight::from_parts(10, 0)
				}]
			);
			assert_eq!(CurrentPhase::<T>::get(), Phase::Snapshot(2));

			// next 2 blocks happen fine
			roll_next();
			assert_eq!(election_events_since_last_call(), vec![]);
			assert_eq!(CurrentPhase::<T>::get(), Phase::Snapshot(1));

			roll_next();
			assert_eq!(election_events_since_last_call(), vec![]);
			assert_eq!(CurrentPhase::<T>::get(), Phase::Snapshot(0));

			// transition to signed
			roll_next();
			assert_eq!(
				election_events_since_last_call(),
				vec![ElectionEvent::PhaseTransitioned {
					from: Phase::Snapshot(0),
					to: Phase::Signed(3)
				}]
			);
			assert_eq!(CurrentPhase::<T>::get(), Phase::Signed(3));

			// roll 1
			roll_next();
			assert_eq!(election_events_since_last_call(), vec![]);
			assert_eq!(CurrentPhase::<T>::get(), Phase::Signed(2));

			// unlikely: we have zero weight, we won't progress
			NextPollWeight::set(Some(Weight::default()));
			roll_next();
			assert_eq!(
				election_events_since_last_call(),
				vec![ElectionEvent::UnexpectedPhaseTransitionOutOfWeight {
					from: Phase::Signed(2),
					to: Phase::Signed(1),
					required: Weight::from_parts(10, 0),
					had: Weight::from_parts(0, 0)
				}]
			);
			assert_eq!(CurrentPhase::<T>::get(), Phase::Signed(2));

			// submit a signed solution
			let solution = OffchainWorkerMiner::<T>::mine_solution(3, true).unwrap();
			assert_ok!(MultiBlockSigned::register(RuntimeOrigin::signed(1), solution.score));
			for (index, page) in solution.solution_pages.into_iter().enumerate() {
				assert_ok!(MultiBlockSigned::submit_page(
					RuntimeOrigin::signed(1),
					index as u32,
					Some(Box::new(page))
				));
			}

			// go to signed validation
			roll_until_matches(|| CurrentPhase::<T>::get() == Phase::SignedValidation(6), false);
			assert_eq!(MultiBlockVerifier::status_storage(), Status::Ongoing(2));
			assert_eq!(
				election_events_since_last_call(),
				vec![ElectionEvent::PhaseTransitioned {
					from: Phase::Signed(0),
					to: Phase::SignedValidation(6)
				}]
			);

			// first block rolls okay
			roll_next();
			assert_eq!(verifier_events_since_last_call(), vec![VerifierEvent::Verified(2, 4)]);
			assert_eq!(election_events_since_last_call(), vec![]);
			assert_eq!(CurrentPhase::<T>::get(), Phase::SignedValidation(5));
			assert_eq!(MultiBlockVerifier::status_storage(), Status::Ongoing(1));

			// next block has not enough weight left for verification (verification of non-terminal
			// pages requires MEDIUM)
			NextPollWeight::set(Some(crate::ah::weights::SMALL));
			roll_next();
			assert_eq!(verifier_events_since_last_call(), vec![]);
			assert_eq!(
				election_events_since_last_call(),
				vec![ElectionEvent::UnexpectedPhaseTransitionOutOfWeight {
					from: Phase::SignedValidation(5),
					to: Phase::SignedValidation(4),
					required: Weight::from_parts(1010, 0),
					had: Weight::from_parts(10, 0)
				}]
			);
			assert_eq!(CurrentPhase::<T>::get(), Phase::SignedValidation(5));
			assert_eq!(MultiBlockVerifier::status_storage(), Status::Ongoing(1));

			// rest go by fine, roll until done
			roll_until_matches(|| CurrentPhase::<T>::get() == Phase::Done, false);
			assert_eq!(
				verifier_events_since_last_call(),
				vec![
					VerifierEvent::Verified(1, 0),
					VerifierEvent::Verified(0, 3),
					VerifierEvent::Queued(
						ElectionScore {
							minimal_stake: 100,
							sum_stake: 800,
							sum_stake_squared: 180000
						},
						None
					)
				]
			);
			assert_eq!(
				election_events_since_last_call(),
				vec![
					ElectionEvent::PhaseTransitioned {
						from: Phase::SignedValidation(0),
						to: Phase::Unsigned(3)
					},
					ElectionEvent::PhaseTransitioned { from: Phase::Unsigned(0), to: Phase::Done },
				]
			);

			// first export page goes by fine
			assert_eq!(pallet_staking_async::NextElectionPage::<T>::get(), None);
			roll_next();
			assert_eq!(
				election_events_since_last_call(),
				vec![ElectionEvent::PhaseTransitioned { from: Phase::Done, to: Phase::Export(1) }]
			);
			assert_eq!(CurrentPhase::<T>::get(), Phase::Export(1));
			assert_eq!(
				staking_events_since_last_call(),
				vec![StakingEvent::PagedElectionProceeded { page: 2, result: Ok(4) }]
			);
			assert_eq!(pallet_staking_async::NextElectionPage::<T>::get(), Some(1));

			// second page goes by fine
			roll_next();
			assert_eq!(election_events_since_last_call(), vec![]);
			assert_eq!(CurrentPhase::<T>::get(), Phase::Export(0));
			assert_eq!(
				staking_events_since_last_call(),
				vec![StakingEvent::PagedElectionProceeded { page: 1, result: Ok(0) }]
			);
			assert_eq!(pallet_staking_async::NextElectionPage::<T>::get(), Some(0));

			// last (LARGE page) runs out of weight
			NextPollWeight::set(Some(crate::ah::weights::MEDIUM));
			roll_next();
			assert_eq!(election_events_since_last_call(), vec![]);
			assert_eq!(CurrentPhase::<T>::get(), Phase::Export(0));
			assert_eq!(
				staking_events_since_last_call(),
				vec![StakingEvent::Unexpected(
					pallet_staking_async::UnexpectedKind::PagedElectionOutOfWeight {
						page: 0,
						required: Weight::from_parts(1000, 0),
						had: Weight::from_parts(100, 0)
					}
				)]
			);
			assert_eq!(pallet_staking_async::NextElectionPage::<T>::get(), Some(0));

			// next time it goes by fine
			roll_next();
			assert_eq!(
				election_events_since_last_call(),
				vec![ElectionEvent::PhaseTransitioned { from: Phase::Export(0), to: Phase::Off }]
			);
			assert_eq!(CurrentPhase::<T>::get(), Phase::Off);
			assert_eq!(
				staking_events_since_last_call(),
				vec![StakingEvent::PagedElectionProceeded { page: 0, result: Ok(0) }]
			);
			assert_eq!(pallet_staking_async::NextElectionPage::<T>::get(), None);

			// outgoing message is queued
			assert!(pallet_staking_async_rc_client::OutgoingValidatorSet::<T>::get().is_some());
		})
	}

	#[test]
	fn slashing_processing_while_election() {
		// This is merely a more realistic example of the above. As staking is ready to receive the
		// election result, an ongoing slash will cause too much weight to be consumed
		// on-initialize, causing not enough weight in the on-poll to process. Everything works as
		// expected, but we get a bit slow.
		//
		// The only other meaningful difference is here that we see in action that first on-init
		// runs, and then the leftover weight is given to on-poll. This is done through the mock
		// setup of this test.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			// first, we roll 1 era so have some validators to slash
			let active_validators = roll_until_next_active(0);
			let _ = staking_events_since_last_call();

			// given initial state of AH
			assert_eq!(System::block_number(), 27);
			assert_eq!(CurrentEra::<T>::get(), Some(1));
			assert_eq!(Rotator::<Runtime>::active_era_start_session_index(), 5);
			assert_eq!(ActiveEra::<T>::get(), Some(ActiveEraInfo { index: 1, start: Some(1000) }));
			assert!(pallet_staking_async_rc_client::OutgoingValidatorSet::<T>::get().is_none());

			// receive first 3 session reports that don't trigger election
			for i in 5..8 {
				assert_ok!(rc_client::Pallet::<T>::relay_session_report(
					RuntimeOrigin::root(),
					rc_client::SessionReport {
						end_index: i,
						validator_points: vec![(1, 10)],
						activation_timestamp: None,
						leftover: false,
					}
				));

				assert_eq!(
					staking_events_since_last_call(),
					vec![StakingEvent::SessionRotated {
						starting_session: i + 1,
						active_era: 1,
						planned_era: 1
					}]
				);
			}

			// receive session 4 which causes election to start
			assert_ok!(rc_client::Pallet::<T>::relay_session_report(
				RuntimeOrigin::root(),
				rc_client::SessionReport {
					end_index: 8,
					validator_points: vec![(1, 10)],
					activation_timestamp: None,
					leftover: false,
				}
			));

			assert_eq!(
				staking_events_since_last_call(),
				vec![StakingEvent::SessionRotated {
					starting_session: 9,
					active_era: 1,
					// planned era 1 indicates election start signal is sent.
					planned_era: 2
				}]
			);

			// roll until signed and submit a solution.
			roll_until_matches(|| MultiBlock::current_phase().is_signed(), false);
			let solution = OffchainWorkerMiner::<T>::mine_solution(3, true).unwrap();
			assert_ok!(MultiBlockSigned::register(RuntimeOrigin::signed(1), solution.score));
			for (index, page) in solution.solution_pages.into_iter().enumerate() {
				assert_ok!(MultiBlockSigned::submit_page(
					RuntimeOrigin::signed(1),
					index as u32,
					Some(Box::new(page))
				));
			}

			// then roll to done, waiting for staking to start processing it. Indeed, something is
			// queued for export now.
			roll_until_matches(|| MultiBlock::current_phase().is_done(), false);
			assert!(MultiBlockVerifier::queued_score().is_some());

			assert_ok!(rc_client::Pallet::<Runtime>::relay_new_offence_paged(
				RuntimeOrigin::root(),
				vec![(
					// index of the last received session report
					8,
					rc_client::Offence {
						offender: active_validators[0],
						reporters: vec![],
						slash_fraction: Perbill::from_percent(10),
					}
				)]
			));

			assert_eq!(
				staking_events_since_last_call(),
				vec![StakingEvent::OffenceReported {
					offence_era: 1,
					validator: active_validators[0],
					fraction: Perbill::from_percent(10)
				}]
			);

			assert!(pallet_staking_async::NextElectionPage::<T>::get().is_none());

			// now as we roll-next, because weight of `process_offence_queue` is max block...
			roll_next();
			// staking has not moved forward in terms of fetching election pages
			assert!(pallet_staking_async::NextElectionPage::<T>::get().is_none());
			// same with our EPMB
			assert!(MultiBlock::current_phase().is_done());
			// and for tracking we have
			assert_eq!(
				staking_events_since_last_call(),
				vec![
					// slash processing happened..
					StakingEvent::SlashComputed {
						offence_era: 1,
						slash_era: 3,
						offender: active_validators[0],
						page: 0
					},
					// but not this.
					StakingEvent::Unexpected(
						pallet_staking_async::UnexpectedKind::PagedElectionOutOfWeight {
							page: 2,
							required: Weight::from_parts(100, 0),
							had: Weight::from_parts(0, 0)
						}
					)
				]
			);
		});
	}
}
