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
	/// The type to return from the [`Inspect::inspect`] function.
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
	/// The type of state update to accept in the [`Update::update`] function.
	type Update<'u>;

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
/// and what [`Update`](UpdateStrategy::Update) type is used.
pub trait Update<Strategy: UpdateStrategy>: AssetDefinition {
	/// Update state information of the asset
	/// using the given `id`, the update `strategy`, and the `update` value.
	///
	/// The ID type is retrieved from the [`AssetDefinition`].
	fn update(
		id: &Self::Id,
		strategy: Strategy,
		update: Strategy::Update<'_>,
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
