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

use core::marker::PhantomData;
use frame_support::traits::tokens::asset_ops::{
	common_strategies::{
		ChangeOwnerFrom, ConfigValue, DeriveAndReportId, IfOwnedBy, Owner, WithConfig,
		WithConfigValue,
	},
	AssetDefinition, Create, Restore, Stash, Update,
};
use xcm::latest::prelude::*;
use xcm_executor::traits::{ConvertLocation, Error as MatchError, MatchesInstance, TransactAsset};

use super::NonFungibleAsset;

const LOG_TARGET: &str = "xcm::unique_instances";

/// The `UniqueInstancesAdapter` implements the [`TransactAsset`] for existing unique instances
/// (NFT-like entities), for which the `Matcher` can deduce the instance ID from the XCM
/// [`AssetId`].
///
/// The adapter uses the following asset operations:
/// * [`Restore`] with the strategy to restore the instance to a given owner.
/// * [`Update`] with the strategy to change the instance's owner from one to another.
/// * [`Stash`] with the strategy to check the current owner before stashing.
///
/// Note on teleports: This adapter doesn't implement teleports since unique instances have
/// associated data that also should be teleported. Currently, neither XCM can transfer such data
/// nor does a standard approach exist in the ecosystem for this use case.
pub struct UniqueInstancesAdapter<AccountId, AccountIdConverter, Matcher, InstanceOps>(
	PhantomData<(AccountId, AccountIdConverter, Matcher, InstanceOps)>,
);

impl<AccountId, AccountIdConverter, Matcher, InstanceOps> TransactAsset
	for UniqueInstancesAdapter<AccountId, AccountIdConverter, Matcher, InstanceOps>
where
	AccountId: 'static,
	AccountIdConverter: ConvertLocation<AccountId>,
	Matcher: MatchesInstance<InstanceOps::Id>,
	InstanceOps: AssetDefinition
		+ Restore<WithConfig<ConfigValue<Owner<AccountId>>>>
		+ Update<ChangeOwnerFrom<AccountId>>
		+ Stash<IfOwnedBy<AccountId>>,
{
	fn deposit_asset(what: &Asset, who: &Location, context: Option<&XcmContext>) -> XcmResult {
		tracing::trace!(
			target: LOG_TARGET,
			?what,
			?who,
			?context,
			"deposit_asset",
		);

		let instance_id = Matcher::matches_instance(what)?;
		let who = AccountIdConverter::convert_location(who)
			.ok_or(MatchError::AccountIdConversionFailed)?;

		InstanceOps::restore(&instance_id, WithConfig::from(Owner::with_config_value(who)))
			.map_err(|e| XcmError::FailedToTransactAsset(e.into()))
	}

	fn withdraw_asset(
		what: &Asset,
		who: &Location,
		maybe_context: Option<&XcmContext>,
	) -> Result<xcm_executor::AssetsInHolding, XcmError> {
		tracing::trace!(
			target: LOG_TARGET,
			?what,
			?who,
			?maybe_context,
			"withdraw_asset",
		);

		let instance_id = Matcher::matches_instance(what)?;
		let who = AccountIdConverter::convert_location(who)
			.ok_or(MatchError::AccountIdConversionFailed)?;

		InstanceOps::stash(&instance_id, IfOwnedBy::check(who))
			.map_err(|e| XcmError::FailedToTransactAsset(e.into()))?;

		Ok(what.clone().into())
	}

	fn internal_transfer_asset(
		what: &Asset,
		from: &Location,
		to: &Location,
		context: &XcmContext,
	) -> Result<xcm_executor::AssetsInHolding, XcmError> {
		tracing::trace!(
			target: LOG_TARGET,
			?what,
			?from,
			?to,
			?context,
			"internal_transfer_asset",
		);

		let instance_id = Matcher::matches_instance(what)?;
		let from = AccountIdConverter::convert_location(from)
			.ok_or(MatchError::AccountIdConversionFailed)?;
		let to = AccountIdConverter::convert_location(to)
			.ok_or(MatchError::AccountIdConversionFailed)?;

		InstanceOps::update(&instance_id, ChangeOwnerFrom::check(from), &to)
			.map_err(|e| XcmError::FailedToTransactAsset(e.into()))?;

		Ok(what.clone().into())
	}
}

/// The `UniqueInstancesDepositAdapter` implements the [`TransactAsset`] to create unique instances
/// (NFT-like entities), for which no `Matcher` can deduce the instance ID from the XCM
/// [`AssetId`]. Instead, this adapter requires the `InstanceCreateOp` to create an instance using
/// [`NonFungibleAsset`] as ID derivation parameter.
pub struct UniqueInstancesDepositAdapter<AccountId, AccountIdConverter, Id, InstanceCreateOp>(
	PhantomData<(AccountId, AccountIdConverter, Id, InstanceCreateOp)>,
);

impl<AccountId, AccountIdConverter, Id, InstanceCreateOp> TransactAsset
	for UniqueInstancesDepositAdapter<AccountId, AccountIdConverter, Id, InstanceCreateOp>
where
	AccountIdConverter: ConvertLocation<AccountId>,
	InstanceCreateOp:
		Create<WithConfig<ConfigValue<Owner<AccountId>>, DeriveAndReportId<NonFungibleAsset, Id>>>,
{
	fn deposit_asset(what: &Asset, who: &Location, context: Option<&XcmContext>) -> XcmResult {
		tracing::trace!(
			target: LOG_TARGET,
			?what,
			?who,
			?context,
			"deposit_asset",
		);

		let asset = match what.fun {
			Fungibility::NonFungible(asset_instance) => (what.id.clone(), asset_instance),
			_ => return Err(MatchError::AssetNotHandled.into()),
		};

		let who = AccountIdConverter::convert_location(who)
			.ok_or(MatchError::AccountIdConversionFailed)?;

		InstanceCreateOp::create(WithConfig::new(
			Owner::with_config_value(who),
			DeriveAndReportId::from(asset),
		))
		.map(|_reported_id| ())
		.map_err(|e| XcmError::FailedToTransactAsset(e.into()))
	}
}
