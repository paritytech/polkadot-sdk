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
//! See the [`crate::traits::fungibles`] doc for more information about fungibles traits.

use super::*;
use crate::traits::{
	fungible,
	misc::{SameOrOther, TryDrop},
	tokens::{imbalance::Imbalance as ImbalanceT, AssetId, Balance},
};
use frame_support_procedural::{EqNoBound, PartialEqNoBound, RuntimeDebugNoBound};
use sp_runtime::traits::Zero;
use sp_std::marker::PhantomData;

/// Handler for when an imbalance gets dropped. This could handle either a credit (negative) or
/// debt (positive) imbalance.
pub trait HandleImbalanceDrop<AssetId, Balance> {
	fn handle(asset: AssetId, amount: Balance);
}

/// An imbalance in the system, representing a divergence of recorded token supply from the sum of
/// the balances of all accounts. This is `must_use` in order to ensure it gets handled (placing
/// into an account, settling from an account or altering the supply).
///
/// Importantly, it has a special `Drop` impl, and cannot be created outside of this module.
#[must_use]
#[derive(EqNoBound, PartialEqNoBound, RuntimeDebugNoBound)]
pub struct Imbalance<
	A: AssetId,
	B: Balance,
	OnDrop: HandleImbalanceDrop<A, B>,
	OppositeOnDrop: HandleImbalanceDrop<A, B>,
> {
	asset: A,
	amount: B,
	_phantom: PhantomData<(OnDrop, OppositeOnDrop)>,
}

impl<
		A: AssetId,
		B: Balance,
		OnDrop: HandleImbalanceDrop<A, B>,
		OppositeOnDrop: HandleImbalanceDrop<A, B>,
	> Drop for Imbalance<A, B, OnDrop, OppositeOnDrop>
{
	fn drop(&mut self) {
		if !self.amount.is_zero() {
			OnDrop::handle(self.asset.clone(), self.amount)
		}
	}
}

impl<
		A: AssetId,
		B: Balance,
		OnDrop: HandleImbalanceDrop<A, B>,
		OppositeOnDrop: HandleImbalanceDrop<A, B>,
	> TryDrop for Imbalance<A, B, OnDrop, OppositeOnDrop>
{
	/// Drop an instance cleanly. Only works if its value represents "no-operation".
	fn try_drop(self) -> Result<(), Self> {
		self.drop_zero()
	}
}

impl<
		A: AssetId,
		B: Balance,
		OnDrop: HandleImbalanceDrop<A, B>,
		OppositeOnDrop: HandleImbalanceDrop<A, B>,
	> Imbalance<A, B, OnDrop, OppositeOnDrop>
{
	pub fn zero(asset: A) -> Self {
		Self { asset, amount: Zero::zero(), _phantom: PhantomData }
	}

	pub(crate) fn new(asset: A, amount: B) -> Self {
		Self { asset, amount, _phantom: PhantomData }
	}

	/// Forget the imbalance without invoking the on-drop handler.
	pub(crate) fn forget(imbalance: Self) {
		sp_std::mem::forget(imbalance);
	}

	pub fn drop_zero(self) -> Result<(), Self> {
		if self.amount.is_zero() {
			sp_std::mem::forget(self);
			Ok(())
		} else {
			Err(self)
		}
	}

	pub fn split(self, amount: B) -> (Self, Self) {
		let first = self.amount.min(amount);
		let second = self.amount - first;
		let asset = self.asset.clone();
		sp_std::mem::forget(self);
		(Imbalance::new(asset.clone(), first), Imbalance::new(asset, second))
	}

	/// Mutate `self` by extracting a new instance with at most `amount` value, reducing `self`
	/// accordingly.
	pub fn extract(&mut self, amount: B) -> Self {
		let new = self.amount.min(amount);
		self.amount = self.amount - new;
		Imbalance::new(self.asset.clone(), new)
	}

	pub fn merge(mut self, other: Self) -> Result<Self, (Self, Self)> {
		if self.asset == other.asset {
			self.amount = self.amount.saturating_add(other.amount);
			sp_std::mem::forget(other);
			Ok(self)
		} else {
			Err((self, other))
		}
	}
	pub fn subsume(&mut self, other: Self) -> Result<(), Self> {
		if self.asset == other.asset {
			self.amount = self.amount.saturating_add(other.amount);
			sp_std::mem::forget(other);
			Ok(())
		} else {
			Err(other)
		}
	}
	pub fn offset(
		self,
		other: Imbalance<A, B, OppositeOnDrop, OnDrop>,
	) -> Result<
		SameOrOther<Self, Imbalance<A, B, OppositeOnDrop, OnDrop>>,
		(Self, Imbalance<A, B, OppositeOnDrop, OnDrop>),
	> {
		if self.asset == other.asset {
			let (a, b) = (self.amount, other.amount);
			let asset = self.asset.clone();
			sp_std::mem::forget((self, other));

			if a == b {
				Ok(SameOrOther::None)
			} else if a > b {
				Ok(SameOrOther::Same(Imbalance::new(asset, a - b)))
			} else {
				Ok(SameOrOther::Other(Imbalance::<A, B, OppositeOnDrop, OnDrop>::new(asset, b - a)))
			}
		} else {
			Err((self, other))
		}
	}
	pub fn peek(&self) -> B {
		self.amount
	}

	pub fn asset(&self) -> A {
		self.asset.clone()
	}
}

/// Converts a `fungible` `imbalance` instance to an instance of a `fungibles` imbalance type using
/// a specified `asset`.
///
/// This function facilitates imbalance conversions within the implementations of
/// [`frame_support::traits::fungibles::UnionOf`], [`frame_support::traits::fungible::UnionOf`], and
/// [`frame_support::traits::fungible::ItemOf`] adapters. It is intended only for internal use
/// within the current crate.
pub(crate) fn from_fungible<
	A: AssetId,
	B: Balance,
	OnDropIn: fungible::HandleImbalanceDrop<B>,
	OppositeIn: fungible::HandleImbalanceDrop<B>,
	OnDropOut: HandleImbalanceDrop<A, B>,
	OppositeOut: HandleImbalanceDrop<A, B>,
>(
	imbalance: fungible::Imbalance<B, OnDropIn, OppositeIn>,
	asset: A,
) -> Imbalance<A, B, OnDropOut, OppositeOut> {
	let new = Imbalance::new(asset, imbalance.peek());
	fungible::Imbalance::forget(imbalance);
	new
}

/// Converts a `fungibles` `imbalance` instance of one type to another using a specified `asset`.
///
/// This function facilitates imbalance conversions within the implementations of
/// [`frame_support::traits::fungibles::UnionOf`], [`frame_support::traits::fungible::UnionOf`], and
/// [`frame_support::traits::fungible::ItemOf`] adapters. It is intended only for internal use
/// within the current crate.
pub(crate) fn from_fungibles<
	A: AssetId,
	B: Balance,
	OnDropIn: HandleImbalanceDrop<A, B>,
	OppositeIn: HandleImbalanceDrop<A, B>,
	AssetOut: AssetId,
	OnDropOut: HandleImbalanceDrop<AssetOut, B>,
	OppositeOut: HandleImbalanceDrop<AssetOut, B>,
>(
	imbalance: Imbalance<A, B, OnDropIn, OppositeIn>,
	asset: AssetOut,
) -> Imbalance<AssetOut, B, OnDropOut, OppositeOut> {
	let new = Imbalance::new(asset, imbalance.peek());
	Imbalance::forget(imbalance);
	new
}

/// Imbalance implying that the total_issuance value is less than the sum of all account balances.
pub type Debt<AccountId, B> = Imbalance<
	<B as Inspect<AccountId>>::AssetId,
	<B as Inspect<AccountId>>::Balance,
	// This will generally be implemented by increasing the total_issuance value.
	<B as Balanced<AccountId>>::OnDropDebt,
	<B as Balanced<AccountId>>::OnDropCredit,
>;

/// Imbalance implying that the total_issuance value is greater than the sum of all account
/// balances.
pub type Credit<AccountId, B> = Imbalance<
	<B as Inspect<AccountId>>::AssetId,
	<B as Inspect<AccountId>>::Balance,
	// This will generally be implemented by decreasing the total_issuance value.
	<B as Balanced<AccountId>>::OnDropCredit,
	<B as Balanced<AccountId>>::OnDropDebt,
>;
