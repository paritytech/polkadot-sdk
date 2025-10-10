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

//! Benchmarks for the bags list pallet.

use super::*;
use crate::list::List;
use alloc::{vec, vec::Vec};
use frame_benchmarking::v1::{
	account, benchmarks_instance_pallet, whitelist_account, whitelisted_caller,
};
use frame_election_provider_support::ScoreProvider;
use frame_support::{assert_ok, traits::Get};
use frame_system::RawOrigin as SystemOrigin;
use sp_runtime::traits::One;

benchmarks_instance_pallet! {
	// iteration of any number of items should only touch that many nodes and bags.
	#[extra]
	iter {
		let n = 100;

		// clear any pre-existing storage.
		List::<T, _>::unsafe_clear();

		// add n nodes, half to the first bag and half to the second bag.
		let bag_thresh = T::BagThresholds::get()[0];
		let second_bag_thresh = T::BagThresholds::get()[1];


		for i in 0..n/2 {
			let node: T::AccountId = account("node", i, 0);
			assert_ok!(List::<T, _>::insert(node.clone(), bag_thresh - One::one()));
		}
		for i in 0..n/2 {
			let node: T::AccountId = account("node", i, 1);
			assert_ok!(List::<T, _>::insert(node.clone(), bag_thresh + One::one()));
		}
		assert_eq!(
			List::<T, _>::get_bags().into_iter().map(|(bag, nodes)| (bag, nodes.len())).collect::<Vec<_>>(),
			vec![
				(bag_thresh, (n / 2) as usize),
				(second_bag_thresh, (n / 2) as usize),
			]
		);
	}: {
		let voters = <Pallet<T, _> as SortedListProvider<T::AccountId>>::iter();
		let len = voters.collect::<Vec<_>>().len();
		assert_eq!(len as u32, n,"len is {}, expected {}", len, n);
	}

	// iteration of any number of items should only touch that many nodes and bags.
	#[extra]
	iter_take {
		let n = 100;

		// clear any pre-existing storage.
		List::<T, _>::unsafe_clear();

		// add n nodes, half to the first bag and half to the second bag.
		let bag_thresh = T::BagThresholds::get()[0];
		let second_bag_thresh = T::BagThresholds::get()[1];


		for i in 0..n/2 {
			let node: T::AccountId = account("node", i, 0);
			assert_ok!(List::<T, _>::insert(node.clone(), bag_thresh - One::one()));
		}
		for i in 0..n/2 {
			let node: T::AccountId = account("node", i, 1);
			assert_ok!(List::<T, _>::insert(node.clone(), bag_thresh + One::one()));
		}
		assert_eq!(
			List::<T, _>::get_bags().into_iter().map(|(bag, nodes)| (bag, nodes.len())).collect::<Vec<_>>(),
			vec![
				(bag_thresh, (n / 2) as usize),
				(second_bag_thresh, (n / 2) as usize),
			]
		);
	}: {
		// this should only go into one of the bags
		let voters = <Pallet<T, _> as SortedListProvider<T::AccountId>>::iter().take(n as usize / 4 );
		let len = voters.collect::<Vec<_>>().len();
		assert_eq!(len as u32, n / 4,"len is {}, expected {}", len, n / 4);
	}

	#[extra]
	iter_next {
		let n = 100;

		// clear any pre-existing storage.
		List::<T, _>::unsafe_clear();

		// add n nodes, half to the first bag and half to the second bag.
		let bag_thresh = T::BagThresholds::get()[0];
		let second_bag_thresh = T::BagThresholds::get()[1];


		for i in 0..n/2 {
			let node: T::AccountId = account("node", i, 0);
			assert_ok!(List::<T, _>::insert(node.clone(), bag_thresh - One::one()));
		}
		for i in 0..n/2 {
			let node: T::AccountId = account("node", i, 1);
			assert_ok!(List::<T, _>::insert(node.clone(), bag_thresh + One::one()));
		}
		assert_eq!(
			List::<T, _>::get_bags().into_iter().map(|(bag, nodes)| (bag, nodes.len())).collect::<Vec<_>>(),
			vec![
				(bag_thresh, (n / 2) as usize),
				(second_bag_thresh, (n / 2) as usize),
			]
		);
	}: {
		// this should only go into one of the bags
		let mut iter_var = <Pallet<T, _> as SortedListProvider<T::AccountId>>::iter();
		let mut voters = Vec::<T::AccountId>::with_capacity((n/4) as usize);
		for _ in 0..(n/4) {
			let next = iter_var.next().unwrap();
			voters.push(next);
		}

		let len = voters.len();
		assert_eq!(len as u32, n / 4,"len is {}, expected {}", len, n / 4);
	}

	#[extra]
	iter_from {
		let n = 100;

		// clear any pre-existing storage.
		List::<T, _>::unsafe_clear();

		// populate the first 4 bags with n/4 nodes each
		let bag_thresh = T::BagThresholds::get()[0];

		for i in 0..n/4 {
			let node: T::AccountId = account("node", i, 0);
			assert_ok!(List::<T, _>::insert(node.clone(), bag_thresh - One::one()));
		}
		for i in 0..n/4 {
			let node: T::AccountId = account("node", i, 1);
			assert_ok!(List::<T, _>::insert(node.clone(), bag_thresh + One::one()));
		}

		let bag_thresh = T::BagThresholds::get()[2];

		for i in 0..n/4 {
			let node: T::AccountId = account("node", i, 2);
			assert_ok!(List::<T, _>::insert(node.clone(), bag_thresh - One::one()));
		}

		for i in 0..n/4 {
			let node: T::AccountId = account("node", i, 3);
			assert_ok!(List::<T, _>::insert(node.clone(), bag_thresh + One::one()));
		}

		assert_eq!(
			List::<T, _>::get_bags().into_iter().map(|(bag, nodes)| (bag, nodes.len())).collect::<Vec<_>>(),
			vec![
				(T::BagThresholds::get()[0], (n / 4) as usize),
				(T::BagThresholds::get()[1], (n / 4) as usize),
				(T::BagThresholds::get()[2], (n / 4) as usize),
				(T::BagThresholds::get()[3], (n / 4) as usize),
			]
		);

		// iter from someone in the 3rd bag, so this should touch ~75 nodes and 3 bags
		let from: T::AccountId = account("node", 0, 2);
	}: {
		let voters = <Pallet<T, _> as SortedListProvider<T::AccountId>>::iter_from(&from).unwrap();
		let len = voters.collect::<Vec<_>>().len();
		assert_eq!(len as u32, 74,"len is {}, expected {}", len, 74);
	}


	rebag_non_terminal {
		// An expensive case for rebag-ing (rebag a non-terminal node):
		//
		// - The node to be rebagged, _R_, should exist as a non-terminal node in a bag with at
		//   least 2 other nodes. Thus _R_ will have both its `prev` and `next` nodes updated when
		//   it is removed. (3 W/R)
		// - The destination bag is not empty, thus we need to update the `next` pointer of the last
		//   node in the destination in addition to the work we do otherwise. (2 W/R)

		// clear any pre-existing storage.
		// NOTE: safe to call outside block production
		List::<T, _>::unsafe_clear();

		// define our origin and destination thresholds.
		let origin_bag_thresh = T::BagThresholds::get()[0];
		let dest_bag_thresh = T::BagThresholds::get()[1];

		// seed items in the origin bag.
		let origin_head: T::AccountId = account("origin_head", 0, 0);
		assert_ok!(List::<T, _>::insert(origin_head.clone(), origin_bag_thresh));

		let origin_middle: T::AccountId = account("origin_middle", 0, 0); // the node we rebag (_R_)
		assert_ok!(List::<T, _>::insert(origin_middle.clone(), origin_bag_thresh));

		let origin_tail: T::AccountId  = account("origin_tail", 0, 0);
		assert_ok!(List::<T, _>::insert(origin_tail.clone(), origin_bag_thresh));

		// seed items in the destination bag.
		let dest_head: T::AccountId  = account("dest_head", 0, 0);
		assert_ok!(List::<T, _>::insert(dest_head.clone(), dest_bag_thresh));

		let origin_middle_lookup = T::Lookup::unlookup(origin_middle.clone());

		// the bags are in the expected state after initial setup.
		assert_eq!(
			List::<T, _>::get_bags(),
			vec![
				(origin_bag_thresh, vec![origin_head.clone(), origin_middle.clone(), origin_tail.clone()]),
				(dest_bag_thresh, vec![dest_head.clone()])
			]
		);

		let caller = whitelisted_caller();
		// update the weight of `origin_middle` to guarantee it will be rebagged into the destination.
		T::ScoreProvider::set_score_of(&origin_middle, dest_bag_thresh);
	}: rebag(SystemOrigin::Signed(caller), origin_middle_lookup.clone())
	verify {
		// check the bags have updated as expected.
		assert_eq!(
			List::<T, _>::get_bags(),
			vec![
				(
					origin_bag_thresh,
					vec![origin_head, origin_tail],
				),
				(
					dest_bag_thresh,
					vec![dest_head, origin_middle],
				)
			]
		);
	}

	rebag_terminal {
		// An expensive case for rebag-ing (rebag a terminal node):
		//
		// - The node to be rebagged, _R_, is a terminal node; so _R_, the node pointing to _R_ and
		//   the origin bag itself will need to be updated. (3 W/R)
		// - The destination bag is not empty, thus we need to update the `next` pointer of the last
		//   node in the destination in addition to the work we do otherwise. (2 W/R)

		// clear any pre-existing storage.
		// NOTE: safe to call outside block production
		List::<T, I>::unsafe_clear();

		// define our origin and destination thresholds.
		let origin_bag_thresh = T::BagThresholds::get()[0];
		let dest_bag_thresh = T::BagThresholds::get()[1];

		// seed items in the origin bag.
		let origin_head: T::AccountId = account("origin_head", 0, 0);
		assert_ok!(List::<T, _>::insert(origin_head.clone(), origin_bag_thresh));

		let origin_tail: T::AccountId  = account("origin_tail", 0, 0); // the node we rebag (_R_)
		assert_ok!(List::<T, _>::insert(origin_tail.clone(), origin_bag_thresh));

		// seed items in the destination bag.
		let dest_head: T::AccountId  = account("dest_head", 0, 0);
		assert_ok!(List::<T, _>::insert(dest_head.clone(), dest_bag_thresh));

		let origin_tail_lookup = T::Lookup::unlookup(origin_tail.clone());

		// the bags are in the expected state after initial setup.
		assert_eq!(
			List::<T, _>::get_bags(),
			vec![
				(origin_bag_thresh, vec![origin_head.clone(), origin_tail.clone()]),
				(dest_bag_thresh, vec![dest_head.clone()])
			]
		);

		let caller = whitelisted_caller();
		// update the weight of `origin_tail` to guarantee it will be rebagged into the destination.
		T::ScoreProvider::set_score_of(&origin_tail, dest_bag_thresh);
	}: rebag(SystemOrigin::Signed(caller), origin_tail_lookup.clone())
	verify {
		// check the bags have updated as expected.
		assert_eq!(
			List::<T, _>::get_bags(),
			vec![
				(origin_bag_thresh, vec![origin_head.clone()]),
				(dest_bag_thresh, vec![dest_head.clone(), origin_tail])
			]
		);
	}

	put_in_front_of {
		// The most expensive case for `put_in_front_of`:
		//
		// - both heavier's `prev` and `next` are nodes that will need to be read and written.
		// - `lighter` is the bag's `head`, so the bag will need to be read and written.

		// clear any pre-existing storage.
		// NOTE: safe to call outside block production
		List::<T, I>::unsafe_clear();

		let bag_thresh = T::BagThresholds::get()[0];

		// insert the nodes in order
		let lighter: T::AccountId = account("lighter", 0, 0);
		assert_ok!(List::<T, _>::insert(lighter.clone(), bag_thresh));

		let heavier_prev: T::AccountId = account("heavier_prev", 0, 0);
		assert_ok!(List::<T, _>::insert(heavier_prev.clone(), bag_thresh));

		let heavier: T::AccountId = account("heavier", 0, 0);
		assert_ok!(List::<T, _>::insert(heavier.clone(), bag_thresh));

		let heavier_next: T::AccountId = account("heavier_next", 0, 0);
		assert_ok!(List::<T, _>::insert(heavier_next.clone(), bag_thresh));

		T::ScoreProvider::set_score_of(&lighter, bag_thresh - One::one());
		T::ScoreProvider::set_score_of(&heavier, bag_thresh);

		let lighter_lookup = T::Lookup::unlookup(lighter.clone());

		assert_eq!(
			List::<T, _>::iter().map(|n| n.id().clone()).collect::<Vec<_>>(),
			vec![lighter.clone(), heavier_prev.clone(), heavier.clone(), heavier_next.clone()]
		);

		whitelist_account!(heavier);
	}: _(SystemOrigin::Signed(heavier.clone()), lighter_lookup.clone())
	verify {
		assert_eq!(
			List::<T, _>::iter().map(|n| n.id().clone()).collect::<Vec<_>>(),
			vec![heavier, lighter, heavier_prev, heavier_next]
		)
	}

	on_idle {
		// This benchmark generates weights for `on_idle` based on runtime configuration.
		// The main input is the runtime's `MaxAutoRebagPerBlock` type, which defines how many
		// nodes can be rebagged per block.
		// This benchmark simulates a scenario with both pending rebag processing
		// and fragmented rebag scenario.

		List::<T, _>::unsafe_clear();

		let bag_thresh = T::BagThresholds::get();
		let low = bag_thresh[0];
		let mid = bag_thresh[1];
		let high = bag_thresh[2];

		let rebag_budget = <T as Config<I>>::MaxAutoRebagPerBlock::get();

		// Adjust counts to ensure exact budget usage
		let pending_count = rebag_budget / 3; // Smaller portion for pending
		let regular_count = rebag_budget + 5;

		// Insert regular nodes with varying scores
		for i in 0..regular_count {
			let node: T::AccountId = account("regular_node", i, 0);
			let score = match i % 3 {
				0 => low - One::one(),
				1 => mid - One::one(),
				_ => high - One::one(),
			};
			assert_ok!(List::<T, _>::insert(node.clone(), score));
		}

		// Corrupt some nodes to simulate edge cases
		for i in (0..regular_count).step_by(4) {
			let node: T::AccountId = account("regular_node", i, 0);
			let _ = List::<T, _>::remove(&node); // orphan nodes
		}

		// Lock the list and simulate pending rebag insertions
		<Pallet<T, I>>::lock();

		// Create pending rebag entries (mix of valid and corrupted)
		for i in 0..pending_count {
			let pending_node: T::AccountId = account("pending_node", i, 0);
			let pending_score = match i % 3 {
				0 => mid,
				1 => high,
				_ => high + high,
			};

			// Set score first for most nodes, but skip some to simulate cleanup scenarios
			if i % 7 != 0 {
				T::ScoreProvider::set_score_of(&pending_node, pending_score);
			}

			let _ = <Pallet<T, I> as SortedListProvider<T::AccountId>>::on_insert(
				pending_node, pending_score
			);
		}

		<Pallet<T, I>>::unlock();

		// Now set new scores that will move nodes into higher bags
		for i in 0..regular_count {
			let node: T::AccountId = account("regular_node", i, 0);
			let new_score = match i % 3 {
				0 => mid,
				1 => high,
				_ => high + high, // force into a new top bag
			};
			T::ScoreProvider::set_score_of(&node, new_score);
		}

		assert_eq!(
			PendingRebag::<T, I>::count(),
			pending_count,
			"Expected exactly {} pending rebag entries",
			pending_count
		);
		// Ensure we have at least three bags populated before rebag
		assert!(List::<T, _>::get_bags().len() >= 2);
	}
	: {
		use frame_support::traits::Hooks;
		<Pallet<T, I> as Hooks<_>>::on_idle(Default::default(), Weight::MAX);
	}
	verify {
		// Verify all pending rebag entries were processed.
		// This should always be true since pending_count = rebag_budget / 3 < rebag_budget,
		// and pending accounts are processed first so all pending entries fit within the budget.
		assert_eq!(PendingRebag::<T, I>::count(), 0, "All pending rebag entries should be processed");

		// Count how many nodes ended up in higher bags
		let total_rebagged: usize = List::<T, _>::get_bags()
			.iter()
			.filter(|(b, _)| *b > T::BagThresholds::get()[0])
			.map(|(_, nodes)| nodes.len())
			.sum();

		let expected = <T as Config<I>>::MaxAutoRebagPerBlock::get() as usize;
		assert_eq!(total_rebagged, expected, "Expected exactly {:?} rebagged nodes, found {:?}", expected, total_rebagged);
	}

	impl_benchmark_test_suite!(
		Pallet,
		mock::ExtBuilder::default().skip_genesis_ids().build(),
		mock::Runtime
	);
}
