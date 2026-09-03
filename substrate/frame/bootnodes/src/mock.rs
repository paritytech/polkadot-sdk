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

//! Test runtime for `pallet_bootnodes`.

use crate as pallet_bootnodes;
use frame_support::{derive_impl, parameter_types, traits::ConstU32};
use frame_system::EnsureRoot;
use sp_core::H256;
use sp_keystore::{testing::MemoryKeystore, KeystoreExt};
use sp_runtime::BuildStorage;

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Bootnodes: pallet_bootnodes,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

pub const PARA_ID: u32 = 2_000;
pub const MAX_BOOTNODES: u32 = 4;
pub const MAX_ADDR_LEN: u32 = 128;

parameter_types! {
	pub JamGenesisHash: H256 = H256::repeat_byte(0x4a);
}

impl pallet_bootnodes::Config for Test {
	type AdminOrigin = EnsureRoot<u64>;
	type ParaId = ConstU32<PARA_ID>;
	type JamGenesisHash = JamGenesisHash;
	type MaxBootnodes = ConstU32<MAX_BOOTNODES>;
	type MaxAddrLen = ConstU32<MAX_ADDR_LEN>;
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext = sp_io::TestExternalities::new(storage);
	// The default `BenchmarkHelper` signs through the keystore.
	ext.register_extension(KeystoreExt::new(MemoryKeystore::new()));
	ext.execute_with(|| System::set_block_number(1));
	ext
}
