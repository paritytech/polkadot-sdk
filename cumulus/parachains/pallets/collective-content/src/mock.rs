// Copyright (C) 2023 Parity Technologies (UK) Ltd.
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

//! Test utilities.

pub use crate as pallet_collective_content;
use crate::WeightInfo;
use frame_support::{
	derive_impl, ord_parameter_types, parameter_types,
	traits::{ConstU32, ConstU64},
	weights::Weight,
};
use frame_system::EnsureSignedBy;
use sp_runtime::{traits::IdentityLookup, BuildStorage};

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		CollectiveContent: pallet_collective_content,
	}
);

type AccountId = u64;
type Block = frame_system::mocking::MockBlock<Test>;

ord_parameter_types! {
	pub const CharterManager: u64 = 1;
	pub const AnnouncementManager: u64 = 2;
	pub const SomeAccount: u64 = 3;
}

parameter_types! {
	pub const AnnouncementLifetime: u64 = 100;
	pub const MaxAnnouncements: u32 = 5;
}

impl pallet_collective_content::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type AnnouncementLifetime = AnnouncementLifetime;
	type AnnouncementOrigin = EnsureSignedBy<AnnouncementManager, AccountId>;
	type MaxAnnouncements = MaxAnnouncements;
	type CharterOrigin = EnsureSignedBy<CharterManager, AccountId>;
	type WeightInfo = CCWeightInfo;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = ();
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Block = Block;
	type Hash = sp_core::H256;
	type Hashing = sp_runtime::traits::BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
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
pub struct CCWeightInfo;
impl WeightInfo for CCWeightInfo {
	fn set_charter() -> Weight {
		Weight::zero()
	}
	fn announce() -> Weight {
		Weight::zero()
	}
	fn remove_announcement() -> Weight {
		Weight::zero()
	}
}

// Build test environment.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = RuntimeGenesisConfig::default().build_storage().unwrap().into();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

#[cfg(feature = "runtime-benchmarks")]
pub fn new_bench_ext() -> sp_io::TestExternalities {
	RuntimeGenesisConfig::default().build_storage().unwrap().into()
}
