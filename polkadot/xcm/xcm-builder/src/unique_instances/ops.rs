//! Utilities for redefining and auto-implementing the unique instances operations.

use core::marker::PhantomData;
use frame_support::traits::tokens::asset_ops::{
	AssetDefinition, Restore, RestoreStrategy, Stash, StashStrategy, Update, UpdateStrategy,
};
use sp_runtime::DispatchError;

/// The `UniqueInstancesOps` is a tool for combining
/// different implementations of `Restore`, `Update`, and `Stash` operations
/// into one type to be used in [`UniqueInstancesAdapter`](super::adapters::UniqueInstancesAdapter).
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
