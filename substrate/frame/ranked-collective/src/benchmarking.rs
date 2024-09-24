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

//! Staking pallet benchmarking.

use super::*;
#[allow(unused_imports)]
use crate::Pallet as RankedCollective;
use alloc::vec::Vec;

use frame_benchmarking::{
	v1::{account, BenchmarkError},
	v2::*,
};

use frame_support::{assert_err, assert_ok};
use frame_system::RawOrigin as SystemOrigin;

const SEED: u32 = 0;

fn assert_last_event<T: Config<I>, I: 'static>(generic_event: <T as Config<I>>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

fn assert_has_event<T: Config<I>, I: 'static>(generic_event: <T as Config<I>>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_has_event(generic_event.into());
}

fn make_member<T: Config<I>, I: 'static>(rank: Rank) -> T::AccountId {
	let who = account::<T::AccountId>("member", MemberCount::<T, I>::get(0), SEED);
	let who_lookup = T::Lookup::unlookup(who.clone());
	assert_ok!(Pallet::<T, I>::add_member(
		T::AddOrigin::try_successful_origin()
			.expect("AddOrigin has no successful origin required for the benchmark"),
		who_lookup.clone(),
	));
	for _ in 0..rank {
		assert_ok!(Pallet::<T, I>::promote_member(
			T::PromoteOrigin::try_successful_origin()
				.expect("PromoteOrigin has no successful origin required for the benchmark"),
			who_lookup.clone(),
		));
	}
	who
}

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn add_member() -> Result<(), BenchmarkError> {
		// Generate a test account for the new member.
		let who = account::<T::AccountId>("member", 0, SEED);
		let who_lookup = T::Lookup::unlookup(who.clone());

		// Attempt to get the successful origin for adding a member.
		let origin =
			T::AddOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, who_lookup);

		// Ensure the member count has increased (or is 1 for rank 0).
		assert_eq!(MemberCount::<T, I>::get(0), 1);

		// Check that the correct event was emitted.
		assert_last_event::<T, I>(Event::MemberAdded { who }.into());

		Ok(())
	}

	#[benchmark]
	fn remove_member(r: Linear<0, 10>) -> Result<(), BenchmarkError> {
		// Convert `r` to a rank and create members.
		let rank = r as u16;
		let who = make_member::<T, I>(rank);
		let who_lookup = T::Lookup::unlookup(who.clone());
		let last = make_member::<T, I>(rank);

		// Collect the index of the `last` member for each rank.
		let last_index: Vec<_> =
			(0..=rank).map(|r| IdToIndex::<T, I>::get(r, &last).unwrap()).collect();

		// Fetch the remove origin.
		let origin =
			T::RemoveOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, who_lookup, rank);

		for r in 0..=rank {
			assert_eq!(MemberCount::<T, I>::get(r), 1);
			assert_ne!(last_index[r as usize], IdToIndex::<T, I>::get(r, &last).unwrap());
		}

		// Ensure the correct event was emitted for the member removal.
		assert_last_event::<T, I>(Event::MemberRemoved { who, rank }.into());

		Ok(())
	}

	#[benchmark]
	fn promote_member(r: Linear<0, 10>) -> Result<(), BenchmarkError> {
		// Convert `r` to a rank and create the member.
		let rank = r as u16;
		let who = make_member::<T, I>(rank);
		let who_lookup = T::Lookup::unlookup(who.clone());

		// Try to fetch the promotion origin.
		let origin =
			T::PromoteOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, who_lookup);

		// Ensure the member's rank has increased by 1.
		assert_eq!(Members::<T, I>::get(&who).unwrap().rank, rank + 1);

		// Ensure the correct event was emitted for the rank change.
		assert_last_event::<T, I>(Event::RankChanged { who, rank: rank + 1 }.into());

		Ok(())
	}

	#[benchmark]
	fn demote_member(r: Linear<0, 10>) -> Result<(), BenchmarkError> {
		// Convert `r` to a rank and create necessary members for the benchmark.
		let rank = r as u16;
		let who = make_member::<T, I>(rank);
		let who_lookup = T::Lookup::unlookup(who.clone());
		let last = make_member::<T, I>(rank);

		// Get the last index for the member.
		let last_index = IdToIndex::<T, I>::get(rank, &last).unwrap();

		// Try to fetch the demotion origin.
		let origin =
			T::DemoteOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, who_lookup);

		// Ensure the member's rank has decreased by 1.
		assert_eq!(Members::<T, I>::get(&who).map(|x| x.rank), rank.checked_sub(1));

		// Ensure the member count remains as expected.
		assert_eq!(MemberCount::<T, I>::get(rank), 1);

		// Ensure the index of the last member has changed.
		assert_ne!(last_index, IdToIndex::<T, I>::get(rank, &last).unwrap());

		// Ensure the correct event was emitted depending on the member's rank.
		assert_last_event::<T, I>(
			match rank {
				0 => Event::MemberRemoved { who, rank: 0 },
				r => Event::RankChanged { who, rank: r - 1 },
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn vote() -> Result<(), BenchmarkError> {
		// Find the first class or use a default one if none exists.
		let (class_exists, class) = T::Polls::classes()
			.into_iter()
			.next()
			.map(|c| (true, c))
			.unwrap_or_else(|| (false, Default::default()));

		// Convert the class to a rank and create a caller based on that rank.
		let rank = T::MinRankOfClass::convert(class.clone());
		let caller = make_member::<T, I>(rank);

		// If the class exists, create an ongoing poll, otherwise use a default poll.
		let poll = if class_exists {
			T::Polls::create_ongoing(class.clone())
				.expect("Should always be able to create a poll for rank 0")
		} else {
			Default::default()
		};

		// Benchmark the vote logic.
		#[block]
		{
			let vote_result =
				Pallet::<T, I>::vote(SystemOrigin::Signed(caller.clone()).into(), poll, true);

			// If the class exists, expect a successful vote, otherwise expect a `NotPolling` error.
			if class_exists {
				assert_ok!(vote_result);
			} else {
				assert_err!(vote_result, crate::Error::<T, I>::NotPolling);
			}
		}

		// Vote again with a different decision (false).
		let vote_false =
			Pallet::<T, I>::vote(SystemOrigin::Signed(caller.clone()).into(), poll, false);

		if class_exists {
			assert_ok!(vote_false);
		} else {
			assert_err!(vote_false, crate::Error::<T, I>::NotPolling);
		}

		// If the class exists, verify the vote event and tally.
		if class_exists {
			let tally = Tally::from_parts(0, 0, 1);
			let vote_event = Event::Voted { who: caller, poll, vote: VoteRecord::Nay(1), tally };
			assert_last_event::<T, I>(vote_event.into());
		}

		Ok(())
	}

	#[benchmark]
	fn cleanup_poll(n: Linear<0, 100>) -> Result<(), BenchmarkError> {
		// Try to find an existing class or default to a new one.
		let (class_exists, class) = T::Polls::classes()
			.into_iter()
			.next()
			.map(|c| (true, c))
			.unwrap_or_else(|| (false, Default::default()));
		let alice: T::AccountId = whitelisted_caller();
		let origin = SystemOrigin::Signed(alice.clone());

		// Get rank and create a poll if the class exists.
		let rank = T::MinRankOfClass::convert(class.clone());
		let poll = if class_exists {
			T::Polls::create_ongoing(class.clone())
				.expect("Poll creation should succeed for rank 0")
		} else {
			Default::default()
		};

		// Simulate voting in the poll by `n` members.
		for _ in 0..n {
			let voter = make_member::<T, I>(rank);
			let result = Pallet::<T, I>::vote(SystemOrigin::Signed(voter).into(), poll, true);

			// Check voting results based on class existence.
			if class_exists {
				assert_ok!(result);
			} else {
				assert_err!(result, crate::Error::<T, I>::NotPolling);
			}
		}

		// End the poll if the class exists.
		if class_exists {
			T::Polls::end_ongoing(poll, false).expect("Poll should be able to end");
		}

		// Verify the number of votes cast.
		let expected_votes = if class_exists { n as usize } else { 0 };
		assert_eq!(Voting::<T, I>::iter_prefix(poll).count(), expected_votes);

		// Benchmark the cleanup function.
		#[extrinsic_call]
		_(origin, poll, n);

		// Ensure all votes are cleaned up.
		assert_eq!(Voting::<T, I>::iter().count(), 0);
		Ok(())
	}

	#[benchmark]
	fn exchange_member() -> Result<(), BenchmarkError> {
		// Create an existing member.
		let who = make_member::<T, I>(1);
		T::BenchmarkSetup::ensure_member(&who);
		let who_lookup = T::Lookup::unlookup(who.clone());

		// Create a new account for the new member.
		let new_who = account::<T::AccountId>("new-member", 0, SEED);
		let new_who_lookup = T::Lookup::unlookup(new_who.clone());

		// Attempt to get the successful origin for exchanging a member.
		let origin =
			T::ExchangeOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, who_lookup, new_who_lookup);

		// Check that the new member was successfully exchanged and holds the correct rank.
		assert_eq!(Members::<T, I>::get(&new_who).unwrap().rank, 1);

		// Ensure the old member no longer exists.
		assert_eq!(Members::<T, I>::get(&who), None);

		// Ensure the correct event was emitted.
		assert_has_event::<T, I>(Event::MemberExchanged { who, new_who }.into());

		Ok(())
	}

	impl_benchmark_test_suite!(
		RankedCollective,
		crate::tests::ExtBuilder::default().build(),
		crate::tests::Test
	);
}
