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

//! # Test environment for OPF pallet.
use crate as pallet_opf;
use crate::{
	traits::ReferendumTrait, Convert, Error, Fortitude, HoldReason, Preservation, ReferendumStates,
};
// Removed unused import: use codec::{Decode, Encode};
pub use frame_support::{
	derive_impl, ord_parameter_types,
	pallet_prelude::{DecodeWithMemTracking, MaxEncodedLen, TypeInfo, *},
	parameter_types,
	traits::{
		fungible::{Inspect, InspectHold, MutateHold},
		ConstU32, ConstU64, EqualPrivilegeOnly, OnFinalize, OnInitialize, OriginTrait, PollStatus,
		Polling, VoteTally,
	},
	weights::Weight,
	PalletId,
};
pub use frame_system::{EnsureRoot, EnsureSignedBy};
pub use sp_runtime::{
	str_array as s,
	traits::{AccountIdConversion, IdentityLookup},
	BuildStorage, Cow, DispatchError, Perbill,
};
use sp_std::{cell::RefCell, collections::btree_map::BTreeMap};

use pallet_conviction_voting::{
	AccountVote, Status, Tally as TallyOfConviction, TallyOf, VotingHooks,
};

pub use pallet_referenda::{Curve, Track, TrackInfo, TracksInfo};
pub type Block = frame_system::mocking::MockBlock<Test>;
pub type Balance = u64;
pub type AccountId = u64;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub struct Test {
		System: frame_system,
		Balances: pallet_balances,
		Preimage: pallet_preimage,
		Scheduler: pallet_scheduler,
		Opf: pallet_opf,
		Referenda: pallet_referenda,
		ConvictionVoting: pallet_conviction_voting,
	}
);

parameter_types! {
	pub MaxWeight: Weight = Weight::from_parts(2_000_000_000_000, u64::MAX);
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type AccountId = AccountId;
	type AccountData = pallet_balances::AccountData<u64>;
	type Block = Block;
	type Lookup = IdentityLookup<Self::AccountId>;
}

impl pallet_conviction_voting::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Currency = pallet_balances::Pallet<Self>;
	type VoteLockingPeriod = ConstU64<3>;
	type MaxVotes = ConstU32<3>;
	type WeightInfo = ();
	type MaxTurnout = frame_support::traits::TotalIssuanceOf<Balances, Self::AccountId>;
	type Polls = Referenda;
	type BlockNumberProvider = System;
	type VotingHooks = HooksHandler;
}
impl pallet_preimage::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type Currency = Balances;
	type ManagerOrigin = EnsureRoot<u64>;
	type Consideration = ();
}
impl pallet_scheduler::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeOrigin = RuntimeOrigin;
	type PalletsOrigin = OriginCaller;
	type RuntimeCall = RuntimeCall;
	type MaximumWeight = MaxWeight;
	type ScheduleOrigin = EnsureRoot<u64>;
	type MaxScheduledPerBlock = ConstU32<100>;
	type WeightInfo = ();
	type OriginPrivilegeCmp = EqualPrivilegeOnly;
	type Preimages = Preimage;
	type BlockNumberProvider = frame_system::Pallet<Test>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
}

parameter_types! {
	pub static AlarmInterval: u64 = 1;
}
ord_parameter_types! {
	pub const One: u64 = 1;
	pub const Two: u64 = 2;
	pub const Three: u64 = 3;
	pub const Four: u64 = 4;
	pub const Five: u64 = 5;
	pub const Six: u64 = 6;
}

pub struct TestTracksInfo;
impl TracksInfo<u64, u64> for TestTracksInfo {
	type Id = u8;
	type RuntimeOrigin = <RuntimeOrigin as OriginTrait>::PalletsOrigin;

	fn tracks() -> impl Iterator<Item = Cow<'static, Track<Self::Id, u64, u64>>> {
		static DATA: [Track<u8, u64, u64>; 3] = [
			Track {
				id: 0u8,
				info: TrackInfo {
					name: s("root"),
					max_deciding: 1,
					decision_deposit: 10,
					prepare_period: 4,
					decision_period: 4,
					confirm_period: 2,
					min_enactment_period: 4,
					min_approval: Curve::LinearDecreasing {
						length: Perbill::from_percent(2),
						floor: Perbill::from_percent(1),
						ceil: Perbill::from_percent(2),
					},
					min_support: Curve::LinearDecreasing {
						length: Perbill::from_percent(2),
						floor: Perbill::from_percent(0),
						ceil: Perbill::from_percent(2),
					},
				},
			},
			Track {
				id: 1u8,
				info: TrackInfo {
					name: s("none"),
					max_deciding: 3,
					decision_deposit: 1,
					prepare_period: 2,
					decision_period: 2,
					confirm_period: 1,
					min_enactment_period: 2,
					min_approval: Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(95),
						ceil: Perbill::from_percent(100),
					},
					min_support: Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(90),
						ceil: Perbill::from_percent(100),
					},
				},
			},
			Track {
				id: 2u8,
				info: TrackInfo {
					name: s("none"),
					max_deciding: 3,
					decision_deposit: 1,
					prepare_period: 2,
					decision_period: 2,
					confirm_period: 1,
					min_enactment_period: 0,
					min_approval: Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(95),
						ceil: Perbill::from_percent(100),
					},
					min_support: Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(90),
						ceil: Perbill::from_percent(100),
					},
				},
			},
		];
		DATA.iter().map(Cow::Borrowed)
	}
	fn track_for(id: &Self::RuntimeOrigin) -> Result<Self::Id, ()> {
		if let Ok(system_origin) = frame_system::RawOrigin::try_from(id.clone()) {
			match system_origin {
				frame_system::RawOrigin::Root => Ok(0),
				frame_system::RawOrigin::None => Ok(1),
				frame_system::RawOrigin::Signed(1) => Ok(2),
				_ => Err(()),
			}
		} else {
			Err(())
		}
	}
}
impl pallet_referenda::Config for Test {
	type WeightInfo = ();
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type Scheduler = Scheduler;
	type Currency = pallet_balances::Pallet<Self>;
	type SubmitOrigin = frame_system::EnsureSigned<u64>;
	type CancelOrigin = EnsureSignedBy<Four, u64>;
	type KillOrigin = EnsureRoot<u64>;
	type Slash = ();
	type Votes = pallet_conviction_voting::VotesOf<Test>;
	type Tally = pallet_conviction_voting::TallyOf<Test>;
	type SubmissionDeposit = ConstU64<2>;
	type MaxQueued = ConstU32<3>;
	type UndecidingTimeout = ConstU64<20>;
	type AlarmInterval = AlarmInterval;
	type Tracks = TestTracksInfo;
	type Preimages = Preimage;
	type BlockNumberProvider = System;
}

parameter_types! {
	pub const PotId: PalletId = PalletId(*b"py/potid");
	pub const MaxProjects:u32 = 50;
	pub const TemporaryRewards: Balance = 100_000;
	pub const VotingPeriod:u64 = 2;
}
impl pallet_opf::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type AdminOrigin = frame_system::EnsureRoot<Self::AccountId>;
	type NativeBalance = Balances;
	type PotId = PotId;
	type RuntimeHoldReason = RuntimeHoldReason;
	type MaxProjects = MaxProjects;
	type VotingPeriod = VotingPeriod;
	type ClaimingPeriod = VotingPeriod;
	type VoteValidityPeriod = VotingPeriod;
	type BlockNumberProvider = System;
	type TemporaryRewards = TemporaryRewards;
	type EnactmentPeriod = ConstU64<1>;
	type Governance = Referenda;
	type Conviction = ConvictionVoting;
	type WeightInfo = ();
}

impl Convert<RuntimeCall, RuntimeCall> for RuntimeCall {
	fn convert(call: RuntimeCall) -> RuntimeCall {
		let call_encoded: Vec<u8> = call.encode();
		let ref_call_encoded = &call_encoded;
		if let Ok(call_formatted) = RuntimeCall::decode(&mut &ref_call_encoded[..]) {
			call_formatted
		} else {
			call
		}
	}
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TestPollState {
	Ongoing(TallyOf<Test>, u8),
	Completed(u64, bool),
}
use TestPollState::*;

parameter_types! {
	pub static Polls: BTreeMap<u8, TestPollState> = vec![
		(1, Completed(1, true)),
		(2, Completed(2, false)),
		(3, Ongoing(TallyOfConviction::from_parts(0, 0, 0), 0)),
	].into_iter().collect();
}

pub struct TestPolls;
impl Polling<TallyOf<Test>> for TestPolls {
	type Index = u32;
	type Votes = u64;
	type Moment = u64;
	type Class = u8;
	fn classes() -> Vec<u8> {
		vec![0, 1, 2]
	}
	fn as_ongoing(index: u32) -> Option<(TallyOf<Test>, Self::Class)> {
		Polls::get().remove(&(index.try_into().unwrap())).and_then(|x| {
			if let TestPollState::Ongoing(t, c) = x {
				Some((t, c))
			} else {
				None
			}
		})
	}
	fn access_poll<R>(
		index: Self::Index,
		f: impl FnOnce(PollStatus<&mut TallyOf<Test>, u64, u8>) -> R,
	) -> R {
		let mut polls = Polls::get();
		let entry = polls.get_mut(&(index.try_into().unwrap()));
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
		f: impl FnOnce(PollStatus<&mut TallyOf<Test>, u64, u8>) -> Result<R, DispatchError>,
	) -> Result<R, DispatchError> {
		let mut polls = Polls::get();
		let entry = polls.get_mut(&(index.try_into().unwrap()));
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
		polls.insert(i, Ongoing(TallyOfConviction::new(0), class));
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

thread_local! {
	static LAST_ON_VOTE_DATA: RefCell<Option<(u64, u8, AccountVote<u64>)>> = RefCell::new(None);
	static LAST_ON_REMOVE_VOTE_DATA: RefCell<Option<(u64, u8, Status)>> = RefCell::new(None);
	static LAST_LOCKED_IF_UNSUCCESSFUL_VOTE_DATA: RefCell<Option<(u64, u8)>> = RefCell::new(None);
	static REMOVE_VOTE_LOCKED_AMOUNT: RefCell<Option<u64>> = RefCell::new(None);
}

pub struct HooksHandler;

impl VotingHooks<u64, u32, u64> for HooksHandler {
	fn on_before_vote(who: &u64, ref_index: u32, vote: AccountVote<u64>) -> DispatchResult {
		// lock user's funds
		let ref_info =
			<Test as pallet_opf::Config>::Governance::get_referendum_info(ref_index).unwrap();
		let ref_status =
			<Test as pallet_opf::Config>::Governance::handle_referendum_info(ref_info.clone())
				.unwrap();
		match ref_status {
			ReferendumStates::Ongoing => {
				let amount = vote.balance();
				// Check that voter has enough funds to vote
				let voter_balance = <Test as pallet_opf::Config>::NativeBalance::reducible_balance(
					&who,
					Preservation::Preserve,
					Fortitude::Polite,
				);
				ensure!(voter_balance >= amount, pallet_opf::Error::<Test>::NotEnoughFunds);
				// Check the available un-holded balance
				let voter_holds = <Test as pallet_opf::Config>::NativeBalance::balance_on_hold(
					&<Test as pallet_opf::Config>::RuntimeHoldReason::from(
						HoldReason::FundsReserved,
					),
					&who,
				);
				let available_funds = voter_balance.saturating_sub(voter_holds);
				ensure!(available_funds > amount, Error::<Test>::NotEnoughFunds);
				// Lock the necessary amount
				<Test as pallet_opf::Config>::NativeBalance::hold(
					&HoldReason::FundsReserved.into(),
					&who,
					amount,
				)?;
			},
			_ => return Err(DispatchError::Other("Not an ongoing referendum")),
		};
		LAST_ON_VOTE_DATA.with(|data| {
			*data.borrow_mut() = Some((*who, ref_index.try_into().unwrap(), vote));
		});
		Ok(())
	}

	fn on_remove_vote(who: &u64, ref_index: u32, ongoing: Status) {
		LAST_ON_REMOVE_VOTE_DATA.with(|data| {
			*data.borrow_mut() = Some((*who, ref_index.try_into().unwrap(), ongoing));
		});

		let ref_info =
			match <Test as pallet_opf::Config>::Governance::get_referendum_info(ref_index.into()) {
				Some(info) => info,
				None => return,
			};
		let ref_status = match <Test as pallet_opf::Config>::Governance::handle_referendum_info(
			ref_info.clone(),
		) {
			Some(status) => status,
			None => return,
		};
		match ref_status {
			ReferendumStates::Ongoing => {
				let vote_infos = match crate::Votes::<Test>::get(&ref_index, who) {
					Some(vote_infos) => vote_infos,
					None => return,
				};
				let vote_info = vote_infos;
				let amount = vote_info.amount;
				// Unlock user's funds
				<Test as pallet_opf::Config>::NativeBalance::release(
					&HoldReason::FundsReserved.into(),
					&who,
					amount,
					crate::Precision::Exact,
				)
				.ok()
			},
			_ => {
				// No-op
				None
			},
		};
	}

	fn lock_balance_on_unsuccessful_vote(who: &u64, ref_index: u32) -> Option<u64> {
		LAST_LOCKED_IF_UNSUCCESSFUL_VOTE_DATA.with(|data| {
			*data.borrow_mut() = Some((*who, ref_index.try_into().unwrap()));

			REMOVE_VOTE_LOCKED_AMOUNT.with(|data| *data.borrow())
		})
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn on_vote_worst_case(_who: &u64) {}

	#[cfg(feature = "runtime-benchmarks")]
	fn on_remove_vote_worst_case(_who: &u64) {}
}

#[derive(
	Encode, Debug, Decode, DecodeWithMemTracking, TypeInfo, Eq, PartialEq, Clone, MaxEncodedLen,
)]
pub struct Tally {
	pub ayes: u32,
	pub nays: u32,
}

impl<Class> VoteTally<u32, Class> for Tally {
	fn new(_: Class) -> Self {
		Self { ayes: 0, nays: 0 }
	}

	fn ayes(&self, _: Class) -> u32 {
		self.ayes
	}

	fn support(&self, _: Class) -> Perbill {
		Perbill::from_percent(self.ayes)
	}

	fn approval(&self, _: Class) -> Perbill {
		if self.ayes + self.nays > 0 {
			Perbill::from_rational(self.ayes, self.ayes + self.nays)
		} else {
			Perbill::zero()
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn unanimity(_: Class) -> Self {
		Self { ayes: 100, nays: 0 }
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn rejection(_: Class) -> Self {
		Self { ayes: 0, nays: 100 }
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn from_requirements(support: Perbill, approval: Perbill, _: Class) -> Self {
		let ayes = support.mul_ceil(100u32);
		let nays = ((ayes as u64) * 1_000_000_000u64 / approval.deconstruct() as u64) as u32 - ayes;
		Self { ayes, nays }
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn setup(_: Class, _: Perbill) {}
}

//Define some accounts and use them
pub const ALICE: AccountId = 10;
pub const BOB: AccountId = 11;
pub const DAVE: AccountId = 12;
pub const EVE: AccountId = 13;
pub const PROJECT101: AccountId = 101;
pub const PROJECT102: AccountId = 102;
pub const PROJECT103: AccountId = 103;
pub const PROJECT104: AccountId = 104;
pub const PROJECT105: AccountId = 105;
pub const BSX: Balance = 100_000_000_000;

pub fn expect_events(e: Vec<RuntimeEvent>) {
	e.into_iter().for_each(frame_system::Pallet::<Test>::assert_has_event);
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let pot_account = PotId::get().into_account_truncating();

	pallet_balances::GenesisConfig::<Test> {
		balances: vec![
			(ALICE, 200_000 * BSX),
			(BOB, 200_000 * BSX),
			(DAVE, 150_000 * BSX),
			(EVE, 150_000 * BSX),
			(PROJECT101, 150_000 * BSX),
			(PROJECT102, 150_000 * BSX),
			(PROJECT103, 150_000 * BSX),
			(PROJECT104, 150_000 * BSX),
			(PROJECT105, 150_000 * BSX),
			(pot_account, 150_000_000 * BSX),
		],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}
