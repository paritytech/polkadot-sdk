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

use crate::{mock::*, StakeImbalance};

use frame_election_provider_support::{ScoreProvider, SortedListProvider};
use frame_support::assert_ok;
use sp_staking::{OnStakingUpdate, Stake, StakerStatus, StakingInterface};

// keeping tests clean.
type A = AccountId;
type B = Balance;

#[test]
fn setup_works() {
	ExtBuilder::default().build_and_execute(|| {
		assert!(TestNominators::get().is_empty());
		assert_eq!(VoterBagsList::count(), 0);

		assert!(TestValidators::get().is_empty());
		assert_eq!(TargetBagsList::count(), 0);
	});

	ExtBuilder::default().populate_lists().build_and_execute(|| {
		assert!(!TestNominators::get().is_empty());
		assert_eq!(VoterBagsList::count(), 4); // voter list has 2x nominatiors + 2x validators

		assert!(!TestValidators::get().is_empty());
		assert_eq!(TargetBagsList::count(), 2);
	});
}

#[test]
fn update_target_score_works() {
	ExtBuilder::default().populate_lists().build_and_execute(|| {
		assert!(TargetBagsList::contains(&10));
		assert_eq!(TargetBagsList::get_score(&10), Ok(300));

		crate::Pallet::<Test>::update_target_score(&10, StakeImbalance::Negative(100));
		assert_eq!(TargetBagsList::get_score(&10), Ok(200));

		crate::Pallet::<Test>::update_target_score(&10, StakeImbalance::Positive(100));
		assert_eq!(TargetBagsList::get_score(&10), Ok(300));

		let current_score = TargetBagsList::get_score(&10).unwrap();
		crate::Pallet::<Test>::update_target_score(&10, StakeImbalance::Negative(current_score));

		// score dropped to 0, node is removed.
		assert!(!TargetBagsList::contains(&10));
		assert!(TargetBagsList::get_score(&10).is_err());
	})
}

// same as test above but does not panic after defensive so we can test invariants.
#[test]
#[cfg(not(debug_assertions))]
fn update_score_below_zero_defensive_no_panic_works() {
	ExtBuilder::default().populate_lists().build_and_execute(|| {
		assert!(VoterBagsList::contains(&1));
		assert_eq!(VoterBagsList::get_score(&1), Ok(100));
		// updating the score below 0 is unexpected and saturates to 0.
		crate::Pallet::<Test>::update_score::<VoterBagsList>(&1, StakeImbalance::Negative(500));
		assert!(VoterBagsList::contains(&1));
		assert_eq!(VoterBagsList::get_score(&1), Ok(0));

		let n = TestNominators::get();
		assert!(n.get(&1).is_some());
	})
}

#[test]
fn on_stake_update_works() {
	ExtBuilder::default().populate_lists().build_and_execute(|| {
		assert!(VoterBagsList::contains(&1));
		let stake_before = stake_of(1);

		let nominations = <StakingMock as StakingInterface>::nominations(&1).unwrap();
		assert!(nominations.len() == 1);
		let nomination_score_before = TargetBagsList::get_score(&nominations[0]).unwrap();

		// manually change the stake of the voter.
		let new_stake = Stake { total: 10, active: 10 };
		// assert imbalance of the operation is negative.
		assert!(stake_before.unwrap().active > new_stake.active);

		TestNominators::mutate(|n| {
			n.insert(1, (new_stake, nominations.clone()));
		});

		<StakeTracker as OnStakingUpdate<A, B>>::on_stake_update(&1, stake_before, new_stake);

		assert_eq!(VoterBagsList::get_score(&1).unwrap(), new_stake.active);

		// now, the score of the nominated by 1 has `stake_score` less stake than before the
		// nominator's stake was updated.
		let nomination_score_after = TargetBagsList::get_score(&nominations[0]).unwrap();
		assert_eq!(
			nomination_score_after,
			nomination_score_before - (stake_before.unwrap().active - new_stake.active) as u128
		);
	});

	ExtBuilder::default().populate_lists().build_and_execute(|| {
		assert!(TargetBagsList::contains(&10));
		assert!(VoterBagsList::contains(&10));
		let stake_before = stake_of(10);
		let target_score_before = TargetBagsList::get_score(&10).unwrap();

		// validator has no nominations, as expected.
		assert!(<StakingMock as StakingInterface>::nominations(&10).unwrap().is_empty());

		// manually change the self stake.
		let new_stake = Stake { total: 10, active: 10 };
		// assert imbalance of the operation is negative.
		assert!(stake_before.unwrap().active > new_stake.active);
		TestNominators::mutate(|n| {
			n.insert(10, (new_stake, vec![]));
		});

		let stake_imbalance = stake_before.unwrap().active - new_stake.total;

		<StakeTracker as OnStakingUpdate<A, B>>::on_stake_update(&10, stake_before, new_stake);

		assert_eq!(VoterBagsList::get_score(&10).unwrap(), new_stake.active);
		assert_eq!(StakingMock::stake(&10), Ok(new_stake));

		// target bags list was updated as expected (new score is difference between previous and
		// the stake imbalance of previous and the new stake, in order to not touch the nomination's
		// weight in the total target score).
		let target_score_after = TargetBagsList::get_score(&10).unwrap();
		assert_eq!(target_score_after, target_score_before - stake_imbalance as u128);
	})
}

#[test]
fn on_stake_update_lazy_voters_works() {
	ExtBuilder::default()
		.populate_lists()
		.voter_update_mode(crate::VoterUpdateMode::Lazy)
		.build_and_execute(|| {
			assert!(VoterBagsList::contains(&1));
			let stake_before = stake_of(1);

			let nominations = <StakingMock as StakingInterface>::nominations(&1).unwrap();
			assert!(nominations.len() == 1);

			let target_score_before = TargetBagsList::get_score(&10).unwrap();

			// manually change the stake of the voter.
			let new_stake = Stake { total: 10, active: 10 };
			// assert imbalance of the operation is negative.
			assert!(stake_before.unwrap().active > new_stake.active);
			let stake_imbalance = stake_before.unwrap().active - new_stake.total;

			TestNominators::mutate(|n| {
				n.insert(1, (new_stake, nominations.clone()));
			});

			<StakeTracker as OnStakingUpdate<A, B>>::on_stake_update(&1, stake_before, new_stake);

			// score of voter did not update, since the voter list is lazily updated.
			assert_eq!(VoterBagsList::get_score(&1).unwrap(), stake_before.unwrap().active);

			// however, the target's approvals are *always* updated, regardless of the voter's
			// sorting mode.
			let target_score_after = TargetBagsList::get_score(&10).unwrap();
			assert_eq!(target_score_after, target_score_before - stake_imbalance as u128);
		});
}

#[test]
fn on_stake_update_sorting_works() {
	ExtBuilder::default().populate_lists().build_and_execute(|| {
		let initial_sort = TargetBagsList::iter().collect::<Vec<_>>();

		// 10 starts with more score than 11.
		assert_eq!(score_of_target(10), 300);
		assert_eq!(score_of_target(11), 200);
		assert!(score_of_target(10) > score_of_target(11));
		assert_eq!(initial_sort, [10, 11]);

		// add new nominator that add +200 score to 11, which reverts the target list order.
		add_nominator_with_nominations(5, 200, vec![11]);
		assert_eq!(score_of_target(11), 400);
		assert!(score_of_target(10) < score_of_target(11));
		// sorting is now reverted as expected.
		assert_eq!(
			TargetBagsList::iter().collect::<Vec<_>>(),
			initial_sort.iter().rev().cloned().collect::<Vec<_>>()
		);

		// now we remove the staker 5 to get back to the initial state.
		remove_staker(5);
		assert_eq!(score_of_target(10), 300);
		assert_eq!(score_of_target(11), 200);
		assert!(score_of_target(10) > score_of_target(11));
		assert_eq!(TargetBagsList::iter().collect::<Vec<_>>(), initial_sort);

		// double-check, events from target bags list: scores being updated and rebag.
		assert_eq!(
			target_bags_events(),
			[
				pallet_bags_list::Event::Rebagged { who: 11, from: 200, to: 400 },
				pallet_bags_list::Event::ScoreUpdated { who: 11, new_score: 400 },
				pallet_bags_list::Event::Rebagged { who: 11, from: 400, to: 200 },
				pallet_bags_list::Event::ScoreUpdated { who: 11, new_score: 200 },
			],
		);
	});

	ExtBuilder::default().populate_lists().build_and_execute(|| {
		// [(10, 100), (11, 100), (1, 100), (2, 100)]
		let voter_scores_before = voter_scores();
		assert_eq!(voter_scores_before, [(10, 100), (11, 100), (1, 100), (2, 100)]);

		// noop, nothing changes.
		let initial_stake = stake_of(11);
		<StakeTracker as OnStakingUpdate<A, B>>::on_stake_update(
			&11,
			initial_stake,
			initial_stake.unwrap(),
		);
		assert_eq!(voter_scores_before, voter_scores());

		// now let's change the self-vote of 11 and call `on_stake_update` again.
		let nominations = <StakingMock as StakingInterface>::nominations(&11).unwrap();
		let new_stake = Stake { total: 1, active: 1 };
		TestNominators::mutate(|n| {
			n.insert(11, (new_stake, nominations.clone()));
		});

		<StakeTracker as OnStakingUpdate<A, B>>::on_stake_update(&11, initial_stake, new_stake);

		// although the voter score of 11 is 1, the voter list sorting has not been updated
		// automatically.
		assert_eq!(VoterBagsList::get_score(&11), Ok(1));
		// [(10, 100), (11, 1), (1, 100), (2, 100)]
		assert_eq!(
			voter_scores_before.iter().cloned().map(|(v, _)| v).collect::<Vec<_>>(),
			VoterBagsList::iter().collect::<Vec<_>>()
		);

		// double-check, events from voter bags list: scores being updated but no rebag.
		assert_eq!(
			voter_bags_events(),
			[
				pallet_bags_list::Event::ScoreUpdated { who: 11, new_score: 100 },
				pallet_bags_list::Event::ScoreUpdated { who: 11, new_score: 1 }
			],
		);
	});
}

#[test]
#[should_panic = "Defensive failure has been triggered!: NodeNotFound: \"staker should exist in VoterList, as per the contract with staking.\""]
fn on_stake_update_defensive_not_in_list_works() {
	ExtBuilder::default().populate_lists().build_and_execute(|| {
		assert!(VoterBagsList::contains(&1));
		// removes 1 from nominator's list manually, while keeping it as staker.
		assert_ok!(VoterBagsList::on_remove(&1));

		<StakeTracker as OnStakingUpdate<A, B>>::on_stake_update(&1, None, Stake::default());
	})
}

#[test]
#[should_panic = "Defensive failure has been triggered!: \"staker should exist when calling `on_stake_update` and have a valid status\""]
fn on_stake_update_defensive_not_staker_works() {
	ExtBuilder::default().build_and_execute(|| {
		assert!(!VoterBagsList::contains(&1));

		<StakeTracker as OnStakingUpdate<A, B>>::on_stake_update(&1, None, Stake::default());
	})
}

#[test]
fn on_nominator_add_works() {
	ExtBuilder::default().build_and_execute(|| {
		let n = TestNominators::get();
		assert!(!VoterBagsList::contains(&5));
		assert_eq!(n.get(&5), None);

		add_nominator(5, 10);
		assert!(VoterBagsList::contains(&5));
	})
}

#[test]
fn on_validator_add_works() {
	ExtBuilder::default().build_and_execute(|| {
		let n = TestNominators::get();
		let v = TestValidators::get();
		assert!(!VoterBagsList::contains(&5));
		assert!(!TargetBagsList::contains(&5));
		assert!(n.get(&5).is_none() && v.get(&5).is_none());

		// add 5 as staker (target and voter).
		TestNominators::mutate(|n| {
			n.insert(5, Default::default());
		});
		TestValidators::mutate(|n| {
			n.insert(5, Default::default());
		});
	})
}

#[test]
#[should_panic = "Defensive failure has been triggered!: Duplicate: \"the nominator must not exist in the list as per the contract with staking.\""]
fn on_nominator_add_already_exists_defensive_works() {
	ExtBuilder::default().populate_lists().build_and_execute(|| {
		assert!(VoterBagsList::contains(&1));
		assert_eq!(VoterBagsList::count(), 4);
		assert_eq!(<VoterBagsList as ScoreProvider<A>>::score(&1), 100);

		let voter_list_before = VoterBagsList::iter().collect::<Vec<_>>();
		let target_list_before = TargetBagsList::iter().collect::<Vec<_>>();

		// noop.
		let nominations = <StakingMock as StakingInterface>::nominations(&1).unwrap();
		<StakeTracker as OnStakingUpdate<A, B>>::on_nominator_add(&1, nominations);
		assert!(VoterBagsList::contains(&1));
		assert_eq!(VoterBagsList::count(), 4);
		assert_eq!(<VoterBagsList as ScoreProvider<A>>::score(&1), 100);

		assert_eq!(VoterBagsList::iter().collect::<Vec<_>>(), voter_list_before);
		assert_eq!(TargetBagsList::iter().collect::<Vec<_>>(), target_list_before);
	});
}

#[test]
#[should_panic]
fn on_validator_add_already_exists_works() {
	ExtBuilder::default().populate_lists().build_and_execute(|| {
		assert!(TargetBagsList::contains(&10));
		assert_eq!(TargetBagsList::count(), 2);
		assert_eq!(<TargetBagsList as ScoreProvider<A>>::score(&10), 300);

		let voter_list_before = VoterBagsList::iter().collect::<Vec<_>>();
		let target_list_before = TargetBagsList::iter().collect::<Vec<_>>();

		// noop
		<StakeTracker as OnStakingUpdate<A, B>>::on_validator_add(
			&10,
			Some(Stake { total: 300, active: 300 }),
		);
		assert!(TargetBagsList::contains(&10));
		assert_eq!(TargetBagsList::count(), 2);
		assert_eq!(<TargetBagsList as ScoreProvider<A>>::score(&10), 300);

		assert_eq!(VoterBagsList::iter().collect::<Vec<_>>(), voter_list_before);
		assert_eq!(TargetBagsList::iter().collect::<Vec<_>>(), target_list_before);
	});
}

#[test]
fn on_nominator_remove_works() {
	ExtBuilder::default().populate_lists().build_and_execute(|| {
		assert!(VoterBagsList::contains(&1));
		let nominator_score = VoterBagsList::get_score(&1).unwrap();

		let nominations = <StakingMock as StakingInterface>::nominations(&1).unwrap();
		assert!(nominations.len() == 1);
		let nomination_score_before = TargetBagsList::get_score(&nominations[0]).unwrap();

		<StakeTracker as OnStakingUpdate<A, B>>::on_nominator_remove(&1, nominations.clone());

		// the nominator was removed from the voter list.
		assert!(!VoterBagsList::contains(&1));

		// now, the score of the nominated by 1 has less `nominator_score` stake than before the
		// nominator was removed.
		let nomination_score_after = TargetBagsList::get_score(&nominations[0]).unwrap();
		assert!(nomination_score_after == nomination_score_before - nominator_score as u128);
	})
}

#[test]
#[should_panic = "Defensive failure has been triggered!: NodeNotFound: \"the nominator must exist in the list as per the contract with staking.\""]
fn on_nominator_remove_defensive_works() {
	ExtBuilder::default().populate_lists().build_and_execute(|| {
		assert!(VoterBagsList::contains(&1));

		// remove 1 from the voter list to check if the defensive is triggered in the next call,
		// while maintaining it as a staker so it does not early exist at the staking mock
		// implementation.
		assert_ok!(VoterBagsList::on_remove(&1));

		<StakeTracker as OnStakingUpdate<A, B>>::on_nominator_remove(&1, vec![]);
	})
}

#[test]
#[should_panic = "Defensive failure has been triggered!: \"on_validator_remove called on a non-existing target.\""]
fn on_validator_remove_defensive_works() {
	ExtBuilder::default().build_and_execute(|| {
		assert!(!TargetBagsList::contains(&1));
		<StakeTracker as OnStakingUpdate<A, B>>::on_validator_remove(&1);
	})
}

mod staking_integration {

	use super::*;

	#[test]
	fn staking_interface_works() {
		ExtBuilder::default().build_and_execute(|| {
			assert_eq!(TestNominators::get().len(), 0);
			assert_eq!(TestValidators::get().len(), 0);

			add_nominator(1, 100);
			let n = TestNominators::get();
			assert_eq!(n.get(&1).unwrap().0, Stake { active: 100u64, total: 100u64 });
			assert_eq!(StakingMock::status(&1), Ok(StakerStatus::Nominator(vec![])));

			add_validator(2, 200);
			let v = TestValidators::get();
			assert_eq!(v.get(&2).copied().unwrap(), Stake { active: 200u64, total: 200u64 });
			assert_eq!(StakingMock::status(&2), Ok(StakerStatus::Validator));

			chill_staker(1);
			assert_eq!(StakingMock::status(&1), Ok(StakerStatus::Idle));
			// a chilled nominator is removed from the voter list right away.
			assert!(!VoterBagsList::contains(&1));

			remove_staker(1);
			assert!(StakingMock::status(&1).is_err());
			assert!(!VoterBagsList::contains(&1));

			chill_staker(2);
			assert_eq!(StakingMock::status(&2), Ok(StakerStatus::Idle));
			// a chilled validator is dropped from the target list if its score is 0.
			assert!(!TargetBagsList::contains(&2));
			assert!(!VoterBagsList::contains(&2));
		})
	}

	#[test]
	fn on_add_stakers_works() {
		ExtBuilder::default().build_and_execute(|| {
			add_nominator(1, 100);
			assert_eq!(TargetBagsList::count(), 0);
			assert_eq!(VoterBagsList::count(), 1);
			assert_eq!(VoterBagsList::get_score(&1).unwrap(), 100);

			add_validator(10, 200);
			assert_eq!(VoterBagsList::count(), 2); // 1x nominator + 1x validator
			assert_eq!(TargetBagsList::count(), 1);
			assert_eq!(TargetBagsList::get_score(&10).unwrap(), 200);
		})
	}

	#[test]
	fn on_update_stake_works() {
		ExtBuilder::default().build_and_execute(|| {
			add_nominator(1, 100);
			assert_eq!(VoterBagsList::get_score(&1).unwrap(), 100);
			update_stake(1, 200, stake_of(1));
			assert_eq!(VoterBagsList::get_score(&1).unwrap(), 200);

			add_validator(10, 100);
			assert_eq!(TargetBagsList::get_score(&10).unwrap(), 100);
			update_stake(10, 200, stake_of(10));
			assert_eq!(TargetBagsList::get_score(&10).unwrap(), 200);
		})
	}

	#[test]
	fn on_remove_stakers_works() {
		ExtBuilder::default().build_and_execute(|| {
			add_nominator(1, 100);
			assert!(VoterBagsList::contains(&1));

			remove_staker(1);
			assert!(!VoterBagsList::contains(&1));

			add_validator(10, 100);
			assert!(TargetBagsList::contains(&10));
			remove_staker(10);
			assert!(!TargetBagsList::contains(&10));
		})
	}

	#[test]
	fn on_remove_stakers_with_nominations_works() {
		ExtBuilder::default().populate_lists().build_and_execute(|| {
			assert_eq!(target_scores(), vec![(10, 300), (11, 200)]);

			assert!(VoterBagsList::contains(&1));
			assert_eq!(VoterBagsList::get_score(&1), Ok(100));
			assert_eq!(TargetBagsList::get_score(&10), Ok(300));

			// remove nominator deletes node from voter list and updates the stake of its
			// nominations.
			remove_staker(1);
			assert!(!VoterBagsList::contains(&1));
			assert_eq!(TargetBagsList::get_score(&10), Ok(200));
		})
	}

	#[test]
	fn on_nominator_update_works() {
		ExtBuilder::default().populate_lists().build_and_execute(|| {
			assert_eq!(voter_scores(), vec![(10, 100), (11, 100), (1, 100), (2, 100)]);
			assert_eq!(target_scores(), vec![(10, 300), (11, 200)]);

			add_validator(20, 500);
			// removes nomination from 10 and adds nomination to new validator, 20.
			update_nominations_of(2, vec![11, 20]);

			assert_eq!(voter_scores(), [(20, 500), (10, 100), (11, 100), (1, 100), (2, 100)]);

			// target list has been updated:
			assert_eq!(target_scores(), vec![(20, 600), (11, 200), (10, 200)]);
		})
	}

	#[test]
	fn on_nominator_update_lazy_voter_works() {
		ExtBuilder::default()
			.populate_lists()
			.voter_update_mode(crate::VoterUpdateMode::Lazy)
			.build_and_execute(|| {
				assert_eq!(voter_scores(), vec![(10, 100), (11, 100), (1, 100), (2, 100)]);
				assert_eq!(target_scores(), vec![(10, 300), (11, 200)]);

				add_validator(20, 500);
				// removes nomination from 10 and adds nomination to new validator, 20.
				update_nominations_of(2, vec![11, 20]);

				// even in lazy mode, the new voter node is inserted.
				assert_eq!(voter_scores(), [(20, 500), (10, 100), (11, 100), (1, 100), (2, 100)]);

				// target list has been updated:
				assert_eq!(target_scores(), vec![(20, 600), (11, 200), (10, 200)]);
			})
	}

	#[test]
	fn target_chill_remove_lifecycle_works() {
		ExtBuilder::default().populate_lists().build_and_execute(|| {
			assert!(TargetBagsList::contains(&10));
			// 10 has 2 nominations from 1 and 2. Each with 100 approvals, so 300 in total (2x
			// nominated
			// + 1x self-stake).
			assert_eq!(<TargetBagsList as ScoreProvider<A>>::score(&10), 300);

			// chill validator 10.
			chill_staker(10);
			assert_eq!(StakingMock::status(&10), Ok(StakerStatus::Idle));

			// chilling removed the self stake (100) from score, but the nominations approvals
			// remain.
			assert!(TargetBagsList::contains(&10));
			assert_eq!(<TargetBagsList as ScoreProvider<A>>::score(&10), 200);

			// even if the validator is removed, the target node remains in the list since approvals
			// score != 0.
			remove_staker(10);
			assert!(TargetBagsList::contains(&10));
			assert_eq!(<TargetBagsList as ScoreProvider<A>>::score(&10), 200);
			assert!(StakingMock::status(&10).is_err());

			// 1 stops nominating 10.
			update_nominations_of(1, vec![]);
			assert!(TargetBagsList::contains(&10));
			assert_eq!(<TargetBagsList as ScoreProvider<A>>::score(&10), 100);

			// 2 stops nominating 10 and its approavals dropped to 0, thus the target node has been
			// removed.
			update_nominations_of(2, vec![]);
			assert!(!TargetBagsList::contains(&10));
			assert_eq!(<TargetBagsList as ScoreProvider<A>>::score(&10), 0);
		})
	}

	#[test]
	fn target_remove_lifecycle_works() {
		ExtBuilder::default().populate_lists().build_and_execute(|| {
			assert!(TargetBagsList::contains(&10));
			// 10 has 2 nominations from 1 and 2. Each with 100 approvals, so 300 in total (2x
			// nominated
			// + 1x self-stake).
			assert_eq!(<TargetBagsList as ScoreProvider<A>>::score(&10), 300);

			// remove validator 10.
			remove_staker(10);

			// but the target list keeps track of the remaining approvals of 10, without the self
			// stake.
			assert!(TargetBagsList::contains(&10));
			assert_eq!(<TargetBagsList as ScoreProvider<A>>::score(&10), 200);

			// 1 stops nominating 10.
			update_nominations_of(1, vec![]);
			assert!(TargetBagsList::contains(&10));
			assert_eq!(<TargetBagsList as ScoreProvider<A>>::score(&10), 100);

			// 2 stops nominating 10. Since approvals of 10 drop to 0, the target list node is
			// removed.
			update_nominations_of(2, vec![]);
			assert!(!TargetBagsList::contains(&10));
			assert_eq!(<TargetBagsList as ScoreProvider<A>>::score(&10), 0);
		})
	}
}
