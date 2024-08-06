//! Utilities for redefining and auto-implementing the unique instances operations.

use core::marker::PhantomData;
use frame_support::traits::{
	tokens::asset_ops::{
		common_asset_kinds::Instance,
		common_strategies::{FromTo, IfOwnedBy, IfRestorable, Owned, PredefinedId},
		AssetDefinition, Create, CreateStrategy, Destroy, DestroyStrategy, Restore, Stash,
		Transfer, TransferStrategy,
	},
	TypedGet,
};
use sp_runtime::{DispatchError, DispatchResult};

/// The `UniqueInstancesOps` allows the creation of a new "NFT engine" capable of creating,
/// transferring, and destroying the unique instances by merging three distinct implementations.
///
/// The resulting "NFT engine" can be used in the
/// [`UniqueInstancesAdapter`](super::UniqueInstancesAdapter).
pub struct UniqueInstancesOps<CreateOp, TransferOp, DestroyOp>(
	PhantomData<(CreateOp, TransferOp, DestroyOp)>,
);
impl<CreateOp, TransferOp, DestroyOp> AssetDefinition<Instance>
	for UniqueInstancesOps<CreateOp, TransferOp, DestroyOp>
where
	TransferOp: AssetDefinition<Instance>,
	DestroyOp: AssetDefinition<Instance, Id = TransferOp::Id>,
{
	type Id = TransferOp::Id;
}
impl<Strategy, CreateOp, TransferOp, DestroyOp> Create<Instance, Strategy>
	for UniqueInstancesOps<CreateOp, TransferOp, DestroyOp>
where
	Strategy: CreateStrategy,
	CreateOp: Create<Instance, Strategy>,
{
	fn create(strategy: Strategy) -> Result<Strategy::Success, DispatchError> {
		CreateOp::create(strategy)
	}
}
impl<Strategy, CreateOp, TransferOp, DestroyOp> Transfer<Instance, Strategy>
	for UniqueInstancesOps<CreateOp, TransferOp, DestroyOp>
where
	Strategy: TransferStrategy,
	TransferOp: Transfer<Instance, Strategy>,
	DestroyOp: AssetDefinition<Instance, Id = TransferOp::Id>,
{
	fn transfer(id: &Self::Id, strategy: Strategy) -> Result<Strategy::Success, DispatchError> {
		TransferOp::transfer(id, strategy)
	}
}
impl<Strategy, CreateOp, TransferOp, DestroyOp> Destroy<Instance, Strategy>
	for UniqueInstancesOps<CreateOp, TransferOp, DestroyOp>
where
	Strategy: DestroyStrategy,
	TransferOp: AssetDefinition<Instance>,
	DestroyOp: AssetDefinition<Instance, Id = TransferOp::Id> + Destroy<Instance, Strategy>,
{
	fn destroy(id: &Self::Id, strategy: Strategy) -> Result<Strategy::Success, DispatchError> {
		DestroyOp::destroy(id, strategy)
	}
}

/// The `SimpleStash` implements both the [`Stash`] and [`Restore`] operations
/// by utilizing the [`Transfer`] operation.
/// Stashing with the [`IfOwnedBy`] strategy is implemented as the transfer to the stash account
/// using the [`FromTo`] strategy. Restoring with the [`IfRestorable`] is implemented symmetrically
/// as the transfer from the stash account using the [`FromTo`] strategy.
pub struct SimpleStash<StashAccount, InstanceOps>(PhantomData<(StashAccount, InstanceOps)>);
impl<StashAccount, InstanceOps> AssetDefinition<Instance> for SimpleStash<StashAccount, InstanceOps>
where
	InstanceOps: AssetDefinition<Instance>,
{
	type Id = InstanceOps::Id;
}
impl<StashAccount, InstanceOps> Stash<Instance, IfOwnedBy<StashAccount::Type>>
	for SimpleStash<StashAccount, InstanceOps>
where
	StashAccount: TypedGet,
	InstanceOps: Transfer<Instance, FromTo<StashAccount::Type>>,
{
	fn stash(
		id: &Self::Id,
		IfOwnedBy(possible_owner): IfOwnedBy<StashAccount::Type>,
	) -> DispatchResult {
		InstanceOps::transfer(id, FromTo(possible_owner, StashAccount::get()))
	}
}
impl<StashAccount, InstanceOps> Restore<Instance, IfRestorable<StashAccount::Type>>
	for SimpleStash<StashAccount, InstanceOps>
where
	StashAccount: TypedGet,
	InstanceOps: Transfer<Instance, FromTo<StashAccount::Type>>,
{
	fn restore(
		id: &Self::Id,
		IfRestorable(owner): IfRestorable<StashAccount::Type>,
	) -> DispatchResult {
		InstanceOps::transfer(id, FromTo(StashAccount::get(), owner))
	}
}

/// The `RestoreOnCreate` implements the [`Create`] operation by utilizing the [`Restore`]
/// operation. The creation is implemented using the [`Owned`] strategy with the [`PredefinedId`] ID
/// assignment. Such creation is modeled by the [`Restore`] operation using the [`IfRestorable`]
/// strategy.
///
/// The implemented [`Create`] operation can be used in the
/// [`UniqueInstancesAdapter`](super::UniqueInstancesAdapter) via the [`UniqueInstancesOps`].
pub struct RestoreOnCreate<InstanceOps>(PhantomData<InstanceOps>);
impl<InstanceOps> AssetDefinition<Instance> for RestoreOnCreate<InstanceOps>
where
	InstanceOps: AssetDefinition<Instance>,
{
	type Id = InstanceOps::Id;
}
impl<AccountId, InstanceOps> Create<Instance, Owned<AccountId, PredefinedId<InstanceOps::Id>>>
	for RestoreOnCreate<InstanceOps>
where
	InstanceOps: Restore<Instance, IfRestorable<AccountId>>,
{
	fn create(
		strategy: Owned<AccountId, PredefinedId<InstanceOps::Id>>,
	) -> Result<InstanceOps::Id, DispatchError> {
		let Owned { owner, id_assignment, .. } = strategy;
		let instance_id = id_assignment.params;

		InstanceOps::restore(&instance_id, IfRestorable(owner))?;

		Ok(instance_id)
	}
}

/// The `StashOnDestroy` implements the [`Destroy`] operation by utilizing the [`Stash`] operation.
/// The destroy operation is implemented using the [`IfOwnedBy`] strategy
/// and  modeled by the [`Stash`] operation using the same strategy.
///
/// The implemented [`Destroy`] operation can be used in the
/// [`UniqueInstancesAdapter`](super::UniqueInstancesAdapter) via the [`UniqueInstancesOps`].
pub struct StashOnDestroy<InstanceOps>(PhantomData<InstanceOps>);
impl<InstanceOps> AssetDefinition<Instance> for StashOnDestroy<InstanceOps>
where
	InstanceOps: AssetDefinition<Instance>,
{
	type Id = InstanceOps::Id;
}
impl<AccountId, InstanceOps> Destroy<Instance, IfOwnedBy<AccountId>> for StashOnDestroy<InstanceOps>
where
	InstanceOps: Stash<Instance, IfOwnedBy<AccountId>>,
{
	fn destroy(id: &Self::Id, strategy: IfOwnedBy<AccountId>) -> DispatchResult {
		InstanceOps::stash(id, strategy)
	}
}
