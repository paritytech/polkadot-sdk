#[cfg(test)]
pub mod ah;
#[cfg(test)]
pub mod rc;

#[cfg(test)]
pub mod shared;

// shared tests.
#[cfg(test)]
mod tests {
	use super::*;
	use crate::rc::RootOffences;
	use ah_client::OperatingMode;
	use frame::testing_prelude::*;
	use pallet_election_provider_multi_block as multi_block;
	use pallet_staking as staking_classic;
	use pallet_staking_async::{ActiveEra, ActiveEraInfo};
	use pallet_staking_async_ah_client as ah_client;
	use pallet_staking_async_rc_client as rc_client;

	#[test]
	fn rc_session_change_reported_to_ah() {
		// sets up AH chain with current and active era.
		shared::put_ah_state(ah::ExtBuilder::default().build());
		shared::put_rc_state(rc::ExtBuilder::default().build());
		// shared::RC_STATE.with(|state| *state.get_mut() = rc::ExtBuilder::default().build());

		// initial state of ah
		shared::in_ah(|| {
			assert_eq!(frame_system::Pallet::<ah::Runtime>::block_number(), 1);
			assert_eq!(pallet_staking_async::CurrentEra::<ah::Runtime>::get(), Some(0));
			assert_eq!(
				ActiveEra::<ah::Runtime>::get(),
				Some(ActiveEraInfo { index: 0, start: Some(0) })
			);
		});

		shared::in_rc(|| {
			// initial state of rc
			assert_eq!(ah_client::Mode::<rc::Runtime>::get(), OperatingMode::Active);
			// go to session 1 in RC and test.
			// when
			assert!(frame_system::Pallet::<rc::Runtime>::block_number() == 1);

			// given end session 0, start session 1, plan 2
			rc::roll_until_matches(
				|| pallet_session::CurrentIndex::<rc::Runtime>::get() == 1,
				true,
			);

			// then
			assert_eq!(frame_system::Pallet::<rc::Runtime>::block_number(), rc::Period::get());
		});

		shared::in_rc(|| {
			// roll a few more sessions
			rc::roll_until_matches(
				|| pallet_session::CurrentIndex::<rc::Runtime>::get() == 4,
				true,
			);
		});

		shared::in_ah(|| {
			// ah's rc-client has also progressed some blocks, equal to 4 sessions
			assert_eq!(frame_system::Pallet::<ah::Runtime>::block_number(), 120);
			// election is ongoing, and has just started
			assert!(matches!(
				multi_block::CurrentPhase::<ah::Runtime>::get(),
				multi_block::Phase::Snapshot(_)
			));
		});

		// go to session 5 in rc, and forward AH too.
		shared::in_rc(|| {
			rc::roll_until_matches(
				|| pallet_session::CurrentIndex::<rc::Runtime>::get() == 5,
				true,
			);
		});

		// ah has bumped the current era, but not the active era
		shared::in_ah(|| {
			assert_eq!(pallet_staking_async::CurrentEra::<ah::Runtime>::get(), Some(1));
			assert_eq!(
				ActiveEra::<ah::Runtime>::get(),
				Some(ActiveEraInfo { index: 0, start: Some(0) })
			);
		});

		// go to session 6 in rc, and forward AH too.
		shared::in_rc(|| {
			rc::roll_until_matches(
				|| pallet_session::CurrentIndex::<rc::Runtime>::get() == 6,
				true,
			);
		});
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

	#[test]
	fn ah_know_good_era_duration() {
		// era duration and rewards work.
	}

	#[test]
	fn ah_takes_over_staking_post_migration() {
		// SCENE (1): Pre AHM Migration
		shared::put_rc_state(rc::ExtBuilder::default().pre_migration().build());
		shared::put_ah_state(ah::ExtBuilder::default().pre_migration().build());

		shared::in_rc(|| {
			assert!(staking_classic::ActiveEra::<rc::Runtime>::get().is_none());

			// - staking-classic is active on RC.
			rc::roll_until_matches(
				|| {
					staking_classic::ActiveEra::<rc::Runtime>::get().map(|a| a.index).unwrap_or(0) ==
						1
				},
				true,
			);

			// offence is handled by RC.
			assert!(staking_classic::UnappliedSlashes::<rc::Runtime>::get(5).is_empty());

			assert_ok!(RootOffences::create_offence(
				rc::RuntimeOrigin::root(),
				vec![(2, Perbill::from_percent(100))],
			));

			// offence is expected to be deferred to era 1 + 3 = 4
			assert_eq!(staking_classic::UnappliedSlashes::<rc::Runtime>::get(4).len(), 1);
		});

		// Ensure AH does not receive any
		// - offences
		// - session change reports.
		assert_eq!(shared::CounterRCAHNewOffence::get(), 0);
		assert_eq!(shared::CounterRCAHSessionReport::get(), 0);

		// SCENE (2): AHM migration begins
		shared::in_rc(|| {
			let pre_migration_era_points =
				staking_classic::ErasRewardPoints::<rc::Runtime>::get(1).total;
			ah_client::Pallet::<rc::Runtime>::on_migration_start();
			assert_eq!(ah_client::Mode::<rc::Runtime>::get(), OperatingMode::Buffered);
			// get current session
			let mut current_session = pallet_session::CurrentIndex::<rc::Runtime>::get();
			// go forward by more than `SessionsPerEra` sessions.
			rc::roll_until_matches(
				|| {
					pallet_session::CurrentIndex::<rc::Runtime>::get() ==
						current_session + ah::SessionsPerEra::get() + 1
				},
				true,
			);
			current_session = pallet_session::CurrentIndex::<rc::Runtime>::get();

			// ensure era is still 1 on RC.
			// (Session events are received by AHClient and never passed on to staking-classic once
			// migration starts)
			assert_eq!(staking_classic::ActiveEra::<rc::Runtime>::get().unwrap().index, 1);
			// no new era is planned
			assert_eq!(staking_classic::CurrentEra::<rc::Runtime>::get().unwrap(), 1);

			// no new block author points accumulated
			assert_eq!(
				staking_classic::ErasRewardPoints::<rc::Runtime>::get(1).total,
				pre_migration_era_points
			);

			// let's create a new offence.
			assert_ok!(RootOffences::create_offence(
				rc::RuntimeOrigin::root(),
				vec![(5, Perbill::from_percent(100))],
			));

			// no new unapplied slashes are created (other than the previously created).
			assert_eq!(staking_classic::UnappliedSlashes::<rc::Runtime>::get(4).len(), 1);

			// there is a buffered offence in the AHClient.
			assert_eq!(ah_client::BufferedOffences::<rc::Runtime>::get().len(), 1);
			assert_eq!(
				ah_client::BufferedOffences::<rc::Runtime>::get()[0],
				(
					current_session,
					vec![rc_client::Offence {
						offender: 5,
						reporters: vec![],
						slash_fraction: Perbill::from_percent(100),
					}],
				)
			);
		});

		// Ensure AH still does not receive any offence while migration is ongoing.
		assert_eq!(shared::CounterRCAHNewOffence::get(), 0);
		assert_eq!(shared::CounterRCAHSessionReport::get(), 0);

		// let's migrate state from RC::staking-classic to AH::staking-async
		shared::migrate_state();

		// SCENE (3): AHM migration ends.
		// TODO
		// - era reward points during migration are accounted correctly.
		shared::in_rc(|| {
			ah_client::Pallet::<rc::Runtime>::on_migration_end();
			assert_eq!(ah_client::Mode::<rc::Runtime>::get(), OperatingMode::Active);

			// offence in the migration period is reported to AH.
			assert_eq!(shared::CounterRCAHNewOffence::get(), 1);
		});

		shared::in_ah(|| {
			// TODO: Centralise following migration end logic in one place.
			// rc_client::Pallet::<ah::Runtime>::on_migration_end();

			// This should be done at the end of migration
			pallet_staking_async::ForceEra::<ah::Runtime>::set(
				pallet_staking_async::Forcing::NotForcing,
			);

			assert_eq!(
				pallet_staking_async::OffenceQueue::<ah::Runtime>::get(1, 5).unwrap(),
				pallet_staking_async::slashing::OffenceRecord {
					reporter: None,
					reported_era: 1,
					exposure_page: 0,
					slash_fraction: Perbill::from_percent(100),
					prior_slash_fraction: Perbill::from_percent(0),
				}
			);

			// next block would process this offence
			ah::roll_next();

			assert_eq!(pallet_staking_async::OffenceQueue::<ah::Runtime>::get(1, 5), None);
			// offence is deferred by two eras, ie 1 + 2 = 3. Note that this is one era less than
			// staking-classic since slashing happens in multi-block, and we want to apply all
			// slashes before the era 4 starts.
			assert!(pallet_staking_async::UnappliedSlashes::<ah::Runtime>::get(
				3,
				(5, Perbill::from_percent(100), 0)
			)
			.is_some());
		});

		// NOW: lets verify we kick off the election at the appropriate time
		shared::in_ah(|| {
			// roll another block just to strongly prove election is not kicked off at the end of
			// migration.
			ah::roll_next();

			// ensure no election is kicked off yet
			// (when election is kicked off, current_era = active_era + 1)
			assert_eq!(pallet_staking_async::CurrentEra::<ah::Runtime>::get(), Some(1));
			assert_eq!(pallet_staking_async::ActiveEra::<ah::Runtime>::get().unwrap().index, 1);
			// also no session report is sent to AH yet.
			assert_eq!(shared::CounterRCAHSessionReport::get(), 0);
		});

		// It was more than 6 sessions since the last election, on RC, so an election is already
		// overdue. The next session change should trigger an election.

		shared::in_rc(|| {
			assert_eq!(pallet_session::CurrentIndex::<rc::Runtime>::get(), 12);
			rc::roll_until_matches(
				|| pallet_session::CurrentIndex::<rc::Runtime>::get() == 13,
				true,
			);
		});

		// AH receives the session report.
		assert_eq!(shared::CounterRCAHSessionReport::get(), 1);
		shared::in_ah(|| {
			assert_eq!(
				pallet_staking_async::ForceEra::<ah::Runtime>::get(),
				pallet_staking_async::Forcing::NotForcing
			);
			assert_eq!(pallet_staking_async::ActiveEra::<ah::Runtime>::get().unwrap().index, 1);
			assert_eq!(pallet_staking_async::CurrentEra::<ah::Runtime>::get(), Some(1 + 1));

			// ensure new validator is sent once election is complete.
			ah::roll_until_matches(|| shared::CounterAHRCValidatorSet::get() == 1, true)
		});

		shared::in_rc(|| {
			let (planned_era, next_validator_set) =
				ah_client::ValidatorSet::<rc::Runtime>::get().unwrap();
			assert_eq!(planned_era, 2);
			assert!(next_validator_set.len() >= rc::MinimumValidatorSetSize::get() as usize);
		});

		shared::in_ah(|| {
			assert_eq!(pallet_staking_async::ActiveEra::<ah::Runtime>::get().unwrap().index, 1);
			// at next session, the validator set is queued but not applied yet.
			ah::roll_until_matches(|| shared::CounterRCAHSessionReport::get() == 2, true);
			// active era is still 1.
			assert_eq!(pallet_staking_async::ActiveEra::<ah::Runtime>::get().unwrap().index, 1);
			// the following session, the validator set is applied.
			ah::roll_until_matches(|| shared::CounterRCAHSessionReport::get() == 3, true);
			assert_eq!(pallet_staking_async::ActiveEra::<ah::Runtime>::get().unwrap().index, 2);
		});
	}
}
