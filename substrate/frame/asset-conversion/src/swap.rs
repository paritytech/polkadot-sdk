// This file is part of Substrate.

// Copyright (C) 2023 Parity Technologies (UK) Ltd.
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

//! TODO

use super::*;
use frame_support::storage::with_transaction;
use sp_runtime::TransactionOutcome;

/// Trait for providing methods to swap between the various asset classes.
pub trait Swap<AccountId, Balance, MultiAssetId> {
	/// Swap exactly `amount_in` of asset `path[0]` for asset `path[1]`.
	/// If an `amount_out_min` is specified, it will return an error if it is unable to acquire
	/// the amount desired.
	///
	/// Withdraws the `path[0]` asset from `sender`, deposits the `path[1]` asset to `send_to`,
	/// respecting `keep_alive`.
	///
	/// If successful, returns the amount of `path[1]` acquired for the `amount_in`.
	fn swap_exact_tokens_for_tokens(
		sender: AccountId,
		path: Vec<MultiAssetId>,
		amount_in: Balance,
		amount_out_min: Option<Balance>,
		send_to: AccountId,
		keep_alive: bool,
	) -> Result<Balance, DispatchError>;

	/// Take the `path[0]` asset and swap some amount for `amount_out` of the `path[1]`. If an
	/// `amount_in_max` is specified, it will return an error if acquiring `amount_out` would be
	/// too costly.
	///
	/// Withdraws `path[0]` asset from `sender`, deposits `path[1]` asset to `send_to`,
	/// respecting `keep_alive`.
	///
	/// If successful returns the amount of the `path[0]` taken to provide `path[1]`.
	fn swap_tokens_for_exact_tokens(
		sender: AccountId,
		path: Vec<MultiAssetId>,
		amount_out: Balance,
		amount_in_max: Option<Balance>,
		send_to: AccountId,
		keep_alive: bool,
	) -> Result<Balance, DispatchError>;
}

/// TODO
pub trait SwapCredit<AccountId, MultiAssetId, Credit> {
	/// TODO
	type Balance;
	/// TODO
	fn swap_exact_tokens_for_tokens(
		path: Vec<MultiAssetId>,
		credit_in: Credit,
		amount_out_min: Option<Self::Balance>,
	) -> Result<Credit, (Credit, DispatchError)>;

	/// TODO
	fn swap_tokens_for_exact_tokens(
		path: Vec<MultiAssetId>,
		amount_out: Self::Balance,
		credit_in: Credit,
	) -> Result<(Credit, Credit), (Credit, DispatchError)>;
}

impl<T: Config> Swap<T::AccountId, T::AssetBalance, T::MultiAssetId> for Pallet<T> {
	fn swap_exact_tokens_for_tokens(
		sender: T::AccountId,
		path: Vec<T::MultiAssetId>,
		amount_in: T::AssetBalance,
		amount_out_min: Option<T::AssetBalance>,
		send_to: T::AccountId,
		keep_alive: bool,
	) -> Result<T::AssetBalance, DispatchError> {
		let path = path.try_into().map_err(|_| Error::<T>::PathError)?;

		let transaction = with_transaction(|| {
			let swap = Self::do_swap_exact_tokens_for_tokens(
				sender,
				path,
				amount_in,
				amount_out_min,
				send_to,
				keep_alive,
			);

			match &swap {
				Ok(_) => TransactionOutcome::Commit(swap),
				_ => TransactionOutcome::Rollback(swap),
			}
		});

		return match transaction {
			Ok(out) => Ok(out),
			Err(err) => Err(err),
		}
	}

	fn swap_tokens_for_exact_tokens(
		sender: T::AccountId,
		path: Vec<T::MultiAssetId>,
		amount_out: T::AssetBalance,
		amount_in_max: Option<T::AssetBalance>,
		send_to: T::AccountId,
		keep_alive: bool,
	) -> Result<T::AssetBalance, DispatchError> {
		let path = path.try_into().map_err(|_| Error::<T>::PathError)?;

		let transaction = with_transaction(|| {
			let swap = Self::do_swap_tokens_for_exact_tokens(
				sender,
				path,
				amount_out,
				amount_in_max,
				send_to,
				keep_alive,
			);

			match &swap {
				Ok(_) => TransactionOutcome::Commit(swap),
				_ => TransactionOutcome::Rollback(swap),
			}
		});

		return match transaction {
			Ok(out) => Ok(out),
			Err(err) => Err(err),
		}
	}
}

/// TODO
impl<T: Config> SwapCredit<T::AccountId, T::MultiAssetId, Credit<T>> for Pallet<T> {
	type Balance = T::AssetBalance;

	/// TODO
	fn swap_exact_tokens_for_tokens(
		path: Vec<T::MultiAssetId>,
		credit_in: Credit<T>,
		amount_out_min: Option<Self::Balance>,
	) -> Result<Credit<T>, (Credit<T>, DispatchError)> {
		// TODO ready / implementation

		// TODO ready / wrap `with_transaction`
		// TODO ready / swap

		Err((credit_in, Error::<T>::PoolNotFound.into()))
	}

	/// TODO
	fn swap_tokens_for_exact_tokens(
		path: Vec<T::MultiAssetId>,
		amount_out: Self::Balance,
		credit_in: Credit<T>,
	) -> Result<(Credit<T>, Credit<T>), (Credit<T>, DispatchError)> {
		ensure!(amount_out > Zero::zero(), (credit_in, Error::<T>::ZeroAmount.into()));
		ensure!(credit_in.peek() > Zero::zero(), (credit_in, Error::<T>::ZeroAmount.into()));

		// TODO validate swap path (unique pools)

		let (credit_in, path) =
			path.try_into().map_with_prefix(credit_in, |_| Error::<T>::PathError.into())?;

		let (credit_in, _) = Self::validate_swap_path(&path).map_with_prefix(credit_in, |e| e)?;

		let (credit_in, amounts) = Self::get_amounts_in(&amount_out, &path)
			.map_with_prefix(credit_in, |_| Error::<T>::PathError.into())?;

		let (credit_in, amount_in) = amounts
			.first()
			.ok_or(Error::<T>::PathError.into())
			.map_with_prefix(credit_in, |e| e)?;

		ensure!(
			credit_in.peek() >= *amount_in,
			(credit_in, Error::<T>::ProvidedMaximumNotSufficientForSwap.into())
		);
		let (credit_in, credit_change) = credit_in.split(*amount_in);

		let credit_balance_path = Self::balance_path_from_amount_out(amount_out, path)
			.map_with_prefix(credit_in, |e| e)?;

		// Note
		// with_transaction forces Error: From<DispatchError>, not present in (Credit,
		// DispatchError) if we implement From<DispatchError> for the tuple, then there exists an
		// error tuple that can be returned with the credit on default (::zero()), specifically
		// TransactionalError::LimitReached, which is a nested transactional level of 255.
		// Temporary workaround is this mutable binding, there is probably a better workaround
		let mut credit_error: Credit<T> = Credit::<T>::Native(Default::default());

		let transaction = with_transaction(|| {
			match Self::do_swap(credit_balance_path.0, credit_balance_path.1) {
				Ok(swap) => TransactionOutcome::Commit(Ok(swap)),
				Err(err) => {
					credit_error = err.0;
					TransactionOutcome::Rollback(Err(err.1))
				},
			}
		});

		return match transaction {
			Ok(out) => Ok((out, credit_change)),
			Err(err) => Err((credit_error, err)),
		}
	}
}
