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

use frame_support::storage::with_storage_layer;
use sp_staking::StakingInterface;

use super::*;

#[test]
fn force_unstake_with_slash_works() {
	ExtBuilder::default().build_and_execute(|| {
		// without slash
		let _ = with_storage_layer::<(), _, _>(|| {
			// bond an account, can unstake
			assert_eq!(Staking::bonded(&11), Some(11));
			assert_ok!(<Staking as StakingInterface>::force_unstake(11));
			Err(DispatchError::from("revert"))
		});

		// bond again and add a slash, still can unstake.
		assert_eq!(Staking::bonded(&11), Some(11));
		add_slash(&11);
		assert_ok!(<Staking as StakingInterface>::force_unstake(11));
	});
}

#[test]
fn do_withdraw_unbonded_with_wrong_slash_spans_works_as_expected() {
	ExtBuilder::default().build_and_execute(|| {
		on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(100)]);

		assert_eq!(Staking::bonded(&11), Some(11));

		assert_noop!(
			Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 0),
			Error::<Test>::IncorrectSlashingSpans
		);

		let num_slashing_spans =
			SlashingSpans::<Test>::get(&11).map_or(0, |s| s.iter().count());
		assert_ok!(Staking::withdraw_unbonded(
			RuntimeOrigin::signed(11),
			num_slashing_spans as u32
		));
	});
}

#[test]
fn do_withdraw_unbonded_can_kill_stash_with_existential_deposit_zero() {
	ExtBuilder::default()
		.existential_deposit(0)
		.nominate(false)
		.build_and_execute(|| {
			// Initial state of 11
			assert_eq!(Staking::bonded(&11), Some(11));
			assert_eq!(
				Staking::ledger(11.into()).unwrap(),
				StakingLedgerInspect {
					stash: 11,
					total: 1000,
					active: 1000,
					unlocking: Default::default(),
					legacy_claimed_rewards: bounded_vec![],
				}
			);
			assert_eq!(
				Staking::eras_stakers(active_era(), &11),
				Exposure { total: 1000, own: 1000, others: vec![] }
			);

			// Unbond all of the funds in stash.
			Staking::chill(RuntimeOrigin::signed(11)).unwrap();
			Staking::unbond(RuntimeOrigin::signed(11), 1000).unwrap();
			assert_eq!(
				Staking::ledger(11.into()).unwrap(),
				StakingLedgerInspect {
					stash: 11,
					total: 1000,
					active: 0,
					unlocking: bounded_vec![UnlockChunk { value: 1000, era: 3 }],
					legacy_claimed_rewards: bounded_vec![],
				},
			);

			// trigger future era.
			mock::start_active_era(3);

			// withdraw unbonded
			assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 0));

			// empty stash has been reaped
			assert!(!<Ledger<Test>>::contains_key(&11));
			assert!(!<Bonded<Test>>::contains_key(&11));
			assert!(!<Validators<Test>>::contains_key(&11));
			assert!(!<Payee<Test>>::contains_key(&11));
			// lock is removed.
			assert_eq!(asset::staked::<Test>(&11), 0);
		});
}

#[test]
fn status() {
	ExtBuilder::default().build_and_execute(|| {
		// stash of a validator is identified as a validator
		assert_eq!(Staking::status(&11).unwrap(), StakerStatus::Validator);
		// .. but not the controller.
		assert!(Staking::status(&10).is_err());

		// stash of nominator is identified as a nominator
		assert_eq!(Staking::status(&101).unwrap(), StakerStatus::Nominator(vec![11, 21]));
		// .. but not the controller.
		assert!(Staking::status(&100).is_err());

		// stash of chilled is identified as a chilled
		assert_eq!(Staking::status(&41).unwrap(), StakerStatus::Idle);
		// .. but not the controller.
		assert!(Staking::status(&40).is_err());

		// random other account.
		assert!(Staking::status(&42).is_err());
	})
}
