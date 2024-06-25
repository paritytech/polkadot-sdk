//! Utilities for working with unique instances derivatives.

use core::marker::PhantomData;
use frame_support::{
	ensure,
	traits::tokens::asset_ops::{
		common_asset_kinds::{Class, Instance},
		common_strategies::{AutoId, DeriveAndReportId, FromTo, IfOwnedBy, IfRestorable, Owned},
		AssetDefinition, Create, Destroy,
	},
};
use sp_runtime::{DispatchError, DispatchResult};
use xcm::latest::prelude::*;
use xcm_executor::traits::{Error, MatchesInstance};

use super::NonFungibleAsset;

/// A registry abstracts the mapping between an `Original` entity and a `Derivative` entity.
///
/// The primary use cases of the registry are:
/// * a map between a [`NonFungibleAsset`] and a derivative instance ID
/// * a map between an [`AssetId`] and a derive ID parameters for the [`DeriveAndReportId`]
/// to create a new derivative instance
pub trait DerivativesRegistry<Original, Derivative> {
	fn try_register_derivative(original: &Original, derivative: &Derivative) -> DispatchResult;

	fn try_deregister_derivative(derivative: &Derivative) -> DispatchResult;

	fn get_derivative(original: &Original) -> Option<Derivative>;

	fn get_original(derivative: &Derivative) -> Option<Original>;
}

/// Parameters for registering a new derivative instance.
pub struct DerivativeRegisterParams<DerivativeIdParams> {
	/// The XCM identified of the original unique instance.
	pub foreign_nonfungible: NonFungibleAsset,

	/// The derive ID parameters for the [`DeriveAndReportId`]
	/// to create a new derivative instance.
	pub derivative_id_params: DerivativeIdParams,
}

/// The `RegisterOnCreate` is a utility for creating a new instance
/// and immediately binding it to the original instance using the [`DerivativesRegistry`].
///
/// It implements the [`Create`] operation using the [`Owned`] strategy
/// and the [`DeriveAndReportId`] ID assignment, accepting the [`DerivativeRegisterParams`] as the
/// parameters.
///
/// The `RegisterOnCreate` will create a new derivative instance using the `InstanceOps`
/// and then bind it to the original instance via the `Registry`'s
/// [`try_register_derivative`](DerivativesRegistry::try_register_derivative).
///
/// The `InstanceOps` must be capable of creating a new instance by deriving the ID
/// based on the [`derivative_id_params`](DerivativeRegisterParams::derivative_id_params).
pub struct RegisterOnCreate<Registry, InstanceOps>(PhantomData<(Registry, InstanceOps)>);
impl<Registry, InstanceOps> AssetDefinition<Instance> for RegisterOnCreate<Registry, InstanceOps>
where
	InstanceOps: AssetDefinition<Instance>,
{
	type Id = InstanceOps::Id;
}
impl<AccountId, DerivativeIdParams, Registry, InstanceOps>
	Create<
		Instance,
		Owned<
			AccountId,
			DeriveAndReportId<DerivativeRegisterParams<DerivativeIdParams>, InstanceOps::Id>,
		>,
	> for RegisterOnCreate<Registry, InstanceOps>
where
	Registry: DerivativesRegistry<NonFungibleAsset, InstanceOps::Id>,
	InstanceOps: AssetDefinition<Instance>
		+ Create<Instance, Owned<AccountId, DeriveAndReportId<DerivativeIdParams, InstanceOps::Id>>>,
{
	fn create(
		strategy: Owned<
			AccountId,
			DeriveAndReportId<DerivativeRegisterParams<DerivativeIdParams>, InstanceOps::Id>,
		>,
	) -> Result<InstanceOps::Id, DispatchError> {
		let Owned { owner, id_assignment, .. } = strategy;

		let DerivativeRegisterParams { foreign_nonfungible, derivative_id_params } =
			id_assignment.params;

		if Registry::get_derivative(&foreign_nonfungible).is_some() {
			return Err(DispatchError::Other(
				"an attempt to register a duplicate of an existing derivative instance",
			));
		}

		let instance_id =
			InstanceOps::create(Owned::new(owner, DeriveAndReportId::from(derivative_id_params)))?;

		Registry::try_register_derivative(&foreign_nonfungible, &instance_id)?;

		Ok(instance_id)
	}
}

/// The `DeregisterOnDestroy` is a utility for destroying a derivative instance
/// and immediately removing its binding to the original instance via the [`DerivativesRegistry`].
///
/// It implements the [`Destroy`] operation using the [`IfOwnedBy`] strategy.
///
/// The `DeregisterOnDestroy` will destroy a derivative instance using the `InstanceOps`
/// and then unbind it from the original instance via the `Registry`'s
/// [`try_deregister_derivative`](DerivativesRegistry::try_deregister_derivative).
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

/// The `MatchDerivativeRegisterParams` is an XCM Matcher
/// that returns [the parameters](DerivativeRegisterParams) for registering a new derivative
/// instance.
///
/// This Matcher can be used in the
/// [`UniqueInstancesDepositAdapter`](super::UniqueInstancesDepositAdapter).
pub struct MatchDerivativeRegisterParams<Registry>(PhantomData<Registry>);
impl<Registry: DerivativesRegistry<AssetId, DerivativeIdParams>, DerivativeIdParams>
	MatchesInstance<DerivativeRegisterParams<DerivativeIdParams>>
	for MatchDerivativeRegisterParams<Registry>
{
	fn matches_instance(
		asset: &Asset,
	) -> Result<DerivativeRegisterParams<DerivativeIdParams>, Error> {
		match asset.fun {
			Fungibility::NonFungible(asset_instance) => {
				let derivative_id_params =
					Registry::get_derivative(&asset.id).ok_or(Error::AssetNotHandled)?;

				Ok(DerivativeRegisterParams {
					foreign_nonfungible: (asset.id.clone(), asset_instance),
					derivative_id_params,
				})
			},
			Fungibility::Fungible(_) => Err(Error::AssetNotHandled),
		}
	}
}

/// The `MatchDerivativeInstances` is an XCM Matcher
/// that uses a [`DerivativesRegistry`] to match the XCM identification of the original instance
/// to a derivative instance.
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

/// The `EnsureNotDerivativeInstance` is an XCM Matcher that
/// ensures that the instance returned by the inner `Matcher` isn't a derivative.
///
/// The check is performed using the [`DerivativesRegistry`].
///
/// This Matcher is needed if derivative instances are created within the same NFT engine
/// as this chain's original instances,
/// i.e. if addressing a derivative instance using the local XCM identification is possible.
///
/// For example, suppose this chain's original instances (for which this chain is the reserve
/// location) can be addressed like this `id: PalletInstance(111)/GeneralIndex(<ClassId>), fun:
/// NonFungible(Index(<InClassInstanceId>))`. So, this chain is the reserve location for all
/// instances matching the above identification.
///
/// However, if some of the instances within Pallet #111 could be derivatives as well,
/// we need to ensure that this chain won't act as the reserve location for these instances.
/// If we allow this, this chain could send a derivative as if it were the original NFT on this
/// chain. The other chain can't know that this instance isn't the original.
/// We must prevent that so this chain will act as an honest reserve location.
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
