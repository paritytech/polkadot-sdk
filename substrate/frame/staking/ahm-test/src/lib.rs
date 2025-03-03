#[cfg(test)]
pub mod ah;
#[cfg(test)]
pub mod rc;
#[cfg(test)]
pub mod shared;

#[cfg(test)]
mod tests {
	use std::cell::RefCell;

	use super::*;
	use codec::Decode;
	use frame::deps::frame_system;
	use pallet_staking::ActiveEraInfo;
	// shared tests.

	#[test]
	fn rc_session_change_reported_to_ah() {
		use std::rc::Rc;
		let mut rc = rc::ExtBuilder::default().build();
		let mut ah = ah::ExtBuilder::default().build();

		shared::AH_STATE.with(|state| {
			// set the shared thread local state to the one created here.
			state = &Rc::clone(&ah)
		});
		shared::RC_STATE.with(|state| {
			// set the shared thread local state to the one created here.
			state = &Rc::clone(&rc)
		});

		// initial state of ah
		ah.execute_with(|| {
			assert_eq!(frame_system::Pallet::<ah::Runtime>::block_number(), 0);
			assert_eq!(pallet_staking::CurrentPlannedSession::<ah::Runtime>::get(), 0);
			assert_eq!(pallet_staking::CurrentEra::<ah::Runtime>::get(), None);
			assert_eq!(pallet_staking::ActiveEra::<ah::Runtime>::get(), None);
		});

		// rc reports session change to ah
		rc.execute_with(|| {
			// when
			assert!(frame_system::Pallet::<rc::Runtime>::block_number() == 0);

			// given end session 0, start session 1, plan 2
			rc::roll_until_matches(
				|| pallet_session::CurrentIndex::<rc::Runtime>::get() == 1,
				true,
			);

			// then
			assert_eq!(frame_system::Pallet::<rc::Runtime>::block_number(), rc::Period::get());
		});

		rc.execute_with(|| {
			// roll a few more sessions
			rc::roll_until_matches(
				|| pallet_session::CurrentIndex::<rc::Runtime>::get() == 4,
				true,
			);
		});

		// ah's rc-client has reported the session change to staking
		ah.execute_with(|| {
			assert_eq!(frame_system::Pallet::<ah::Runtime>::block_number(), 0);
			assert_eq!(pallet_staking::CurrentPlannedSession::<ah::Runtime>::get(), 2);
			assert_eq!(pallet_staking::CurrentEra::<ah::Runtime>::get(), None);
			assert_eq!(pallet_staking::ActiveEra::<ah::Runtime>::get(), None);
		});

		// rc-client reports session change to staking
		// staking progresses era accordingly
		// staking calls `ElectionProvider::start` accordingly.
	}

	#[test]
	fn election_result_on_ah_reported_to_rc() {
		// when election result is complete
		// staking stores all exposures
		// validators reported to rc
		// validators enacted for next session
	}

	#[test]
	fn rc_continues_with_same_validators_if_ah_is_late() {
		// A test where ah is late to give us election result.
	}

	#[test]
	fn authoring_points_reported_to_ah_per_session() {}

	#[test]
	fn rc_is_late_to_report_session_change() {}

	#[test]
	fn pruning_is_at_least_bonding_duration() {}

	#[test]
	fn ah_eras_are_delayed() {
		// rc will trigger new sessions,
		// ah cannot start a new era (election fail)
		// we don't prune anything, because era should not be increased.
	}
}
