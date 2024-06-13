use core::marker::PhantomData;

use frame_support::traits::{
	tokens::asset_ops::{
		common_asset_kinds::{Class, Instance},
		common_strategies::{
			AssignId, AutoId, DeriveAndReportId, FromTo, IfOwnedBy, IfRestorable, Owned,
		},
		AssetDefinition, Create, CreateStrategy, Destroy, DestroyStrategy, Restore, Stash,
		Transfer, TransferStrategy,
	},
	TypedGet,
};
use sp_runtime::{DispatchError, DispatchResult};
use xcm::latest::prelude::*;

use super::NonFungibleAsset;

pub trait TryRegisterDerivative<InstanceId> {
	fn try_register_derivative(
		foreign_asset: &NonFungibleAsset,
		instance_id: &InstanceId,
	) -> DispatchResult;

	fn is_derivative_registered(foreign_asset: &NonFungibleAsset) -> bool;
}

pub trait TryDeregisterDerivative<InstanceId> {
	fn try_deregister_derivative(instance_id: &InstanceId) -> DispatchResult;

	fn is_derivative(instance_id: &InstanceId) -> bool;
}

pub struct RegisterDerivativeId<InstanceIdSource> {
	pub foreign_asset: NonFungibleAsset,
	pub instance_id_source: InstanceIdSource,
}

pub struct RegisterOnCreate<Registrar, InstanceOps>(PhantomData<(Registrar, InstanceOps)>);
impl<AccountId, InstanceIdSource, Registrar, InstanceOps>
	Create<Instance, Owned<AccountId, AssignId<RegisterDerivativeId<InstanceIdSource>>>>
	for RegisterOnCreate<Registrar, InstanceOps>
where
	Registrar: TryRegisterDerivative<InstanceOps::Id>,
	InstanceOps: AssetDefinition<Instance>
		+ Create<Instance, Owned<AccountId, DeriveAndReportId<InstanceIdSource, InstanceOps::Id>>>,
{
	fn create(
		strategy: Owned<AccountId, AssignId<RegisterDerivativeId<InstanceIdSource>>>,
	) -> DispatchResult {
		let Owned {
			owner,
			id_assignment: AssignId(RegisterDerivativeId { foreign_asset, instance_id_source }),
			..
		} = strategy;

		if Registrar::is_derivative_registered(&foreign_asset) {
			return Err(DispatchError::Other(
				"an attempt to register a duplicate of an existing derivative instance",
			));
		}

		let instance_id =
			InstanceOps::create(Owned::new(owner, DeriveAndReportId::from(instance_id_source)))?;

		Registrar::try_register_derivative(&foreign_asset, &instance_id)
	}
}

pub struct DeregisterOnDestroy<Registrar, InstanceOps>(PhantomData<(Registrar, InstanceOps)>);
impl<Registrar, InstanceOps> AssetDefinition<Instance>
	for DeregisterOnDestroy<Registrar, InstanceOps>
where
	InstanceOps: AssetDefinition<Instance>,
{
	type Id = InstanceOps::Id;
}
impl<AccountId, Registrar, InstanceOps> Destroy<Instance, IfOwnedBy<AccountId>>
	for DeregisterOnDestroy<Registrar, InstanceOps>
where
	Registrar: TryDeregisterDerivative<InstanceOps::Id>,
	InstanceOps: Destroy<Instance, IfOwnedBy<AccountId>>,
{
	fn destroy(id: &Self::Id, strategy: IfOwnedBy<AccountId>) -> DispatchResult {
		if !Registrar::is_derivative(id) {
			return Err(DispatchError::Other(
				"an attempt to deregister an instance that isn't a derivative",
			));
		}

		InstanceOps::destroy(id, strategy)?;

		Registrar::try_deregister_derivative(id)
	}
}
