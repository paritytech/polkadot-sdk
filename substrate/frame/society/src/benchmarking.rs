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

//! Society pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::v2::*;
use frame_system::RawOrigin;

use alloc::vec;
use sp_runtime::traits::Bounded;

use crate::Pallet as Society;

fn set_block_number<T: Config<I>, I: 'static>(n: BlockNumberFor<T, I>) {
	<T as Config<I>>::BlockNumberProvider::set_block_number(n);
}

fn mock_balance_deposit<T: Config<I>, I: 'static>() -> BalanceOf<T, I> {
	T::Currency::minimum_balance().saturating_mul(1_000u32.into())
}

fn make_deposit<T: Config<I>, I: 'static>(who: &T::AccountId) -> BalanceOf<T, I> {
	let amount = mock_balance_deposit::<T, I>();
	let required = amount.saturating_add(T::Currency::minimum_balance());
	if T::Currency::free_balance(who) < required {
		T::Currency::make_free_balance_be(who, required);
	}
	T::Currency::reserve(who, amount).expect("Pre-funded account; qed");
	amount
}

fn make_bid<T: Config<I>, I: 'static>(
	who: &T::AccountId,
) -> BidKind<T::AccountId, BalanceOf<T, I>> {
	BidKind::Deposit(make_deposit::<T, I>(who))
}

fn fund_society<T: Config<I>, I: 'static>() {
	T::Currency::make_free_balance_be(
		&Society::<T, I>::account_id(),
		BalanceOf::<T, I>::max_value(),
	);
	Pot::<T, I>::put(&BalanceOf::<T, I>::max_value());
}

// Set up Society
fn setup_society<T: Config<I>, I: 'static>() -> Result<T::AccountId, &'static str> {
	let origin = T::FounderSetOrigin::try_successful_origin().map_err(|_| "No origin")?;
	let founder: T::AccountId = account("founder", 0, 0);
	let founder_lookup: <T::Lookup as StaticLookup>::Source = T::Lookup::unlookup(founder.clone());
	let max_members = 5u32;
	let max_intake = 3u32;
	let max_strikes = 3u32;
	Society::<T, I>::found_society(
		origin,
		founder_lookup,
		max_members,
		max_intake,
		max_strikes,
		mock_balance_deposit::<T, I>(),
		b"benchmarking-society".to_vec(),
	)?;
	T::Currency::make_free_balance_be(
		&Society::<T, I>::account_id(),
		T::Currency::minimum_balance(),
	);
	T::Currency::make_free_balance_be(&Society::<T, I>::payouts(), T::Currency::minimum_balance());
	Ok(founder)
}

fn setup_funded_society<T: Config<I>, I: 'static>() -> Result<T::AccountId, &'static str> {
	let founder = setup_society::<T, I>()?;
	fund_society::<T, I>();
	Ok(founder)
}

fn add_candidate<T: Config<I>, I: 'static>(
	name: &'static str,
	tally: Tally,
	skeptic_struck: bool,
) -> T::AccountId {
	let candidate: T::AccountId = account(name, 0, 0);
	let candidacy = Candidacy {
		round: RoundCount::<T, I>::get(),
		kind: make_bid::<T, I>(&candidate),
		bid: 0u32.into(),
		tally,
		skeptic_struck,
	};
	Candidates::<T, I>::insert(&candidate, &candidacy);
	candidate
}

fn increment_round<T: Config<I>, I: 'static>() {
	let mut round_count = RoundCount::<T, I>::get();
	round_count.saturating_inc();
	RoundCount::<T, I>::put(round_count);
}

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn bid() -> Result<(), BenchmarkError> {
		setup_society::<T, I>()?;
		let caller: T::AccountId = whitelisted_caller();
		T::Currency::make_free_balance_be(&caller, BalanceOf::<T, I>::max_value());

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), 10u32.into());

		let first_bid: Bid<T::AccountId, BalanceOf<T, I>> = Bid {
			who: caller.clone(),
			kind: BidKind::Deposit(mock_balance_deposit::<T, I>()),
			value: 10u32.into(),
		};
		assert_eq!(Bids::<T, I>::get(), vec![first_bid]);
		Ok(())
	}

	#[benchmark]
	fn unbid() -> Result<(), BenchmarkError> {
		setup_society::<T, I>()?;
		let caller: T::AccountId = whitelisted_caller();
		T::Currency::make_free_balance_be(&caller, BalanceOf::<T, I>::max_value());
		let mut bids = Bids::<T, I>::get();
		Society::<T, I>::insert_bid(&mut bids, &caller, 10u32.into(), make_bid::<T, I>(&caller));
		Bids::<T, I>::put(bids);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()));

		assert_eq!(Bids::<T, I>::get(), vec![]);
		Ok(())
	}

	#[benchmark]
	fn vouch() -> Result<(), BenchmarkError> {
		setup_society::<T, I>()?;
		let caller: T::AccountId = whitelisted_caller();
		let vouched: T::AccountId = account("vouched", 0, 0);
		T::Currency::make_free_balance_be(&caller, BalanceOf::<T, I>::max_value());
		let _ = Society::<T, I>::insert_member(&caller, 1u32.into());
		let vouched_lookup: <T::Lookup as StaticLookup>::Source =
			T::Lookup::unlookup(vouched.clone());

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), vouched_lookup, 0u32.into(), 0u32.into());

		let bids = Bids::<T, I>::get();
		let vouched_bid: Bid<T::AccountId, BalanceOf<T, I>> = Bid {
			who: vouched.clone(),
			kind: BidKind::Vouch(caller.clone(), 0u32.into()),
			value: 0u32.into(),
		};
		assert_eq!(bids, vec![vouched_bid]);
		Ok(())
	}

	#[benchmark]
	fn unvouch() -> Result<(), BenchmarkError> {
		setup_society::<T, I>()?;
		let caller: T::AccountId = whitelisted_caller();
		T::Currency::make_free_balance_be(&caller, BalanceOf::<T, I>::max_value());
		let mut bids = Bids::<T, I>::get();
		Society::<T, I>::insert_bid(
			&mut bids,
			&caller,
			10u32.into(),
			BidKind::Vouch(caller.clone(), 0u32.into()),
		);
		Bids::<T, I>::put(bids);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()));

		assert_eq!(Bids::<T, I>::get(), vec![]);
		Ok(())
	}

	#[benchmark]
	fn vote() -> Result<(), BenchmarkError> {
		setup_society::<T, I>()?;
		let caller: T::AccountId = whitelisted_caller();
		T::Currency::make_free_balance_be(&caller, BalanceOf::<T, I>::max_value());
		let _ = Society::<T, I>::insert_member(&caller, 1u32.into());
		let candidate = add_candidate::<T, I>("candidate", Default::default(), false);
		let candidate_lookup: <T::Lookup as StaticLookup>::Source =
			T::Lookup::unlookup(candidate.clone());

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), candidate_lookup, true);

		let maybe_vote: Vote = <Votes<T, I>>::get(candidate.clone(), caller).unwrap();
		assert_eq!(maybe_vote.approve, true);
		Ok(())
	}

	#[benchmark]
	fn defender_vote() -> Result<(), BenchmarkError> {
		setup_society::<T, I>()?;
		let caller: T::AccountId = whitelisted_caller();
		T::Currency::make_free_balance_be(&caller, BalanceOf::<T, I>::max_value());
		let _ = Society::<T, I>::insert_member(&caller, 1u32.into());
		let defender: T::AccountId = account("defender", 0, 0);
		Defending::<T, I>::put((defender, caller.clone(), Tally::default()));

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), false);

		let round = RoundCount::<T, I>::get();
		let skeptic_vote: Vote = DefenderVotes::<T, I>::get(round, &caller).unwrap();
		assert_eq!(skeptic_vote.approve, false);
		Ok(())
	}

	#[benchmark]
	fn payout() -> Result<(), BenchmarkError> {
		setup_funded_society::<T, I>()?;
		// Payee's account already exists and is a member.
		let caller: T::AccountId = whitelisted_caller();
		T::Currency::make_free_balance_be(&caller, mock_balance_deposit::<T, I>());
		let _ = Society::<T, I>::insert_member(&caller, 0u32.into());
		// Introduce payout.
		Society::<T, I>::bump_payout(&caller, 0u32.into(), 1u32.into());

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()));

		let record = Payouts::<T, I>::get(caller);
		assert!(record.payouts.is_empty());
		Ok(())
	}

	#[benchmark]
	fn waive_repay() -> Result<(), BenchmarkError> {
		setup_funded_society::<T, I>()?;
		let caller: T::AccountId = whitelisted_caller();
		T::Currency::make_free_balance_be(&caller, BalanceOf::<T, I>::max_value());
		let _ = Society::<T, I>::insert_member(&caller, 0u32.into());
		Society::<T, I>::bump_payout(&caller, 0u32.into(), 1u32.into());

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), 1u32.into());

		let record = Payouts::<T, I>::get(caller);
		assert!(record.payouts.is_empty());
		Ok(())
	}

	#[benchmark]
	fn found_society() -> Result<(), BenchmarkError> {
		let founder: T::AccountId = whitelisted_caller();
		let can_found = T::FounderSetOrigin::try_successful_origin().map_err(|_| "No origin")?;
		let founder_lookup: <T::Lookup as StaticLookup>::Source =
			T::Lookup::unlookup(founder.clone());

		#[extrinsic_call]
		_(
			can_found as T::RuntimeOrigin,
			founder_lookup,
			5,
			3,
			3,
			mock_balance_deposit::<T, I>(),
			b"benchmarking-society".to_vec(),
		);

		assert_eq!(Founder::<T, I>::get(), Some(founder.clone()));
		Ok(())
	}

	#[benchmark]
	fn dissolve() -> Result<(), BenchmarkError> {
		let founder = setup_society::<T, I>()?;
		let members_and_candidates = vec![("m1", "c1"), ("m2", "c2"), ("m3", "c3"), ("m4", "c4")];
		let members_count = members_and_candidates.clone().len() as u32;
		for (m, c) in members_and_candidates {
			let member: T::AccountId = account(m, 0, 0);
			let _ = Society::<T, I>::insert_member(&member, 100u32.into());
			let candidate = add_candidate::<T, I>(
				c,
				Tally { approvals: 1u32.into(), rejections: 1u32.into() },
				false,
			);
			let candidate_lookup: <T::Lookup as StaticLookup>::Source =
				T::Lookup::unlookup(candidate);
			let _ = Society::<T, I>::vote(RawOrigin::Signed(member).into(), candidate_lookup, true);
		}
		// Leaving only Founder member.
		MemberCount::<T, I>::mutate(|i| i.saturating_reduce(members_count));

		#[extrinsic_call]
		_(RawOrigin::Signed(founder));

		assert_eq!(Founder::<T, I>::get(), None);
		Ok(())
	}

	#[benchmark]
	fn judge_suspended_member() -> Result<(), BenchmarkError> {
		let founder = setup_society::<T, I>()?;
		let caller: T::AccountId = whitelisted_caller();
		let caller_lookup: <T::Lookup as StaticLookup>::Source =
			T::Lookup::unlookup(caller.clone());
		let _ = Society::<T, I>::insert_member(&caller, 0u32.into());
		let _ = Society::<T, I>::suspend_member(&caller);

		#[extrinsic_call]
		_(RawOrigin::Signed(founder), caller_lookup, false);

		assert_eq!(SuspendedMembers::<T, I>::contains_key(&caller), false);
		Ok(())
	}

	#[benchmark]
	fn set_parameters() -> Result<(), BenchmarkError> {
		let founder = setup_society::<T, I>()?;
		let max_members = 10u32;
		let max_intake = 10u32;
		let max_strikes = 10u32;
		let candidate_deposit: BalanceOf<T, I> = 10u32.into();
		let params = GroupParams { max_members, max_intake, max_strikes, candidate_deposit };

		#[extrinsic_call]
		_(RawOrigin::Signed(founder), max_members, max_intake, max_strikes, candidate_deposit);

		assert_eq!(Parameters::<T, I>::get(), Some(params));
		Ok(())
	}

	#[benchmark]
	fn punish_skeptic() -> Result<(), BenchmarkError> {
		setup_society::<T, I>()?;
		let candidate = add_candidate::<T, I>("candidate", Default::default(), false);
		let skeptic: T::AccountId = account("skeptic", 0, 0);
		let _ = Society::<T, I>::insert_member(&skeptic, 0u32.into());
		Skeptic::<T, I>::put(&skeptic);
		if let Period::Voting { more, .. } = Society::<T, I>::period() {
			set_block_number::<T, I>(T::BlockNumberProvider::current_block_number() + more)
		}

		#[extrinsic_call]
		_(RawOrigin::Signed(candidate.clone()));

		let candidacy = Candidates::<T, I>::get(&candidate).unwrap();
		assert_eq!(candidacy.skeptic_struck, true);
		Ok(())
	}

	#[benchmark]
	fn claim_membership() -> Result<(), BenchmarkError> {
		setup_society::<T, I>()?;
		let candidate = add_candidate::<T, I>(
			"candidate",
			Tally { approvals: 3u32.into(), rejections: 0u32.into() },
			false,
		);
		increment_round::<T, I>();

		#[extrinsic_call]
		_(RawOrigin::Signed(candidate.clone()));

		assert!(!Candidates::<T, I>::contains_key(&candidate));
		assert!(Members::<T, I>::contains_key(&candidate));
		Ok(())
	}

	#[benchmark]
	fn bestow_membership() -> Result<(), BenchmarkError> {
		let founder = setup_society::<T, I>()?;
		let candidate = add_candidate::<T, I>(
			"candidate",
			Tally { approvals: 3u32.into(), rejections: 1u32.into() },
			false,
		);
		increment_round::<T, I>();

		#[extrinsic_call]
		_(RawOrigin::Signed(founder), candidate.clone());

		assert!(!Candidates::<T, I>::contains_key(&candidate));
		assert!(Members::<T, I>::contains_key(&candidate));
		Ok(())
	}

	#[benchmark]
	fn kick_candidate() -> Result<(), BenchmarkError> {
		let founder = setup_society::<T, I>()?;
		let candidate = add_candidate::<T, I>(
			"candidate",
			Tally { approvals: 1u32.into(), rejections: 1u32.into() },
			false,
		);
		increment_round::<T, I>();

		#[extrinsic_call]
		_(RawOrigin::Signed(founder), candidate.clone());

		assert!(!Candidates::<T, I>::contains_key(&candidate));
		Ok(())
	}

	#[benchmark]
	fn resign_candidacy() -> Result<(), BenchmarkError> {
		setup_society::<T, I>()?;
		let candidate = add_candidate::<T, I>(
			"candidate",
			Tally { approvals: 0u32.into(), rejections: 0u32.into() },
			false,
		);

		#[extrinsic_call]
		_(RawOrigin::Signed(candidate.clone()));

		assert!(!Candidates::<T, I>::contains_key(&candidate));
		Ok(())
	}

	#[benchmark]
	fn drop_candidate() -> Result<(), BenchmarkError> {
		setup_society::<T, I>()?;
		let candidate = add_candidate::<T, I>(
			"candidate",
			Tally { approvals: 0u32.into(), rejections: 3u32.into() },
			false,
		);
		let caller: T::AccountId = whitelisted_caller();
		let _ = Society::<T, I>::insert_member(&caller, 0u32.into());
		let mut round_count = RoundCount::<T, I>::get();
		round_count = round_count.saturating_add(2u32);
		RoundCount::<T, I>::put(round_count);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), candidate.clone());

		assert!(!Candidates::<T, I>::contains_key(&candidate));
		Ok(())
	}

	#[benchmark]
	fn cleanup_candidacy() -> Result<(), BenchmarkError> {
		setup_society::<T, I>()?;
		let candidate = add_candidate::<T, I>(
			"candidate",
			Tally { approvals: 0u32.into(), rejections: 0u32.into() },
			false,
		);
		let member_one: T::AccountId = account("one", 0, 0);
		let member_two: T::AccountId = account("two", 0, 0);
		let _ = Society::<T, I>::insert_member(&member_one, 0u32.into());
		let _ = Society::<T, I>::insert_member(&member_two, 0u32.into());
		let candidate_lookup: <T::Lookup as StaticLookup>::Source =
			T::Lookup::unlookup(candidate.clone());
		let _ = Society::<T, I>::vote(
			RawOrigin::Signed(member_one.clone()).into(),
			candidate_lookup.clone(),
			true,
		);
		let _ = Society::<T, I>::vote(
			RawOrigin::Signed(member_two.clone()).into(),
			candidate_lookup,
			true,
		);
		Candidates::<T, I>::remove(&candidate);

		#[extrinsic_call]
		_(RawOrigin::Signed(member_one), candidate.clone(), 5);

		assert_eq!(Votes::<T, I>::get(&candidate, &member_two), None);
		Ok(())
	}

	#[benchmark]
	fn cleanup_challenge() -> Result<(), BenchmarkError> {
		setup_society::<T, I>()?;
		ChallengeRoundCount::<T, I>::put(1u32);
		let member: T::AccountId = whitelisted_caller();
		let _ = Society::<T, I>::insert_member(&member, 0u32.into());
		let defender: T::AccountId = account("defender", 0, 0);
		Defending::<T, I>::put((defender.clone(), member.clone(), Tally::default()));
		let _ = Society::<T, I>::defender_vote(RawOrigin::Signed(member.clone()).into(), true);
		ChallengeRoundCount::<T, I>::put(2u32);
		let mut challenge_round = ChallengeRoundCount::<T, I>::get();
		challenge_round = challenge_round.saturating_sub(1u32);

		#[extrinsic_call]
		_(RawOrigin::Signed(member.clone()), challenge_round, 1u32);

		assert_eq!(DefenderVotes::<T, I>::get(challenge_round, &defender), None);
		Ok(())
	}

	#[benchmark]
	fn poke_deposit() -> Result<(), BenchmarkError> {
		// Set up society
		setup_society::<T, I>()?;
		let bidder: T::AccountId = whitelisted_caller();
		T::Currency::make_free_balance_be(&bidder, BalanceOf::<T, I>::max_value());

		// Make initial bid
		let initial_deposit = mock_balance_deposit::<T, I>();
		Society::<T, I>::bid(RawOrigin::Signed(bidder.clone()).into(), 0u32.into())?;

		// Verify initial state
		assert_eq!(T::Currency::reserved_balance(&bidder), initial_deposit);
		let bids = Bids::<T, I>::get();
		let existing_bid = bids.iter().find(|b| b.who == bidder).expect("Bid should exist");
		assert_eq!(existing_bid.kind, BidKind::Deposit(initial_deposit));

		// Artificially increase deposit in storage and reserve extra balance
		let extra_amount = 2u32.into();
		let increased_deposit = initial_deposit.saturating_add(extra_amount);
		Bids::<T, I>::try_mutate(|bids| -> Result<(), BenchmarkError> {
			if let Some(existing_bid) = bids.iter_mut().find(|b| b.who == bidder) {
				existing_bid.kind = BidKind::Deposit(increased_deposit);
				Ok(())
			} else {
				Err(BenchmarkError::Stop("Bid not found"))
			}
		})?;
		T::Currency::reserve(&bidder, extra_amount)?;

		// Verify increased state
		assert_eq!(T::Currency::reserved_balance(&bidder), increased_deposit);
		let bids = Bids::<T, I>::get();
		let existing_bid = bids.iter().find(|b| b.who == bidder).expect("Bid should exist");
		assert_eq!(existing_bid.kind, BidKind::Deposit(increased_deposit));

		#[extrinsic_call]
		_(RawOrigin::Signed(bidder.clone()));

		// Verify final state returned to initial deposit
		assert_eq!(T::Currency::reserved_balance(&bidder), initial_deposit);
		let bids = Bids::<T, I>::get();
		let existing_bid = bids.iter().find(|b| b.who == bidder).expect("Bid should exist");
		assert_eq!(existing_bid.kind, BidKind::Deposit(initial_deposit));

		Ok(())
	}

	impl_benchmark_test_suite!(
		Society,
		sp_io::TestExternalities::from(
			<frame_system::GenesisConfig::<crate::mock::Test> as sp_runtime::BuildStorage>::build_storage(
				&frame_system::GenesisConfig::default()).unwrap()
			),
		crate::mock::Test
	);
}
