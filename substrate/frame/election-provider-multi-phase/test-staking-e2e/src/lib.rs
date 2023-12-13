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

#![cfg(test)]
mod mock;

pub(crate) const LOG_TARGET: &str = "tests::e2e-epm";

use frame_support::{assert_err, assert_noop, assert_ok};
use mock::*;
use sp_core::Get;
use sp_runtime::Perbill;

use crate::mock::RuntimeOrigin;

// syntactic sugar for logging.
#[macro_export]
macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: crate::LOG_TARGET,
			concat!("🛠️  ", $patter)  $(, $values)*
		)
	};
}

fn log_current_time() {
	log!(
		trace,
		"block: {:?}, session: {:?}, era: {:?}, EPM phase: {:?} ts: {:?}",
		System::block_number(),
		Session::current_index(),
		Staking::current_era(),
		ElectionProviderMultiPhase::current_phase(),
		Timestamp::now()
	);
}

#[test]
fn block_progression_works() {
	let (mut ext, pool_state, _) = ExtBuilder::default().build_offchainify();

	ext.execute_with(|| {
		assert_eq!(active_era(), 0);
		assert_eq!(Session::current_index(), 0);
		assert!(ElectionProviderMultiPhase::current_phase().is_off());

		assert!(start_next_active_era(pool_state.clone()).is_ok());
		assert_eq!(active_era(), 1);
		assert_eq!(Session::current_index(), <SessionsPerEra as Get<u32>>::get());

		assert!(ElectionProviderMultiPhase::current_phase().is_off());

		roll_to_epm_signed();
		assert!(ElectionProviderMultiPhase::current_phase().is_signed());
	});

	let (mut ext, pool_state, _) = ExtBuilder::default().build_offchainify();

	ext.execute_with(|| {
		assert_eq!(active_era(), 0);
		assert_eq!(Session::current_index(), 0);
		assert!(ElectionProviderMultiPhase::current_phase().is_off());

		assert!(start_next_active_era_delayed_solution(pool_state).is_ok());
		// if the solution is delayed, EPM will end up in emergency mode..
		assert!(ElectionProviderMultiPhase::current_phase().is_emergency());
		// .. era won't progress..
		assert_eq!(active_era(), 0);
		// .. but session does.
		assert_eq!(Session::current_index(), 2);
	})
}

#[test]
fn offchainify_works() {
	use pallet_election_provider_multi_phase::QueuedSolution;

	let staking_builder = StakingExtBuilder::default();
	let epm_builder = EpmExtBuilder::default();
	let (mut ext, pool_state, _) = ExtBuilder::default()
		.epm(epm_builder)
		.staking(staking_builder)
		.build_offchainify();

	ext.execute_with(|| {
		// test ocw progression and solution queue if submission when unsigned phase submission is
		// not delayed.
		for _ in 0..100 {
			roll_one(pool_state.clone(), false);
			let current_phase = ElectionProviderMultiPhase::current_phase();

			assert!(
				match QueuedSolution::<Runtime>::get() {
					Some(_) => current_phase.is_unsigned(),
					None => !current_phase.is_unsigned(),
				},
				"solution must be queued *only* in unsigned phase"
			);
		}

		// test ocw solution queue if submission in unsigned phase is delayed.
		for _ in 0..100 {
			roll_one(pool_state.clone(), true);
			assert_eq!(
				QueuedSolution::<Runtime>::get(),
				None,
				"solution must never be submitted and stored since it is delayed"
			);
		}
	})
}

#[test]
/// Inspired by the Kusama incident of 8th Dec 2022 and its resolution through the governance
/// fallback.
///
/// Mass slash of validators shoudn't disable more than 1/3 of them (the byzantine threshold). Also
/// no new era should be forced which could lead to EPM entering emergency mode.
fn mass_slash_doesnt_enter_emergency_phase() {
	let epm_builder = EpmExtBuilder::default().disable_emergency_throttling();
	let staking_builder = StakingExtBuilder::default().validator_count(7);
	let (mut ext, _, _) = ExtBuilder::default()
		.epm(epm_builder)
		.staking(staking_builder)
		.build_offchainify();

	ext.execute_with(|| {
		assert_eq!(pallet_staking::ForceEra::<Runtime>::get(), pallet_staking::Forcing::NotForcing);

		// Slash more than 1/3 of the active validators
		slash_half_the_active_set();

		// We are not forcing a new era
		assert_eq!(pallet_staking::ForceEra::<Runtime>::get(), pallet_staking::Forcing::NotForcing);

		// And no more than `1/3` of the validators are disabled
		assert_eq!(
			Session::disabled_validators().len(),
			pallet_staking::UpToByzantineThresholdDisablingStrategy::byzantine_threshold(
				Session::validators().len()
			)
		);
	});
}

#[test]
/// Continuously slash 10% of the active validators per era.
///
/// Since the `OffendingValidatorsThreshold` is only checked per era staking does not force a new
/// era even as the number of active validators is decreasing across eras. When processing a new
/// slash, staking calculates the offending threshold based on the length of the current list of
/// active validators. Thus, slashing a percentage of the current validators that is lower than
/// `OffendingValidatorsThreshold` will never force a new era. However, as the slashes progress, if
/// the subsequent elections do not meet the minimum election untrusted score, the election will
/// fail and enter in emenergency mode.
fn continous_slashes_below_offending_threshold() {
	let staking_builder = StakingExtBuilder::default().validator_count(10);
	let epm_builder = EpmExtBuilder::default().disable_emergency_throttling();

	let (mut ext, pool_state, _) = ExtBuilder::default()
		.epm(epm_builder)
		.staking(staking_builder)
		.build_offchainify();

	ext.execute_with(|| {
		assert_eq!(Session::validators().len(), 10);
		let mut active_validator_set = Session::validators();

		roll_to_epm_signed();

		// set a minimum election score.
		assert!(set_minimum_election_score(500, 1000, 500).is_ok());

		// slash 10% of the active validators and progress era until the minimum trusted score
		// is reached.
		while active_validator_set.len() > 0 {
			let slashed = slash_percentage(Perbill::from_percent(10));
			assert_eq!(slashed.len(), 1);

			// break loop when era does not progress; EPM is in emergency phase as election
			// failed due to election minimum score.
			if start_next_active_era(pool_state.clone()).is_err() {
				assert!(ElectionProviderMultiPhase::current_phase().is_emergency());
				break
			}

			active_validator_set = Session::validators();

			log!(
				trace,
				"slashed 10% of active validators ({:?}). After slash: {:?}",
				slashed,
				active_validator_set
			);
		}
	});
}

#[test]
/// Slashed validator sets intentions in the same era of slashing.
///
/// When validators are slashed, they are chilled and removed from the current `VoterList`. Thus,
/// the slashed validator should not be considered in the next validator set. However, if the
/// slashed validator sets its intention to validate again in the same era when it was slashed and
/// chilled, the validator may not be removed from the active validator set across eras, provided
/// it would selected in the subsequent era if there was no slash. Nominators of the slashed
/// validator will also be slashed and chilled, as expected, but the nomination intentions will
/// remain after the validator re-set the intention to be validating again.
///
/// This behaviour is due to removing implicit chill upon slash
/// <https://github.com/paritytech/substrate/pull/12420>.
///
/// Related to <https://github.com/paritytech/substrate/issues/13714>.
fn set_validation_intention_after_chilled() {
	use frame_election_provider_support::SortedListProvider;
	use pallet_staking::{Event, Nominators};

	let (mut ext, pool_state, _) = ExtBuilder::default()
		.epm(EpmExtBuilder::default())
		.staking(StakingExtBuilder::default())
		.build_offchainify();

	ext.execute_with(|| {
		assert_eq!(active_era(), 0);
		// validator is part of the validator set.
		assert!(Session::validators().contains(&41));
		assert!(<Runtime as pallet_staking::Config>::VoterList::contains(&41));

		// nominate validator 81.
		assert_ok!(Staking::nominate(RuntimeOrigin::signed(21), vec![41]));
		assert_eq!(Nominators::<Runtime>::get(21).unwrap().targets, vec![41]);

		// validator is slashed. it is removed from the `VoterList` through chilling but in the
		// current era, the validator is still part of the active validator set.
		add_slash(&41);
		assert!(Session::validators().contains(&41));
		assert!(!<Runtime as pallet_staking::Config>::VoterList::contains(&41));
		assert_eq!(
			staking_events(),
			[
				Event::Chilled { stash: 41 },
				Event::SlashReported {
					validator: 41,
					slash_era: 0,
					fraction: Perbill::from_percent(10)
				}
			],
		);

		// after the nominator is slashed and chilled, the nominations remain.
		assert_eq!(Nominators::<Runtime>::get(21).unwrap().targets, vec![41]);

		// validator sets intention to stake again in the same era it was chilled.
		assert_ok!(Staking::validate(RuntimeOrigin::signed(41), Default::default()));

		// progress era and check that the slashed validator is still part of the validator
		// set.
		assert!(start_next_active_era(pool_state).is_ok());
		assert_eq!(active_era(), 1);
		assert!(Session::validators().contains(&41));
		assert!(<Runtime as pallet_staking::Config>::VoterList::contains(&41));

		// nominations are still active as before the slash.
		assert_eq!(Nominators::<Runtime>::get(21).unwrap().targets, vec![41]);
	})
}

#[test]
/// Active ledger balance may fall below ED if account chills before unbounding.
///
/// Unbonding call fails if the remaining ledger's stash balance falls below the existential
/// deposit. However, if the stash is chilled before unbonding, the ledger's active balance may
/// be below ED. In that case, only the stash (or root) can kill the ledger entry by calling
/// `withdraw_unbonded` after the bonding period has passed.
///
/// Related to <https://github.com/paritytech/substrate/issues/14246>.
fn ledger_consistency_active_balance_below_ed() {
	use pallet_staking::{Error, Event};

	let (mut ext, pool_state, _) =
		ExtBuilder::default().staking(StakingExtBuilder::default()).build_offchainify();

	ext.execute_with(|| {
		assert_eq!(Staking::ledger(11.into()).unwrap().active, 1000);

		// unbonding total of active stake fails because the active ledger balance would fall
		// below the `MinNominatorBond`.
		assert_noop!(
			Staking::unbond(RuntimeOrigin::signed(11), 1000),
			Error::<Runtime>::InsufficientBond
		);

		// however, chilling works as expected.
		assert_ok!(Staking::chill(RuntimeOrigin::signed(11)));

		// now unbonding the full active balance works, since remainer of the active balance is
		// not enforced to be below `MinNominatorBond` if the stash has been chilled.
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 1000));

		// the active balance of the ledger entry is 0, while total balance is 1000 until
		// `withdraw_unbonded` is called.
		assert_eq!(Staking::ledger(11.into()).unwrap().active, 0);
		assert_eq!(Staking::ledger(11.into()).unwrap().total, 1000);

		// trying to withdraw the unbonded balance won't work yet because not enough bonding
		// eras have passed.
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 0));
		assert_eq!(Staking::ledger(11.into()).unwrap().total, 1000);

		// tries to reap stash after chilling, which fails since the stash total balance is
		// above ED.
		assert_err!(
			Staking::reap_stash(RuntimeOrigin::signed(11), 21, 0),
			Error::<Runtime>::FundedTarget,
		);

		// check the events so far: 1x Chilled and 1x Unbounded
		assert_eq!(
			staking_events(),
			[Event::Chilled { stash: 11 }, Event::Unbonded { stash: 11, amount: 1000 }]
		);

		// after advancing `BondingDuration` eras, the `withdraw_unbonded` will unlock the
		// chunks and the ledger entry will be cleared, since the ledger active balance is 0.
		advance_eras(
			<Runtime as pallet_staking::Config>::BondingDuration::get() as usize,
			pool_state,
		);
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 0));
		assert!(Staking::ledger(11.into()).is_err());
	});
}
