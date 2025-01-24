//! Abstract asset operations traits.
//!
//! The following operations are defined:
//! * [`InspectMetadata`]
//! * [`UpdateMetadata`]
//! * [`Create`]
//! * [`Transfer`]
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

/// A strategy for use in the [`InspectMetadata`] implementations.
///
/// The common inspect strategies are:
/// * [`Bytes`](common_strategies::Bytes)
/// * [`Ownership`](common_strategies::Ownership)
/// * [`CanCreate`](common_strategies::CanCreate)
/// * [`CanTransfer`](common_strategies::CanTransfer)
/// * [`CanDestroy`](common_strategies::CanDestroy)
/// * [`CanUpdateMetadata`](common_strategies::CanUpdateMetadata)
pub trait MetadataInspectStrategy {
	/// The type to return from the [`InspectMetadata::inspect_metadata`] function.
	type Value;
}

/// A trait representing the ability of a certain asset to **provide** its metadata
/// information.
///
/// This trait can be implemented multiple times using different
/// [`inspect strategies`](MetadataInspectStrategy).
///
/// An inspect strategy defines how the asset metadata is identified/retrieved
/// and what [`Value`](MetadataInspectStrategy::Value) type is returned.
pub trait InspectMetadata<Strategy: MetadataInspectStrategy>: AssetDefinition {
	/// Inspect metadata information of the asset
	/// using the given `id` and the inspect `strategy`.
	///
	/// The ID type is retrieved from the [`AssetDefinition`].
	fn inspect_metadata(
		id: &Self::Id,
		strategy: Strategy,
	) -> Result<Strategy::Value, DispatchError>;
}

/// A strategy for use in the [`UpdateMetadata`] implementations.
///
/// The common update strategies are:
/// * [`Bytes`](common_strategies::Bytes)
/// * [`CanCreate`](common_strategies::CanCreate)
/// * [`CanTransfer`](common_strategies::CanTransfer)
/// * [`CanDestroy`](common_strategies::CanDestroy)
/// * [`CanUpdateMetadata`](common_strategies::CanUpdateMetadata)
pub trait MetadataUpdateStrategy {
	/// The type of metadata update to accept in the [`UpdateMetadata::update_metadata`] function.
	type Update<'u>;

	/// This type represents a successful asset metadata update.
	/// It will be in the [`Result`] type of the [`UpdateMetadata::update_metadata`] function.
	type Success;
}

/// A trait representing the ability of a certain asset to **update** its metadata information.
///
/// This trait can be implemented multiple times using different
/// [`update strategies`](MetadataUpdateStrategy).
///
/// An update strategy defines how the asset metadata is identified
/// and what [`Update`](MetadataUpdateStrategy::Update) type is used.
pub trait UpdateMetadata<Strategy: MetadataUpdateStrategy>: AssetDefinition {
	/// Update metadata information of the asset
	/// using the given `id`, the update `strategy`, and the `update` value.
	///
	/// The ID type is retrieved from the [`AssetDefinition`].
	fn update_metadata(
		id: &Self::Id,
		strategy: Strategy,
		update: Strategy::Update<'_>,
	) -> Result<Strategy::Success, DispatchError>;
}

/// A strategy for use in the [`Create`] implementations.
///
/// The common "create" strategies are:
/// * [`Owned`](common_strategies::Owned)
/// * [`WithAdmin`](common_strategies::WithAdmin)
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

/// A strategy for use in the [`Transfer`] implementations.
///
/// The common transfer strategies are:
/// * [`JustDo`](common_strategies::JustDo)
/// * [`FromTo`](common_strategies::FromTo)
pub trait TransferStrategy {
	/// This type represents a successful asset transfer.
	/// It will be in the [`Result`] type of the [`Transfer::transfer`] function.
	type Success;
}

/// A trait representing the ability of a certain asset to be transferred.
///
/// This trait can be implemented multiple times using different
/// [`transfer strategies`](TransferStrategy).
///
/// A transfer strategy defines transfer parameters.
pub trait Transfer<Strategy: TransferStrategy>: AssetDefinition {
	/// Transfer the asset identified by the given `id` using the provided `strategy`.
	///
	/// The ID type is retrieved from the [`AssetDefinition`].
	fn transfer(id: &Self::Id, strategy: Strategy) -> Result<Strategy::Success, DispatchError>;
}

/// A strategy for use in the [`Destroy`] implementations.
///
/// The common destroy strategies are:
/// * [`JustDo`](common_strategies::JustDo)
/// * [`IfOwnedBy`](common_strategies::IfOwnedBy)
/// * [`WithWitness`](common_strategies::WithWitness)
/// * [`IfOwnedByWithWitness`](common_strategies::IfOwnedByWithWitness)
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
/// * [`JustDo`](common_strategies::JustDo)
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
/// * [`JustDo`](common_strategies::JustDo)
/// * [`IfRestorable`](common_strategies::IfRestorable)
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
