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

//! Abstract asset operations traits.
//!
//! The following operations are defined:
//! * [`Inspect`]
//! * [`Update`]
//! * [`Create`]
//! * [`Destroy`]
//! * [`Stash`]
//! * [`Restore`]
//!
//! Also, all the operations above (except the `Create` operation) use
//! the [`AssetDefinition`] to retrieve the `Id` type of the asset.
//!
//! An asset operation can be implemented multiple times
//! using different strategies associated with this operation.
//!
//! A strategy defines the operation behavior,
//! may supply additional parameters,
//! and may define a return value type of the operation.
//!
//! ### Usage Example
//!
//! This example shows how to interact with pallet-uniques (assuming the pallet called Uniques in
//! the chain’s Runtime) via the asset ops.
//!
//! If you are interested in the implementation example, you can look at the pallet-uniques
//! implementation. You can check out the pallet-uniques tests if you want more examples of usage.
//!
//! ```rust,ignore
//! type Collection = pallet_uniques::asset_ops::Collection<Uniques>;
//! type Item = pallet_uniques::asset_ops::Item<Uniques>;
//!
//! // Collection creation
//! //
//! // Note the `Owner` and `Admin` are inspect strategies.
//! //
//! // **Any** inspect strategy can be used to produce a config value
//! // using the `WithConfig` creation strategy.
//! Collection::create(WithConfig::new(
//!     (
//!         Owner::with_config_value(collection_owner),
//!         Admin::with_config_value(collection_admin)
//!     ),
//!     PredefinedId::from(collection_id),
//! )).unwrap();
//!
//! // Get the collection owner
//! let owner = Collection::inspect(&collection_id, Owner::default()).unwrap();
//!
//! // Get the collection admin
//! let admin = Collection::inspect(&collection_id, Admin::default()).unwrap();
//!
//! // Get collection metadata
//! let metadata = Collection::inspect(&collection_id, Bytes::default()).unwrap();
//!
//! // Get collection attribute
//! use pallet_uniques::asset_strategies::Attribute;
//! let attr_key = "example-key";
//! let attr_value = Collection::inspect(
//!    &collection_id,
//!    Bytes(Attribute(attr_key.as_slice())),
//! ).unwrap();
//!
//! // Item creation (note the usage of the same strategy -- WithConfig)
//! Item::create(WithConfig::new(
//!     Owner::with_config_value(item_owner),
//!     PredefinedId::from(item_id),
//! )).unwrap();
//!
//! // Get the item owner
//! let item_owner = Item::inspect(&(collection_id, item_id), Owner::default()).unwrap();
//!
//! // Get item attribute
//! let attr_key = "example-key";
//! let attr_value = Item::inspect(
//!     &(collection_id, item_id),
//!     Bytes(Attribute(attr_key.as_slice())),
//! ).unwrap();
//!
//! // Unconditionally update the item's owner (unchecked transfer)
//! Item::update(&(collection_id, item_id), Owner::default(), &bob).unwrap();
//!
//! // CheckOrigin then transfer
//! Item::update(
//!     &(collection_id, item_id),
//!     CheckOrigin(RuntimeOrigin::root(), Owner::default()),
//!     &bob,
//! ).unwrap();
//!
//! // From-To transfer
//! Item::update(
//!     &(collection_id, item_id),
//!     ChangeOwnerFrom::check(alice),
//!     &bob,
//! ).unwrap();
//!
//! // Lock item (forbid changing its Owner)
//! //
//! // Note that Owner strategy is turned into the `CanUpdate<Owner>` strategy
//! // via the `as_can_update` function.
//! //
//! // **Any** update strategy can be turned into the `CanUpdate` this way.
//! Item::update(
//!     &(collection_id, item_id),
//!     Owner::default().as_can_update(),
//!     false,
//! );
//! ```

use core::marker::PhantomData;
use sp_runtime::DispatchError;
use sp_std::vec::Vec;

pub mod common_strategies;

/// Trait for defining an asset.
/// The definition must provide the `Id` type to identify the asset.
pub trait AssetDefinition {
	/// Type for identifying the asset.
	type Id;
}

/// Get the `Id` type of the asset definition.
pub type AssetIdOf<T> = <T as AssetDefinition>::Id;

/// A strategy for use in the [`Inspect`] implementations.
///
/// The common inspect strategies are:
/// * [`Bytes`](common_strategies::Bytes)
/// * [`Owner`](common_strategies::Owner)
/// * [`CanCreate`](common_strategies::CanCreate)
/// * [`CanDestroy`](common_strategies::CanDestroy)
/// * [`CanUpdate`](common_strategies::CanUpdate)
pub trait InspectStrategy {
	/// The value representing the asset's state related to this `InspectStrategy`.
	type Value;
}

/// A trait representing the ability of a certain asset to **provide** its state
/// information.
///
/// This trait can be implemented multiple times using different
/// [`inspect strategies`](InspectStrategy).
///
/// An inspect strategy defines how the asset state is identified/retrieved
/// and what [`Value`](InspectStrategy::Value) type is returned.
pub trait Inspect<Strategy: InspectStrategy>: AssetDefinition {
	/// Inspect state information of the asset
	/// using the given `id` and the inspect `strategy`.
	///
	/// The ID type is retrieved from the [`AssetDefinition`].
	fn inspect(id: &Self::Id, strategy: Strategy) -> Result<Strategy::Value, DispatchError>;
}

/// A strategy for use in the [`Update`] implementations.
///
/// The common update strategies are:
/// * [`Bytes`](common_strategies::Bytes)
/// * [`CanCreate`](common_strategies::CanCreate)
/// * [`CanDestroy`](common_strategies::CanDestroy)
/// * [`CanUpdate`](common_strategies::CanUpdate)
pub trait UpdateStrategy {
	/// The value to update the asset's state.
	/// Usually, it should be related to the corresponding `InspectStrategy::Value`.
	///
	/// For instance:
	/// * If the `Value` is `Vec<u8>`, the `UpdateValue` can be `Option<&'a [u8]>` (e.g., asset
	///   attributes that can be modified or deleted).
	/// * If the `Value` is `bool`, the `UpdateValue` can also be `bool`.
	type UpdateValue<'a>;

	/// This type represents a successful asset state update.
	/// It will be in the [`Result`] type of the [`Update::update`] function.
	type Success;
}

/// A trait representing the ability of a certain asset to **update** its state information.
///
/// This trait can be implemented multiple times using different
/// [`update strategies`](UpdateStrategy).
///
/// An update strategy defines how the asset state is identified
/// and what [`UpdateValue`](UpdateStrategy::UpdateValue) type is used.
pub trait Update<Strategy: UpdateStrategy>: AssetDefinition {
	/// Update the state information of the asset
	/// using the given `id`, the update `strategy`, and the strategy's `update_value`.
	///
	/// The ID type is retrieved from the [`AssetDefinition`].
	fn update(
		id: &Self::Id,
		strategy: Strategy,
		update_value: Strategy::UpdateValue<'_>,
	) -> Result<Strategy::Success, DispatchError>;
}

/// A strategy for use in the [`Create`] implementations.
///
/// The common "create" strategy is [`WithConfig`](common_strategies::WithConfig).
pub trait CreateStrategy {
	/// This type represents a successful asset creation.
	/// It will be in the [`Result`] type of the [`Create::create`] function.
	type Success;
}

/// An ID assignment approach to use in the "create" strategies.
///
/// The common ID assignments are:
/// * [`AutoId`](common_strategies::AutoId)
/// * [`PredefinedId`](common_strategies::PredefinedId)
/// * [`DeriveAndReportId`](common_strategies::DeriveAndReportId)
pub trait IdAssignment {
	/// The reported ID type.
	///
	/// Examples:
	/// * [`AutoId`](common_strategies::AutoId) returns the ID of the newly created asset
	/// * [`PredefinedId`](common_strategies::PredefinedId) accepts the ID to be assigned to the
	///   newly created asset
	/// * [`DeriveAndReportId`](common_strategies::DeriveAndReportId) returns the ID derived from
	///   the input parameters
	type ReportedId;
}

/// A trait representing the ability of a certain asset to be created.
///
/// This trait can be implemented multiple times using different
/// [`"create" strategies`](CreateStrategy).
///
/// A create strategy defines all aspects of asset creation including how an asset ID is assigned.
pub trait Create<Strategy: CreateStrategy> {
	/// Create a new asset using the provided `strategy`.
	fn create(strategy: Strategy) -> Result<Strategy::Success, DispatchError>;
}

/// A strategy for use in the [`Destroy`] implementations.
///
/// The common destroy strategies are:
/// * [`NoParams`](common_strategies::NoParams)
/// * [`IfOwnedBy`](common_strategies::IfOwnedBy)
/// * [`WithWitness`](common_strategies::WithWitness)
pub trait DestroyStrategy {
	/// This type represents a successful asset destruction.
	/// It will be in the [`Result`] type of the [`Destroy::destroy`] function.
	type Success;
}

/// A trait representing the ability of a certain asset to be destroyed.
///
/// This trait can be implemented multiple times using different
/// [`destroy strategies`](DestroyStrategy).
///
/// A destroy strategy defines destroy parameters and the result value type.
pub trait Destroy<Strategy: DestroyStrategy>: AssetDefinition {
	/// Destroy the asset identified by the given `id` using the provided `strategy`.
	///
	/// The ID type is retrieved from the [`AssetDefinition`].
	fn destroy(id: &Self::Id, strategy: Strategy) -> Result<Strategy::Success, DispatchError>;
}

/// A strategy for use in the [`Stash`] implementations.
///
/// The common stash strategies are:
/// * [`NoParams`](common_strategies::NoParams)
/// * [`IfOwnedBy`](common_strategies::IfOwnedBy)
pub trait StashStrategy {
	/// This type represents a successful asset stashing.
	/// It will be in the [`Result`] type of the [`Stash::stash`] function.
	type Success;
}

/// A trait representing the ability of a certain asset to be stashed.
///
/// This trait can be implemented multiple times using different
/// [`stash strategies`](StashStrategy).
///
/// A stash strategy defines stash parameters.
pub trait Stash<Strategy: StashStrategy>: AssetDefinition {
	/// Stash the asset identified by the given `id` using the provided `strategy`.
	///
	/// The ID type is retrieved from the [`AssetDefinition`].
	fn stash(id: &Self::Id, strategy: Strategy) -> Result<Strategy::Success, DispatchError>;
}

/// A strategy for use in the [`Restore`] implementations.
/// The common restore strategies are:
/// * [`NoParams`](common_strategies::NoParams)
/// * [`WithConfig`](common_strategies::WithConfig)
pub trait RestoreStrategy {
	/// This type represents a successful asset restoration.
	/// It will be in the [`Result`] type of the [`Restore::restore`] function.
	type Success;
}

/// A trait representing the ability of a certain asset to be restored.
///
/// This trait can be implemented multiple times using different
/// [`restore strategies`](RestoreStrategy).
///
/// A restore strategy defines restore parameters.
pub trait Restore<Strategy: RestoreStrategy>: AssetDefinition {
	/// Restore the asset identified by the given `id` using the provided `strategy`.
	///
	/// The ID type is retrieved from the [`AssetDefinition`].
	fn restore(id: &Self::Id, strategy: Strategy) -> Result<Strategy::Success, DispatchError>;
}
