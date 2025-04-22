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

//! Mock helpers for Session.

use super::*;
use crate as pallet_session;
#[cfg(feature = "historical")]
use crate::historical as pallet_session_historical;
use pallet_balances::{self, AccountData};

use std::collections::BTreeMap;

use sp_core::crypto::key_types::DUMMY;
use sp_runtime::{impl_opaque_keys, testing::UintAuthorityId, BuildStorage};
use sp_staking::SessionIndex;
use sp_state_machine::BasicExternalities;

use frame_support::{
	derive_impl, parameter_types,
	traits::{ConstU64, ConstU32, WithdrawReasons, Currency, ReservableCurrency, 
	SignedImbalance, StoredMap, tokens::{fungible::{
		hold::{Mutate as HoldMutate, Inspect as HoldInspect, Unbalanced as UnbalancedHold},
		Inspect as FungibleInspect, Unbalanced as FungibleUnbalanced, Dust
	}, Preservation, Fortitude}}, 
	traits::{
		KeyOwnerProofSystem, ValidatorSet, ValidatorSetWithIdentification,
	},
	pallet_prelude::*,
};
use scale_info::TypeInfo;
use sp_runtime::traits::{Convert, OpaqueKeys};
use frame_support::traits::VariantCount;

impl_opaque_keys! {
	pub struct MockSessionKeys {
		pub dummy: UintAuthorityId,
	}
}

impl From<UintAuthorityId> for MockSessionKeys {
	fn from(dummy: UintAuthorityId) -> Self {
		Self { dummy }
	}
}

pub const KEY_ID_A: KeyTypeId = KeyTypeId([4; 4]);
pub const KEY_ID_B: KeyTypeId = KeyTypeId([9; 4]);

#[derive(Debug, Clone, codec::Encode, codec::Decode, PartialEq, Eq)]
pub struct PreUpgradeMockSessionKeys {
	pub a: [u8; 32],
	pub b: [u8; 64],
}

impl OpaqueKeys for PreUpgradeMockSessionKeys {
	type KeyTypeIdProviders = ();

	fn key_ids() -> &'static [KeyTypeId] {
		&[KEY_ID_A, KEY_ID_B]
	}

	fn get_raw(&self, i: KeyTypeId) -> &[u8] {
		match i {
			i if i == KEY_ID_A => &self.a[..],
			i if i == KEY_ID_B => &self.b[..],
			_ => &[],
		}
	}
}

type Block = frame_system::mocking::MockBlock<Test>;

#[derive(
	Copy,
	Clone,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Encode,
	Decode,
	RuntimeDebug,
	MaxEncodedLen,
	TypeInfo,
	codec::DecodeWithMemTracking,
)]
pub enum MockHoldReason {
	SessionKeys,
}

impl VariantCount for MockHoldReason {
	const VARIANT_COUNT: u32 = 1;
}

impl Get<MockHoldReason> for MockHoldReason {
	fn get() -> MockHoldReason {
		MockHoldReason::SessionKeys
	}
}

#[cfg(feature = "historical")]
frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Session: pallet_session,
		Balances: pallet_balances,
		Historical: pallet_session_historical,
	}
);

#[cfg(not(feature = "historical"))]
frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Session: pallet_session,
		Balances: pallet_balances,
	}
);

parameter_types! {
	pub static Validators: Vec<u64> = vec![1, 2, 3];
	pub static NextValidators: Vec<u64> = vec![1, 2, 3];
	pub static Authorities: Vec<UintAuthorityId> =
		vec![UintAuthorityId(1), UintAuthorityId(2), UintAuthorityId(3)];
	pub static ForceSessionEnd: bool = false;
	pub static SessionLength: u64 = 2;
	pub static SessionChanged: bool = false;
	pub static TestSessionChanged: bool = false;
	pub static Disabled: bool = false;
	pub static BeforeSessionEndCalled: bool = false;
	pub static ValidatorAccounts: BTreeMap<u64, u64> = BTreeMap::new();
	pub static CurrencyBalance: u64 = 100;
	pub const KeyDeposit: u64 = 10;
	pub static ReservedBalances: BTreeMap<u64, BTreeMap<Vec<u8>, u64>> = BTreeMap::new();
	pub static ExistentialDeposit: u64 = 1;
}

pub struct TestShouldEndSession;
impl ShouldEndSession<u64> for TestShouldEndSession {
	fn should_end_session(now: u64) -> bool {
		let l = SessionLength::get();
		now % l == 0 ||
			ForceSessionEnd::mutate(|l| {
				let r = *l;
				*l = false;
				r
			})
	}
}

pub struct TestSessionHandler;
impl SessionHandler<u64> for TestSessionHandler {
	const KEY_TYPE_IDS: &'static [sp_runtime::KeyTypeId] = &[UintAuthorityId::ID];
	fn on_genesis_session<T: OpaqueKeys>(_validators: &[(u64, T)]) {}
	fn on_new_session<T: OpaqueKeys>(
		changed: bool,
		validators: &[(u64, T)],
		_queued_validators: &[(u64, T)],
	) {
		SessionChanged::mutate(|l| *l = changed);
		Authorities::mutate(|l| {
			*l = validators
				.iter()
				.map(|(_, id)| id.get::<UintAuthorityId>(DUMMY).unwrap_or_default())
				.collect()
		});
	}
	fn on_disabled(_validator_index: u32) {
		Disabled::mutate(|l| *l = true)
	}
	fn on_before_session_ending() {
		BeforeSessionEndCalled::mutate(|b| *b = true);
	}
}

pub struct TestSessionManager;
impl SessionManager<u64> for TestSessionManager {
	fn end_session(_: SessionIndex) {}
	fn start_session(_: SessionIndex) {}
	fn new_session(_: SessionIndex) -> Option<Vec<u64>> {
		if !TestSessionChanged::get() {
			Validators::mutate(|v| {
				*v = NextValidators::get().clone();
				Some(v.clone())
			})
		} else if Disabled::mutate(|l| std::mem::replace(&mut *l, false)) {
			Some(Validators::get().clone())
		} else {
			None
		}
	}
}

#[cfg(feature = "historical")]
impl crate::historical::SessionManager<u64, u64> for TestSessionManager {
	fn end_session(_: SessionIndex) {}
	fn start_session(_: SessionIndex) {}
	fn new_session(new_index: SessionIndex) -> Option<Vec<(u64, u64)>> {
		<Self as SessionManager<_>>::new_session(new_index)
			.map(|vals| vals.into_iter().map(|val| (val, val)).collect())
	}
}

pub fn authorities() -> Vec<UintAuthorityId> {
	Authorities::get().to_vec()
}

pub fn force_new_session() {
	ForceSessionEnd::mutate(|l| *l = true)
}

pub fn set_session_length(x: u64) {
	SessionLength::mutate(|l| *l = x)
}

pub fn session_changed() -> bool {
	SessionChanged::get()
}

pub fn set_next_validators(next: Vec<u64>) {
	NextValidators::mutate(|v| *v = next);
}

pub fn before_session_end_called() -> bool {
	BeforeSessionEndCalled::get()
}

pub fn reset_before_session_end_called() {
	BeforeSessionEndCalled::mutate(|b| *b = false);
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![
			(1, 100),
			(2, 100),
			(3, 100),
			(4, 100),
			(69, 100),
			(999, ExistentialDeposit::get()),
			(1000, 100),
		],
		dev_accounts: None,
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let keys: Vec<_> = NextValidators::get()
		.iter()
		.cloned()
		.map(|i| (i, i, UintAuthorityId(i).into()))
		.collect();
	BasicExternalities::execute_with_storage(&mut t, || {});
	pallet_session::GenesisConfig::<Test> { keys, ..Default::default() }
		.assimilate_storage(&mut t)
		.unwrap();

	let v = NextValidators::get().iter().map(|&i| (i, i)).collect();
	ValidatorAccounts::mutate(|m| *m = v);
	sp_io::TestExternalities::new(t)
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = AccountData<u64>;
	type RuntimeEvent = RuntimeEvent;
}

impl pallet_timestamp::Config for Test {
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = ConstU64<5>;
	type WeightInfo = ();
}

pub struct TestValidatorIdOf;
impl TestValidatorIdOf {
	pub fn set(v: BTreeMap<u64, u64>) {
		ValidatorAccounts::mutate(|m| *m = v);
	}
}
impl Convert<u64, Option<u64>> for TestValidatorIdOf {
	fn convert(x: u64) -> Option<u64> {
		ValidatorAccounts::get().get(&x).cloned()
	}
}

pub(crate) const DISABLING_LIMIT_FACTOR: usize = 3;

impl Config for Test {
	type ShouldEndSession = TestShouldEndSession;
	#[cfg(feature = "historical")]
	type SessionManager = crate::historical::NoteHistoricalRoot<Test, TestSessionManager>;
	#[cfg(not(feature = "historical"))]
	type SessionManager = TestSessionManager;
	type SessionHandler = TestSessionHandler;
	type ValidatorId = u64;
	type ValidatorIdOf = TestValidatorIdOf;
	type Keys = MockSessionKeys;
	type RuntimeEvent = RuntimeEvent;
	type NextSessionRotation = ();
	type DisablingStrategy =
		disabling::UpToLimitWithReEnablingDisablingStrategy<DISABLING_LIMIT_FACTOR>;
	type WeightInfo = ();
	type Currency = pallet_balances::Pallet<Test>;
	type HoldReason = MockHoldReason;
	type KeyDeposit = KeyDeposit;
}

#[cfg(feature = "historical")]
impl crate::historical::Config for Test {
	type FullIdentification = u64;
	type FullIdentificationOf = sp_runtime::traits::ConvertInto;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig as pallet_balances::DefaultConfig)]
impl pallet_balances::Config for Test {
	type Balance = u64;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type MaxReserves = ConstU32<2>;
	type ReserveIdentifier = ();
	type RuntimeHoldReason = MockHoldReason;
	type RuntimeFreezeReason = ();
	type FreezeIdentifier = ();
	type MaxFreezes = ConstU32<0>;
	type WeightInfo = ();
	type MaxLocks = ConstU32<50>;
	type DoneSlashHandler = ();
	type RuntimeEvent = RuntimeEvent;
}


