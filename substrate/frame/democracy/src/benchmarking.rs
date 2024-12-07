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

//! Democracy pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::v2::*;
use frame_support::{
	assert_noop, assert_ok,
	traits::{Currency, EnsureOrigin, Get, OnInitialize, UnfilteredDispatchable},
};
use frame_system::{pallet_prelude::BlockNumberFor, RawOrigin};
use sp_runtime::{traits::Bounded, BoundedVec};

use crate::Pallet as Democracy;

const REFERENDUM_COUNT_HINT: u32 = 10;
const SEED: u32 = 0;

fn funded_account<T: Config>(name: &'static str, index: u32) -> T::AccountId {
	let caller: T::AccountId = account(name, index, SEED);
	// Give the account half of the maximum value of the `Balance` type.
	// Otherwise some transfers will fail with an overflow error.
	T::Currency::make_free_balance_be(&caller, BalanceOf::<T>::max_value() / 2u32.into());
	caller
}

fn make_proposal<T: Config>(n: u32) -> BoundedCallOf<T> {
	let call: CallOf<T> = frame_system::Call::remark { remark: n.encode() }.into();
	<T as Config>::Preimages::bound(call).unwrap()
}

fn add_proposal<T: Config>(n: u32) -> Result<T::Hash, &'static str> {
	let other = funded_account::<T>("proposer", n);
	let value = T::MinimumDeposit::get();
	let proposal = make_proposal::<T>(n);
	Democracy::<T>::propose(RawOrigin::Signed(other).into(), proposal.clone(), value)?;
	Ok(proposal.hash())
}

// add a referendum with a metadata.
fn add_referendum<T: Config>(n: u32) -> (ReferendumIndex, T::Hash, T::Hash) {
	let vote_threshold = VoteThreshold::SimpleMajority;
	let proposal = make_proposal::<T>(n);
	let hash = proposal.hash();
	let index = Democracy::<T>::inject_referendum(
		T::LaunchPeriod::get(),
		proposal,
		vote_threshold,
		0u32.into(),
	);
	let preimage_hash = note_preimage::<T>();
	MetadataOf::<T>::insert(crate::MetadataOwner::Referendum(index), preimage_hash);
	(index, hash, preimage_hash)
}

fn account_vote<T: Config>(b: BalanceOf<T>) -> AccountVote<BalanceOf<T>> {
	let v = Vote { aye: true, conviction: Conviction::Locked1x };

	AccountVote::Standard { vote: v, balance: b }
}

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

fn assert_has_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_has_event(generic_event.into());
}

// note a new preimage.
fn note_preimage<T: Config>() -> T::Hash {
	use alloc::borrow::Cow;
	use core::sync::atomic::{AtomicU8, Ordering};
	// note a new preimage on every function invoke.
	static COUNTER: AtomicU8 = AtomicU8::new(0);
	let data = Cow::from(vec![COUNTER.fetch_add(1, Ordering::Relaxed)]);
	let hash = <T as Config>::Preimages::note(data).unwrap();
	hash
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn propose() -> Result<(), BenchmarkError> {
		let p = T::MaxProposals::get();

		for i in 0..(p - 1) {
			add_proposal::<T>(i)?;
		}

		let caller = funded_account::<T>("caller", 0);
		let proposal = make_proposal::<T>(0);
		let value = T::MinimumDeposit::get();
		whitelist_account!(caller);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), proposal, value);

		assert_eq!(PublicProps::<T>::get().len(), p as usize, "Proposals not created.");
		Ok(())
	}

	#[benchmark]
	fn second() -> Result<(), BenchmarkError> {
		let caller = funded_account::<T>("caller", 0);
		add_proposal::<T>(0)?;

		// Create s existing "seconds"
		// we must reserve one deposit for the `proposal` and one for our benchmarked `second` call.
		for i in 0..T::MaxDeposits::get() - 2 {
			let seconder = funded_account::<T>("seconder", i);
			Democracy::<T>::second(RawOrigin::Signed(seconder).into(), 0)?;
		}

		let deposits = DepositOf::<T>::get(0).ok_or("Proposal not created")?;
		assert_eq!(deposits.0.len(), (T::MaxDeposits::get() - 1) as usize, "Seconds not recorded");
		whitelist_account!(caller);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), 0);

		let deposits = DepositOf::<T>::get(0).ok_or("Proposal not created")?;
		assert_eq!(
			deposits.0.len(),
			(T::MaxDeposits::get()) as usize,
			"`second` benchmark did not work"
		);
		Ok(())
	}

	#[benchmark]
	fn vote_new() -> Result<(), BenchmarkError> {
		let caller = funded_account::<T>("caller", 0);
		let account_vote = account_vote::<T>(100u32.into());

		// We need to create existing direct votes
		for i in 0..T::MaxVotes::get() - 1 {
			let ref_index = add_referendum::<T>(i).0;
			Democracy::<T>::vote(
				RawOrigin::Signed(caller.clone()).into(),
				ref_index,
				account_vote,
			)?;
		}
		let votes = match VotingOf::<T>::get(&caller) {
			Voting::Direct { votes, .. } => votes,
			_ => return Err("Votes are not direct".into()),
		};
		assert_eq!(votes.len(), (T::MaxVotes::get() - 1) as usize, "Votes were not recorded.");

		let ref_index = add_referendum::<T>(T::MaxVotes::get() - 1).0;
		whitelist_account!(caller);

		#[extrinsic_call]
		vote(RawOrigin::Signed(caller.clone()), ref_index, account_vote);

		let votes = match VotingOf::<T>::get(&caller) {
			Voting::Direct { votes, .. } => votes,
			_ => return Err("Votes are not direct".into()),
		};

		assert_eq!(votes.len(), T::MaxVotes::get() as usize, "Vote was not recorded.");
		Ok(())
	}

	#[benchmark]
	fn vote_existing() -> Result<(), BenchmarkError> {
		let caller = funded_account::<T>("caller", 0);
		let account_vote = account_vote::<T>(100u32.into());

		// We need to create existing direct votes
		for i in 0..T::MaxVotes::get() {
			let ref_index = add_referendum::<T>(i).0;
			Democracy::<T>::vote(
				RawOrigin::Signed(caller.clone()).into(),
				ref_index,
				account_vote,
			)?;
		}
		let votes = match VotingOf::<T>::get(&caller) {
			Voting::Direct { votes, .. } => votes,
			_ => return Err("Votes are not direct".into()),
		};
		assert_eq!(votes.len(), T::MaxVotes::get() as usize, "Votes were not recorded.");

		// Change vote from aye to nay
		let nay = Vote { aye: false, conviction: Conviction::Locked1x };
		let new_vote = AccountVote::Standard { vote: nay, balance: 1000u32.into() };
		let ref_index = ReferendumCount::<T>::get() - 1;

		// This tests when a user changes a vote
		whitelist_account!(caller);

		#[extrinsic_call]
		vote(RawOrigin::Signed(caller.clone()), ref_index, new_vote);

		let votes = match VotingOf::<T>::get(&caller) {
			Voting::Direct { votes, .. } => votes,
			_ => return Err("Votes are not direct".into()),
		};
		assert_eq!(votes.len(), T::MaxVotes::get() as usize, "Vote was incorrectly added");
		let referendum_info =
			ReferendumInfoOf::<T>::get(ref_index).ok_or("referendum doesn't exist")?;
		let tally = match referendum_info {
			ReferendumInfo::Ongoing(r) => r.tally,
			_ => return Err("referendum not ongoing".into()),
		};
		assert_eq!(tally.nays, 1000u32.into(), "changed vote was not recorded");
		Ok(())
	}

	#[benchmark]
	fn emergency_cancel() -> Result<(), BenchmarkError> {
		let origin = T::CancellationOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;
		let (ref_index, _, preimage_hash) = add_referendum::<T>(0);
		assert_ok!(Democracy::<T>::referendum_status(ref_index));

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, ref_index);
		// Referendum has been canceled
		assert_noop!(Democracy::<T>::referendum_status(ref_index), Error::<T>::ReferendumInvalid,);
		assert_last_event::<T>(
			crate::Event::MetadataCleared {
				owner: MetadataOwner::Referendum(ref_index),
				hash: preimage_hash,
			}
			.into(),
		);
		Ok(())
	}

	#[benchmark]
	fn blacklist() -> Result<(), BenchmarkError> {
		// Place our proposal at the end to make sure it's worst case.
		for i in 0..T::MaxProposals::get() - 1 {
			add_proposal::<T>(i)?;
		}
		// We should really add a lot of seconds here, but we're not doing it elsewhere.

		// Add a referendum of our proposal.
		let (ref_index, hash, preimage_hash) = add_referendum::<T>(0);
		assert_ok!(Democracy::<T>::referendum_status(ref_index));
		// Place our proposal in the external queue, too.
		assert_ok!(Democracy::<T>::external_propose(
			T::ExternalOrigin::try_successful_origin()
				.expect("ExternalOrigin has no successful origin required for the benchmark"),
			make_proposal::<T>(0)
		));
		let origin =
			T::BlacklistOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, hash, Some(ref_index));

		// Referendum has been canceled
		assert_noop!(Democracy::<T>::referendum_status(ref_index), Error::<T>::ReferendumInvalid);
		assert_has_event::<T>(
			crate::Event::MetadataCleared {
				owner: MetadataOwner::Referendum(ref_index),
				hash: preimage_hash,
			}
			.into(),
		);
		Ok(())
	}

	// Worst case scenario, we external propose a previously blacklisted proposal
	#[benchmark]
	fn external_propose() -> Result<(), BenchmarkError> {
		let origin =
			T::ExternalOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let proposal = make_proposal::<T>(0);
		// Add proposal to blacklist with block number 0

		let addresses: BoundedVec<_, _> = (0..(T::MaxBlacklisted::get() - 1))
			.into_iter()
			.map(|i| account::<T::AccountId>("blacklist", i, SEED))
			.collect::<Vec<_>>()
			.try_into()
			.unwrap();
		Blacklist::<T>::insert(proposal.hash(), (BlockNumberFor::<T>::zero(), addresses));
		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, proposal);

		// External proposal created
		ensure!(NextExternal::<T>::exists(), "External proposal didn't work");
		Ok(())
	}

	#[benchmark]
	fn external_propose_majority() -> Result<(), BenchmarkError> {
		let origin = T::ExternalMajorityOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;
		let proposal = make_proposal::<T>(0);
		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, proposal);

		// External proposal created
		ensure!(NextExternal::<T>::exists(), "External proposal didn't work");
		Ok(())
	}

	#[benchmark]
	fn external_propose_default() -> Result<(), BenchmarkError> {
		let origin = T::ExternalDefaultOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;
		let proposal = make_proposal::<T>(0);
		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, proposal);

		// External proposal created
		ensure!(NextExternal::<T>::exists(), "External proposal didn't work");
		Ok(())
	}

	#[benchmark]
	fn fast_track() -> Result<(), BenchmarkError> {
		let origin_propose = T::ExternalDefaultOrigin::try_successful_origin()
			.expect("ExternalDefaultOrigin has no successful origin required for the benchmark");
		let proposal = make_proposal::<T>(0);
		let proposal_hash = proposal.hash();
		Democracy::<T>::external_propose_default(origin_propose.clone(), proposal)?;
		// Set metadata to the external proposal.
		let preimage_hash = note_preimage::<T>();
		assert_ok!(Democracy::<T>::set_metadata(
			origin_propose,
			MetadataOwner::External,
			Some(preimage_hash)
		));
		// NOTE: Instant origin may invoke a little bit more logic, but may not always succeed.
		let origin_fast_track =
			T::FastTrackOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let voting_period = T::FastTrackVotingPeriod::get();
		let delay = 0u32;
		#[extrinsic_call]
		_(origin_fast_track as T::RuntimeOrigin, proposal_hash, voting_period, delay.into());

		assert_eq!(ReferendumCount::<T>::get(), 1, "referendum not created");
		assert_last_event::<T>(
			crate::Event::MetadataTransferred {
				prev_owner: MetadataOwner::External,
				owner: MetadataOwner::Referendum(0),
				hash: preimage_hash,
			}
			.into(),
		);
		Ok(())
	}

	#[benchmark]
	fn veto_external() -> Result<(), BenchmarkError> {
		let proposal = make_proposal::<T>(0);
		let proposal_hash = proposal.hash();

		let origin_propose = T::ExternalDefaultOrigin::try_successful_origin()
			.expect("ExternalDefaultOrigin has no successful origin required for the benchmark");
		Democracy::<T>::external_propose_default(origin_propose.clone(), proposal)?;

		let preimage_hash = note_preimage::<T>();
		assert_ok!(Democracy::<T>::set_metadata(
			origin_propose,
			MetadataOwner::External,
			Some(preimage_hash)
		));

		let mut vetoers: BoundedVec<T::AccountId, _> = Default::default();
		for i in 0..(T::MaxBlacklisted::get() - 1) {
			vetoers.try_push(account::<T::AccountId>("vetoer", i, SEED)).unwrap();
		}
		vetoers.sort();
		Blacklist::<T>::insert(proposal_hash, (BlockNumberFor::<T>::zero(), vetoers));

		let origin =
			T::VetoOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		ensure!(NextExternal::<T>::get().is_some(), "no external proposal");
		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, proposal_hash);

		assert!(NextExternal::<T>::get().is_none());
		let (_, new_vetoers) = Blacklist::<T>::get(&proposal_hash).ok_or("no blacklist")?;
		assert_eq!(new_vetoers.len(), T::MaxBlacklisted::get() as usize, "vetoers not added");
		Ok(())
	}

	#[benchmark]
	fn cancel_proposal() -> Result<(), BenchmarkError> {
		// Place our proposal at the end to make sure it's worst case.
		for i in 0..T::MaxProposals::get() {
			add_proposal::<T>(i)?;
		}
		// Add metadata to the first proposal.
		let proposer = funded_account::<T>("proposer", 0);
		let preimage_hash = note_preimage::<T>();
		assert_ok!(Democracy::<T>::set_metadata(
			RawOrigin::Signed(proposer).into(),
			MetadataOwner::Proposal(0),
			Some(preimage_hash)
		));
		let cancel_origin = T::CancelProposalOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;
		#[extrinsic_call]
		_(cancel_origin as T::RuntimeOrigin, 0);

		assert_last_event::<T>(
			crate::Event::MetadataCleared {
				owner: MetadataOwner::Proposal(0),
				hash: preimage_hash,
			}
			.into(),
		);
		Ok(())
	}

	#[benchmark]
	fn cancel_referendum() -> Result<(), BenchmarkError> {
		let (ref_index, _, preimage_hash) = add_referendum::<T>(0);
		#[extrinsic_call]
		_(RawOrigin::Root, ref_index);

		assert_last_event::<T>(
			crate::Event::MetadataCleared {
				owner: MetadataOwner::Referendum(0),
				hash: preimage_hash,
			}
			.into(),
		);
		Ok(())
	}

	#[benchmark(extra)]
	fn on_initialize_external(r: Linear<0, REFERENDUM_COUNT_HINT>) -> Result<(), BenchmarkError> {
		for i in 0..r {
			add_referendum::<T>(i);
		}

		assert_eq!(ReferendumCount::<T>::get(), r, "referenda not created");

		// Launch external
		LastTabledWasExternal::<T>::put(false);

		let origin = T::ExternalMajorityOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;
		let proposal = make_proposal::<T>(r);
		let call = Call::<T>::external_propose_majority { proposal };
		call.dispatch_bypass_filter(origin)?;
		// External proposal created
		ensure!(NextExternal::<T>::exists(), "External proposal didn't work");

		let block_number = T::LaunchPeriod::get();

		#[block]
		{
			Democracy::<T>::on_initialize(block_number);
		}

		// One extra because of next external
		assert_eq!(ReferendumCount::<T>::get(), r + 1, "referenda not created");
		ensure!(!NextExternal::<T>::exists(), "External wasn't taken");

		// All but the new next external should be finished
		for i in 0..r {
			if let Some(value) = ReferendumInfoOf::<T>::get(i) {
				match value {
					ReferendumInfo::Finished { .. } => (),
					ReferendumInfo::Ongoing(_) => return Err("Referendum was not finished".into()),
				}
			}
		}
		Ok(())
	}

	#[benchmark(extra)]
	fn on_initialize_public(
		r: Linear<0, { T::MaxVotes::get() - 1 }>,
	) -> Result<(), BenchmarkError> {
		for i in 0..r {
			add_referendum::<T>(i);
		}

		assert_eq!(ReferendumCount::<T>::get(), r, "referenda not created");

		// Launch public
		assert!(add_proposal::<T>(r).is_ok(), "proposal not created");
		LastTabledWasExternal::<T>::put(true);

		let block_number = T::LaunchPeriod::get();

		#[block]
		{
			Democracy::<T>::on_initialize(block_number);
		}

		// One extra because of next public
		assert_eq!(ReferendumCount::<T>::get(), r + 1, "proposal not accepted");

		// All should be finished
		for i in 0..r {
			if let Some(value) = ReferendumInfoOf::<T>::get(i) {
				match value {
					ReferendumInfo::Finished { .. } => (),
					ReferendumInfo::Ongoing(_) => return Err("Referendum was not finished".into()),
				}
			}
		}
		Ok(())
	}

	// No launch no maturing referenda.
	#[benchmark]
	fn on_initialize_base(r: Linear<0, { T::MaxVotes::get() - 1 }>) -> Result<(), BenchmarkError> {
		for i in 0..r {
			add_referendum::<T>(i);
		}

		for (key, mut info) in ReferendumInfoOf::<T>::iter() {
			if let ReferendumInfo::Ongoing(ref mut status) = info {
				status.end += 100u32.into();
			}
			ReferendumInfoOf::<T>::insert(key, info);
		}

		assert_eq!(ReferendumCount::<T>::get(), r, "referenda not created");
		assert_eq!(LowestUnbaked::<T>::get(), 0, "invalid referenda init");

		#[block]
		{
			Democracy::<T>::on_initialize(1u32.into());
		}

		// All should be on going
		for i in 0..r {
			if let Some(value) = ReferendumInfoOf::<T>::get(i) {
				match value {
					ReferendumInfo::Finished { .. } =>
						return Err("Referendum has been finished".into()),
					ReferendumInfo::Ongoing(_) => (),
				}
			}
		}
		Ok(())
	}

	#[benchmark]
	fn on_initialize_base_with_launch_period(
		r: Linear<0, { T::MaxVotes::get() - 1 }>,
	) -> Result<(), BenchmarkError> {
		for i in 0..r {
			add_referendum::<T>(i);
		}

		for (key, mut info) in ReferendumInfoOf::<T>::iter() {
			if let ReferendumInfo::Ongoing(ref mut status) = info {
				status.end += 100u32.into();
			}
			ReferendumInfoOf::<T>::insert(key, info);
		}

		assert_eq!(ReferendumCount::<T>::get(), r, "referenda not created");
		assert_eq!(LowestUnbaked::<T>::get(), 0, "invalid referenda init");

		let block_number = T::LaunchPeriod::get();

		#[block]
		{
			Democracy::<T>::on_initialize(block_number);
		}

		// All should be on going
		for i in 0..r {
			if let Some(value) = ReferendumInfoOf::<T>::get(i) {
				match value {
					ReferendumInfo::Finished { .. } =>
						return Err("Referendum has been finished".into()),
					ReferendumInfo::Ongoing(_) => (),
				}
			}
		}
		Ok(())
	}

	#[benchmark]
	fn delegate(r: Linear<0, { T::MaxVotes::get() - 1 }>) -> Result<(), BenchmarkError> {
		let initial_balance: BalanceOf<T> = 100u32.into();
		let delegated_balance: BalanceOf<T> = 1000u32.into();

		let caller = funded_account::<T>("caller", 0);
		// Caller will initially delegate to `old_delegate`
		let old_delegate: T::AccountId = funded_account::<T>("old_delegate", r);
		let old_delegate_lookup = T::Lookup::unlookup(old_delegate.clone());
		Democracy::<T>::delegate(
			RawOrigin::Signed(caller.clone()).into(),
			old_delegate_lookup,
			Conviction::Locked1x,
			delegated_balance,
		)?;
		let (target, balance) = match VotingOf::<T>::get(&caller) {
			Voting::Delegating { target, balance, .. } => (target, balance),
			_ => return Err("Votes are not direct".into()),
		};
		assert_eq!(target, old_delegate, "delegation target didn't work");
		assert_eq!(balance, delegated_balance, "delegation balance didn't work");
		// Caller will now switch to `new_delegate`
		let new_delegate: T::AccountId = funded_account::<T>("new_delegate", r);
		let new_delegate_lookup = T::Lookup::unlookup(new_delegate.clone());
		let account_vote = account_vote::<T>(initial_balance);
		// We need to create existing direct votes for the `new_delegate`
		for i in 0..r {
			let ref_index = add_referendum::<T>(i).0;
			Democracy::<T>::vote(
				RawOrigin::Signed(new_delegate.clone()).into(),
				ref_index,
				account_vote,
			)?;
		}
		let votes = match VotingOf::<T>::get(&new_delegate) {
			Voting::Direct { votes, .. } => votes,
			_ => return Err("Votes are not direct".into()),
		};
		assert_eq!(votes.len(), r as usize, "Votes were not recorded.");
		whitelist_account!(caller);

		#[extrinsic_call]
		_(
			RawOrigin::Signed(caller.clone()),
			new_delegate_lookup,
			Conviction::Locked1x,
			delegated_balance,
		);

		let (target, balance) = match VotingOf::<T>::get(&caller) {
			Voting::Delegating { target, balance, .. } => (target, balance),
			_ => return Err("Votes are not direct".into()),
		};
		assert_eq!(target, new_delegate, "delegation target didn't work");
		assert_eq!(balance, delegated_balance, "delegation balance didn't work");
		let delegations = match VotingOf::<T>::get(&new_delegate) {
			Voting::Direct { delegations, .. } => delegations,
			_ => return Err("Votes are not direct".into()),
		};
		assert_eq!(delegations.capital, delegated_balance, "delegation was not recorded.");
		Ok(())
	}

	#[benchmark]
	fn undelegate(r: Linear<0, { T::MaxVotes::get() - 1 }>) -> Result<(), BenchmarkError> {
		let initial_balance: BalanceOf<T> = 100u32.into();
		let delegated_balance: BalanceOf<T> = 1000u32.into();

		let caller = funded_account::<T>("caller", 0);
		// Caller will delegate
		let the_delegate: T::AccountId = funded_account::<T>("delegate", r);
		let the_delegate_lookup = T::Lookup::unlookup(the_delegate.clone());
		Democracy::<T>::delegate(
			RawOrigin::Signed(caller.clone()).into(),
			the_delegate_lookup,
			Conviction::Locked1x,
			delegated_balance,
		)?;
		let (target, balance) = match VotingOf::<T>::get(&caller) {
			Voting::Delegating { target, balance, .. } => (target, balance),
			_ => return Err("Votes are not direct".into()),
		};
		assert_eq!(target, the_delegate, "delegation target didn't work");
		assert_eq!(balance, delegated_balance, "delegation balance didn't work");
		// We need to create votes direct votes for the `delegate`
		let account_vote = account_vote::<T>(initial_balance);
		for i in 0..r {
			let ref_index = add_referendum::<T>(i).0;
			Democracy::<T>::vote(
				RawOrigin::Signed(the_delegate.clone()).into(),
				ref_index,
				account_vote,
			)?;
		}
		let votes = match VotingOf::<T>::get(&the_delegate) {
			Voting::Direct { votes, .. } => votes,
			_ => return Err("Votes are not direct".into()),
		};
		assert_eq!(votes.len(), r as usize, "Votes were not recorded.");
		whitelist_account!(caller);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()));

		// Voting should now be direct
		match VotingOf::<T>::get(&caller) {
			Voting::Direct { .. } => (),
			_ => return Err("undelegation failed".into()),
		}
		Ok(())
	}

	#[benchmark]
	fn clear_public_proposals() -> Result<(), BenchmarkError> {
		add_proposal::<T>(0)?;

		#[extrinsic_call]
		_(RawOrigin::Root);

		Ok(())
	}

	// Test when unlock will remove locks
	#[benchmark]
	fn unlock_remove(r: Linear<0, { T::MaxVotes::get() - 1 }>) -> Result<(), BenchmarkError> {
		let locker = funded_account::<T>("locker", 0);
		let locker_lookup = T::Lookup::unlookup(locker.clone());
		// Populate votes so things are locked
		let base_balance: BalanceOf<T> = 100u32.into();
		let small_vote = account_vote::<T>(base_balance);
		// Vote and immediately unvote
		for i in 0..r {
			let ref_index = add_referendum::<T>(i).0;
			Democracy::<T>::vote(RawOrigin::Signed(locker.clone()).into(), ref_index, small_vote)?;
			Democracy::<T>::remove_vote(RawOrigin::Signed(locker.clone()).into(), ref_index)?;
		}

		let caller = funded_account::<T>("caller", 0);
		whitelist_account!(caller);

		#[extrinsic_call]
		unlock(RawOrigin::Signed(caller), locker_lookup);

		// Note that we may want to add a `get_lock` api to actually verify
		let voting = VotingOf::<T>::get(&locker);
		assert_eq!(voting.locked_balance(), BalanceOf::<T>::zero());
		Ok(())
	}

	// Test when unlock will set a new value
	#[benchmark]
	fn unlock_set(r: Linear<0, { T::MaxVotes::get() - 1 }>) -> Result<(), BenchmarkError> {
		let locker = funded_account::<T>("locker", 0);
		let locker_lookup = T::Lookup::unlookup(locker.clone());
		// Populate votes so things are locked
		let base_balance: BalanceOf<T> = 100u32.into();
		let small_vote = account_vote::<T>(base_balance);
		for i in 0..r {
			let ref_index = add_referendum::<T>(i).0;
			Democracy::<T>::vote(RawOrigin::Signed(locker.clone()).into(), ref_index, small_vote)?;
		}

		// Create a big vote so lock increases
		let big_vote = account_vote::<T>(base_balance * 10u32.into());
		let ref_index = add_referendum::<T>(r).0;
		Democracy::<T>::vote(RawOrigin::Signed(locker.clone()).into(), ref_index, big_vote)?;

		let votes = match VotingOf::<T>::get(&locker) {
			Voting::Direct { votes, .. } => votes,
			_ => return Err("Votes are not direct".into()),
		};
		assert_eq!(votes.len(), (r + 1) as usize, "Votes were not recorded.");

		let voting = VotingOf::<T>::get(&locker);
		assert_eq!(voting.locked_balance(), base_balance * 10u32.into());

		Democracy::<T>::remove_vote(RawOrigin::Signed(locker.clone()).into(), ref_index)?;

		let caller = funded_account::<T>("caller", 0);
		whitelist_account!(caller);

		#[extrinsic_call]
		unlock(RawOrigin::Signed(caller), locker_lookup);

		let votes = match VotingOf::<T>::get(&locker) {
			Voting::Direct { votes, .. } => votes,
			_ => return Err("Votes are not direct".into()),
		};
		assert_eq!(votes.len(), r as usize, "Vote was not removed");

		let voting = VotingOf::<T>::get(&locker);
		// Note that we may want to add a `get_lock` api to actually verify
		assert_eq!(voting.locked_balance(), if r > 0 { base_balance } else { 0u32.into() });
		Ok(())
	}

	#[benchmark]
	fn remove_vote(r: Linear<1, { T::MaxVotes::get() }>) -> Result<(), BenchmarkError> {
		let caller = funded_account::<T>("caller", 0);
		let account_vote = account_vote::<T>(100u32.into());

		for i in 0..r {
			let ref_index = add_referendum::<T>(i).0;
			Democracy::<T>::vote(
				RawOrigin::Signed(caller.clone()).into(),
				ref_index,
				account_vote,
			)?;
		}

		let votes = match VotingOf::<T>::get(&caller) {
			Voting::Direct { votes, .. } => votes,
			_ => return Err("Votes are not direct".into()),
		};
		assert_eq!(votes.len(), r as usize, "Votes not created");

		let ref_index = r - 1;
		whitelist_account!(caller);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), ref_index);

		let votes = match VotingOf::<T>::get(&caller) {
			Voting::Direct { votes, .. } => votes,
			_ => return Err("Votes are not direct".into()),
		};
		assert_eq!(votes.len(), (r - 1) as usize, "Vote was not removed");
		Ok(())
	}

	// Worst case is when target == caller and referendum is ongoing
	#[benchmark]
	fn remove_other_vote(r: Linear<1, { T::MaxVotes::get() }>) -> Result<(), BenchmarkError> {
		let caller = funded_account::<T>("caller", r);
		let caller_lookup = T::Lookup::unlookup(caller.clone());
		let account_vote = account_vote::<T>(100u32.into());

		for i in 0..r {
			let ref_index = add_referendum::<T>(i).0;
			Democracy::<T>::vote(
				RawOrigin::Signed(caller.clone()).into(),
				ref_index,
				account_vote,
			)?;
		}

		let votes = match VotingOf::<T>::get(&caller) {
			Voting::Direct { votes, .. } => votes,
			_ => return Err("Votes are not direct".into()),
		};
		assert_eq!(votes.len(), r as usize, "Votes not created");

		let ref_index = r - 1;
		whitelist_account!(caller);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), caller_lookup, ref_index);

		let votes = match VotingOf::<T>::get(&caller) {
			Voting::Direct { votes, .. } => votes,
			_ => return Err("Votes are not direct".into()),
		};
		assert_eq!(votes.len(), (r - 1) as usize, "Vote was not removed");
		Ok(())
	}

	#[benchmark]
	fn set_external_metadata() -> Result<(), BenchmarkError> {
		let origin = T::ExternalOrigin::try_successful_origin()
			.expect("ExternalOrigin has no successful origin required for the benchmark");
		assert_ok!(Democracy::<T>::external_propose(origin.clone(), make_proposal::<T>(0)));
		let owner = MetadataOwner::External;
		let hash = note_preimage::<T>();

		#[extrinsic_call]
		set_metadata(origin as T::RuntimeOrigin, owner.clone(), Some(hash));

		assert_last_event::<T>(crate::Event::MetadataSet { owner, hash }.into());
		Ok(())
	}

	#[benchmark]
	fn clear_external_metadata() -> Result<(), BenchmarkError> {
		let origin = T::ExternalOrigin::try_successful_origin()
			.expect("ExternalOrigin has no successful origin required for the benchmark");
		assert_ok!(Democracy::<T>::external_propose(origin.clone(), make_proposal::<T>(0)));
		let owner = MetadataOwner::External;
		let _proposer = funded_account::<T>("proposer", 0);
		let hash = note_preimage::<T>();
		assert_ok!(Democracy::<T>::set_metadata(origin.clone(), owner.clone(), Some(hash)));

		#[extrinsic_call]
		set_metadata(origin as T::RuntimeOrigin, owner.clone(), None);

		assert_last_event::<T>(crate::Event::MetadataCleared { owner, hash }.into());
		Ok(())
	}

	#[benchmark]
	fn set_proposal_metadata() -> Result<(), BenchmarkError> {
		// Place our proposal at the end to make sure it's worst case.
		for i in 0..T::MaxProposals::get() {
			add_proposal::<T>(i)?;
		}
		let owner = MetadataOwner::Proposal(0);
		let proposer = funded_account::<T>("proposer", 0);
		let hash = note_preimage::<T>();

		#[extrinsic_call]
		set_metadata(RawOrigin::Signed(proposer), owner.clone(), Some(hash));

		assert_last_event::<T>(crate::Event::MetadataSet { owner, hash }.into());
		Ok(())
	}

	#[benchmark]
	fn clear_proposal_metadata() -> Result<(), BenchmarkError> {
		// Place our proposal at the end to make sure it's worst case.
		for i in 0..T::MaxProposals::get() {
			add_proposal::<T>(i)?;
		}
		let proposer = funded_account::<T>("proposer", 0);
		let owner = MetadataOwner::Proposal(0);
		let hash = note_preimage::<T>();
		assert_ok!(Democracy::<T>::set_metadata(
			RawOrigin::Signed(proposer.clone()).into(),
			owner.clone(),
			Some(hash)
		));

		#[extrinsic_call]
		set_metadata::<T::RuntimeOrigin>(RawOrigin::Signed(proposer), owner.clone(), None);

		assert_last_event::<T>(crate::Event::MetadataCleared { owner, hash }.into());
		Ok(())
	}

	#[benchmark]
	fn set_referendum_metadata() -> Result<(), BenchmarkError> {
		// create not ongoing referendum.
		ReferendumInfoOf::<T>::insert(
			0,
			ReferendumInfo::Finished { end: BlockNumberFor::<T>::zero(), approved: true },
		);
		let owner = MetadataOwner::Referendum(0);
		let _caller = funded_account::<T>("caller", 0);
		let hash = note_preimage::<T>();

		#[extrinsic_call]
		set_metadata::<T::RuntimeOrigin>(RawOrigin::Root, owner.clone(), Some(hash));

		assert_last_event::<T>(crate::Event::MetadataSet { owner, hash }.into());
		Ok(())
	}

	#[benchmark]
	fn clear_referendum_metadata() -> Result<(), BenchmarkError> {
		// create not ongoing referendum.
		ReferendumInfoOf::<T>::insert(
			0,
			ReferendumInfo::Finished { end: BlockNumberFor::<T>::zero(), approved: true },
		);
		let owner = MetadataOwner::Referendum(0);
		let hash = note_preimage::<T>();
		MetadataOf::<T>::insert(owner.clone(), hash);
		let caller = funded_account::<T>("caller", 0);

		#[extrinsic_call]
		set_metadata::<T::RuntimeOrigin>(RawOrigin::Signed(caller), owner.clone(), None);

		assert_last_event::<T>(crate::Event::MetadataCleared { owner, hash }.into());
		Ok(())
	}

	impl_benchmark_test_suite!(Democracy, crate::tests::new_test_ext(), crate::tests::Test);
}
