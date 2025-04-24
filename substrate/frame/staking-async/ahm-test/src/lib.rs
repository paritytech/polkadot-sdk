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
	use frame_support::traits::Get;
	use pallet_election_provider_multi_block as multi_block;
	use pallet_staking as staking_classic;
	use pallet_staking_async::{ActiveEra, ActiveEraInfo, Forcing};
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
	fn ah_takes_over_staking_post_migration() {
		// SCENE (1): Pre AHM Migration
		shared::put_rc_state(
			rc::ExtBuilder::default()
				.pre_migration()
				// set session keys for all "potential" validators
				.session_keys(vec![1, 2, 3, 4, 5, 6, 7, 8])
				.build(),
		);
		shared::put_ah_state(ah::ExtBuilder::default().build());

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

			// No offence exist so far
			assert!(staking_classic::UnappliedSlashes::<rc::Runtime>::get(4).is_empty());

			dbg!(pallet_session::Validators::<rc::Runtime>::get());

			assert_ok!(RootOffences::create_offence(
				rc::RuntimeOrigin::root(),
				vec![(2, Perbill::from_percent(100))],
				None,
				None
			));

			// offence is expected to be deferred to era 1 + 3 = 4
			assert_eq!(staking_classic::UnappliedSlashes::<rc::Runtime>::get(4).len(), 1);
		});

		// nothing happened in ah-staking so far
		shared::in_ah(|| {
			// Ensure AH does not receive any
			// - offences
			// - session change reports.
			assert_eq!(shared::CounterRCAHNewOffence::get(), 0);
			assert_eq!(shared::CounterRCAHSessionReport::get(), 0);

			assert_eq!(ah::mock::staking_events_since_last_call(), vec![]);
		});

		// SCENE (2): AHM migration begins
		let mut pre_migration_block_number = 0;
		shared::in_rc(|| {
			rc::roll_next();

			let pre_migration_era_points =
				staking_classic::ErasRewardPoints::<rc::Runtime>::get(1).total;

			ah_client::Pallet::<rc::Runtime>::on_migration_start();
			assert_eq!(ah_client::Mode::<rc::Runtime>::get(), OperatingMode::Buffered);

			// get current session
			let mut current_session = pallet_session::CurrentIndex::<rc::Runtime>::get();
			pre_migration_block_number = frame_system::Pallet::<rc::Runtime>::block_number();

			// assume migration takes at least one era
			// go forward by more than `SessionsPerEra` sessions -- staking will not rotate a new
			// era.
			rc::roll_until_matches(
				|| {
					pallet_session::CurrentIndex::<rc::Runtime>::get() ==
						current_session + ah::SessionsPerEra::get() + 1
				},
				true,
			);
			current_session = pallet_session::CurrentIndex::<rc::Runtime>::get();
			let migration_start_block_number = frame_system::Pallet::<rc::Runtime>::block_number();

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

			// some validator points have been recorded in ah-client
			assert_eq!(
				ah_client::ValidatorPoints::<rc::Runtime>::iter().count(),
				1,
				"only 11 has authored blocks in rc"
			);
			assert_eq!(
				ah_client::ValidatorPoints::<rc::Runtime>::get(&11),
				(migration_start_block_number - pre_migration_block_number) as u32 *
					<<rc::Runtime as ah_client::Config>::PointsPerBlock as Get<u32>>::get()
			);

			// let's create a new offence.
			assert_ok!(RootOffences::create_offence(
				rc::RuntimeOrigin::root(),
				vec![(5, Perbill::from_percent(100))],
				None,
				None,
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
		shared::in_ah(|| {
			assert_eq!(shared::CounterRCAHNewOffence::get(), 0);
			assert_eq!(shared::CounterRCAHSessionReport::get(), 0);

			assert_eq!(ah::mock::staking_events_since_last_call(), vec![]);
		});

		// let's migrate state from RC::staking-classic to AH::staking-async
		shared::migrate_state();

		// SCENE (3): AHM migration ends.
		shared::in_rc(|| {
			ah_client::Pallet::<rc::Runtime>::on_migration_end();
			assert_eq!(ah_client::Mode::<rc::Runtime>::get(), OperatingMode::Active);

			// offence in the migration period is reported to AH.
			assert_eq!(shared::CounterRCAHNewOffence::get(), 1);
		});

		let mut post_migration_era_reward_points = 0;
		shared::in_ah(|| {
			post_migration_era_reward_points =
				pallet_staking_async::ErasRewardPoints::<ah::Runtime>::get(1).total;
			// staking async has always been in NotForcing, not doing anything since no session
			// reports come in
			assert_eq!(pallet_staking_async::ForceEra::<ah::Runtime>::get(), Forcing::NotForcing);

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

			assert_eq!(
				ah::mock::staking_events_since_last_call(),
				vec![
					pallet_staking_async::Event::OffenceReported {
						offence_era: 1,
						validator: 5,
						fraction: Perbill::from_percent(100)
					},
					pallet_staking_async::Event::SlashComputed {
						offence_era: 1,
						slash_era: 3,
						offender: 5,
						page: 0
					},
				]
			);

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

		let mut post_migration_session_block_number = 0;
		shared::in_rc(|| {
			assert_eq!(pallet_session::CurrentIndex::<rc::Runtime>::get(), 12);
			rc::roll_until_matches(
				|| pallet_session::CurrentIndex::<rc::Runtime>::get() == 13,
				true,
			);
			post_migration_session_block_number =
				frame_system::Pallet::<rc::Runtime>::block_number();

			// all the buffered validators points are flushed
			assert_eq!(ah_client::ValidatorPoints::<rc::Runtime>::iter().count(), 0,);
		});

		// AH receives the session report.
		assert_eq!(shared::CounterRCAHSessionReport::get(), 1);
		shared::in_ah(|| {
			assert_eq!(pallet_staking_async::ActiveEra::<ah::Runtime>::get().unwrap().index, 1);
			assert_eq!(pallet_staking_async::CurrentEra::<ah::Runtime>::get(), Some(1 + 1));

			// by now one session report should have been received in staking
			assert_eq!(
				ah::rc_client_events_since_last_call(),
				vec![
					rc_client::Event::OffenceReceived { slash_session: 12, offences_count: 1 },
					rc_client::Event::SessionReportReceived {
						end_index: 12,
						activation_timestamp: None,
						validator_points_counts: 1,
						leftover: false
					}
				]
			);

			assert_eq!(
				ah::mock::staking_events_since_last_call(),
				vec![pallet_staking_async::Event::SessionRotated {
					starting_session: 13,
					active_era: 1,
					planned_era: 2
				}]
			);

			// all expected era reward points are here
			assert_eq!(
				pallet_staking_async::ErasRewardPoints::<ah::Runtime>::get(1).total,
				((post_migration_session_block_number - pre_migration_block_number) * 20) as u32 +
				// --- ^^ these were buffered in ah-client
					post_migration_era_reward_points // --- ^^ these were migrated as part of AHM
			);

			// ensure new validator is sent once election is complete.
			ah::roll_until_matches(|| shared::CounterAHRCValidatorSet::get() == 1, true);

			assert_eq!(
				ah::staking_events_since_last_call(),
				vec![
					pallet_staking_async::Event::PagedElectionProceeded { page: 2, result: Ok(4) },
					pallet_staking_async::Event::PagedElectionProceeded { page: 1, result: Ok(0) },
					pallet_staking_async::Event::PagedElectionProceeded { page: 0, result: Ok(0) }
				]
			);
		});

		shared::in_rc(|| {
			assert_eq!(
				rc::ah_client_events_since_last_call(),
				vec![ah_client::Event::ValidatorSetReceived {
					id: 2,
					new_validator_set_count: 4,
					prune_up_to: None,
					leftover: false
				}]
			);

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
	fn election_provider_fails_to_start() {
		// call to ElectionProvider::start fails because it is already ongoing. What do we do?
	}

	#[test]
	fn overlapping_election() {
		// while one election is ongoing, enough sessions pass that we think we should plan yet
		// another era.
	}

	#[test]
	fn session_report_burst() {
		// AH is offline for a while, and it suddenly receives 3 eras worth of session reports. What
		// do we do?
	}
}
