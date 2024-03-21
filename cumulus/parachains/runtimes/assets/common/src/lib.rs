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

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarks;
pub mod foreign_creators;
pub mod fungible_conversion;
pub mod local_and_foreign_assets;
pub mod matching;
pub mod runtime_api;

use crate::matching::{LocalLocationPattern, ParentLocation};
use frame_support::traits::{Equals, EverythingBut};
use parachains_common::{AssetIdForTrustBackedAssets, CollectionId, ItemId};
use xcm_builder::{
	AsPrefixedGeneralIndex, MatchedConvertedConcreteId, StartsWith, V4V3LocationConverter,
};
use xcm_executor::traits::JustTry;

/// `Location` vs `AssetIdForTrustBackedAssets` converter for `TrustBackedAssets`
pub type AssetIdForTrustBackedAssetsConvert<TrustBackedAssetsPalletLocation> =
	AsPrefixedGeneralIndex<
		TrustBackedAssetsPalletLocation,
		AssetIdForTrustBackedAssets,
		JustTry,
		xcm::v3::Location,
	>;

pub type AssetIdForTrustBackedAssetsConvertLatest<TrustBackedAssetsPalletLocation> =
	AsPrefixedGeneralIndex<TrustBackedAssetsPalletLocation, AssetIdForTrustBackedAssets, JustTry>;

/// `Location` vs `CollectionId` converter for `Uniques`
pub type CollectionIdForUniquesConvert<UniquesPalletLocation> =
	AsPrefixedGeneralIndex<UniquesPalletLocation, CollectionId, JustTry>;

/// [`MatchedConvertedConcreteId`] converter dedicated for `TrustBackedAssets`
pub type TrustBackedAssetsConvertedConcreteId<TrustBackedAssetsPalletLocation, Balance> =
	MatchedConvertedConcreteId<
		AssetIdForTrustBackedAssets,
		Balance,
		StartsWith<TrustBackedAssetsPalletLocation>,
		AssetIdForTrustBackedAssetsConvertLatest<TrustBackedAssetsPalletLocation>,
		JustTry,
	>;

/// [`MatchedConvertedConcreteId`] converter dedicated for `Uniques`
pub type UniquesConvertedConcreteId<UniquesPalletLocation> = MatchedConvertedConcreteId<
	CollectionId,
	ItemId,
	// The asset starts with the uniques pallet. The `CollectionId` of the asset is specified as a
	// junction within the pallet itself.
	StartsWith<UniquesPalletLocation>,
	CollectionIdForUniquesConvert<UniquesPalletLocation>,
	JustTry,
>;

/// [`MatchedConvertedConcreteId`] converter dedicated for storing `AssetId` as `Location`.
pub type LocationConvertedConcreteId<LocationFilter, Balance> = MatchedConvertedConcreteId<
	xcm::v3::Location,
	Balance,
	LocationFilter,
	V4V3LocationConverter,
	JustTry,
>;

/// [`MatchedConvertedConcreteId`] converter dedicated for `TrustBackedAssets`
pub type TrustBackedAssetsAsLocation<TrustBackedAssetsPalletLocation, Balance> =
	MatchedConvertedConcreteId<
		xcm::v3::Location,
		Balance,
		StartsWith<TrustBackedAssetsPalletLocation>,
		V4V3LocationConverter,
		JustTry,
	>;

/// [`MatchedConvertedConcreteId`] converter dedicated for storing `ForeignAssets` with `AssetId` as
/// `Location`.
///
/// Excludes by default:
/// - parent as relay chain
/// - all local Locations
///
/// `AdditionalLocationExclusionFilter` can customize additional excluded Locations
pub type ForeignAssetsConvertedConcreteId<AdditionalLocationExclusionFilter, Balance> =
	LocationConvertedConcreteId<
		EverythingBut<(
			// Excludes relay/parent chain currency
			Equals<ParentLocation>,
			// Here we rely on fact that something like this works:
			// assert!(Location::new(1,
			// [Parachain(100)]).starts_with(&Location::parent()));
			// assert!([Parachain(100)].into().starts_with(&Here));
			StartsWith<LocalLocationPattern>,
			// Here we can exclude more stuff or leave it as `()`
			AdditionalLocationExclusionFilter,
		)>,
		Balance,
	>;

type AssetIdForPoolAssets = u32;
/// `Location` vs `AssetIdForPoolAssets` converter for `PoolAssets`.
pub type AssetIdForPoolAssetsConvert<PoolAssetsPalletLocation> =
	AsPrefixedGeneralIndex<PoolAssetsPalletLocation, AssetIdForPoolAssets, JustTry>;
/// [`MatchedConvertedConcreteId`] converter dedicated for `PoolAssets`
pub type PoolAssetsConvertedConcreteId<PoolAssetsPalletLocation, Balance> =
	MatchedConvertedConcreteId<
		AssetIdForPoolAssets,
		Balance,
		StartsWith<PoolAssetsPalletLocation>,
		AssetIdForPoolAssetsConvert<PoolAssetsPalletLocation>,
		JustTry,
	>;

#[cfg(test)]
mod tests {
	use super::*;
	use sp_runtime::traits::MaybeEquivalence;
	use xcm::prelude::*;
	use xcm_builder::StartsWithExplicitGlobalConsensus;
	use xcm_executor::traits::{Error as MatchError, MatchesFungibles};

	#[test]
	fn asset_id_for_trust_backed_assets_convert_works() {
		frame_support::parameter_types! {
			pub TrustBackedAssetsPalletLocation: Location = Location::new(5, [PalletInstance(13)]);
		}
		let local_asset_id = 123456789 as AssetIdForTrustBackedAssets;
		let expected_reverse_ref =
			Location::new(5, [PalletInstance(13), GeneralIndex(local_asset_id.into())]);

		assert_eq!(
			AssetIdForTrustBackedAssetsConvertLatest::<TrustBackedAssetsPalletLocation>::convert_back(
				&local_asset_id
			)
			.unwrap(),
			expected_reverse_ref
		);
		assert_eq!(
			AssetIdForTrustBackedAssetsConvertLatest::<TrustBackedAssetsPalletLocation>::convert(
				&expected_reverse_ref
			)
			.unwrap(),
			local_asset_id
		);
	}

	#[test]
	fn trust_backed_assets_match_fungibles_works() {
		frame_support::parameter_types! {
			pub TrustBackedAssetsPalletLocation: Location = Location::new(0, [PalletInstance(13)]);
		}
		// setup convert
		type TrustBackedAssetsConvert =
			TrustBackedAssetsConvertedConcreteId<TrustBackedAssetsPalletLocation, u128>;

		let test_data = vec![
			// missing GeneralIndex
			(ma_1000(0, [PalletInstance(13)].into()), Err(MatchError::AssetIdConversionFailed)),
			(
				ma_1000(0, [PalletInstance(13), GeneralKey { data: [0; 32], length: 32 }].into()),
				Err(MatchError::AssetIdConversionFailed),
			),
			(
				ma_1000(0, [PalletInstance(13), Parachain(1000)].into()),
				Err(MatchError::AssetIdConversionFailed),
			),
			// OK
			(ma_1000(0, [PalletInstance(13), GeneralIndex(1234)].into()), Ok((1234, 1000))),
			(
				ma_1000(0, [PalletInstance(13), GeneralIndex(1234), GeneralIndex(2222)].into()),
				Ok((1234, 1000)),
			),
			(
				ma_1000(
					0,
					[
						PalletInstance(13),
						GeneralIndex(1234),
						GeneralIndex(2222),
						GeneralKey { data: [0; 32], length: 32 },
					]
					.into(),
				),
				Ok((1234, 1000)),
			),
			// wrong pallet instance
			(
				ma_1000(0, [PalletInstance(77), GeneralIndex(1234)].into()),
				Err(MatchError::AssetNotHandled),
			),
			(
				ma_1000(0, [PalletInstance(77), GeneralIndex(1234), GeneralIndex(2222)].into()),
				Err(MatchError::AssetNotHandled),
			),
			// wrong parent
			(
				ma_1000(1, [PalletInstance(13), GeneralIndex(1234)].into()),
				Err(MatchError::AssetNotHandled),
			),
			(
				ma_1000(1, [PalletInstance(13), GeneralIndex(1234), GeneralIndex(2222)].into()),
				Err(MatchError::AssetNotHandled),
			),
			(
				ma_1000(1, [PalletInstance(77), GeneralIndex(1234)].into()),
				Err(MatchError::AssetNotHandled),
			),
			(
				ma_1000(1, [PalletInstance(77), GeneralIndex(1234), GeneralIndex(2222)].into()),
				Err(MatchError::AssetNotHandled),
			),
			// wrong parent
			(
				ma_1000(2, [PalletInstance(13), GeneralIndex(1234)].into()),
				Err(MatchError::AssetNotHandled),
			),
			(
				ma_1000(2, [PalletInstance(13), GeneralIndex(1234), GeneralIndex(2222)].into()),
				Err(MatchError::AssetNotHandled),
			),
			// missing GeneralIndex
			(ma_1000(0, [PalletInstance(77)].into()), Err(MatchError::AssetNotHandled)),
			(ma_1000(1, [PalletInstance(13)].into()), Err(MatchError::AssetNotHandled)),
			(ma_1000(2, [PalletInstance(13)].into()), Err(MatchError::AssetNotHandled)),
		];

		for (asset, expected_result) in test_data {
			assert_eq!(
				<TrustBackedAssetsConvert as MatchesFungibles<AssetIdForTrustBackedAssets, u128>>::matches_fungibles(&asset.clone().try_into().unwrap()),
				expected_result, "asset: {:?}", asset);
		}
	}

	#[test]
	fn location_converted_concrete_id_converter_works() {
		frame_support::parameter_types! {
			pub Parachain100Pattern: Location = Location::new(1, [Parachain(100)]);
			pub UniversalLocationNetworkId: NetworkId = NetworkId::ByGenesis([9; 32]);
		}

		// setup convert
		type Convert = ForeignAssetsConvertedConcreteId<
			(
				StartsWith<Parachain100Pattern>,
				StartsWithExplicitGlobalConsensus<UniversalLocationNetworkId>,
			),
			u128,
		>;

		let test_data = vec![
			// excluded as local
			(ma_1000(0, Here), Err(MatchError::AssetNotHandled)),
			(ma_1000(0, [Parachain(100)].into()), Err(MatchError::AssetNotHandled)),
			(
				ma_1000(0, [PalletInstance(13), GeneralIndex(1234)].into()),
				Err(MatchError::AssetNotHandled),
			),
			// excluded as parent
			(ma_1000(1, Here), Err(MatchError::AssetNotHandled)),
			// excluded as additional filter - Parachain100Pattern
			(ma_1000(1, [Parachain(100)].into()), Err(MatchError::AssetNotHandled)),
			(
				ma_1000(1, [Parachain(100), GeneralIndex(1234)].into()),
				Err(MatchError::AssetNotHandled),
			),
			(
				ma_1000(1, [Parachain(100), PalletInstance(13), GeneralIndex(1234)].into()),
				Err(MatchError::AssetNotHandled),
			),
			// excluded as additional filter - StartsWithExplicitGlobalConsensus
			(
				ma_1000(1, [GlobalConsensus(NetworkId::ByGenesis([9; 32]))].into()),
				Err(MatchError::AssetNotHandled),
			),
			(
				ma_1000(2, [GlobalConsensus(NetworkId::ByGenesis([9; 32]))].into()),
				Err(MatchError::AssetNotHandled),
			),
			(
				ma_1000(
					2,
					[
						GlobalConsensus(NetworkId::ByGenesis([9; 32])),
						Parachain(200),
						GeneralIndex(1234),
					]
					.into(),
				),
				Err(MatchError::AssetNotHandled),
			),
			// ok
			(
				ma_1000(1, [Parachain(200)].into()),
				Ok((xcm::v3::Location::new(1, [xcm::v3::Junction::Parachain(200)]), 1000)),
			),
			(
				ma_1000(2, [Parachain(200)].into()),
				Ok((xcm::v3::Location::new(2, [xcm::v3::Junction::Parachain(200)]), 1000)),
			),
			(
				ma_1000(1, [Parachain(200), GeneralIndex(1234)].into()),
				Ok((
					xcm::v3::Location::new(
						1,
						[xcm::v3::Junction::Parachain(200), xcm::v3::Junction::GeneralIndex(1234)],
					),
					1000,
				)),
			),
			(
				ma_1000(2, [Parachain(200), GeneralIndex(1234)].into()),
				Ok((
					xcm::v3::Location::new(
						2,
						[xcm::v3::Junction::Parachain(200), xcm::v3::Junction::GeneralIndex(1234)],
					),
					1000,
				)),
			),
			(
				ma_1000(2, [GlobalConsensus(NetworkId::ByGenesis([7; 32]))].into()),
				Ok((
					xcm::v3::Location::new(
						2,
						[xcm::v3::Junction::GlobalConsensus(xcm::v3::NetworkId::ByGenesis(
							[7; 32],
						))],
					),
					1000,
				)),
			),
			(
				ma_1000(
					2,
					[
						GlobalConsensus(NetworkId::ByGenesis([7; 32])),
						Parachain(200),
						GeneralIndex(1234),
					]
					.into(),
				),
				Ok((
					xcm::v3::Location::new(
						2,
						[
							xcm::v3::Junction::GlobalConsensus(xcm::v3::NetworkId::ByGenesis(
								[7; 32],
							)),
							xcm::v3::Junction::Parachain(200),
							xcm::v3::Junction::GeneralIndex(1234),
						],
					),
					1000,
				)),
			),
		];

		for (asset, expected_result) in test_data {
			assert_eq!(
				<Convert as MatchesFungibles<xcm::v3::Location, u128>>::matches_fungibles(
					&asset.clone().try_into().unwrap()
				),
				expected_result,
				"asset: {:?}",
				asset
			);
		}
	}

	// Create Asset
	fn ma_1000(parents: u8, interior: Junctions) -> Asset {
		(Location::new(parents, interior), 1000).into()
	}
}
