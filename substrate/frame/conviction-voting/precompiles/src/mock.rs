// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.
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

use frame_support::{
	derive_impl, parameter_types,
	traits::{ConstU32, ConstU64, Contains, PollStatus, Polling},
};
use pallet_conviction_voting::{AccountVote, Status, Tally, TallyOf, VotingHooks};
use sp_runtime::{
	traits::IdentityLookup, AccountId32, BuildStorage, DispatchError, DispatchResult,
};
use std::{cell::RefCell, collections::BTreeMap};

pub type AccountId = AccountId32;
pub type Balance = u128;
pub type ReferendumIndex = u32;
pub type TrackId = u16;
pub type Moment = u64;
type Block = frame_system::mocking::MockBlock<Test>;

use crate::ConvictionVotingPrecompile;

pub const ALICE: AccountId32 = AccountId::new([0u8; 32]);
pub const BOB: AccountId32 = AccountId::new([1u8; 32]);
pub const CHARLIE: AccountId32 = AccountId::new([2u8; 32]);
pub const DAVE: AccountId32 = AccountId::new([3u8; 32]);

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,
		ConvictionVoting: pallet_conviction_voting,
		Revive: pallet_revive,
	}
);

// Test that a filtered call can be dispatched.
pub struct BaseFilter;
impl Contains<RuntimeCall> for BaseFilter {
	fn contains(call: &RuntimeCall) -> bool {
		!matches!(call, &RuntimeCall::Balances(pallet_balances::Call::force_set_balance { .. }))
	}
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type BaseCallFilter = BaseFilter;
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
}

parameter_types! {
	pub ExistentialDeposit: Balance = 1;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type Balance = Balance;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
}

#[derive_impl(pallet_revive::config_preludes::TestDefaultConfig)]
impl pallet_revive::Config for Test {
	type AddressMapper = pallet_revive::AccountId32Mapper<Self>;
	type Balance = Balance;
	type Currency = Balances;
	type Precompiles = (ConvictionVotingPrecompile<Self>,);
	type UploadOrigin = frame_system::EnsureSigned<AccountId>;
	type InstantiateOrigin = frame_system::EnsureSigned<AccountId>;
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TestPollState {
	Ongoing(TallyOf<Test>, TrackId),
	Completed(Moment, bool),
}
use TestPollState::*;

parameter_types! {
	pub static Polls: BTreeMap<ReferendumIndex, TestPollState> = vec![
		(1u32, Completed(1, true)),
		(2u32, Completed(2, false)),
		(3u32, Ongoing(Tally::from_parts(0, 0, 0), 0)),
	].into_iter().collect();
}

pub struct TestPolls;
impl Polling<TallyOf<Test>> for TestPolls {
	type Index = ReferendumIndex;
	type Class = TrackId;
	type Votes = Balance;
	type Moment = Moment;
	fn classes() -> Vec<TrackId> {
		vec![0u16, 1u16, 2u16]
	}
	fn as_ongoing(index: ReferendumIndex) -> Option<(TallyOf<Test>, Self::Class)> {
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
		f: impl FnOnce(PollStatus<&mut TallyOf<Test>, Moment, TrackId>) -> R,
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
		f: impl FnOnce(PollStatus<&mut TallyOf<Test>, Moment, TrackId>) -> Result<R, DispatchError>,
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
		polls.insert(i, Ongoing(Tally::new(0), class));
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

impl pallet_conviction_voting::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Currency = pallet_balances::Pallet<Self>;
	type VoteLockingPeriod = ConstU64<3>;
	type MaxVotes = ConstU32<3>;
	type WeightInfo = ();
	type MaxTurnout = frame_support::traits::TotalIssuanceOf<Balances, Self::AccountId>;
	type Polls = TestPolls;
	type BlockNumberProvider = System;
	type VotingHooks = HooksHandler;
}

thread_local! {
	static LAST_ON_VOTE_DATA: RefCell<Option<(AccountId, ReferendumIndex, AccountVote<Balance>)>> = RefCell::new(None);
	static LAST_ON_REMOVE_VOTE_DATA: RefCell<Option<(AccountId, ReferendumIndex, Status)>> = RefCell::new(None);
	static LAST_LOCKED_IF_UNSUCCESSFUL_VOTE_DATA: RefCell<Option<(AccountId, ReferendumIndex)>> = RefCell::new(None);
	static REMOVE_VOTE_LOCKED_AMOUNT: RefCell<Option<Balance>> = RefCell::new(None);
}

pub struct HooksHandler;
impl HooksHandler {
	fn last_on_vote_data() -> Option<(AccountId, ReferendumIndex, AccountVote<Balance>)> {
		LAST_ON_VOTE_DATA.with(|data| data.borrow().clone())
	}

	fn last_on_remove_vote_data() -> Option<(AccountId, ReferendumIndex, Status)> {
		LAST_ON_REMOVE_VOTE_DATA.with(|data| data.borrow().clone())
	}

	fn last_locked_if_unsuccessful_vote_data() -> Option<(AccountId, ReferendumIndex)> {
		LAST_LOCKED_IF_UNSUCCESSFUL_VOTE_DATA.with(|data| data.borrow().clone())
	}

	fn reset() {
		LAST_ON_VOTE_DATA.with(|data| *data.borrow_mut() = None);
		LAST_ON_REMOVE_VOTE_DATA.with(|data| *data.borrow_mut() = None);
		LAST_LOCKED_IF_UNSUCCESSFUL_VOTE_DATA.with(|data| *data.borrow_mut() = None);
		REMOVE_VOTE_LOCKED_AMOUNT.with(|data| *data.borrow_mut() = None);
	}

	fn with_remove_locked_amount(v: Balance) {
		REMOVE_VOTE_LOCKED_AMOUNT.with(|data| *data.borrow_mut() = Some(v));
	}
}

impl VotingHooks<AccountId, ReferendumIndex, Balance> for HooksHandler {
	fn on_before_vote(
		who: &AccountId,
		ref_index: ReferendumIndex,
		vote: AccountVote<Balance>,
	) -> DispatchResult {
		LAST_ON_VOTE_DATA.with(|data| {
			*data.borrow_mut() = Some((who.clone(), ref_index, vote));
		});
		Ok(())
	}

	fn on_remove_vote(who: &AccountId, ref_index: ReferendumIndex, ongoing: Status) {
		LAST_ON_REMOVE_VOTE_DATA.with(|data| {
			*data.borrow_mut() = Some((who.clone(), ref_index, ongoing));
		});
	}

	fn lock_balance_on_unsuccessful_vote(
		who: &AccountId,
		ref_index: ReferendumIndex,
	) -> Option<Balance> {
		LAST_LOCKED_IF_UNSUCCESSFUL_VOTE_DATA.with(|data| {
			*data.borrow_mut() = Some((who.clone(), ref_index));

			REMOVE_VOTE_LOCKED_AMOUNT.with(|data| *data.borrow())
		})
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn on_vote_worst_case(_who: &AccountId) {}

	#[cfg(feature = "runtime-benchmarks")]
	fn on_remove_vote_worst_case(_who: &AccountId) {}
}

/// Declares a new test externality, funds ALICE, BOB, CHARLIE and DAVE accounts.
pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

	let initial_balance = Balance::MAX.saturating_div(100000);
	let balances = vec![
		(ALICE, initial_balance),
		(BOB, initial_balance),
		(CHARLIE, initial_balance),
		(DAVE, initial_balance),
	];

	pallet_balances::GenesisConfig::<Test> { balances, ..Default::default() }
		.assimilate_storage(&mut t)
		.unwrap();

	pallet_revive::GenesisConfig::<Test> {
		mapped_accounts: vec![ALICE, BOB, CHARLIE, DAVE],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

pub(crate) fn new_test_ext_with_balances(
	balances: Vec<(AccountId, Balance)>,
) -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

	pallet_balances::GenesisConfig::<Test> { balances, ..Default::default() }
		.assimilate_storage(&mut t)
		.unwrap();

	pallet_revive::GenesisConfig::<Test> {
		mapped_accounts: vec![ALICE, BOB, CHARLIE, DAVE],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}
