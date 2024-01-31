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

//! Integration test together with the ranked-collective pallet.

use std::collections::BTreeMap;

use frame_support::{
	assert_noop, assert_ok, derive_impl, hypothetically, ord_parameter_types,
	pallet_prelude::Weight,
	parameter_types,
	traits::{
		ConstU16, ConstU32, ConstU64, EitherOf, Everything, IsInVec, MapSuccess, PollStatus,
		Polling, TryMapSuccess,
	},
};
use frame_system::EnsureSignedBy;
use pallet_ranked_collective::{EnsureRanked, Geometric, Rank, Tally, TallyOf, Votes};
use sp_core::{Get, H256};
use sp_runtime::{
	traits::{BlakeTwo256, Convert, IdentityLookup, ReduceBy, TryMorphInto},
	BuildStorage, DispatchError, DispatchResult,
};
type Class = Rank;
use sp_std::cell::RefCell;

use crate as pallet_core_fellowship;
use crate::*;

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		CoreFellowship: pallet_core_fellowship,
		Club: pallet_ranked_collective,
	}
);

parameter_types! {
	pub BlockWeights: frame_system::limits::BlockWeights =
		frame_system::limits::BlockWeights::simple_max(Weight::from_parts(1_000_000, u64::max_value()));
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type Nonce = u64;
	type RuntimeCall = RuntimeCall;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = ConstU64<250>;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
}

parameter_types! {
	pub static MinRankOfClassDelta: Rank = 0;
}

thread_local! {
	pub static CLUB: RefCell<BTreeMap<u64, u16>> = RefCell::new(BTreeMap::new());
}

pub struct TestClub;
impl RankedMembers for TestClub {
	type AccountId = u64;
	type Rank = u16;
	fn min_rank() -> Self::Rank {
		0
	}
	fn rank_of(who: &Self::AccountId) -> Option<Self::Rank> {
		CLUB.with(|club| club.borrow().get(who).cloned())
	}
	fn induct(who: &Self::AccountId) -> DispatchResult {
		CLUB.with(|club| club.borrow_mut().insert(*who, 0));
		Ok(())
	}
	fn promote(who: &Self::AccountId) -> DispatchResult {
		CLUB.with(|club| {
			club.borrow_mut().entry(*who).and_modify(|r| *r += 1);
		});
		Ok(())
	}
	fn demote(who: &Self::AccountId) -> DispatchResult {
		CLUB.with(|club| match Self::rank_of(who) {
			None => Err(sp_runtime::DispatchError::Unavailable),
			Some(0) => {
				club.borrow_mut().remove(&who);
				Ok(())
			},
			Some(_) => {
				club.borrow_mut().entry(*who).and_modify(|x| *x -= 1);
				Ok(())
			},
		})
	}
}

fn set_rank(who: u64, rank: u16) {
	CLUB.with(|club| club.borrow_mut().insert(who, rank));
}

parameter_types! {
	pub ZeroToNine: Vec<u64> = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
	pub EvidenceSize: u32 = 1024;
}
ord_parameter_types! {
	pub const One: u64 = 1;
}

impl Config for Test {
	type WeightInfo = ();
	type RuntimeEvent = RuntimeEvent;
	type Members = TestClub;
	type Balance = u64;
	type ParamsOrigin = EnsureSignedBy<One, u64>;
	type InductOrigin = EnsureInducted<Test, (), 1>;
	type ApproveOrigin = TryMapSuccess<EnsureSignedBy<IsInVec<ZeroToNine>, u64>, TryMorphInto<u16>>;
	type PromoteOrigin = TryMapSuccess<EnsureSignedBy<IsInVec<ZeroToNine>, u64>, TryMorphInto<u16>>;
	type EvidenceSize = EvidenceSize;
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TestPollState {
	Ongoing(TallyOf<Test>, Rank),
	Completed(u64, bool),
}
use TestPollState::*;

parameter_types! {
	pub static Polls: BTreeMap<u8, TestPollState> = vec![
		(1, Completed(1, true)),
		(2, Completed(2, false)),
		(3, Ongoing(Tally::from_parts(0, 0, 0), 1)),
	].into_iter().collect();
}

pub struct TestPolls;
impl Polling<TallyOf<Test>> for TestPolls {
	type Index = u8;
	type Votes = Votes;
	type Moment = u64;
	type Class = Class;
	fn classes() -> Vec<Self::Class> {
		vec![0, 1, 2]
	}
	fn as_ongoing(index: u8) -> Option<(TallyOf<Test>, Self::Class)> {
		Polls::get().remove(&index).and_then(|x| {
			if let TestPollState::Ongoing(t, c) = x {
				Some((t, c))
			} else {
				None
			}
		})
	}
	fn access_poll<R>(
		index: Self::Index,
		f: impl FnOnce(PollStatus<&mut TallyOf<Test>, Self::Moment, Self::Class>) -> R,
	) -> R {
		let mut polls = Polls::get();
		let entry = polls.get_mut(&index);
		let r = match entry {
			Some(Ongoing(ref mut tally_mut_ref, class)) =>
				f(PollStatus::Ongoing(tally_mut_ref, *class)),
			Some(Completed(when, succeeded)) => f(PollStatus::Completed(*when, *succeeded)),
			None => f(PollStatus::None),
		};
		Polls::set(polls);
		r
	}
	fn try_access_poll<R>(
		index: Self::Index,
		f: impl FnOnce(
			PollStatus<&mut TallyOf<Test>, Self::Moment, Self::Class>,
		) -> Result<R, DispatchError>,
	) -> Result<R, DispatchError> {
		let mut polls = Polls::get();
		let entry = polls.get_mut(&index);
		let r = match entry {
			Some(Ongoing(ref mut tally_mut_ref, class)) =>
				f(PollStatus::Ongoing(tally_mut_ref, *class)),
			Some(Completed(when, succeeded)) => f(PollStatus::Completed(*when, *succeeded)),
			None => f(PollStatus::None),
		}?;
		Polls::set(polls);
		Ok(r)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn create_ongoing(class: Self::Class) -> Result<Self::Index, ()> {
		let mut polls = Polls::get();
		let i = polls.keys().rev().next().map_or(0, |x| x + 1);
		polls.insert(i, Ongoing(Tally::new(class), class));
		Polls::set(polls);
		Ok(i)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn end_ongoing(index: Self::Index, approved: bool) -> Result<(), ()> {
		let mut polls = Polls::get();
		match polls.get(&index) {
			Some(Ongoing(..)) => {},
			_ => return Err(()),
		}
		let now = frame_system::Pallet::<Test>::block_number();
		polls.insert(index, Completed(now, approved));
		Polls::set(polls);
		Ok(())
	}
}

/// Convert the tally class into the minimum rank required to vote on the poll.
/// MinRank(Class) = Class - Delta
pub struct MinRankOfClass<Delta>(PhantomData<Delta>);
impl<Delta: Get<Rank>> Convert<Class, Rank> for MinRankOfClass<Delta> {
	fn convert(a: Class) -> Rank {
		a.saturating_sub(Delta::get())
	}
}

impl pallet_ranked_collective::Config for Test {
	type WeightInfo = ();
	type RuntimeEvent = RuntimeEvent;
	type PromoteOrigin = EitherOf<
		// Root can promote arbitrarily.
		frame_system::EnsureRootWithSuccess<Self::AccountId, ConstU16<65535>>,
		// Members can promote up to the rank of 2 below them.
		MapSuccess<EnsureRanked<Test, (), 2>, ReduceBy<ConstU16<2>>>,
	>;
	type DemoteOrigin = EitherOf<
		// Root can demote arbitrarily.
		frame_system::EnsureRootWithSuccess<Self::AccountId, ConstU16<65535>>,
		// Members can demote up to the rank of 3 below them.
		MapSuccess<EnsureRanked<Test, (), 3>, ReduceBy<ConstU16<3>>>,
	>;
	type ExchangeOrigin = EitherOf<
		// Root can exchange arbitrarily.
		frame_system::EnsureRootWithSuccess<Self::AccountId, ConstU16<65535>>,
		// Members can exchange up to the rank of 2 below them.
		MapSuccess<EnsureRanked<Test, (), 2>, ReduceBy<ConstU16<2>>>,
	>;
	type Polls = TestPolls;
	type MinRankOfClass = MinRankOfClass<MinRankOfClassDelta>;
	type VoteWeight = Geometric;
	type MemberSwappedHandler = CoreFellowship;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| {
		let params = ParamsType {
			active_salary: [10, 20, 30, 40, 50, 60, 70, 80, 90],
			passive_salary: [1, 2, 3, 4, 5, 6, 7, 8, 9],
			demotion_period: [2, 4, 6, 8, 10, 12, 14, 16, 18],
			min_promotion_period: [3, 6, 9, 12, 15, 18, 21, 24, 27],
			offboard_timeout: 1,
		};
		assert_ok!(CoreFellowship::set_params(signed(1), Box::new(params)));
		System::set_block_number(1);
	});
	ext
}

fn signed(who: u64) -> RuntimeOrigin {
	RuntimeOrigin::signed(who)
}

fn assert_last_event(generic_event: <Test as Config>::RuntimeEvent) {
	let events = frame_system::Pallet::<Test>::events();
	let system_event: <Test as frame_system::Config>::RuntimeEvent = generic_event.into();
	let frame_system::EventRecord { event, .. } = events.last().expect("Event expected");
	assert_eq!(event, &system_event.into());
}

fn evidence(e: u32) -> Evidence<Test, ()> {
	e.encode()
		.into_iter()
		.cycle()
		.take(1024)
		.collect::<Vec<_>>()
		.try_into()
		.expect("Static length matches")
}

#[test]
fn swap_simple_works() {
	new_test_ext().execute_with(|| {
		for i in 0u16..9 {
			let acc = i as u64;

			assert_ok!(Club::add_member(RuntimeOrigin::root(), acc));
			set_rank(acc, i);
			assert_ok!(CoreFellowship::import(signed(acc)));

			// Swapping normally works:
			assert_ok!(Club::exchange_member(RuntimeOrigin::root(), acc, acc + 10));
			assert_last_event(Event::Swapped { who: acc, new_who: acc + 10 }.into());
		}
	});
}

/// Exhaustively test that adding member `1` is equivalent to adding member `0` and then swapping.
///
/// The member also submits evidence before the swap.
#[test]
fn swap_exhaustive_works() {
	new_test_ext().execute_with(|| {
		let root_add = hypothetically!({
			assert_ok!(Club::add_member(RuntimeOrigin::root(), 1));
			set_rank(1, 4);
			assert_ok!(CoreFellowship::import(signed(1)));
			assert_ok!(CoreFellowship::submit_evidence(signed(1), Wish::Retention, evidence(1)));

			// The events mess up the storage root:
			System::reset_events();
			sp_io::storage::root(sp_runtime::StateVersion::V1)
		});

		let root_swap = hypothetically!({
			assert_ok!(Club::add_member(RuntimeOrigin::root(), 0));
			set_rank(0, 4);
			assert_ok!(CoreFellowship::import(signed(0)));
			assert_ok!(CoreFellowship::submit_evidence(signed(0), Wish::Retention, evidence(1)));

			// Now we swap:
			assert_ok!(Club::exchange_member(RuntimeOrigin::root(), 0, 1));

			System::reset_events();
			sp_io::storage::root(sp_runtime::StateVersion::V1)
		});

		assert_eq!(root_add, root_swap);
	});
}

#[test]
fn swap_bad_noops() {
	new_test_ext().execute_with(|| {
		assert_ok!(Club::add_member(RuntimeOrigin::root(), 0));
		set_rank(0, 0);
		assert_ok!(CoreFellowship::import(signed(0)));
		assert_ok!(Club::add_member(RuntimeOrigin::root(), 1));
		set_rank(1, 1);
		assert_ok!(CoreFellowship::import(signed(1)));

		// Swapping for another member is a noop:
		assert_noop!(
			Club::exchange_member(RuntimeOrigin::root(), 0, 1),
			pallet_ranked_collective::Error::<Test>::AlreadyMember
		);
		// Swapping for the same member is a noop:
		assert_noop!(
			Club::exchange_member(RuntimeOrigin::root(), 0, 0),
			pallet_ranked_collective::Error::<Test>::SameMember
		);
	});
}
