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

//! PGAS-backed storage deposit support.
//!
//! Revive can back storage deposits either with the native currency (DOT-convertible) or with
//! PGAS. The [`PgasBackend`] trait abstracts PGAS access so the pallet does not depend on any
//! particular fungibles implementation. Runtimes without PGAS leave the default `()` binding,
//! which reports a zero balance and rejects any withdrawal: the DOT path is then used for every
//! deposit, matching pre-PGAS behaviour.

use crate::{BalanceOf, Config, Error};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use core::marker::PhantomData;
use frame_support::traits::{
	Get,
	tokens::{
		Fortitude, Precision, Preservation,
		fungibles::{self, Credit},
	},
};
use scale_info::TypeInfo;
use sp_runtime::{DispatchError, traits::Zero};

/// Which asset paid for a deposit.
///
/// Used as a tag on deposits whose depositor is recorded explicitly (code upload, address
/// mapping) so the refund can be returned in the same asset. For contract storage deposits,
/// where many users contribute into one contract, the DOT-convertible portion is tracked per
/// `(contract, user)` in a dedicated map instead.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	Clone,
	Copy,
	PartialEq,
	Eq,
	Debug,
)]
pub enum DepositAsset {
	/// Native, DOT-convertible asset (the pre-PGAS default).
	DotConvertible,
	/// PGAS — cannot be refunded as DOT-convertible value.
	Pgas,
}

impl Default for DepositAsset {
	fn default() -> Self {
		Self::DotConvertible
	}
}

/// Abstraction over the PGAS asset used to back storage deposits.
///
/// The trait deals in credits so PGAS never has to be routed through an intermediate account:
/// [`withdraw`](Self::withdraw) takes balance from a user as a credit, and
/// [`resolve`](Self::resolve) deposits that credit into another account (the contract or the
/// pallet account).
pub trait PgasBackend<T: Config> {
	/// Credit type produced by [`Self::withdraw`] and consumed by [`Self::resolve`].
	type Credit;

	/// Amount of PGAS `who` can currently spend without being reaped.
	fn reducible(who: &T::AccountId) -> BalanceOf<T>;

	/// Amount of PGAS `who` can give up in full, even if the account is reaped.
	///
	/// Used on contract termination to drain the contract's PGAS balance before the account is
	/// removed.
	fn drainable(who: &T::AccountId) -> BalanceOf<T>;

	/// Withdraw `amount` PGAS from `who` as a credit.
	fn withdraw(who: &T::AccountId, amount: BalanceOf<T>) -> Result<Self::Credit, DispatchError>;

	/// Withdraw `amount` PGAS from `who` allowing the account to be reaped.
	///
	/// Used on contract termination alongside [`Self::drainable`].
	fn withdraw_expendable(
		who: &T::AccountId,
		amount: BalanceOf<T>,
	) -> Result<Self::Credit, DispatchError>;

	/// Resolve `credit` into `who`'s account.
	///
	/// On failure returns the unresolved credit so the caller can merge or drop it.
	fn resolve(who: &T::AccountId, credit: Self::Credit) -> Result<(), Self::Credit>;
}

/// No-op PGAS backend used when a runtime does not enable PGAS-backed deposits.
///
/// [`Self::reducible`] always reports zero, so the charge path never selects PGAS.
impl<T: Config> PgasBackend<T> for () {
	type Credit = ();

	fn reducible(_: &T::AccountId) -> BalanceOf<T> {
		Zero::zero()
	}

	fn drainable(_: &T::AccountId) -> BalanceOf<T> {
		Zero::zero()
	}

	fn withdraw(_: &T::AccountId, _: BalanceOf<T>) -> Result<Self::Credit, DispatchError> {
		Err(Error::<T>::StorageDepositNotEnoughFunds.into())
	}

	fn withdraw_expendable(
		_: &T::AccountId,
		_: BalanceOf<T>,
	) -> Result<Self::Credit, DispatchError> {
		Err(Error::<T>::StorageDepositNotEnoughFunds.into())
	}

	fn resolve(_: &T::AccountId, _: Self::Credit) -> Result<(), Self::Credit> {
		Ok(())
	}
}

/// Adapter exposing a [`fungibles::Balanced`] asset identified by `Id` as a [`PgasBackend`].
///
/// Mirrors the wiring used by `pallet-pgas-allowance`: runtimes that already bind a PGAS asset
/// for transaction-fee payment can reuse the same types here.
pub struct PgasAssets<Assets, Id>(PhantomData<(Assets, Id)>);

impl<T, Assets, Id> PgasBackend<T> for PgasAssets<Assets, Id>
where
	T: Config,
	Assets: fungibles::Balanced<T::AccountId, Balance = BalanceOf<T>>,
	Id: Get<<Assets as fungibles::Inspect<T::AccountId>>::AssetId>,
{
	type Credit = Credit<T::AccountId, Assets>;

	fn reducible(who: &T::AccountId) -> BalanceOf<T> {
		<Assets as fungibles::Inspect<T::AccountId>>::reducible_balance(
			Id::get(),
			who,
			Preservation::Preserve,
			Fortitude::Polite,
		)
	}

	fn drainable(who: &T::AccountId) -> BalanceOf<T> {
		<Assets as fungibles::Inspect<T::AccountId>>::reducible_balance(
			Id::get(),
			who,
			Preservation::Expendable,
			Fortitude::Polite,
		)
	}

	fn withdraw(who: &T::AccountId, amount: BalanceOf<T>) -> Result<Self::Credit, DispatchError> {
		<Assets as fungibles::Balanced<T::AccountId>>::withdraw(
			Id::get(),
			who,
			amount,
			Precision::Exact,
			Preservation::Preserve,
			Fortitude::Polite,
		)
	}

	fn withdraw_expendable(
		who: &T::AccountId,
		amount: BalanceOf<T>,
	) -> Result<Self::Credit, DispatchError> {
		<Assets as fungibles::Balanced<T::AccountId>>::withdraw(
			Id::get(),
			who,
			amount,
			Precision::Exact,
			Preservation::Expendable,
			Fortitude::Polite,
		)
	}

	fn resolve(who: &T::AccountId, credit: Self::Credit) -> Result<(), Self::Credit> {
		<Assets as fungibles::Balanced<T::AccountId>>::resolve(who, credit)
	}
}
