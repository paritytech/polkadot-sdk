// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Adapters to work with [`frame_support::traits::tokens::nonfungible`] through XCM.

use crate::MintLocation;
use frame_support::{
	ensure,
	traits::{tokens::nonfungible, Get},
};
use sp_std::{marker::PhantomData, prelude::*, result};
use xcm::latest::prelude::*;
use xcm_executor::traits::{
	ConvertLocation, Error as MatchError, MatchesNonFungible, TransactAsset,
};

const LOG_TARGET: &str = "xcm::nonfungible_adapter";

/// [`TransactAsset`] implementation that allows the use of a [`nonfungible`] implementation for
/// handling an asset in the XCM executor.
/// Only works for transfers.
pub struct NonFungibleTransferAdapter<NonFungible, Matcher, AccountIdConverter, AccountId>(
	PhantomData<(NonFungible, Matcher, AccountIdConverter, AccountId)>,
);
impl<
		NonFungible: nonfungible::Transfer<AccountId>,
		Matcher: MatchesNonFungible<NonFungible::ItemId>,
		AccountIdConverter: ConvertLocation<AccountId>,
		AccountId: Clone, // can't get away without it since Currency is generic over it.
	> TransactAsset
	for NonFungibleTransferAdapter<NonFungible, Matcher, AccountIdConverter, AccountId>
{
	fn transfer_asset(
		what: &Asset,
		from: &Location,
		to: &Location,
		context: &XcmContext,
	) -> result::Result<xcm_executor::AssetsInHolding, XcmError> {
		log::trace!(
			target: LOG_TARGET,
			"transfer_asset what: {:?}, from: {:?}, to: {:?}, context: {:?}",
			what,
			from,
			to,
			context,
		);
		// Check we handle this asset.
		let instance = Matcher::matches_nonfungible(what).ok_or(MatchError::AssetNotHandled)?;
		let destination = AccountIdConverter::convert_location(to)
			.ok_or(MatchError::AccountIdConversionFailed)?;
		NonFungible::transfer(&instance, &destination)
			.map_err(|e| XcmError::FailedToTransactAsset(e.into()))?;
		Ok(what.clone().into())
	}
}

/// [`TransactAsset`] implementation that allows the use of a [`nonfungible`] implementation for
/// handling an asset in the XCM executor.
/// Works for teleport bookkeeping.
pub struct NonFungibleMutateAdapter<
	NonFungible,
	Matcher,
	AccountIdConverter,
	AccountId,
	CheckingAccount,
>(PhantomData<(NonFungible, Matcher, AccountIdConverter, AccountId, CheckingAccount)>);

impl<
		NonFungible: nonfungible::Mutate<AccountId>,
		Matcher: MatchesNonFungible<NonFungible::ItemId>,
		AccountIdConverter: ConvertLocation<AccountId>,
		AccountId: Clone + Eq, // can't get away without it since Currency is generic over it.
		CheckingAccount: Get<Option<(AccountId, MintLocation)>>,
	> NonFungibleMutateAdapter<NonFungible, Matcher, AccountIdConverter, AccountId, CheckingAccount>
{
	fn can_accrue_checked(instance: NonFungible::ItemId) -> XcmResult {
		ensure!(NonFungible::owner(&instance).is_none(), XcmError::NotDepositable);
		Ok(())
	}
	fn can_reduce_checked(checking_account: AccountId, instance: NonFungible::ItemId) -> XcmResult {
		// This is an asset whose teleports we track.
		let owner = NonFungible::owner(&instance);
		ensure!(owner == Some(checking_account), XcmError::NotWithdrawable);
		ensure!(NonFungible::can_transfer(&instance), XcmError::NotWithdrawable);
		Ok(())
	}
	fn accrue_checked(checking_account: AccountId, instance: NonFungible::ItemId) {
		let ok = NonFungible::mint_into(&instance, &checking_account).is_ok();
		debug_assert!(ok, "`mint_into` cannot generally fail; qed");
	}
	fn reduce_checked(instance: NonFungible::ItemId) {
		let ok = NonFungible::burn(&instance, None).is_ok();
		debug_assert!(ok, "`can_check_in` must have returned `true` immediately prior; qed");
	}
}

impl<
		NonFungible: nonfungible::Mutate<AccountId>,
		Matcher: MatchesNonFungible<NonFungible::ItemId>,
		AccountIdConverter: ConvertLocation<AccountId>,
		AccountId: Clone + Eq, // can't get away without it since Currency is generic over it.
		CheckingAccount: Get<Option<(AccountId, MintLocation)>>,
	> TransactAsset
	for NonFungibleMutateAdapter<NonFungible, Matcher, AccountIdConverter, AccountId, CheckingAccount>
{
	fn can_check_in(_origin: &Location, what: &Asset, context: &XcmContext) -> XcmResult {
		log::trace!(
			target: LOG_TARGET,
			"can_check_in origin: {:?}, what: {:?}, context: {:?}",
			_origin,
			what,
			context,
		);
		// Check we handle this asset.
		let instance = Matcher::matches_nonfungible(what).ok_or(MatchError::AssetNotHandled)?;
		match CheckingAccount::get() {
			// We track this asset's teleports to ensure no more come in than have gone out.
			Some((checking_account, MintLocation::Local)) =>
				Self::can_reduce_checked(checking_account, instance),
			// We track this asset's teleports to ensure no more go out than have come in.
			Some((_, MintLocation::NonLocal)) => Self::can_accrue_checked(instance),
			_ => Ok(()),
		}
	}

	fn check_in(_origin: &Location, what: &Asset, context: &XcmContext) {
		log::trace!(
			target: LOG_TARGET,
			"check_in origin: {:?}, what: {:?}, context: {:?}",
			_origin,
			what,
			context,
		);
		if let Some(instance) = Matcher::matches_nonfungible(what) {
			match CheckingAccount::get() {
				// We track this asset's teleports to ensure no more come in than have gone out.
				Some((_, MintLocation::Local)) => Self::reduce_checked(instance),
				// We track this asset's teleports to ensure no more go out than have come in.
				Some((checking_account, MintLocation::NonLocal)) =>
					Self::accrue_checked(checking_account, instance),
				_ => (),
			}
		}
	}

	fn can_check_out(_dest: &Location, what: &Asset, context: &XcmContext) -> XcmResult {
		log::trace!(
			target: LOG_TARGET,
			"can_check_out dest: {:?}, what: {:?}, context: {:?}",
			_dest,
			what,
			context,
		);
		// Check we handle this asset.
		let instance = Matcher::matches_nonfungible(what).ok_or(MatchError::AssetNotHandled)?;
		match CheckingAccount::get() {
			// We track this asset's teleports to ensure no more come in than have gone out.
			Some((_, MintLocation::Local)) => Self::can_accrue_checked(instance),
			// We track this asset's teleports to ensure no more go out than have come in.
			Some((checking_account, MintLocation::NonLocal)) =>
				Self::can_reduce_checked(checking_account, instance),
			_ => Ok(()),
		}
	}

	fn check_out(_dest: &Location, what: &Asset, context: &XcmContext) {
		log::trace!(
			target: LOG_TARGET,
			"check_out dest: {:?}, what: {:?}, context: {:?}",
			_dest,
			what,
			context,
		);
		if let Some(instance) = Matcher::matches_nonfungible(what) {
			match CheckingAccount::get() {
				// We track this asset's teleports to ensure no more come in than have gone out.
				Some((checking_account, MintLocation::Local)) =>
					Self::accrue_checked(checking_account, instance),
				// We track this asset's teleports to ensure no more go out than have come in.
				Some((_, MintLocation::NonLocal)) => Self::reduce_checked(instance),
				_ => (),
			}
		}
	}

	fn deposit_asset(what: &Asset, who: &Location, context: Option<&XcmContext>) -> XcmResult {
		log::trace!(
			target: LOG_TARGET,
			"deposit_asset what: {:?}, who: {:?}, context: {:?}",
			what,
			who,
			context,
		);
		// Check we handle this asset.
		let instance = Matcher::matches_nonfungible(what).ok_or(MatchError::AssetNotHandled)?;
		let who = AccountIdConverter::convert_location(who)
			.ok_or(MatchError::AccountIdConversionFailed)?;
		NonFungible::mint_into(&instance, &who)
			.map_err(|e| XcmError::FailedToTransactAsset(e.into()))
	}

	fn withdraw_asset(
		what: &Asset,
		who: &Location,
		maybe_context: Option<&XcmContext>,
	) -> result::Result<xcm_executor::AssetsInHolding, XcmError> {
		log::trace!(
			target: LOG_TARGET,
			"withdraw_asset what: {:?}, who: {:?}, maybe_context: {:?}",
			what,
			who,
			maybe_context,
		);
		// Check we handle this asset.
		let who = AccountIdConverter::convert_location(who)
			.ok_or(MatchError::AccountIdConversionFailed)?;
		let instance = Matcher::matches_nonfungible(what).ok_or(MatchError::AssetNotHandled)?;
		NonFungible::burn(&instance, Some(&who))
			.map_err(|e| XcmError::FailedToTransactAsset(e.into()))?;
		Ok(what.clone().into())
	}
}

/// [`TransactAsset`] implementation that allows the use of a [`nonfungible`] implementation for
/// handling an asset in the XCM executor.
/// Works for everything.
pub struct NonFungibleAdapter<NonFungible, Matcher, AccountIdConverter, AccountId, CheckingAccount>(
	PhantomData<(NonFungible, Matcher, AccountIdConverter, AccountId, CheckingAccount)>,
);
impl<
		NonFungible: nonfungible::Mutate<AccountId> + nonfungible::Transfer<AccountId>,
		Matcher: MatchesNonFungible<NonFungible::ItemId>,
		AccountIdConverter: ConvertLocation<AccountId>,
		AccountId: Clone + Eq, // can't get away without it since Currency is generic over it.
		CheckingAccount: Get<Option<(AccountId, MintLocation)>>,
	> TransactAsset
	for NonFungibleAdapter<NonFungible, Matcher, AccountIdConverter, AccountId, CheckingAccount>
{
	fn can_check_in(origin: &Location, what: &Asset, context: &XcmContext) -> XcmResult {
		NonFungibleMutateAdapter::<
			NonFungible,
			Matcher,
			AccountIdConverter,
			AccountId,
			CheckingAccount,
		>::can_check_in(origin, what, context)
	}

	fn check_in(origin: &Location, what: &Asset, context: &XcmContext) {
		NonFungibleMutateAdapter::<
			NonFungible,
			Matcher,
			AccountIdConverter,
			AccountId,
			CheckingAccount,
		>::check_in(origin, what, context)
	}

	fn can_check_out(dest: &Location, what: &Asset, context: &XcmContext) -> XcmResult {
		NonFungibleMutateAdapter::<
			NonFungible,
			Matcher,
			AccountIdConverter,
			AccountId,
			CheckingAccount,
		>::can_check_out(dest, what, context)
	}

	fn check_out(dest: &Location, what: &Asset, context: &XcmContext) {
		NonFungibleMutateAdapter::<
			NonFungible,
			Matcher,
			AccountIdConverter,
			AccountId,
			CheckingAccount,
		>::check_out(dest, what, context)
	}

	fn deposit_asset(what: &Asset, who: &Location, context: Option<&XcmContext>) -> XcmResult {
		NonFungibleMutateAdapter::<
			NonFungible,
			Matcher,
			AccountIdConverter,
			AccountId,
			CheckingAccount,
		>::deposit_asset(what, who, context)
	}

	fn withdraw_asset(
		what: &Asset,
		who: &Location,
		maybe_context: Option<&XcmContext>,
	) -> result::Result<xcm_executor::AssetsInHolding, XcmError> {
		NonFungibleMutateAdapter::<
			NonFungible,
			Matcher,
			AccountIdConverter,
			AccountId,
			CheckingAccount,
		>::withdraw_asset(what, who, maybe_context)
	}

	fn transfer_asset(
		what: &Asset,
		from: &Location,
		to: &Location,
		context: &XcmContext,
	) -> result::Result<xcm_executor::AssetsInHolding, XcmError> {
		NonFungibleTransferAdapter::<NonFungible, Matcher, AccountIdConverter, AccountId>::transfer_asset(
			what, from, to, context,
		)
	}
}
