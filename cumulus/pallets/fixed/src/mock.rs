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

//! # Preimage test environment.

use super::*;

use crate as fixed;
use frame_support::{
	ord_parameter_types, parameter_types,
	traits::{ConstBool, ConstU32, ConstU64, FindAuthor, ValidatorRegistration},
};
use frame_system as system;
use frame_system::EnsureSignedBy;
use sp_core::H256;
use sp_runtime::{
	testing::UintAuthorityId,
	traits::{BlakeTwo256, IdentityLookup, OpaqueKeys},
	BuildStorage, RuntimeAppPublic,
};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Timestamp: pallet_timestamp,
		Session: pallet_session,
		Aura: pallet_aura,
		Fixed: fixed,
		Authorship: pallet_authorship,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

impl system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
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
	type BlockHashCount = BlockHashCount;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = SS58Prefix;
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

pub struct Author4;
impl FindAuthor<u64> for Author4 {
	fn find_author<'a, I>(_digests: I) -> Option<u64>
	where
		I: 'a + IntoIterator<Item = (frame_support::ConsensusEngineId, &'a [u8])>,
	{
		Some(4)
	}
}

impl pallet_authorship::Config for Test {
	type FindAuthor = Author4;
	type EventHandler = Fixed;
}

impl pallet_timestamp::Config for Test {
	type Moment = u64;
	type OnTimestampSet = Aura;
	type MinimumPeriod = ConstU64<1>;
	type WeightInfo = ();
}

impl pallet_aura::Config for Test {
	type AuthorityId = sp_consensus_aura::sr25519::AuthorityId;
	type MaxAuthorities = ConstU32<100_000>;
	type DisabledValidators = ();
	type AllowMultipleBlocksPerSlot = ConstBool<false>;
	#[cfg(feature = "experimental")]
	type SlotDuration = pallet_aura::MinimumPeriodTimesTwo<Self>;
}

sp_runtime::impl_opaque_keys! {
	pub struct MockSessionKeys {
		// a key for aura authoring
		pub aura: UintAuthorityId,
	}
}

impl From<UintAuthorityId> for MockSessionKeys {
	fn from(aura: sp_runtime::testing::UintAuthorityId) -> Self {
		Self { aura }
	}
}

parameter_types! {
	pub static SessionHandlerCollators: Vec<u64> = Vec::new();
	pub static SessionChangeBlock: u64 = 0;
}

pub struct TestSessionHandler;
impl pallet_session::SessionHandler<u64> for TestSessionHandler {
	const KEY_TYPE_IDS: &'static [sp_runtime::KeyTypeId] = &[UintAuthorityId::ID];
	fn on_genesis_session<Ks: OpaqueKeys>(keys: &[(u64, Ks)]) {
		SessionHandlerCollators::set(keys.iter().map(|(a, _)| *a).collect::<Vec<_>>())
	}
	fn on_new_session<Ks: OpaqueKeys>(_: bool, keys: &[(u64, Ks)], _: &[(u64, Ks)]) {
		SessionChangeBlock::set(System::block_number());
		dbg!(keys.len());
		SessionHandlerCollators::set(keys.iter().map(|(a, _)| *a).collect::<Vec<_>>())
	}
	fn on_before_session_ending() {}
	fn on_disabled(_: u32) {}
}

parameter_types! {
	pub const Offset: u64 = 0;
	pub const Period: u64 = 10;
}

impl pallet_session::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	// we don't have stash and controller, thus we don't need the convert as well.
	type ValidatorIdOf = IdentityCollator;
	type ShouldEndSession = pallet_session::PeriodicSessions<Period, Offset>;
	type NextSessionRotation = pallet_session::PeriodicSessions<Period, Offset>;
	type SessionManager = Fixed;
	type SessionHandler = TestSessionHandler;
	type Keys = MockSessionKeys;
	type WeightInfo = ();
}

ord_parameter_types! {
	pub const RootAccount: u64 = 777;
}

pub struct IsRegistered;
impl ValidatorRegistration<u64> for IsRegistered {
	fn is_registered(id: &u64) -> bool {
		*id != 42u64
	}
}

impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type UpdateOrigin = EnsureSignedBy<RootAccount, u64>;
	type MaxCollators = ConstU32<20>;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type ValidatorIdOf = IdentityCollator;
	type ValidatorRegistration = IsRegistered;
	type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	sp_tracing::try_init_simple();
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let collators = vec![2, 1]; // unsorted

	let accounts = vec![1, 2, 3, 4, 5];
	let keys = accounts
		.into_iter()
		.map(|i| (i, i, MockSessionKeys { aura: UintAuthorityId(i) }))
		.collect::<Vec<_>>();

	let fixed_collators = fixed::GenesisConfig::<Test> { collators };
	let session = pallet_session::GenesisConfig::<Test> { keys };

	// fixed collator selection must be initialized before session.
	fixed_collators.assimilate_storage(&mut t).unwrap();
	session.assimilate_storage(&mut t).unwrap();

	t.into()
}

pub fn initialize_to_block(n: u64) {
	for i in System::block_number() + 1..=n {
		System::set_block_number(i);
		<AllPalletsWithSystem as frame_support::traits::OnInitialize<u64>>::on_initialize(i);
	}
}
