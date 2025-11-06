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

use frame_support::{assert_noop, assert_ok, assert_storage_noop, traits::IntegrityTest};

use super::*;
use frame_election_provider_support::{SortedListProvider, VoteWeight};
use list::Bag;
use mock::{test_utils::*, *};
use substrate_test_utils::assert_eq_uvec;

#[docify::export]
#[test]
fn examples_work() {
	ExtBuilder::default()
		.skip_genesis_ids()
		// initially set the score of 11 for 22 to push it next to 12
		.add_ids(vec![(25, 25), (21, 21), (12, 12), (22, 11), (5, 5), (7, 7), (3, 3)])
		.build_and_execute(|| {
			// initial bags
			assert_eq!(
				List::<Runtime>::get_bags(),
				vec![
					// bag 0 -> 10
					(10, vec![5, 7, 3]),
					// bag 10 -> 20
					(20, vec![12, 22]),
					// bag 20 -> 30
					(30, vec![25, 21])
				]
			);

			// set score of 22 to 22
			StakingMock::set_score_of(&22, 22);

			// now we rebag 22 to the first bag
			assert_ok!(BagsList::rebag(RuntimeOrigin::signed(42), 22));

			assert_eq!(
				List::<Runtime>::get_bags(),
				vec![
					// bag 0 -> 10
					(10, vec![5, 7, 3]),
					// bag 10 -> 20
					(20, vec![12]),
					// bag 20 -> 30
					(30, vec![25, 21, 22])
				]
			);

			// now we put 7 at the front of bag 0
			assert_ok!(BagsList::put_in_front_of(RuntimeOrigin::signed(7), 5));

			assert_eq!(
				List::<Runtime>::get_bags(),
				vec![
					// bag 0 -> 10
					(10, vec![7, 5, 3]),
					// bag 10 -> 20
					(20, vec![12]),
					// bag 20 -> 30
					(30, vec![25, 21, 22])
				]
			);
		})
}

mod pallet {
	use super::*;

	#[test]
	fn rebag_works() {
		ExtBuilder::default().add_ids(vec![(42, 20)]).build_and_execute(|| {
			// given
			assert_eq!(
				List::<Runtime>::get_bags(),
				vec![(10, vec![1]), (20, vec![42]), (1_000, vec![2, 3, 4])]
			);

			// when increasing score to the level of non-existent bag
			assert_eq!(List::<Runtime>::get_score(&42).unwrap(), 20);
			StakingMock::set_score_of(&42, 2_000);
			assert_ok!(BagsList::rebag(RuntimeOrigin::signed(0), 42));
			assert_eq!(List::<Runtime>::get_score(&42).unwrap(), 2_000);

			// then a new bag is created and the id moves into it
			assert_eq!(
				List::<Runtime>::get_bags(),
				vec![(10, vec![1]), (1_000, vec![2, 3, 4]), (2_000, vec![42])]
			);

			// when decreasing score within the range of the current bag
			StakingMock::set_score_of(&42, 1_001);
			assert_ok!(BagsList::rebag(RuntimeOrigin::signed(0), 42));

			// then the id does not move
			assert_eq!(
				List::<Runtime>::get_bags(),
				vec![(10, vec![1]), (1_000, vec![2, 3, 4]), (2_000, vec![42])]
			);
			// but the score is updated
			assert_eq!(List::<Runtime>::get_score(&42).unwrap(), 1_001);

			// when reducing score to the level of a non-existent bag
			StakingMock::set_score_of(&42, 30);
			assert_ok!(BagsList::rebag(RuntimeOrigin::signed(0), 42));

			// then a new bag is created and the id moves into it
			assert_eq!(
				List::<Runtime>::get_bags(),
				vec![(10, vec![1]), (30, vec![42]), (1_000, vec![2, 3, 4])]
			);
			assert_eq!(List::<Runtime>::get_score(&42).unwrap(), 30);

			// when increasing score to the level of a pre-existing bag
			StakingMock::set_score_of(&42, 500);
			assert_ok!(BagsList::rebag(RuntimeOrigin::signed(0), 42));

			// then the id moves into that bag
			assert_eq!(
				List::<Runtime>::get_bags(),
				vec![(10, vec![1]), (1_000, vec![2, 3, 4, 42])]
			);
			assert_eq!(List::<Runtime>::get_score(&42).unwrap(), 500);
		});
	}

	#[test]
	fn rebag_when_missing() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4])]);

			// when
			NEXT_VOTE_WEIGHT_MAP.with(|m| m.borrow_mut().remove(&3));

			// then
			assert_ok!(BagsList::rebag(RuntimeOrigin::signed(0), 3));

			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 4])]);
		});
	}

	#[test]
	fn rebag_when_added() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4])]);

			// when 5 is added, but somehow it is not present in the bags list.
			NEXT_VOTE_WEIGHT_MAP.with(|m| m.borrow_mut().insert(5, 10));

			// then
			assert_ok!(BagsList::rebag(RuntimeOrigin::signed(0), 5));

			// 5 is added
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1, 5]), (1_000, vec![2, 3, 4])]);
		});
	}

	// Rebagging the tail of a bag results in the old bag having a new tail and an overall correct
	// state.
	#[test]
	fn rebag_tail_works() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4])]);

			// when
			StakingMock::set_score_of(&4, 10);
			assert_ok!(BagsList::rebag(RuntimeOrigin::signed(0), 4));

			// then
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1, 4]), (1_000, vec![2, 3])]);
			assert_eq!(Bag::<Runtime>::get(1_000).unwrap(), Bag::new(Some(2), Some(3), 1_000));

			// when
			StakingMock::set_score_of(&3, 10);
			assert_ok!(BagsList::rebag(RuntimeOrigin::signed(0), 3));

			// then
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1, 4, 3]), (1_000, vec![2])]);

			assert_eq!(Bag::<Runtime>::get(10).unwrap(), Bag::new(Some(1), Some(3), 10));
			assert_eq!(Bag::<Runtime>::get(1_000).unwrap(), Bag::new(Some(2), Some(2), 1_000));
			assert_eq!(get_list_as_ids(), vec![2u64, 1, 4, 3]);

			// when
			StakingMock::set_score_of(&2, 10);
			assert_ok!(BagsList::rebag(RuntimeOrigin::signed(0), 2));

			// then
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1, 4, 3, 2])]);
			assert_eq!(Bag::<Runtime>::get(1_000), None);
		});
	}

	// Rebagging the head of a bag results in the old bag having a new head and an overall correct
	// state.
	#[test]
	fn rebag_head_works() {
		ExtBuilder::default().build_and_execute(|| {
			// when
			StakingMock::set_score_of(&2, 10);
			assert_ok!(BagsList::rebag(RuntimeOrigin::signed(0), 2));

			// then
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1, 2]), (1_000, vec![3, 4])]);
			assert_eq!(Bag::<Runtime>::get(1_000).unwrap(), Bag::new(Some(3), Some(4), 1_000));

			// when
			StakingMock::set_score_of(&3, 10);
			assert_ok!(BagsList::rebag(RuntimeOrigin::signed(0), 3));

			// then
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1, 2, 3]), (1_000, vec![4])]);
			assert_eq!(Bag::<Runtime>::get(1_000).unwrap(), Bag::new(Some(4), Some(4), 1_000));

			// when
			StakingMock::set_score_of(&4, 10);
			assert_ok!(BagsList::rebag(RuntimeOrigin::signed(0), 4));

			// then
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1, 2, 3, 4])]);
			assert_eq!(Bag::<Runtime>::get(1_000), None);
		});
	}

	#[test]
	fn wrong_rebag_errs() {
		ExtBuilder::default().build_and_execute(|| {
			let node_3 = list::Node::<Runtime>::get(&3).unwrap();

			NEXT_VOTE_WEIGHT_MAP.with(|m| m.borrow_mut().insert(500, 500));
			// when account 3 is _not_ misplaced with score 500
			assert!(!node_3.is_misplaced(500));

			// then calling rebag on account 3 with score 500 is a noop
			assert_storage_noop!(assert_eq!(BagsList::rebag(RuntimeOrigin::signed(0), 3), Ok(())));

			// when account 42 is not in the list
			assert!(!BagsList::contains(&42));
			// then rebag-ing account 42 is an error
			assert_storage_noop!(assert!(matches!(
				BagsList::rebag(RuntimeOrigin::signed(0), 42),
				Err(_)
			)));
		});
	}

	#[test]
	#[should_panic = "thresholds must strictly increase, and have no duplicates"]
	fn duplicate_in_bags_threshold_panics() {
		const DUPE_THRESH: &[VoteWeight; 4] = &[10, 20, 30, 30];
		BagThresholds::set(DUPE_THRESH);
		BagsList::integrity_test();
	}

	#[test]
	#[should_panic = "thresholds must strictly increase, and have no duplicates"]
	fn decreasing_in_bags_threshold_panics() {
		const DECREASING_THRESH: &[VoteWeight; 4] = &[10, 30, 20, 40];
		BagThresholds::set(DECREASING_THRESH);
		BagsList::integrity_test();
	}

	#[test]
	fn empty_threshold_works() {
		BagThresholds::set(Default::default()); // which is the same as passing `()` to `Get<_>`.
		ExtBuilder::default().build_and_execute(|| {
			// everyone in the same bag.
			assert_eq!(List::<Runtime>::get_bags(), vec![(VoteWeight::MAX, vec![1, 2, 3, 4])]);

			// any insertion goes there as well.
			assert_ok!(List::<Runtime>::insert(5, 999));
			assert_ok!(List::<Runtime>::insert(6, 0));
			assert_eq!(
				List::<Runtime>::get_bags(),
				vec![(VoteWeight::MAX, vec![1, 2, 3, 4, 5, 6])]
			);

			// any rebag is noop.
			assert_storage_noop!(assert_eq!(BagsList::rebag(RuntimeOrigin::signed(0), 1), Ok(())));
		})
	}

	#[test]
	fn put_in_front_of_other_can_be_permissionless() {
		ExtBuilder::default()
			.skip_genesis_ids()
			.add_ids(vec![(10, 15), (11, 16), (12, 19)])
			.build_and_execute(|| {
				// given
				assert_eq!(List::<Runtime>::get_bags(), vec![(20, vec![10, 11, 12])]);
				// 11 now has more weight than 10 and can be moved before it.
				StakingMock::set_score_of(&11u64, 17);

				// when
				assert_ok!(BagsList::put_in_front_of_other(RuntimeOrigin::signed(42), 11u64, 10));

				// then
				assert_eq!(List::<Runtime>::get_bags(), vec![(20, vec![11, 10, 12])]);
			});
	}

	#[test]
	fn put_in_front_of_two_node_bag_heavier_is_tail() {
		ExtBuilder::default()
			.skip_genesis_ids()
			.add_ids(vec![(10, 15), (11, 16)])
			.build_and_execute(|| {
				// given
				assert_eq!(List::<Runtime>::get_bags(), vec![(20, vec![10, 11])]);

				// when
				assert_ok!(BagsList::put_in_front_of(RuntimeOrigin::signed(11), 10));

				// then
				assert_eq!(List::<Runtime>::get_bags(), vec![(20, vec![11, 10])]);
			});
	}

	#[test]
	fn put_in_front_of_two_node_bag_heavier_is_head() {
		ExtBuilder::default()
			.skip_genesis_ids()
			.add_ids(vec![(11, 16), (10, 15)])
			.build_and_execute(|| {
				// given
				assert_eq!(List::<Runtime>::get_bags(), vec![(20, vec![11, 10])]);

				// when
				assert_ok!(BagsList::put_in_front_of(RuntimeOrigin::signed(11), 10));

				// then
				assert_eq!(List::<Runtime>::get_bags(), vec![(20, vec![11, 10])]);
			});
	}

	#[test]
	fn put_in_front_of_non_terminal_nodes_heavier_behind() {
		ExtBuilder::default().add_ids(vec![(5, 1_000)]).build_and_execute(|| {
			// given
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4, 5])]);

			StakingMock::set_score_of(&3, 999);

			// when
			assert_ok!(BagsList::put_in_front_of(RuntimeOrigin::signed(4), 3));

			// then
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 4, 3, 5])]);
		});
	}

	#[test]
	fn put_in_front_of_non_terminal_nodes_heavier_in_front() {
		ExtBuilder::default()
			.add_ids(vec![(5, 1_000), (6, 1_000)])
			.build_and_execute(|| {
				// given
				assert_eq!(
					List::<Runtime>::get_bags(),
					vec![(10, vec![1]), (1_000, vec![2, 3, 4, 5, 6])]
				);

				StakingMock::set_score_of(&5, 999);

				// when
				assert_ok!(BagsList::put_in_front_of(RuntimeOrigin::signed(3), 5));

				// then
				assert_eq!(
					List::<Runtime>::get_bags(),
					vec![(10, vec![1]), (1_000, vec![2, 4, 3, 5, 6])]
				);
			});
	}

	#[test]
	fn put_in_front_of_lighter_is_head_heavier_is_non_terminal() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4])]);

			StakingMock::set_score_of(&2, 999);

			// when
			assert_ok!(BagsList::put_in_front_of(RuntimeOrigin::signed(3), 2));

			// then
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![3, 2, 4])]);
		});
	}

	#[test]
	fn put_in_front_of_heavier_is_tail_lighter_is_non_terminal() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4])]);

			StakingMock::set_score_of(&3, 999);

			// when
			assert_ok!(BagsList::put_in_front_of(RuntimeOrigin::signed(4), 3));

			// then
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 4, 3])]);
		});
	}

	#[test]
	fn put_in_front_of_heavier_is_tail_lighter_is_head() {
		ExtBuilder::default().add_ids(vec![(5, 1_000)]).build_and_execute(|| {
			// given
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4, 5])]);

			StakingMock::set_score_of(&2, 999);

			// when
			assert_ok!(BagsList::put_in_front_of(RuntimeOrigin::signed(5), 2));

			// then
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![5, 2, 3, 4])]);
		});
	}

	#[test]
	fn put_in_front_of_heavier_is_head_lighter_is_not_terminal() {
		ExtBuilder::default().add_ids(vec![(5, 1_000)]).build_and_execute(|| {
			// given
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4, 5])]);

			StakingMock::set_score_of(&4, 999);

			// when
			BagsList::put_in_front_of(RuntimeOrigin::signed(2), 4).unwrap();

			// then
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![3, 2, 4, 5])]);
		});
	}

	#[test]
	fn put_in_front_of_lighter_is_tail_heavier_is_not_terminal() {
		ExtBuilder::default().add_ids(vec![(5, 900)]).build_and_execute(|| {
			// given
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4, 5])]);

			// when
			BagsList::put_in_front_of(RuntimeOrigin::signed(3), 5).unwrap();

			// then
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 4, 3, 5])]);
		});
	}

	#[test]
	fn put_in_front_of_lighter_is_tail_heavier_is_head() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4])]);

			StakingMock::set_score_of(&4, 999);

			// when
			assert_ok!(BagsList::put_in_front_of(RuntimeOrigin::signed(2), 4));

			// then
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![3, 2, 4])]);
		});
	}

	#[test]
	fn put_in_front_of_errors_if_heavier_is_less_than_lighter() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4])]);

			StakingMock::set_score_of(&3, 999);

			// then
			assert_noop!(
				BagsList::put_in_front_of(RuntimeOrigin::signed(3), 2),
				crate::pallet::Error::<Runtime>::List(ListError::NotHeavier)
			);
		});
	}

	#[test]
	fn put_in_front_of_errors_if_heavier_is_equal_weight_to_lighter() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4])]);

			// then
			assert_noop!(
				BagsList::put_in_front_of(RuntimeOrigin::signed(3), 4),
				crate::pallet::Error::<Runtime>::List(ListError::NotHeavier)
			);
		});
	}

	#[test]
	fn put_in_front_of_errors_if_nodes_not_found() {
		// `heavier` not found
		ExtBuilder::default().build_and_execute(|| {
			// given
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4])]);

			assert!(!ListNodes::<Runtime>::contains_key(5));

			// then
			assert_noop!(
				BagsList::put_in_front_of(RuntimeOrigin::signed(5), 4),
				crate::pallet::Error::<Runtime>::List(ListError::NodeNotFound)
			);
		});

		// `lighter` not found
		ExtBuilder::default().build_and_execute(|| {
			// given
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4])]);

			assert!(!ListNodes::<Runtime>::contains_key(5));

			// then
			assert_noop!(
				BagsList::put_in_front_of(RuntimeOrigin::signed(4), 5),
				crate::pallet::Error::<Runtime>::List(ListError::NodeNotFound)
			);
		});
	}

	#[test]
	fn put_in_front_of_errors_if_nodes_not_in_same_bag() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4])]);

			// then
			assert_noop!(
				BagsList::put_in_front_of(RuntimeOrigin::signed(4), 1),
				crate::pallet::Error::<Runtime>::List(ListError::NotInSameBag)
			);
		});
	}
}

mod sorted_list_provider {
	use super::*;

	#[test]
	fn iter_works() {
		ExtBuilder::default().build_and_execute(|| {
			let expected = vec![2, 3, 4, 1];
			for (i, id) in BagsList::iter().enumerate() {
				assert_eq!(id, expected[i])
			}
		});
	}

	#[test]
	fn iter_from_works() {
		ExtBuilder::default().add_ids(vec![(5, 5), (6, 15)]).build_and_execute(|| {
			// given
			assert_eq!(
				List::<Runtime>::get_bags(),
				vec![(10, vec![1, 5]), (20, vec![6]), (1000, vec![2, 3, 4])]
			);

			assert_eq!(BagsList::iter_from(&2).unwrap().collect::<Vec<_>>(), vec![3, 4, 6, 1, 5]);
			assert_eq!(BagsList::iter_from(&3).unwrap().collect::<Vec<_>>(), vec![4, 6, 1, 5]);
			assert_eq!(BagsList::iter_from(&4).unwrap().collect::<Vec<_>>(), vec![6, 1, 5]);
			assert_eq!(BagsList::iter_from(&6).unwrap().collect::<Vec<_>>(), vec![1, 5]);
			assert_eq!(BagsList::iter_from(&1).unwrap().collect::<Vec<_>>(), vec![5]);
			assert!(BagsList::iter_from(&5).unwrap().collect::<Vec<_>>().is_empty());
			assert!(BagsList::iter_from(&7).is_err());

			assert_storage_noop!(assert!(BagsList::iter_from(&8).is_err()));
		});
	}

	#[test]
	fn count_works() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			assert_eq!(BagsList::count(), 4);

			// when inserting
			assert_ok!(BagsList::on_insert(201, 0));
			// then the count goes up
			assert_eq!(BagsList::count(), 5);

			// when removing
			BagsList::on_remove(&201).unwrap();
			// then the count goes down
			assert_eq!(BagsList::count(), 4);

			// when updating
			assert_noop!(BagsList::on_update(&201, VoteWeight::MAX), ListError::NodeNotFound);
			// then the count stays the same
			assert_eq!(BagsList::count(), 4);
		});
	}

	#[test]
	fn on_insert_works() {
		ExtBuilder::default().build_and_execute(|| {
			// when
			assert_ok!(BagsList::on_insert(6, 1_000));

			// then the bags
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4, 6])]);
			// and list correctly include the new id,
			assert_eq!(BagsList::iter().collect::<Vec<_>>(), vec![2, 3, 4, 6, 1]);
			// and the count is incremented.
			assert_eq!(BagsList::count(), 5);

			// when
			assert_ok!(BagsList::on_insert(7, 1_001));

			// then the bags
			assert_eq!(
				List::<Runtime>::get_bags(),
				vec![(10, vec![1]), (1_000, vec![2, 3, 4, 6]), (2_000, vec![7])]
			);
			// and list correctly include the new id,
			assert_eq!(BagsList::iter().collect::<Vec<_>>(), vec![7, 2, 3, 4, 6, 1]);
			// and the count is incremented.
			assert_eq!(BagsList::count(), 6);
		})
	}

	#[test]
	fn on_insert_errors_with_duplicate_id() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			assert!(get_list_as_ids().contains(&3));

			// then
			assert_storage_noop!(assert_eq!(
				BagsList::on_insert(3, 20).unwrap_err(),
				ListError::Duplicate
			));
		});
	}

	#[test]
	fn on_update_works() {
		ExtBuilder::default().add_ids(vec![(42, 20)]).build_and_execute(|| {
			// given
			assert_eq!(
				List::<Runtime>::get_bags(),
				vec![(10, vec![1]), (20, vec![42]), (1_000, vec![2, 3, 4])]
			);
			assert_eq!(BagsList::count(), 5);

			// when increasing score to the level of non-existent bag
			BagsList::on_update(&42, 2_000).unwrap();

			// then the bag is created with the id in it,
			assert_eq!(
				List::<Runtime>::get_bags(),
				vec![(10, vec![1]), (1_000, vec![2, 3, 4]), (2000, vec![42])]
			);
			// and the id position is updated in the list.
			assert_eq!(BagsList::iter().collect::<Vec<_>>(), vec![42, 2, 3, 4, 1]);

			// when decreasing score within the range of the current bag
			BagsList::on_update(&42, 1_001).unwrap();

			// then the id does not change bags,
			assert_eq!(
				List::<Runtime>::get_bags(),
				vec![(10, vec![1]), (1_000, vec![2, 3, 4]), (2000, vec![42])]
			);
			// or change position in the list.
			assert_eq!(BagsList::iter().collect::<Vec<_>>(), vec![42, 2, 3, 4, 1]);

			// when increasing score to the level of a non-existent bag with the max threshold
			BagsList::on_update(&42, VoteWeight::MAX).unwrap();

			// the the new bag is created with the id in it,
			assert_eq!(
				List::<Runtime>::get_bags(),
				vec![(10, vec![1]), (1_000, vec![2, 3, 4]), (VoteWeight::MAX, vec![42])]
			);
			// and the id position is updated in the list.
			assert_eq!(BagsList::iter().collect::<Vec<_>>(), vec![42, 2, 3, 4, 1]);

			// when decreasing the score to a pre-existing bag
			BagsList::on_update(&42, 1_000).unwrap();

			// then id is moved to the correct bag (as the last member),
			assert_eq!(
				List::<Runtime>::get_bags(),
				vec![(10, vec![1]), (1_000, vec![2, 3, 4, 42])]
			);
			// and the id position is updated in the list.
			assert_eq!(BagsList::iter().collect::<Vec<_>>(), vec![2, 3, 4, 42, 1]);

			// since we have only called on_update, the `count` has not changed.
			assert_eq!(BagsList::count(), 5);
		});
	}

	#[test]
	fn on_remove_works() {
		let ensure_left = |id, counter| {
			assert!(!ListNodes::<Runtime>::contains_key(id));
			assert_eq!(BagsList::count(), counter);
			assert_eq!(ListNodes::<Runtime>::count(), counter);
			assert_eq!(ListNodes::<Runtime>::iter().count() as u32, counter);
		};

		ExtBuilder::default().build_and_execute(|| {
			// it is a noop removing a non-existent id
			assert!(!ListNodes::<Runtime>::contains_key(42));
			assert_noop!(BagsList::on_remove(&42), ListError::NodeNotFound);

			// when removing a node from a bag with multiple nodes
			BagsList::on_remove(&2).unwrap();

			// then
			assert_eq!(get_list_as_ids(), vec![3, 4, 1]);
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![3, 4])]);
			ensure_left(2, 3);

			// when removing a node from a bag with only one node
			BagsList::on_remove(&1).unwrap();

			// then
			assert_eq!(get_list_as_ids(), vec![3, 4]);
			assert_eq!(List::<Runtime>::get_bags(), vec![(1_000, vec![3, 4])]);
			ensure_left(1, 2);

			// when removing all remaining ids
			BagsList::on_remove(&4).unwrap();
			assert_eq!(get_list_as_ids(), vec![3]);
			ensure_left(4, 1);
			BagsList::on_remove(&3).unwrap();

			// then the storage is completely cleaned up
			assert_eq!(get_list_as_ids(), Vec::<AccountId>::new());
			ensure_left(3, 0);
		});
	}

	#[test]
	fn contains_works() {
		ExtBuilder::default().build_and_execute(|| {
			assert!(GENESIS_IDS.iter().all(|(id, _)| BagsList::contains(id)));

			let non_existent_ids = vec![&42, &666, &13];
			assert!(non_existent_ids.iter().all(|id| !BagsList::contains(id)));
		})
	}
}

mod on_idle {
	use super::*;
	use frame_support::traits::OnIdle;

	fn run_to_block(n: u64, on_idle_weight: Weight) -> Weight {
		let mut total_weight = Weight::zero();

		System::run_to_block_with::<AllPalletsWithSystem>(
			n,
			frame_system::RunToBlockHooks::default().after_initialize(|bn| {
				let w = AllPalletsWithSystem::on_idle(bn, on_idle_weight);
				total_weight = total_weight.saturating_add(w);
			}),
		);

		total_weight
	}

	#[test]
	fn does_nothing_when_feature_is_disabled() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			// Set auto-rebag limit to 0 nodes per block
			<Runtime as Config>::MaxAutoRebagPerBlock::set(0);
			assert_eq!(<Runtime as Config>::MaxAutoRebagPerBlock::get(), 0);

			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4])]);

			// Change the score of node 3 to make it need rebagging
			StakingMock::set_score_of(&3, 10);

			// Call on_idle
			run_to_block(1, Weight::MAX);

			// The bags should remain unchanged
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4])]);

			// LastNodeAutoRebagged should not be set
			assert_eq!(NextNodeAutoRebagged::<Runtime>::get(), None);
		});
	}

	#[test]
	fn rebags_nodes_when_feature_is_enabled() {
		ExtBuilder::default().build_and_execute(|| {
			// Set auto-rebag limit to 2 nodes per block
			<Runtime as Config>::MaxAutoRebagPerBlock::set(2);

			// given
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4])]);

			// Change score of node 3 to move it into the 10 bag
			StakingMock::set_score_of(&3, 10); // <-- ВНУТРИ build_and_execute!

			// Trigger on_idle
			run_to_block(1, Weight::MAX);

			// Assert rebagging occurred
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1, 3]), (1_000, vec![2, 4])]);
			assert_eq!(NextNodeAutoRebagged::<Runtime>::get(), Some(4));
		});
	}

	#[test]
	fn does_nothing_when_list_empty() {
		ExtBuilder::default().skip_genesis_ids().build_and_execute(|| {
			// Set auto-rebag limit to 2 nodes per block
			<Runtime as Config>::MaxAutoRebagPerBlock::set(2);

			// given
			assert_eq!(List::<Runtime>::get_bags(), vec![]);
			assert_eq!(NextNodeAutoRebagged::<Runtime>::get(), None);

			// when
			run_to_block(1, Weight::MAX);

			// then
			assert_eq!(List::<Runtime>::get_bags(), vec![]);
			assert_eq!(NextNodeAutoRebagged::<Runtime>::get(), None);
		})
	}

	#[test]
	fn rebags_limited_by_budget() {
		ExtBuilder::default().build_and_execute(|| {
			// Set auto-rebag limit to 2 nodes per block
			<Runtime as Config>::MaxAutoRebagPerBlock::set(2);

			// given
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4])]);
			assert_eq!(NextNodeAutoRebagged::<Runtime>::get(), None);

			// Change the score of all nodes
			StakingMock::set_score_of(&1, 1000);
			StakingMock::set_score_of(&2, 10);
			StakingMock::set_score_of(&3, 10);
			StakingMock::set_score_of(&4, 10);

			// Trigger on_idle
			run_to_block(1, Weight::MAX);

			// Assert only 2 rebagging happened
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1, 2, 3]), (1_000, vec![4])]);
			assert_eq!(NextNodeAutoRebagged::<Runtime>::get(), Some(4));
		});
	}

	#[test]
	fn rebags_resumes_from_node_after_rebagging() {
		ExtBuilder::default().build_and_execute(|| {
			// Set auto-rebag limit to 1 node per block
			<Runtime as Config>::MaxAutoRebagPerBlock::set(1);

			// given
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4])]);
			assert_eq!(NextNodeAutoRebagged::<Runtime>::get(), None);

			// Change the score of all nodes
			StakingMock::set_score_of(&1, 1000);
			StakingMock::set_score_of(&2, 10);
			StakingMock::set_score_of(&3, 10);
			StakingMock::set_score_of(&4, 10);

			// Trigger on_idle for 2 blocks
			run_to_block(2, Weight::MAX);

			// Assert only 2 rebagging happened
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1, 2, 3]), (1_000, vec![4])]);
			assert_eq!(NextNodeAutoRebagged::<Runtime>::get(), Some(4));
		});
	}
	#[test]
	fn can_rebag_across_bags() {
		ExtBuilder::default().build_and_execute(|| {
			// Set the auto-rebag limit to a large enough value to process all
			<Runtime as Config>::MaxAutoRebagPerBlock::set(4);

			// given
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4])]);
			assert_eq!(NextNodeAutoRebagged::<Runtime>::get(), None);

			// Change scores to make rebag across bags
			// Move 1 to 2_000 bag
			StakingMock::set_score_of(&1, 2_000);
			// Move 2,3,4 to 10 bag
			StakingMock::set_score_of(&2, 10);
			StakingMock::set_score_of(&3, 10);
			StakingMock::set_score_of(&4, 10);

			// Trigger on_idle
			run_to_block(2, Weight::MAX);

			// then — assert nodes are rebagged across bags
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![2, 3, 4]), (2_000, vec![1])]);

			// and the cursor is cleared (end of a list)
			assert_eq!(NextNodeAutoRebagged::<Runtime>::get(), None);
		});
	}

	#[test]
	fn when_we_hit_the_end_of_the_list() {
		ExtBuilder::default().build_and_execute(|| {
			// Set the auto-rebag limit to a large enough value to process all
			<Runtime as Config>::MaxAutoRebagPerBlock::set(2);

			// given
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4])]);
			assert_eq!(NextNodeAutoRebagged::<Runtime>::get(), None);

			// Change scores to make rebag across bags
			// Move 1 to 2_000 bag
			StakingMock::set_score_of(&1, 2_000);
			// Move 2,3,4 to 10 bag
			StakingMock::set_score_of(&2, 10);
			StakingMock::set_score_of(&3, 10);
			StakingMock::set_score_of(&4, 10);

			// Trigger on_idle
			run_to_block(4, Weight::MAX);

			// then — assert nodes are rebagged across bags
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![2, 3, 4]), (2_000, vec![1])]);

			// and the cursor is cleared (end of a list)
			assert_eq!(NextNodeAutoRebagged::<Runtime>::get(), None);
		});
	}

	#[test]
	fn does_nothing_when_no_weight_left() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			// Set MaxAutoRebagPerBlock to a non-zero value to allow rebagging in theory
			<Runtime as Config>::MaxAutoRebagPerBlock::set(2);

			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4])]);
			assert_eq!(NextNodeAutoRebagged::<Runtime>::get(), None);

			// Modify scores to trigger rebagging logic.
			StakingMock::set_score_of(&1, 2_000);
			StakingMock::set_score_of(&2, 10);
			StakingMock::set_score_of(&3, 10);
			StakingMock::set_score_of(&4, 10);

			// Trigger on_idle with zero available weight
			let weight_used = run_to_block(4, Weight::zero());

			// Confirm no weight was consumed
			assert_eq!(weight_used, Weight::zero());

			// Nothing should change due to lack of available weight
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4])]);

			// Cursor should not have advanced
			assert_eq!(NextNodeAutoRebagged::<Runtime>::get(), None);
		});
	}

	#[test]
	fn pending_rebag_not_used_when_auto_rebag_disabled() {
		ExtBuilder::default().skip_genesis_ids().build_and_execute(|| {
			<Runtime as Config>::MaxAutoRebagPerBlock::set(0);

			assert_ok!(List::<Runtime>::insert(1, 10));
			assert_eq!(PendingRebag::<Runtime>::count(), 0);

			BagsList::lock();

			// Try to insert while locked with auto-rebag disabled
			assert_eq!(BagsList::on_insert(5, 15), Err(ListError::Locked));

			// Should NOT be added to PendingRebag since auto-rebag is disabled
			assert_eq!(PendingRebag::<Runtime>::count(), 0);
			assert!(!PendingRebag::<Runtime>::contains_key(&5));

			BagsList::unlock();

			// The account can still be inserted manually via rebag extrinsic
			StakingMock::set_score_of(&5, 15);
			assert_ok!(List::<Runtime>::insert(5, 15));
			assert!(List::<Runtime>::contains(&5));
		});
	}

	/// Tests the PendingRebag feature that handles accounts that fail to be inserted due to
	/// locking.
	#[test]
	fn pending_rebag() {
		ExtBuilder::default().skip_genesis_ids().build_and_execute(|| {
			<Runtime as Config>::MaxAutoRebagPerBlock::set(3);

			// Create more initial nodes to ensure cursor is set
			assert_ok!(List::<Runtime>::insert(1, 10));
			assert_ok!(List::<Runtime>::insert(2, 1000));
			assert_ok!(List::<Runtime>::insert(3, 1000));
			assert_ok!(List::<Runtime>::insert(4, 1000));
			assert_ok!(List::<Runtime>::insert(9, 1000));
			assert_ok!(List::<Runtime>::insert(10, 1000));

			assert_eq!(
				List::<Runtime>::get_bags(),
				vec![(10, vec![1]), (1_000, vec![2, 3, 4, 9, 10])]
			);
			assert_eq!(PendingRebag::<Runtime>::count(), 0);

			BagsList::lock();

			// Try to insert 6 new nodes while locked - 5 regular + 1 that will lose staking status
			assert_eq!(BagsList::on_insert(5, 15), Err(ListError::Locked));
			assert_eq!(BagsList::on_insert(6, 45), Err(ListError::Locked));
			assert_eq!(BagsList::on_insert(7, 55), Err(ListError::Locked));
			assert_eq!(BagsList::on_insert(8, 1500), Err(ListError::Locked));
			assert_eq!(BagsList::on_insert(11, 100), Err(ListError::Locked));
			assert_eq!(BagsList::on_insert(99, 500), Err(ListError::Locked)); // Will lose staking

			// Verify they're in PendingRebag
			let pending: Vec<_> = PendingRebag::<Runtime>::iter_keys().collect();
			assert_eq_uvec!(pending, vec![5, 6, 7, 8, 11, 99]);

			// Verify they're NOT in the list yet
			assert!(!List::<Runtime>::contains(&5));
			assert!(!List::<Runtime>::contains(&6));
			assert!(!List::<Runtime>::contains(&7));
			assert!(!List::<Runtime>::contains(&8));
			assert!(!List::<Runtime>::contains(&11));
			assert!(!List::<Runtime>::contains(&99));

			BagsList::unlock();

			// Set scores for regular nodes
			StakingMock::set_score_of(&1, 10); // Keep account 1 at score 10
			StakingMock::set_score_of(&2, 10); // Regular node needs rebagging
			StakingMock::set_score_of(&3, 20); // Regular node needs rebagging
			StakingMock::set_score_of(&4, 1000); // Keep account 4 at score 1000
			StakingMock::set_score_of(&9, 1000); // Keep account 9 at score 1000
			StakingMock::set_score_of(&10, 1000); // Keep account 10 at score 1000

			StakingMock::set_score_of(&5, 15);
			StakingMock::set_score_of(&6, 45);
			StakingMock::set_score_of(&7, 55);
			StakingMock::set_score_of(&8, 1500);
			StakingMock::set_score_of(&11, 100);
			// Note: account 99 deliberately has NO score provider - it will be cleaned up

			// Run on_idle with budget of 3 - processes first 3 accounts: [6, 5, 8]
			let weight_used = run_to_block(1, Weight::MAX);
			assert!(weight_used.ref_time() > 0);
			assert!(weight_used.proof_size() > 0);

			// With iteration order [6, 5, 8, 7, 11, 99] and budget of 3, we process: [6, 5, 8]
			// Account 99 remains pending (not reached with budget 3)
			assert_eq!(PendingRebag::<Runtime>::count(), 3); // Three pending accounts remain (7, 11, 99)

			let expected_processed = vec![5, 6, 8];
			let expected_unprocessed = vec![7, 11, 99];

			let actual_processed: Vec<_> = [5, 6, 7, 8, 11]
				.iter()
				.filter(|id| List::<Runtime>::contains(id))
				.copied()
				.collect();
			assert_eq!(actual_processed, expected_processed);

			let actual_pending: Vec<_> = [5, 6, 7, 8, 11, 99]
				.iter()
				.filter(|id| PendingRebag::<Runtime>::contains_key(id))
				.copied()
				.collect();
			assert_eq!(actual_pending, expected_unprocessed);

			// Verify account 99 is still pending (not processed yet due to budget limit)
			assert!(!List::<Runtime>::contains(&99));
			assert!(PendingRebag::<Runtime>::contains_key(&99));

			assert_eq!(List::<Runtime>::get_score(&6).unwrap(), 45);
			assert_eq!(List::<Runtime>::get_score(&5).unwrap(), 15);
			assert_eq!(List::<Runtime>::get_score(&8).unwrap(), 1500);

			// Verify the bags contain the right accounts
			// After processing 3 pending accounts (5, 6, 8):
			// - Account 1: score 10 -> bag 10 (unchanged)
			// - Account 2: score 10 -> still in bag 1000 (not rebagged, only pending accounts
			//   processed)
			// - Account 3: score 20 -> still in bag 1000 (not rebagged, only pending accounts
			//   processed)
			// - Account 4: score 1000 -> bag 1000 (unchanged)
			// - Account 5: score 15 -> bag 20 (newly inserted from pending)
			// - Account 6: score 45 -> bag 50 (newly inserted from pending)
			// - Account 8: score 1500 -> bag 2000 (newly inserted from pending)
			// - Account 9: score 1000 -> bag 1000 (unchanged)
			// - Account 10: score 1000 -> bag 1000 (unchanged)
			assert_eq!(
				List::<Runtime>::get_bags(),
				vec![
					(10, vec![1]),
					(20, vec![5]),
					(50, vec![6]),
					(1_000, vec![2, 3, 4, 9, 10]),
					(2_000, vec![8])
				]
			);

			// After processing 3 pending accounts [6, 5, 8] with budget of 3:
			// The 4th account collected would be the next pending account (7), but since
			// we filter out pending accounts from the cursor, it should be None
			let cursor_after_first = NextNodeAutoRebagged::<Runtime>::get();
			assert_eq!(
				cursor_after_first, None,
				"Cursor should be None since we only processed pending accounts"
			);

			// Process remaining pending accounts (budget 3: accounts 7, 11, 99)
			run_to_block(2, Weight::MAX);

			// All pending accounts should now be processed (including cleanup of 99)
			assert_eq!(PendingRebag::<Runtime>::count(), 0);

			// Verify accounts 7 and 11 are now in the list
			assert!(List::<Runtime>::contains(&7));
			assert!(List::<Runtime>::contains(&11));

			// Verify account 99 was cleaned up (not in list, removed from pending)
			assert!(!List::<Runtime>::contains(&99));
			assert!(!PendingRebag::<Runtime>::contains_key(&99));

			// Verify all processed accounts are in their correct bags
			let final_bags = List::<Runtime>::get_bags();
			assert!(final_bags.iter().any(|(t, accs)| *t == 20 && accs.contains(&5)));
			assert!(final_bags.iter().any(|(t, accs)| *t == 50 && accs.contains(&6)));
			assert!(final_bags.iter().any(|(t, accs)| *t == 60 && accs.contains(&7)));
			assert!(final_bags.iter().any(|(t, accs)| *t == 2_000 && accs.contains(&8)));
			assert!(final_bags.iter().any(|(t, accs)| *t == 1_000 && accs.contains(&11)));

			// Verify final list contains exactly the expected accounts (original + successfully
			// inserted pending)
			let final_list: Vec<_> = List::<Runtime>::iter().map(|n| *n.id()).collect();
			let mut expected_final = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
			expected_final.sort();
			let mut actual_final = final_list.clone();
			actual_final.sort();
			assert_eq!(actual_final, expected_final);
		});
	}

	#[test]
	fn rebag_extrinsic_removes_from_pending() {
		ExtBuilder::default().skip_genesis_ids().build_and_execute(|| {
			BagsList::lock();

			// Try to insert while locked - should go to PendingRebag
			StakingMock::set_score_of(&1, 1000);
			assert_eq!(BagsList::on_insert(1, 1000), Err(ListError::Locked));
			assert!(PendingRebag::<Runtime>::contains_key(&1));
			assert!(!List::<Runtime>::contains(&1));

			// Unlock
			BagsList::unlock();

			// Call rebag extrinsic - should insert the account and remove from PendingRebag
			assert_ok!(BagsList::rebag(RuntimeOrigin::signed(0), 1));

			// Verify account is now in the list and removed from PendingRebag
			assert!(!PendingRebag::<Runtime>::contains_key(&1));
			assert_eq!(List::<Runtime>::get_bags(), vec![(1000, vec![1])]);
		});
	}
}

pub mod lock {
	use super::*;

	#[test]
	fn lock_prevents_list_update() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4])]);

			// when
			BagsList::lock();

			assert_noop!(BagsList::on_update(&3, 2_000), ListError::Locked);
			assert_noop!(BagsList::on_increase(&3, 2_000), ListError::Locked);
			assert_noop!(BagsList::on_decrease(&3, 2_000), ListError::Locked);
			assert_noop!(BagsList::on_remove(&3), ListError::Locked);

			// when
			BagsList::unlock();

			// then
			assert_ok!(BagsList::on_remove(&3));
		})
	}

	#[test]
	fn lock_prevents_calls() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			assert_eq!(List::<Runtime>::get_bags(), vec![(10, vec![1]), (1_000, vec![2, 3, 4])]);

			// when
			BagsList::lock();

			// then
			assert_noop!(BagsList::rebag(RuntimeOrigin::signed(0), 3), Error::<Runtime>::Locked);
			assert_noop!(
				BagsList::put_in_front_of(RuntimeOrigin::signed(3), 4),
				Error::<Runtime>::Locked
			);
			assert_noop!(
				BagsList::put_in_front_of_other(RuntimeOrigin::signed(0), 3u64, 4),
				Error::<Runtime>::Locked
			);
		})
	}
}
