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

use crate::{self as pallet_origin_and_gate, AndGate};
// Import pallet types directly
use frame_support::{
	derive_impl, parameter_types,
	traits::{ConstU32, ConstU64, EnsureOrigin, Everything},
};
use frame_system::{self as system, RawOrigin};
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage, Perbill,
};

type Block = frame_system::mocking::MockBlock<Test>;
pub type AccountId = u64;
pub type BlockNumber = u64;

// Custom origins for testing
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustomOriginType {
	Alice,
	Bob,
	None,
}

impl Default for CustomOriginType {
	fn default() -> Self {
		Self::None
	}
}

pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;
pub const CHARLIE: AccountId = 3;

// Origin identifiers
pub const ALICE_ORIGIN_ID: u8 = 1;
pub const BOB_ORIGIN_ID: u8 = 2;
pub const CHARLIE_ORIGIN_ID: u8 = 3;
pub const ROOT_ORIGIN_ID: u8 = 0;

// Custom origin checks if sender is Alice
pub struct AliceOrigin;

impl AliceOrigin {
	pub fn origin_type() -> CustomOriginType {
		CustomOriginType::Alice
	}
}

impl EnsureOrigin<RuntimeOrigin> for AliceOrigin {
	type Success = ();

	fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
		<RuntimeOrigin as Into<Result<RawOrigin<AccountId>, RuntimeOrigin>>>::into(o).and_then(
			|o| match o {
				RawOrigin::Signed(who) if who == ALICE => Ok(()),
				r => Err(RuntimeOrigin::from(r)),
			},
		)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
		let alice_origin = RuntimeOrigin::from(RawOrigin::Signed(ALICE));
		Ok(alice_origin)
	}
}

// Custom origin checks if sender is Bob
pub struct BobOrigin;

impl BobOrigin {
	pub fn origin_type() -> CustomOriginType {
		CustomOriginType::Bob
	}
}

impl EnsureOrigin<RuntimeOrigin> for BobOrigin {
	type Success = ();

	fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
		<RuntimeOrigin as Into<Result<RawOrigin<AccountId>, RuntimeOrigin>>>::into(o).and_then(
			|o| match o {
				RawOrigin::Signed(who) if who == BOB => Ok(()),
				r => Err(RuntimeOrigin::from(r)),
			},
		)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
		let bob_origin = RuntimeOrigin::from(RawOrigin::Signed(BOB));
		Ok(bob_origin)
	}
}

// Type aliases for origin wrappers for use in tests
pub type SignedByAlice = frame_system::EnsureSignedBy<AliceOrigin, AccountId>;
pub type SignedByBob = frame_system::EnsureSignedBy<BobOrigin, AccountId>;

// Define custom EnsureOrigin implementations for use in tests
pub type AliceAndBob = AndGate<AliceOrigin, BobOrigin>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		OriginAndGate: pallet_origin_and_gate,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const MaximumBlockWeight: u64 = 1024;
	pub const MaximumBlockLength: u32 = 2 * 1024;
	pub const AvailableBlockRatio: Perbill = Perbill::one();
	pub const SS58Prefix: u8 = 42;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl system::Config for Test {
	type BaseCallFilter = Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<u64>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
	type BlockHashCount = ConstU64<250>;
	type RuntimeTask = ();
	type PreInherents = ();
	type PostInherents = ();
	type PostTransactions = ();
	type SingleBlockMigrations = ();
	type MultiBlockMigrator = ();
	type ExtensionsWeightInfo = ();
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
}

impl Clone for MaxApprovals {
	fn clone(&self) -> Self {
		*self
	}
}

impl Copy for MaxApprovals {}

// Move to parameter_types instead
pub type OriginId = u8;

parameter_types! {
	pub const MaxApprovals: u32 = 2;
	// TODO: Add OriginId
	pub const ProposalLifetime: BlockNumber = 100;
}

impl pallet_origin_and_gate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type MaxApprovals = MaxApprovals;
	type Hashing = BlakeTwo256;
	type OriginId = OriginId;
	type ProposalLifetime = ProposalLifetime;
	type WeightInfo = ();
}

// This function basically just builds a genesis storage key/value store according to
// our desired mockup.
#[cfg(any(feature = "runtime-benchmarks", test))]
pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}
