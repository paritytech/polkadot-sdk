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

#![cfg(test)]

use super::*;
use crate::ReferendaPrecompile;

use alloc::borrow::Cow;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::TypeInfo;
use frame_support::traits::{QueryPreimage, StorePreimage};
use frame_support::{
	assert_ok, derive_impl, ord_parameter_types, parameter_types,
	traits::{
		ConstU128, ConstU32, ConstU64, Contains, EqualPrivilegeOnly, OnInitialize, OriginTrait,
		VoteTally,
	},
	weights::Weight,
};
use frame_system::{EnsureRoot, EnsureSignedBy};
use pallet_referenda::{Curve, Track, TrackInfo, TracksInfo};
use sp_runtime::{
	str_array as s,
	traits::{BlakeTwo256, Hash, IdentityLookup},
	AccountId32, BuildStorage, Perbill,
};

type Block = frame_system::mocking::MockBlock<Test>;

// ========== types =========
pub type AccountId = AccountId32;
pub type Balance = u128;

pub const ALICE: AccountId32 = AccountId32::new([1u8; 32]);
pub const BOB: AccountId32 = AccountId32::new([2u8; 32]);
pub const CHARLIE: AccountId32 = AccountId32::new([3u8; 32]);
pub const DAVE: AccountId32 = AccountId32::new([4u8; 32]);
pub const EVE: AccountId32 = AccountId32::new([5u8; 32]);
pub const FERDIE: AccountId32 = AccountId32::new([6u8; 32]);
pub const POOR: AccountId32 = AccountId32::new([7u8; 32]);

// ========== runtime =========
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		Timestamp: pallet_timestamp,
		Preimage: pallet_preimage,
		Scheduler: pallet_scheduler,
		Referenda: pallet_referenda,
		Revive: pallet_revive,

	}
);

// ========== struct and parameters =========
pub struct TestTracksInfo;

pub struct BaseFilter;

#[derive(
	Encode, Debug, Decode, DecodeWithMemTracking, TypeInfo, Eq, PartialEq, Clone, MaxEncodedLen,
)]
pub struct Tally {
	pub ayes: u32,
	pub nays: u32,
}

parameter_types! {
	pub const MinimumPeriod: u64 = 1;
	pub MaxWeight: Weight = Weight::from_parts(2_000_000_000_000, u64::MAX);
	pub ExistentialDeposit: Balance = 1;
	pub static AlarmInterval: u64 = 1;
}

ord_parameter_types! {
	pub const FourAccount: AccountId = AccountId32::new([4u8; 32]);
}

// ================ impl here ================

impl Contains<RuntimeCall> for BaseFilter {
	fn contains(_call: &RuntimeCall) -> bool {
		true
	}
}

impl pallet_timestamp::Config for Test {
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = MinimumPeriod;
	type WeightInfo = ();
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = BaseFilter;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type Balance = Balance;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
}

// Configure pallet-revive with our precompile
#[derive_impl(pallet_revive::config_preludes::TestDefaultConfig)]
impl pallet_revive::Config for Test {
	type AddressMapper = pallet_revive::AccountId32Mapper<Self>;
	type Balance = Balance;
	type Currency = Balances;
	type Precompiles = (ReferendaPrecompile<Self>,);
	type Time = Timestamp;
	type UploadOrigin = frame_system::EnsureSigned<AccountId>;
	type InstantiateOrigin = frame_system::EnsureSigned<AccountId>;
}

impl pallet_preimage::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type Currency = Balances;
	type ManagerOrigin = EnsureRoot<AccountId>;
	type Consideration = ();
}

impl pallet_scheduler::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeOrigin = RuntimeOrigin;
	type PalletsOrigin = OriginCaller;
	type RuntimeCall = RuntimeCall;
	type MaximumWeight = MaxWeight;
	type ScheduleOrigin = EnsureRoot<AccountId>;
	type MaxScheduledPerBlock = ConstU32<100>;
	type WeightInfo = ();
	type OriginPrivilegeCmp = EqualPrivilegeOnly;
	type Preimages = Preimage;
	type BlockNumberProvider = frame_system::Pallet<Test>;
}

impl pallet_referenda::Config for Test {
	type WeightInfo = ();
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type Scheduler = Scheduler;
	type Currency = pallet_balances::Pallet<Self>;
	type SubmitOrigin = frame_system::EnsureSigned<AccountId>;
	type CancelOrigin = EnsureSignedBy<FourAccount, AccountId>;
	type KillOrigin = EnsureRoot<AccountId>;
	type Slash = ();
	type Votes = u32;
	type Tally = Tally;
	type SubmissionDeposit = ConstU128<2>; // FIX 3: Changed from ConstU64 to ConstU128
	type MaxQueued = ConstU32<3>;
	type UndecidingTimeout = ConstU64<20>;
	type AlarmInterval = AlarmInterval;
	type Tracks = TestTracksInfo;
	type Preimages = Preimage;
	type BlockNumberProvider = System;
}

impl TracksInfo<Balance, u64> for TestTracksInfo {
	type Id = u8;
	type RuntimeOrigin = <RuntimeOrigin as OriginTrait>::PalletsOrigin;

	fn tracks() -> impl Iterator<Item = Cow<'static, Track<Self::Id, Balance, u64>>> {
		static DATA: [Track<u8, Balance, u64>; 3] = [
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
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(50),
						ceil: Perbill::from_percent(100),
					},
					min_support: Curve::LinearDecreasing {
						length: Perbill::from_percent(100),
						floor: Perbill::from_percent(0),
						ceil: Perbill::from_percent(100),
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
				frame_system::RawOrigin::Signed(_) => Ok(2),
				_ => Err(()),
			}
		} else {
			Err(())
		}
	}
}

// FIX 2: Changed from impl<Class> VoteTally<u32, Class> to impl VoteTally<u32, u8>
impl VoteTally<u32, u8> for Tally {
	fn new(_: u8) -> Self {
		Self { ayes: 0, nays: 0 }
	}

	fn ayes(&self, _: u8) -> u32 {
		self.ayes
	}

	fn support(&self, _: u8) -> Perbill {
		Perbill::from_percent(self.ayes)
	}

	fn approval(&self, _: u8) -> Perbill {
		if self.ayes + self.nays > 0 {
			Perbill::from_rational(self.ayes, self.ayes + self.nays)
		} else {
			Perbill::zero()
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn unanimity(_: u8) -> Self {
		Self { ayes: 100, nays: 0 }
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn rejection(_: u8) -> Self {
		Self { ayes: 0, nays: 100 }
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn from_requirements(support: Perbill, approval: Perbill, _: u8) -> Self {
		let ayes = support.mul_ceil(100u32);
		let nays = ((ayes as u64) * 1_000_000_000u64 / approval.deconstruct() as u64) as u32 - ayes;
		Self { ayes, nays }
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn setup(_: u8, _: Perbill) {}
}
// preimage
/// note a new preimage without registering.
pub fn note_preimage(who: AccountId32) -> <Test as frame_system::Config>::Hash {
	use std::sync::atomic::{AtomicU8, Ordering};
	// note a new preimage on every function invoke.
	static COUNTER: AtomicU8 = AtomicU8::new(0);
	let data = vec![COUNTER.fetch_add(1, Ordering::Relaxed)];
	assert_ok!(Preimage::note_preimage(RuntimeOrigin::signed(who), data.clone()));
	let hash = BlakeTwo256::hash(&data);
	assert!(!Preimage::is_requested(&hash));
	hash
}
// ====== helper functions =====
pub fn set_balance_proposal_bounded(value: u128) -> BoundedCallOf<Test, ()> {
	let who = AccountId32::new([42u8; 32]);
	let c =
		RuntimeCall::Balances(pallet_balances::Call::force_set_balance { who, new_free: value });
	<Preimage as StorePreimage>::bound(c).unwrap()
}

/// Create a proposal call encoded as bytes for inline submission
pub fn set_balance_proposal_bytes(value: u128) -> Vec<u8> {
	let who = AccountId32::new([42u8; 32]);
	let c =
		RuntimeCall::Balances(pallet_balances::Call::force_set_balance { who, new_free: value });
	c.encode()
}

// ====== transaction builder =====
pub struct ExtBuilder {}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self {}
	}
}

impl ExtBuilder {
	pub fn build(self) -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
		let balances = vec![
			(ALICE, 100),
			(BOB, 100),
			(CHARLIE, 100),
			(DAVE, 100),
			(EVE, 100),
			(FERDIE, 100),
			(POOR, 1),
		];
		pallet_balances::GenesisConfig::<Test> { balances, ..Default::default() }
			.assimilate_storage(&mut t)
			.unwrap();

		// Set up account mapping for EVM precompiles
		pallet_revive::GenesisConfig::<Test> {
			mapped_accounts: vec![ALICE, BOB, CHARLIE, POOR],
			..Default::default()
		}
		.assimilate_storage(&mut t)
		.unwrap();

		let mut ext = sp_io::TestExternalities::new(t);
		ext.execute_with(|| System::set_block_number(1));
		ext
	}

	pub fn build_and_execute(self, test: impl FnOnce()) {
		self.build().execute_with(|| {
			test();
			// Removed do_try_state() - it doesn't exist in pallet_referenda
		})
	}

	/// Submit a referendum directly via the pallet (not through precompile).
	/// Returns the referendum index.
	/// The referendum will NOT have a decision deposit placed.
	/// Must be called within `execute_with` context.
	pub fn submit_referendum(submitter: AccountId32) -> u32 {
		use frame_support::traits::schedule::DispatchTime;

		let proposal_origin =
			OriginCaller::system(frame_system::RawOrigin::Signed(submitter.clone()));
		let proposal = set_balance_proposal_bounded(100u128);
		let enactment_moment = DispatchTime::At(10u64.into());

		assert_ok!(pallet_referenda::Pallet::<Test>::submit(
			RuntimeOrigin::signed(submitter),
			Box::new(proposal_origin),
			proposal,
			enactment_moment,
		));

		let count = pallet_referenda::ReferendumCount::<Test>::get();
		assert!(count > 0, "Referendum should be created");
		count - 1 // Return the index (0-based)
	}

	/// Submit a referendum directly via the pallet and place decision deposit.
	/// Returns the referendum index.
	/// The referendum will have a decision deposit placed by the depositor.
	/// Must be called within `execute_with` context.
	pub fn submit_referendum_with_decision_deposit(
		submitter: AccountId32,
		depositor: AccountId32,
	) -> u32 {
		// First submit the referendum
		let referendum_index = Self::submit_referendum(submitter);

		// Then place the decision deposit
		assert_ok!(pallet_referenda::Pallet::<Test>::place_decision_deposit(
			RuntimeOrigin::signed(depositor),
			referendum_index,
		));

		// Verify deposit was placed
		let referendum_info = pallet_referenda::ReferendumInfoFor::<Test>::get(referendum_index);
		if let Some(pallet_referenda::ReferendumInfo::Ongoing(status)) = referendum_info {
			assert!(status.decision_deposit.is_some(), "Decision deposit should be placed");
		}

		referendum_index
	}
}
