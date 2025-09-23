// This file is part of Substrate.

// Copyright (C) 2020-2025 Acala Foundation.
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

use crate as pallet_oracle;

use frame_support::{
	construct_runtime, derive_impl, parameter_types,
	traits::{ConstU32, SortedMembers},
};
use sp_runtime::{traits::IdentityLookup, BuildStorage};

pub type AccountId = u128;
type Key = u32;
type Value = u32;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
}

parameter_types! {
	pub static TIME: u32 = 0;
	pub static MEMBERS: Vec<AccountId> = vec![1, 2, 3];
}

pub struct Timestamp;
impl Time for Timestamp {
	type Moment = u32;

	fn now() -> Self::Moment {
		TIME::get()
	}
}

impl Timestamp {
	pub fn set_timestamp(val: u32) {
		TIME::set(val);
	}
}

pub struct Members;

impl SortedMembers<AccountId> for Members {
	fn sorted_members() -> Vec<AccountId> {
		MEMBERS::get().clone()
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn add(who: &AccountId) {
		MEMBERS::mutate(|members| {
			members.push(*who);
			members.sort();
		})
	}
}
parameter_types! {
	pub const RootOperatorAccountId: AccountId = 4;
	pub const MaxFeedValues: u32 = 5;
}

impl Config for Test {
	type OnNewData = ();
	type CombineData = DefaultCombineData<Self, ConstU32<3>, ConstU32<600>>;
	type Time = Timestamp;
	type OracleKey = Key;
	type OracleValue = Value;
	type RootOperatorAccountId = RootOperatorAccountId;
	type Members = Members;
	type WeightInfo = ();
	type MaxHasDispatchedSize = ConstU32<100>;
	type MaxFeedValues = MaxFeedValues;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
	pub enum Test {
		System: frame_system,
		ModuleOracle: pallet_oracle,
	}
);

pub fn set_members(members: Vec<AccountId>) {
	MEMBERS::set(members);
}

// This function basically just builds a genesis storage key/value store
// according to our desired mockup.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

	let mut t: sp_io::TestExternalities = storage.into();

	t.execute_with(|| {
		Timestamp::set_timestamp(12345);
	});

	t
}
