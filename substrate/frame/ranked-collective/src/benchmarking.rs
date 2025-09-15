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

use frame_support::{assert_err, assert_ok, traits::NoOpPoll};
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

#[instance_benchmarks(
where
	<<T as pallet::Config<I>>::Polls as frame_support::traits::Polling<Tally<T, I, pallet::Pallet<T, I>>>>::Index: From<u8>,
	<T as frame_system::Config>::RuntimeEvent: TryInto<pallet::Event<T, I>>,
)]
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
		// Get the first available class or set it to None if no class exists.
		let class = T::Polls::classes().into_iter().next();

		// Convert the class to a rank if it exists, otherwise use the default rank.
		let rank = class.as_ref().map_or(
			<Pallet<T, I> as frame_support::traits::RankedMembers>::Rank::default(),
			|class| T::MinRankOfClass::convert(class.clone()),
		);

		// Create a caller based on the rank.
		let caller = make_member::<T, I>(rank);

		// Determine the poll to use: create an ongoing poll if class exists, or use an invalid
		// poll.
		let poll = if let Some(ref class) = class {
			T::Polls::create_ongoing(class.clone())
				.expect("Poll creation should succeed for rank 0")
		} else {
			<NoOpPoll as Polling<T>>::Index::MAX.into()
		};

		// Benchmark the vote logic for a positive vote (true).
		#[block]
		{
			let vote_result =
				Pallet::<T, I>::vote(SystemOrigin::Signed(caller.clone()).into(), poll, true);

			// If the class exists, expect success; otherwise expect a "NotPolling" error.
			if class.is_some() {
				assert_ok!(vote_result);
			} else {
				assert_err!(vote_result, crate::Error::<T, I>::NotPolling);
			};
		}

		// Vote logic for a negative vote (false).
		let vote_result =
			Pallet::<T, I>::vote(SystemOrigin::Signed(caller.clone()).into(), poll, false);

		// Check the result of the negative vote.
		if class.is_some() {
			assert_ok!(vote_result);
		} else {
			assert_err!(vote_result, crate::Error::<T, I>::NotPolling);
		};

		// If the class exists, verify the vote event and tally.
		if let Some(_) = class {
			// Get the actual vote weight from the latest event's VoteRecord::Nay
			let mut events = frame_system::Pallet::<T>::events();
			let last_event = events.pop().expect("At least one event should exist");
			let event: Event<T, I> = last_event
				.event
				.try_into()
				.unwrap_or_else(|_| panic!("Event conversion failed"));

			match event {
				Event::Voted { vote: VoteRecord::Nay(vote_weight), who, poll: poll2, tally } => {
					assert_eq!(tally, Tally::from_parts(0, 0, vote_weight));
					assert_eq!(caller, who);
					assert_eq!(poll, poll2);
				},
				_ => panic!("Invalid event"),
			};
		}

		Ok(())
	}

	#[benchmark]
	fn cleanup_poll(n: Linear<0, 100>) -> Result<(), BenchmarkError> {
		let alice: T::AccountId = whitelisted_caller();
		let origin = SystemOrigin::Signed(alice.clone());

		// Try to retrieve the first class if it exists.
		let class = T::Polls::classes().into_iter().next();

		// Convert the class to a rank, or use a default rank if no class exists.
		let rank = class.as_ref().map_or(
			<Pallet<T, I> as frame_support::traits::RankedMembers>::Rank::default(),
			|class| T::MinRankOfClass::convert(class.clone()),
		);

		// Determine the poll to use: create an ongoing poll if class exists, or use an invalid
		// poll.
		let poll = if let Some(ref class) = class {
			T::Polls::create_ongoing(class.clone())
				.expect("Poll creation should succeed for rank 0")
		} else {
			<NoOpPoll as Polling<T>>::Index::MAX.into()
		};

		// Simulate voting by `n` members.
		for _ in 0..n {
			let voter = make_member::<T, I>(rank);
			let result = Pallet::<T, I>::vote(SystemOrigin::Signed(voter).into(), poll, true);

			// Check voting results based on class existence.
			if class.is_some() {
				assert_ok!(result);
			} else {
				assert_err!(result, crate::Error::<T, I>::NotPolling);
			}
		}

		// End the poll if the class exists.
		if class.is_some() {
			T::Polls::end_ongoing(poll, false)
				.map_err(|_| BenchmarkError::Stop("Failed to end poll"))?;
		}

		// Verify the number of votes cast.
		let expected_votes = if class.is_some() { n as usize } else { 0 };
		assert_eq!(Voting::<T, I>::iter_prefix(poll).count(), expected_votes);

		// Benchmark the cleanup function.
		#[extrinsic_call]
		_(origin, poll, n);

		// Ensure all votes are cleaned up after the extrinsic call.
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
