use crate::ah::mock::*;

use pallet_staking::{ActiveEra, ActiveEraInfo, CurrentEra, CurrentPlannedSession};

// Tests that are specific to Asset Hub.
#[test]
fn on_receive_session_report() {
	// todo(ank4n):
    // - Ensure some validator points are sent.
    // - Ensure staking takes into account those validator points.
    // - Ensure staking rewards can be claimed only after era change.
    ExtBuilder::default().local_queue().build().execute_with(|| {
        // verify initial state of ah
        assert_eq!(System::block_number(), 0);
        assert_eq!(CurrentPlannedSession::<Runtime>::get(), 0);
        assert_eq!(CurrentEra::<Runtime>::get(), Some(0));
        assert_eq!(pallet_staking::ErasStartSessionIndex::<Runtime>::get(0), Some(0));
        assert_eq!(
            ActiveEra::<Runtime>::get(),
            Some(ActiveEraInfo { index: 0, start: Some(0) })
        );
		// roll_until_matches(|| pallet_session::CurrentIndex::<Runtime>::get() == 10, false);
		assert_eq!(LocalQueue::get().unwrap(),  vec![]);
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
