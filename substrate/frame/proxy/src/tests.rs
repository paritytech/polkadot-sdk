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

// Tests for Proxy Pallet

#![cfg(test)]

use super::*;
use crate as proxy;
use alloc::{vec, vec::Vec};
use frame::testing_prelude::*;

type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
	pub struct Test {
		System: frame_system,
		Balances: pallet_balances,
		Proxy: proxy,
		Utility: pallet_utility,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type BaseCallFilter = BaseFilter;
	type AccountData = pallet_balances::AccountData<u64>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type ReserveIdentifier = [u8; 8];
	type AccountStore = System;
}

impl pallet_utility::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type PalletsOrigin = OriginCaller;
	type WeightInfo = ();
}

#[derive(
	Copy,
	Clone,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Encode,
	Decode,
	DecodeWithMemTracking,
	Debug,
	MaxEncodedLen,
	scale_info::TypeInfo,
)]
pub enum ProxyType {
	Any,
	JustTransfer,
	JustUtility,
}
impl Default for ProxyType {
	fn default() -> Self {
		Self::Any
	}
}
impl frame::traits::InstanceFilter<RuntimeCall> for ProxyType {
	fn filter(&self, c: &RuntimeCall) -> bool {
		match self {
			ProxyType::Any => true,
			ProxyType::JustTransfer => {
				matches!(
					c,
					RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death { .. })
				)
			},
			ProxyType::JustUtility => matches!(c, RuntimeCall::Utility { .. }),
		}
	}
	fn is_superset(&self, o: &Self) -> bool {
		self == &ProxyType::Any || self == o
	}
}
pub struct BaseFilter;
impl Contains<RuntimeCall> for BaseFilter {
	fn contains(c: &RuntimeCall) -> bool {
		match *c {
			// Remark is used as a no-op call in the benchmarking
			RuntimeCall::System(SystemCall::remark { .. }) => true,
			RuntimeCall::System(_) => false,
			_ => true,
		}
	}
}

parameter_types! {
	pub static ProxyDepositBase: u64 = 1;
	pub static ProxyDepositFactor: u64 = 1;
	pub static AnnouncementDepositBase: u64 = 1;
	pub static AnnouncementDepositFactor: u64 = 1;
}

impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type Currency = Balances;
	type ProxyType = ProxyType;
	type ProxyDepositBase = ProxyDepositBase;
	type ProxyDepositFactor = ProxyDepositFactor;
	type MaxProxies = ConstU32<4>;
	type WeightInfo = ();
	type CallHasher = BlakeTwo256;
	type MaxPending = ConstU32<2>;
	type AnnouncementDepositBase = AnnouncementDepositBase;
	type AnnouncementDepositFactor = AnnouncementDepositFactor;
	type BlockNumberProvider = frame_system::Pallet<Test>;
}

use super::{Call as ProxyCall, Event as ProxyEvent};
use frame_system::Call as SystemCall;
use pallet_balances::{Call as BalancesCall, Error as BalancesError, Event as BalancesEvent};
use pallet_utility::{Call as UtilityCall, Event as UtilityEvent};

type SystemError = frame_system::Error<Test>;

pub fn new_test_ext() -> TestState {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(1, 10), (2, 10), (3, 10), (4, 10), (5, 3)],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();
	let mut ext = TestState::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

/// Run `test` in a fresh externality, then assert the pallet invariants hold.
fn new_test_ext_and_execute(test: impl FnOnce()) {
	new_test_ext().execute_with(|| {
		test();
		Proxy::do_try_state().expect("All invariants must hold after each test");
	});
}

/// [`new_test_ext_and_execute`] for tests ending with a deposit left stale by a `*Deposit*`
/// parameter change, which the hook only warns about.
///
/// These tests change parameters, so the `fuzzing` premise that they are constant does not hold
/// and that one error is tolerated. Every other invariant is still asserted under both features.
fn new_test_ext_and_execute_with_stale_deposit(test: impl FnOnce()) {
	#[cfg(not(feature = "fuzzing"))]
	new_test_ext_and_execute(test);

	#[cfg(feature = "fuzzing")]
	new_test_ext().execute_with(|| {
		test();
		if let Err(e) = Proxy::do_try_state() {
			assert_eq!(
				e,
				TryRuntimeError::Other("Proxies deposit does not match the current parameters"),
				"All invariants but the stale deposit must hold after each test"
			);
		}
	});
}

fn last_events(n: usize) -> Vec<RuntimeEvent> {
	frame_system::Pallet::<Test>::events()
		.into_iter()
		.rev()
		.take(n)
		.rev()
		.map(|e| e.event)
		.collect()
}

fn expect_events(e: Vec<RuntimeEvent>) {
	assert_eq!(last_events(e.len()), e);
}

fn call_transfer(dest: u64, value: u64) -> RuntimeCall {
	RuntimeCall::Balances(BalancesCall::transfer_allow_death { dest, value })
}

#[test]
fn announcement_works() {
	new_test_ext_and_execute(|| {
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 3, ProxyType::Any, 1));
		System::assert_last_event(
			ProxyEvent::ProxyAdded {
				delegator: 1,
				delegatee: 3,
				proxy_type: ProxyType::Any,
				delay: 1,
			}
			.into(),
		);
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(2), 3, ProxyType::Any, 1));
		assert_eq!(Balances::reserved_balance(3), 0);

		assert_ok!(Proxy::announce(RuntimeOrigin::signed(3), 1, [1; 32].into()));
		let announcements = Announcements::<Test>::get(3);
		assert_eq!(
			announcements.0,
			vec![Announcement { real: 1, call_hash: [1; 32].into(), height: 1 }]
		);
		assert_eq!(Balances::reserved_balance(3), announcements.1);

		assert_ok!(Proxy::announce(RuntimeOrigin::signed(3), 2, [2; 32].into()));
		let announcements = Announcements::<Test>::get(3);
		assert_eq!(
			announcements.0,
			vec![
				Announcement { real: 1, call_hash: [1; 32].into(), height: 1 },
				Announcement { real: 2, call_hash: [2; 32].into(), height: 1 },
			]
		);
		assert_eq!(Balances::reserved_balance(3), announcements.1);

		assert_noop!(
			Proxy::announce(RuntimeOrigin::signed(3), 2, [3; 32].into()),
			Error::<Test>::TooMany
		);
	});
}

#[test]
fn remove_announcement_works() {
	new_test_ext_and_execute(|| {
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 3, ProxyType::Any, 1));
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(2), 3, ProxyType::Any, 1));
		assert_ok!(Proxy::announce(RuntimeOrigin::signed(3), 1, [1; 32].into()));
		assert_ok!(Proxy::announce(RuntimeOrigin::signed(3), 2, [2; 32].into()));
		let e = Error::<Test>::NotFound;
		assert_noop!(Proxy::remove_announcement(RuntimeOrigin::signed(3), 1, [0; 32].into()), e);
		assert_ok!(Proxy::remove_announcement(RuntimeOrigin::signed(3), 1, [1; 32].into()));
		let announcements = Announcements::<Test>::get(3);
		assert_eq!(
			announcements.0,
			vec![Announcement { real: 2, call_hash: [2; 32].into(), height: 1 }]
		);
		assert_eq!(Balances::reserved_balance(3), announcements.1);
	});
}

#[test]
fn reject_announcement_works() {
	new_test_ext_and_execute(|| {
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 3, ProxyType::Any, 1));
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(2), 3, ProxyType::Any, 1));
		assert_ok!(Proxy::announce(RuntimeOrigin::signed(3), 1, [1; 32].into()));
		assert_ok!(Proxy::announce(RuntimeOrigin::signed(3), 2, [2; 32].into()));
		let e = Error::<Test>::NotFound;
		assert_noop!(Proxy::reject_announcement(RuntimeOrigin::signed(1), 3, [0; 32].into()), e);
		let e = Error::<Test>::NotFound;
		assert_noop!(Proxy::reject_announcement(RuntimeOrigin::signed(4), 3, [1; 32].into()), e);
		assert_ok!(Proxy::reject_announcement(RuntimeOrigin::signed(1), 3, [1; 32].into()));
		let announcements = Announcements::<Test>::get(3);
		assert_eq!(
			announcements.0,
			vec![Announcement { real: 2, call_hash: [2; 32].into(), height: 1 }]
		);
		assert_eq!(Balances::reserved_balance(3), announcements.1);
	});
}

#[test]
fn announcer_must_be_proxy() {
	new_test_ext_and_execute(|| {
		assert_noop!(
			Proxy::announce(RuntimeOrigin::signed(2), 1, H256::zero()),
			Error::<Test>::NotProxy
		);
	});
}

#[test]
fn calling_proxy_doesnt_remove_announcement() {
	new_test_ext_and_execute(|| {
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 2, ProxyType::Any, 0));

		let call = Box::new(call_transfer(6, 1));
		let call_hash = BlakeTwo256::hash_of(&call);

		assert_ok!(Proxy::announce(RuntimeOrigin::signed(2), 1, call_hash));
		assert_ok!(Proxy::proxy(RuntimeOrigin::signed(2), 1, None, call));

		// The announcement is not removed by calling proxy.
		let announcements = Announcements::<Test>::get(2);
		assert_eq!(announcements.0, vec![Announcement { real: 1, call_hash, height: 1 }]);
	});
}

#[test]
fn delayed_requires_pre_announcement() {
	new_test_ext_and_execute(|| {
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 2, ProxyType::Any, 1));
		let call = Box::new(call_transfer(6, 1));
		let e = Error::<Test>::Unannounced;
		assert_noop!(Proxy::proxy(RuntimeOrigin::signed(2), 1, None, call.clone()), e);
		let e = Error::<Test>::Unannounced;
		assert_noop!(Proxy::proxy_announced(RuntimeOrigin::signed(0), 2, 1, None, call.clone()), e);
		let call_hash = BlakeTwo256::hash_of(&call);
		assert_ok!(Proxy::announce(RuntimeOrigin::signed(2), 1, call_hash));
		frame_system::Pallet::<Test>::set_block_number(2);
		assert_ok!(Proxy::proxy_announced(RuntimeOrigin::signed(0), 2, 1, None, call.clone()));
	});
}

#[test]
fn proxy_announced_removes_announcement_and_returns_deposit() {
	new_test_ext_and_execute(|| {
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 3, ProxyType::Any, 1));
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(2), 3, ProxyType::Any, 1));
		let call = Box::new(call_transfer(6, 1));
		let call_hash = BlakeTwo256::hash_of(&call);
		assert_ok!(Proxy::announce(RuntimeOrigin::signed(3), 1, call_hash));
		assert_ok!(Proxy::announce(RuntimeOrigin::signed(3), 2, call_hash));
		// Too early to execute announced call
		let e = Error::<Test>::Unannounced;
		assert_noop!(Proxy::proxy_announced(RuntimeOrigin::signed(0), 3, 1, None, call.clone()), e);

		frame_system::Pallet::<Test>::set_block_number(2);
		assert_ok!(Proxy::proxy_announced(RuntimeOrigin::signed(0), 3, 1, None, call.clone()));
		let announcements = Announcements::<Test>::get(3);
		assert_eq!(announcements.0, vec![Announcement { real: 2, call_hash, height: 1 }]);
		assert_eq!(Balances::reserved_balance(3), announcements.1);
	});
}

#[test]
fn filtering_works() {
	new_test_ext_and_execute(|| {
		Balances::make_free_balance_be(&1, 1000);
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 2, ProxyType::Any, 0));
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 3, ProxyType::JustTransfer, 0));
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 4, ProxyType::JustUtility, 0));

		let call = Box::new(call_transfer(6, 1));
		assert_ok!(Proxy::proxy(RuntimeOrigin::signed(2), 1, None, call.clone()));
		System::assert_last_event(ProxyEvent::ProxyExecuted { result: Ok(()) }.into());
		assert_ok!(Proxy::proxy(RuntimeOrigin::signed(3), 1, None, call.clone()));
		System::assert_last_event(ProxyEvent::ProxyExecuted { result: Ok(()) }.into());
		assert_ok!(Proxy::proxy(RuntimeOrigin::signed(4), 1, None, call.clone()));
		System::assert_last_event(
			ProxyEvent::ProxyExecuted { result: Err(SystemError::CallFiltered.into()) }.into(),
		);

		let derivative_id = pallet_utility::derivative_account_id(1, 0);
		Balances::make_free_balance_be(&derivative_id, 1000);
		let inner = Box::new(call_transfer(6, 1));

		let call = Box::new(RuntimeCall::Utility(UtilityCall::as_derivative {
			index: 0,
			call: inner.clone(),
		}));
		assert_ok!(Proxy::proxy(RuntimeOrigin::signed(2), 1, None, call.clone()));
		System::assert_last_event(ProxyEvent::ProxyExecuted { result: Ok(()) }.into());
		assert_ok!(Proxy::proxy(RuntimeOrigin::signed(3), 1, None, call.clone()));
		System::assert_last_event(
			ProxyEvent::ProxyExecuted { result: Err(SystemError::CallFiltered.into()) }.into(),
		);
		assert_ok!(Proxy::proxy(RuntimeOrigin::signed(4), 1, None, call.clone()));
		System::assert_last_event(
			ProxyEvent::ProxyExecuted { result: Err(SystemError::CallFiltered.into()) }.into(),
		);

		let call = Box::new(RuntimeCall::Utility(UtilityCall::batch { calls: vec![*inner] }));
		assert_ok!(Proxy::proxy(RuntimeOrigin::signed(2), 1, None, call.clone()));
		expect_events(vec![
			UtilityEvent::BatchCompleted.into(),
			ProxyEvent::ProxyExecuted { result: Ok(()) }.into(),
		]);
		assert_ok!(Proxy::proxy(RuntimeOrigin::signed(3), 1, None, call.clone()));
		System::assert_last_event(
			ProxyEvent::ProxyExecuted { result: Err(SystemError::CallFiltered.into()) }.into(),
		);
		assert_ok!(Proxy::proxy(RuntimeOrigin::signed(4), 1, None, call.clone()));
		expect_events(vec![
			UtilityEvent::BatchInterrupted { index: 0, error: SystemError::CallFiltered.into() }
				.into(),
			ProxyEvent::ProxyExecuted { result: Ok(()) }.into(),
		]);

		let inner = Box::new(RuntimeCall::Proxy(ProxyCall::new_call_variant_add_proxy(
			5,
			ProxyType::Any,
			0,
		)));
		let call = Box::new(RuntimeCall::Utility(UtilityCall::batch { calls: vec![*inner] }));
		assert_ok!(Proxy::proxy(RuntimeOrigin::signed(2), 1, None, call.clone()));
		expect_events(vec![
			UtilityEvent::BatchCompleted.into(),
			ProxyEvent::ProxyExecuted { result: Ok(()) }.into(),
		]);
		assert_ok!(Proxy::proxy(RuntimeOrigin::signed(3), 1, None, call.clone()));
		System::assert_last_event(
			ProxyEvent::ProxyExecuted { result: Err(SystemError::CallFiltered.into()) }.into(),
		);
		assert_ok!(Proxy::proxy(RuntimeOrigin::signed(4), 1, None, call.clone()));
		expect_events(vec![
			UtilityEvent::BatchInterrupted { index: 0, error: SystemError::CallFiltered.into() }
				.into(),
			ProxyEvent::ProxyExecuted { result: Ok(()) }.into(),
		]);

		let call = Box::new(RuntimeCall::Proxy(ProxyCall::remove_proxies {}));
		assert_ok!(Proxy::proxy(RuntimeOrigin::signed(3), 1, None, call.clone()));
		System::assert_last_event(
			ProxyEvent::ProxyExecuted { result: Err(SystemError::CallFiltered.into()) }.into(),
		);
		assert_ok!(Proxy::proxy(RuntimeOrigin::signed(4), 1, None, call.clone()));
		System::assert_last_event(
			ProxyEvent::ProxyExecuted { result: Err(SystemError::CallFiltered.into()) }.into(),
		);
		assert_ok!(Proxy::proxy(RuntimeOrigin::signed(2), 1, None, call.clone()));
		expect_events(vec![
			BalancesEvent::<Test>::Unreserved { who: 1, amount: 5 }.into(),
			ProxyEvent::ProxyRemoved {
				delegator: 1,
				delegatee: 2,
				proxy_type: ProxyType::Any,
				delay: 0,
			}
			.into(),
			ProxyEvent::ProxyRemoved {
				delegator: 1,
				delegatee: 3,
				proxy_type: ProxyType::JustTransfer,
				delay: 0,
			}
			.into(),
			ProxyEvent::ProxyRemoved {
				delegator: 1,
				delegatee: 4,
				proxy_type: ProxyType::JustUtility,
				delay: 0,
			}
			.into(),
			ProxyEvent::ProxyRemoved {
				delegator: 1,
				delegatee: 5,
				proxy_type: ProxyType::Any,
				delay: 0,
			}
			.into(),
			ProxyEvent::ProxyExecuted { result: Ok(()) }.into(),
		]);
	});
}

#[test]
fn add_remove_proxies_works() {
	new_test_ext_and_execute(|| {
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 2, ProxyType::Any, 0));
		assert_noop!(
			Proxy::add_proxy(RuntimeOrigin::signed(1), 2, ProxyType::Any, 0),
			Error::<Test>::Duplicate
		);
		assert_eq!(Balances::reserved_balance(1), 2);
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 2, ProxyType::JustTransfer, 0));
		assert_eq!(Balances::reserved_balance(1), 3);
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 3, ProxyType::Any, 0));
		assert_eq!(Balances::reserved_balance(1), 4);
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 4, ProxyType::JustUtility, 0));
		assert_eq!(Balances::reserved_balance(1), 5);
		assert_noop!(
			Proxy::add_proxy(RuntimeOrigin::signed(1), 4, ProxyType::Any, 0),
			Error::<Test>::TooMany
		);
		assert_noop!(
			Proxy::remove_proxy(RuntimeOrigin::signed(1), 3, ProxyType::JustTransfer, 0),
			Error::<Test>::NotFound
		);
		assert_ok!(Proxy::remove_proxy(RuntimeOrigin::signed(1), 4, ProxyType::JustUtility, 0));
		System::assert_last_event(
			ProxyEvent::ProxyRemoved {
				delegator: 1,
				delegatee: 4,
				proxy_type: ProxyType::JustUtility,
				delay: 0,
			}
			.into(),
		);
		assert_eq!(Balances::reserved_balance(1), 4);
		assert_ok!(Proxy::remove_proxy(RuntimeOrigin::signed(1), 3, ProxyType::Any, 0));
		assert_eq!(Balances::reserved_balance(1), 3);
		System::assert_last_event(
			ProxyEvent::ProxyRemoved {
				delegator: 1,
				delegatee: 3,
				proxy_type: ProxyType::Any,
				delay: 0,
			}
			.into(),
		);
		assert_ok!(Proxy::remove_proxy(RuntimeOrigin::signed(1), 2, ProxyType::Any, 0));
		assert_eq!(Balances::reserved_balance(1), 2);
		System::assert_last_event(
			ProxyEvent::ProxyRemoved {
				delegator: 1,
				delegatee: 2,
				proxy_type: ProxyType::Any,
				delay: 0,
			}
			.into(),
		);
		assert_ok!(Proxy::remove_proxy(RuntimeOrigin::signed(1), 2, ProxyType::JustTransfer, 0));
		assert_eq!(Balances::reserved_balance(1), 0);
		System::assert_last_event(
			ProxyEvent::ProxyRemoved {
				delegator: 1,
				delegatee: 2,
				proxy_type: ProxyType::JustTransfer,
				delay: 0,
			}
			.into(),
		);
		assert_noop!(
			Proxy::add_proxy(RuntimeOrigin::signed(1), 1, ProxyType::Any, 0),
			Error::<Test>::NoSelfProxy
		);
	});
}

#[test]
fn cannot_add_proxy_without_balance() {
	new_test_ext_and_execute(|| {
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(5), 3, ProxyType::Any, 0));
		assert_eq!(Balances::reserved_balance(5), 2);
		assert_noop!(
			Proxy::add_proxy(RuntimeOrigin::signed(5), 4, ProxyType::Any, 0),
			DispatchError::ConsumerRemaining,
		);
	});
}

#[test]
fn proxying_works() {
	new_test_ext_and_execute(|| {
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 2, ProxyType::JustTransfer, 0));
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 3, ProxyType::Any, 0));

		let call = Box::new(call_transfer(6, 1));
		assert_noop!(
			Proxy::proxy(RuntimeOrigin::signed(4), 1, None, call.clone()),
			Error::<Test>::NotProxy
		);
		assert_noop!(
			Proxy::proxy(RuntimeOrigin::signed(2), 1, Some(ProxyType::Any), call.clone()),
			Error::<Test>::NotProxy
		);
		assert_ok!(Proxy::proxy(RuntimeOrigin::signed(2), 1, None, call.clone()));
		System::assert_last_event(ProxyEvent::ProxyExecuted { result: Ok(()) }.into());
		assert_eq!(Balances::free_balance(6), 1);

		let call = Box::new(RuntimeCall::System(SystemCall::set_code { code: vec![] }));
		assert_ok!(Proxy::proxy(RuntimeOrigin::signed(3), 1, None, call.clone()));
		System::assert_last_event(
			ProxyEvent::ProxyExecuted { result: Err(SystemError::CallFiltered.into()) }.into(),
		);

		let call = Box::new(RuntimeCall::Balances(BalancesCall::transfer_keep_alive {
			dest: 6,
			value: 1,
		}));
		assert_ok!(RuntimeCall::Proxy(super::Call::new_call_variant_proxy(1, None, call.clone()))
			.dispatch(RuntimeOrigin::signed(2)));
		System::assert_last_event(
			ProxyEvent::ProxyExecuted { result: Err(SystemError::CallFiltered.into()) }.into(),
		);
		assert_ok!(Proxy::proxy(RuntimeOrigin::signed(3), 1, None, call.clone()));
		System::assert_last_event(ProxyEvent::ProxyExecuted { result: Ok(()) }.into());
		assert_eq!(Balances::free_balance(6), 2);
	});
}

#[test]
fn pure_works() {
	new_test_ext_and_execute(|| {
		Balances::make_free_balance_be(&1, 11); // An extra one for the ED.
		assert_ok!(Proxy::create_pure(RuntimeOrigin::signed(1), ProxyType::Any, 0, 0));
		let anon = Proxy::pure_account(&1, &ProxyType::Any, 0, None);
		System::assert_last_event(
			ProxyEvent::PureCreated {
				pure: anon,
				who: 1,
				proxy_type: ProxyType::Any,
				disambiguation_index: 0,
				at: <Test as Config>::BlockNumberProvider::current_block_number(),
				extrinsic_index: System::extrinsic_index().unwrap(),
			}
			.into(),
		);

		// other calls to pure allowed as long as they're not exactly the same.
		assert_ok!(Proxy::create_pure(RuntimeOrigin::signed(1), ProxyType::JustTransfer, 0, 0));
		assert_ok!(Proxy::create_pure(RuntimeOrigin::signed(1), ProxyType::Any, 0, 1));
		let anon2 = Proxy::pure_account(&2, &ProxyType::Any, 0, None);
		assert_ok!(Proxy::create_pure(RuntimeOrigin::signed(2), ProxyType::Any, 0, 0));
		assert_noop!(
			Proxy::create_pure(RuntimeOrigin::signed(1), ProxyType::Any, 0, 0),
			Error::<Test>::Duplicate
		);
		System::set_extrinsic_index(1);
		assert_ok!(Proxy::create_pure(RuntimeOrigin::signed(1), ProxyType::Any, 0, 0));
		System::set_extrinsic_index(0);
		System::set_block_number(2);
		assert_ok!(Proxy::create_pure(RuntimeOrigin::signed(1), ProxyType::Any, 0, 0));

		let call = Box::new(call_transfer(6, 1));
		assert_ok!(Balances::transfer_allow_death(RuntimeOrigin::signed(3), anon, 5));
		assert_eq!(Balances::free_balance(6), 0);
		assert_ok!(Proxy::proxy(RuntimeOrigin::signed(1), anon, None, call));
		System::assert_last_event(ProxyEvent::ProxyExecuted { result: Ok(()) }.into());
		assert_eq!(Balances::free_balance(6), 1);

		let call = Box::new(RuntimeCall::Proxy(ProxyCall::new_call_variant_kill_pure(
			1,
			ProxyType::Any,
			0,
			1,
			0,
		)));
		assert_ok!(Proxy::proxy(RuntimeOrigin::signed(2), anon2, None, call.clone()));
		let de: DispatchError = DispatchError::from(Error::<Test>::NoPermission).stripped();
		System::assert_last_event(ProxyEvent::ProxyExecuted { result: Err(de) }.into());
		assert_noop!(
			Proxy::kill_pure(RuntimeOrigin::signed(1), 1, ProxyType::Any, 0, 1, 0),
			Error::<Test>::NoPermission
		);
		assert_eq!(Balances::free_balance(1), 1);
		assert_ok!(Proxy::proxy(RuntimeOrigin::signed(1), anon, None, call.clone()));
		assert_eq!(Balances::free_balance(1), 3);
		assert_noop!(
			Proxy::proxy(RuntimeOrigin::signed(1), anon, None, call.clone()),
			Error::<Test>::NotProxy
		);

		// Actually kill the pure proxy.
		assert_ok!(Proxy::kill_pure(RuntimeOrigin::signed(anon), 1, ProxyType::Any, 0, 1, 0));
		System::assert_last_event(
			ProxyEvent::PureKilled {
				pure: anon,
				spawner: 1,
				proxy_type: ProxyType::Any,
				disambiguation_index: 0,
			}
			.into(),
		);
	});
}

#[test]
fn poke_deposit_works_for_proxy_deposits() {
	new_test_ext_and_execute(|| {
		// Add a proxy and check initial deposit
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 2, ProxyType::Any, 0));
		assert_eq!(Balances::reserved_balance(1), 2); // Base(1) + Factor(1) * 1

		// Change the proxy deposit base to trigger deposit update
		ProxyDepositBase::set(2);
		let result = Proxy::poke_deposit(RuntimeOrigin::signed(1));
		assert_ok!(result.as_ref());
		assert_eq!(result.unwrap().pays_fee, Pays::No);
		assert_eq!(Balances::reserved_balance(1), 3); // New Base(2) + Factor(1) * 1
		System::assert_last_event(
			ProxyEvent::DepositPoked {
				who: 1,
				kind: DepositKind::Proxies,
				old_deposit: 2,
				new_deposit: 3,
			}
			.into(),
		);
		assert!(System::events()
			.iter()
			.any(|record| matches!(record.event, RuntimeEvent::Proxy(Event::DepositPoked { .. }))));
	});
}

#[test]
fn poke_deposit_works_for_announcement_deposits() {
	new_test_ext_and_execute(|| {
		// Setup proxy and make announcement
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 3, ProxyType::Any, 1));
		assert_eq!(Balances::reserved_balance(1), 2); // Base(1) + Factor(1) * 1
		assert_ok!(Proxy::announce(RuntimeOrigin::signed(3), 1, [1; 32].into()));
		let announcements = Announcements::<Test>::get(3);
		assert_eq!(
			announcements.0,
			vec![Announcement { real: 1, call_hash: [1; 32].into(), height: 1 }]
		);
		assert_eq!(Balances::reserved_balance(3), announcements.1);
		let initial_deposit = Balances::reserved_balance(3);

		// Change announcement deposit base to trigger update
		AnnouncementDepositBase::set(2);
		let result = Proxy::poke_deposit(RuntimeOrigin::signed(3));
		assert_ok!(result.as_ref());
		assert_eq!(result.unwrap().pays_fee, Pays::No);
		let new_deposit = initial_deposit.saturating_add(1); // Base increased by 1
		assert_eq!(Balances::reserved_balance(3), new_deposit);
		System::assert_last_event(
			ProxyEvent::DepositPoked {
				who: 3,
				kind: DepositKind::Announcements,
				old_deposit: initial_deposit,
				new_deposit,
			}
			.into(),
		);
		assert!(System::events()
			.iter()
			.any(|record| matches!(record.event, RuntimeEvent::Proxy(Event::DepositPoked { .. }))));
	});
}

#[test]
fn poke_deposit_charges_fee_when_deposit_unchanged() {
	new_test_ext_and_execute(|| {
		// Add a proxy and check initial deposit
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 3, ProxyType::Any, 0));
		assert_eq!(Balances::reserved_balance(1), 2); // Base(1) + Factor(1) * 1

		// Poke the deposit without changing deposit required and check fee
		let result = Proxy::poke_deposit(RuntimeOrigin::signed(1));
		assert_ok!(result.as_ref());
		assert_eq!(result.unwrap().pays_fee, Pays::Yes); // Pays fee
		assert_eq!(Balances::reserved_balance(1), 2); // No change

		// No event emitted
		assert!(!System::events()
			.iter()
			.any(|record| matches!(record.event, RuntimeEvent::Proxy(Event::DepositPoked { .. }))));

		// Add an announcement and check initial deposit
		assert_ok!(Proxy::announce(RuntimeOrigin::signed(3), 1, [1; 32].into()));
		let announcements = Announcements::<Test>::get(3);
		assert_eq!(
			announcements.0,
			vec![Announcement { real: 1, call_hash: [1; 32].into(), height: 1 }]
		);
		assert_eq!(Balances::reserved_balance(3), announcements.1);
		let initial_deposit = Balances::reserved_balance(3);

		// Poke the deposit without changing deposit required and check fee
		let result = Proxy::poke_deposit(RuntimeOrigin::signed(3));
		assert_ok!(result.as_ref());
		assert_eq!(result.unwrap().pays_fee, Pays::Yes); // Pays fee
		assert_eq!(Balances::reserved_balance(3), initial_deposit); // No change

		// No event emitted
		assert!(!System::events()
			.iter()
			.any(|record| matches!(record.event, RuntimeEvent::Proxy(Event::DepositPoked { .. }))));
	});
}

#[test]
fn poke_deposit_handles_insufficient_balance() {
	// Ends stale by design: account 5 cannot afford the raised base, so its deposit stays behind.
	new_test_ext_and_execute_with_stale_deposit(|| {
		// Setup with account that has minimal balance
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(5), 3, ProxyType::Any, 0));
		let initial_deposit = Balances::reserved_balance(5);

		// Change deposit base to require more than available balance
		ProxyDepositBase::set(10);

		// Poking should fail due to insufficient balance
		assert_noop!(
			Proxy::poke_deposit(RuntimeOrigin::signed(5)),
			BalancesError::<Test, _>::InsufficientBalance,
		);

		// Original deposit should remain unchanged
		assert_eq!(Balances::reserved_balance(5), initial_deposit);
	});
}

#[test]
fn poke_deposit_updates_both_proxy_and_announcement_deposits() {
	// Ends stale by design: only account 2 pokes, so account 1's entry keeps the older price.
	new_test_ext_and_execute_with_stale_deposit(|| {
		// Setup both proxy and announcement for the same account
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 2, ProxyType::Any, 0));
		assert_eq!(Balances::reserved_balance(1), 2); // Base(1) + Factor(1) * 1
		assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(2), 3, ProxyType::Any, 1));
		assert_eq!(Balances::reserved_balance(2), 2); // Base(1) + Factor(1) * 1
		assert_ok!(Proxy::announce(RuntimeOrigin::signed(2), 1, [1; 32].into()));
		let announcements = Announcements::<Test>::get(2);
		assert_eq!(
			announcements.0,
			vec![Announcement { real: 1, call_hash: [1; 32].into(), height: 1 }]
		);
		assert_eq!(announcements.1, 2); // Base(1) + Factor(1) * 1

		// Record initial deposits
		let initial_proxy_deposit = Proxies::<Test>::get(2).1;
		let initial_announcement_deposit = Announcements::<Test>::get(2).1;

		// Total reserved = deposit for proxy + deposit for announcement
		assert_eq!(
			Balances::reserved_balance(2),
			initial_proxy_deposit.saturating_add(initial_announcement_deposit)
		);

		// Change both deposit requirements
		ProxyDepositBase::set(2);
		AnnouncementDepositBase::set(2);

		// Poke deposits - should update both deposits and emit two events
		let result = Proxy::poke_deposit(RuntimeOrigin::signed(2));
		assert_ok!(result.as_ref());
		assert_eq!(result.unwrap().pays_fee, Pays::No);

		// Check both deposits were updated
		let (_, new_proxy_deposit) = Proxies::<Test>::get(2);
		let (_, new_announcement_deposit) = Announcements::<Test>::get(2);
		assert_eq!(new_proxy_deposit, 3); // Base(2) + Factor(1) * 1
		assert_eq!(new_announcement_deposit, 3); // Base(2) + Factor(1) * 1
		assert_eq!(
			Balances::reserved_balance(2),
			new_proxy_deposit.saturating_add(new_announcement_deposit)
		);

		// Verify both events were emitted in the correct order
		let events = System::events();
		let relevant_events: Vec<_> = events
			.iter()
			.filter(|record| {
				matches!(record.event, RuntimeEvent::Proxy(ProxyEvent::DepositPoked { .. }))
			})
			.collect();

		assert_eq!(relevant_events.len(), 2);

		// First event should be for Proxies
		assert_eq!(
			relevant_events[0].event,
			ProxyEvent::DepositPoked {
				who: 2,
				kind: DepositKind::Proxies,
				old_deposit: initial_proxy_deposit,
				new_deposit: new_proxy_deposit,
			}
			.into()
		);

		// Second event should be for Announcements
		assert_eq!(
			relevant_events[1].event,
			ProxyEvent::DepositPoked {
				who: 2,
				kind: DepositKind::Announcements,
				old_deposit: initial_announcement_deposit,
				new_deposit: new_announcement_deposit,
			}
			.into()
		);

		// Poking again should charge fee as nothing changes
		let result = Proxy::poke_deposit(RuntimeOrigin::signed(2));
		assert_ok!(result.as_ref());
		assert_eq!(result.unwrap().pays_fee, Pays::Yes);

		// Verify deposits remained the same
		assert_eq!(Proxies::<Test>::get(2).1, new_proxy_deposit);
		assert_eq!(Announcements::<Test>::get(2).1, new_announcement_deposit);
		assert_eq!(
			Balances::reserved_balance(2),
			new_proxy_deposit.saturating_add(new_announcement_deposit)
		);
	});
}

#[test]
fn poke_deposit_fails_for_unsigned_origin() {
	new_test_ext_and_execute(|| {
		assert_noop!(Proxy::poke_deposit(RuntimeOrigin::none()), DispatchError::BadOrigin,);
	});
}

mod try_state {
	use super::*;

	type ProxiesValue =
		(BoundedVec<ProxyDefinition<u64, ProxyType, u64>, <Test as Config>::MaxProxies>, u64);
	type AnnouncementsValue =
		(BoundedVec<Announcement<u64, CallHashOf<Test>, u64>, <Test as Config>::MaxPending>, u64);

	#[test]
	fn passes_on_genesis_state() {
		new_test_ext().execute_with(|| {
			assert_ok!(Proxy::do_try_state());
		});
	}

	#[test]
	fn passes_with_pure_proxy_and_announcement() {
		// Exercises the trickiest legitimate case: a pure proxy's deposit is held by its
		// spawner, not by the pure account itself, which must not trip the deposit check.
		new_test_ext_and_execute(|| {
			assert_ok!(Proxy::create_pure(RuntimeOrigin::signed(1), ProxyType::Any, 0, 0));
			assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 2, ProxyType::Any, 0));
			assert_ok!(Proxy::announce(RuntimeOrigin::signed(2), 1, [1; 32].into()));
		});
	}

	#[test]
	fn detects_empty_proxies_entry() {
		new_test_ext().execute_with(|| {
			let value: ProxiesValue = (vec![].try_into().unwrap(), 0);
			Proxies::<Test>::insert(1, value);
			assert_eq!(
				Proxy::do_try_state().unwrap_err(),
				TryRuntimeError::Other("Proxies entry must never be empty")
			);
		});
	}

	#[test]
	fn detects_unsorted_proxies_entry() {
		new_test_ext().execute_with(|| {
			let higher = ProxyDefinition { delegate: 3, proxy_type: ProxyType::Any, delay: 0 };
			let lower = ProxyDefinition { delegate: 2, proxy_type: ProxyType::Any, delay: 0 };
			let value: ProxiesValue = (vec![higher, lower].try_into().unwrap(), 0);
			Proxies::<Test>::insert(1, value);
			assert_eq!(
				Proxy::do_try_state().unwrap_err(),
				TryRuntimeError::Other("Proxies must be strictly sorted and duplicate-free")
			);
		});
	}

	#[test]
	fn detects_duplicate_proxies_entry() {
		new_test_ext().execute_with(|| {
			let def = ProxyDefinition { delegate: 2, proxy_type: ProxyType::Any, delay: 0 };
			let value: ProxiesValue = (vec![def, def].try_into().unwrap(), 0);
			Proxies::<Test>::insert(1, value);
			assert_eq!(
				Proxy::do_try_state().unwrap_err(),
				TryRuntimeError::Other("Proxies must be strictly sorted and duplicate-free")
			);
		});
	}

	#[test]
	fn detects_self_proxy() {
		new_test_ext().execute_with(|| {
			let def = ProxyDefinition { delegate: 1, proxy_type: ProxyType::Any, delay: 0 };
			let value: ProxiesValue = (vec![def].try_into().unwrap(), 0);
			Proxies::<Test>::insert(1, value);
			assert_eq!(
				Proxy::do_try_state().unwrap_err(),
				TryRuntimeError::Other(
					"Proxies entry must not list the key account as its own delegate"
				)
			);
		});
	}

	#[test]
	fn proxies_deposit_shortfall_only_warns() {
		// Mirrors an untouched pure proxy: a deposit is recorded but nothing is reserved on
		// the key account itself. This must not fail the check, only warn.
		new_test_ext().execute_with(|| {
			let def = ProxyDefinition { delegate: 2, proxy_type: ProxyType::Any, delay: 0 };
			let value: ProxiesValue = (vec![def].try_into().unwrap(), 2);
			Proxies::<Test>::insert(1, value);
			assert_ok!(Proxy::do_try_state());
		});
	}

	#[test]
	fn detects_proxies_deposit_disagreeing_with_formula() {
		// A deposit that is fully reserved but no longer priced by the current parameters, as
		// left behind by a governance change until the account calls `poke_deposit`. Legal on a
		// live chain, hence a warning; a hard error under `fuzzing`, where parameters are fixed.
		new_test_ext().execute_with(|| {
			assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 2, ProxyType::Any, 0));
			// One proxy priced at `ProxyDepositBase + ProxyDepositFactor`, fully reserved.
			assert_eq!(Proxies::<Test>::get(1).1, 2);
			assert_eq!(Balances::reserved_balance(1), 2);

			// Governance drops the per-proxy factor: the entry now prices at 1, not 2.
			ProxyDepositFactor::set(0);
			assert_eq!(Proxy::deposit(1), 1);
			assert_eq!(Proxies::<Test>::get(1).1, 2);

			#[cfg(not(feature = "fuzzing"))]
			assert_ok!(Proxy::do_try_state());
			#[cfg(feature = "fuzzing")]
			assert_eq!(
				Proxy::do_try_state().unwrap_err(),
				TryRuntimeError::Other("Proxies deposit does not match the current parameters")
			);
		});
	}

	#[test]
	fn detects_empty_announcements_entry() {
		new_test_ext().execute_with(|| {
			let value: AnnouncementsValue = (vec![].try_into().unwrap(), 0);
			Announcements::<Test>::insert(1, value);
			assert_eq!(
				Proxy::do_try_state().unwrap_err(),
				TryRuntimeError::Other("Announcements entry must never be empty")
			);
		});
	}

	#[test]
	fn detects_decreasing_announcement_heights() {
		new_test_ext().execute_with(|| {
			let later = Announcement { real: 2, call_hash: [1; 32].into(), height: 5 };
			let earlier = Announcement { real: 2, call_hash: [2; 32].into(), height: 4 };
			let value: AnnouncementsValue = (vec![later, earlier].try_into().unwrap(), 0);
			Announcements::<Test>::insert(1, value);
			assert_eq!(
				Proxy::do_try_state().unwrap_err(),
				TryRuntimeError::Other("Announcements heights must be non-decreasing")
			);
		});
	}

	#[test]
	fn detects_self_announcement() {
		new_test_ext().execute_with(|| {
			let ann = Announcement { real: 1, call_hash: [1; 32].into(), height: 1 };
			let value: AnnouncementsValue = (vec![ann].try_into().unwrap(), 0);
			Announcements::<Test>::insert(1, value);
			assert_eq!(
				Proxy::do_try_state().unwrap_err(),
				TryRuntimeError::Other(
					"Announcements entry must not name the key account as `real`"
				)
			);
		});
	}

	#[test]
	fn detects_future_announcement_height() {
		new_test_ext().execute_with(|| {
			// `new_test_ext` starts at block 1.
			let ann = Announcement { real: 2, call_hash: [1; 32].into(), height: 100 };
			let value: AnnouncementsValue = (vec![ann].try_into().unwrap(), 0);
			Announcements::<Test>::insert(1, value);
			assert_eq!(
				Proxy::do_try_state().unwrap_err(),
				TryRuntimeError::Other(
					"Announcements entry has a height later than the current block"
				)
			);
		});
	}

	#[test]
	fn detects_underreserved_announcement_deposit() {
		new_test_ext().execute_with(|| {
			let ann = Announcement { real: 2, call_hash: [1; 32].into(), height: 1 };
			// Nothing is reserved on account 1, but a deposit is recorded.
			let value: AnnouncementsValue = (vec![ann].try_into().unwrap(), 5);
			Announcements::<Test>::insert(1, value);
			assert_eq!(
				Proxy::do_try_state().unwrap_err(),
				TryRuntimeError::Other(
					"Announcements deposit exceeds the key account's reserved balance"
				)
			);
		});
	}

	#[test]
	fn detects_announcement_deposit_disagreeing_with_formula() {
		// The `Announcements` mirror of `detects_proxies_deposit_disagreeing_with_formula`: fully
		// reserved, but no longer what the current parameters price the entry at.
		new_test_ext().execute_with(|| {
			assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 2, ProxyType::Any, 0));
			assert_ok!(Proxy::announce(RuntimeOrigin::signed(2), 1, [1; 32].into()));
			// One announcement priced at `AnnouncementDepositBase + AnnouncementDepositFactor`,
			// fully reserved.
			assert_eq!(Announcements::<Test>::get(2).1, 2);
			assert_eq!(Balances::reserved_balance(2), 2);

			// Governance drops the per-announcement factor: the entry now prices at 1, not 2.
			AnnouncementDepositFactor::set(0);
			assert_eq!(Announcements::<Test>::get(2).1, 2);

			#[cfg(not(feature = "fuzzing"))]
			assert_ok!(Proxy::do_try_state());
			#[cfg(feature = "fuzzing")]
			assert_eq!(
				Proxy::do_try_state().unwrap_err(),
				TryRuntimeError::Other(
					"Announcements deposit does not match the current parameters"
				)
			);
		});
	}

	#[test]
	fn detects_reserve_shared_between_both_deposits() {
		// Account 2 has an entry in both maps, each deposit covered by the reserve on its own,
		// but the two together are not: the same units back both claims. Checks 4 and 10 read one
		// map each and pass; only the cross-map check sees it.
		new_test_ext().execute_with(|| {
			// Account 2 delegates to 3, and announces for 1, who delegates to it.
			assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(2), 3, ProxyType::Any, 0));
			assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 2, ProxyType::Any, 0));
			assert_ok!(Proxy::announce(RuntimeOrigin::signed(2), 1, [1; 32].into()));
			// Both entries priced at 2, and both are genuinely reserved.
			assert_eq!(Proxies::<Test>::get(2).1, 2);
			assert_eq!(Announcements::<Test>::get(2).1, 2);
			assert_eq!(Balances::reserved_balance(2), 4);

			// Release half the reserve, leaving 2 to cover a sum of 4.
			Balances::unreserve(&2, 2);
			assert_eq!(Balances::reserved_balance(2), 2);

			assert_eq!(
				Proxy::do_try_state().unwrap_err(),
				TryRuntimeError::Other("Reserve does not cover the sum of both deposits")
			);
		});
	}

	#[test]
	fn pure_proxy_reserve_shortfall_skips_the_sum_check() {
		// The guard on the cross-map check: a pure proxy's `Proxies` deposit is reserved on its
		// spawner, so the key account's own reserve covers neither that deposit nor the sum. Only
		// check 4's warning applies, and the sum check must stay silent.
		new_test_ext().execute_with(|| {
			// Account 2 announces for 1, reserving its announcement deposit of 2 on itself.
			assert_ok!(Proxy::add_proxy(RuntimeOrigin::signed(1), 2, ProxyType::Any, 0));
			assert_ok!(Proxy::announce(RuntimeOrigin::signed(2), 1, [1; 32].into()));
			assert_eq!(Announcements::<Test>::get(2).1, 2);
			assert_eq!(Balances::reserved_balance(2), 2);

			// It also holds a `Proxies` entry whose deposit exceeds that reserve, as a pure
			// proxy's does, its deposit being reserved on the spawner instead.
			let proxy_def = ProxyDefinition { delegate: 3, proxy_type: ProxyType::Any, delay: 0 };
			let proxies: ProxiesValue = (vec![proxy_def].try_into().unwrap(), 3);
			Proxies::<Test>::insert(2, proxies);

			// Uncovered `Proxies` deposit: check 4 warns and the sum check is skipped, rather
			// than the pair failing this legal state.
			#[cfg(not(feature = "fuzzing"))]
			assert_ok!(Proxy::do_try_state());
			// Under `fuzzing` the fabricated deposit of 3 trips check 5 first, and the sum check
			// is still not what fires.
			#[cfg(feature = "fuzzing")]
			assert_eq!(
				Proxy::do_try_state().unwrap_err(),
				TryRuntimeError::Other("Proxies deposit does not match the current parameters")
			);
		});
	}
}
