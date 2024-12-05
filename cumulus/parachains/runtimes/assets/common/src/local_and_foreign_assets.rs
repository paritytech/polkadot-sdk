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

use frame_support::traits::{
	fungibles::{Balanced, Create, HandleImbalanceDrop, Inspect, Mutate, Unbalanced},
	tokens::{
		DepositConsequence, Fortitude, Precision, Preservation, Provenance, WithdrawConsequence,
	},
	AccountTouch, Contains, ContainsPair, Get, PalletInfoAccess,
};
use pallet_asset_conversion::{MultiAssetIdConversionResult, MultiAssetIdConverter};
use parachains_common::AccountId;
use sp_runtime::{traits::MaybeEquivalence, DispatchError, DispatchResult};
use sp_std::{boxed::Box, marker::PhantomData};
use xcm::latest::MultiLocation;

pub struct MultiLocationConverter<NativeAssetLocation: Get<MultiLocation>, MultiLocationMatcher> {
	_phantom: PhantomData<(NativeAssetLocation, MultiLocationMatcher)>,
}

impl<NativeAssetLocation, MultiLocationMatcher>
	MultiAssetIdConverter<Box<MultiLocation>, MultiLocation>
	for MultiLocationConverter<NativeAssetLocation, MultiLocationMatcher>
where
	NativeAssetLocation: Get<MultiLocation>,
	MultiLocationMatcher: Contains<MultiLocation>,
{
	fn get_native() -> Box<MultiLocation> {
		Box::new(NativeAssetLocation::get())
	}

	fn is_native(asset_id: &Box<MultiLocation>) -> bool {
		*asset_id == Self::get_native()
	}

	fn try_convert(
		asset_id: &Box<MultiLocation>,
	) -> MultiAssetIdConversionResult<Box<MultiLocation>, MultiLocation> {
		if Self::is_native(&asset_id) {
			return MultiAssetIdConversionResult::Native
		}

		if MultiLocationMatcher::contains(&asset_id) {
			MultiAssetIdConversionResult::Converted(*asset_id.clone())
		} else {
			MultiAssetIdConversionResult::Unsupported(asset_id.clone())
		}
	}
}

pub trait MatchesLocalAndForeignAssetsMultiLocation {
	fn is_local(location: &MultiLocation) -> bool;
	fn is_foreign(location: &MultiLocation) -> bool;
}

pub struct LocalAndForeignAssets<Assets, LocalAssetIdConverter, ForeignAssets> {
	_phantom: PhantomData<(Assets, LocalAssetIdConverter, ForeignAssets)>,
}

impl<Assets, LocalAssetIdConverter, ForeignAssets> Unbalanced<AccountId>
	for LocalAndForeignAssets<Assets, LocalAssetIdConverter, ForeignAssets>
where
	Assets: Inspect<AccountId, Balance = u128, AssetId = u32>
		+ Unbalanced<AccountId>
		+ Balanced<AccountId>
		+ PalletInfoAccess,
	LocalAssetIdConverter: MaybeEquivalence<MultiLocation, u32>,
	ForeignAssets: Inspect<AccountId, Balance = u128, AssetId = MultiLocation>
		+ Unbalanced<AccountId>
		+ Balanced<AccountId>,
{
	fn handle_dust(dust: frame_support::traits::fungibles::Dust<AccountId, Self>) {
		let credit = dust.into_credit();

		if let Some(asset) = LocalAssetIdConverter::convert(&credit.asset()) {
			Assets::handle_raw_dust(asset, credit.peek());
		} else {
			ForeignAssets::handle_raw_dust(credit.asset(), credit.peek());
		}

		// As we have already handled the dust, we must stop credit's drop from happening:
		sp_std::mem::forget(credit);
	}

	fn write_balance(
		asset: <Self as Inspect<AccountId>>::AssetId,
		who: &AccountId,
		amount: <Self as Inspect<AccountId>>::Balance,
	) -> Result<Option<<Self as Inspect<AccountId>>::Balance>, DispatchError> {
		if let Some(asset) = LocalAssetIdConverter::convert(&asset) {
			Assets::write_balance(asset, who, amount)
		} else {
			ForeignAssets::write_balance(asset, who, amount)
		}
	}

	/// Set the total issuance of `asset` to `amount`.
	fn set_total_issuance(asset: Self::AssetId, amount: Self::Balance) {
		if let Some(asset) = LocalAssetIdConverter::convert(&asset) {
			Assets::set_total_issuance(asset, amount)
		} else {
			ForeignAssets::set_total_issuance(asset, amount)
		}
	}

	fn decrease_balance(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
		precision: Precision,
		preservation: Preservation,
		force: Fortitude,
	) -> Result<Self::Balance, DispatchError> {
		if let Some(asset) = LocalAssetIdConverter::convert(&asset) {
			Assets::decrease_balance(asset, who, amount, precision, preservation, force)
		} else {
			ForeignAssets::decrease_balance(asset, who, amount, precision, preservation, force)
		}
	}

	fn increase_balance(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
		precision: Precision,
	) -> Result<Self::Balance, DispatchError> {
		if let Some(asset) = LocalAssetIdConverter::convert(&asset) {
			Assets::increase_balance(asset, who, amount, precision)
		} else {
			ForeignAssets::increase_balance(asset, who, amount, precision)
		}
	}
}

impl<Assets, LocalAssetIdConverter, ForeignAssets> Inspect<AccountId>
	for LocalAndForeignAssets<Assets, LocalAssetIdConverter, ForeignAssets>
where
	Assets: Inspect<AccountId, Balance = u128, AssetId = u32>,
	LocalAssetIdConverter: MaybeEquivalence<MultiLocation, u32>,
	ForeignAssets: Inspect<AccountId, Balance = u128, AssetId = MultiLocation>,
{
	type AssetId = MultiLocation;
	type Balance = u128;

	/// The total amount of issuance in the system.
	fn total_issuance(asset: Self::AssetId) -> Self::Balance {
		if let Some(asset) = LocalAssetIdConverter::convert(&asset) {
			Assets::total_issuance(asset)
		} else {
			ForeignAssets::total_issuance(asset)
		}
	}

	/// The minimum balance any single account may have.
	fn minimum_balance(asset: Self::AssetId) -> Self::Balance {
		if let Some(asset) = LocalAssetIdConverter::convert(&asset) {
			Assets::minimum_balance(asset)
		} else {
			ForeignAssets::minimum_balance(asset)
		}
	}

	fn total_balance(
		asset: <Self as Inspect<AccountId>>::AssetId,
		account: &AccountId,
	) -> <Self as Inspect<AccountId>>::Balance {
		if let Some(asset) = LocalAssetIdConverter::convert(&asset) {
			Assets::total_balance(asset, account)
		} else {
			ForeignAssets::total_balance(asset, account)
		}
	}

	/// Get the `asset` balance of `who`.
	fn balance(asset: Self::AssetId, who: &AccountId) -> Self::Balance {
		if let Some(asset) = LocalAssetIdConverter::convert(&asset) {
			Assets::balance(asset, who)
		} else {
			ForeignAssets::balance(asset, who)
		}
	}

	/// Get the maximum amount of `asset` that `who` can withdraw/transfer successfully.
	fn reducible_balance(
		asset: Self::AssetId,
		who: &AccountId,
		presevation: Preservation,
		fortitude: Fortitude,
	) -> Self::Balance {
		if let Some(asset) = LocalAssetIdConverter::convert(&asset) {
			Assets::reducible_balance(asset, who, presevation, fortitude)
		} else {
			ForeignAssets::reducible_balance(asset, who, presevation, fortitude)
		}
	}

	/// Returns `true` if the `asset` balance of `who` may be increased by `amount`.
	///
	/// - `asset`: The asset that should be deposited.
	/// - `who`: The account of which the balance should be increased by `amount`.
	/// - `amount`: How much should the balance be increased?
	/// - `mint`: Will `amount` be minted to deposit it into `account`?
	fn can_deposit(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
		mint: Provenance,
	) -> DepositConsequence {
		if let Some(asset) = LocalAssetIdConverter::convert(&asset) {
			Assets::can_deposit(asset, who, amount, mint)
		} else {
			ForeignAssets::can_deposit(asset, who, amount, mint)
		}
	}

	/// Returns `Failed` if the `asset` balance of `who` may not be decreased by `amount`, otherwise
	/// the consequence.
	fn can_withdraw(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
	) -> WithdrawConsequence<Self::Balance> {
		if let Some(asset) = LocalAssetIdConverter::convert(&asset) {
			Assets::can_withdraw(asset, who, amount)
		} else {
			ForeignAssets::can_withdraw(asset, who, amount)
		}
	}

	/// Returns `true` if an `asset` exists.
	fn asset_exists(asset: Self::AssetId) -> bool {
		if let Some(asset) = LocalAssetIdConverter::convert(&asset) {
			Assets::asset_exists(asset)
		} else {
			ForeignAssets::asset_exists(asset)
		}
	}
}

impl<Assets, LocalAssetIdConverter, ForeignAssets> Mutate<AccountId>
	for LocalAndForeignAssets<Assets, LocalAssetIdConverter, ForeignAssets>
where
	Assets: Mutate<AccountId>
		+ Inspect<AccountId, Balance = u128, AssetId = u32>
		+ Balanced<AccountId>
		+ PalletInfoAccess,
	LocalAssetIdConverter: MaybeEquivalence<MultiLocation, u32>,
	ForeignAssets: Mutate<AccountId, Balance = u128>
		+ Inspect<AccountId, Balance = u128, AssetId = MultiLocation>
		+ Balanced<AccountId>,
{
	/// Transfer funds from one account into another.
	fn transfer(
		asset: MultiLocation,
		source: &AccountId,
		dest: &AccountId,
		amount: Self::Balance,
		keep_alive: Preservation,
	) -> Result<Self::Balance, DispatchError> {
		if let Some(asset_id) = LocalAssetIdConverter::convert(&asset) {
			Assets::transfer(asset_id, source, dest, amount, keep_alive)
		} else {
			ForeignAssets::transfer(asset, source, dest, amount, keep_alive)
		}
	}
}

impl<Assets, LocalAssetIdConverter, ForeignAssets> Create<AccountId>
	for LocalAndForeignAssets<Assets, LocalAssetIdConverter, ForeignAssets>
where
	Assets: Create<AccountId> + Inspect<AccountId, Balance = u128, AssetId = u32>,
	LocalAssetIdConverter: MaybeEquivalence<MultiLocation, u32>,
	ForeignAssets: Create<AccountId> + Inspect<AccountId, Balance = u128, AssetId = MultiLocation>,
{
	/// Create a new fungible asset.
	fn create(
		asset_id: Self::AssetId,
		admin: AccountId,
		is_sufficient: bool,
		min_balance: Self::Balance,
	) -> DispatchResult {
		if let Some(asset_id) = LocalAssetIdConverter::convert(&asset_id) {
			Assets::create(asset_id, admin, is_sufficient, min_balance)
		} else {
			ForeignAssets::create(asset_id, admin, is_sufficient, min_balance)
		}
	}
}

impl<Assets, LocalAssetIdConverter, ForeignAssets> AccountTouch<MultiLocation, AccountId>
	for LocalAndForeignAssets<Assets, LocalAssetIdConverter, ForeignAssets>
where
	Assets: AccountTouch<u32, AccountId, Balance = u128>,
	LocalAssetIdConverter: MaybeEquivalence<MultiLocation, u32>,
	ForeignAssets: AccountTouch<MultiLocation, AccountId, Balance = u128>,
{
	type Balance = u128;

	fn deposit_required(
		asset_id: MultiLocation,
	) -> <Self as AccountTouch<MultiLocation, AccountId>>::Balance {
		if let Some(asset_id) = LocalAssetIdConverter::convert(&asset_id) {
			Assets::deposit_required(asset_id)
		} else {
			ForeignAssets::deposit_required(asset_id)
		}
	}

	fn touch(
		asset_id: MultiLocation,
		who: AccountId,
		depositor: AccountId,
	) -> Result<(), DispatchError> {
		if let Some(asset_id) = LocalAssetIdConverter::convert(&asset_id) {
			Assets::touch(asset_id, who, depositor)
		} else {
			ForeignAssets::touch(asset_id, who, depositor)
		}
	}
}

/// Implements [`ContainsPair`] trait for a pair of asset and account IDs.
impl<Assets, LocalAssetIdConverter, ForeignAssets> ContainsPair<MultiLocation, AccountId>
	for LocalAndForeignAssets<Assets, LocalAssetIdConverter, ForeignAssets>
where
	Assets: PalletInfoAccess + ContainsPair<u32, AccountId>,
	LocalAssetIdConverter: MaybeEquivalence<MultiLocation, u32>,
	ForeignAssets: ContainsPair<MultiLocation, AccountId>,
{
	/// Check if an account with the given asset ID and account address exists.
	fn contains(asset_id: &MultiLocation, who: &AccountId) -> bool {
		if let Some(asset_id) = LocalAssetIdConverter::convert(asset_id) {
			Assets::contains(&asset_id, &who)
		} else {
			ForeignAssets::contains(&asset_id, &who)
		}
	}
}

impl<Assets, LocalAssetIdConverter, ForeignAssets> Balanced<AccountId>
	for LocalAndForeignAssets<Assets, LocalAssetIdConverter, ForeignAssets>
where
	Assets:
		Balanced<AccountId> + Inspect<AccountId, Balance = u128, AssetId = u32> + PalletInfoAccess,
	LocalAssetIdConverter: MaybeEquivalence<MultiLocation, u32>,
	ForeignAssets:
		Balanced<AccountId> + Inspect<AccountId, Balance = u128, AssetId = MultiLocation>,
{
	type OnDropDebt = DebtDropIndirection<Assets, LocalAssetIdConverter, ForeignAssets>;
	type OnDropCredit = CreditDropIndirection<Assets, LocalAssetIdConverter, ForeignAssets>;
}

pub struct DebtDropIndirection<Assets, LocalAssetIdConverter, ForeignAssets> {
	_phantom: PhantomData<LocalAndForeignAssets<Assets, LocalAssetIdConverter, ForeignAssets>>,
}

impl<Assets, LocalAssetIdConverter, ForeignAssets> HandleImbalanceDrop<MultiLocation, u128>
	for DebtDropIndirection<Assets, LocalAssetIdConverter, ForeignAssets>
where
	Assets: Balanced<AccountId> + Inspect<AccountId, Balance = u128, AssetId = u32>,
	LocalAssetIdConverter: MaybeEquivalence<MultiLocation, u32>,
	ForeignAssets:
		Balanced<AccountId> + Inspect<AccountId, Balance = u128, AssetId = MultiLocation>,
{
	fn handle(asset: MultiLocation, amount: u128) {
		if let Some(asset_id) = LocalAssetIdConverter::convert(&asset) {
			Assets::OnDropDebt::handle(asset_id, amount);
		} else {
			ForeignAssets::OnDropDebt::handle(asset, amount);
		}
	}
}

pub struct CreditDropIndirection<Assets, LocalAssetIdConverter, ForeignAssets> {
	_phantom: PhantomData<LocalAndForeignAssets<Assets, LocalAssetIdConverter, ForeignAssets>>,
}

impl<Assets, LocalAssetIdConverter, ForeignAssets> HandleImbalanceDrop<MultiLocation, u128>
	for CreditDropIndirection<Assets, LocalAssetIdConverter, ForeignAssets>
where
	Assets: Balanced<AccountId> + Inspect<AccountId, Balance = u128, AssetId = u32>,
	LocalAssetIdConverter: MaybeEquivalence<MultiLocation, u32>,
	ForeignAssets:
		Balanced<AccountId> + Inspect<AccountId, Balance = u128, AssetId = MultiLocation>,
{
	fn handle(asset: MultiLocation, amount: u128) {
		if let Some(asset_id) = LocalAssetIdConverter::convert(&asset) {
			Assets::OnDropCredit::handle(asset_id, amount);
		} else {
			ForeignAssets::OnDropCredit::handle(asset, amount);
		}
	}
}

#[cfg(test)]
mod tests {
	use crate::{
		local_and_foreign_assets::MultiLocationConverter, matching::StartsWith,
		AssetIdForPoolAssetsConvert, AssetIdForTrustBackedAssetsConvert,
	};
	use frame_support::traits::EverythingBut;
	use pallet_asset_conversion::{MultiAssetIdConversionResult, MultiAssetIdConverter};
	use sp_runtime::traits::MaybeEquivalence;
	use xcm::latest::prelude::*;

	#[test]
	fn test_multi_location_converter_works() {
		frame_support::parameter_types! {
			pub const WestendLocation: MultiLocation = MultiLocation::parent();
			pub TrustBackedAssetsPalletLocation: MultiLocation = PalletInstance(50_u8).into();
			pub PoolAssetsPalletLocation: MultiLocation = PalletInstance(55_u8).into();
		}

		type C = MultiLocationConverter<
			WestendLocation,
			EverythingBut<StartsWith<PoolAssetsPalletLocation>>,
		>;

		let native_asset = WestendLocation::get();
		let local_asset =
			AssetIdForTrustBackedAssetsConvert::<TrustBackedAssetsPalletLocation>::convert_back(
				&123,
			)
			.unwrap();
		let pool_asset =
			AssetIdForPoolAssetsConvert::<PoolAssetsPalletLocation>::convert_back(&456).unwrap();
		let foreign_asset1 = MultiLocation { parents: 1, interior: X1(Parachain(2222)) };
		let foreign_asset2 = MultiLocation {
			parents: 2,
			interior: X2(GlobalConsensus(ByGenesis([1; 32])), Parachain(2222)),
		};

		assert!(C::is_native(&Box::new(native_asset)));
		assert!(!C::is_native(&Box::new(local_asset)));
		assert!(!C::is_native(&Box::new(pool_asset)));
		assert!(!C::is_native(&Box::new(foreign_asset1)));
		assert!(!C::is_native(&Box::new(foreign_asset2)));

		assert_eq!(C::try_convert(&Box::new(native_asset)), MultiAssetIdConversionResult::Native);
		assert_eq!(
			C::try_convert(&Box::new(local_asset)),
			MultiAssetIdConversionResult::Converted(local_asset)
		);
		assert_eq!(
			C::try_convert(&Box::new(pool_asset)),
			MultiAssetIdConversionResult::Unsupported(Box::new(pool_asset))
		);
		assert_eq!(
			C::try_convert(&Box::new(foreign_asset1)),
			MultiAssetIdConversionResult::Converted(foreign_asset1)
		);
		assert_eq!(
			C::try_convert(&Box::new(foreign_asset2)),
			MultiAssetIdConversionResult::Converted(foreign_asset2)
		);
	}
}
