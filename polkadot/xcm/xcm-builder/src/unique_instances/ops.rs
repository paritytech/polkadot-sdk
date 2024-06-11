use core::marker::PhantomData;
use frame_support::traits::{
	tokens::asset_ops::{
		common_asset_kinds::Instance,
		common_strategies::{AssignId, DeriveAndReportId, FromTo, IfOwnedBy, IfRestorable, Owned},
		AssetDefinition, Create, CreateStrategy, Destroy, DestroyStrategy, Restore, Stash,
		Transfer, TransferStrategy,
	},
	TypedGet,
};
use sp_runtime::{DispatchError, DispatchResult};

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
	fn transfer(id: &Self::Id, strategy: Strategy) -> DispatchResult {
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

pub struct SimpleStash<StashAccount, InstanceOps>(PhantomData<(StashAccount, InstanceOps)>);
impl<StashAccount, InstanceOps> AssetDefinition<Instance> for SimpleStash<StashAccount, InstanceOps>
where
	InstanceOps: AssetDefinition<Instance>,
{
	type Id = InstanceOps::Id;
}
impl<'a, StashAccount, InstanceOps> Stash<Instance, IfOwnedBy<'a, StashAccount::Type>>
	for SimpleStash<StashAccount, InstanceOps>
where
	StashAccount: TypedGet,
	InstanceOps: for<'b> Transfer<Instance, FromTo<'b, StashAccount::Type>>,
{
	fn stash(
		id: &Self::Id,
		IfOwnedBy(possible_owner): IfOwnedBy<StashAccount::Type>,
	) -> DispatchResult {
		InstanceOps::transfer(id, FromTo(possible_owner, &StashAccount::get()))
	}
}
impl<'a, StashAccount, InstanceOps> Restore<Instance, IfRestorable<'a, StashAccount::Type>>
	for SimpleStash<StashAccount, InstanceOps>
where
	StashAccount: TypedGet,
	InstanceOps: for<'b> Transfer<Instance, FromTo<'b, StashAccount::Type>>,
{
	fn restore(
		id: &Self::Id,
		IfRestorable(owner): IfRestorable<StashAccount::Type>,
	) -> DispatchResult {
		InstanceOps::transfer(id, FromTo(&StashAccount::get(), owner))
	}
}

pub struct RestoreOnCreate<InstanceOps>(PhantomData<InstanceOps>);
impl<'a, AccountId, InstanceOps>
	Create<Instance, Owned<'a, AccountId, AssignId<'a, InstanceOps::Id>>>
	for RestoreOnCreate<InstanceOps>
where
	InstanceOps: for<'b> Restore<Instance, IfRestorable<'b, AccountId>>,
{
	fn create(strategy: Owned<AccountId, AssignId<InstanceOps::Id>>) -> DispatchResult {
		let Owned { owner, id_assignment: AssignId(instance_id), .. } = strategy;

		InstanceOps::restore(instance_id, IfRestorable(owner))
	}
}

pub struct StashOnDestroy<InstanceOps>(PhantomData<InstanceOps>);
impl<InstanceOps> AssetDefinition<Instance> for StashOnDestroy<InstanceOps>
where
	InstanceOps: AssetDefinition<Instance>,
{
	type Id = InstanceOps::Id;
}
impl<'a, AccountId, InstanceOps> Destroy<Instance, IfOwnedBy<'a, AccountId>>
	for StashOnDestroy<InstanceOps>
where
	InstanceOps: for<'b> Stash<Instance, IfOwnedBy<'b, AccountId>>,
{
	fn destroy(id: &Self::Id, strategy: IfOwnedBy<'a, AccountId>) -> DispatchResult {
		InstanceOps::stash(id, strategy)
	}
}
