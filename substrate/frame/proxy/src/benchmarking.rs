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
use alloc::{boxed::Box, vec::Vec};
use frame::{
	benchmarking::prelude::{
		account, benchmarks, impl_test_function, whitelisted_caller, BenchmarkError, RawOrigin,
	},
	traits::fungible::{InspectHold, Mutate as FunMutate, MutateHold},
};

const SEED: u32 = 0;

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

fn assert_has_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_has_event(generic_event.into());
}

fn add_proxies<T: Config>(n: u32, maybe_who: Option<T::AccountId>) -> Result<(), &'static str>
where
	T::Currency: FunMutate<T::AccountId>,
{
	let caller = maybe_who.unwrap_or_else(whitelisted_caller);
	// Mint sufficient balance for operations and deposits
	let balance_amount = BalanceOf::<T>::max_value() / 100u32.into();
	let _ = <T::Currency as FunMutate<_>>::mint_into(&caller, balance_amount);
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
) -> Result<(), &'static str>
where
	T::Currency: FunMutate<T::AccountId>,
{
	let caller = if let Some(who) = maybe_who {
		who
	} else {
		let caller = account("caller", 0, SEED);
		// Mint sufficient balance for operations and deposits
		let balance_amount = BalanceOf::<T>::max_value() / 100u32.into();
		let _ = <T::Currency as FunMutate<_>>::mint_into(&caller, balance_amount);
		caller
	};
	let caller_lookup = T::Lookup::unlookup(caller.clone());
	let real = if let Some(real) = maybe_real {
		real
	} else {
		let real = account("real", 0, SEED);
		let _ = <T::Currency as FunMutate<_>>::mint_into(
			&real,
			BalanceOf::<T>::max_value() / 2u32.into(),
		);
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

#[benchmarks(where T::Currency: FunMutate<T::AccountId>)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn proxy(p: Linear<1, { T::MaxProxies::get() - 1 }>) -> Result<(), BenchmarkError> {
		add_proxies::<T>(p, None)?;
		// In this case the caller is the "target" proxy
		let caller: T::AccountId = account("target", p - 1, SEED);
		// Mint sufficient balance for operations and deposits
		let balance_amount = BalanceOf::<T>::max_value() / 100u32.into();
		let _ = <T::Currency as FunMutate<_>>::mint_into(&caller, balance_amount);
		// ... and "real" is the traditional caller. This is not a typo.
		let real: T::AccountId = whitelisted_caller();
		let real_lookup = T::Lookup::unlookup(real.clone());
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
		let _ = <T::Currency as FunMutate<_>>::mint_into(
			&delegate,
			BalanceOf::<T>::max_value() / 2u32.into(),
		);
		// ... and "real" is the traditional caller. This is not a typo.
		let real: T::AccountId = whitelisted_caller();
		let real_lookup = T::Lookup::unlookup(real.clone());
		let call: <T as Config>::RuntimeCall =
			frame_system::Call::<T>::remark { remark: vec![] }.into();
		Proxy::<T>::announce(
			RawOrigin::Signed(delegate.clone()).into(),
			real_lookup.clone(),
			T::CallHasher::hash_of(&call),
		)?;
		add_announcements::<T>(a, Some(delegate.clone()), Some(real.clone()))?;

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
		// Mint sufficient balance for operations and deposits
		let balance_amount = BalanceOf::<T>::max_value() / 100u32.into();
		let _ = <T::Currency as FunMutate<_>>::mint_into(&caller, balance_amount);
		// ... and "real" is the traditional caller. This is not a typo.
		let real: T::AccountId = whitelisted_caller();
		let real_lookup = T::Lookup::unlookup(real.clone());
		let call: <T as Config>::RuntimeCall =
			frame_system::Call::<T>::remark { remark: vec![] }.into();
		Proxy::<T>::announce(
			RawOrigin::Signed(caller.clone()).into(),
			real_lookup.clone(),
			T::CallHasher::hash_of(&call),
		)?;
		add_announcements::<T>(a, Some(caller.clone()), Some(real.clone()))?;

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
		// Mint sufficient balance for operations and deposits
		let balance_amount = BalanceOf::<T>::max_value() / 100u32.into();
		let _ = <T::Currency as FunMutate<_>>::mint_into(&caller, balance_amount);
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
		add_announcements::<T>(a, Some(caller.clone()), Some(real.clone()))?;

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
		// ... and "real" is the traditional caller. This is not a typo.
		let real: T::AccountId = whitelisted_caller();
		let real_lookup = T::Lookup::unlookup(real.clone());
		// Mint sufficient balance for announcement deposits
		let _ = <T::Currency as FunMutate<_>>::mint_into(
			&caller,
			BalanceOf::<T>::max_value() / 10u32.into(),
		);
		// Pass real so caller announces for the correct account (whitelisted_caller)
		add_announcements::<T>(a, Some(caller.clone()), Some(real.clone()))?;
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
				at: <T as Config>::BlockNumberProvider::current_block_number(),
				extrinsic_index: frame_system::Pallet::<T>::extrinsic_index().unwrap_or_default(),
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn kill_pure(p: Linear<0, { T::MaxProxies::get() - 2 }>) -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let caller_lookup = T::Lookup::unlookup(caller.clone());
		// Mint sufficient balance for operations and deposits
		let balance_amount = BalanceOf::<T>::max_value() / 100u32.into();
		let _ = <T::Currency as FunMutate<_>>::mint_into(&caller, balance_amount);
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
		let _ = <T::Currency as FunMutate<_>>::mint_into(
			&account_1,
			BalanceOf::<T>::max_value() / 100u8.into(),
		);
		let _ = <T::Currency as FunMutate<_>>::mint_into(
			&account_2,
			BalanceOf::<T>::max_value() / 100u8.into(),
		);
		let _ = <T::Currency as FunMutate<_>>::mint_into(
			&account_3,
			BalanceOf::<T>::max_value() / 100u8.into(),
		);

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
		let proxy_hold = T::Currency::balance_on_hold(&HoldReason::ProxyDeposit.into(), &account_2);
		assert_eq!(initial_proxy_deposit, proxy_hold);

		// Create announcement
		Proxy::<T>::announce(
			RawOrigin::Signed(account_2.clone()).into(),
			T::Lookup::unlookup(account_1.clone()),
			T::CallHasher::hash_of(&("add_announcement", 1)),
		)?;
		let (announcements, initial_announcement_deposit) = Announcements::<T>::get(&account_2);
		assert!(!initial_announcement_deposit.is_zero());
		let announcement_hold =
			T::Currency::balance_on_hold(&HoldReason::AnnouncementDeposit.into(), &account_2);
		let total_hold = proxy_hold.saturating_add(announcement_hold);
		assert_eq!(initial_announcement_deposit.saturating_add(initial_proxy_deposit), total_hold);

		// Artificially inflate deposits and hold the extra amount to simulate deposits being too
		// high. This is to test that poke_deposit correctly reduces the deposits and releases the
		// excess
		let extra_proxy_deposit = initial_proxy_deposit; // Double the deposit
		let extra_announcement_deposit = initial_announcement_deposit; // Double the deposit

		T::Currency::hold(&HoldReason::ProxyDeposit.into(), &account_2, extra_proxy_deposit)?;
		T::Currency::hold(
			&HoldReason::AnnouncementDeposit.into(),
			&account_2,
			extra_announcement_deposit,
		)?;

		let initial_total_hold =
			T::Currency::balance_on_hold(&HoldReason::ProxyDeposit.into(), &account_2)
				.saturating_add(T::Currency::balance_on_hold(
					&HoldReason::AnnouncementDeposit.into(),
					&account_2,
				));
		let expected_total = initial_proxy_deposit
			.saturating_add(initial_announcement_deposit)
			.saturating_add(extra_proxy_deposit)
			.saturating_add(extra_announcement_deposit);
		assert_eq!(initial_total_hold, expected_total); // Double

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

		let final_proxy_hold =
			T::Currency::balance_on_hold(&HoldReason::ProxyDeposit.into(), &account_2);
		let final_announcement_hold =
			T::Currency::balance_on_hold(&HoldReason::AnnouncementDeposit.into(), &account_2);
		let final_total_hold = final_proxy_hold.saturating_add(final_announcement_hold);
		let expected_final = initial_proxy_deposit.saturating_add(initial_announcement_deposit);
		assert_eq!(final_total_hold, expected_final);

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

	/// Benchmark the v1 migration step for proxy reserves to holds conversion.
	/// This measures the weight of migrating accounts from the old reserves system to the new holds
	/// system.
	#[benchmark]
	fn migrate_proxy_account(p: Linear<0, { T::MaxProxies::get() }>) -> Result<(), BenchmarkError> {
		use crate::migrations::v1::{
			benchmarking_helpers::BenchmarkOldCurrency, MigrateReservesToHolds, MigrationStats,
		};

		let proxy_count = p;
		let who: T::AccountId = account("proxy_owner", 0, 0);

		// Set up proxy data for benchmarking
		let deposit_per_proxy = T::ProxyDepositBase::get() + T::ProxyDepositFactor::get();
		let total_deposit = deposit_per_proxy * proxy_count.into();

		let mut proxies = Vec::new();
		for i in 0..proxy_count {
			let delegate: T::AccountId = account("proxy_delegate", i, 0);
			proxies.push(ProxyDefinition {
				delegate,
				proxy_type: T::ProxyType::default(),
				delay: BlockNumberFor::<T>::default(),
			});
		}
		let proxies: BoundedVec<_, T::MaxProxies> =
			proxies.try_into().map_err(|_| BenchmarkError::Stop("Too many proxies"))?;

		// Set up proxy storage entry
		Proxies::<T>::insert(&who, (&proxies, total_deposit));
		let mut stats = MigrationStats::default();

		#[block]
		{
			let _result =
				MigrateReservesToHolds::<
					T,
					BenchmarkOldCurrency<T>,
					crate::weights::SubstrateWeight<T>,
				>::migrate_proxy_account(&who, proxies.clone(), total_deposit, &mut stats);
		}

		Ok(())
	}

	/// Benchmark the v1 migration step for announcement reserves to holds conversion.
	/// This measures the weight of migrating announcement accounts from reserves to holds.
	#[benchmark]
	fn migrate_announcement_account(
		a: Linear<0, { T::MaxPending::get() }>,
	) -> Result<(), BenchmarkError> {
		use crate::migrations::v1::{
			benchmarking_helpers::BenchmarkOldCurrency, MigrateReservesToHolds, MigrationStats,
		};

		let announcement_count = a;
		let who: T::AccountId = account("announcement_owner", 0, 0);

		// Set up announcement data for benchmarking
		let deposit_per_announcement =
			T::AnnouncementDepositBase::get() + T::AnnouncementDepositFactor::get();
		let total_deposit = deposit_per_announcement * announcement_count.into();

		let mut announcements = Vec::new();
		for i in 0..announcement_count {
			let real: T::AccountId = account("announcement_real", i, 0);
			announcements.push(Announcement {
				real,
				call_hash: <T::CallHasher as frame::traits::Hash>::Output::default(),
				height: BlockNumberFor::<T>::default(),
			});
		}
		let announcements: BoundedVec<_, T::MaxPending> = announcements
			.try_into()
			.map_err(|_| BenchmarkError::Stop("Too many announcements"))?;

		// Set up announcement storage entry
		Announcements::<T>::insert(&who, (&announcements, total_deposit));
		let mut stats = MigrationStats::default();

		#[block]
		{
			let _result = MigrateReservesToHolds::<
				T,
				BenchmarkOldCurrency<T>,
				crate::weights::SubstrateWeight<T>,
			>::migrate_announcement_account(
				&who, announcements.clone(), total_deposit, &mut stats
			);
		}

		Ok(())
	}

	/// Benchmark the complete v1 migration process until completion.
	/// This simulates the full multi-block migration behavior.
	#[benchmark]
	fn migration_complete(
		n: Linear<1, { T::MaxProxies::get().max(T::MaxPending::get()) }>,
	) -> Result<(), BenchmarkError> {
		use crate::migrations::v1::{
			benchmarking_helpers::BenchmarkOldCurrency, MigrateReservesToHolds,
		};
		use frame::deps::frame_support::{migrations::SteppedMigration, weights::WeightMeter};

		let total_accounts = n;

		let proxy_accounts = total_accounts / 2;
		let announcement_accounts = total_accounts - proxy_accounts;
		let mut account_id = 0u32;

		let deposit_per_proxy = T::ProxyDepositBase::get() + T::ProxyDepositFactor::get();
		let proxies_per_account = T::MaxProxies::get().min(10u32); // Up to 10 proxies per account, limited by MaxProxies
		let total_proxy_deposit = deposit_per_proxy * proxies_per_account.into();

		(0..proxy_accounts).try_for_each(|_| -> Result<(), BenchmarkError> {
			let who: T::AccountId = account("proxy_owner", account_id, 0);
			account_id = account_id.saturating_add(1);

			let proxies: BoundedVec<_, T::MaxProxies> = (0..proxies_per_account)
				.map(|_| {
					let delegate: T::AccountId = account("proxy_delegate", account_id, 0);
					account_id = account_id.saturating_add(1);
					ProxyDefinition {
						delegate,
						proxy_type: T::ProxyType::default(),
						delay: BlockNumberFor::<T>::default(),
					}
				})
				.collect::<Vec<_>>()
				.try_into()
				.map_err(|_| BenchmarkError::Stop("Too many proxies"))?;

			Proxies::<T>::insert(&who, (&proxies, total_proxy_deposit));
			Ok(())
		})?;

		let deposit_per_announcement =
			T::AnnouncementDepositBase::get() + T::AnnouncementDepositFactor::get();
		let announcements_per_account = T::MaxPending::get().min(5u32); // Up to 5 announcements per account, limited by MaxPending
		let total_announcement_deposit =
			deposit_per_announcement * announcements_per_account.into();

		(0..announcement_accounts).try_for_each(|_| -> Result<(), BenchmarkError> {
			let who: T::AccountId = account("announcement_owner", account_id, 0);
			account_id = account_id.saturating_add(1);

			let announcements: BoundedVec<_, T::MaxPending> = (0..announcements_per_account)
				.map(|i| {
					let real: T::AccountId = account("announcement_real", account_id + i, 0);
					Announcement {
						real,
						call_hash: <T::CallHasher as frame::traits::Hash>::Output::default(),
						height: BlockNumberFor::<T>::default(),
					}
				})
				.collect::<Vec<_>>()
				.try_into()
				.map_err(|_| BenchmarkError::Stop("Too many announcements"))?;

			account_id = account_id.saturating_add(announcements_per_account);
			Announcements::<T>::insert(&who, (&announcements, total_announcement_deposit));
			Ok(())
		})?;

		#[block]
		{
			// Process complete migration - loop until completion
			let mut cursor = None;
			loop {
				let block_weights = T::BlockWeights::get();
				let weight_limit = block_weights.max_block.saturating_div(10);
				let mut meter = WeightMeter::with_limit(weight_limit);

				match MigrateReservesToHolds::<
					T,
					BenchmarkOldCurrency<T>,
					crate::weights::SubstrateWeight<T>,
				>::step(cursor, &mut meter)
				{
					Ok(Some(next_cursor)) => {
						cursor = Some(next_cursor);
					},
					Ok(None) => {
						// Migration complete
						break;
					},
					Err(_) => return Err(BenchmarkError::Stop("Migration step failed")),
				}
			}
		}

		Ok(())
	}

	impl_benchmark_test_suite!(Proxy, crate::tests::new_test_ext(), crate::tests::Test);
}
