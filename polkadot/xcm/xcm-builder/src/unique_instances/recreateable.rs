use super::{transfer_instance, LOG_TARGET};
use core::marker::PhantomData;
use frame_support::traits::tokens::asset_ops::{
	common_asset_kinds::Instance,
	common_strategies::{FromTo, IfOwnedBy, Owned, PredefinedId},
	AssetDefinition, Create, Destroy, Transfer,
};
use xcm::latest::prelude::*;
use xcm_executor::traits::{ConvertLocation, Error as MatchError, MatchesInstance, TransactAsset};

/// The `RecreateableInstanceAdapter` implements the `TransactAsset` for unique instances (NFT-like
/// entities).
/// The adapter uses the following asset operations:
/// * [`Create`] with the [`Owned`] strategy that uses the [`PredefinedId`].
/// The `Id` used is the one from the [asset's definition](AssetDefinition).
/// * [`Transfer`] with [`FromTo`] strategy
/// * [`Destroy`] with [`IfOwnedBy`] strategy
///
/// This adapter assumes that the asset can be safely destroyed
/// without the loss of any important data. It will destroy the asset on withdrawal.
/// Similarly, it assumes that the asset can be recreated with the same ID on deposit.
///
/// Transfers work without additional assumptions.
///
/// If the above assumptions are true,
/// this adapter can be used both to work with the original instances in a reserve location
/// and to work with derivatives in other locations.
///
/// Only the assets matched by `Matcher` are affected.
/// If the `Matcher` recognizes the instance, it should return its `Id`.
///
/// Note on teleports:
/// This adapter doesn't implement teleports at the moment since unique instances have associated
/// data that should be teleported along.
/// Currently, neither XCM has the ability to transfer such data
/// nor a standard approach exists in the ecosystem for this use case.
pub struct RecreateableInstanceAdapter<AccountId, AccountIdConverter, Matcher, InstanceOps>(
	PhantomData<(AccountId, AccountIdConverter, Matcher, InstanceOps)>,
);

impl<AccountId, AccountIdConverter, Matcher, InstanceOps> TransactAsset
	for RecreateableInstanceAdapter<AccountId, AccountIdConverter, Matcher, InstanceOps>
where
	AccountIdConverter: ConvertLocation<AccountId>,
	Matcher: MatchesInstance<InstanceOps::Id>,
	for<'a> InstanceOps: AssetDefinition<Instance>
		+ Create<Instance, Owned<'a, PredefinedId<'a, InstanceOps::Id>, AccountId>>
		+ Transfer<Instance, FromTo<'a, AccountId>>
		+ Destroy<Instance, IfOwnedBy<'a, AccountId>>,
{
	fn deposit_asset(what: &Asset, who: &Location, context: Option<&XcmContext>) -> XcmResult {
		log::trace!(
			target: LOG_TARGET,
			"RecreateableInstanceAdapter::deposit_asset what: {:?}, who: {:?}, context: {:?}",
			what,
			who,
			context,
		);

		let instance_id = Matcher::matches_instance(what)?;
		let who = AccountIdConverter::convert_location(who)
			.ok_or(MatchError::AccountIdConversionFailed)?;

		InstanceOps::create(Owned::new(PredefinedId(&instance_id), &who))
			.map_err(|e| XcmError::FailedToTransactAsset(e.into()))
	}

	fn withdraw_asset(
		what: &Asset,
		who: &Location,
		maybe_context: Option<&XcmContext>,
	) -> Result<xcm_executor::AssetsInHolding, XcmError> {
		log::trace!(
			target: LOG_TARGET,
			"RecreateableInstanceAdapter::withdraw_asset what: {:?}, who: {:?}, context: {:?}",
			what,
			who,
			maybe_context,
		);
		let instance_id = Matcher::matches_instance(what)?;
		let who = AccountIdConverter::convert_location(who)
			.ok_or(MatchError::AccountIdConversionFailed)?;

		InstanceOps::destroy(&instance_id, IfOwnedBy(&who))
			.map_err(|e| XcmError::FailedToTransactAsset(e.into()))?;

		Ok(what.clone().into())
	}

	fn internal_transfer_asset(
		what: &Asset,
		from: &Location,
		to: &Location,
		context: &XcmContext,
	) -> Result<xcm_executor::AssetsInHolding, XcmError> {
		log::trace!(
			target: LOG_TARGET,
			"RecreateableInstanceAdapter::internal_transfer_asset what: {:?}, from: {:?}, to: {:?}, context: {:?}",
			what,
			from,
			to,
			context,
		);

		transfer_instance::<AccountId, AccountIdConverter, Matcher, InstanceOps>(what, from, to)?;

		Ok(what.clone().into())
	}
}
