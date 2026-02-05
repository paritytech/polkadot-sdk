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
// limitations under the License

use crate::{evm::fees::InfoT, BalanceOf, Config, StorageDeposit};
use frame_support::DebugNoBound;
use sp_core::Get;
use sp_runtime::{FixedPointNumber, Saturating};

/// The type for negative and positive gas amounts.
///
/// We need to be able to represent negative amounts as they occur when storage deposit refunds are
/// involved.
///
/// The structure of this type resembles `StorageDeposit` but the enum variants have a more obvious
/// name to avoid confusion and errors
#[derive(Clone, Eq, PartialEq, DebugNoBound)]
pub enum SignedGas<T: Config> {
	/// Positive gas amount
	Positive(BalanceOf<T>),
	/// Negative gas amount
	/// Invariant: `BalanceOf<T>` is never 0 for `Negative`
	Negative(BalanceOf<T>),
}

use SignedGas::{Negative, Positive};

impl<T: Config> Default for SignedGas<T> {
	fn default() -> Self {
		Self::Positive(Default::default())
	}
}

impl<T: Config> SignedGas<T> {
	/// Safely construct a negative `SignedGas` amount
	///
	/// Ensures the invariant that `Negative` must not be used for zero
	pub fn safe_new_negative(amount: BalanceOf<T>) -> Self {
		if amount == Default::default() {
			Positive(amount)
		} else {
			Negative(amount)
		}
	}

	/// Transform a weight fee into a gas amount.
	pub fn from_weight_fee(weight_fee: BalanceOf<T>) -> Self {
		Self::Positive(weight_fee)
	}

	/// Transform an Ethereum gas amount coming from outside the metering system and transform into
	/// the internally used SignedGas.
	pub fn from_ethereum_gas(gas: BalanceOf<T>) -> Self {
		let gas_scale = <T as Config>::GasScale::get();
		Self::Positive(gas.saturating_mul(gas_scale.into()))
	}

	/// Transform a storage deposit into a gas value. The value will be adjusted by dividing it
	/// through the next fee multiplier. Charges are treated as a positive numbers and refunds as
	/// negative numbers.
	pub fn from_adjusted_deposit_charge(deposit: &StorageDeposit<BalanceOf<T>>) -> Self {
		let multiplier = T::FeeInfo::next_fee_multiplier_reciprocal();

		match deposit {
			StorageDeposit::Charge(amount) => Positive(multiplier.saturating_mul_int(*amount)),
			StorageDeposit::Refund(amount) =>
				Self::safe_new_negative(multiplier.saturating_mul_int(*amount)),
		}
	}

	/// Transform the gas amount to a weight fee amount
	/// Returns None if the gas amount is negative.
	pub fn to_weight_fee(&self) -> Option<BalanceOf<T>> {
		match self {
			Positive(amount) => Some(*amount),
			Negative(..) => None,
		}
	}

	/// Transform the gas amount to an Ethereum gas amount usable for external purposes
	/// Returns None if the gas amount is negative.
	pub fn to_ethereum_gas(&self) -> Option<BalanceOf<T>> {
		let gas_scale: BalanceOf<T> = <T as Config>::GasScale::get().into();

		match self {
			Positive(amount) =>
				Some((amount.saturating_add(gas_scale.saturating_sub(1u32.into()))) / gas_scale),
			Negative(..) => None,
		}
	}

	/// Transform the gas amount to a deposit charge. The amount will be adjusted by multiplying it
	/// with the next fee multiplier.
	/// Returns None if the gas amount is negative.
	pub fn to_adjusted_deposit_charge(&self) -> Option<BalanceOf<T>> {
		match self {
			Positive(amount) => {
				let multiplier = T::FeeInfo::next_fee_multiplier();
				Some(multiplier.saturating_mul_int(*amount))
			},
			_ => None,
		}
	}

	/// This is essentially a saturating signed add.
	pub fn saturating_add(&self, rhs: &Self) -> Self {
		match (self, rhs) {
			(Positive(lhs), Positive(rhs)) => Positive(lhs.saturating_add(*rhs)),
			(Negative(lhs), Negative(rhs)) => Self::safe_new_negative(lhs.saturating_add(*rhs)),
			(Positive(lhs), Negative(rhs)) =>
				if lhs >= rhs {
					Positive(lhs.saturating_sub(*rhs))
				} else {
					Self::safe_new_negative(rhs.saturating_sub(*lhs))
				},
			(Negative(lhs), Positive(rhs)) =>
				if lhs > rhs {
					Self::safe_new_negative(lhs.saturating_sub(*rhs))
				} else {
					Positive(rhs.saturating_sub(*lhs))
				},
		}
	}

	/// This is essentially a saturating signed sub.
	pub fn saturating_sub(&self, rhs: &Self) -> Self {
		match (self, rhs) {
			(Positive(lhs), Negative(rhs)) => Positive(lhs.saturating_add(*rhs)),
			(Negative(lhs), Positive(rhs)) => Self::safe_new_negative(lhs.saturating_add(*rhs)),
			(Positive(lhs), Positive(rhs)) =>
				if lhs >= rhs {
					Positive(lhs.saturating_sub(*rhs))
				} else {
					Self::safe_new_negative(rhs.saturating_sub(*lhs))
				},
			(Negative(lhs), Negative(rhs)) =>
				if lhs > rhs {
					Self::safe_new_negative(lhs.saturating_sub(*rhs))
				} else {
					Positive(rhs.saturating_sub(*lhs))
				},
		}
	}

	// Determine the minimum of two signed gas values.
	pub fn min(&self, other: &Self) -> Self {
		match (self, other) {
			(Positive(_), Negative(rhs)) => Self::safe_new_negative(*rhs),
			(Negative(lhs), Positive(_)) => Self::safe_new_negative(*lhs),
			(Positive(lhs), Positive(rhs)) => Positive((*lhs).min(*rhs)),
			(Negative(lhs), Negative(rhs)) => Self::safe_new_negative((*lhs).max(*rhs)),
		}
	}
}
