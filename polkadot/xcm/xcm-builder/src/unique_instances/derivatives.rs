use core::marker::PhantomData;

use frame_support::{
	ensure,
	traits::{
		tokens::asset_ops::{
			common_asset_kinds::{Class, Instance},
			common_strategies::{
				AssignId, AutoId, DeriveAndReportId, FromTo, IfOwnedBy, IfRestorable, Owned,
			},
			AssetDefinition, Create, CreateStrategy, Destroy, DestroyStrategy, Restore, Stash,
			Transfer, TransferStrategy,
		},
		TypedGet,
	},
};
use sp_runtime::{DispatchError, DispatchResult};
use xcm::latest::prelude::*;
use xcm_executor::traits::{Error, MatchesInstance};

use super::NonFungibleAsset;

pub trait DerivativesRegistry<Original, Derivative> {
	fn try_register_derivative(original: &Original, derivative: &Derivative) -> DispatchResult;

	fn try_deregister_derivative(derivative: &Derivative) -> DispatchResult;

	fn get_derivative(original: &Original) -> Option<Derivative>;

	fn get_original(derivative: &Derivative) -> Option<Original>;
}

pub struct RegisterDerivativeId<InstanceIdSource> {
	pub foreign_nonfungible: NonFungibleAsset,
	pub instance_id_source: InstanceIdSource,
}

pub struct RegisterOnCreate<Registry, InstanceOps>(PhantomData<(Registry, InstanceOps)>);
impl<AccountId, InstanceIdSource, Registry, InstanceOps>
	Create<Instance, Owned<AccountId, AssignId<RegisterDerivativeId<InstanceIdSource>>>>
	for RegisterOnCreate<Registry, InstanceOps>
where
	Registry: DerivativesRegistry<NonFungibleAsset, InstanceOps::Id>,
	InstanceOps: AssetDefinition<Instance>
		+ Create<Instance, Owned<AccountId, DeriveAndReportId<InstanceIdSource, InstanceOps::Id>>>,
{
	fn create(
		strategy: Owned<AccountId, AssignId<RegisterDerivativeId<InstanceIdSource>>>,
	) -> DispatchResult {
		let Owned {
			owner,
			id_assignment:
				AssignId(RegisterDerivativeId { foreign_nonfungible, instance_id_source }),
			..
		} = strategy;

		if Registry::get_derivative(&foreign_nonfungible).is_some() {
			return Err(DispatchError::Other(
				"an attempt to register a duplicate of an existing derivative instance",
			));
		}

		let instance_id =
			InstanceOps::create(Owned::new(owner, DeriveAndReportId::from(instance_id_source)))?;

		Registry::try_register_derivative(&foreign_nonfungible, &instance_id)
	}
}

pub struct DeregisterOnDestroy<Registry, InstanceOps>(PhantomData<(Registry, InstanceOps)>);
impl<Registry, InstanceOps> AssetDefinition<Instance> for DeregisterOnDestroy<Registry, InstanceOps>
where
	InstanceOps: AssetDefinition<Instance>,
{
	type Id = InstanceOps::Id;
}
impl<AccountId, Registry, InstanceOps> Destroy<Instance, IfOwnedBy<AccountId>>
	for DeregisterOnDestroy<Registry, InstanceOps>
where
	Registry: DerivativesRegistry<NonFungibleAsset, InstanceOps::Id>,
	InstanceOps: Destroy<Instance, IfOwnedBy<AccountId>>,
{
	fn destroy(id: &Self::Id, strategy: IfOwnedBy<AccountId>) -> DispatchResult {
		if Registry::get_original(id).is_none() {
			return Err(DispatchError::Other(
				"an attempt to deregister an instance that isn't a derivative",
			));
		}

		InstanceOps::destroy(id, strategy)?;

		Registry::try_deregister_derivative(id)
	}
}

pub struct MatchDerivativeIdSources<Registry>(PhantomData<Registry>);
impl<Registry: DerivativesRegistry<AssetId, DerivativeIdSource>, DerivativeIdSource>
	MatchesInstance<AssignId<RegisterDerivativeId<DerivativeIdSource>>>
	for MatchDerivativeIdSources<Registry>
{
	fn matches_instance(
		asset: &Asset,
	) -> Result<AssignId<RegisterDerivativeId<DerivativeIdSource>>, Error> {
		match asset.fun {
			Fungibility::NonFungible(asset_instance) => {
				let instance_id_source =
					Registry::get_derivative(&asset.id).ok_or(Error::AssetNotHandled)?;

				Ok(AssignId(RegisterDerivativeId {
					foreign_nonfungible: (asset.id.clone(), asset_instance),
					instance_id_source,
				}))
			},
			Fungibility::Fungible(_) => Err(Error::AssetNotHandled),
		}
	}
}

pub struct MatchDerivativeInstances<Registry>(PhantomData<Registry>);
impl<Registry: DerivativesRegistry<NonFungibleAsset, DerivativeId>, DerivativeId>
	MatchesInstance<DerivativeId> for MatchDerivativeInstances<Registry>
{
	fn matches_instance(asset: &Asset) -> Result<DerivativeId, Error> {
		match asset.fun {
			Fungibility::NonFungible(asset_instance) =>
				Registry::get_derivative(&(asset.id.clone(), asset_instance))
					.ok_or(Error::AssetNotHandled),
			Fungibility::Fungible(_) => Err(Error::AssetNotHandled),
		}
	}
}

pub struct EnsureNotDerivativeInstance<Registry, Matcher>(PhantomData<(Registry, Matcher)>);
impl<
		Registry: DerivativesRegistry<NonFungibleAsset, DerivativeId>,
		Matcher: MatchesInstance<DerivativeId>,
		DerivativeId,
	> MatchesInstance<DerivativeId> for EnsureNotDerivativeInstance<Registry, Matcher>
{
	fn matches_instance(asset: &Asset) -> Result<DerivativeId, Error> {
		let instance_id = Matcher::matches_instance(asset)?;

		ensure!(Registry::get_original(&instance_id).is_none(), Error::AssetNotHandled,);

		Ok(instance_id)
	}
}
