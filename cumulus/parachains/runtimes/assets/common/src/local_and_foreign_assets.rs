// Copyright (C) 2021 Parity Technologies (UK) Ltd.
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

use crate::local_and_foreign_assets::fungibles::Inspect;
use cumulus_primitives_core::InteriorMultiLocation;
use frame_support::{
	pallet_prelude::DispatchError,
	traits::{
		fungibles::{
			self, Balanced, Create, HandleImbalanceDrop, Mutate as MutateFungible, Unbalanced,
		},
		tokens::{DepositConsequence, Fortitude, Preservation, Provenance, WithdrawConsequence},
		AccountTouch, ContainsPair, Get, PalletInfoAccess,
	},
};
use pallet_asset_conversion::MultiAssetIdConverter;
use parachains_common::{AccountId, AssetIdForTrustBackedAssets};
use sp_runtime::{traits::MaybeEquivalence, DispatchResult};
use sp_std::{boxed::Box, marker::PhantomData};
use xcm::{latest::MultiLocation, opaque::lts::Junctions::Here};
use xcm_builder::AsPrefixedGeneralIndex;
use xcm_executor::traits::JustTry;

/// Whether the multilocation refers to an asset in the local assets pallet or not,
/// and if return the asset id.
fn is_local<TrustBackedAssetsPalletLocation: Get<MultiLocation>>(
	multilocation: MultiLocation,
) -> Option<u32> {
	AsPrefixedGeneralIndex::<TrustBackedAssetsPalletLocation, AssetIdForTrustBackedAssets, JustTry>::convert(&multilocation)
}

pub struct MultiLocationConverter<Balances, ParachainLocation: Get<InteriorMultiLocation>> {
	_phantom: PhantomData<(Balances, ParachainLocation)>,
}

impl<Balances, ParachainLocation> MultiAssetIdConverter<Box<MultiLocation>, MultiLocation>
	for MultiLocationConverter<Balances, ParachainLocation>
where
	Balances: PalletInfoAccess,
	ParachainLocation: Get<InteriorMultiLocation>,
{
	fn get_native() -> Box<MultiLocation> {
		Box::new(MultiLocation { parents: 0, interior: Here })
	}

	fn is_native(asset_id: &Box<MultiLocation>) -> bool {
		let mut asset_id = asset_id.clone();
		asset_id.simplify(&ParachainLocation::get());
		*asset_id == *Self::get_native()
	}

	fn try_convert(asset_id: &Box<MultiLocation>) -> Result<MultiLocation, ()> {
		let mut asset_id = asset_id.clone();
		asset_id.simplify(&ParachainLocation::get());
		if Self::is_native(&asset_id) {
			// Otherwise it will try and touch the asset to create an account.
			return Err(())
		}
		// Return simplified MultiLocation:
		Ok(*asset_id)
	}

	fn into_multiasset_id(asset_id: &MultiLocation) -> Box<MultiLocation> {
		let mut asset_id = *asset_id;
		asset_id.simplify(&ParachainLocation::get());
		Box::new(asset_id)
	}
}

pub struct LocalAndForeignAssets<Assets, ForeignAssets, Location> {
	_phantom: PhantomData<(Assets, ForeignAssets, Location)>,
}

impl<Assets, ForeignAssets, Location> Unbalanced<AccountId>
	for LocalAndForeignAssets<Assets, ForeignAssets, Location>
where
	Location: Get<MultiLocation>,
	ForeignAssets: Inspect<AccountId, Balance = u128, AssetId = MultiLocation>
		+ Unbalanced<AccountId>
		+ Balanced<AccountId>,
	Assets: Inspect<AccountId, Balance = u128, AssetId = u32>
		+ Unbalanced<AccountId>
		+ Balanced<AccountId>
		+ PalletInfoAccess,
{
	fn handle_dust(dust: frame_support::traits::fungibles::Dust<AccountId, Self>) {
		let credit = dust.into_credit();

		if let Some(asset) = is_local::<Location>(credit.asset()) {
			Assets::handle_raw_dust(asset, credit.peek());
		} else {
			ForeignAssets::handle_raw_dust(credit.asset(), credit.peek());
		}

		// As we have already handled the dust, we must stop credit's drop from happening:
		sp_std::mem::forget(credit);
	}

	fn write_balance(
		asset: <Self as frame_support::traits::fungibles::Inspect<AccountId>>::AssetId,
		who: &AccountId,
		amount: <Self as frame_support::traits::fungibles::Inspect<AccountId>>::Balance,
	) -> Result<
		Option<<Self as frame_support::traits::fungibles::Inspect<AccountId>>::Balance>,
		sp_runtime::DispatchError,
	> {
		if let Some(asset) = is_local::<Location>(asset) {
			Assets::write_balance(asset, who, amount)
		} else {
			ForeignAssets::write_balance(asset, who, amount)
		}
	}

	/// Set the total issuance of `asset` to `amount`.
	fn set_total_issuance(asset: Self::AssetId, amount: Self::Balance) {
		if let Some(asset) = is_local::<Location>(asset) {
			Assets::set_total_issuance(asset, amount)
		} else {
			ForeignAssets::set_total_issuance(asset, amount)
		}
	}
}

impl<Assets, ForeignAssets, Location> Inspect<AccountId>
	for LocalAndForeignAssets<Assets, ForeignAssets, Location>
where
	Location: Get<MultiLocation>,
	ForeignAssets: Inspect<AccountId, Balance = u128, AssetId = MultiLocation>,
	Assets: Inspect<AccountId, Balance = u128, AssetId = u32>,
{
	type AssetId = MultiLocation;
	type Balance = u128;

	/// The total amount of issuance in the system.
	fn total_issuance(asset: Self::AssetId) -> Self::Balance {
		if let Some(asset) = is_local::<Location>(asset) {
			Assets::total_issuance(asset)
		} else {
			ForeignAssets::total_issuance(asset)
		}
	}

	/// The minimum balance any single account may have.
	fn minimum_balance(asset: Self::AssetId) -> Self::Balance {
		if let Some(asset) = is_local::<Location>(asset) {
			Assets::minimum_balance(asset)
		} else {
			ForeignAssets::minimum_balance(asset)
		}
	}

	/// Get the `asset` balance of `who`.
	fn balance(asset: Self::AssetId, who: &AccountId) -> Self::Balance {
		if let Some(asset) = is_local::<Location>(asset) {
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
		if let Some(asset) = is_local::<Location>(asset) {
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
		if let Some(asset) = is_local::<Location>(asset) {
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
		if let Some(asset) = is_local::<Location>(asset) {
			Assets::can_withdraw(asset, who, amount)
		} else {
			ForeignAssets::can_withdraw(asset, who, amount)
		}
	}

	/// Returns `true` if an `asset` exists.
	fn asset_exists(asset: Self::AssetId) -> bool {
		if let Some(asset) = is_local::<Location>(asset) {
			Assets::asset_exists(asset)
		} else {
			ForeignAssets::asset_exists(asset)
		}
	}

	fn total_balance(
		asset: <Self as frame_support::traits::fungibles::Inspect<AccountId>>::AssetId,
		account: &AccountId,
	) -> <Self as frame_support::traits::fungibles::Inspect<AccountId>>::Balance {
		if let Some(asset) = is_local::<Location>(asset) {
			Assets::total_balance(asset, account)
		} else {
			ForeignAssets::total_balance(asset, account)
		}
	}
}

impl<Assets, ForeignAssets, Location> MutateFungible<AccountId>
	for LocalAndForeignAssets<Assets, ForeignAssets, Location>
where
	Location: Get<MultiLocation>,
	ForeignAssets: MutateFungible<AccountId, Balance = u128>
		+ Inspect<AccountId, Balance = u128, AssetId = MultiLocation>
		+ Balanced<AccountId>,
	Assets: MutateFungible<AccountId>
		+ Inspect<AccountId, Balance = u128, AssetId = u32>
		+ Balanced<AccountId>
		+ PalletInfoAccess,
{
	/// Transfer funds from one account into another.
	fn transfer(
		asset: MultiLocation,
		source: &AccountId,
		dest: &AccountId,
		amount: Self::Balance,
		keep_alive: Preservation,
	) -> Result<Self::Balance, DispatchError> {
		if let Some(asset_id) = is_local::<Location>(asset) {
			Assets::transfer(asset_id, source, dest, amount, keep_alive)
		} else {
			ForeignAssets::transfer(asset, source, dest, amount, keep_alive)
		}
	}
}

impl<Assets, ForeignAssets, Location> Create<AccountId>
	for LocalAndForeignAssets<Assets, ForeignAssets, Location>
where
	Location: Get<MultiLocation>,
	ForeignAssets: Create<AccountId> + Inspect<AccountId, Balance = u128, AssetId = MultiLocation>,
	Assets: Create<AccountId> + Inspect<AccountId, Balance = u128, AssetId = u32>,
{
	/// Create a new fungible asset.
	fn create(
		asset_id: Self::AssetId,
		admin: AccountId,
		is_sufficient: bool,
		min_balance: Self::Balance,
	) -> DispatchResult {
		if let Some(asset_id) = is_local::<Location>(asset_id) {
			Assets::create(asset_id, admin, is_sufficient, min_balance)
		} else {
			ForeignAssets::create(asset_id, admin, is_sufficient, min_balance)
		}
	}
}

impl<Assets, ForeignAssets, Location> AccountTouch<MultiLocation, AccountId>
	for LocalAndForeignAssets<Assets, ForeignAssets, Location>
where
	Location: Get<MultiLocation>,
	ForeignAssets: AccountTouch<MultiLocation, AccountId, Balance = u128>,
	Assets: AccountTouch<u32, AccountId, Balance = u128>,
{
	type Balance = u128;

	fn deposit_required(
		asset_id: MultiLocation,
	) -> <Self as AccountTouch<MultiLocation, AccountId>>::Balance {
		if let Some(asset_id) = is_local::<Location>(asset_id) {
			Assets::deposit_required(asset_id)
		} else {
			ForeignAssets::deposit_required(asset_id)
		}
	}

	fn touch(
		asset_id: MultiLocation,
		who: AccountId,
		depositor: AccountId,
	) -> Result<(), sp_runtime::DispatchError> {
		if let Some(asset_id) = is_local::<Location>(asset_id) {
			Assets::touch(asset_id, who, depositor)
		} else {
			ForeignAssets::touch(asset_id, who, depositor)
		}
	}
}

/// Implements [`ContainsPair`] trait for a pair of asset and account IDs.
impl<Assets, ForeignAssets, Location> ContainsPair<MultiLocation, AccountId>
	for LocalAndForeignAssets<Assets, ForeignAssets, Location>
where
	Location: Get<MultiLocation>,
	ForeignAssets: ContainsPair<MultiLocation, AccountId>,
	Assets: PalletInfoAccess + ContainsPair<u32, AccountId>,
{
	/// Check if an account with the given asset ID and account address exists.
	fn contains(asset_id: &MultiLocation, who: &AccountId) -> bool {
		if let Some(asset_id) = is_local::<Location>(*asset_id) {
			Assets::contains(&asset_id, &who)
		} else {
			ForeignAssets::contains(&asset_id, &who)
		}
	}
}

impl<Assets, ForeignAssets, Location> Balanced<AccountId>
	for LocalAndForeignAssets<Assets, ForeignAssets, Location>
where
	Location: Get<MultiLocation>,
	ForeignAssets:
		Balanced<AccountId> + Inspect<AccountId, Balance = u128, AssetId = MultiLocation>,
	Assets:
		Balanced<AccountId> + Inspect<AccountId, Balance = u128, AssetId = u32> + PalletInfoAccess,
{
	type OnDropDebt = DebtDropIndirection<Assets, ForeignAssets, Location>;
	type OnDropCredit = CreditDropIndirection<Assets, ForeignAssets, Location>;
}

pub struct DebtDropIndirection<Assets, ForeignAssets, Location> {
	_phantom: PhantomData<LocalAndForeignAssets<Assets, ForeignAssets, Location>>,
}

impl<Assets, ForeignAssets, Location> HandleImbalanceDrop<MultiLocation, u128>
	for DebtDropIndirection<Assets, ForeignAssets, Location>
where
	Location: Get<MultiLocation>,
	ForeignAssets:
		Balanced<AccountId> + Inspect<AccountId, Balance = u128, AssetId = MultiLocation>,
	Assets: Balanced<AccountId> + Inspect<AccountId, Balance = u128, AssetId = u32>,
{
	fn handle(asset: MultiLocation, amount: u128) {
		if let Some(asset_id) = is_local::<Location>(asset) {
			Assets::OnDropDebt::handle(asset_id, amount);
		} else {
			ForeignAssets::OnDropDebt::handle(asset, amount);
		}
	}
}

pub struct CreditDropIndirection<Assets, ForeignAssets, Location> {
	_phantom: PhantomData<LocalAndForeignAssets<Assets, ForeignAssets, Location>>,
}

impl<Assets, ForeignAssets, Location> HandleImbalanceDrop<MultiLocation, u128>
	for CreditDropIndirection<Assets, ForeignAssets, Location>
where
	Location: Get<MultiLocation>,
	ForeignAssets:
		Balanced<AccountId> + Inspect<AccountId, Balance = u128, AssetId = MultiLocation>,
	Assets: Balanced<AccountId> + Inspect<AccountId, Balance = u128, AssetId = u32>,
{
	fn handle(asset: MultiLocation, amount: u128) {
		if let Some(asset_id) = is_local::<Location>(asset) {
			Assets::OnDropCredit::handle(asset_id, amount);
		} else {
			ForeignAssets::OnDropCredit::handle(asset, amount);
		}
	}
}
