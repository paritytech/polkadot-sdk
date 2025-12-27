// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Miscellaneous traits and types for working with unique instances derivatives.

use core::marker::PhantomData;
use frame_support::{
	ensure, parameter_types,
	traits::{
		tokens::asset_ops::{
			common_strategies::{
				AutoId, ConfigValue, ConfigValueMarker, DeriveAndReportId, Owner, WithConfig,
			},
			Create,
		},
		Incrementable,
	},
};
use sp_runtime::{
	traits::{Convert, TypedGet},
	DispatchError, DispatchResult,
};
use xcm::latest::prelude::*;
use xcm_builder::unique_instances::NonFungibleAsset;
use xcm_executor::traits::{ConvertLocation, Error, MatchesInstance};

/// A registry abstracts the mapping between an `Original` entity and a `Derivative` entity.
///
/// The primary use cases of the registry are:
/// * a map between an `AssetId` and an chain-local asset ID.
/// For instance, it could be chain-local currency ID or an NFT collection ID.
/// * a map between a [`NonFungibleAsset`] and a derivative instance ID
/// to create a new derivative instance
pub trait DerivativesRegistry<Original, Derivative> {
	fn try_register_derivative(original: &Original, derivative: &Derivative) -> DispatchResult;

	fn try_deregister_derivative_of(original: &Original) -> DispatchResult;

	fn get_derivative(original: &Original) -> Result<Derivative, DispatchError>;

	fn get_original(derivative: &Derivative) -> Result<Original, DispatchError>;
}

/// The `OriginalToDerivativeConvert` uses the provided [DerivativesRegistry] to
/// convert the `Original` value to the `Derivative` one.
pub struct OriginalToDerivativeConvert<R>(PhantomData<R>);
impl<Original, Derivative, R: DerivativesRegistry<Original, Derivative>>
	Convert<Original, Result<Derivative, DispatchError>> for OriginalToDerivativeConvert<R>
{
	fn convert(a: Original) -> Result<Derivative, DispatchError> {
		R::get_derivative(&a)
	}
}

/// The `DerivativeToOriginalConvert` uses the provided [DerivativesRegistry] to
/// convert the `Derivative` value to the `Original` one.
pub struct DerivativeToOriginalConvert<R>(PhantomData<R>);
impl<Original, Derivative, R: DerivativesRegistry<Original, Derivative>>
	Convert<Derivative, Result<Original, DispatchError>> for DerivativeToOriginalConvert<R>
{
	fn convert(a: Derivative) -> Result<Original, DispatchError> {
		R::get_original(&a)
	}
}

/// The `RegisterDerivative` implements a creation operation with `DeriveAndReportId`,
/// which takes the `Original` and derives the corresponding `Derivative`.
///
/// The mapping between them will be registered via the registry `R`.
pub struct RegisterDerivative<R, CreateOp>(PhantomData<(R, CreateOp)>);
impl<Original, Derivative, R, CreateOp> Create<DeriveAndReportId<Original, Derivative>>
	for RegisterDerivative<R, CreateOp>
where
	Original: Clone,
	R: DerivativesRegistry<Original, Derivative>,
	CreateOp: Create<DeriveAndReportId<Original, Derivative>>,
{
	fn create(
		id_assignment: DeriveAndReportId<Original, Derivative>,
	) -> Result<Derivative, DispatchError> {
		let original = id_assignment.params;
		let derivative = CreateOp::create(DeriveAndReportId::from(original.clone()))?;
		R::try_register_derivative(&original, &derivative)?;

		Ok(derivative)
	}
}
impl<Original, Derivative, R, Config, CreateOp>
	Create<WithConfig<Config, DeriveAndReportId<Original, Derivative>>>
	for RegisterDerivative<R, CreateOp>
where
	Original: Clone,
	R: DerivativesRegistry<Original, Derivative>,
	Config: ConfigValueMarker,
	CreateOp: Create<WithConfig<Config, DeriveAndReportId<Original, Derivative>>>,
{
	fn create(
		strategy: WithConfig<Config, DeriveAndReportId<Original, Derivative>>,
	) -> Result<Derivative, DispatchError> {
		let WithConfig { config, extra: id_assignment } = strategy;
		let original = id_assignment.params;
		let derivative =
			CreateOp::create(WithConfig::new(config, DeriveAndReportId::from(original.clone())))?;
		R::try_register_derivative(&original, &derivative)?;

		Ok(derivative)
	}
}

/// Iterator utilities for a derivatives registry.
pub trait IterDerivativesRegistry<Original, Derivative> {
	fn iter_originals() -> impl Iterator<Item = Original>;

	fn iter_derivatives() -> impl Iterator<Item = Derivative>;

	fn iter() -> impl Iterator<Item = (Original, Derivative)>;
}

/// Derivatives extra data.
pub trait DerivativesExtra<Derivative, Extra> {
	fn get_derivative_extra(derivative: &Derivative) -> Option<Extra>;

	fn set_derivative_extra(derivative: &Derivative, extra: Option<Extra>) -> DispatchResult;
}

/// The `ConcatIncrementalExtra` implements a creation operation that takes a derivative.
/// It takes the derivative's extra data and passes the tuple of the derivative and its extra data
/// to the underlying `CreateOp` (i.e., concatenates the derivative and its extra).
///
/// The extra data gets incremented using the [Incrementable::increment] function, and the new extra
/// value is set for the given derivative. The initial extra value is produced using the
/// [Incrementable::initial_value] function.
pub struct ConcatIncrementalExtra<Derivative, Extra, Registry, CreateOp>(
	PhantomData<(Derivative, Extra, Registry, CreateOp)>,
);
impl<Derivative, Extra, ReportedId, Registry, CreateOp>
	Create<DeriveAndReportId<Derivative, ReportedId>>
	for ConcatIncrementalExtra<Derivative, Extra, Registry, CreateOp>
where
	Extra: Incrementable,
	Registry: DerivativesExtra<Derivative, Extra>,
	CreateOp: Create<DeriveAndReportId<(Derivative, Extra), ReportedId>>,
{
	fn create(
		id_assignment: DeriveAndReportId<Derivative, ReportedId>,
	) -> Result<ReportedId, DispatchError> {
		let derivative = id_assignment.params;

		let id = Registry::get_derivative_extra(&derivative).or(Extra::initial_value()).ok_or(
			DispatchError::Other(
				"ConcatIncrementalExtra: unable to initialize incremental derivative extra",
			),
		)?;
		let next_id = id
			.increment()
			.ok_or(DispatchError::Other("ConcatIncrementalExtra: failed to increment the id"))?;

		Registry::set_derivative_extra(&derivative, Some(next_id))?;

		CreateOp::create(DeriveAndReportId::from((derivative, id)))
	}
}
impl<Config, Derivative, Extra, ReportedId, Registry, CreateOp>
	Create<WithConfig<Config, DeriveAndReportId<Derivative, ReportedId>>>
	for ConcatIncrementalExtra<Derivative, Extra, Registry, CreateOp>
where
	Config: ConfigValueMarker,
	Extra: Incrementable,
	Registry: DerivativesExtra<Derivative, Extra>,
	CreateOp: Create<WithConfig<Config, DeriveAndReportId<(Derivative, Extra), ReportedId>>>,
{
	fn create(
		strategy: WithConfig<Config, DeriveAndReportId<Derivative, ReportedId>>,
	) -> Result<ReportedId, DispatchError> {
		let WithConfig { config, extra: id_assignment } = strategy;
		let derivative = id_assignment.params;

		let id = Registry::get_derivative_extra(&derivative)
			.or(Extra::initial_value())
			.ok_or(DispatchError::Other("ConcatIncrementalExtra: no derivative extra is found"))?;
		let next_id = id
			.increment()
			.ok_or(DispatchError::Other("ConcatIncrementalExtra: failed to increment the id"))?;

		Registry::set_derivative_extra(&derivative, Some(next_id))?;

		CreateOp::create(WithConfig::new(config, DeriveAndReportId::from((derivative, id))))
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

parameter_types! {
	pub OwnerConvertedLocationDefaultErr: DispatchError = DispatchError::Other("OwnerConvertedLocation: failed to convert the location");
}

/// Converts a given `AssetId` to a `WithConfig` strategy with the owner account set to the asset's
/// location converted to an account ID.
pub struct OwnerConvertedLocation<CL, IdAssignment, Err = OwnerConvertedLocationDefaultErr>(
	PhantomData<(CL, IdAssignment, Err)>,
);
impl<AccountId, CL, Err, ReportedId>
	Convert<
		AssetId,
		Result<
			WithConfig<ConfigValue<Owner<AccountId>>, DeriveAndReportId<AssetId, ReportedId>>,
			DispatchError,
		>,
	> for OwnerConvertedLocation<CL, DeriveAndReportId<AssetId, ReportedId>, Err>
where
	CL: ConvertLocation<AccountId>,
	Err: TypedGet,
	Err::Type: Into<DispatchError>,
{
	fn convert(
		AssetId(location): AssetId,
	) -> Result<
		WithConfig<ConfigValue<Owner<AccountId>>, DeriveAndReportId<AssetId, ReportedId>>,
		DispatchError,
	> {
		CL::convert_location(&location)
			.map(|account| {
				WithConfig::new(ConfigValue(account), DeriveAndReportId::from(AssetId(location)))
			})
			.ok_or(Err::get().into())
	}
}
impl<AccountId, CL, Err, ReportedId>
	Convert<
		AssetId,
		Result<WithConfig<ConfigValue<Owner<AccountId>>, AutoId<ReportedId>>, DispatchError>,
	> for OwnerConvertedLocation<CL, AutoId<ReportedId>, Err>
where
	CL: ConvertLocation<AccountId>,
	Err: TypedGet,
	Err::Type: Into<DispatchError>,
{
	fn convert(
		AssetId(location): AssetId,
	) -> Result<WithConfig<ConfigValue<Owner<AccountId>>, AutoId<ReportedId>>, DispatchError> {
		CL::convert_location(&location)
			.map(|account| WithConfig::new(ConfigValue(account), AutoId::auto()))
			.ok_or(Err::get().into())
	}
}
