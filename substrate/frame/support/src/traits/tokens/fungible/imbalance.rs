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

//! The imbalance type and its associates, which handles keeps everything adding up properly with
//! unbalanced operations.
//!
//! See the [`crate::traits::fungible`] doc for more information about fungible traits.

use super::{super::Imbalance as ImbalanceT, Balanced, *};
use crate::traits::{
	fungibles,
	misc::{SameOrOther, TryDrop},
	tokens::{AssetId, Balance},
};
use frame_support_procedural::{EqNoBound, PartialEqNoBound, RuntimeDebugNoBound};
use sp_runtime::traits::Zero;
use sp_std::marker::PhantomData;

/// Handler for when an imbalance gets dropped. This could handle either a credit (negative) or
/// debt (positive) imbalance.
pub trait HandleImbalanceDrop<Balance> {
	/// Some something with the imbalance's value which is being dropped.
	fn handle(amount: Balance);
}

impl<Balance> HandleImbalanceDrop<Balance> for () {
	fn handle(_: Balance) {}
}

/// An imbalance in the system, representing a divergence of recorded token supply from the sum of
/// the balances of all accounts. This is `must_use` in order to ensure it gets handled (placing
/// into an account, settling from an account or altering the supply).
///
/// Importantly, it has a special `Drop` impl, and cannot be created outside of this module.
#[must_use]
#[derive(EqNoBound, PartialEqNoBound, RuntimeDebugNoBound)]
pub struct Imbalance<
	B: Balance,
	OnDrop: HandleImbalanceDrop<B>,
	OppositeOnDrop: HandleImbalanceDrop<B>,
> {
	amount: B,
	_phantom: PhantomData<(OnDrop, OppositeOnDrop)>,
}

impl<B: Balance, OnDrop: HandleImbalanceDrop<B>, OppositeOnDrop: HandleImbalanceDrop<B>> Drop
	for Imbalance<B, OnDrop, OppositeOnDrop>
{
	fn drop(&mut self) {
		if !self.amount.is_zero() {
			OnDrop::handle(self.amount)
		}
	}
}

impl<B: Balance, OnDrop: HandleImbalanceDrop<B>, OppositeOnDrop: HandleImbalanceDrop<B>> TryDrop
	for Imbalance<B, OnDrop, OppositeOnDrop>
{
	/// Drop an instance cleanly. Only works if its value represents "no-operation".
	fn try_drop(self) -> Result<(), Self> {
		self.drop_zero()
	}
}

impl<B: Balance, OnDrop: HandleImbalanceDrop<B>, OppositeOnDrop: HandleImbalanceDrop<B>> Default
	for Imbalance<B, OnDrop, OppositeOnDrop>
{
	fn default() -> Self {
		Self::zero()
	}
}

impl<B: Balance, OnDrop: HandleImbalanceDrop<B>, OppositeOnDrop: HandleImbalanceDrop<B>>
	Imbalance<B, OnDrop, OppositeOnDrop>
{
	pub(crate) fn new(amount: B) -> Self {
		Self { amount, _phantom: PhantomData }
	}

	/// Forget the imbalance without invoking the on-drop handler.
	pub(crate) fn forget(imbalance: Self) {
		sp_std::mem::forget(imbalance);
	}
}

impl<B: Balance, OnDrop: HandleImbalanceDrop<B>, OppositeOnDrop: HandleImbalanceDrop<B>>
	ImbalanceT<B> for Imbalance<B, OnDrop, OppositeOnDrop>
{
	type Opposite = Imbalance<B, OppositeOnDrop, OnDrop>;

	fn zero() -> Self {
		Self { amount: Zero::zero(), _phantom: PhantomData }
	}

	fn drop_zero(self) -> Result<(), Self> {
		if self.amount.is_zero() {
			sp_std::mem::forget(self);
			Ok(())
		} else {
			Err(self)
		}
	}

	fn split(self, amount: B) -> (Self, Self) {
		let first = self.amount.min(amount);
		let second = self.amount - first;
		sp_std::mem::forget(self);
		(Imbalance::new(first), Imbalance::new(second))
	}

	fn extract(&mut self, amount: B) -> Self {
		let new = self.amount.min(amount);
		self.amount = self.amount - new;
		Imbalance::new(new)
	}

	fn merge(mut self, other: Self) -> Self {
		self.amount = self.amount.saturating_add(other.amount);
		sp_std::mem::forget(other);
		self
	}
	fn subsume(&mut self, other: Self) {
		self.amount = self.amount.saturating_add(other.amount);
		sp_std::mem::forget(other);
	}
	fn offset(
		self,
		other: Imbalance<B, OppositeOnDrop, OnDrop>,
	) -> SameOrOther<Self, Imbalance<B, OppositeOnDrop, OnDrop>> {
		let (a, b) = (self.amount, other.amount);
		sp_std::mem::forget((self, other));

		if a == b {
			SameOrOther::None
		} else if a > b {
			SameOrOther::Same(Imbalance::new(a - b))
		} else {
			SameOrOther::Other(Imbalance::<B, OppositeOnDrop, OnDrop>::new(b - a))
		}
	}
	fn peek(&self) -> B {
		self.amount
	}
}

/// Converts a `fungibles` `imbalance` instance to an instance of a `fungible` imbalance type.
///
/// This function facilitates imbalance conversions within the implementations of
/// [`frame_support::traits::fungibles::UnionOf`], [`frame_support::traits::fungible::UnionOf`], and
/// [`frame_support::traits::fungible::ItemOf`] adapters. It is intended only for internal use
/// within the current crate.
pub(crate) fn from_fungibles<
	A: AssetId,
	B: Balance,
	OnDropIn: fungibles::HandleImbalanceDrop<A, B>,
	OppositeIn: fungibles::HandleImbalanceDrop<A, B>,
	OnDropOut: HandleImbalanceDrop<B>,
	OppositeOut: HandleImbalanceDrop<B>,
>(
	imbalance: fungibles::Imbalance<A, B, OnDropIn, OppositeIn>,
) -> Imbalance<B, OnDropOut, OppositeOut> {
	let new = Imbalance::new(imbalance.peek());
	fungibles::Imbalance::forget(imbalance);
	new
}

/// Imbalance implying that the total_issuance value is less than the sum of all account balances.
pub type Debt<AccountId, B> = Imbalance<
	<B as Inspect<AccountId>>::Balance,
	// This will generally be implemented by increasing the total_issuance value.
	<B as Balanced<AccountId>>::OnDropDebt,
	<B as Balanced<AccountId>>::OnDropCredit,
>;

/// Imbalance implying that the total_issuance value is greater than the sum of all account
/// balances.
pub type Credit<AccountId, B> = Imbalance<
	<B as Inspect<AccountId>>::Balance,
	// This will generally be implemented by decreasing the total_issuance value.
	<B as Balanced<AccountId>>::OnDropCredit,
	<B as Balanced<AccountId>>::OnDropDebt,
>;
