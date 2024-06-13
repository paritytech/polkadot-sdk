use core::marker::PhantomData;
use frame_support::traits::tokens::asset_ops::{
	self,
	common_asset_kinds::Instance,
	common_strategies::{AssignId, FromTo, IfOwnedBy, Owned},
	AssetDefinition, Create, Destroy, Transfer,
};
use xcm::latest::prelude::*;
use xcm_executor::traits::{ConvertLocation, Error as MatchError, MatchesInstance, TransactAsset};

const LOG_TARGET: &str = "xcm::unique_instances";

/// The `UniqueInstancesAdapter` implements the [`TransactAsset`] for unique instances (NFT-like
/// entities), for which the `Matcher` can deduce the instance ID from the XCM [`AssetId`].
///
/// The adapter uses the following asset operations:
/// * [`Create`] with the [`Owned`] strategy, which uses the [`AssignId`] approach
/// to assign the instance ID deduced by the `Matcher`.
/// * [`Transfer`] with [`FromTo`] strategy
/// * [`Destroy`] with [`IfOwnedBy`] strategy
///
/// This adapter assumes that the asset can be safely destroyed
/// without destroying any important data.
/// However, the "destroy" operation can be replaced by another operation.
/// For instance, one can use the [`StashOnDestroy`](super::ops::StashOnDestroy) type to stash the
/// instance instead of destroying it. See other similar types in the [`ops`](super::ops) module.
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
	AccountIdConverter: ConvertLocation<AccountId>,
	Matcher: MatchesInstance<InstanceOps::Id>,
	InstanceOps: AssetDefinition<Instance>
		+ Create<Instance, Owned<AccountId, AssignId<InstanceOps::Id>>>
		+ Transfer<Instance, FromTo<AccountId>>
		+ Destroy<Instance, IfOwnedBy<AccountId>>,
{
	fn deposit_asset(what: &Asset, who: &Location, context: Option<&XcmContext>) -> XcmResult {
		log::trace!(
			target: LOG_TARGET,
			"UniqueInstancesAdapter::deposit_asset what: {:?}, who: {:?}, context: {:?}",
			what,
			who,
			context,
		);

		let instance_id = Matcher::matches_instance(what)?;
		let who = AccountIdConverter::convert_location(who)
			.ok_or(MatchError::AccountIdConversionFailed)?;

		InstanceOps::create(Owned::new(who, AssignId(instance_id)))
			.map_err(|e| XcmError::FailedToTransactAsset(e.into()))
	}

	fn withdraw_asset(
		what: &Asset,
		who: &Location,
		maybe_context: Option<&XcmContext>,
	) -> Result<xcm_executor::AssetsInHolding, XcmError> {
		log::trace!(
			target: LOG_TARGET,
			"UniqueInstancesAdapter::withdraw_asset what: {:?}, who: {:?}, context: {:?}",
			what,
			who,
			maybe_context,
		);
		let instance_id = Matcher::matches_instance(what)?;
		let who = AccountIdConverter::convert_location(who)
			.ok_or(MatchError::AccountIdConversionFailed)?;

		InstanceOps::destroy(&instance_id, IfOwnedBy(who))
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
			"UniqueInstancesAdapter::internal_transfer_asset what: {:?}, from: {:?}, to: {:?}, context: {:?}",
			what,
			from,
			to,
			context,
		);

		let instance_id = Matcher::matches_instance(what)?;
		let from = AccountIdConverter::convert_location(from)
			.ok_or(MatchError::AccountIdConversionFailed)?;
		let to = AccountIdConverter::convert_location(to)
			.ok_or(MatchError::AccountIdConversionFailed)?;

		InstanceOps::transfer(&instance_id, FromTo(from, to))
			.map_err(|e| XcmError::FailedToTransactAsset(e.into()))?;

		Ok(what.clone().into())
	}
}

/// The `UniqueDerivedInstancesAdapter` implements the [`TransactAsset`] to create unique instances
/// (NFT-like entities), for which the `Matcher` can **not** deduce the instance ID from the XCM
/// [`AssetId`]. Instead, this adapter requires the `Matcher` to return
/// the [instance ID assignment approach](asset_ops::IdAssignment)
/// so a new instance can be created using this approach and then deposited to a beneficiary.
pub struct UniqueDerivedInstancesAdapter<
	AccountId,
	AccountIdConverter,
	IdAssignment,
	Matcher,
	InstanceCreateOp,
>(PhantomData<(AccountId, AccountIdConverter, IdAssignment, Matcher, InstanceCreateOp)>);

impl<AccountId, AccountIdConverter, Matcher, IdAssignment, InstanceCreateOp> TransactAsset
	for UniqueDerivedInstancesAdapter<
		AccountId,
		AccountIdConverter,
		IdAssignment,
		Matcher,
		InstanceCreateOp,
	> where
	AccountIdConverter: ConvertLocation<AccountId>,
	IdAssignment: asset_ops::IdAssignment,
	Matcher: MatchesInstance<IdAssignment>,
	InstanceCreateOp: Create<Instance, Owned<AccountId, IdAssignment>>,
{
	fn deposit_asset(what: &Asset, who: &Location, context: Option<&XcmContext>) -> XcmResult {
		log::trace!(
			target: LOG_TARGET,
			"UniqueDerivedInstancesAdapter::deposit_asset what: {:?}, who: {:?}, context: {:?}",
			what,
			who,
			context,
		);

		let instance_id_assignment = Matcher::matches_instance(what)?;
		let who = AccountIdConverter::convert_location(who)
			.ok_or(MatchError::AccountIdConversionFailed)?;

		InstanceCreateOp::create(Owned::new(who, instance_id_assignment))
			.map(|_reported_id| ())
			.map_err(|e| XcmError::FailedToTransactAsset(e.into()))
	}
}
