// This file is part of Substrate.

// Copyright (C) 2018-2022 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Runtime API definition for assets.

use crate::runtime_api::FungiblesAccessError;
use sp_std::{borrow::Borrow, vec::Vec};
use xcm::latest::{MultiAsset, MultiLocation};
use xcm_builder::ConvertedConcreteId;
use xcm_executor::traits::{Convert, MatchesFungibles};

/// Converting any [`(AssetId, Balance)`] to [`MultiAsset`]
pub trait MultiAssetConverter<AssetId, Balance, ConvertAssetId, ConvertBalance>:
	MatchesFungibles<AssetId, Balance>
where
	AssetId: Clone,
	Balance: Clone,
	ConvertAssetId: Convert<MultiLocation, AssetId>,
	ConvertBalance: Convert<u128, Balance>,
{
	fn convert_ref(
		value: impl Borrow<(AssetId, Balance)>,
	) -> Result<MultiAsset, FungiblesAccessError>;
}

impl<
		AssetId: Clone,
		Balance: Clone,
		ConvertAssetId: Convert<MultiLocation, AssetId>,
		ConvertBalance: Convert<u128, Balance>,
	> MultiAssetConverter<AssetId, Balance, ConvertAssetId, ConvertBalance>
	for ConvertedConcreteId<AssetId, Balance, ConvertAssetId, ConvertBalance>
{
	fn convert_ref(
		value: impl Borrow<(AssetId, Balance)>,
	) -> Result<MultiAsset, FungiblesAccessError> {
		let (asset_id, balance) = value.borrow();
		match ConvertAssetId::reverse_ref(asset_id) {
			Ok(asset_id_as_multilocation) => match ConvertBalance::reverse_ref(balance) {
				Ok(amount) => Ok((asset_id_as_multilocation, amount).into()),
				Err(_) => Err(FungiblesAccessError::AmountToBalanceConversionFailed),
			},
			Err(_) => Err(FungiblesAccessError::AssetIdConversionFailed),
		}
	}
}

/// Helper function to convert collections with [`(AssetId, Balance)`] to [`MultiAsset`]
pub fn convert<'a, AssetId, Balance, ConvertAssetId, ConvertBalance, Converter>(
	items: impl Iterator<Item = &'a (AssetId, Balance)>,
) -> Result<Vec<MultiAsset>, FungiblesAccessError>
where
	AssetId: Clone + 'a,
	Balance: Clone + 'a,
	ConvertAssetId: Convert<MultiLocation, AssetId>,
	ConvertBalance: Convert<u128, Balance>,
	Converter: MultiAssetConverter<AssetId, Balance, ConvertAssetId, ConvertBalance>,
{
	items.map(Converter::convert_ref).collect()
}

/// Helper function to convert `Balance` with MultiLocation` to `MultiAsset`
pub fn convert_balance<
	T: frame_support::pallet_prelude::Get<MultiLocation>,
	Balance: TryInto<u128>,
>(
	balance: Balance,
) -> Result<MultiAsset, FungiblesAccessError> {
	match balance.try_into() {
		Ok(balance) => Ok((T::get(), balance).into()),
		Err(_) => Err(FungiblesAccessError::AmountToBalanceConversionFailed),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use xcm::latest::prelude::*;
	use xcm_executor::traits::{Identity, JustTry};

	type Converter = ConvertedConcreteId<MultiLocation, u64, Identity, JustTry>;

	#[test]
	fn converted_concrete_id_fungible_multi_asset_conversion_roundtrip_works() {
		let location = MultiLocation::new(0, X1(GlobalConsensus(ByGenesis([0; 32]))));
		let amount = 123456_u64;
		let expected_multi_asset = MultiAsset {
			id: Concrete(MultiLocation::new(0, X1(GlobalConsensus(ByGenesis([0; 32]))))),
			fun: Fungible(123456_u128),
		};

		assert_eq!(
			Converter::matches_fungibles(&expected_multi_asset).map_err(|_| ()),
			Ok((location, amount))
		);

		assert_eq!(Converter::convert_ref((location, amount)), Ok(expected_multi_asset));
	}

	#[test]
	fn converted_concrete_id_fungible_multi_asset_conversion_collection_works() {
		let data = vec![
			(MultiLocation::new(0, X1(GlobalConsensus(ByGenesis([0; 32])))), 123456_u64),
			(MultiLocation::new(1, X1(GlobalConsensus(ByGenesis([1; 32])))), 654321_u64),
		];

		let expected_data = vec![
			MultiAsset {
				id: Concrete(MultiLocation::new(0, X1(GlobalConsensus(ByGenesis([0; 32]))))),
				fun: Fungible(123456_u128),
			},
			MultiAsset {
				id: Concrete(MultiLocation::new(1, X1(GlobalConsensus(ByGenesis([1; 32]))))),
				fun: Fungible(654321_u128),
			},
		];

		assert_eq!(convert::<_, _, _, _, Converter>(data.iter()), Ok(expected_data));
	}
}
