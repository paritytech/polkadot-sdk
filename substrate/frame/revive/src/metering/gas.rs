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

use crate::{evm::fees::InfoT, BalanceOf, Config, StorageDeposit};
use frame_support::{traits::tokens::Balance as BalanceT, DebugNoBound};
use sp_core::Get;
use sp_runtime::FixedPointNumber;

/// Internal scaled representation of Ethereum Gas
/// Compared to the Ethereum Gas amounts that are visible externally, this is scaled by
/// `Config::GasScale`
#[derive(Debug, Eq, PartialEq, Clone, Copy, Default)]
pub struct InternalGas<Balance>(Balance);

impl<Balance: BalanceT> InternalGas<Balance> {
	pub fn into_external_gas<T>(self) -> Balance
	where
		T: Config<Balance = Balance>,
	{
		let gas_scale = <T as Config>::GasScale::get();

		self.0 / gas_scale
	}

	pub fn into_weight_fee(self) -> Balance {
		self.0
	}

	pub fn from_weight_fee(weight_fee: Balance) -> Self {
		Self(weight_fee)
	}

	pub fn from_external_gas<T>(gas: Balance) -> Self
	where
		T: Config<Balance = Balance>,
	{
		let gas_scale = <T as Config>::GasScale::get();

		Self(gas.saturating_mul(gas_scale))
	}

	pub fn saturating_add(&self, rhs: &Self) -> Self {
		Self(self.0.saturating_add(rhs.0))
	}

	pub fn saturating_sub(&self, rhs: &Self) -> Self {
		Self(self.0.saturating_sub(rhs.0))
	}

	pub fn min(self, other: Self) -> Self {
		Self(self.0.min(other.0))
	}

	pub fn into_adjusted_deposit<T>(self) -> Balance
	where
		T: Config<Balance = Balance>,
	{
		let multiplier = T::FeeInfo::next_fee_multiplier();

		multiplier.saturating_mul_int(self.0)
	}
}

/// The signed version of internal gas.
/// The structure of this type resembles `StorageDeposit` but the enum variants have a more obvious
/// name to avoid confusion and errors
#[derive(Clone, Eq, PartialEq, DebugNoBound)]
pub enum SignedGas<T: Config> {
	/// Positive gas amount
	Positive(InternalGas<BalanceOf<T>>),
	/// Negative gas amount
	Negative(InternalGas<BalanceOf<T>>),
}

impl<T: Config> Default for SignedGas<T> {
	fn default() -> Self {
		Self::Positive(Default::default())
	}
}

impl<T: Config> SignedGas<T> {
	pub fn from_weight_fee(weight_fee: BalanceOf<T>) -> Self {
		Self::Positive(InternalGas::from_weight_fee(weight_fee))
	}

	pub fn from_external_gas(gas: BalanceOf<T>) -> Self {
		Self::Positive(InternalGas::from_external_gas::<T>(gas))
	}

	/// This is essentially a saturating signed add.
	pub fn saturating_add(&self, rhs: &Self) -> Self {
		use SignedGas::*;
		match (self, rhs) {
			(Positive(lhs), Positive(rhs)) => Positive(lhs.saturating_add(rhs)),
			(Negative(lhs), Negative(rhs)) => Negative(lhs.saturating_add(rhs)),
			(Positive(lhs), Negative(rhs)) =>
				if lhs.0 >= rhs.0 {
					Positive(lhs.saturating_sub(rhs))
				} else {
					Negative(rhs.saturating_sub(lhs))
				},
			(Negative(lhs), Positive(rhs)) =>
				if lhs.0 > rhs.0 {
					Negative(lhs.saturating_sub(rhs))
				} else {
					Positive(rhs.saturating_sub(lhs))
				},
		}
	}

	/// This is essentially a saturating signed sub.
	pub fn saturating_sub(&self, rhs: &Self) -> Self {
		use SignedGas::*;
		match (self, rhs) {
			(Positive(lhs), Negative(rhs)) => Positive(lhs.saturating_add(rhs)),
			(Negative(lhs), Positive(rhs)) => Negative(lhs.saturating_add(rhs)),
			(Positive(lhs), Positive(rhs)) =>
				if lhs.0 >= rhs.0 {
					Positive(lhs.saturating_sub(rhs))
				} else {
					Negative(rhs.saturating_sub(lhs))
				},
			(Negative(lhs), Negative(rhs)) =>
				if lhs.0 > rhs.0 {
					Negative(lhs.saturating_sub(rhs))
				} else {
					Positive(rhs.saturating_sub(lhs))
				},
		}
	}

	/// transform a storage deposit into a gas value and treat a charge as a positive number
	pub fn from_adjusted_deposit_charge(deposit: &StorageDeposit<BalanceOf<T>>) -> Self {
		use SignedGas::*;

		let multiplier = T::FeeInfo::next_fee_multiplier_reciprocal();
		match deposit {
			StorageDeposit::Charge(amount) =>
				Positive(InternalGas(multiplier.saturating_mul_int(*amount))),
			StorageDeposit::Refund(amount) if *amount == Default::default() =>
				Positive(InternalGas(*amount)),
			StorageDeposit::Refund(amount) =>
				Negative(InternalGas(multiplier.saturating_mul_int(*amount))),
		}
	}

	/// Return the balance of the `SignedGas` if it is `Positive`, otherwise return `None`
	pub fn as_positive(&self) -> Option<InternalGas<BalanceOf<T>>> {
		use SignedGas::*;

		match self {
			Positive(amount) => Some(amount.clone()),
			Negative(_amount) => None,
		}
	}
}
