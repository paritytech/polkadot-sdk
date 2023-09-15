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

impl<T: Config> Swap<T::AccountId, T::HigherPrecisionBalance, T::MultiAssetId> for Pallet<T> {
	fn swap_exact_tokens_for_tokens(
		sender: T::AccountId,
		path: Vec<T::MultiAssetId>,
		amount_in: T::HigherPrecisionBalance,
		amount_out_min: Option<T::HigherPrecisionBalance>,
		send_to: T::AccountId,
		keep_alive: bool,
	) -> Result<T::HigherPrecisionBalance, DispatchError> {
		let path = path.try_into().map_err(|_| Error::<T>::PathError)?;
		let amount_out_min = amount_out_min.map(Self::convert_hpb_to_asset_balance).transpose()?;
		let amount_out = Self::do_swap_exact_tokens_for_tokens(
			sender,
			path,
			Self::convert_hpb_to_asset_balance(amount_in)?,
			amount_out_min,
			send_to,
			keep_alive,
		)?;
		Ok(amount_out.into())
	}

	fn swap_tokens_for_exact_tokens(
		sender: T::AccountId,
		path: Vec<T::MultiAssetId>,
		amount_out: T::HigherPrecisionBalance,
		amount_in_max: Option<T::HigherPrecisionBalance>,
		send_to: T::AccountId,
		keep_alive: bool,
	) -> Result<T::HigherPrecisionBalance, DispatchError> {
		let path = path.try_into().map_err(|_| Error::<T>::PathError)?;
		let amount_in_max = amount_in_max.map(Self::convert_hpb_to_asset_balance).transpose()?;
		let amount_in = Self::do_swap_tokens_for_exact_tokens(
			sender,
			path,
			Self::convert_hpb_to_asset_balance(amount_out)?,
			amount_in_max,
			send_to,
			keep_alive,
		)?;
		Ok(amount_in.into())
	}
}
