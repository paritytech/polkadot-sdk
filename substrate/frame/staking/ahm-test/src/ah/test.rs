use crate::ah::mock::*;

use frame_support::{assert_noop, assert_ok};
use pallet_election_provider_multi_block::{Event as ElectionEvent, Phase};
use pallet_staking::{
	ActiveEra, ActiveEraInfo, CurrentEra, CurrentPlannedSession, Event as StakingEvent,
};
use pallet_staking_rc_client::ValidatorSetReport;
use pallet_staking_rc_client as rc_client;

// Tests that are specific to Asset Hub.
#[test]
fn on_receive_session_report() {
	ExtBuilder::default().local_queue().build().execute_with(|| {
		// GIVEN genesis state of ah
		assert_eq!(System::block_number(), 0);
		assert_eq!(CurrentPlannedSession::<T>::get(), 0);
		assert_eq!(CurrentEra::<T>::get(), Some(0));
		assert_eq!(pallet_staking::ErasStartSessionIndex::<T>::get(0), Some(0));
		assert_eq!(ActiveEra::<T>::get(), Some(ActiveEraInfo { index: 0, start: Some(0) }));
		// initialise events.
		roll_next();

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
		assert_eq!(CurrentPlannedSession::<T>::get(), 2);
		let era_points = pallet_staking::ErasRewardPoints::<T>::get(&0);
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

			let era_points = pallet_staking::ErasRewardPoints::<T>::get(&0);
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

		// current planned session is 4 (ongoing 3, last ended 2)
		assert_eq!(CurrentPlannedSession::<T>::get(), 4);

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
			vec![ElectionEvent::PhaseTransitioned { from: Phase::Off, to: Phase::Snapshot(3) }]
		);

		// roll some blocks to finish election.
		roll_many(31);
		// todo: make sense of these phases!!
		assert_eq!(
			election_events_since_last_call(),
			vec![
				ElectionEvent::PhaseTransitioned { from: Phase::Snapshot(0), to: Phase::Signed(3) },
				ElectionEvent::PhaseTransitioned {
					from: Phase::Signed(0),
					to: Phase::SignedValidation(5)
				},
				ElectionEvent::PhaseTransitioned {
					from: Phase::SignedValidation(0),
					to: Phase::Unsigned(3)
				},
				ElectionEvent::PhaseTransitioned { from: Phase::Unsigned(0), to: Phase::Done },
				ElectionEvent::PhaseTransitioned { from: Phase::Done, to: Phase::Export(2) },
				ElectionEvent::PhaseTransitioned { from: Phase::Export(0), to: Phase::Off }
			]
		);


		assert_eq!(
			LocalQueue::get().unwrap(),
			vec![(
				42,
				OutgoingMessages::ValidatorSet(ValidatorSetReport {
					new_validator_set: vec![3, 5, 6, 8],
					id: 0, // todo: ensure this is sent as CurrentEra.
					prune_up_to: 0, // todo: Ensure this is sent.
					leftover: false
				})
			)]
		);
	})
}

#[test]
fn on_new_offence() {
	// todo(ank4n):
	// - Offence Report sent to AH.
	// - Offence processed, and slashed.
	// - Check if offenders only one at a time!
	// Tests processing of offence and slashing
	ExtBuilder::default().local_queue().build().execute_with(|| {});
}
