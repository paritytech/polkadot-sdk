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

use std::collections::BTreeMap;

use sp_core::crypto::key_types::DUMMY;
use sp_runtime::{impl_opaque_keys, testing::UintAuthorityId, BuildStorage};
use sp_staking::SessionIndex;
use sp_state_machine::BasicExternalities;

use frame_support::{
	derive_impl, parameter_types, traits::{ConstU64, WithdrawReasons, Currency, ReservableCurrency, 
	SignedImbalance, NamedReservableCurrency, BalanceStatus}
};

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

#[cfg(feature = "historical")]
frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Session: pallet_session,
		Historical: pallet_session_historical,
	}
);

#[cfg(not(feature = "historical"))]
frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Session: pallet_session,
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
	// Stores if `on_before_session_end` was called
	pub static BeforeSessionEndCalled: bool = false;
	pub static ValidatorAccounts: BTreeMap<u64, u64> = BTreeMap::new();
	pub static CurrencyBalance: u64 = 100;
	pub const KeyDeposit: u64 = 10;
	// Track reserved balances for test accounts - use Vecs for simplicity
	pub static ReservedBalances: BTreeMap<u64, BTreeMap<Vec<u8>, u64>> = BTreeMap::new();
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
			// If there was a disabled validator, underlying conditions have changed
			// so we return `Some`.
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
	let keys: Vec<_> = NextValidators::get()
		.iter()
		.cloned()
		.map(|i| (i, i, UintAuthorityId(i).into()))
		.collect();
	BasicExternalities::execute_with_storage(&mut t, || {
		for (ref k, ..) in &keys {
			frame_system::Pallet::<Test>::inc_providers(k);
		}
		frame_system::Pallet::<Test>::inc_providers(&4);
		// An additional identity that we use.
		frame_system::Pallet::<Test>::inc_providers(&69);
	});
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

// Disabling threshold for `UpToLimitDisablingStrategy` and
// `UpToLimitWithReEnablingDisablingStrategy``
pub(crate) const DISABLING_LIMIT_FACTOR: usize = 3;

// Type to represent session keys in the test
pub type SessionKeysId = [u8; 12];
pub const TEST_SESSION_KEYS_ID: SessionKeysId = *b"session_keys";

// Define TestReserveIdentifier with the necessary traits
#[derive(Debug, Clone, PartialEq, Eq, codec::Encode, codec::Decode, scale_info::TypeInfo)]
pub struct TestReserveIdentifier;
impl Get<SessionKeysId> for TestReserveIdentifier {
	fn get() -> SessionKeysId {
		TEST_SESSION_KEYS_ID
	}
}

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
	type ReserveIdentifier = TestReserveIdentifier;
	type KeyDeposit = KeyDeposit;
}

#[cfg(feature = "historical")]
impl crate::historical::Config for Test {
	type FullIdentification = u64;
	type FullIdentificationOf = sp_runtime::traits::ConvertInto;
}

pub mod pallet_balances {
	use super::*;
	use frame_support::pallet_prelude::*;

	pub struct Pallet<T>(core::marker::PhantomData<T>);

	impl<T> Currency<u64> for Pallet<T> {
		type Balance = u64;
		type PositiveImbalance = ();
		type NegativeImbalance = ();

		fn total_balance(_: &u64) -> Self::Balance {
			CurrencyBalance::get()
		}

		fn can_slash(_: &u64, _: Self::Balance) -> bool {
			true
		}

		fn total_issuance() -> Self::Balance {
			0
		}

		fn minimum_balance() -> Self::Balance {
			0
		}

		fn burn(_: Self::Balance) -> Self::PositiveImbalance {
			()
		}

		fn issue(_: Self::Balance) -> Self::NegativeImbalance {
			()
		}

		fn free_balance(_: &u64) -> Self::Balance {
			CurrencyBalance::get()
		}

		fn ensure_can_withdraw(
			_: &u64,
			_: Self::Balance,
			_: WithdrawReasons,
			_: Self::Balance,
		) -> DispatchResult {
			Ok(())
		}

		fn transfer(
			_: &u64,
			_: &u64,
			_: Self::Balance,
			_: frame_support::traits::ExistenceRequirement,
		) -> DispatchResult {
			Ok(())
		}

		fn slash(_: &u64, _: Self::Balance) -> (Self::NegativeImbalance, Self::Balance) {
			((), 0)
		}

		fn deposit_into_existing(_: &u64, _: Self::Balance) -> Result<Self::PositiveImbalance, DispatchError> {
			Ok(())
		}

		fn deposit_creating(_: &u64, _: Self::Balance) -> Self::PositiveImbalance {
			()
		}

		fn withdraw(
			_: &u64,
			_: Self::Balance,
			_: WithdrawReasons,
			_: frame_support::traits::ExistenceRequirement,
		) -> Result<Self::NegativeImbalance, DispatchError> {
			Ok(())
		}

		fn make_free_balance_be(
			_: &u64,
			_: Self::Balance,
		) -> SignedImbalance<Self::Balance, Self::PositiveImbalance> {
			frame_support::traits::SignedImbalance::Positive(())
		}
	}

	impl<T> ReservableCurrency<u64> for Pallet<T> {
		fn can_reserve(who: &u64, amount: Self::Balance) -> bool {
			// Account 999 is special and always has insufficient funds for testing
			if *who == 999 {
				return false
			}
			CurrencyBalance::get() >= amount
		}

		fn reserved_balance(who: &u64) -> Self::Balance {
			// Sum up all reserved balances for the account
			ReservedBalances::get()
				.get(who)
				.map(|reserves| reserves.values().sum())
				.unwrap_or(0)
		}

		fn reserve(who: &u64, amount: Self::Balance) -> DispatchResult {
			if !Self::can_reserve(who, amount) {
				return Err(DispatchError::Other("InsufficientBalance"))
			}
			
			// Use an empty ID for anonymous reserves
			let id = Vec::new();
			
			// Update the reserved balance
			ReservedBalances::mutate(|balances| {
				let account_reserves = balances.entry(*who).or_insert_with(BTreeMap::new);
				let reserved = account_reserves.entry(id).or_insert(0);
				*reserved += amount;
			});
			
			Ok(())
		}

		fn unreserve(who: &u64, amount: Self::Balance) -> Self::Balance {
			// Use an empty ID for anonymous reserves
			let id = Vec::new();
			
			// Get the current reserved amount
			let mut remaining = amount;
			ReservedBalances::mutate(|balances| {
				if let Some(account_reserves) = balances.get_mut(who) {
					if let Some(reserved) = account_reserves.get_mut(&id) {
						if *reserved >= amount {
							*reserved -= amount;
							remaining = 0;
						} else {
							remaining = amount - *reserved;
							*reserved = 0;
						}
						
						// Clean up empty reserves
						if *reserved == 0 {
							account_reserves.remove(&id);
						}
					}
					
					// Clean up empty accounts
					if account_reserves.is_empty() {
						balances.remove(who);
					}
				}
			});
			
			remaining
		}

		fn slash_reserved(_: &u64, _: Self::Balance) -> (Self::NegativeImbalance, Self::Balance) {
			((), 0)
		}

		fn repatriate_reserved(
			_: &u64,
			_: &u64,
			_: Self::Balance,
			_: frame_support::traits::BalanceStatus,
		) -> Result<Self::Balance, DispatchError> {
			Ok(0)
		}
	}
	
	impl<T> NamedReservableCurrency<u64> for Pallet<T> {
		type ReserveIdentifier = [u8; 12];
		
		fn reserved_balance_named(id: &Self::ReserveIdentifier, who: &u64) -> Self::Balance {
			// Convert fixed array to Vec for lookup
			let id_vec = id.to_vec();
			
			ReservedBalances::get()
				.get(who)
				.and_then(|reserves| reserves.get(&id_vec))
				.cloned()
				.unwrap_or(0)
		}
		
		fn slash_reserved_named(
			_id: &Self::ReserveIdentifier,
			_who: &u64,
			_amount: Self::Balance,
		) -> (Self::NegativeImbalance, Self::Balance) {
			((), 0)
		}
		
		fn reserve_named(
			id: &Self::ReserveIdentifier,
			who: &u64,
			amount: Self::Balance,
		) -> DispatchResult {
			if !Self::can_reserve(who, amount) {
				return Err(DispatchError::Other("InsufficientBalance"))
			}
			
			// Convert fixed array to Vec for storage
			let id_vec = id.to_vec();
			
			// Update the reserved balance
			ReservedBalances::mutate(|balances| {
				let account_reserves = balances.entry(*who).or_insert_with(BTreeMap::new);
				let reserved = account_reserves.entry(id_vec).or_insert(0);
				*reserved += amount;
			});
			
			Ok(())
		}
		
		fn unreserve_named(
			id: &Self::ReserveIdentifier,
			who: &u64,
			amount: Self::Balance,
		) -> Self::Balance {
			// Convert fixed array to Vec for lookup/storage
			let id_vec = id.to_vec();
			
			// Get the current reserved amount
			let mut remaining = amount;
			ReservedBalances::mutate(|balances| {
				if let Some(account_reserves) = balances.get_mut(who) {
					if let Some(reserved) = account_reserves.get_mut(&id_vec) {
						if *reserved >= amount {
							*reserved -= amount;
							remaining = 0;
						} else {
							remaining = amount - *reserved;
							*reserved = 0;
						}
						
						// Clean up empty reserves
						if *reserved == 0 {
							account_reserves.remove(&id_vec);
						}
					}
					
					// Clean up empty accounts
					if account_reserves.is_empty() {
						balances.remove(who);
					}
				}
			});
			
			remaining
		}
		
		fn repatriate_reserved_named(
			_id: &Self::ReserveIdentifier,
			_slashed: &u64,
			_beneficiary: &u64,
			_amount: Self::Balance,
			_status: BalanceStatus,
		) -> Result<Self::Balance, DispatchError> {
			Ok(0)
		}
	}
}
