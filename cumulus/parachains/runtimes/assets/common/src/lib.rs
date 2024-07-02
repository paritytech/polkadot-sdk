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
use core::marker::PhantomData;
use frame_support::traits::{
	fungibles, Equals, EverythingBut, Get,
};
use pallet_asset_conversion::SwapCredit as SwapCreditT;
use parachains_common::{AssetIdForTrustBackedAssets, CollectionId, ItemId};
use sp_runtime::traits::{TryConvertInto, Zero};
use sp_std::vec;
use xcm::prelude::*;
use xcm_builder::{
	AsPrefixedGeneralIndex, MatchedConvertedConcreteId, StartsWith, WithLatestLocationConverter,
};
use xcm_executor::traits::{AssetConversion, MatchesFungibles};

/// `Location` vs `AssetIdForTrustBackedAssets` converter for `TrustBackedAssets`
pub type AssetIdForTrustBackedAssetsConvert<TrustBackedAssetsPalletLocation, L = Location> =
	AsPrefixedGeneralIndex<
		TrustBackedAssetsPalletLocation,
		AssetIdForTrustBackedAssets,
		TryConvertInto,
		L,
	>;

/// `Location` vs `CollectionId` converter for `Uniques`
pub type CollectionIdForUniquesConvert<UniquesPalletLocation> =
	AsPrefixedGeneralIndex<UniquesPalletLocation, CollectionId, TryConvertInto>;

/// [`MatchedConvertedConcreteId`] converter dedicated for `TrustBackedAssets`
pub type TrustBackedAssetsConvertedConcreteId<
	TrustBackedAssetsPalletLocation,
	Balance,
	L = Location,
> = MatchedConvertedConcreteId<
	AssetIdForTrustBackedAssets,
	Balance,
	StartsWith<TrustBackedAssetsPalletLocation>,
	AssetIdForTrustBackedAssetsConvert<TrustBackedAssetsPalletLocation, L>,
	TryConvertInto,
>;

/// [`MatchedConvertedConcreteId`] converter dedicated for `Uniques`
pub type UniquesConvertedConcreteId<UniquesPalletLocation> = MatchedConvertedConcreteId<
	CollectionId,
	ItemId,
	// The asset starts with the uniques pallet. The `CollectionId` of the asset is specified as a
	// junction within the pallet itself.
	StartsWith<UniquesPalletLocation>,
	CollectionIdForUniquesConvert<UniquesPalletLocation>,
	TryConvertInto,
>;

/// [`MatchedConvertedConcreteId`] converter dedicated for `TrustBackedAssets`,
/// it is a similar implementation to `TrustBackedAssetsConvertedConcreteId`,
/// but it converts `AssetId` to `xcm::v*::Location` type instead of `AssetIdForTrustBackedAssets =
/// u32`
pub type TrustBackedAssetsAsLocation<
	TrustBackedAssetsPalletLocation,
	Balance,
	L,
	LocationConverter = WithLatestLocationConverter<L>,
> = MatchedConvertedConcreteId<
	L,
	Balance,
	StartsWith<TrustBackedAssetsPalletLocation>,
	LocationConverter,
	TryConvertInto,
>;

/// [`MatchedConvertedConcreteId`] converter dedicated for storing `ForeignAssets` with `AssetId` as
/// `Location`.
///
/// Excludes by default:
/// - parent as relay chain
/// - all local Locations
///
/// `AdditionalLocationExclusionFilter` can customize additional excluded Locations
pub type ForeignAssetsConvertedConcreteId<
	AdditionalLocationExclusionFilter,
	Balance,
	AssetId,
	LocationToAssetIdConverter = WithLatestLocationConverter<AssetId>,
	BalanceConverter = TryConvertInto,
> = MatchedConvertedConcreteId<
	AssetId,
	Balance,
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
	LocationToAssetIdConverter,
	BalanceConverter,
>;

type AssetIdForPoolAssets = u32;
/// `Location` vs `AssetIdForPoolAssets` converter for `PoolAssets`.
pub type AssetIdForPoolAssetsConvert<PoolAssetsPalletLocation> =
	AsPrefixedGeneralIndex<PoolAssetsPalletLocation, AssetIdForPoolAssets, TryConvertInto>;
/// [`MatchedConvertedConcreteId`] converter dedicated for `PoolAssets`
pub type PoolAssetsConvertedConcreteId<PoolAssetsPalletLocation, Balance> =
	MatchedConvertedConcreteId<
		AssetIdForPoolAssets,
		Balance,
		StartsWith<PoolAssetsPalletLocation>,
		AssetIdForPoolAssetsConvert<PoolAssetsPalletLocation>,
		TryConvertInto,
	>;

/// Implementation of `AssetConverter` that pays delivery fees by swapping the given asset to the
/// `Target` asset accepted for fees.
pub struct SwapAssetConverter<Target, Runtime, Fungibles, Matcher, SwapCredit, AccountId>(
	PhantomData<(Target, Runtime, Fungibles, Matcher, SwapCredit, AccountId)>,
);
impl<Target, Runtime, Fungibles, Matcher, SwapCredit, AccountId> AssetConversion
	for SwapAssetConverter<Target, Runtime, Fungibles, Matcher, SwapCredit, AccountId>
where
	Target: Get<Fungibles::AssetId>,
	Runtime: pallet_asset_conversion::Config<Balance = u128, AssetKind = Fungibles::AssetId>,
	Fungibles: fungibles::Balanced<AccountId, Balance = u128>,
	Matcher: MatchesFungibles<Fungibles::AssetId, Fungibles::Balance>,
	SwapCredit: SwapCreditT<
		AccountId,
		Balance = u128,
		AssetKind = Fungibles::AssetId,
		Credit = fungibles::Credit<AccountId, Fungibles>,
	>,
{
	fn convert_asset(asset: &Asset, asset_id: &AssetId) -> Result<Asset, XcmError> {
		// We return `asset` immediately if that's the asset wanted to pay for fees.
		if asset.id == *asset_id {
			return Ok(asset.clone());
		}
		let desired_asset: Asset = (asset_id.clone(), 1u128).into(); // To comply with the interface.
		let (fungibles_asset, _) = Matcher::matches_fungibles(&desired_asset).map_err(|_| {
			// Using `trace` instead of `error` since we expect this to happen
			// when using multiple implementations in a tuple.
			log::trace!(
				target: "xcm::SwapAssetConverter::convert_asset",
				"Could not map XCM asset {:?} to FRAME asset",
				asset_id,
			);
			XcmError::AssetNotFound
		})?;
		let Fungibility::Fungible(old_asset_amount) = asset.fun else {
			log::error!(
				target: "xcm::SwapAssetConverter::convert_asset",
				"Fee asset {:?} is not fungible",
				asset,
			);
			return Err(XcmError::AssetNotFound);
		};

		let new_asset_amount =
			pallet_asset_conversion::Pallet::<Runtime>::quote_price_tokens_for_exact_tokens(
				fungibles_asset,
				Target::get(),
				old_asset_amount,
				true,
			)
			.ok_or(XcmError::FeesNotMet)?;

		Ok((asset_id.clone(), new_asset_amount).into())
	}

	fn swap(give: &Asset, want: &Asset) -> Result<Asset, XcmError> {
		if *give == *want {
			// Swap not needed.
			return Ok(give.clone());
		}
		let (fungibles_asset, balance) = Matcher::matches_fungibles(&give).map_err(|error| {
			log::trace!(
				target: "xcm::SwapAssetConverter::swap",
				"Could not map XCM asset {:?} to FRAME asset. Error: {:?}",
				give,
				error,
			);
			XcmError::AssetNotFound
		})?;
		let Fungibility::Fungible(fee_amount) = want.fun else {
			log::error!(
				target: "xcm::SwapAssetConverter::swap",
				"Fee asset is not fungible",
			);
			return Err(XcmError::AssetNotFound);
		};

		let swap_asset = fungibles_asset.clone().into();
		let credit_in = Fungibles::issue(fungibles_asset, balance);

		// Swap the user's asset for `asset`.
		let (credit_out, credit_change) = SwapCredit::swap_tokens_for_exact_tokens(
			vec![swap_asset, Target::get()],
			credit_in,
			fee_amount,
		)
		.map_err(|(credit_in, _)| {
			drop(credit_in);
			XcmError::FeesNotMet
		})?;

		assert!(credit_change.peek() == Zero::zero());

		Ok((want.id.clone(), credit_out.peek()).into())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_runtime::traits::MaybeEquivalence;
	use xcm::prelude::*;
	use xcm_builder::{StartsWithExplicitGlobalConsensus, WithLatestLocationConverter};
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
			AssetIdForTrustBackedAssetsConvert::<TrustBackedAssetsPalletLocation>::convert_back(
				&local_asset_id
			)
			.unwrap(),
			expected_reverse_ref
		);
		assert_eq!(
			AssetIdForTrustBackedAssetsConvert::<TrustBackedAssetsPalletLocation>::convert(
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
		// set up a converter
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
	fn foreign_assets_converted_concrete_id_converter_works() {
		frame_support::parameter_types! {
			pub Parachain100Pattern: Location = Location::new(1, [Parachain(100)]);
			pub UniversalLocationNetworkId: NetworkId = NetworkId::ByGenesis([9; 32]);
		}

		// set up a converter which uses `xcm::v3::Location` under the hood
		type Convert = ForeignAssetsConvertedConcreteId<
			(
				StartsWith<Parachain100Pattern>,
				StartsWithExplicitGlobalConsensus<UniversalLocationNetworkId>,
			),
			u128,
			xcm::v3::Location,
			WithLatestLocationConverter<xcm::v3::Location>,
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
