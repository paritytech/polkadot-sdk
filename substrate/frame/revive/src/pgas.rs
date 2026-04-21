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
//! which reports a zero balance and rejects any transfer: the DOT path is then used for every
//! deposit, matching pre-PGAS behaviour.

use crate::{BalanceOf, Config, Error};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use core::marker::PhantomData;
use frame_support::traits::{
	Get,
	tokens::{Fortitude, Precision, Preservation, fungibles},
};
use scale_info::TypeInfo;
use sp_runtime::{DispatchError, DispatchResult, traits::Zero};

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
	/// PGAS, cannot be refunded as DOT-convertible value.
	Pgas,
}

impl Default for DepositAsset {
	fn default() -> Self {
		Self::DotConvertible
	}
}

/// Abstraction over the PGAS asset used to back storage deposits.
///
/// Runtimes without PGAS leave the default `()` binding, which reports a zero balance and
/// rejects any transfer.
pub trait PgasBackend<T: Config> {
	/// The PGAS balance of `who` that can be transferred out while keeping the account alive.
	fn balance(who: &T::AccountId) -> BalanceOf<T>;

	/// Transfer `amount` of PGAS from `from` to `to`.
	///
	/// `preservation` controls whether `from` may be reaped by the transfer. Termination uses
	/// [`Preservation::Expendable`] to drain the contract's PGAS balance before the account is
	/// removed; regular charges/refunds use [`Preservation::Preserve`].
	fn transfer(
		from: &T::AccountId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
		preservation: Preservation,
	) -> DispatchResult;
}

/// No-op PGAS backend used when a runtime does not enable PGAS-backed deposits.
///
/// [`Self::balance`] always reports zero, so the charge path never selects PGAS.
impl<T: Config> PgasBackend<T> for () {
	fn balance(_: &T::AccountId) -> BalanceOf<T> {
		Zero::zero()
	}

	fn transfer(
		_: &T::AccountId,
		_: &T::AccountId,
		_: BalanceOf<T>,
		_: Preservation,
	) -> DispatchResult {
		Err(Error::<T>::StorageDepositNotEnoughFunds.into())
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
	fn balance(who: &T::AccountId) -> BalanceOf<T> {
		<Assets as fungibles::Inspect<T::AccountId>>::reducible_balance(
			Id::get(),
			who,
			Preservation::Preserve,
			Fortitude::Polite,
		)
	}

	fn transfer(
		from: &T::AccountId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
		preservation: Preservation,
	) -> DispatchResult {
		let credit = <Assets as fungibles::Balanced<T::AccountId>>::withdraw(
			Id::get(),
			from,
			amount,
			Precision::Exact,
			preservation,
			Fortitude::Polite,
		)?;
		<Assets as fungibles::Balanced<T::AccountId>>::resolve(to, credit)
			.map_err(|_| DispatchError::Other("pallet-revive: failed to resolve PGAS credit"))?;
		Ok(())
	}
}
