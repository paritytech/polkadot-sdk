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

//! Mock runtime for scarcity pallet tests.

#![cfg(test)]

use crate as pallet_scarcity;
use frame_support::{
	derive_impl, parameter_types,
	traits::{ConstU32, ConstU64, UnixTime},
	weights::constants::RocksDbWeight,
};
use sp_runtime::{traits::IdentityLookup, BuildStorage};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system = 0,
		Scarcity: pallet_scarcity = 1,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Nonce = u64;
	type Block = Block;
	type BlockHashCount = ConstU64<250>;
	type DbWeight = RocksDbWeight;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
}

parameter_types! {
	pub static MockNow: u64 = 0;
}

/// Test-controlled Unix time source.
pub struct MockUnixTime;
impl UnixTime for MockUnixTime {
	fn now() -> core::time::Duration {
		core::time::Duration::from_secs(MockNow::get())
	}
}

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type UnixTime = MockUnixTime;
	type MaxStats = ConstU32<16>;
	type MaxMetadata = ConstU32<256>;
	type LockPeriod = ConstU64<60>;
	type MaxTransferPriority = ConstU64<1_000_000>;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext: sp_io::TestExternalities =
		frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into();
	ext.execute_with(|| {
		MockNow::set(0);
		System::set_block_number(1);
	});
	ext
}
