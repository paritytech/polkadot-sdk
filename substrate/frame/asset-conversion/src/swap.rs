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

/// TODO
impl<T: Config> SwapCredit<T::AccountId, T::MultiAssetId, Credit<T>> for Pallet<T> {
	type Balance = T::AssetBalance;

	/// TODO
	fn swap_exact_tokens_for_tokens(
		path: Vec<T::MultiAssetId>,
		credit_in: Credit<T>,
		amount_out_min: Option<Self::Balance>,
	) -> Result<Credit<T>, (Credit<T>, DispatchError)> {
		// TODO implementation
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

		// TODO wrap `with_transaction`
		// TODO swap

		Err((credit_in, Error::<T>::PoolNotFound.into()))
	}
}
