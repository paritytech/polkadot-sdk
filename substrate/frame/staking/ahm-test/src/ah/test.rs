use crate::ah::mock::*;

use frame_support::{assert_noop, assert_ok};
use pallet_staking::{ActiveEra, ActiveEraInfo, CurrentEra, CurrentPlannedSession};
use pallet_staking_rc_client as rc_client;

// Tests that are specific to Asset Hub.
#[test]
fn on_receive_session_report() {
	// todo(ank4n):
	// - Ensure some validator points are sent.
	// - Ensure staking takes into account those validator points.
	// - Ensure staking rewards can be claimed only after era change.
	ExtBuilder::default().local_queue().build().execute_with(|| {
		// GIVEN genesis state of ah
		assert_eq!(System::block_number(), 0);
		assert_eq!(CurrentPlannedSession::<T>::get(), 0);
		assert_eq!(CurrentEra::<T>::get(), Some(0));
		assert_eq!(pallet_staking::ErasStartSessionIndex::<T>::get(0), Some(0));
		assert_eq!(ActiveEra::<T>::get(), Some(ActiveEraInfo { index: 0, start: Some(0) }));

		// WHEN session ends on RC and session report is received by AH.
        let session_report = rc_client::SessionReport {
            end_index: 0,
            validator_points: (1..9).into_iter().map(|v| (v as AccountId, v*10)).collect(),
            activation_timestamp: None,
            leftover: false,
        };

		assert_ok!(rc_client::Pallet::<T>::relay_session_report(
			RuntimeOrigin::root(),
			session_report.clone(),
		));

		// THEN session 1 starts and session 2 is planned.
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


		assert_eq!(LocalQueue::get().unwrap(), vec![]);
	})
}

#[test]
fn start_election_prep() {
	// todo(ank4n):
	// - At session x, election prep should start.
	// - roll until election finishes.
	// - validator set should be sent to RC.
	ExtBuilder::default().local_queue().build().execute_with(|| {
		// roll_until_matches(|| pallet_session::CurrentIndex::<Runtime>::get() == 10, false);
		assert_eq!(LocalQueue::get().unwrap(), vec![]);
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
