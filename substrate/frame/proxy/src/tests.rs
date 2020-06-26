// This file is part of Substrate.

// Copyright (C) 2019-2020 Parity Technologies (UK) Ltd.
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

// Tests for Proxy Pallet

#![cfg(test)]

use super::*;

use frame_support::{
	assert_ok, assert_noop, impl_outer_origin, parameter_types, impl_outer_dispatch,
	weights::Weight, impl_outer_event, RuntimeDebug, dispatch::DispatchError, traits::Filter,
};
use codec::{Encode, Decode};
use sp_core::H256;
use sp_runtime::{Perbill, traits::{BlakeTwo256, IdentityLookup}, testing::Header};
use crate as proxy;

impl_outer_origin! {
	pub enum Origin for Test where system = frame_system {}
}
impl_outer_event! {
	pub enum TestEvent for Test {
		system<T>,
		pallet_balances<T>,
		proxy<T>,
		pallet_utility,
	}
}
impl_outer_dispatch! {
	pub enum Call for Test where origin: Origin {
		frame_system::System,
		pallet_balances::Balances,
		proxy::Proxy,
		pallet_utility::Utility,
	}
}

// For testing the pallet, we construct most of a mock runtime. This means
// first constructing a configuration type (`Test`) which `impl`s each of the
// configuration traits of pallets we want to use.
#[derive(Clone, Eq, PartialEq)]
pub struct Test;
parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const MaximumBlockWeight: Weight = 1024;
	pub const MaximumBlockLength: u32 = 2 * 1024;
	pub const AvailableBlockRatio: Perbill = Perbill::one();
}
impl frame_system::Trait for Test {
	type BaseCallFilter = BaseFilter;
	type Origin = Origin;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Call = Call;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type Event = TestEvent;
	type BlockHashCount = BlockHashCount;
	type MaximumBlockWeight = MaximumBlockWeight;
	type DbWeight = ();
	type BlockExecutionWeight = ();
	type ExtrinsicBaseWeight = ();
	type MaximumExtrinsicWeight = MaximumBlockWeight;
	type MaximumBlockLength = MaximumBlockLength;
	type AvailableBlockRatio = AvailableBlockRatio;
	type Version = ();
	type ModuleToIndex = ();
	type AccountData = pallet_balances::AccountData<u64>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
}
parameter_types! {
	pub const ExistentialDeposit: u64 = 1;
}
impl pallet_balances::Trait for Test {
	type Balance = u64;
	type Event = TestEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
}
impl pallet_utility::Trait for Test {
	type Event = TestEvent;
	type Call = Call;
}
parameter_types! {
	pub const ProxyDepositBase: u64 = 1;
	pub const ProxyDepositFactor: u64 = 1;
	pub const MaxProxies: u16 = 4;
}
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Encode, Decode, RuntimeDebug)]
pub enum ProxyType {
	Any,
	JustTransfer,
	JustUtility,
}
impl Default for ProxyType { fn default() -> Self { Self::Any } }
impl InstanceFilter<Call> for ProxyType {
	fn filter(&self, c: &Call) -> bool {
		match self {
			ProxyType::Any => true,
			ProxyType::JustTransfer => matches!(c, Call::Balances(pallet_balances::Call::transfer(..))),
			ProxyType::JustUtility => matches!(c, Call::Utility(..)),
		}
	}
	fn is_superset(&self, o: &Self) -> bool {
		self == &ProxyType::Any || self == o
	}
}
pub struct BaseFilter;
impl Filter<Call> for BaseFilter {
	fn filter(c: &Call) -> bool {
		match *c {
			// Remark is used as a no-op call in the benchmarking
			Call::System(SystemCall::remark(_)) => true,
			Call::System(_) => false,
			_ => true,
		}
	}
}
impl Trait for Test {
	type Event = TestEvent;
	type Call = Call;
	type Currency = Balances;
	type ProxyType = ProxyType;
	type ProxyDepositBase = ProxyDepositBase;
	type ProxyDepositFactor = ProxyDepositFactor;
	type MaxProxies = MaxProxies;
}

type System = frame_system::Module<Test>;
type Balances = pallet_balances::Module<Test>;
type Utility = pallet_utility::Module<Test>;
type Proxy = Module<Test>;

use frame_system::Call as SystemCall;
use pallet_balances::Call as BalancesCall;
use pallet_balances::Error as BalancesError;
use pallet_balances::Event as BalancesEvent;
use pallet_utility::Call as UtilityCall;
use pallet_utility::Event as UtilityEvent;
use super::Call as ProxyCall;

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(1, 10), (2, 10), (3, 10), (4, 10), (5, 2)],
	}.assimilate_storage(&mut t).unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

fn last_event() -> TestEvent {
	system::Module::<Test>::events().pop().expect("Event expected").event
}

fn expect_event<E: Into<TestEvent>>(e: E) {
	assert_eq!(last_event(), e.into());
}

fn last_events(n: usize) -> Vec<TestEvent> {
	system::Module::<Test>::events().into_iter().rev().take(n).rev().map(|e| e.event).collect()
}

fn expect_events(e: Vec<TestEvent>) {
	assert_eq!(last_events(e.len()), e);
}

#[test]
fn filtering_works() {
	new_test_ext().execute_with(|| {
		Balances::mutate_account(&1, |a| a.free = 1000);
		assert_ok!(Proxy::add_proxy(Origin::signed(1), 2, ProxyType::Any));
		assert_ok!(Proxy::add_proxy(Origin::signed(1), 3, ProxyType::JustTransfer));
		assert_ok!(Proxy::add_proxy(Origin::signed(1), 4, ProxyType::JustUtility));

		let call = Box::new(Call::Balances(BalancesCall::transfer(6, 1)));
		assert_ok!(Proxy::proxy(Origin::signed(2), 1, None, call.clone()));
		expect_event(RawEvent::ProxyExecuted(Ok(())));
		assert_ok!(Proxy::proxy(Origin::signed(3), 1, None, call.clone()));
		expect_event(RawEvent::ProxyExecuted(Ok(())));
		assert_ok!(Proxy::proxy(Origin::signed(4), 1, None, call.clone()));
		expect_event(RawEvent::ProxyExecuted(Err(DispatchError::BadOrigin)));

		let derivative_id = Utility::derivative_account_id(1, 0);
		Balances::mutate_account(&derivative_id, |a| a.free = 1000);
		let inner = Box::new(Call::Balances(BalancesCall::transfer(6, 1)));

		let call = Box::new(Call::Utility(UtilityCall::as_derivative(0, inner.clone())));
		assert_ok!(Proxy::proxy(Origin::signed(2), 1, None, call.clone()));
		expect_event(RawEvent::ProxyExecuted(Ok(())));
		assert_ok!(Proxy::proxy(Origin::signed(3), 1, None, call.clone()));
		expect_event(RawEvent::ProxyExecuted(Err(DispatchError::BadOrigin)));
		assert_ok!(Proxy::proxy(Origin::signed(4), 1, None, call.clone()));
		expect_event(RawEvent::ProxyExecuted(Err(DispatchError::BadOrigin)));

		let call = Box::new(Call::Utility(UtilityCall::batch(vec![*inner])));
		assert_ok!(Proxy::proxy(Origin::signed(2), 1, None, call.clone()));
		expect_events(vec![UtilityEvent::BatchCompleted.into(), RawEvent::ProxyExecuted(Ok(())).into()]);
		assert_ok!(Proxy::proxy(Origin::signed(3), 1, None, call.clone()));
		expect_event(RawEvent::ProxyExecuted(Err(DispatchError::BadOrigin)));
		assert_ok!(Proxy::proxy(Origin::signed(4), 1, None, call.clone()));
		expect_events(vec![
			UtilityEvent::BatchInterrupted(0, DispatchError::BadOrigin).into(),
			RawEvent::ProxyExecuted(Ok(())).into(),
		]);

		let inner = Box::new(Call::Proxy(ProxyCall::add_proxy(5, ProxyType::Any)));
		let call = Box::new(Call::Utility(UtilityCall::batch(vec![*inner])));
		assert_ok!(Proxy::proxy(Origin::signed(2), 1, None, call.clone()));
		expect_events(vec![UtilityEvent::BatchCompleted.into(), RawEvent::ProxyExecuted(Ok(())).into()]);
		assert_ok!(Proxy::proxy(Origin::signed(3), 1, None, call.clone()));
		expect_event(RawEvent::ProxyExecuted(Err(DispatchError::BadOrigin)));
		assert_ok!(Proxy::proxy(Origin::signed(4), 1, None, call.clone()));
		expect_events(vec![
			UtilityEvent::BatchInterrupted(0, DispatchError::BadOrigin).into(),
			RawEvent::ProxyExecuted(Ok(())).into(),
		]);

		let call = Box::new(Call::Proxy(ProxyCall::remove_proxies()));
		assert_ok!(Proxy::proxy(Origin::signed(3), 1, None, call.clone()));
		expect_event(RawEvent::ProxyExecuted(Err(DispatchError::BadOrigin)));
		assert_ok!(Proxy::proxy(Origin::signed(4), 1, None, call.clone()));
		expect_event(RawEvent::ProxyExecuted(Err(DispatchError::BadOrigin)));
		assert_ok!(Proxy::proxy(Origin::signed(2), 1, None, call.clone()));
		expect_events(vec![BalancesEvent::<Test>::Unreserved(1, 5).into(), RawEvent::ProxyExecuted(Ok(())).into()]);
	});
}

#[test]
fn add_remove_proxies_works() {
	new_test_ext().execute_with(|| {
		assert_ok!(Proxy::add_proxy(Origin::signed(1), 2, ProxyType::Any));
		assert_noop!(Proxy::add_proxy(Origin::signed(1), 2, ProxyType::Any), Error::<Test>::Duplicate);
		assert_eq!(Balances::reserved_balance(1), 2);
		assert_ok!(Proxy::add_proxy(Origin::signed(1), 2, ProxyType::JustTransfer));
		assert_eq!(Balances::reserved_balance(1), 3);
		assert_ok!(Proxy::add_proxy(Origin::signed(1), 3, ProxyType::Any));
		assert_eq!(Balances::reserved_balance(1), 4);
		assert_ok!(Proxy::add_proxy(Origin::signed(1), 4, ProxyType::JustUtility));
		assert_eq!(Balances::reserved_balance(1), 5);
		assert_noop!(Proxy::add_proxy(Origin::signed(1), 4, ProxyType::Any), Error::<Test>::TooMany);
		assert_noop!(Proxy::remove_proxy(Origin::signed(1), 3, ProxyType::JustTransfer), Error::<Test>::NotFound);
		assert_ok!(Proxy::remove_proxy(Origin::signed(1), 4, ProxyType::JustUtility));
		assert_eq!(Balances::reserved_balance(1), 4);
		assert_ok!(Proxy::remove_proxy(Origin::signed(1), 3, ProxyType::Any));
		assert_eq!(Balances::reserved_balance(1), 3);
		assert_ok!(Proxy::remove_proxy(Origin::signed(1), 2, ProxyType::Any));
		assert_eq!(Balances::reserved_balance(1), 2);
		assert_ok!(Proxy::remove_proxy(Origin::signed(1), 2, ProxyType::JustTransfer));
		assert_eq!(Balances::reserved_balance(1), 0);
	});
}

#[test]
fn cannot_add_proxy_without_balance() {
	new_test_ext().execute_with(|| {
		assert_ok!(Proxy::add_proxy(Origin::signed(5), 3, ProxyType::Any));
		assert_eq!(Balances::reserved_balance(5), 2);
		assert_noop!(
			Proxy::add_proxy(Origin::signed(5), 4, ProxyType::Any),
			BalancesError::<Test, _>::InsufficientBalance
		);
	});
}

#[test]
fn proxying_works() {
	new_test_ext().execute_with(|| {
		assert_ok!(Proxy::add_proxy(Origin::signed(1), 2, ProxyType::JustTransfer));
		assert_ok!(Proxy::add_proxy(Origin::signed(1), 3, ProxyType::Any));

		let call = Box::new(Call::Balances(BalancesCall::transfer(6, 1)));
		assert_noop!(Proxy::proxy(Origin::signed(4), 1, None, call.clone()), Error::<Test>::NotProxy);
		assert_noop!(
			Proxy::proxy(Origin::signed(2), 1, Some(ProxyType::Any), call.clone()),
			Error::<Test>::NotProxy
		);
		assert_ok!(Proxy::proxy(Origin::signed(2), 1, None, call.clone()));
		expect_event(RawEvent::ProxyExecuted(Ok(())));
		assert_eq!(Balances::free_balance(6), 1);

		let call = Box::new(Call::System(SystemCall::set_code(vec![])));
		assert_ok!(Proxy::proxy(Origin::signed(3), 1, None, call.clone()));
		expect_event(RawEvent::ProxyExecuted(Err(DispatchError::BadOrigin)));

		let call = Box::new(Call::Balances(BalancesCall::transfer_keep_alive(6, 1)));
		assert_ok!(Call::Proxy(super::Call::proxy(1, None, call.clone())).dispatch(Origin::signed(2)));
		expect_event(RawEvent::ProxyExecuted(Err(DispatchError::BadOrigin)));
		assert_ok!(Proxy::proxy(Origin::signed(3), 1, None, call.clone()));
		expect_event(RawEvent::ProxyExecuted(Ok(())));
		assert_eq!(Balances::free_balance(6), 2);
	});
}

#[test]
fn anonymous_works() {
	new_test_ext().execute_with(|| {
		assert_ok!(Proxy::anonymous(Origin::signed(1), ProxyType::Any, 0));
		let anon = Proxy::anonymous_account(&1, &ProxyType::Any, 0, None);
		expect_event(RawEvent::AnonymousCreated(anon.clone(), 1, ProxyType::Any, 0));

		// other calls to anonymous allowed as long as they're not exactly the same.
		assert_ok!(Proxy::anonymous(Origin::signed(1), ProxyType::JustTransfer, 0));
		assert_ok!(Proxy::anonymous(Origin::signed(1), ProxyType::Any, 1));
		let anon2 = Proxy::anonymous_account(&2, &ProxyType::Any, 0, None);
		assert_ok!(Proxy::anonymous(Origin::signed(2), ProxyType::Any, 0));
		assert_noop!(Proxy::anonymous(Origin::signed(1), ProxyType::Any, 0), Error::<Test>::Duplicate);
		System::set_extrinsic_index(1);
		assert_ok!(Proxy::anonymous(Origin::signed(1), ProxyType::Any, 0));
		System::set_extrinsic_index(0);
		System::set_block_number(2);
		assert_ok!(Proxy::anonymous(Origin::signed(1), ProxyType::Any, 0));

		let call = Box::new(Call::Balances(BalancesCall::transfer(6, 1)));
		assert_ok!(Balances::transfer(Origin::signed(3), anon, 5));
		assert_ok!(Proxy::proxy(Origin::signed(1), anon, None, call));
		expect_event(RawEvent::ProxyExecuted(Ok(())));
		assert_eq!(Balances::free_balance(6), 1);

		let call = Box::new(Call::Proxy(ProxyCall::kill_anonymous(1, ProxyType::Any, 0, 1, 0)));
		assert_ok!(Proxy::proxy(Origin::signed(2), anon2, None, call.clone()));
		let de = DispatchError::from(Error::<Test>::NoPermission).stripped();
		expect_event(RawEvent::ProxyExecuted(Err(de)));
		assert_noop!(
			Proxy::kill_anonymous(Origin::signed(1), 1, ProxyType::Any, 0, 1, 0),
			Error::<Test>::NoPermission
		);
		assert_eq!(Balances::free_balance(1), 0);
		assert_ok!(Proxy::proxy(Origin::signed(1), anon, None, call.clone()));
		assert_eq!(Balances::free_balance(1), 2);
		assert_noop!(Proxy::proxy(Origin::signed(1), anon, None, call.clone()), Error::<Test>::NotProxy);
	});
}
