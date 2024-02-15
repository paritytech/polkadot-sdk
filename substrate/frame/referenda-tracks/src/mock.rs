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

//! Test environment for remarks pallet.

use crate as pallet_referenda_tracks;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	parameter_types,
	traits::{ConstU16, ConstU32, ConstU64, EnsureOriginWithArg, EqualPrivilegeOnly, VoteTally},
	weights::Weight,
};
use frame_system::EnsureRoot;
use pallet_referenda::{PalletsOriginOf, TrackIdOf, TrackInfoOf, TracksInfo};
use scale_info::TypeInfo;
use sp_core::H256;
use sp_io::TestExternalities;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage, Perbill,
};

pub type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
		Balances: pallet_balances,
		Preimage: pallet_preimage,
		Scheduler: pallet_scheduler,
		Referenda: pallet_referenda,
		Tracks: pallet_referenda_tracks,
	}
);

impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = ConstU64<250>;
	type DbWeight = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<u64>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ConstU16<42>;
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
}
impl pallet_preimage::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type Currency = Balances;
	type ManagerOrigin = EnsureRoot<u64>;
	type Consideration = ();
}
parameter_types! {
	pub MaxWeight: Weight = Weight::from_parts(2_000_000_000_000, u64::MAX);
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
impl pallet_balances::Config for Test {
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type MaxLocks = ConstU32<10>;
	type Balance = u64;
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ConstU64<1>;
	type AccountStore = System;
	type WeightInfo = ();
	type FreezeIdentifier = ();
	type MaxFreezes = ();
	type RuntimeHoldReason = ();
	type RuntimeFreezeReason = ();
	type MaxHolds = ();
}

pub struct EnsureOriginToTrack;
impl EnsureOriginWithArg<RuntimeOrigin, TrackIdOf<Test, ()>> for EnsureOriginToTrack {
	type Success = ();

	fn try_origin(
		o: RuntimeOrigin,
		id: &TrackIdOf<Test, ()>,
	) -> Result<Self::Success, RuntimeOrigin> {
		let track_id_for_origin: TrackIdOf<Test, ()> =
			Tracks::track_for(&o.clone().caller).map_err(|_| o.clone())?;
		frame_support::ensure!(&track_id_for_origin == id, o);

		Ok(())
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin(id: &TrackIdOf<Test, ()>) -> Result<RuntimeOrigin, ()> {
		let (origin, _) = <crate::OriginToTrackId<Test, ()>>::iter()
			.find(|(_, track_id)| id == track_id)
			.expect("track should exist");
		Ok(origin.into())
	}
}

#[cfg(feature = "runtime-benchmarks")]
pub struct BenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl crate::BenchmarkHelper<Test, ()> for BenchmarkHelper {
	fn track_id(id: u32) -> TrackIdOf<Test, ()> {
		id
	}
}

parameter_types! {
	pub const MaxTracks: u32 = u8::MAX as u32;
}
impl pallet_referenda_tracks::Config for Test {
	type TrackId = u32;
	type RuntimeEvent = RuntimeEvent;
	type MaxTracks = MaxTracks;
	type AdminOrigin = EnsureRoot<u64>;
	type UpdateOrigin = EnsureOriginToTrack;
	type WeightInfo = ();

	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = BenchmarkHelper;
}

parameter_types! {
	pub static AlarmInterval: u64 = 1;
}
impl pallet_referenda::Config for Test {
	type WeightInfo = ();
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type Scheduler = Scheduler;
	type Currency = pallet_balances::Pallet<Self>;
	type SubmitOrigin = frame_system::EnsureSigned<u64>;
	type CancelOrigin = EnsureRoot<u64>;
	type KillOrigin = EnsureRoot<u64>;
	type Slash = ();
	type Votes = u32;
	type Tally = Tally;
	type SubmissionDeposit = ConstU64<2>;
	type MaxQueued = ConstU32<3>;
	type UndecidingTimeout = ConstU64<20>;
	type AlarmInterval = AlarmInterval;
	type Tracks = Tracks;
	type Preimages = Preimage;
}

#[derive(Encode, Debug, Decode, TypeInfo, Eq, PartialEq, Clone, MaxEncodedLen)]
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
		Perbill::from_rational(self.ayes, 1.max(self.ayes + self.nays))
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
	fn from_requirements(_: Perbill, approval: Perbill, _: Class) -> Self {
		Self { ayes: approval * 100, nays: (Perbill::from_percent(100) - approval) * 100 }
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn setup(_: Class, _: Perbill) {}
}

pub fn new_test_ext(
	maybe_tracks: Option<Vec<(TrackIdOf<Test, ()>, TrackInfoOf<Test, ()>, PalletsOriginOf<Test>)>>,
) -> sp_io::TestExternalities {
	let balances = vec![(1, 100), (2, 100), (3, 100), (4, 100), (5, 100), (6, 100)];

	let t = RuntimeGenesisConfig {
		system: Default::default(),
		balances: pallet_balances::GenesisConfig::<Test> { balances },
	}
	.build_storage()
	.unwrap();

	let mut ext = TestExternalities::new(t);
	ext.execute_with(|| {
		System::set_block_number(1);

		if let Some(tracks) = maybe_tracks {
			for (id, info, pallet_origin) in tracks {
				crate::Pallet::<Test, ()>::insert(RuntimeOrigin::root(), id, info, pallet_origin)
					.expect("can insert track");
			}

			System::reset_events();
		} else {
		}
	});

	ext
}
