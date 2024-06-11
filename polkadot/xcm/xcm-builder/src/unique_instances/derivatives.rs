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
impl<'a, AccountId, InstanceIdSource, Registrar, InstanceOps>
	Create<Instance, Owned<'a, AccountId, AssignId<'a, RegisterDerivativeId<InstanceIdSource>>>>
	for RegisterOnCreate<Registrar, InstanceOps>
where
	Registrar: TryRegisterDerivative<InstanceOps::Id>,
	InstanceOps: AssetDefinition<Instance>
		+ for<'b> Create<
			Instance,
			Owned<'b, AccountId, DeriveAndReportId<'b, InstanceIdSource, InstanceOps::Id>>,
		>,
{
	fn create(
		strategy: Owned<AccountId, AssignId<RegisterDerivativeId<InstanceIdSource>>>,
	) -> DispatchResult {
		let Owned {
			owner,
			id_assignment: AssignId(RegisterDerivativeId { foreign_asset, instance_id_source }),
			..
		} = strategy;

		if Registrar::is_derivative_registered(foreign_asset) {
			return Err(DispatchError::Other(
				"an attempt to register a duplicate of an existing derivative instance",
			));
		}

		let instance_id =
			InstanceOps::create(Owned::new(owner, DeriveAndReportId::from(instance_id_source)))?;

		Registrar::try_register_derivative(foreign_asset, &instance_id)
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
impl<'a, AccountId, Registrar, InstanceOps> Destroy<Instance, IfOwnedBy<'a, AccountId>>
	for DeregisterOnDestroy<Registrar, InstanceOps>
where
	Registrar: TryDeregisterDerivative<InstanceOps::Id>,
	InstanceOps: for<'b> Destroy<Instance, IfOwnedBy<'b, AccountId>>,
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

// pub enum DerivativeInClassStatus<ClassId, InstanceId> {
// 	Registrable { foreign_asset: NonFungibleAsset, in_class: ClassId },

// 	Restorable(InstanceId),
// }

// pub enum ClasslessDerivativeStatus<InstanceId> {
// 	Registrable(NonFungibleAsset),

// 	Restorable(InstanceId),
// }

// pub struct RegisterOnCreate<Registrar, CreateOp, RestoreOp>(PhantomData<(Registrar, CreateOp,
// RestoreOp)>); impl<'a, AccountId, ClassId, Registrar, CreateOp, RestoreOp>
//     Create<Instance, Owned<'a, AccountId, AssignId<'a, DerivativeInClassStatus<ClassId,
// CreateOp::Id>>>>     for RegisterOnCreate<Registrar, CreateOp, RestoreOp>
// where
//     Registrar: RegisterDerivative<CreateOp::Id>,
//     CreateOp: AssetDefinition<Instance> + for<'b> Create<Instance, Owned<'b, AccountId,
// DeriveAndReportId<'b, ClassId, CreateOp::Id>>>,     RestoreOp: AssetDefinition<Instance, Id =
// CreateOp::Id> + for<'c> Restore<Instance, IfRestorable<'c, AccountId>>, {
//     fn create(strategy: Owned<AccountId, AssignId<DerivativeInClassStatus<ClassId,
// CreateOp::Id>>>) -> DispatchResult {         let Owned { owner, id_assignment:
// AssignId(status), .. } = strategy;

// 		match status {
// 			DerivativeInClassStatus::Registrable { foreign_asset, in_class } => {
// 				let instance_id =
// 					CreateOp::create(Owned::new(owner, DeriveAndReportId::from(in_class)))?;

// 				Registrar::register_derivative(foreign_asset, &instance_id)?;
// 			},
// 			DerivativeInClassStatus::Restorable(instance_id) =>
// 				RestoreOp::restore(instance_id, IfRestorable(owner))?,
// 		}

//         Ok(())
//     }
// }
// impl<'a, AccountId, Registrar, CreateOp, RestoreOp>
//     Create<Instance, Owned<'a, AccountId, AssignId<'a,
// ClasslessDerivativeStatus<CreateOp::Id>>>>     for RegisterOnCreate<Registrar, CreateOp,
// RestoreOp> where
//     Registrar: RegisterDerivative<CreateOp::Id>,
//     CreateOp: AssetDefinition<Instance> + for<'b> Create<Instance, Owned<'b, AccountId,
// AutoId<CreateOp::Id>>>,     RestoreOp: AssetDefinition<Instance, Id = CreateOp::Id> + for<'c>
// Restore<Instance, IfRestorable<'c, AccountId>>, {
//     fn create(strategy: Owned<AccountId, AssignId<ClasslessDerivativeStatus<CreateOp::Id>>>)
// -> DispatchResult {         let Owned { owner, id_assignment: AssignId(status), .. } =
// strategy;

// 		match status {
// 			ClasslessDerivativeStatus::Registrable(foreign_asset) => {
// 				let instance_id =
// 					CreateOp::create(Owned::new(owner, AutoId::new()))?;

// 				Registrar::register_derivative(foreign_asset, &instance_id)?;
// 			},
// 			ClasslessDerivativeStatus::Restorable(instance_id) =>
// 				RestoreOp::restore(instance_id, IfRestorable(owner))?,
// 		}

//         Ok(())
//     }
// }
