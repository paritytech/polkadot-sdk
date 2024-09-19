//! Utilities for working with unique instances derivatives.

use core::marker::PhantomData;
use frame_support::{
	ensure,
	traits::tokens::asset_ops::{
		common_asset_kinds::Instance,
		common_strategies::{DeriveAndReportId, IfOwnedBy, Owned},
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

	fn get_derivative(original: &Original) -> Result<Derivative, DispatchError>;

	fn get_original(derivative: &Derivative) -> Result<Original, DispatchError>;
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
					.map_err(|_| Error::AssetNotHandled),
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

		ensure!(Registry::get_original(&instance_id).is_err(), Error::AssetNotHandled);

		Ok(instance_id)
	}
}
