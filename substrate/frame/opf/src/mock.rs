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

//! Test environment for OPF pallet.
use crate as pallet_opf;
use crate::Convert;
use codec::{Decode, Encode};
pub use frame_support::{
	derive_impl, ord_parameter_types, parameter_types,
	traits::{
		ConstU32, ConstU64, EqualPrivilegeOnly, OnFinalize, OnInitialize, OriginTrait,
		SortedMembers, VoteTally,
	},
	weights::Weight,
	PalletId,
};
pub use frame_system::{EnsureRoot, EnsureSigned, EnsureSignedBy};
pub use sp_runtime::{
	traits::{AccountIdConversion, IdentityLookup},
	BuildStorage,
};
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
		Democracy: pallet_democracy,
	}
);

parameter_types! {
	pub MaxWeight: Weight = Weight::from_parts(2_000_000_000_000, u64::MAX);
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type AccountId = AccountId;
	type AccountData = pallet_balances::AccountData<Balance>;
	type Block = Block;
	type Lookup = IdentityLookup<Self::AccountId>;
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
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
}

parameter_types! {
	pub static PreimageByteDeposit: u64 = 0;
	pub static InstantAllowed: bool = false;
}
ord_parameter_types! {
	pub const One: u64 = 1;
	pub const Two: u64 = 2;
	pub const Three: u64 = 3;
	pub const Four: u64 = 4;
	pub const Five: u64 = 5;
	pub const Six: u64 = 6;
}

pub struct OneToFive;
impl SortedMembers<u64> for OneToFive {
	fn sorted_members() -> Vec<u64> {
		vec![1, 2, 3, 4, 5]
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn add(_m: &u64) {}
}

impl pallet_democracy::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Currency = pallet_balances::Pallet<Self>;
	type EnactmentPeriod = ConstU64<1>;
	type LaunchPeriod = ConstU64<2>;
	type VotingPeriod = ConstU64<2>;
	type VoteLockingPeriod = ConstU64<3>;
	type FastTrackVotingPeriod = ConstU64<2>;
	type MinimumDeposit = ConstU64<1>;
	type MaxDeposits = ConstU32<1000>;
	type MaxBlacklisted = ConstU32<5>;
	type SubmitOrigin = EnsureSigned<Self::AccountId>;
	type ExternalOrigin = EnsureSignedBy<Two, u64>;
	type ExternalMajorityOrigin = EnsureSignedBy<Three, u64>;
	type ExternalDefaultOrigin = EnsureSignedBy<One, u64>;
	type FastTrackOrigin = EnsureSignedBy<Five, u64>;
	type CancellationOrigin = EnsureSignedBy<Four, u64>;
	type BlacklistOrigin = EnsureRoot<u64>;
	type CancelProposalOrigin = EnsureRoot<u64>;
	type VetoOrigin = EnsureSignedBy<OneToFive, u64>;
	type CooloffPeriod = ConstU64<2>;
	type Slash = ();
	type InstantOrigin = EnsureSignedBy<Six, u64>;
	type InstantAllowed = InstantAllowed;
	type Scheduler = Scheduler;
	type MaxVotes = ConstU32<100>;
	type PalletsOrigin = OriginCaller;
	type WeightInfo = ();
	type MaxProposals = ConstU32<100>;
	type Preimages = Preimage;
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

//Define some accounts and use them
pub const ALICE: AccountId = 10;
pub const BOB: AccountId = 11;
pub const DAVE: AccountId = 12;
pub const EVE: AccountId = 13;
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
			(pot_account, 150_000_000 * BSX),
		],
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}
