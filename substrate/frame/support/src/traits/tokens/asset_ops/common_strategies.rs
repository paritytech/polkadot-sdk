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

//! This modules contains the common asset ops strategies.

use super::*;
use crate::pallet_prelude::RuntimeDebug;
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// The `CheckState` is a strategy that accepts an `Inspect` value and the `Inner` strategy.
///
/// It is meant to be used when the asset state check should be performed
/// prior to the `Inner` strategy execution.
/// **The inspected state must be equal to the provided value.**
///
/// The `CheckState` implements all potentially state-mutating strategies that the `Inner`
/// implements.
pub struct CheckState<Inspect: InspectStrategy, Inner = NoParams>(pub Inspect::Value, pub Inner);
impl<Inspect: InspectStrategy, Inner: Default> CheckState<Inspect, Inner> {
	/// This function creates a `CheckState` strategy.
	/// The operation that accepts it must check if the provided `expected` value
	/// equals the in-storage one.
	///
	/// If so, the operation must, in turn, proceed according to the default value of the `Inner`
	/// strategy.
	pub fn check(expected: Inspect::Value) -> Self {
		Self(expected, Default::default())
	}
}
impl<Inspect: InspectStrategy, Inner> CheckState<Inspect, Inner> {
	/// This function creates a `CheckState` strategy.
	/// The operation that accepts it must check if the provided `expected` value
	/// equals the in-storage one.
	///
	/// If so, the operation must, in turn, proceed according to the provided value of the `Inner`
	/// strategy.
	pub fn new(expected: Inspect::Value, inner: Inner) -> Self {
		Self(expected, inner)
	}
}
impl<Inspect: InspectStrategy, Inner: UpdateStrategy> UpdateStrategy
	for CheckState<Inspect, Inner>
{
	type UpdateValue<'u> = Inner::UpdateValue<'u>;
	type Success = Inner::Success;
}
impl<Inspect: InspectStrategy, Inner: CreateStrategy> CreateStrategy
	for CheckState<Inspect, Inner>
{
	type Success = Inner::Success;
}
impl<Inspect: InspectStrategy, Inner: DestroyStrategy> DestroyStrategy
	for CheckState<Inspect, Inner>
{
	type Success = Inner::Success;
}
impl<Inspect: InspectStrategy, Inner: StashStrategy> StashStrategy for CheckState<Inspect, Inner> {
	type Success = Inner::Success;
}
impl<Inspect: InspectStrategy, Inner: RestoreStrategy> RestoreStrategy
	for CheckState<Inspect, Inner>
{
	type Success = Inner::Success;
}

/// The `CheckOrigin` is a strategy that accepts a runtime origin and the `Inner` strategy.
///
/// It is meant to be used when the origin check should be performed
/// prior to the `Inner` strategy execution.
///
/// The `CheckOrigin` implements all potentially state-mutating strategies that the `Inner`
/// implements.
pub struct CheckOrigin<RuntimeOrigin, Inner = NoParams>(pub RuntimeOrigin, pub Inner);
impl<RuntimeOrigin, Inner: Default> CheckOrigin<RuntimeOrigin, Inner> {
	/// This function creates a `CheckOrigin` strategy.
	/// The operation that accepts it must check if the provided `origin` is allowed to perform it.
	///
	/// If so, the operation must, in turn, proceed according to the default value of the `Inner`
	/// strategy.
	pub fn check(origin: RuntimeOrigin) -> Self {
		Self(origin, Default::default())
	}
}
impl<RuntimeOrigin, Inner> CheckOrigin<RuntimeOrigin, Inner> {
	/// This function creates a `CheckOrigin` strategy.
	/// The operation that accepts it must check if the provided `origin` is allowed to perform it.
	///
	/// If so, the operation must, in turn, proceed according to the provided value of the `Inner`
	/// strategy.
	pub fn new(origin: RuntimeOrigin, inner: Inner) -> Self {
		Self(origin, inner)
	}
}

impl<RuntimeOrigin, Inner: UpdateStrategy> UpdateStrategy for CheckOrigin<RuntimeOrigin, Inner> {
	type UpdateValue<'u> = Inner::UpdateValue<'u>;
	type Success = Inner::Success;
}
impl<RuntimeOrigin, Inner: CreateStrategy> CreateStrategy for CheckOrigin<RuntimeOrigin, Inner> {
	type Success = Inner::Success;
}
impl<RuntimeOrigin, Inner: DestroyStrategy> DestroyStrategy for CheckOrigin<RuntimeOrigin, Inner> {
	type Success = Inner::Success;
}
impl<RuntimeOrigin, Inner: StashStrategy> StashStrategy for CheckOrigin<RuntimeOrigin, Inner> {
	type Success = Inner::Success;
}
impl<RuntimeOrigin, Inner: RestoreStrategy> RestoreStrategy for CheckOrigin<RuntimeOrigin, Inner> {
	type Success = Inner::Success;
}

/// The NoParams represents the simplest state-mutating strategy,
/// which doesn't require any parameters to perform the operation.
///
/// It can be used as the following strategies:
/// * [`destroy strategy`](DestroyStrategy)
/// * [`stash strategy`](StashStrategy)
/// * [`restore strategy`](RestoreStrategy)
#[derive(Default)]
pub struct NoParams;
impl DestroyStrategy for NoParams {
	type Success = ();
}
impl StashStrategy for NoParams {
	type Success = ();
}
impl RestoreStrategy for NoParams {
	type Success = ();
}

/// The `Bytes` strategy represents raw state bytes.
/// It is both an [inspect](InspectStrategy) and [update](UpdateStrategy)
/// strategy.
///
/// * As the inspect strategy, it returns `Vec<u8>`.
/// * As the update strategy, it accepts `Option<&[u8]>`, where `None` means data removal.
///
/// By default, the `Bytes` identifies a byte blob associated with the asset (the only one
/// blob). However, a user can define several variants of this strategy by supplying the
/// `Request` type. The `Request` type can also contain additional data (like a byte key) to
/// identify a certain byte data.
/// For instance, there can be several named byte attributes. In that case, the `Request` might
/// be something like `Attribute(/* name: */ String)`.
pub struct Bytes<Request = ()>(pub Request);
impl Default for Bytes<()> {
	fn default() -> Self {
		Self(())
	}
}
impl<Request> InspectStrategy for Bytes<Request> {
	type Value = Vec<u8>;
}
impl<Request> UpdateStrategy for Bytes<Request> {
	type UpdateValue<'u> = Option<&'u [u8]>;
	type Success = ();
}

/// The `Owner` strategy is both [inspect](InspectStrategy) and [update](UpdateStrategy) strategy
/// allows getting and setting the owner of an asset.
#[derive(RuntimeDebug, PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct Owner<AccountId>(PhantomData<AccountId>);
impl<AccountId> Default for Owner<AccountId> {
	fn default() -> Self {
		Self(PhantomData)
	}
}
impl<AccountId> InspectStrategy for Owner<AccountId> {
	type Value = AccountId;
}
impl<AccountId: 'static> UpdateStrategy for Owner<AccountId> {
	type UpdateValue<'u> = &'u AccountId;
	type Success = ();
}

/// The `Admin` strategy is both [inspect](InspectStrategy) and [update](UpdateStrategy) strategy
/// allows getting and setting the admin of an asset.
#[derive(RuntimeDebug, PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct Admin<AccountId>(PhantomData<AccountId>);
impl<AccountId> Default for Admin<AccountId> {
	fn default() -> Self {
		Self(PhantomData)
	}
}
impl<AccountId> InspectStrategy for Admin<AccountId> {
	type Value = AccountId;
}
impl<AccountId: 'static> UpdateStrategy for Admin<AccountId> {
	type UpdateValue<'u> = &'u AccountId;
	type Success = ();
}

/// The `Witness` strategy is an [inspect](InspectStrategy) strategy,
/// which gets the specified `WitnessData` from the asset.
///
/// The `WitnessData` can be anything descriptive about the asset that helps perform a related
/// operation. For instance, a witness could be required to destroy an NFT collection because the
/// corresponding extrinsic's weight couldn't be known ahead of time without providing, for example,
/// the number of items within the collection. In this case, the number of items is the witness
/// data. The said extrinsic, in turn, could use the destroy operation with the `WithWitness`
/// strategy, which will compare the provided witness with the actual chain state before attempting
/// the collection destruction.
#[derive(RuntimeDebug, PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct Witness<WitnessData>(PhantomData<WitnessData>);
impl<WitnessData> Default for Witness<WitnessData> {
	fn default() -> Self {
		Self(PhantomData)
	}
}
impl<WitnessData> InspectStrategy for Witness<WitnessData> {
	type Value = WitnessData;
}

/// The operation implementation must check
/// if the given account owns the asset and act according to the inner strategy.
pub type IfOwnedBy<AccountId, Inner = NoParams> = CheckState<Owner<AccountId>, Inner>;

/// The operation implementation must check
/// if the given account owns the asset and only then perform the owner update to the one supplied
/// to the `Update::update` function.
pub type ChangeOwnerFrom<AccountId> = CheckState<Owner<AccountId>, Owner<AccountId>>;

/// The operation implementation must check
/// if the given witness represents the correct state of the asset.
/// If so, the operation must act according to the inner strategy.
pub type WithWitness<WitnessData, Inner = NoParams> = CheckState<Witness<WitnessData>, Inner>;

/// The `CanCreate` strategy represents the ability to create an asset.
/// It is both an [inspect](InspectStrategy) and [update](UpdateStrategy)
/// strategy.
///
/// * As the inspect strategy, it returns `bool`.
/// * As the update strategy, it accepts `bool`.
///
/// By default, this strategy means the ability to create an asset "in general".
/// However, a user can define several variants of this strategy by supplying the `Condition`
/// type. Using the `Condition` value, we are formulating the question, "Can this be created
/// under the given condition?". For instance, "Can **a specific user** create an asset?".
pub struct CanCreate<Condition = ()>(pub Condition);
impl Default for CanCreate<()> {
	fn default() -> Self {
		Self(())
	}
}
impl<Condition> InspectStrategy for CanCreate<Condition> {
	type Value = bool;
}
impl<Condition> UpdateStrategy for CanCreate<Condition> {
	type UpdateValue<'u> = bool;
	type Success = ();
}

/// The `CanDestroy` strategy represents the ability to destroy an asset.
/// It is both an [inspect](InspectStrategy) and [update](UpdateStrategy)
/// strategy.
///
/// * As the inspect strategy, it returns `bool`.
/// * As the update strategy, it accepts `bool`.
///
/// By default, this strategy means the ability to destroy an asset "in general".
/// However, a user can define several variants of this strategy by supplying the `Condition`
/// type. Using the `Condition` value, we are formulating the question, "Can this be destroyed
/// under the given condition?". For instance, "Can **a specific user** destroy an asset of
/// **another user**?".
pub struct CanDestroy<Condition = ()>(pub Condition);
impl Default for CanDestroy<()> {
	fn default() -> Self {
		Self(())
	}
}
impl<Condition> InspectStrategy for CanDestroy<Condition> {
	type Value = bool;
}
impl<Condition> UpdateStrategy for CanDestroy<Condition> {
	type UpdateValue<'u> = bool;
	type Success = ();
}

/// The `CanUpdate` strategy represents the ability to update the state of an asset.
/// It is both an [inspect](InspectStrategy) and [update](UpdateStrategy)
/// strategy.
///
/// * As the inspect strategy, it returns `bool`.
/// * As the update strategy is accepts `bool`.
///
/// By default, this strategy means the ability to update the state of an asset "in general".
/// However, a user can define several flavors of this strategy by supplying the `Flavor` type.
/// The `Flavor` type can add more details to the strategy.
/// For instance, "Can **a specific user** update the state of an asset **under a certain
/// key**?".
pub struct CanUpdate<Flavor = ()>(pub Flavor);
impl Default for CanUpdate<()> {
	fn default() -> Self {
		Self(())
	}
}
impl<Flavor> InspectStrategy for CanUpdate<Flavor> {
	type Value = bool;
}
impl<Flavor> UpdateStrategy for CanUpdate<Flavor> {
	type UpdateValue<'u> = bool;
	type Success = ();
}

/// This trait converts the given [UpdateStrategy] into the corresponding [CanUpdate] strategy
/// representing the ability to update the asset using the provided strategy.
pub trait AsCanUpdate: Sized + UpdateStrategy {
	fn as_can_update(self) -> CanUpdate<Self>;
}
impl<T: UpdateStrategy> AsCanUpdate for T {
	fn as_can_update(self) -> CanUpdate<Self> {
		CanUpdate(self)
	}
}

/// The `AutoId` is an ID assignment approach intended to be used in
/// [`"create" strategies`](CreateStrategy).
///
/// It accepts the `Id` type of the asset.
/// The "create" strategy should report the value of type `ReportedId` upon successful asset
/// creation.
pub type AutoId<ReportedId> = DeriveAndReportId<(), ReportedId>;

/// The `PredefinedId` is an ID assignment approach intended to be used in
/// [`"create" strategies`](CreateStrategy).
///
/// It accepts the `Id` that should be assigned to the newly created asset.
///
/// The "create" strategy should report the `Id` value upon successful asset creation.
pub type PredefinedId<Id> = DeriveAndReportId<Id, Id>;

/// The `DeriveAndReportId` is an ID assignment approach intended to be used in
/// [`"create" strategies`](CreateStrategy).
///
/// It accepts the `Params` and the `Id`.
/// The `ReportedId` value should be computed by the "create" strategy using the `Params` value.
///
/// The "create" strategy should report the `ReportedId` value upon successful asset creation.
///
/// An example of ID derivation is the creation of an NFT inside a collection using the
/// collection ID as `Params`. The `ReportedId` in this case is the full ID of the NFT.
#[derive(RuntimeDebug, PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct DeriveAndReportId<Params, ReportedId> {
	pub params: Params,
	_phantom: PhantomData<ReportedId>,
}
impl<ReportedId> DeriveAndReportId<(), ReportedId> {
	pub fn auto() -> AutoId<ReportedId> {
		Self { params: (), _phantom: PhantomData }
	}
}
impl<Params, ReportedId> DeriveAndReportId<Params, ReportedId> {
	pub fn from(params: Params) -> Self {
		Self { params, _phantom: PhantomData }
	}
}
impl<Params, ReportedId> IdAssignment for DeriveAndReportId<Params, ReportedId> {
	type ReportedId = ReportedId;
}

/// Represents the value of an [InspectStrategy] to be used as a configuration value in the
/// [WithConfig] strategy.
#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct ConfigValue<Inspect: InspectStrategy>(pub Inspect::Value);

/// This trait marks a config value to be used in the [WithConfig] strategy.
/// It is used to make compiler error messages clearer if invalid type is supplied into the
/// [WithConfig].
pub trait ConfigValueMarker {}
impl<Inspect: InspectStrategy> ConfigValueMarker for ConfigValue<Inspect> {}

#[impl_trait_for_tuples::impl_for_tuples(1, 8)]
impl ConfigValueMarker for Tuple {}

/// This trait converts the given [InspectStrategy] into the config value to be used in the
/// [WithConfig] strategy.
pub trait WithConfigValue: Sized + InspectStrategy {
	fn with_config_value(value: Self::Value) -> ConfigValue<Self>;
}
impl<T: InspectStrategy> WithConfigValue for T {
	fn with_config_value(value: Self::Value) -> ConfigValue<Self> {
		ConfigValue::<Self>(value)
	}
}

/// The `WithConfig` is a [create](CreateStrategy) and [restore](RestoreStrategy) strategy.
/// It facilitates setting the asset's properties that can be later inspected via the corresponding
/// [inspect strategies](InspectStrategy). The provided asset's properties are considered its
/// config. Every inspect strategy can be used to create a config value.
///
/// For instance, one can use `WithConfig` to restore an asset to the given owner using the [Owner]
/// inspect strategy:
///
/// ```rust,ignore
/// NftEngine::restore(WithConfig::from(Owner::with_config_value(OWNER_ACCOUNT)))
/// ```
///
/// The extra parameters can be supplied to provide additional context to the operation.
/// They're required for creation operation as they provide the [id assignment
/// approach](IdAssignment), but they're optional for the restoring operation.
///
/// For instance, one can use `WithConfig` to create an asset with a predefined id this way:
///
/// ```rust,ignore
/// NftEngine::create(WithConfig::new(
///     Owner::with_config_value(OWNER_ACCOUNT),
///     PredefinedId::from(ASSET_ID),
/// ))
/// ```
///
/// Note: you can use several config values by providing a tuple of them:
///
/// ```rust,ignore
/// NftEngine::create(WithConfig::new(
///     (
///          Owner::with_config_value(OWNER_ACCOUNT),
///          Admin::with_config_value(ADMIN_ACCOUNT),
///     ),
///     PredefinedId::from(ASSET_ID),
/// ))
/// ```
#[derive(RuntimeDebug, PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct WithConfig<ConfigValue: ConfigValueMarker, Extra = ()> {
	pub config: ConfigValue,
	pub extra: Extra,
}

impl<ConfigValue: ConfigValueMarker> WithConfig<ConfigValue> {
	pub fn from(config: ConfigValue) -> Self {
		Self { config, extra: () }
	}
}
impl<ConfigValue: ConfigValueMarker, Extra> WithConfig<ConfigValue, Extra> {
	pub fn new(config: ConfigValue, extra: Extra) -> Self {
		Self { config, extra }
	}
}
impl<ConfigValue: ConfigValueMarker, Assignment: IdAssignment> CreateStrategy
	for WithConfig<ConfigValue, Assignment>
{
	type Success = Assignment::ReportedId;
}
impl<ConfigValue: ConfigValueMarker, Extra> RestoreStrategy for WithConfig<ConfigValue, Extra> {
	type Success = ();
}
