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
#![cfg(feature = "runtime-benchmarks")]

extern crate alloc;

use super::*;
use alloc::vec::Vec;
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	traits::{
		fungible::{Inspect, Mutate},
		Get, Polling,
	},
};
use pallet_revive::ExecOrigin;
use frame_system::RawOrigin;
use pallet_conviction_voting::{AccountVote, BalanceOf, ClassOf, Conviction, IndexOf, Vote};
use pallet_conviction_voting_precompiles::{ConvictionVotingPrecompile, IConvictionVoting};
use pallet_revive::{
	precompiles::run::{precompile as run_precompile, CallSetup},
	H160,
};
use scale_info::prelude::collections::BTreeMap;
use sp_runtime::{traits::StaticLookup, Saturating};

use pallet_conviction_voting::{Pallet as ConvictionVoting, VotingHooks};
use pallet_revive::AddressMapper;

/// Fill all classes as much as possible up to `MaxVotes` and return the Class with the most votes
/// ongoing.
fn fill_voting<T: Config<I>, I: 'static>(
) -> (ClassOf<T, I>, BTreeMap<ClassOf<T, I>, Vec<IndexOf<T, I>>>) {
	let mut r = BTreeMap::<ClassOf<T, I>, Vec<IndexOf<T, I>>>::new();
	for class in T::Polls::classes().into_iter() {
		for _ in 0..T::MaxVotes::get() {
			match T::Polls::create_ongoing(class.clone()) {
				Ok(i) => r.entry(class.clone()).or_default().push(i),
				Err(()) => break,
			}
		}
	}
	let c = r.iter().max_by_key(|(_, v)| v.len()).unwrap().0.clone();
	(c, r)
}

fn funded_mapped_account<T: Config<I>, I: 'static>(name: &'static str, index: u32) -> T::AccountId {
	let account: T::AccountId = account(name, index, 0u32);

	let funding_amount =
		<T as pallet_revive::Config>::Currency::minimum_balance().saturating_mul(100_000u32.into());

	assert_ok!(<T as pallet_revive::Config>::Currency::mint_into(&account, funding_amount));

	assert_ok!(pallet_revive::Pallet::<T>::map_account(RawOrigin::Signed(account.clone()).into()));

	account
}

#[benchmarks(
	where
	T: crate::Config,
	BalanceOf<T, ()>: TryFrom<u128> + Into<u128>,
    IndexOf<T, ()>: TryFrom<u32> + TryInto<u32>,
    ClassOf<T, ()>: TryFrom<u16> + TryInto<u16>,
)]
mod benchmarks {
	use super::*;

	#[benchmark(pov_mode = Measured)]
	fn vote_new() {
		let account = funded_mapped_account::<T, ()>("caller", 0);
		T::VotingHooks::on_vote_worst_case(&account);

		let (class, all_polls) = fill_voting::<T, ()>();
		let polls = &all_polls[&class];

		let vote = Vote { aye: true, conviction: Conviction::Locked1x };
		let balance: BalanceOf<T, ()> = 10u32.into();
		let dummy_vote = AccountVote::Standard { vote, balance };
		// We need to create existing votes, we skip 1 as we voting new
		for i in polls.iter().skip(1) {
			ConvictionVoting::<T>::vote(RawOrigin::Signed(account.clone()).into(), *i, dummy_vote)
				.unwrap();
		}

		let call = IConvictionVoting::IConvictionVotingCalls::voteSplitAbstain(
			IConvictionVoting::voteSplitAbstainCall {
				referendumIndex: 0u32,
				ayeAmount: 10u128,
				nayAmount: 10u128,
				abstainAmount: 10u128,
			},
		);

		let mut call_setup = CallSetup::<T>::default();
		let (mut ext, _) = call_setup.ext();

		let result;
		#[block]
		{
			result = run_precompile::<ConvictionVotingPrecompile<T>, _>(
				&mut ext,
				H160::from_low_u64_be(0xC0000).as_fixed_bytes(),
				&call,
			);
		}

		assert!(result.is_ok());
	}

	#[benchmark(pov_mode = Measured)]
	fn vote_existing() {
		let account = funded_mapped_account::<T, ()>("caller", 0);

		T::VotingHooks::on_vote_worst_case(&account);

		let (class, all_polls) = fill_voting::<T, ()>();
		let polls = &all_polls[&class];

		let vote = Vote { aye: true, conviction: Conviction::Locked1x };
		let balance: BalanceOf<T, ()> = 10u32.into();
		let dummy_vote = AccountVote::Standard { vote, balance };

		// We need to create existing votes
		for i in polls.iter() {
			ConvictionVoting::<T>::vote(RawOrigin::Signed(account.clone()).into(), *i, dummy_vote)
				.unwrap();
		}

		let call = IConvictionVoting::IConvictionVotingCalls::voteSplitAbstain(
			IConvictionVoting::voteSplitAbstainCall {
				referendumIndex: 0u32,
				ayeAmount: 10u128,
				nayAmount: 10u128,
				abstainAmount: 10u128,
			},
		);

		let mut call_setup = CallSetup::<T>::default();
		let (mut ext, _) = call_setup.ext();

		let result;
		#[block]
		{
			result = run_precompile::<ConvictionVotingPrecompile<T>, _>(
				&mut ext,
				H160::from_low_u64_be(0xC0000).as_fixed_bytes(),
				&call,
			);
		}

		assert!(result.is_ok());
	}

	#[benchmark(pov_mode = Measured)]
	fn delegate(r: Linear<0, { T::MaxVotes::get().min(T::Polls::max_ongoing().1) }>) {
		let all_polls = fill_voting::<T, ()>().1;
		let class = T::Polls::max_ongoing().0;
		let polls = &all_polls[&class];
		let voter = funded_mapped_account::<T, ()>("voter", 0);

		let vote = Vote { aye: true, conviction: Conviction::Locked1x };
		let balance: BalanceOf<T, ()> = 10u32.into();
		let delegate_vote = AccountVote::Standard { vote, balance };

		// We need to create existing delegations
		for i in polls.iter().take(r as usize) {
			ConvictionVoting::<T, ()>::vote(
				RawOrigin::Signed(voter.clone()).into(),
				*i,
				delegate_vote,
			)
			.unwrap();
		}

		let track_id: u16 = class.clone().try_into().ok().unwrap();

		let call =
			IConvictionVoting::IConvictionVotingCalls::delegate(IConvictionVoting::delegateCall {
				trackId: track_id,
				to: T::AddressMapper::to_address(&voter).0.into(),
				conviction: IConvictionVoting::Conviction::Locked1x,
				balance: 100u128,
			});

		let mut call_setup = CallSetup::<T>::default();
		let (mut ext, _) = call_setup.ext();

		let result;
		#[block]
		{
			result = run_precompile::<ConvictionVotingPrecompile<T>, _>(
				&mut ext,
				H160::from_low_u64_be(0xC0000).as_fixed_bytes(),
				&call,
			);
		}

		assert!(result.is_ok());
	}

	#[benchmark(pov_mode = Measured)]
	fn undelegate(r: Linear<0, { T::MaxVotes::get().min(T::Polls::max_ongoing().1) }>) {
		let all_polls = fill_voting::<T, ()>().1;
		let class = T::Polls::max_ongoing().0;
		let polls = &all_polls[&class];
		let voter = funded_mapped_account::<T, ()>("voter", 0);
		let voter_lookup = T::Lookup::unlookup(voter.clone());
		let caller = funded_mapped_account::<T, ()>("caller", 0);

		let vote = Vote { aye: true, conviction: Conviction::Locked1x };
		let balance: BalanceOf<T, ()> = 10u32.into();
		let delegate_vote = AccountVote::Standard { vote, balance };

		ConvictionVoting::<T, ()>::delegate(
			RawOrigin::Signed(caller.clone()).into(),
			class.clone(),
			voter_lookup,
			Conviction::Locked1x,
			balance,
		)
		.unwrap();

		// We need to create existing delegations
		for i in polls.iter().take(r as usize) {
			ConvictionVoting::<T, ()>::vote(
				RawOrigin::Signed(voter.clone()).into(),
				*i,
				delegate_vote,
			)
			.unwrap();
		}

		let track_id: u16 = class.clone().try_into().ok().unwrap();

		let call = IConvictionVoting::IConvictionVotingCalls::undelegate(
			IConvictionVoting::undelegateCall { trackId: track_id },
		);

		let mut call_setup = CallSetup::<T>::default();
		call_setup.set_origin(ExecOrigin::<T>::Signed(caller));
		let (mut ext, _) = call_setup.ext();

		let result;
		#[block]
		{
			result = run_precompile::<ConvictionVotingPrecompile<T>, _>(
				&mut ext,
				H160::from_low_u64_be(0xC0000).as_fixed_bytes(),
				&call,
			);
		}

		assert!(result.is_ok());
	}

	#[benchmark(pov_mode = Measured)]
	fn get_voting(r: Linear<0, { T::MaxVotes::get().min(T::Polls::max_ongoing().1) }>) {
		let all_polls = fill_voting::<T, ()>().1;
		let class = T::Polls::max_ongoing().0;
		let polls = &all_polls[&class];

		let track_id: u16 = class.clone().try_into().ok().unwrap();
		let referendum_index = 0u32;
		let caller = funded_mapped_account::<T, ()>("caller", 0);

		let vote = Vote { aye: true, conviction: Conviction::Locked1x };
		let balance: BalanceOf<T, ()> = 10u32.into();
		let dummy_vote = AccountVote::Standard { vote, balance };

		// We need to create existing votes
		for i in polls.iter().take(r as usize) {
			ConvictionVoting::<T>::vote(RawOrigin::Signed(caller.clone()).into(), *i, dummy_vote)
				.unwrap();
		}

		let call = IConvictionVoting::IConvictionVotingCalls::getVoting(
			IConvictionVoting::getVotingCall {
				who: T::AddressMapper::to_address(&caller).0.into(),
				trackId: track_id,
				referendumIndex: referendum_index,
			},
		);

		let mut call_setup = CallSetup::<T>::default();
		let (mut ext, _) = call_setup.ext();

		let result;
		#[block]
		{
			result = run_precompile::<ConvictionVotingPrecompile<T>, _>(
				&mut ext,
				H160::from_low_u64_be(0xC0000).as_fixed_bytes(),
				&call,
			);
		}

		assert!(result.is_ok());
	}

	impl_benchmark_test_suite!(
		ConvictionVotingPrecompilesBenchmarks,
		crate::mock::new_test_ext(),
		crate::mock::Test
	);
}
