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

//! Traits and implementations for swap between the various asset classes.

use super::*;

/// Trait for providing methods to swap between the various asset classes.
pub trait Swap<AccountId> {
	/// Measure units of the asset classes for swapping.
	type Balance: Balance;
	/// Kind of assets that are going to be swapped.
	type AssetKind;

	/// Returns the upper limit on the length of the swap path.
	fn max_path_len() -> u32;

	/// Swap exactly `amount_in` of asset `path[0]` for asset `path[last]`.
	/// If an `amount_out_min` is specified, it will return an error if it is unable to acquire
	/// the amount desired.
	///
	/// Withdraws the `path[0]` asset from `sender`, deposits the `path[last]` asset to `send_to`,
	/// respecting `keep_alive`.
	///
	/// If successful, returns the amount of `path[last]` acquired for the `amount_in`.
	///
	/// This operation is expected to be atomic.
	fn swap_exact_tokens_for_tokens(
		sender: AccountId,
		path: Vec<Self::AssetKind>,
		amount_in: Self::Balance,
		amount_out_min: Option<Self::Balance>,
		send_to: AccountId,
		keep_alive: bool,
	) -> Result<Self::Balance, DispatchError>;

	/// Take the `path[0]` asset and swap some amount for `amount_out` of the `path[last]`. If an
	/// `amount_in_max` is specified, it will return an error if acquiring `amount_out` would be
	/// too costly.
	///
	/// Withdraws `path[0]` asset from `sender`, deposits `path[last]` asset to `send_to`,
	/// respecting `keep_alive`.
	///
	/// If successful returns the amount of the `path[0]` taken to provide `path[last]`.
	///
	/// This operation is expected to be atomic.
	fn swap_tokens_for_exact_tokens(
		sender: AccountId,
		path: Vec<Self::AssetKind>,
		amount_out: Self::Balance,
		amount_in_max: Option<Self::Balance>,
		send_to: AccountId,
		keep_alive: bool,
	) -> Result<Self::Balance, DispatchError>;
}

/// Trait providing methods to swap between the various asset classes.
pub trait SwapCredit<AccountId> {
	/// Measure units of the asset classes for swapping.
	type Balance: Balance;
	/// Kind of assets that are going to be swapped.
	type AssetKind;
	/// Credit implying a negative imbalance in the system that can be placed into an account or
	/// alter the total supply.
	type Credit;

	/// Returns the upper limit on the length of the swap path.
	fn max_path_len() -> u32;

	/// Swap exactly `credit_in` of asset `path[0]` for asset `path[last]`.  If `amount_out_min` is
	/// provided and the swap can't achieve at least this amount, an error is returned.
	///
	/// On a successful swap, the function returns the `credit_out` of `path[last]` obtained from
	/// the `credit_in`. On failure, it returns an `Err` containing the original `credit_in` and the
	/// associated error code.
	///
	/// This operation is expected to be atomic.
	fn swap_exact_tokens_for_tokens(
		path: Vec<Self::AssetKind>,
		credit_in: Self::Credit,
		amount_out_min: Option<Self::Balance>,
	) -> Result<Self::Credit, (Self::Credit, DispatchError)>;

	/// Swaps a portion of `credit_in` of `path[0]` asset to obtain the desired `amount_out` of
	/// the `path[last]` asset. The provided `credit_in` must be adequate to achieve the target
	/// `amount_out`, or an error will occur.
	///
	/// On success, the function returns a (`credit_out`, `credit_change`) tuple, where `credit_out`
	/// represents the acquired amount of the `path[last]` asset, and `credit_change` is the
	/// remaining portion from the `credit_in`. On failure, an `Err` with the initial `credit_in`
	/// and error code is returned.
	///
	/// This operation is expected to be atomic.
	fn swap_tokens_for_exact_tokens(
		path: Vec<Self::AssetKind>,
		credit_in: Self::Credit,
		amount_out: Self::Balance,
	) -> Result<(Self::Credit, Self::Credit), (Self::Credit, DispatchError)>;
}

impl<T: Config> Swap<T::AccountId> for Pallet<T> {
	type Balance = T::Balance;
	type AssetKind = T::AssetKind;

	fn max_path_len() -> u32 {
		T::MaxSwapPathLength::get()
	}

	fn swap_exact_tokens_for_tokens(
		sender: T::AccountId,
		path: Vec<Self::AssetKind>,
		amount_in: Self::Balance,
		amount_out_min: Option<Self::Balance>,
		send_to: T::AccountId,
		keep_alive: bool,
	) -> Result<Self::Balance, DispatchError> {
		let amount_out = with_storage_layer(|| {
			Self::do_swap_exact_tokens_for_tokens(
				sender,
				path,
				amount_in,
				amount_out_min,
				send_to,
				keep_alive,
			)
		})?;
		Ok(amount_out)
	}

	fn swap_tokens_for_exact_tokens(
		sender: T::AccountId,
		path: Vec<Self::AssetKind>,
		amount_out: Self::Balance,
		amount_in_max: Option<Self::Balance>,
		send_to: T::AccountId,
		keep_alive: bool,
	) -> Result<Self::Balance, DispatchError> {
		let amount_in = with_storage_layer(|| {
			Self::do_swap_tokens_for_exact_tokens(
				sender,
				path,
				amount_out,
				amount_in_max,
				send_to,
				keep_alive,
			)
		})?;
		Ok(amount_in)
	}
}

impl<T: Config> SwapCredit<T::AccountId> for Pallet<T> {
	type Balance = T::Balance;
	type AssetKind = T::AssetKind;
	type Credit = CreditOf<T>;

	fn max_path_len() -> u32 {
		T::MaxSwapPathLength::get()
	}

	fn swap_exact_tokens_for_tokens(
		path: Vec<Self::AssetKind>,
		credit_in: Self::Credit,
		amount_out_min: Option<Self::Balance>,
	) -> Result<Self::Credit, (Self::Credit, DispatchError)> {
		let credit_asset = credit_in.asset();
		with_transaction(|| -> TransactionOutcome<Result<_, DispatchError>> {
			let res = Self::do_swap_exact_credit_tokens_for_tokens(path, credit_in, amount_out_min);
			match &res {
				Ok(_) => TransactionOutcome::Commit(Ok(res)),
				// wrapping `res` with `Ok`, since our `Err` doesn't satisfy the
				// `From<DispatchError>` bound of the `with_transaction` function.
				Err(_) => TransactionOutcome::Rollback(Ok(res)),
			}
		})
		// should never map an error since `with_transaction` above never returns it.
		.map_err(|_| (Self::Credit::zero(credit_asset), DispatchError::Corruption))?
	}

	fn swap_tokens_for_exact_tokens(
		path: Vec<Self::AssetKind>,
		credit_in: Self::Credit,
		amount_out: Self::Balance,
	) -> Result<(Self::Credit, Self::Credit), (Self::Credit, DispatchError)> {
		let credit_asset = credit_in.asset();
		with_transaction(|| -> TransactionOutcome<Result<_, DispatchError>> {
			let res = Self::do_swap_credit_tokens_for_exact_tokens(path, credit_in, amount_out);
			match &res {
				Ok(_) => TransactionOutcome::Commit(Ok(res)),
				// wrapping `res` with `Ok`, since our `Err` doesn't satisfy the
				// `From<DispatchError>` bound of the `with_transaction` function.
				Err(_) => TransactionOutcome::Rollback(Ok(res)),
			}
		})
		// should never map an error since `with_transaction` above never returns it.
		.map_err(|_| (Self::Credit::zero(credit_asset), DispatchError::Corruption))?
	}
}
