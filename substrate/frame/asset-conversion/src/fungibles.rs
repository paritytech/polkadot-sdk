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

use super::*;

/// TODO
pub type NativeCredit<T> =
	CreditFungible<<T as frame_system::Config>::AccountId, <T as Config>::Currency>;

/// TODO
pub type AssetCredit<T> =
	CreditFungibles<<T as frame_system::Config>::AccountId, <T as Config>::Assets>;

/// TODO
pub enum Credit<T: Config> {
	/// TODO
	Native(NativeCredit<T>),
	/// TODO
	Asset(AssetCredit<T>),
}

impl<T: Config> From<NativeCredit<T>> for Credit<T> {
	fn from(value: NativeCredit<T>) -> Self {
		Credit::Native(value)
	}
}

impl<T: Config> From<AssetCredit<T>> for Credit<T> {
	fn from(value: AssetCredit<T>) -> Self {
		Credit::Asset(value)
	}
}

impl<T: Config> Credit<T> {
	/// TODO
	pub fn peek(&self) -> T::AssetBalance {
		match self {
			Credit::Native(c) => {
				let c: T::HigherPrecisionBalance = c.peek().into();
				c.try_into().ok().unwrap()
			},
			Credit::Asset(c) => c.peek(),
		}
	}

	/// TODO
	pub fn split(self, amount: T::AssetBalance) -> (Self, Self) {
		match self {
			Credit::Native(c) => {
				let (left, right) = c.split({
					let a: T::HigherPrecisionBalance = amount.into();
					a.try_into().ok().unwrap()
				});
				(left.into(), right.into())
			},
			Credit::Asset(c) => {
				let (left, right) = c.split(amount);
				(left.into(), right.into())
			},
		}
	}

	/// TODO
	pub fn asset(&self) -> T::MultiAssetId {
		match self {
			Credit::Native(_) => T::MultiAssetIdConverter::get_native(),
			Credit::Asset(c) => c.asset().into(),
		}
	}
}

impl<T: Config> Pallet<T> {
	/// TODO
	pub fn resolve(who: &T::AccountId, credit: Credit<T>) -> Result<(), Credit<T>> {
		match credit {
			Credit::Native(c) => T::Currency::resolve(who, c).map_err(|c| c.into()),
			Credit::Asset(c) => T::Assets::resolve(who, c).map_err(|c| c.into()),
		}
	}

	/// TODO
	pub fn withdraw(
		asset_id: &T::MultiAssetId,
		who: &T::AccountId,
		amount: T::AssetBalance,
		keep_alive: bool,
	) -> Result<Credit<T>, DispatchError> {
		let preservation = match keep_alive {
			true => Preserve,
			false => Expendable,
		};

		match T::MultiAssetIdConverter::try_convert(asset_id) {
			MultiAssetIdConversionResult::Converted(asset_id) =>
				T::Assets::withdraw(asset_id, who, amount, Exact, preservation, Polite)
					.map(|c| c.into()),
			MultiAssetIdConversionResult::Native => {
				let amount = Self::convert_asset_balance_to_native_balance(amount)?;
				T::Currency::withdraw(who, amount, Exact, preservation, Polite).map(|c| c.into())
			},
			MultiAssetIdConversionResult::Unsupported(_) =>
				Err(Error::<T>::UnsupportedAsset.into()),
		}
	}
}
