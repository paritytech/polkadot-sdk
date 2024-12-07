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
use crate::Pallet as Collective;

use core::mem::size_of;
use sp_runtime::traits::Bounded;

use frame_benchmarking::{
	v1::{account, whitelisted_caller},
	v2::*,
};
use frame_system::{
	pallet_prelude::BlockNumberFor, Call as SystemCall, Pallet as System, RawOrigin as SystemOrigin,
};

const SEED: u32 = 0;

const MAX_BYTES: u32 = 1_024;

fn assert_last_event<T: Config<I>, I: 'static>(generic_event: <T as Config<I>>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

fn assert_has_event<T: Config<I>, I: 'static>(generic_event: <T as Config<I>>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_has_event(generic_event.into());
}

fn id_to_remark_data(id: u32, length: usize) -> Vec<u8> {
	id.to_le_bytes().into_iter().cycle().take(length).collect()
}

#[instance_benchmarks(where T: Config<I>, I: 'static)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn set_members(
		m: Linear<0, { T::MaxMembers::get() }>,
		n: Linear<0, { T::MaxMembers::get() }>,
		p: Linear<0, { T::MaxProposals::get() }>,
	) -> Result<(), BenchmarkError> {
		// Set old members.
		// We compute the difference of old and new members, so it should influence timing.
		let mut old_members = vec![];
		for i in 0..m {
			let old_member = account::<T::AccountId>("old member", i, SEED);
			old_members.push(old_member);
		}

		Collective::<T, I>::set_members(
			SystemOrigin::Root.into(),
			old_members.clone(),
			old_members.last().cloned(),
			T::MaxMembers::get(),
		)?;

		// If there were any old members generate a bunch of proposals.
		if m > 0 {
			let caller = old_members.last().unwrap().clone();
			// Set a high threshold for proposals passing so that they stay around.
			let threshold = m.max(2);
			// Length of the proposals should be irrelevant to `set_members`.
			let length = 100;
			for i in 0..p {
				T::Consideration::ensure_successful(&caller, i);
				// Proposals should be different so that different proposal hashes are generated
				let proposal: T::Proposal =
					SystemCall::<T>::remark { remark: id_to_remark_data(i, length) }.into();
				Collective::<T, I>::propose(
					SystemOrigin::Signed(caller.clone()).into(),
					threshold,
					Box::new(proposal.clone()),
					MAX_BYTES,
				)?;
				let hash = T::Hashing::hash_of(&proposal);
				// Vote on the proposal to increase state relevant for `set_members`.
				// Not voting for last old member because they proposed and not voting for the first
				// member to keep the proposal from passing.
				for j in 2..m - 1 {
					let voter = &old_members[j as usize];
					let approve = true;
					Collective::<T, I>::vote(
						SystemOrigin::Signed(voter.clone()).into(),
						hash,
						i,
						approve,
					)?;
				}
			}
		}

		// Construct `new_members`.
		// It should influence timing since it will sort this vector.
		let mut new_members = vec![];
		for i in 0..n {
			let member = account::<T::AccountId>("member", i, SEED);
			new_members.push(member);
		}
		#[extrinsic_call]
		_(
			SystemOrigin::Root,
			new_members.clone(),
			new_members.last().cloned(),
			T::MaxMembers::get(),
		);

		new_members.sort();
		assert_eq!(Members::<T, I>::get(), new_members);
		Ok(())
	}

	#[benchmark]
	fn execute(
		b: Linear<2, MAX_BYTES>,
		m: Linear<1, { T::MaxMembers::get() }>,
	) -> Result<(), BenchmarkError> {
		let bytes_in_storage = b + size_of::<u32>() as u32;

		// Construct `members`.
		let mut members = vec![];
		for i in 0..m - 1 {
			let member = account::<T::AccountId>("member", i, SEED);
			members.push(member);
		}

		let caller: T::AccountId = whitelisted_caller();
		members.push(caller.clone());

		Collective::<T, I>::set_members(
			SystemOrigin::Root.into(),
			members,
			None,
			T::MaxMembers::get(),
		)?;

		let proposal: T::Proposal =
			SystemCall::<T>::remark { remark: id_to_remark_data(1, b as usize) }.into();

		#[extrinsic_call]
		_(SystemOrigin::Signed(caller), Box::new(proposal.clone()), bytes_in_storage);

		let proposal_hash = T::Hashing::hash_of(&proposal);
		// Note that execution fails due to mis-matched origin
		assert_last_event::<T, I>(Event::MemberExecuted { proposal_hash, result: Ok(()) }.into());
		Ok(())
	}

	// This tests when execution would happen immediately after proposal
	#[benchmark]
	fn propose_execute(
		b: Linear<2, MAX_BYTES>,
		m: Linear<1, { T::MaxMembers::get() }>,
	) -> Result<(), BenchmarkError> {
		let bytes_in_storage = b + size_of::<u32>() as u32;

		// Construct `members`.
		let mut members = vec![];
		for i in 0..m - 1 {
			let member = account::<T::AccountId>("member", i, SEED);
			members.push(member);
		}

		let caller: T::AccountId = whitelisted_caller();
		members.push(caller.clone());

		Collective::<T, I>::set_members(
			SystemOrigin::Root.into(),
			members,
			None,
			T::MaxMembers::get(),
		)?;

		let proposal: T::Proposal =
			SystemCall::<T>::remark { remark: id_to_remark_data(1, b as usize) }.into();
		let threshold = 1;

		#[extrinsic_call]
		propose(
			SystemOrigin::Signed(caller),
			threshold,
			Box::new(proposal.clone()),
			bytes_in_storage,
		);

		let proposal_hash = T::Hashing::hash_of(&proposal);
		// Note that execution fails due to mis-matched origin
		assert_last_event::<T, I>(Event::Executed { proposal_hash, result: Ok(()) }.into());
		Ok(())
	}

	// This tests when proposal is created and queued as "proposed"
	#[benchmark]
	fn propose_proposed(
		b: Linear<2, MAX_BYTES>,
		m: Linear<2, { T::MaxMembers::get() }>,
		p: Linear<1, { T::MaxProposals::get() }>,
	) -> Result<(), BenchmarkError> {
		let bytes_in_storage = b + size_of::<u32>() as u32;

		// Construct `members`.
		let mut members = vec![];
		for i in 0..m - 1 {
			let member = account::<T::AccountId>("member", i, SEED);
			members.push(member);
		}
		let caller: T::AccountId = whitelisted_caller();
		members.push(caller.clone());
		Collective::<T, I>::set_members(
			SystemOrigin::Root.into(),
			members,
			None,
			T::MaxMembers::get(),
		)?;

		let threshold = m;
		// Add previous proposals.
		for i in 0..p - 1 {
			T::Consideration::ensure_successful(&caller, i);
			// Proposals should be different so that different proposal hashes are generated
			let proposal: T::Proposal =
				SystemCall::<T>::remark { remark: id_to_remark_data(i, b as usize) }.into();
			Collective::<T, I>::propose(
				SystemOrigin::Signed(caller.clone()).into(),
				threshold,
				Box::new(proposal),
				bytes_in_storage,
			)?;
		}

		assert_eq!(Proposals::<T, I>::get().len(), (p - 1) as usize);

		T::Consideration::ensure_successful(&caller, p);

		let proposal: T::Proposal =
			SystemCall::<T>::remark { remark: id_to_remark_data(p, b as usize) }.into();
		#[extrinsic_call]
		propose(
			SystemOrigin::Signed(caller.clone()),
			threshold,
			Box::new(proposal.clone()),
			bytes_in_storage,
		);

		// New proposal is recorded
		assert_eq!(Proposals::<T, I>::get().len(), p as usize);
		let proposal_hash = T::Hashing::hash_of(&proposal);
		assert_last_event::<T, I>(
			Event::Proposed { account: caller, proposal_index: p - 1, proposal_hash, threshold }
				.into(),
		);
		Ok(())
	}

	#[benchmark]
	// We choose 5 as a minimum so we always trigger a vote in the voting loop (`for j in ...`)
	fn vote(m: Linear<5, { T::MaxMembers::get() }>) -> Result<(), BenchmarkError> {
		let p = T::MaxProposals::get();
		let b = MAX_BYTES;
		let bytes_in_storage = b + size_of::<u32>() as u32;

		// Construct `members`.
		let mut members = vec![];
		let proposer: T::AccountId = account::<T::AccountId>("proposer", 0, SEED);
		members.push(proposer.clone());
		for i in 1..m - 1 {
			let member = account::<T::AccountId>("member", i, SEED);
			members.push(member);
		}
		let voter: T::AccountId = account::<T::AccountId>("voter", 0, SEED);
		members.push(voter.clone());
		Collective::<T, I>::set_members(
			SystemOrigin::Root.into(),
			members.clone(),
			None,
			T::MaxMembers::get(),
		)?;

		// Threshold is 1 less than the number of members so that one person can vote nay
		let threshold = m - 1;

		// Add previous proposals
		let mut last_hash = T::Hash::default();
		for i in 0..p {
			T::Consideration::ensure_successful(&proposer, i);
			// Proposals should be different so that different proposal hashes are generated
			let proposal: T::Proposal =
				SystemCall::<T>::remark { remark: id_to_remark_data(i, b as usize) }.into();
			Collective::<T, I>::propose(
				SystemOrigin::Signed(proposer.clone()).into(),
				threshold,
				Box::new(proposal.clone()),
				bytes_in_storage,
			)?;
			last_hash = T::Hashing::hash_of(&proposal);
		}

		let index = p - 1;
		// Have almost everyone vote aye on last proposal, while keeping it from passing.
		for j in 0..m - 3 {
			let voter = &members[j as usize];
			let approve = true;
			Collective::<T, I>::vote(
				SystemOrigin::Signed(voter.clone()).into(),
				last_hash,
				index,
				approve,
			)?;
		}
		// Voter votes aye without resolving the vote.
		let approve = true;
		Collective::<T, I>::vote(
			SystemOrigin::Signed(voter.clone()).into(),
			last_hash,
			index,
			approve,
		)?;

		assert_eq!(Proposals::<T, I>::get().len(), p as usize);

		// Voter switches vote to nay, but does not kill the vote, just updates + inserts
		let approve = false;

		// Whitelist voter account from further DB operations.
		let voter_key = frame_system::Account::<T>::hashed_key_for(&voter);
		frame_benchmarking::benchmarking::add_to_whitelist(voter_key.into());

		#[extrinsic_call]
		_(SystemOrigin::Signed(voter), last_hash, index, approve);

		// All proposals exist and the last proposal has just been updated.
		assert_eq!(Proposals::<T, I>::get().len(), p as usize);
		let voting = Voting::<T, I>::get(&last_hash).ok_or("Proposal Missing")?;
		assert_eq!(voting.ayes.len(), (m - 3) as usize);
		assert_eq!(voting.nays.len(), 1);
		Ok(())
	}

	// We choose 4 as a minimum so we always trigger a vote in the voting loop (`for j in ...`)
	#[benchmark]
	fn close_early_disapproved(
		m: Linear<4, { T::MaxMembers::get() }>,
		p: Linear<1, { T::MaxProposals::get() }>,
	) -> Result<(), BenchmarkError> {
		let bytes = 100;
		let bytes_in_storage = bytes + size_of::<u32>() as u32;

		// Construct `members`.
		let mut members = vec![];
		let proposer = account::<T::AccountId>("proposer", 0, SEED);
		members.push(proposer.clone());
		for i in 1..m - 1 {
			let member = account::<T::AccountId>("member", i, SEED);
			members.push(member);
		}
		let voter = account::<T::AccountId>("voter", 0, SEED);
		members.push(voter.clone());
		Collective::<T, I>::set_members(
			SystemOrigin::Root.into(),
			members.clone(),
			None,
			T::MaxMembers::get(),
		)?;

		// Threshold is total members so that one nay will disapprove the vote
		let threshold = m;

		// Add previous proposals
		let mut last_hash = T::Hash::default();
		for i in 0..p {
			T::Consideration::ensure_successful(&proposer, i);
			// Proposals should be different so that different proposal hashes are generated
			let proposal: T::Proposal =
				SystemCall::<T>::remark { remark: id_to_remark_data(i, bytes as usize) }.into();
			Collective::<T, I>::propose(
				SystemOrigin::Signed(proposer.clone()).into(),
				threshold,
				Box::new(proposal.clone()),
				bytes_in_storage,
			)?;
			last_hash = T::Hashing::hash_of(&proposal);
		}

		let index = p - 1;
		// Have most everyone vote aye on last proposal, while keeping it from passing.
		for j in 0..m - 2 {
			let voter = &members[j as usize];
			let approve = true;
			Collective::<T, I>::vote(
				SystemOrigin::Signed(voter.clone()).into(),
				last_hash,
				index,
				approve,
			)?;
		}
		// Voter votes aye without resolving the vote.
		let approve = true;
		Collective::<T, I>::vote(
			SystemOrigin::Signed(voter.clone()).into(),
			last_hash,
			index,
			approve,
		)?;

		assert_eq!(Proposals::<T, I>::get().len(), p as usize);

		// Voter switches vote to nay, which kills the vote
		let approve = false;
		Collective::<T, I>::vote(
			SystemOrigin::Signed(voter.clone()).into(),
			last_hash,
			index,
			approve,
		)?;

		// Whitelist voter account from further DB operations.
		let voter_key = frame_system::Account::<T>::hashed_key_for(&voter);
		frame_benchmarking::benchmarking::add_to_whitelist(voter_key.into());

		#[extrinsic_call]
		close(SystemOrigin::Signed(voter), last_hash, index, Weight::MAX, bytes_in_storage);

		// The last proposal is removed.
		assert_eq!(Proposals::<T, I>::get().len(), (p - 1) as usize);
		assert_last_event::<T, I>(Event::Disapproved { proposal_hash: last_hash }.into());
		Ok(())
	}

	// m: we choose 4 as a minimum so we always trigger a vote in the voting loop (`for j in ...`)
	#[benchmark]
	fn close_early_approved(
		b: Linear<2, MAX_BYTES>,
		m: Linear<4, { T::MaxMembers::get() }>,
		p: Linear<1, { T::MaxProposals::get() }>,
	) -> Result<(), BenchmarkError> {
		let bytes_in_storage = b + size_of::<u32>() as u32;

		// Construct `members`.
		let mut members = vec![];
		for i in 0..m - 1 {
			let member = account::<T::AccountId>("member", i, SEED);
			members.push(member);
		}
		let caller: T::AccountId = whitelisted_caller();
		members.push(caller.clone());
		Collective::<T, I>::set_members(
			SystemOrigin::Root.into(),
			members.clone(),
			None,
			T::MaxMembers::get(),
		)?;

		// Threshold is 2 so any two ayes will approve the vote
		let threshold = 2;

		// Add previous proposals
		let mut last_hash = T::Hash::default();
		for i in 0..p {
			T::Consideration::ensure_successful(&caller, i);
			// Proposals should be different so that different proposal hashes are generated
			let proposal: T::Proposal =
				SystemCall::<T>::remark { remark: id_to_remark_data(i, b as usize) }.into();
			Collective::<T, I>::propose(
				SystemOrigin::Signed(caller.clone()).into(),
				threshold,
				Box::new(proposal.clone()),
				bytes_in_storage,
			)?;
			last_hash = T::Hashing::hash_of(&proposal);
		}

		// Caller switches vote to nay on their own proposal, allowing them to be the deciding
		// approval vote
		Collective::<T, I>::vote(
			SystemOrigin::Signed(caller.clone()).into(),
			last_hash,
			p - 1,
			false,
		)?;

		// Have almost everyone vote nay on last proposal, while keeping it from failing.
		for j in 2..m - 1 {
			let voter = &members[j as usize];
			let approve = false;
			Collective::<T, I>::vote(
				SystemOrigin::Signed(voter.clone()).into(),
				last_hash,
				p - 1,
				approve,
			)?;
		}

		// Member zero is the first aye
		Collective::<T, I>::vote(
			SystemOrigin::Signed(members[0].clone()).into(),
			last_hash,
			p - 1,
			true,
		)?;

		assert_eq!(Proposals::<T, I>::get().len(), p as usize);

		// Caller switches vote to aye, which passes the vote
		let index = p - 1;
		let approve = true;
		Collective::<T, I>::vote(
			SystemOrigin::Signed(caller.clone()).into(),
			last_hash,
			index,
			approve,
		)?;

		#[extrinsic_call]
		close(SystemOrigin::Signed(caller), last_hash, index, Weight::MAX, bytes_in_storage);

		// The last proposal is removed.
		assert_eq!(Proposals::<T, I>::get().len(), (p - 1) as usize);
		assert_last_event::<T, I>(
			Event::Executed { proposal_hash: last_hash, result: Ok(()) }.into(),
		);
		Ok(())
	}

	// m: we choose 4 as a minimum so we always trigger a vote in the voting loop (`for j in ...`)
	#[benchmark]
	fn close_disapproved(
		m: Linear<4, { T::MaxMembers::get() }>,
		p: Linear<1, { T::MaxProposals::get() }>,
	) -> Result<(), BenchmarkError> {
		let bytes = 100;
		let bytes_in_storage = bytes + size_of::<u32>() as u32;

		// Construct `members`.
		let mut members = vec![];
		for i in 0..m - 1 {
			let member = account::<T::AccountId>("member", i, SEED);
			members.push(member);
		}
		let caller: T::AccountId = whitelisted_caller();
		members.push(caller.clone());
		Collective::<T, I>::set_members(
			SystemOrigin::Root.into(),
			members.clone(),
			Some(caller.clone()),
			T::MaxMembers::get(),
		)?;

		// Threshold is one less than total members so that two nays will disapprove the vote
		let threshold = m - 1;

		// Add proposals
		let mut last_hash = T::Hash::default();
		for i in 0..p {
			T::Consideration::ensure_successful(&caller, i);
			// Proposals should be different so that different proposal hashes are generated
			let proposal: T::Proposal =
				SystemCall::<T>::remark { remark: id_to_remark_data(i, bytes as usize) }.into();
			Collective::<T, I>::propose(
				SystemOrigin::Signed(caller.clone()).into(),
				threshold,
				Box::new(proposal.clone()),
				bytes_in_storage,
			)?;
			last_hash = T::Hashing::hash_of(&proposal);
		}

		let index = p - 1;
		// Have almost everyone vote aye on last proposal, while keeping it from passing.
		// A few abstainers will be the nay votes needed to fail the vote.
		let mut yes_votes: MemberCount = 0;
		for j in 2..m - 1 {
			let voter = &members[j as usize];
			let approve = true;
			yes_votes += 1;
			// vote aye till a prime nay vote keeps the proposal disapproved.
			if <<T as Config<I>>::DefaultVote as DefaultVote>::default_vote(
				Some(false),
				yes_votes,
				0,
				m,
			) {
				break;
			}
			Collective::<T, I>::vote(
				SystemOrigin::Signed(voter.clone()).into(),
				last_hash,
				index,
				approve,
			)?;
		}

		// caller is prime, prime votes nay
		Collective::<T, I>::vote(
			SystemOrigin::Signed(caller.clone()).into(),
			last_hash,
			index,
			false,
		)?;

		System::<T>::set_block_number(BlockNumberFor::<T>::max_value());
		assert_eq!(Proposals::<T, I>::get().len(), p as usize);

		// Prime nay will close it as disapproved
		#[extrinsic_call]
		close(SystemOrigin::Signed(caller), last_hash, index, Weight::MAX, bytes_in_storage);

		assert_eq!(Proposals::<T, I>::get().len(), (p - 1) as usize);
		assert_last_event::<T, I>(Event::Disapproved { proposal_hash: last_hash }.into());
		Ok(())
	}

	// m: we choose 4 as a minimum so we always trigger a vote in the voting loop (`for j in ...`)
	#[benchmark]
	fn close_approved(
		b: Linear<2, MAX_BYTES>,
		m: Linear<4, { T::MaxMembers::get() }>,
		p: Linear<1, { T::MaxProposals::get() }>,
	) -> Result<(), BenchmarkError> {
		let bytes_in_storage = b + size_of::<u32>() as u32;

		// Construct `members`.
		let mut members = vec![];
		for i in 0..m - 1 {
			let member = account::<T::AccountId>("member", i, SEED);
			members.push(member);
		}
		let caller: T::AccountId = whitelisted_caller();
		members.push(caller.clone());
		Collective::<T, I>::set_members(
			SystemOrigin::Root.into(),
			members.clone(),
			Some(caller.clone()),
			T::MaxMembers::get(),
		)?;

		// Threshold is two, so any two ayes will pass the vote
		let threshold = 2;

		// Add proposals
		let mut last_hash = T::Hash::default();
		for i in 0..p {
			T::Consideration::ensure_successful(&caller, i);
			// Proposals should be different so that different proposal hashes are generated
			let proposal: T::Proposal =
				SystemCall::<T>::remark { remark: id_to_remark_data(i, b as usize) }.into();
			Collective::<T, I>::propose(
				SystemOrigin::Signed(caller.clone()).into(),
				threshold,
				Box::new(proposal.clone()),
				bytes_in_storage,
			)?;
			last_hash = T::Hashing::hash_of(&proposal);
		}

		// The prime member votes aye, so abstentions default to aye.
		Collective::<T, _>::vote(
			SystemOrigin::Signed(caller.clone()).into(),
			last_hash,
			p - 1,
			true, // Vote aye.
		)?;

		// Have almost everyone vote nay on last proposal, while keeping it from failing.
		// A few abstainers will be the aye votes needed to pass the vote.
		for j in 2..m - 1 {
			let voter = &members[j as usize];
			let approve = false;
			Collective::<T, I>::vote(
				SystemOrigin::Signed(voter.clone()).into(),
				last_hash,
				p - 1,
				approve,
			)?;
		}

		// caller is prime, prime already votes aye by creating the proposal
		System::<T>::set_block_number(BlockNumberFor::<T>::max_value());
		assert_eq!(Proposals::<T, I>::get().len(), p as usize);

		// Prime aye will close it as approved
		#[extrinsic_call]
		close(SystemOrigin::Signed(caller), last_hash, p - 1, Weight::MAX, bytes_in_storage);

		assert_eq!(Proposals::<T, I>::get().len(), (p - 1) as usize);
		assert_last_event::<T, I>(
			Event::Executed { proposal_hash: last_hash, result: Ok(()) }.into(),
		);
		Ok(())
	}

	#[benchmark]
	fn disapprove_proposal(p: Linear<1, { T::MaxProposals::get() }>) -> Result<(), BenchmarkError> {
		let m = 3;
		let b = MAX_BYTES;
		let bytes_in_storage = b + size_of::<u32>() as u32;

		// Construct `members`.
		let mut members = vec![];
		for i in 0..m - 1 {
			let member = account::<T::AccountId>("member", i, SEED);
			members.push(member);
		}
		let caller = account::<T::AccountId>("caller", 0, SEED);
		members.push(caller.clone());
		Collective::<T, I>::set_members(
			SystemOrigin::Root.into(),
			members.clone(),
			Some(caller.clone()),
			T::MaxMembers::get(),
		)?;

		// Threshold is one less than total members so that two nays will disapprove the vote
		let threshold = m - 1;

		// Add proposals
		let mut last_hash = T::Hash::default();
		for i in 0..p {
			T::Consideration::ensure_successful(&caller, i);
			// Proposals should be different so that different proposal hashes are generated
			let proposal: T::Proposal =
				SystemCall::<T>::remark { remark: id_to_remark_data(i, b as usize) }.into();
			Collective::<T, I>::propose(
				SystemOrigin::Signed(caller.clone()).into(),
				threshold,
				Box::new(proposal.clone()),
				bytes_in_storage,
			)?;
			last_hash = T::Hashing::hash_of(&proposal);
		}

		System::<T>::set_block_number(BlockNumberFor::<T>::max_value());
		assert_eq!(Proposals::<T, I>::get().len(), p as usize);

		let origin =
			T::DisapproveOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as <T as frame_system::Config>::RuntimeOrigin, last_hash);

		assert_eq!(Proposals::<T, I>::get().len(), (p - 1) as usize);
		assert_last_event::<T, I>(Event::Disapproved { proposal_hash: last_hash }.into());
		Ok(())
	}

	// d: `0` - if deposit is not present and `1` otherwise.
	#[benchmark]
	fn kill(
		d: Linear<0, 1>,
		p: Linear<1, { T::MaxProposals::get() }>,
	) -> Result<(), BenchmarkError> {
		let m = 3;
		let b = MAX_BYTES;
		let bytes_in_storage = b + size_of::<u32>() as u32;

		// Construct `members`.
		let mut members = vec![];
		for i in 0..m - 1 {
			let member = account::<T::AccountId>("member", i, SEED);
			members.push(member);
		}
		let caller = account::<T::AccountId>("caller", 0, SEED);
		members.push(caller.clone());
		Collective::<T, I>::set_members(
			SystemOrigin::Root.into(),
			members.clone(),
			Some(caller.clone()),
			T::MaxMembers::get(),
		)?;

		// Threshold is one less than total members so that two nays will disapprove the vote
		let threshold = m - 1;

		// Add proposals
		let mut last_hash = T::Hash::default();
		for i in 0..p {
			T::Consideration::ensure_successful(&caller, i);

			// Proposals should be different so that different proposal hashes are generated
			let proposal: T::Proposal =
				SystemCall::<T>::remark { remark: id_to_remark_data(i, b as usize) }.into();
			Collective::<T, I>::propose(
				SystemOrigin::Signed(caller.clone()).into(),
				threshold,
				Box::new(proposal.clone()),
				bytes_in_storage,
			)?;
			last_hash = T::Hashing::hash_of(&proposal);
		}

		System::<T>::set_block_number(BlockNumberFor::<T>::max_value());
		assert_eq!(Proposals::<T, I>::get().len(), p as usize);

		if d == 0 {
			CostOf::<T, I>::remove(last_hash);
		}
		let cost_present = CostOf::<T, I>::get(last_hash).is_some();

		let origin =
			T::KillOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as <T as frame_system::Config>::RuntimeOrigin, last_hash);

		assert_eq!(Proposals::<T, I>::get().len(), (p - 1) as usize);
		assert_last_event::<T, I>(Event::Killed { proposal_hash: last_hash }.into());
		if cost_present {
			assert_has_event::<T, I>(
				Event::ProposalCostBurned { proposal_hash: last_hash, who: caller }.into(),
			);
		}
		Ok(())
	}

	#[benchmark]
	fn release_proposal_cost() -> Result<(), BenchmarkError> {
		let m = 3;
		let p = T::MaxProposals::get();
		let b = MAX_BYTES;
		let bytes_in_storage = b + size_of::<u32>() as u32;

		// Construct `members`.
		let mut members = vec![];
		for i in 0..m - 1 {
			let member = account::<T::AccountId>("member", i, SEED);
			members.push(member);
		}
		let caller = account::<T::AccountId>("caller", 0, SEED);
		members.push(caller.clone());
		Collective::<T, I>::set_members(
			SystemOrigin::Root.into(),
			members.clone(),
			Some(caller.clone()),
			T::MaxMembers::get(),
		)?;

		// Add proposals
		let threshold = 2;
		let mut last_hash = T::Hash::default();
		for i in 0..p {
			T::Consideration::ensure_successful(&caller, i);

			// Proposals should be different so that different proposal hashes are generated
			let proposal: T::Proposal =
				SystemCall::<T>::remark { remark: id_to_remark_data(i, b as usize) }.into();
			Collective::<T, I>::propose(
				SystemOrigin::Signed(caller.clone()).into(),
				threshold,
				Box::new(proposal.clone()),
				bytes_in_storage,
			)?;
			last_hash = T::Hashing::hash_of(&proposal);
		}

		System::<T>::set_block_number(BlockNumberFor::<T>::max_value());
		assert_eq!(Proposals::<T, I>::get().len(), p as usize);

		assert_eq!(Proposals::<T, I>::get().len(), p as usize);
		let _ = Collective::<T, I>::remove_proposal(last_hash);
		assert_eq!(Proposals::<T, I>::get().len(), (p - 1) as usize);

		let cost_present = CostOf::<T, I>::get(last_hash).is_some();

		#[extrinsic_call]
		_(SystemOrigin::Signed(caller.clone()), last_hash);

		assert_eq!(CostOf::<T, I>::get(last_hash), None);
		if cost_present {
			assert_last_event::<T, I>(
				Event::ProposalCostReleased { proposal_hash: last_hash, who: caller }.into(),
			);
		}
		Ok(())
	}

	impl_benchmark_test_suite!(
		Collective,
		crate::tests::ExtBuilder::default().build(),
		crate::tests::Test
	);
}
