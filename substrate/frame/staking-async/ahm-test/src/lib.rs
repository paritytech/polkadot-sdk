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
	use crate::{
		ah::{rc_client_events_since_last_call, staking_events_since_last_call},
		rc::RootOffences,
	};
	use ah_client::OperatingMode;
	use frame::testing_prelude::*;
	use frame_support::traits::Get;
	use pallet_election_provider_multi_block as multi_block;
	use pallet_staking as staking_classic;
	use pallet_staking_async::{ActiveEra, ActiveEraInfo, Forcing};
	use pallet_staking_async_ah_client::{self as ah_client, OffenceSendQueue};
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
				// set a very low `MaxOffenceBatchSize` to test batching behavior
				.max_offence_batch_size(2)
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
			let current_session = pallet_session::CurrentIndex::<rc::Runtime>::get();
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

			// Verify buffered mode doesn't send anything to AH
			let offence_counter_before = shared::CounterRCAHNewOffence::get();

			// ---------- Offences in session 12 (6 total) ----------
			assert_eq!(pallet_session::CurrentIndex::<rc::Runtime>::get(), 12);

			// 3 for validator 2
			assert_ok!(RootOffences::create_offence(
				rc::RuntimeOrigin::root(),
				vec![(2, Perbill::from_percent(50))],
				None,
				None,
			));
			assert_ok!(RootOffences::create_offence(
				rc::RuntimeOrigin::root(),
				vec![(2, Perbill::from_percent(100))],
				None,
				None,
			));
			assert_ok!(RootOffences::create_offence(
				rc::RuntimeOrigin::root(),
				vec![(2, Perbill::from_percent(25))],
				None,
				None,
			));

			// 2 for validator 1
			assert_ok!(RootOffences::create_offence(
				rc::RuntimeOrigin::root(),
				vec![(1, Perbill::from_percent(75))],
				None,
				None,
			));
			assert_ok!(RootOffences::create_offence(
				rc::RuntimeOrigin::root(),
				vec![(1, Perbill::from_percent(60))],
				None,
				None,
			));

			// one for validator 4
			assert_ok!(RootOffences::create_offence(
				rc::RuntimeOrigin::root(),
				vec![(5, Perbill::from_percent(55))],
				None,
				None,
			));

			// Move to the next session to create offences in different sessions for batching test
			rc::roll_to_next_session(false);

			// ---------- Offences in session 13 (4 total) ----------
			assert_eq!(pallet_session::CurrentIndex::<rc::Runtime>::get(), 13);

			// 2 for validator 2
			assert_ok!(RootOffences::create_offence(
				rc::RuntimeOrigin::root(),
				vec![(2, Perbill::from_percent(90))],
				None,
				None,
			));
			assert_ok!(RootOffences::create_offence(
				rc::RuntimeOrigin::root(),
				vec![(2, Perbill::from_percent(80))],
				None,
				None,
			));
			// 1 for validator 1
			assert_ok!(RootOffences::create_offence(
				rc::RuntimeOrigin::root(),
				vec![(1, Perbill::from_percent(85))],
				None,
				None,
			));
			// 1 for validator 5
			assert_ok!(RootOffences::create_offence(
				rc::RuntimeOrigin::root(),
				vec![(5, Perbill::from_percent(45))],
				None,
				None,
			));

			// Move to another session and create more offences
			rc::roll_to_next_session(false);

			// ---------- Offences in session 14 (3 total) ----------
			assert_eq!(pallet_session::CurrentIndex::<rc::Runtime>::get(), 14);

			// 1 for validator 2
			assert_ok!(RootOffences::create_offence(
				rc::RuntimeOrigin::root(),
				vec![(2, Perbill::from_percent(70))],
				None,
				None,
			));
			// 1 for validator 1
			assert_ok!(RootOffences::create_offence(
				rc::RuntimeOrigin::root(),
				vec![(1, Perbill::from_percent(65))],
				None,
				None,
			));
			// 1 for validator 5
			assert_ok!(RootOffences::create_offence(
				rc::RuntimeOrigin::root(),
				vec![(5, Perbill::from_percent(40))],
				None,
				None,
			));

			// ---------- End of offences created so far (13 total) ----------

			// Verify nothing was sent to AH in buffered mode
			assert_eq!(
				shared::CounterRCAHNewOffence::get(),
				offence_counter_before,
				"No offences should be sent to AH in buffered mode"
			);

			// no new unapplied slashes are created in staking-classic (other than the previously
			// created).
			assert_eq!(staking_classic::UnappliedSlashes::<rc::Runtime>::get(4).len(), 1);

			// we have stored a total of 13 offences, in 7 pages
			assert_eq!(OffenceSendQueue::<rc::Runtime>::count(), 13);
			assert_eq!(OffenceSendQueue::<rc::Runtime>::pages(), 7);
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
			// Before migration ends, verify we have 9 buffered offences across multiple sessions
			assert_eq!(OffenceSendQueue::<rc::Runtime>::count(), 13);

			ah_client::Pallet::<rc::Runtime>::on_migration_end();
			assert_eq!(ah_client::Mode::<rc::Runtime>::get(), OperatingMode::Active);

			// `MaxOffenceBatchSize` is set to 2 in this test, so we will send over the 13 offences
			// in 7 next blocks.
			assert_eq!(crate::rc::MaxOffenceBatchSize::get(), 2);

			// in the first block we process the half finished page.
			rc::roll_next();
			assert_eq!(OffenceSendQueue::<rc::Runtime>::count(), 12);
			assert_eq!(shared::CounterRCAHNewOffence::get(), 1);

			// the rest are full pages of 2 offences each.
			rc::roll_next();
			assert_eq!(OffenceSendQueue::<rc::Runtime>::count(), 10);
			assert_eq!(shared::CounterRCAHNewOffence::get(), 3);
			rc::roll_next();
			assert_eq!(OffenceSendQueue::<rc::Runtime>::count(), 8);
			assert_eq!(shared::CounterRCAHNewOffence::get(), 5);
			rc::roll_next();
			assert_eq!(OffenceSendQueue::<rc::Runtime>::count(), 6);
			assert_eq!(shared::CounterRCAHNewOffence::get(), 7);
			rc::roll_next();
			assert_eq!(OffenceSendQueue::<rc::Runtime>::count(), 4);
			assert_eq!(shared::CounterRCAHNewOffence::get(), 9);
			rc::roll_next();
			assert_eq!(OffenceSendQueue::<rc::Runtime>::count(), 2);
			assert_eq!(shared::CounterRCAHNewOffence::get(), 11);
			rc::roll_next();
			assert_eq!(OffenceSendQueue::<rc::Runtime>::count(), 0);
			assert_eq!(shared::CounterRCAHNewOffence::get(), 13);
		});

		let mut post_migration_era_reward_points = 0;
		shared::in_ah(|| {
			// all offences are received by rc-client. The count of these events are not 13, because
			// in each batch we group offenders by session. Note that the sum of offenders count is
			// indeed 13.
			assert_eq!(
				rc_client_events_since_last_call(),
				vec![
					rc_client::Event::OffenceReceived { slash_session: 14, offences_count: 1 },
					rc_client::Event::OffenceReceived { slash_session: 14, offences_count: 2 },
					rc_client::Event::OffenceReceived { slash_session: 13, offences_count: 2 },
					rc_client::Event::OffenceReceived { slash_session: 13, offences_count: 2 },
					rc_client::Event::OffenceReceived { slash_session: 12, offences_count: 2 },
					rc_client::Event::OffenceReceived { slash_session: 12, offences_count: 2 },
					rc_client::Event::OffenceReceived { slash_session: 12, offences_count: 2 }
				]
			);

			post_migration_era_reward_points =
				pallet_staking_async::ErasRewardPoints::<ah::Runtime>::get(1).total;
			// staking async has always been in NotForcing, not doing anything since no session
			// reports come in
			assert_eq!(pallet_staking_async::ForceEra::<ah::Runtime>::get(), Forcing::NotForcing);

			// Verify all offences were properly queued in staking-async.
			// Should have offences for validators 1, 2, and 5 from different sessions (all map to
			// era 1)
			assert!(pallet_staking_async::OffenceQueue::<ah::Runtime>::get(1, 1).is_some());
			assert!(pallet_staking_async::OffenceQueue::<ah::Runtime>::get(1, 2).is_some());
			assert!(pallet_staking_async::OffenceQueue::<ah::Runtime>::get(1, 5).is_some());

			// Verify specific OffenceRecord structure for all three validators
			let offence_record_v1 =
				pallet_staking_async::OffenceQueue::<ah::Runtime>::get(1, 1).unwrap();
			assert_eq!(
				offence_record_v1,
				pallet_staking_async::slashing::OffenceRecord {
					reporter: None,
					reported_era: 1,
					exposure_page: 0,
					slash_fraction: Perbill::from_percent(85), /* Should be the highest slash
					                                            * fraction for validator 1 */
					prior_slash_fraction: Perbill::from_percent(0),
				}
			);

			let offence_record_v2 =
				pallet_staking_async::OffenceQueue::<ah::Runtime>::get(1, 2).unwrap();
			assert_eq!(
				offence_record_v2,
				pallet_staking_async::slashing::OffenceRecord {
					reporter: None,
					reported_era: 1,
					exposure_page: 0,
					slash_fraction: Perbill::from_percent(100), /* Should be the highest slash
					                                             * fraction for validator 2 */
					prior_slash_fraction: Perbill::from_percent(0),
				}
			);

			let offence_record_v5 =
				pallet_staking_async::OffenceQueue::<ah::Runtime>::get(1, 5).unwrap();
			assert_eq!(
				offence_record_v5,
				pallet_staking_async::slashing::OffenceRecord {
					reporter: None,
					reported_era: 1,
					exposure_page: 0,
					slash_fraction: Perbill::from_percent(55), /* Should be the highest slash
					                                            * fraction for validator 5 */
					prior_slash_fraction: Perbill::from_percent(0),
				}
			);

			// NOTE:
			// - We sent 13 total offences across 3 sessions and 7 messages (3 offences per session)
			// - Each session's offences trigger OffenceReported events when received
			// - But only the highest slash fraction per validator per era gets queued for
			//   processing
			// - So we see 13 OffenceReported events but only 3 offences in the processing queue
			// - The queue processing happens one offence per block in staking-async pallet.

			// Process all queued offences (one offence per block)
			// We have 3 offences queued (one per validator), so we need to roll 3 times
			for _ in 0..3 {
				ah::roll_next();
			}

			// Check that offences were processed for multiple validators
			let staking_events = ah::mock::staking_events_since_last_call();

			// Verify that OffenceReported events were emitted for all validators
			let offence_reported_events: Vec<_> = staking_events
				.iter()
				.filter_map(|event| {
					if let pallet_staking_async::Event::OffenceReported {
						offence_era,
						validator,
						fraction,
					} = event
					{
						Some((offence_era, validator, fraction))
					} else {
						None
					}
				})
				.collect();

			// Verify all OffenceReported events
			assert_eq!(
				offence_reported_events,
				vec![
					// 13 offences total, no deduplication here.
					(&1, &5, &Perbill::from_percent(40)),
					(&1, &2, &Perbill::from_percent(70)),
					(&1, &1, &Perbill::from_percent(65)),
					(&1, &1, &Perbill::from_percent(85)),
					(&1, &5, &Perbill::from_percent(45)),
					(&1, &2, &Perbill::from_percent(90)),
					(&1, &2, &Perbill::from_percent(80)),
					(&1, &1, &Perbill::from_percent(60)),
					(&1, &5, &Perbill::from_percent(55)),
					(&1, &2, &Perbill::from_percent(25)),
					(&1, &1, &Perbill::from_percent(75)),
					(&1, &2, &Perbill::from_percent(50)),
					(&1, &2, &Perbill::from_percent(100))
				]
			);

			// Verify that SlashComputed events were emitted for all three validators
			let slash_computed_events: Vec<_> = staking_events
				.iter()
				.filter_map(|event| {
					if let pallet_staking_async::Event::SlashComputed {
						offence_era,
						slash_era,
						offender,
						page,
					} = event
					{
						Some((offence_era, slash_era, offender, page))
					} else {
						None
					}
				})
				.collect();

			// Should have SlashComputed events for all three validators. Here we deduplicate for
			// maximum per session. Note: OffenceQueue uses StorageDoubleMap with Twox64Concat
			// hasher, so iteration order depends on hash(validator_id).
			assert_eq!(
				slash_computed_events,
				vec![
					(&1, &3, &5, &0), /* validator 5: offence_era=1, slash_era=3, offender=5,
					                   * page=0 */
					(&1, &3, &1, &0), /* validator 1: offence_era=1, slash_era=3, offender=1,
					                   * page=0 */
					(&1, &3, &2, &0), /* validator 2: offence_era=1, slash_era=3, offender=2,
					                   * page=0 */
				]
			);

			// Verify that all offences have been processed (no longer in queue)
			assert!(
				!pallet_staking_async::OffenceQueue::<ah::Runtime>::contains_key(1, 1),
				"Expected no remaining offences for validator 1"
			);
			assert!(
				!pallet_staking_async::OffenceQueue::<ah::Runtime>::contains_key(1, 2),
				"Expected no remaining offences for validator 2"
			);
			assert!(
				!pallet_staking_async::OffenceQueue::<ah::Runtime>::contains_key(1, 5),
				"Expected no remaining offences for validator 5"
			);
			// offence is deferred by two eras, ie 1 + 2 = 3. Note that this is one era less than
			// staking-classic since slashing happens in multi-block, and we want to apply all
			// slashes before the era 4 starts.
			// Check if at least one of the validators has an unapplied slash
			// Check for unapplied slashes for all validators with any of the slash fractions

			// Check validator 2 slashes
			let slash_v2_100_present = pallet_staking_async::UnappliedSlashes::<ah::Runtime>::get(
				3,
				(2, Perbill::from_percent(100), 0),
			)
			.is_some();
			let slash_v2_90_present = pallet_staking_async::UnappliedSlashes::<ah::Runtime>::get(
				3,
				(2, Perbill::from_percent(90), 0),
			)
			.is_some();
			let slash_v2_70_present = pallet_staking_async::UnappliedSlashes::<ah::Runtime>::get(
				3,
				(2, Perbill::from_percent(70), 0),
			)
			.is_some();

			let total_slashes_v2 =
				slash_v2_100_present as u8 + slash_v2_90_present as u8 + slash_v2_70_present as u8;
			assert_eq!(
				total_slashes_v2, 1,
				"Expected exactly 1 unapplied slash for validator 2, got {} (100%:{}, 90%:{}, 70%:{})",
				total_slashes_v2, slash_v2_100_present, slash_v2_90_present, slash_v2_70_present
			);

			// Check validator 1 slashes
			let slash_v1_75_present = pallet_staking_async::UnappliedSlashes::<ah::Runtime>::get(
				3,
				(1, Perbill::from_percent(75), 0),
			)
			.is_some();
			let slash_v1_85_present = pallet_staking_async::UnappliedSlashes::<ah::Runtime>::get(
				3,
				(1, Perbill::from_percent(85), 0),
			)
			.is_some();
			let slash_v1_65_present = pallet_staking_async::UnappliedSlashes::<ah::Runtime>::get(
				3,
				(1, Perbill::from_percent(65), 0),
			)
			.is_some();

			let total_slashes_v1 =
				slash_v1_75_present as u8 + slash_v1_85_present as u8 + slash_v1_65_present as u8;
			assert_eq!(
				total_slashes_v1, 1,
				"Expected exactly 1 unapplied slash for validator 1, got {} (75%:{}, 85%:{}, 65%:{})",
				total_slashes_v1, slash_v1_75_present, slash_v1_85_present, slash_v1_65_present
			);

			// Check validator 5 slashes
			let slash_v5_55_present = pallet_staking_async::UnappliedSlashes::<ah::Runtime>::get(
				3,
				(5, Perbill::from_percent(55), 0),
			)
			.is_some();
			let slash_v5_45_present = pallet_staking_async::UnappliedSlashes::<ah::Runtime>::get(
				3,
				(5, Perbill::from_percent(45), 0),
			)
			.is_some();
			let slash_v5_40_present = pallet_staking_async::UnappliedSlashes::<ah::Runtime>::get(
				3,
				(5, Perbill::from_percent(40), 0),
			)
			.is_some();

			let total_slashes_v5 =
				slash_v5_55_present as u8 + slash_v5_45_present as u8 + slash_v5_40_present as u8;
			assert_eq!(
				total_slashes_v5, 1,
				"Expected exactly 1 unapplied slash for validator 5, got {} (55%:{}, 45%:{}, 40%:{})",
				total_slashes_v5, slash_v5_55_present, slash_v5_45_present, slash_v5_40_present
			);
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
			rc::roll_to_next_session(true);
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
				vec![rc_client::Event::SessionReportReceived {
					end_index: 14,
					activation_timestamp: None,
					validator_points_counts: 1,
					leftover: false
				}]
			);

			assert_eq!(
				staking_events_since_last_call(),
				vec![pallet_staking_async::Event::SessionRotated {
					starting_session: 15,
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
	fn ah_knows_good_era_duration() {
		// era duration and rewards work.
	}

	#[test]
	fn election_provider_fails_to_start() {
		// call to ElectionProvider::start fails because it is already ongoing. What do we do?
	}

	#[test]
	fn overlapping_election_wont_happen() {
		// while one election is ongoing, enough sessions pass that we think we should plan yet
		// another era.
	}

	#[test]
	fn session_report_burst() {
		// AH is offline for a while, and it suddenly receives 3 eras worth of session reports. What
		// do we do?
	}

	mod message_queue_sizes {
		use super::*;
		use sp_core::crypto::AccountId32;

		#[test]
		fn normal_session_report() {
			assert_eq!(
				rc_client::SessionReport::<AccountId32> {
					end_index: 0,
					activation_timestamp: Some((0, 0)),
					leftover: false,
					validator_points: (0..1000)
						.map(|i| (AccountId32::from([i as u8; 32]), 1000))
						.collect(),
				}
				.encoded_size(),
				36_020
			);
		}

		#[test]
		fn normal_validator_set() {
			assert_eq!(
				rc_client::ValidatorSetReport::<AccountId32> {
					id: 42,
					leftover: false,
					new_validator_set: (0..1000)
						.map(|i| AccountId32::from([i as u8; 32]))
						.collect(),
					prune_up_to: Some(69),
				}
				.encoded_size(),
				32_012
			);
		}

		#[test]
		fn offence() {
			// when one validator had an offence
			let offences = (0..1)
				.map(|i| rc_client::Offence::<AccountId32> {
					offender: AccountId32::from([i as u8; 32]),
					reporters: vec![AccountId32::from([42; 32])],
					slash_fraction: Perbill::from_percent(50),
				})
				.collect::<Vec<_>>();

			// offence + session-index
			let encoded_size = offences.encoded_size() + 42u32.encoded_size();
			assert_eq!(encoded_size, 74);
		}

		// Kusama has the same configurations as of now.
		const POLKADOT_MAX_DOWNWARD_MESSAGE_SIZE: u32 = 51200; // 50 Kib
		const POLKADOT_MAX_UPWARD_MESSAGE_SIZE: u32 = 65531; // 64 Kib

		#[test]
		fn maximum_session_report() {
			let mut num_validator_points = 1;
			loop {
				let session_report = rc_client::SessionReport::<AccountId32> {
					end_index: 0,
					activation_timestamp: Some((0, 0)),
					leftover: false,
					validator_points: (0..num_validator_points)
						.map(|i| (AccountId32::from([i as u8; 32]), 1000))
						.collect(),
				};

				// Note: the real encoded size of the message will be a few bytes more, due to call
				// indices and XCM instructions, but not significant.
				let encoded_size = session_report.encoded_size() as u32;

				if encoded_size > POLKADOT_MAX_DOWNWARD_MESSAGE_SIZE {
					println!(
						"SessionReport: num_validator_points: {}, encoded len: {}, max: {:?}, largest session report: {}",
						num_validator_points, encoded_size, POLKADOT_MAX_DOWNWARD_MESSAGE_SIZE, num_validator_points - 1
					);
					break;
				}
				num_validator_points += 1;
			}

			// We can send up to 1422 32-octet validators + u32 points in a single message. This
			// should inform the configuration `MaximumValidatorsWithPoints`.
			assert_eq!(num_validator_points, 1422);
		}

		#[test]
		fn maximum_validator_set() {
			let mut num_validators = 1;
			loop {
				let validator_set_report = rc_client::ValidatorSetReport::<AccountId32> {
					id: 42,
					leftover: false,
					new_validator_set: (0..num_validators)
						.map(|i| AccountId32::from([i as u8; 32]))
						.collect(),
					prune_up_to: Some(69),
				};

				// Note: the real encoded size of the message will be a few bytes more, due to call
				// indices and XCM instructions, but not significant.
				let encoded_size = validator_set_report.encoded_size() as u32;
				if encoded_size > POLKADOT_MAX_DOWNWARD_MESSAGE_SIZE {
					println!(
						"ValidatorSetReport: num_validators: {}, encoded len: {}, max: {:?}, largest validator set: {}",
						num_validators, encoded_size, POLKADOT_MAX_DOWNWARD_MESSAGE_SIZE, num_validators - 1
					);
					break;
				}
				num_validators += 1;
			}

			// We can send up to 1599 32-octet validator keys (+ other small metadata) in a single
			// validator set report.
			assert_eq!(num_validators, 1600);
		}

		#[test]
		fn maximum_offence_batched() {
			let mut num_offences = 1;
			let session_index: u32 = 42;
			loop {
				let offences = (0..num_offences)
					.map(|i| {
						(
							session_index,
							rc_client::Offence::<AccountId32> {
								offender: AccountId32::from([i as u8; 32]),
								reporters: vec![AccountId32::from([42; 32])],
								slash_fraction: Perbill::from_percent(50),
							},
						)
					})
					.collect::<Vec<_>>();
				let encoded_size = offences.encoded_size();

				if encoded_size as u32 > POLKADOT_MAX_UPWARD_MESSAGE_SIZE {
					println!(
						"Offence (batched): num_offences: {}, encoded len: {}, max: {:?}, largest offence batch: {}",
						num_offences, encoded_size, POLKADOT_MAX_UPWARD_MESSAGE_SIZE, num_offences - 1
					);
					break;
				}

				num_offences += 1;
			}

			// expectedly, this is a bit less than `offence_legacy` since we encode the session
			// index over and over again.
			assert_eq!(num_offences, 898);
		}
	}
}
