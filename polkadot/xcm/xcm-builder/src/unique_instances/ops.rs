//! Utilities for redefining and auto-implementing the unique instances operations.

use core::marker::PhantomData;
use frame_support::traits::tokens::asset_ops::{
	common_strategies::{ChangeOwnerFrom, Owner, WithConfig, ConfigValue, CheckState, IfOwnedBy, DeriveAndReportId, AutoId},
	AssetDefinition, Restore, RestoreStrategy, Stash, StashStrategy, Update, UpdateStrategy,
};
use sp_runtime::{traits::{Convert, TypedGet, FallibleConvert, parameter_types}, DispatchError, DispatchResult};

use super::NonFungibleAsset;
use xcm::latest::prelude::*;
use xcm_executor::traits::ConvertLocation;

/// The `UniqueInstancesOps` is a tool for combining
/// different implementations of `Restore`, `Update`, and `Stash` operations
/// into one type to be used in [`UniqueInstancesAdapter`](super::adapter::UniqueInstancesAdapter).
///
/// All three operations must use the same ID for instances.
pub struct UniqueInstancesOps<RestoreOp, UpdateOp, StashOp>(
	PhantomData<(RestoreOp, UpdateOp, StashOp)>,
);
impl<RestoreOp, UpdateOp, StashOp> AssetDefinition
	for UniqueInstancesOps<RestoreOp, UpdateOp, StashOp>
where
	RestoreOp: AssetDefinition,
	UpdateOp: AssetDefinition<Id = RestoreOp::Id>,
	StashOp: AssetDefinition<Id = RestoreOp::Id>,
{
	type Id = RestoreOp::Id;
}
impl<Strategy, RestoreOp, UpdateOp, StashOp> Restore<Strategy>
	for UniqueInstancesOps<RestoreOp, UpdateOp, StashOp>
where
	Strategy: RestoreStrategy,
	RestoreOp: Restore<Strategy>,
	UpdateOp: AssetDefinition<Id = RestoreOp::Id>,
	StashOp: AssetDefinition<Id = RestoreOp::Id>,
{
	fn restore(id: &Self::Id, strategy: Strategy) -> Result<Strategy::Success, DispatchError> {
		RestoreOp::restore(id, strategy)
	}
}
impl<Strategy, RestoreOp, UpdateOp, StashOp> Update<Strategy>
	for UniqueInstancesOps<RestoreOp, UpdateOp, StashOp>
where
	Strategy: UpdateStrategy,
	UpdateOp: Update<Strategy>,
	RestoreOp: AssetDefinition,
	UpdateOp: AssetDefinition<Id = RestoreOp::Id>,
	StashOp: AssetDefinition<Id = RestoreOp::Id>,
{
	fn update(
		id: &Self::Id,
		strategy: Strategy,
		update: Strategy::UpdateValue<'_>,
	) -> Result<Strategy::Success, DispatchError> {
		UpdateOp::update(id, strategy, update)
	}
}
impl<Strategy, RestoreOp, UpdateOp, StashOp> Stash<Strategy>
	for UniqueInstancesOps<RestoreOp, UpdateOp, StashOp>
where
	Strategy: StashStrategy,
	StashOp: Stash<Strategy>,
	RestoreOp: AssetDefinition,
	UpdateOp: AssetDefinition<Id = RestoreOp::Id>,
	StashOp: AssetDefinition<Id = RestoreOp::Id>,
{
	fn stash(id: &Self::Id, strategy: Strategy) -> Result<Strategy::Success, DispatchError> {
		StashOp::stash(id, strategy)
	}
}

/// The `UniqueInstancesWithStashAccount` adds the `Stash` and `Restore` implementations to an NFT engine
/// capable of transferring a token from one account to another (i.e. implementing `Update<ChangeOwnerFrom<AccountId>>`).
///
/// On stash, it will transfer the token from the current owner to the `StashAccount`.
/// On restore, it will transfer the token from the `StashAccount` to the given beneficiary.
pub struct UniqueInstancesWithStashAccount<StashAccount, UpdateOp>(PhantomData<(StashAccount, UpdateOp)>);
impl<StashAccount, UpdateOp: AssetDefinition> AssetDefinition for UniqueInstancesWithStashAccount<StashAccount, UpdateOp> {
	type Id = UpdateOp::Id;
}
impl<StashAccount: TypedGet, UpdateOp> Update<ChangeOwnerFrom<StashAccount::Type>> for UniqueInstancesWithStashAccount<StashAccount, UpdateOp>
where
	StashAccount::Type: 'static,
	UpdateOp: Update<ChangeOwnerFrom<StashAccount::Type>>,
{
	fn update(
		id: &Self::Id,
		strategy: ChangeOwnerFrom<StashAccount::Type>,
		update: &StashAccount::Type,
	) -> DispatchResult {
		UpdateOp::update(id, strategy, update)
	}
}
impl<StashAccount, UpdateOp> Restore<WithConfig<ConfigValue<Owner<StashAccount::Type>>>> for UniqueInstancesWithStashAccount<StashAccount, UpdateOp>
where
	StashAccount: TypedGet,
	StashAccount::Type: 'static,
	UpdateOp: Update<ChangeOwnerFrom<StashAccount::Type>>,
{
	fn restore(
		id: &Self::Id,
		strategy: WithConfig<ConfigValue<Owner<StashAccount::Type>>>,
	) -> DispatchResult {
		let WithConfig { config: ConfigValue(beneficiary), .. } = strategy;

		UpdateOp::update(id, ChangeOwnerFrom::check(StashAccount::get()), &beneficiary)
	}
}
impl<StashAccount, UpdateOp> Stash<IfOwnedBy<StashAccount::Type>> for UniqueInstancesWithStashAccount<StashAccount, UpdateOp>
where
	StashAccount: TypedGet,
	StashAccount::Type: 'static,
	UpdateOp: Update<ChangeOwnerFrom<StashAccount::Type>>,
{
	fn stash(
		id: &Self::Id,
		strategy: IfOwnedBy<StashAccount::Type>,
	) -> DispatchResult {
		let CheckState(check_owner, ..) = strategy;

		UpdateOp::update(id, ChangeOwnerFrom::check(check_owner), &StashAccount::get())
	}
}

/// Gets the XCM [AssetId] (i.e., extracts the NFT collection ID) from the [NonFungibleAsset].
pub struct ExtractAssetId;
impl Convert<NonFungibleAsset, AssetId> for ExtractAssetId {
	fn convert((asset_id, _): NonFungibleAsset) -> AssetId {
		asset_id
	}
}

parameter_types! {
	pub OwnerConvertedLocationDefaultErr: DispatchError = DispatchError::Other("OwnerConvertedLocation: failed to convert the location");
}

/// Converts a given `AssetId` to a `WithConfig` strategy with the owner account set to the asset's location converted to an account ID.
pub struct OwnerConvertedLocation<CL, IdAssignment, Err = OwnerConvertedLocationDefaultErr>(PhantomData<(CL, IdAssignment, Err)>);
impl<AccountId, CL, Err, ReportedId> FallibleConvert<
	AssetId,
	WithConfig<ConfigValue<Owner<AccountId>>, DeriveAndReportId<AssetId, ReportedId>>,
> for OwnerConvertedLocation<CL, DeriveAndReportId<AssetId, ReportedId>, Err>
where
	CL: ConvertLocation<AccountId>,
	Err: TypedGet,
	Err::Type: Into<DispatchError>,
{
	fn fallible_convert(AssetId(location): AssetId) -> Result<WithConfig<ConfigValue<Owner<AccountId>>, DeriveAndReportId<AssetId, ReportedId>>, DispatchError> {
		CL::convert_location(&location)
			.map(|account| WithConfig::new(ConfigValue(account), DeriveAndReportId::from(AssetId(location))))
			.ok_or(Err::get().into())
	}
}
impl<AccountId, CL, Err, ReportedId> FallibleConvert<
	AssetId,
	WithConfig<ConfigValue<Owner<AccountId>>, AutoId<ReportedId>>,
> for OwnerConvertedLocation<CL, AutoId<ReportedId>, Err>
where
	CL: ConvertLocation<AccountId>,
	Err: TypedGet,
	Err::Type: Into<DispatchError>,
{
	fn fallible_convert(AssetId(location): AssetId) -> Result<WithConfig<ConfigValue<Owner<AccountId>>, AutoId<ReportedId>>, DispatchError> {
		CL::convert_location(&location)
			.map(|account| WithConfig::new(ConfigValue(account), AutoId::auto()))
			.ok_or(Err::get().into())
	}
}
