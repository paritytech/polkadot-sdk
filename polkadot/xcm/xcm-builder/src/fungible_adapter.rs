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

//! Adapters to work with [`frame_support::traits::fungible`] through XCM.

use super::MintLocation;
use core::{fmt::Debug, marker::PhantomData, result};
use frame_support::traits::{
	tokens::{
		fungible, Fortitude::Polite, Precision::Exact, Preservation::Expendable, Provenance::Minted,
	},
	Get,
};
use xcm::latest::prelude::*;
use xcm_executor::{
	traits::{ConvertLocation, Error as MatchError, MatchesFungible, TransactAsset},
	AssetsInHolding,
};

/// [`TransactAsset`] implementation that allows the use of a [`fungible`] implementation for
/// handling an asset in the XCM executor.
/// Only works for transfers.
pub struct FungibleTransferAdapter<Fungible, Matcher, AccountIdConverter, AccountId>(
	PhantomData<(Fungible, Matcher, AccountIdConverter, AccountId)>,
);
impl<
		Fungible: fungible::Mutate<AccountId>,
		Matcher: MatchesFungible<Fungible::Balance>,
		AccountIdConverter: ConvertLocation<AccountId>,
		AccountId: Eq + Clone + Debug,
	> TransactAsset for FungibleTransferAdapter<Fungible, Matcher, AccountIdConverter, AccountId>
{
	fn internal_transfer_asset(
		what: &Asset,
		from: &Location,
		to: &Location,
		_context: &XcmContext,
	) -> result::Result<AssetsInHolding, XcmError> {
		tracing::trace!(
			target: "xcm::fungible_adapter",
			?what, ?from, ?to,
			"internal_transfer_asset",
		);
		// Check we handle the asset
		let amount = Matcher::matches_fungible(what).ok_or(MatchError::AssetNotHandled)?;
		let source = AccountIdConverter::convert_location(from)
			.ok_or(MatchError::AccountIdConversionFailed)?;
		let dest = AccountIdConverter::convert_location(to)
			.ok_or(MatchError::AccountIdConversionFailed)?;
		Fungible::transfer(&source, &dest, amount, Expendable).map_err(|error| {
			tracing::debug!(
				target: "xcm::fungible_adapter", ?error, ?source, ?dest, ?amount,
				"Failed to transfer asset",
			);
			XcmError::FailedToTransactAsset(error.into())
		})?;
		Ok(what.clone().into())
	}
}

/// [`TransactAsset`] implementation that allows the use of a [`fungible`] implementation for
/// handling an asset in the XCM executor.
/// Works for everything but transfers.
pub struct FungibleMutateAdapter<Fungible, Matcher, AccountIdConverter, AccountId, CheckingAccount>(
	PhantomData<(Fungible, Matcher, AccountIdConverter, AccountId, CheckingAccount)>,
);

impl<
		Fungible: fungible::Mutate<AccountId>,
		Matcher: MatchesFungible<Fungible::Balance>,
		AccountIdConverter: ConvertLocation<AccountId>,
		AccountId: Eq + Clone + Debug,
		CheckingAccount: Get<Option<(AccountId, MintLocation)>>,
	> FungibleMutateAdapter<Fungible, Matcher, AccountIdConverter, AccountId, CheckingAccount>
{
	fn can_accrue_checked(checking_account: AccountId, amount: Fungible::Balance) -> XcmResult {
		Fungible::can_deposit(&checking_account, amount, Minted)
			.into_result()
			.map_err(|error| {
				tracing::debug!(
					target: "xcm::fungible_adapter", ?error, ?checking_account, ?amount,
					"Failed to deposit funds into account",
				);
				XcmError::NotDepositable
			})
	}

	fn can_reduce_checked(checking_account: AccountId, amount: Fungible::Balance) -> XcmResult {
		Fungible::can_withdraw(&checking_account, amount)
			.into_result(false)
			.map_err(|error| {
				tracing::debug!(
					target: "xcm::fungible_adapter", ?error, ?checking_account, ?amount,
					"Failed to withdraw funds from account",
				);
				XcmError::NotWithdrawable
			})
			.map(|_| ())
	}

	fn accrue_checked(checking_account: AccountId, amount: Fungible::Balance) {
		let ok = Fungible::mint_into(&checking_account, amount).is_ok();
		debug_assert!(ok, "`can_accrue_checked` must have returned `true` immediately prior; qed");
	}

	fn reduce_checked(checking_account: AccountId, amount: Fungible::Balance) {
		let ok = Fungible::burn_from(&checking_account, amount, Expendable, Exact, Polite).is_ok();
		debug_assert!(ok, "`can_reduce_checked` must have returned `true` immediately prior; qed");
	}
}

impl<
		Fungible: fungible::Mutate<AccountId>,
		Matcher: MatchesFungible<Fungible::Balance>,
		AccountIdConverter: ConvertLocation<AccountId>,
		AccountId: Eq + Clone + Debug,
		CheckingAccount: Get<Option<(AccountId, MintLocation)>>,
	> TransactAsset
	for FungibleMutateAdapter<Fungible, Matcher, AccountIdConverter, AccountId, CheckingAccount>
{
	fn can_check_in(origin: &Location, what: &Asset, _context: &XcmContext) -> XcmResult {
		tracing::trace!(
			target: "xcm::fungible_adapter",
			?origin, ?what,
			"can_check_in origin",
		);
		// Check we handle this asset
		let amount = Matcher::matches_fungible(what).ok_or(MatchError::AssetNotHandled)?;
		match CheckingAccount::get() {
			Some((checking_account, MintLocation::Local)) =>
				Self::can_reduce_checked(checking_account, amount),
			Some((checking_account, MintLocation::NonLocal)) =>
				Self::can_accrue_checked(checking_account, amount),
			None => Ok(()),
		}
	}

	fn check_in(origin: &Location, what: &Asset, _context: &XcmContext) {
		tracing::trace!(
			target: "xcm::fungible_adapter",
			?origin, ?what,
			"check_in origin",
		);
		if let Some(amount) = Matcher::matches_fungible(what) {
			match CheckingAccount::get() {
				Some((checking_account, MintLocation::Local)) =>
					Self::reduce_checked(checking_account, amount),
				Some((checking_account, MintLocation::NonLocal)) =>
					Self::accrue_checked(checking_account, amount),
				None => (),
			}
		}
	}

	fn can_check_out(dest: &Location, what: &Asset, _context: &XcmContext) -> XcmResult {
		tracing::trace!(
			target: "xcm::fungible_adapter",
			?dest,
			?what,
			"can_check_out",
		);
		let amount = Matcher::matches_fungible(what).ok_or(MatchError::AssetNotHandled)?;
		match CheckingAccount::get() {
			Some((checking_account, MintLocation::Local)) =>
				Self::can_accrue_checked(checking_account, amount),
			Some((checking_account, MintLocation::NonLocal)) =>
				Self::can_reduce_checked(checking_account, amount),
			None => Ok(()),
		}
	}

	fn check_out(dest: &Location, what: &Asset, _context: &XcmContext) {
		tracing::trace!(
			target: "xcm::fungible_adapter",
			?dest,
			?what,
			"check_out",
		);
		if let Some(amount) = Matcher::matches_fungible(what) {
			match CheckingAccount::get() {
				Some((checking_account, MintLocation::Local)) =>
					Self::accrue_checked(checking_account, amount),
				Some((checking_account, MintLocation::NonLocal)) =>
					Self::reduce_checked(checking_account, amount),
				None => (),
			}
		}
	}

	fn deposit_asset(what: &Asset, who: &Location, _context: Option<&XcmContext>) -> XcmResult {
		tracing::trace!(
			target: "xcm::fungible_adapter",
			?what, ?who,
			"deposit_asset",
		);
		let amount = Matcher::matches_fungible(what).ok_or(MatchError::AssetNotHandled)?;
		let who = AccountIdConverter::convert_location(who)
			.ok_or(MatchError::AccountIdConversionFailed)?;
		Fungible::mint_into(&who, amount).map_err(|error| {
			tracing::debug!(
				target: "xcm::fungible_adapter", ?error, ?who, ?amount,
				"Failed to deposit assets",
			);
			XcmError::FailedToTransactAsset(error.into())
		})?;
		Ok(())
	}

	fn withdraw_asset(
		what: &Asset,
		who: &Location,
		_context: Option<&XcmContext>,
	) -> result::Result<AssetsInHolding, XcmError> {
		tracing::trace!(
			target: "xcm::fungible_adapter",
			?what, ?who,
			"withdraw_asset",
		);
		let amount = Matcher::matches_fungible(what).ok_or(MatchError::AssetNotHandled)?;
		let who = AccountIdConverter::convert_location(who)
			.ok_or(MatchError::AccountIdConversionFailed)?;
		Fungible::burn_from(&who, amount, Expendable, Exact, Polite).map_err(|error| {
			tracing::debug!(
				target: "xcm::fungible_adapter", ?error, ?who, ?amount,
				"Failed to withdraw assets",
			);
			XcmError::FailedToTransactAsset(error.into())
		})?;
		Ok(what.clone().into())
	}
}

/// [`TransactAsset`] implementation that allows the use of a [`fungible`] implementation for
/// handling an asset in the XCM executor.
/// Works for everything, transfers and teleport bookkeeping.
pub struct FungibleAdapter<Fungible, Matcher, AccountIdConverter, AccountId, CheckingAccount>(
	PhantomData<(Fungible, Matcher, AccountIdConverter, AccountId, CheckingAccount)>,
);
impl<
		Fungible: fungible::Mutate<AccountId>,
		Matcher: MatchesFungible<Fungible::Balance>,
		AccountIdConverter: ConvertLocation<AccountId>,
		AccountId: Eq + Clone + Debug,
		CheckingAccount: Get<Option<(AccountId, MintLocation)>>,
	> TransactAsset
	for FungibleAdapter<Fungible, Matcher, AccountIdConverter, AccountId, CheckingAccount>
{
	fn can_check_in(origin: &Location, what: &Asset, context: &XcmContext) -> XcmResult {
		FungibleMutateAdapter::<
			Fungible,
			Matcher,
			AccountIdConverter,
			AccountId,
			CheckingAccount,
		>::can_check_in(origin, what, context)
	}

	fn check_in(origin: &Location, what: &Asset, context: &XcmContext) {
		FungibleMutateAdapter::<
			Fungible,
			Matcher,
			AccountIdConverter,
			AccountId,
			CheckingAccount,
		>::check_in(origin, what, context)
	}

	fn can_check_out(dest: &Location, what: &Asset, context: &XcmContext) -> XcmResult {
		FungibleMutateAdapter::<
			Fungible,
			Matcher,
			AccountIdConverter,
			AccountId,
			CheckingAccount,
		>::can_check_out(dest, what, context)
	}

	fn check_out(dest: &Location, what: &Asset, context: &XcmContext) {
		FungibleMutateAdapter::<
			Fungible,
			Matcher,
			AccountIdConverter,
			AccountId,
			CheckingAccount,
		>::check_out(dest, what, context)
	}

	fn deposit_asset(what: &Asset, who: &Location, context: Option<&XcmContext>) -> XcmResult {
		FungibleMutateAdapter::<
			Fungible,
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
	) -> result::Result<AssetsInHolding, XcmError> {
		FungibleMutateAdapter::<
			Fungible,
			Matcher,
			AccountIdConverter,
			AccountId,
			CheckingAccount,
		>::withdraw_asset(what, who, maybe_context)
	}

	fn internal_transfer_asset(
		what: &Asset,
		from: &Location,
		to: &Location,
		context: &XcmContext,
	) -> result::Result<AssetsInHolding, XcmError> {
		FungibleTransferAdapter::<Fungible, Matcher, AccountIdConverter, AccountId>::internal_transfer_asset(
			what, from, to, context
		)
	}
}
