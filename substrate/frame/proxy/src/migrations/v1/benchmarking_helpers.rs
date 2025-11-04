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

//! Helpers for benchmarking pallet-proxy migration v1.

use crate::{BalanceOf, Config};
use frame::{
	arithmetic::{Saturating, Zero},
	deps::frame_support::traits::{Currency, Imbalance, ReservableCurrency},
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
		10000u32.into()
	}
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
