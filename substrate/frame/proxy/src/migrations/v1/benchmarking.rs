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

//! Benchmarks for pallet-proxy migration v1.

use super::migration::*;
use crate::{
	weights::SubstrateWeight as DefaultWeights, Announcement, Announcements, BalanceOf,
	BlockNumberFor, Config, Proxies, ProxyDefinition,
};
use alloc::vec::Vec;
use frame::{
	benchmarking::prelude::*,
	deps::frame_support::{
		migrations::SteppedMigration,
		traits::{Currency, Imbalance, ReservableCurrency},
		BoundedVec,
	},
};

// Custom imbalance types for no_std WASM benchmarking
#[derive(Debug, PartialEq, Eq)]
pub struct BenchmarkPositiveImbalance<T: Config>(BalanceOf<T>, core::marker::PhantomData<T>);

impl<T: Config> BenchmarkPositiveImbalance<T> {
	fn new(amount: BalanceOf<T>) -> Self {
		Self(amount, core::marker::PhantomData)
	}
}

impl<T: Config> Default for BenchmarkPositiveImbalance<T> {
	fn default() -> Self {
		Self::new(Zero::zero())
	}
}

impl<T: Config> frame::deps::frame_support::traits::TryDrop for BenchmarkPositiveImbalance<T> {
	fn try_drop(self) -> Result<(), Self> {
		self.0.is_zero().then_some(()).ok_or(self)
	}
}

impl<T: Config> frame::deps::frame_support::traits::tokens::imbalance::TryMerge
	for BenchmarkPositiveImbalance<T>
{
	fn try_merge(self, other: Self) -> Result<Self, (Self, Self)> {
		Ok(Self::new(self.0.saturating_add(other.0)))
	}
}

impl<T: Config> frame::deps::frame_support::traits::Imbalance<BalanceOf<T>>
	for BenchmarkPositiveImbalance<T>
{
	type Opposite = BenchmarkNegativeImbalance<T>;

	fn zero() -> Self {
		Self::new(Zero::zero())
	}

	fn drop_zero(self) -> Result<(), Self> {
		self.0.is_zero().then_some(()).ok_or(self)
	}

	fn split(self, amount: BalanceOf<T>) -> (Self, Self) {
		let first = self.0.min(amount);
		let second = self.0 - first;
		(Self::new(first), Self::new(second))
	}

	fn extract(&mut self, amount: BalanceOf<T>) -> Self {
		let new = self.0.min(amount);
		self.0 = self.0 - new;
		Self::new(new)
	}

	fn merge(self, other: Self) -> Self {
		Self::new(self.0.saturating_add(other.0))
	}

	fn subsume(&mut self, other: Self) {
		self.0 = self.0.saturating_add(other.0);
	}

	fn offset(
		self,
		other: Self::Opposite,
	) -> frame::deps::frame_support::traits::SameOrOther<Self, Self::Opposite> {
		use frame::deps::frame_support::traits::SameOrOther;
		let (a, b) = (self.0, other.0);
		match a.cmp(&b) {
			core::cmp::Ordering::Greater => SameOrOther::Same(Self::new(a - b)),
			core::cmp::Ordering::Less => SameOrOther::Other(BenchmarkNegativeImbalance::new(b - a)),
			core::cmp::Ordering::Equal => SameOrOther::None,
		}
	}

	fn peek(&self) -> BalanceOf<T> {
		self.0
	}
}

#[derive(Debug, PartialEq, Eq)]
pub struct BenchmarkNegativeImbalance<T: Config>(BalanceOf<T>, core::marker::PhantomData<T>);

impl<T: Config> BenchmarkNegativeImbalance<T> {
	fn new(amount: BalanceOf<T>) -> Self {
		Self(amount, core::marker::PhantomData)
	}
}

impl<T: Config> Default for BenchmarkNegativeImbalance<T> {
	fn default() -> Self {
		Self::new(Zero::zero())
	}
}

impl<T: Config> frame::deps::frame_support::traits::TryDrop for BenchmarkNegativeImbalance<T> {
	fn try_drop(self) -> Result<(), Self> {
		self.0.is_zero().then_some(()).ok_or(self)
	}
}

impl<T: Config> frame::deps::frame_support::traits::tokens::imbalance::TryMerge
	for BenchmarkNegativeImbalance<T>
{
	fn try_merge(self, other: Self) -> Result<Self, (Self, Self)> {
		Ok(Self::new(self.0.saturating_add(other.0)))
	}
}

impl<T: Config> frame::deps::frame_support::traits::Imbalance<BalanceOf<T>>
	for BenchmarkNegativeImbalance<T>
{
	type Opposite = BenchmarkPositiveImbalance<T>;

	fn zero() -> Self {
		Self::new(Zero::zero())
	}

	fn drop_zero(self) -> Result<(), Self> {
		self.0.is_zero().then_some(()).ok_or(self)
	}

	fn split(self, amount: BalanceOf<T>) -> (Self, Self) {
		let first = self.0.min(amount);
		let second = self.0 - first;
		(Self::new(first), Self::new(second))
	}

	fn extract(&mut self, amount: BalanceOf<T>) -> Self {
		let new = self.0.min(amount);
		self.0 = self.0 - new;
		Self::new(new)
	}

	fn merge(self, other: Self) -> Self {
		Self::new(self.0.saturating_add(other.0))
	}

	fn subsume(&mut self, other: Self) {
		self.0 = self.0.saturating_add(other.0);
	}

	fn offset(
		self,
		other: Self::Opposite,
	) -> frame::deps::frame_support::traits::SameOrOther<Self, Self::Opposite> {
		use frame::deps::frame_support::traits::SameOrOther;
		let (a, b) = (self.0, other.0);
		match a.cmp(&b) {
			core::cmp::Ordering::Greater => SameOrOther::Same(Self::new(a - b)),
			core::cmp::Ordering::Less => SameOrOther::Other(BenchmarkPositiveImbalance::new(b - a)),
			core::cmp::Ordering::Equal => SameOrOther::None,
		}
	}

	fn peek(&self) -> BalanceOf<T> {
		self.0
	}
}

// Minimal stateless currency for benchmarking in no_std WASM runtimes
pub struct BenchmarkOldCurrency<T: Config>(core::marker::PhantomData<T>);

impl<T: Config> Currency<T::AccountId> for BenchmarkOldCurrency<T> {
	type Balance = BalanceOf<T>;
	type PositiveImbalance = BenchmarkPositiveImbalance<T>;
	type NegativeImbalance = BenchmarkNegativeImbalance<T>;

	fn total_balance(_who: &T::AccountId) -> Self::Balance {
		10000u32.into()
	}
	fn can_slash(_who: &T::AccountId, _value: Self::Balance) -> bool {
		true
	}
	fn total_issuance() -> Self::Balance {
		1_000_000u32.into()
	}
	fn minimum_balance() -> Self::Balance {
		1u32.into()
	}
	fn burn(_value: Self::Balance) -> Self::PositiveImbalance {
		BenchmarkPositiveImbalance::zero()
	}
	fn issue(_value: Self::Balance) -> Self::NegativeImbalance {
		BenchmarkNegativeImbalance::zero()
	}
	fn free_balance(_who: &T::AccountId) -> Self::Balance {
		10000u32.into()
	}
	fn ensure_can_withdraw(
		_who: &T::AccountId,
		_amount: Self::Balance,
		_reasons: frame::deps::frame_support::traits::WithdrawReasons,
		_new_balance: Self::Balance,
	) -> crate::DispatchResult {
		Ok(())
	}
	fn transfer(
		_source: &T::AccountId,
		_dest: &T::AccountId,
		_value: Self::Balance,
		_existence_requirement: frame::deps::frame_support::traits::ExistenceRequirement,
	) -> Result<(), crate::DispatchError> {
		Ok(())
	}
	fn slash(
		_who: &T::AccountId,
		_value: Self::Balance,
	) -> (Self::NegativeImbalance, Self::Balance) {
		(BenchmarkNegativeImbalance::zero(), 0u32.into())
	}
	fn deposit_into_existing(
		_who: &T::AccountId,
		_value: Self::Balance,
	) -> Result<Self::PositiveImbalance, crate::DispatchError> {
		Ok(BenchmarkPositiveImbalance::zero())
	}
	fn withdraw(
		_who: &T::AccountId,
		_value: Self::Balance,
		_reasons: frame::deps::frame_support::traits::WithdrawReasons,
		_liveness: frame::deps::frame_support::traits::ExistenceRequirement,
	) -> Result<Self::NegativeImbalance, crate::DispatchError> {
		Ok(BenchmarkNegativeImbalance::zero())
	}
	fn deposit_creating(_who: &T::AccountId, _value: Self::Balance) -> Self::PositiveImbalance {
		BenchmarkPositiveImbalance::zero()
	}
	fn make_free_balance_be(
		_who: &T::AccountId,
		_balance: Self::Balance,
	) -> frame::deps::frame_support::traits::SignedImbalance<Self::Balance, Self::PositiveImbalance>
	{
		frame::deps::frame_support::traits::SignedImbalance::Positive(
			BenchmarkPositiveImbalance::zero(),
		)
	}
}

impl<T: Config> ReservableCurrency<T::AccountId> for BenchmarkOldCurrency<T> {
	fn can_reserve(_who: &T::AccountId, _value: Self::Balance) -> bool {
		true
	}
	fn reserved_balance(_who: &T::AccountId) -> Self::Balance {
		1000u32.into()
	} // Assume some reserves for benchmarking
	fn reserve(_who: &T::AccountId, _value: Self::Balance) -> crate::DispatchResult {
		Ok(())
	}
	fn unreserve(_who: &T::AccountId, _value: Self::Balance) -> Self::Balance {
		0u32.into()
	} // All unreserved
	fn slash_reserved(
		_who: &T::AccountId,
		_value: Self::Balance,
	) -> (Self::NegativeImbalance, Self::Balance) {
		(BenchmarkNegativeImbalance::zero(), 0u32.into())
	}
	fn repatriate_reserved(
		_slashed: &T::AccountId,
		_beneficiary: &T::AccountId,
		_value: Self::Balance,
		_status: frame::deps::frame_support::traits::BalanceStatus,
	) -> Result<Self::Balance, crate::DispatchError> {
		Ok(0u32.into())
	}
}

/// Benchmarking functions for migration v1
/// These can be used to generate accurate weights for the migration
pub mod benchmarks {
	use super::*;
	use frame::benchmarking::prelude::account;

	/// Benchmark migrating a proxy account with varying numbers of proxies
	pub fn migrate_proxy_account<T: Config>(
		proxy_count: u32,
	) -> Result<frame::deps::frame_support::weights::Weight, &'static str>
	where
		BalanceOf<T>: From<u32>,
		T::ProxyType: Default,
		BlockNumberFor<T>: Default,
	{
		let who: T::AccountId = account("proxy_owner", 0, 0);
		let deposit_per_proxy = T::ProxyDepositBase::get() + T::ProxyDepositFactor::get();
		let total_deposit = deposit_per_proxy * proxy_count.into();

		// Create proxy definitions
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
			proxies.try_into().map_err(|_| "Too many proxies for benchmark")?;

		// Set up proxy storage entry
		Proxies::<T>::insert(&who, (&proxies, total_deposit));

		let mut stats = MigrationStats::default();

		let result = MigrateReservesToHolds::<
			T,
			BenchmarkOldCurrency<T>,
			DefaultWeights<T>,
		>::migrate_proxy_account(&who, proxies, total_deposit, &mut stats);

		Ok(result.weight())
	}

	/// Benchmark migrating an announcement account with varying numbers of announcements
	pub fn migrate_announcement_account<T: Config>(
		announcement_count: u32,
	) -> Result<frame::deps::frame_support::weights::Weight, &'static str>
	where
		BalanceOf<T>: From<u32>,
		T::CallHasher: Default,
		BlockNumberFor<T>: Default,
	{
		let who: T::AccountId = account("announcement_owner", 0, 0);
		let deposit_per_announcement =
			T::AnnouncementDepositBase::get() + T::AnnouncementDepositFactor::get();
		let total_deposit = deposit_per_announcement * announcement_count.into();

		// Create announcement definitions
		let mut announcements = Vec::new();
		for i in 0..announcement_count {
			let real: T::AccountId = account("announcement_real", i, 0);
			announcements.push(Announcement {
				real,
				call_hash: <T::CallHasher as frame::traits::Hash>::Output::default(),
				height: BlockNumberFor::<T>::default(),
			});
		}
		let announcements: BoundedVec<_, T::MaxPending> =
			announcements.try_into().map_err(|_| "Too many announcements for benchmark")?;

		// Set up announcement storage entry
		Announcements::<T>::insert(&who, (&announcements, total_deposit));

		let mut stats = MigrationStats::default();

		let result =
			MigrateReservesToHolds::<
				T,
				BenchmarkOldCurrency<T>,
				DefaultWeights<T>,
			>::migrate_announcement_account(&who, announcements, total_deposit, &mut stats);

		Ok(result.weight())
	}

	/// Benchmark the complete migration process with varying numbers of accounts
	/// This simulates the multi-block migration behavior until completion
	pub fn migration_complete<T: Config>(
		total_accounts: u32,
	) -> Result<frame::deps::frame_support::weights::Weight, &'static str>
	where
		BalanceOf<T>: From<u32>,
		T::ProxyType: Default,
		T::CallHasher: Default,
		BlockNumberFor<T>: Default,
	{
		use frame::deps::frame_support::weights::WeightMeter;

		// Set up mixed test data
		let proxy_accounts = total_accounts / 2;
		let announcement_accounts = total_accounts - proxy_accounts;

		// Set up proxy accounts
		let mut account_id = 0u32;

		// Create proxy accounts
		for _i in 0..proxy_accounts {
			let who: T::AccountId = account("proxy_owner", account_id, 0);
			account_id = account_id.saturating_add(1);

			let deposit_per_proxy = T::ProxyDepositBase::get() + T::ProxyDepositFactor::get();
			let proxies_per_account = 2; // Fixed to 2 proxies per account for consistent benchmarking
			let total_deposit = deposit_per_proxy * proxies_per_account.into();

			let mut proxies = Vec::new();
			for _j in 0..proxies_per_account {
				let delegate: T::AccountId = account("proxy_delegate", account_id, 0);
				account_id = account_id.saturating_add(1);
				proxies.push(ProxyDefinition {
					delegate,
					proxy_type: T::ProxyType::default(),
					delay: BlockNumberFor::<T>::default(),
				});
			}
			let proxies: BoundedVec<_, T::MaxProxies> =
				proxies.try_into().map_err(|_| "Too many proxies for benchmark")?;

			Proxies::<T>::insert(&who, (&proxies, total_deposit));
		}

		// Create announcement accounts
		for _i in 0..announcement_accounts {
			let who: T::AccountId = account("announcement_owner", account_id, 0);
			account_id = account_id.saturating_add(1);

			let deposit_per_announcement =
				T::AnnouncementDepositBase::get() + T::AnnouncementDepositFactor::get();
			let announcements_per_account = 1; // Fixed to 1 announcement per account
			let total_deposit = deposit_per_announcement * announcements_per_account.into();

			let mut announcements = Vec::new();
			let real: T::AccountId = account("announcement_real", account_id, 0);
			account_id = account_id.saturating_add(1);
			announcements.push(Announcement {
				real,
				call_hash: <T::CallHasher as frame::traits::Hash>::Output::default(),
				height: BlockNumberFor::<T>::default(),
			});
			let announcements: BoundedVec<_, T::MaxPending> =
				announcements.try_into().map_err(|_| "Too many announcements for benchmark")?;

			Announcements::<T>::insert(&who, (&announcements, total_deposit));
		}

		// Process complete migration and measure total weight
		let mut cursor = None;
		let mut total_weight = frame::deps::frame_support::weights::Weight::zero();

		// Loop until migration is complete - assuming it will end in benchmarking
		loop {
			// Use 10% of max block weight to simulate realistic multi-block behavior
			let block_weights = T::BlockWeights::get();
			let weight_limit = block_weights.max_block.saturating_div(10);
			let mut meter = WeightMeter::with_limit(weight_limit);

			// Execute migration step
			match MigrateReservesToHolds::<T, BenchmarkOldCurrency<T>, DefaultWeights<T>>::step(
				cursor, &mut meter,
			) {
				Ok(Some(next_cursor)) => {
					// Migration continues
					total_weight = total_weight.saturating_add(meter.consumed());
					cursor = Some(next_cursor);
				},
				Ok(None) => {
					// Migration complete
					total_weight = total_weight.saturating_add(meter.consumed());
					break;
				},
				Err(_) => return Err("Migration step failed"),
			}
		}

		Ok(total_weight)
	}
}
