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

// Benchmarks for Proxy Pallet

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::Pallet as Proxy;
use alloc::{boxed::Box, vec};
use frame::benchmarking::prelude::{
	account, benchmarks, impl_test_function, whitelisted_caller, BenchmarkError, RawOrigin,
};

const SEED: u32 = 0;

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

fn assert_has_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_has_event(generic_event.into());
}

fn add_proxies<T: Config>(n: u32, maybe_who: Option<T::AccountId>) -> Result<(), &'static str> {
	let caller = maybe_who.unwrap_or_else(whitelisted_caller);
	T::Currency::make_free_balance_be(&caller, BalanceOf::<T>::max_value() / 2u32.into());
	for i in 0..n {
		let real = T::Lookup::unlookup(account("target", i, SEED));

		Proxy::<T>::add_proxy(
			RawOrigin::Signed(caller.clone()).into(),
			real,
			T::ProxyType::default(),
			BlockNumberFor::<T>::zero(),
		)?;
	}
	Ok(())
}

fn add_announcements<T: Config>(
	n: u32,
	maybe_who: Option<T::AccountId>,
	maybe_real: Option<T::AccountId>,
) -> Result<(), &'static str> {
	let caller = maybe_who.unwrap_or_else(|| account("caller", 0, SEED));
	let caller_lookup = T::Lookup::unlookup(caller.clone());
	T::Currency::make_free_balance_be(&caller, BalanceOf::<T>::max_value() / 2u32.into());
	let real = if let Some(real) = maybe_real {
		real
	} else {
		let real = account("real", 0, SEED);
		T::Currency::make_free_balance_be(&real, BalanceOf::<T>::max_value() / 2u32.into());
		Proxy::<T>::add_proxy(
			RawOrigin::Signed(real.clone()).into(),
			caller_lookup,
			T::ProxyType::default(),
			BlockNumberFor::<T>::zero(),
		)?;
		real
	};
	let real_lookup = T::Lookup::unlookup(real);
	for _ in 0..n {
		Proxy::<T>::announce(
			RawOrigin::Signed(caller.clone()).into(),
			real_lookup.clone(),
			T::CallHasher::hash_of(&("add_announcement", n)),
		)?;
	}
	Ok(())
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn proxy(p: Linear<1, { T::MaxProxies::get() - 1 }>) -> Result<(), BenchmarkError> {
		add_proxies::<T>(p, None)?;
		// In this case the caller is the "target" proxy
		let caller: T::AccountId = account("target", p - 1, SEED);
		T::Currency::make_free_balance_be(&caller, BalanceOf::<T>::max_value() / 2u32.into());
		// ... and "real" is the traditional caller. This is not a typo.
		let real: T::AccountId = whitelisted_caller();
		let real_lookup = T::Lookup::unlookup(real);
		let call: <T as Config>::RuntimeCall =
			frame_system::Call::<T>::remark { remark: vec![] }.into();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), real_lookup, Some(T::ProxyType::default()), Box::new(call));

		assert_last_event::<T>(Event::ProxyExecuted { result: Ok(()) }.into());

		Ok(())
	}

	#[benchmark]
	fn proxy_announced(
		a: Linear<0, { T::MaxPending::get() - 1 }>,
		p: Linear<1, { T::MaxProxies::get() - 1 }>,
	) -> Result<(), BenchmarkError> {
		add_proxies::<T>(p, None)?;
		// In this case the caller is the "target" proxy
		let caller: T::AccountId = account("pure", 0, SEED);
		let delegate: T::AccountId = account("target", p - 1, SEED);
		let delegate_lookup = T::Lookup::unlookup(delegate.clone());
		T::Currency::make_free_balance_be(&delegate, BalanceOf::<T>::max_value() / 2u32.into());
		// ... and "real" is the traditional caller. This is not a typo.
		let real: T::AccountId = whitelisted_caller();
		let real_lookup = T::Lookup::unlookup(real);
		let call: <T as Config>::RuntimeCall =
			frame_system::Call::<T>::remark { remark: vec![] }.into();
		Proxy::<T>::announce(
			RawOrigin::Signed(delegate.clone()).into(),
			real_lookup.clone(),
			T::CallHasher::hash_of(&call),
		)?;
		add_announcements::<T>(a, Some(delegate.clone()), None)?;

		#[extrinsic_call]
		_(
			RawOrigin::Signed(caller),
			delegate_lookup,
			real_lookup,
			Some(T::ProxyType::default()),
			Box::new(call),
		);

		assert_last_event::<T>(Event::ProxyExecuted { result: Ok(()) }.into());

		Ok(())
	}

	#[benchmark]
	fn remove_announcement(
		a: Linear<0, { T::MaxPending::get() - 1 }>,
		p: Linear<1, { T::MaxProxies::get() - 1 }>,
	) -> Result<(), BenchmarkError> {
		add_proxies::<T>(p, None)?;
		// In this case the caller is the "target" proxy
		let caller: T::AccountId = account("target", p - 1, SEED);
		T::Currency::make_free_balance_be(&caller, BalanceOf::<T>::max_value() / 2u32.into());
		// ... and "real" is the traditional caller. This is not a typo.
		let real: T::AccountId = whitelisted_caller();
		let real_lookup = T::Lookup::unlookup(real);
		let call: <T as Config>::RuntimeCall =
			frame_system::Call::<T>::remark { remark: vec![] }.into();
		Proxy::<T>::announce(
			RawOrigin::Signed(caller.clone()).into(),
			real_lookup.clone(),
			T::CallHasher::hash_of(&call),
		)?;
		add_announcements::<T>(a, Some(caller.clone()), None)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), real_lookup, T::CallHasher::hash_of(&call));

		let (announcements, _) = Announcements::<T>::get(&caller);
		assert_eq!(announcements.len() as u32, a);

		Ok(())
	}

	#[benchmark]
	fn reject_announcement(
		a: Linear<0, { T::MaxPending::get() - 1 }>,
		p: Linear<1, { T::MaxProxies::get() - 1 }>,
	) -> Result<(), BenchmarkError> {
		add_proxies::<T>(p, None)?;
		// In this case the caller is the "target" proxy
		let caller: T::AccountId = account("target", p - 1, SEED);
		let caller_lookup = T::Lookup::unlookup(caller.clone());
		T::Currency::make_free_balance_be(&caller, BalanceOf::<T>::max_value() / 2u32.into());
		// ... and "real" is the traditional caller. This is not a typo.
		let real: T::AccountId = whitelisted_caller();
		let real_lookup = T::Lookup::unlookup(real.clone());
		let call: <T as Config>::RuntimeCall =
			frame_system::Call::<T>::remark { remark: vec![] }.into();
		Proxy::<T>::announce(
			RawOrigin::Signed(caller.clone()).into(),
			real_lookup,
			T::CallHasher::hash_of(&call),
		)?;
		add_announcements::<T>(a, Some(caller.clone()), None)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(real), caller_lookup, T::CallHasher::hash_of(&call));

		let (announcements, _) = Announcements::<T>::get(&caller);
		assert_eq!(announcements.len() as u32, a);

		Ok(())
	}

	#[benchmark]
	fn announce(
		a: Linear<0, { T::MaxPending::get() - 1 }>,
		p: Linear<1, { T::MaxProxies::get() - 1 }>,
	) -> Result<(), BenchmarkError> {
		add_proxies::<T>(p, None)?;
		// In this case the caller is the "target" proxy
		let caller: T::AccountId = account("target", p - 1, SEED);
		T::Currency::make_free_balance_be(&caller, BalanceOf::<T>::max_value() / 2u32.into());
		// ... and "real" is the traditional caller. This is not a typo.
		let real: T::AccountId = whitelisted_caller();
		let real_lookup = T::Lookup::unlookup(real.clone());
		add_announcements::<T>(a, Some(caller.clone()), None)?;
		let call: <T as Config>::RuntimeCall =
			frame_system::Call::<T>::remark { remark: vec![] }.into();
		let call_hash = T::CallHasher::hash_of(&call);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), real_lookup, call_hash);

		assert_last_event::<T>(Event::Announced { real, proxy: caller, call_hash }.into());

		Ok(())
	}

	#[benchmark]
	fn add_proxy(p: Linear<1, { T::MaxProxies::get() - 1 }>) -> Result<(), BenchmarkError> {
		add_proxies::<T>(p, None)?;
		let caller: T::AccountId = whitelisted_caller();
		let real = T::Lookup::unlookup(account("target", T::MaxProxies::get(), SEED));

		#[extrinsic_call]
		_(
			RawOrigin::Signed(caller.clone()),
			real,
			T::ProxyType::default(),
			BlockNumberFor::<T>::zero(),
		);

		let (proxies, _) = Proxies::<T>::get(caller);
		assert_eq!(proxies.len() as u32, p + 1);

		Ok(())
	}

	#[benchmark]
	fn remove_proxy(p: Linear<1, { T::MaxProxies::get() - 1 }>) -> Result<(), BenchmarkError> {
		add_proxies::<T>(p, None)?;
		let caller: T::AccountId = whitelisted_caller();
		let delegate = T::Lookup::unlookup(account("target", 0, SEED));

		#[extrinsic_call]
		_(
			RawOrigin::Signed(caller.clone()),
			delegate,
			T::ProxyType::default(),
			BlockNumberFor::<T>::zero(),
		);

		let (proxies, _) = Proxies::<T>::get(caller);
		assert_eq!(proxies.len() as u32, p - 1);

		Ok(())
	}

	#[benchmark]
	fn remove_proxies(p: Linear<1, { T::MaxProxies::get() - 1 }>) -> Result<(), BenchmarkError> {
		add_proxies::<T>(p, None)?;
		let caller: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()));

		let (proxies, _) = Proxies::<T>::get(caller);
		assert_eq!(proxies.len() as u32, 0);

		Ok(())
	}

	#[benchmark]
	fn create_pure(p: Linear<1, { T::MaxProxies::get() - 1 }>) -> Result<(), BenchmarkError> {
		add_proxies::<T>(p, None)?;
		let caller: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		_(
			RawOrigin::Signed(caller.clone()),
			T::ProxyType::default(),
			BlockNumberFor::<T>::zero(),
			0,
		);

		let pure_account = Pallet::<T>::pure_account(&caller, &T::ProxyType::default(), 0, None);
		assert_last_event::<T>(
			Event::PureCreated {
				pure: pure_account,
				who: caller,
				proxy_type: T::ProxyType::default(),
				disambiguation_index: 0,
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn kill_pure(p: Linear<0, { T::MaxProxies::get() - 2 }>) -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let caller_lookup = T::Lookup::unlookup(caller.clone());
		T::Currency::make_free_balance_be(&caller, BalanceOf::<T>::max_value());
		Pallet::<T>::create_pure(
			RawOrigin::Signed(whitelisted_caller()).into(),
			T::ProxyType::default(),
			BlockNumberFor::<T>::zero(),
			0,
		)?;
		let height = T::BlockNumberProvider::current_block_number();
		let ext_index = frame_system::Pallet::<T>::extrinsic_index().unwrap_or(0);
		let pure_account = Pallet::<T>::pure_account(&caller, &T::ProxyType::default(), 0, None);

		add_proxies::<T>(p, Some(pure_account.clone()))?;
		ensure!(Proxies::<T>::contains_key(&pure_account), "pure proxy not created");

		#[extrinsic_call]
		_(
			RawOrigin::Signed(pure_account.clone()),
			caller_lookup,
			T::ProxyType::default(),
			0,
			height,
			ext_index,
		);

		assert!(!Proxies::<T>::contains_key(&pure_account));

		Ok(())
	}

	#[benchmark]
	fn poke_deposit() -> Result<(), BenchmarkError> {
		// Create accounts using the same pattern as other benchmarks
		let account_1: T::AccountId = account("account", 1, SEED);
		let account_2: T::AccountId = account("account", 2, SEED);
		let account_3: T::AccountId = account("account", 3, SEED);

		// Fund accounts
		T::Currency::make_free_balance_be(&account_1, BalanceOf::<T>::max_value() / 100u8.into());
		T::Currency::make_free_balance_be(&account_2, BalanceOf::<T>::max_value() / 100u8.into());
		T::Currency::make_free_balance_be(&account_3, BalanceOf::<T>::max_value() / 100u8.into());

		// Add proxy relationships
		Proxy::<T>::add_proxy(
			RawOrigin::Signed(account_1.clone()).into(),
			T::Lookup::unlookup(account_2.clone()),
			T::ProxyType::default(),
			BlockNumberFor::<T>::zero(),
		)?;
		Proxy::<T>::add_proxy(
			RawOrigin::Signed(account_2.clone()).into(),
			T::Lookup::unlookup(account_3.clone()),
			T::ProxyType::default(),
			BlockNumberFor::<T>::zero(),
		)?;
		let (proxies, initial_proxy_deposit) = Proxies::<T>::get(&account_2);
		assert!(!initial_proxy_deposit.is_zero());
		assert_eq!(initial_proxy_deposit, T::Currency::reserved_balance(&account_2));

		// Create announcement
		Proxy::<T>::announce(
			RawOrigin::Signed(account_2.clone()).into(),
			T::Lookup::unlookup(account_1.clone()),
			T::CallHasher::hash_of(&("add_announcement", 1)),
		)?;
		let (announcements, initial_announcement_deposit) = Announcements::<T>::get(&account_2);
		assert!(!initial_announcement_deposit.is_zero());
		assert_eq!(
			initial_announcement_deposit.saturating_add(initial_proxy_deposit),
			T::Currency::reserved_balance(&account_2)
		);

		// Artificially inflate deposits and reserve the extra amount
		let extra_proxy_deposit = initial_proxy_deposit; // Double the deposit
		let extra_announcement_deposit = initial_announcement_deposit; // Double the deposit
		let total = extra_proxy_deposit.saturating_add(extra_announcement_deposit);

		T::Currency::reserve(&account_2, total)?;

		let initial_reserved = T::Currency::reserved_balance(&account_2);
		assert_eq!(initial_reserved, total.saturating_add(total)); // Double

		// Update storage with increased deposits
		Proxies::<T>::insert(
			&account_2,
			(proxies, initial_proxy_deposit.saturating_add(extra_proxy_deposit)),
		);
		Announcements::<T>::insert(
			&account_2,
			(
				announcements,
				initial_announcement_deposit.saturating_add(extra_announcement_deposit),
			),
		);

		// Verify artificial state
		let (_, inflated_proxy_deposit) = Proxies::<T>::get(&account_2);
		let (_, inflated_announcement_deposit) = Announcements::<T>::get(&account_2);
		assert_eq!(
			inflated_proxy_deposit,
			initial_proxy_deposit.saturating_add(extra_proxy_deposit)
		);
		assert_eq!(
			inflated_announcement_deposit,
			initial_announcement_deposit.saturating_add(extra_announcement_deposit)
		);

		#[extrinsic_call]
		_(RawOrigin::Signed(account_2.clone()));

		// Verify results
		let (_, final_proxy_deposit) = Proxies::<T>::get(&account_2);
		let (_, final_announcement_deposit) = Announcements::<T>::get(&account_2);
		assert_eq!(final_proxy_deposit, initial_proxy_deposit);
		assert_eq!(final_announcement_deposit, initial_announcement_deposit);

		let final_reserved = T::Currency::reserved_balance(&account_2);
		assert_eq!(final_reserved, initial_reserved.saturating_sub(total));

		// Verify events
		assert_has_event::<T>(
			Event::DepositPoked {
				who: account_2.clone(),
				kind: DepositKind::Proxies,
				old_deposit: inflated_proxy_deposit,
				new_deposit: final_proxy_deposit,
			}
			.into(),
		);
		assert_last_event::<T>(
			Event::DepositPoked {
				who: account_2,
				kind: DepositKind::Announcements,
				old_deposit: inflated_announcement_deposit,
				new_deposit: final_announcement_deposit,
			}
			.into(),
		);

		Ok(())
	}

	impl_benchmark_test_suite!(Proxy, crate::tests::new_test_ext(), crate::tests::Test);
}
