//! Abstract asset operations traits.
//!
//! The following operations are defined:
//! * [`InspectMetadata`]
//! * [`UpdateMetadata`]
//! * [`Create`]
//! * [`Transfer`]
//! * [`Destroy`]
//!
//! Also, all the operations above (except the `Create` operation) use
//! the [`AssetDefinition`] to retrieve the `Id` type of the asset.
//!
//! Each asset operation can be implemented for different asset kinds
//! such as [`Class`](common_asset_kinds::Class) and [`Instance`](common_asset_kinds::Instance).
//!
//! Also, an asset operation can be implemented multiple times
//! using different strategies associated with this operation.
//!
//! A strategy defines the operation behavior,
//! may supply additional parameters,
//! and may define a return value type of the operation.

use crate::dispatch::DispatchResult;
use core::marker::PhantomData;
use sp_runtime::DispatchError;
use sp_std::vec::Vec;

/// Trait for defining an asset of a certain kind.
/// The definition must provide the `Id` type to identify the asset.
///
/// The common asset kinds are:
/// * The [`Class`](common_asset_kinds::Class) asset kind is of assets that resemble class-like
/// entities. For example, a collection of non-fungible tokens belongs to this kind.
/// * The [`Instance`](common_asset_kinds::Instance) asset kind is of assets that resemble concrete
/// instances of something. For example, a non-fungible token (which may or may not be part of a
/// certain class) belongs to this kind.
///
/// Other asset kinds can be defined.
pub trait AssetDefinition<AssetKind> {
	/// Type for identifying the asset.
	type Id;
}

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

/// A trait representing the ability of a certain asset kind to **provide** its metadata
/// information.
///
/// This trait can be implemented multiple times using different [`inspect
/// strategies`](MetadataInspectStrategy).
///
/// An inspect strategy defines how the asset metadata is identified/retrieved
/// and what [`Value`](MetadataInspectStrategy::Value) type is returned.
pub trait InspectMetadata<AssetKind, Strategy: MetadataInspectStrategy>:
	AssetDefinition<AssetKind>
{
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
}

/// A trait representing the ability of a certain asset kind to **update** its metadata information.
///
/// This trait can be implemented multiple times using different [`update
/// strategies`](MetadataUpdateStrategy).
///
/// An update strategy defines how the asset metadata is identified
/// and what [`Update`](MetadataUpdateStrategy::Update) type is used.
pub trait UpdateMetadata<AssetKind, Strategy: MetadataUpdateStrategy>:
	AssetDefinition<AssetKind>
{
	/// Update metadata information of the asset
	/// using the given `id`, the update `strategy`, and the `update` value.
	///
	/// The ID type is retrieved from the [`AssetDefinition`].
	fn update_metadata(
		id: &Self::Id,
		strategy: Strategy,
		update: Strategy::Update<'_>,
	) -> DispatchResult;
}

/// A strategy for use in the [`Create`] implementations.
///
/// The common "create" strategies are:
/// * [`Owned`](common_strategies::Owned)
/// * [`Adminable`](common_strategies::Adminable)
pub trait CreateStrategy {
	/// This type represents successful asset creation.
	/// It will be the return type of the [`Create::create`] function.
	type Success;
}

/// An ID assignment approach to use in the "create" strategies.
///
/// The common ID assignments are:
/// * [`AutoId`](common_strategies::AutoId)
/// * [`PredefinedId`](common_strategies::PredefinedId)
/// * [`DeriveIdFrom`](common_strategies::DeriveIdFrom)
pub trait IdAssignment {
	/// The reported ID type.
	///
	/// Examples:
	/// * [`AutoId`](common_strategies::AutoId) returns ID of the newly created asset
	/// * [`PredefinedId`](common_strategies::PredefinedId) returns `()` since the ID is already
	///   defined
	/// * [`DeriveIdFrom`](common_strategies::DeriveIdFrom) returns the derived ID
	type ReportedId;
}

/// A trait representing the ability of a certain asset kind to be created.
///
/// This trait can be implemented multiple times using different [`"create"
/// strategies`](CreateStrategy).
///
/// A create strategy defines all aspects of asset creation including how an asset ID is assigned.
pub trait Create<AssetKind, Strategy: CreateStrategy> {
	/// Create a new asset using the provided `strategy`.
	fn create(strategy: Strategy) -> Result<Strategy::Success, DispatchError>;
}

/// A strategy for use in the [`Transfer`] implementations.
///
/// The common transfer strategies are:
/// * [`JustTo`](common_strategies::JustTo)
/// * [`FromTo`](common_strategies::FromTo)
pub trait TransferStrategy {}

/// A trait representing the ability of a certain asset kind to be transferred.
///
/// This trait can be implemented multiple times using different [`transfer
/// strategies`](TransferStrategy).
///
/// A transfer strategy defines transfer parameters.
pub trait Transfer<AssetKind, Strategy: TransferStrategy>: AssetDefinition<AssetKind> {
	/// Transfer the asset identified by the given `id` using the provided `strategy`.
	///
	/// The ID type is retrieved from the [`AssetDefinition`].
	fn transfer(id: &Self::Id, strategy: Strategy) -> DispatchResult;
}

/// A strategy for use in the [`Destroy`] implementations.
///
/// The common destroy strategies are:
/// * [`JustDestroy`](common_strategies::JustDestroy)
/// * [`IfOwnedBy`](common_strategies::IfOwnedBy)
/// * [`WithWitness`](common_strategies::WithWitness)
/// * [`IfOwnedByWithWitness`](common_strategies::IfOwnedByWithWitness)
pub trait DestroyStrategy {
	/// This type represents successful asset destruction.
	/// It will be the return type of the [`Destroy::destroy`] function.
	type Success;
}

/// A trait representing the ability of a certain asset kind to be destroyed.
///
/// This trait can be implemented multiple times using different [`destroy
/// strategies`](DestroyStrategy).
///
/// A destroy strategy defines destroy parameters and the result value type.
pub trait Destroy<AssetKind, Strategy: DestroyStrategy>: AssetDefinition<AssetKind> {
	/// Destroy the asset identified by the given `id` using the provided `strategy`.
	///
	/// The ID type is retrieved from the [`AssetDefinition`].
	fn destroy(id: &Self::Id, strategy: Strategy) -> Result<Strategy::Success, DispatchError>;
}

/// This modules contains the common asset kinds.
pub mod common_asset_kinds {
	/// The `Class` asset kind is of assets that resemble class-like entities.
	/// For instance, a collection of non-fungible tokens is an asset of this kind.
	pub struct Class;

	/// The `Instance` asset kind represents assets resembling instances of something.
	/// For instance, a single non-fungible token is an asset of this kind.
	///
	/// An instance asset is not necessarily bound to a class.
	/// There could be "classless" instances.
	pub struct Instance;
}

/// This modules contains the common asset ops strategies.
pub mod common_strategies {
	use super::*;

	/// The `WithOrigin` is a strategy that accepts a runtime origin and the `Inner` strategy.
	///
	/// It is meant to be used when the origin check should be performed
	/// in addition to the `Inner` strategy.
	///
	/// The `WithOrigin` implements any strategy that the `Inner` implements.
	pub struct WithOrigin<RuntimeOrigin, Inner>(pub RuntimeOrigin, pub Inner);
	impl<RuntimeOrigin, Inner: MetadataInspectStrategy> MetadataInspectStrategy
		for WithOrigin<RuntimeOrigin, Inner>
	{
		type Value = Inner::Value;
	}
	impl<RuntimeOrigin, Inner: MetadataUpdateStrategy> MetadataUpdateStrategy
		for WithOrigin<RuntimeOrigin, Inner>
	{
		type Update<'u> = Inner::Update<'u>;
	}
	impl<RuntimeOrigin, Inner: CreateStrategy> CreateStrategy for WithOrigin<RuntimeOrigin, Inner> {
		type Success = Inner::Success;
	}
	impl<RuntimeOrigin, Inner: TransferStrategy> TransferStrategy for WithOrigin<RuntimeOrigin, Inner> {}
	impl<RuntimeOrigin, Inner: DestroyStrategy> DestroyStrategy for WithOrigin<RuntimeOrigin, Inner> {
		type Success = Inner::Success;
	}

	/// The `Bytes` strategy represents raw metadata bytes.
	/// It is both an [inspect](MetadataInspectStrategy) and [update](MetadataUpdateStrategy)
	/// metadata strategy.
	///
	/// * As the inspect strategy, it returns `Vec<u8>`.
	/// * As the update strategy, it accepts `Option<&[u8]>`, where `None` means data removal.
	///
	/// By default, the `Bytes` identifies a byte blob associated with the asset (the only one
	/// blob). However, a user can define several flavors of this strategy by supplying the `Flavor`
	/// type. The `Flavor` type can also contain additional data (like a byte key) to identify a
	/// certain byte data.
	pub struct Bytes<Flavor = ()>(pub Flavor);
	impl Default for Bytes<()> {
		fn default() -> Self {
			Self(())
		}
	}
	impl<Flavor> MetadataInspectStrategy for Bytes<Flavor> {
		type Value = Vec<u8>;
	}
	impl<Flavor> MetadataUpdateStrategy for Bytes<Flavor> {
		type Update<'u> = Option<&'u [u8]>;
	}

	/// The `Ownership` [inspect](MetadataInspectStrategy) metadata strategy allows getting the
	/// owner of an asset.
	pub struct Ownership<Owner>(PhantomData<Owner>);
	impl<Owner> Default for Ownership<Owner> {
		fn default() -> Self {
			Self(PhantomData)
		}
	}
	impl<Owner> MetadataInspectStrategy for Ownership<Owner> {
		type Value = Owner;
	}

	/// The `CanCreate` strategy represents the ability to create an asset.
	/// It is both an [inspect](MetadataInspectStrategy) and [update](MetadataUpdateStrategy)
	/// metadata strategy.
	///
	/// * As the inspect strategy, it returns `bool`.
	/// * As the update strategy is accepts `bool`.
	///
	/// By default, this strategy means the ability to create an asset "in general".
	/// However, a user can define several flavors of this strategy by supplying the `Flavor` type.
	/// The `Flavor` type can add more details to the strategy.
	/// For instance, "Can **a specific user** create an asset?".
	pub struct CanCreate<Flavor = ()>(pub Flavor);
	impl Default for CanCreate<()> {
		fn default() -> Self {
			Self(())
		}
	}
	impl<Flavor> MetadataInspectStrategy for CanCreate<Flavor> {
		type Value = bool;
	}
	impl<Flavor> MetadataUpdateStrategy for CanCreate<Flavor> {
		type Update<'u> = bool;
	}

	/// The `CanTransfer` strategy represents the ability to transfer an asset.
	/// It is both an [inspect](MetadataInspectStrategy) and [update](MetadataUpdateStrategy)
	/// metadata strategy.
	///
	/// * As the inspect strategy, it returns `bool`.
	/// * As the update strategy is accepts `bool`.
	///
	/// By default, this strategy means the ability to transfer an asset "in general".
	/// However, a user can define several flavors of this strategy by supplying the `Flavor` type.
	/// The `Flavor` type can add more details to the strategy.
	/// For instance, "Can **a specific user** transfer an asset of **another user**?".
	pub struct CanTransfer<Flavor = ()>(pub Flavor);
	impl Default for CanTransfer<()> {
		fn default() -> Self {
			Self(())
		}
	}
	impl<Flavor> MetadataInspectStrategy for CanTransfer<Flavor> {
		type Value = bool;
	}
	impl<Flavor> MetadataUpdateStrategy for CanTransfer<Flavor> {
		type Update<'u> = bool;
	}

	/// The `CanDestroy` strategy represents the ability to destroy an asset.
	/// It is both an [inspect](MetadataInspectStrategy) and [update](MetadataUpdateStrategy)
	/// metadata strategy.
	///
	/// * As the inspect strategy, it returns `bool`.
	/// * As the update strategy is accepts `bool`.
	///
	/// By default, this strategy means the ability to destroy an asset "in general".
	/// However, a user can define several flavors of this strategy by supplying the `Flavor` type.
	/// The `Flavor` type can add more details to the strategy.
	/// For instance, "Can **a specific user** destroy an asset of **another user**?".
	pub struct CanDestroy<Flavor = ()>(pub Flavor);
	impl Default for CanDestroy<()> {
		fn default() -> Self {
			Self(())
		}
	}
	impl<Flavor> MetadataInspectStrategy for CanDestroy<Flavor> {
		type Value = bool;
	}
	impl<Flavor> MetadataUpdateStrategy for CanDestroy<Flavor> {
		type Update<'u> = bool;
	}

	/// The `CanUpdateMetadata` strategy represents the ability to update the metadata of an asset.
	/// It is both an [inspect](MetadataInspectStrategy) and [update](MetadataUpdateStrategy)
	/// metadata strategy.
	///
	/// * As the inspect strategy, it returns `bool`.
	/// * As the update strategy is accepts `bool`.
	///
	/// By default, this strategy means the ability to update the metadata of an asset "in general".
	/// However, a user can define several flavors of this strategy by supplying the `Flavor` type.
	/// The `Flavor` type can add more details to the strategy.
	/// For instance, "Can **a specific user** update the metadata of an asset **under a certain
	/// key**?".
	pub struct CanUpdateMetadata<Flavor = ()>(pub Flavor);
	impl Default for CanUpdateMetadata<()> {
		fn default() -> Self {
			Self(())
		}
	}
	impl<Flavor> MetadataInspectStrategy for CanUpdateMetadata<Flavor> {
		type Value = bool;
	}
	impl<Flavor> MetadataUpdateStrategy for CanUpdateMetadata<Flavor> {
		type Update<'u> = bool;
	}

	/// The `AutoId` is an ID assignment approach intended to be used in [`"create"
	/// strategies`](CreateStrategy).
	///
	/// It accepts the `Id` type of the asset.
	/// The "create" strategy should report the value of type `Id` upon successful asset creation.
	pub struct AutoId<Id>(PhantomData<Id>);
	impl<Id> AutoId<Id> {
		pub fn new() -> Self {
			Self(PhantomData)
		}
	}
	impl<Id> IdAssignment for AutoId<Id> {
		type ReportedId = Id;
	}

	/// The `PredefinedId` is an ID assignment approach intended to be used in [`"create"
	/// strategies`](CreateStrategy).
	///
	/// It accepts a value of the `Id` type.
	/// The "create" strategy should use the provided ID value to create a new asset.
	pub struct PredefinedId<'a, Id>(pub &'a Id);
	impl<'a, Id> IdAssignment for PredefinedId<'a, Id> {
		type ReportedId = ();
	}

	/// The `DeriveIdFrom` is an ID assignment approach intended to be used in [`"create"
	/// strategies`](CreateStrategy).
	///
	/// It accepts the `ParentId` and the `ChildId`.
	/// The `ChildId` value should be computed by the "create" strategy using the `ParentId` value.
	///
	/// The "create" strategy should report the `ChildId` value upon successful asset creation.
	///
	/// An example of ID derivation is the creation of an NFT inside a collection using the
	/// collection ID. The child ID in this case is the full ID of the NFT.
	pub struct DeriveIdFrom<'a, ParentId, ChildId>(pub &'a ParentId, PhantomData<ChildId>);
	impl<'a, ParentId, ChildId> DeriveIdFrom<'a, ParentId, ChildId> {
		pub fn parent_id(primary_id: &'a ParentId) -> Self {
			Self(primary_id, PhantomData)
		}
	}
	impl<'a, ParentId, ChildId> IdAssignment for DeriveIdFrom<'a, ParentId, ChildId> {
		type ReportedId = ChildId;
	}

	/// The `Owned` is a [`"create" strategy`](CreateStrategy).
	///
	/// It accepts:
	/// * The [ID assignment](IdAssignment) approach
	/// * The `owner`
	/// * The optional `config`
	/// * The optional creation `witness`
	///
	/// The [`Success`](CreateStrategy::Success) will contain
	/// the [reported ID](IdAssignment::ReportedId) of the ID assignment approach.
	pub struct Owned<'a, Assignment: IdAssignment, Owner, Config = (), Witness = ()> {
		pub id_assignment: Assignment,
		pub owner: &'a Owner,
		pub config: &'a Config,
		pub witness: &'a Witness,
	}
	impl<'a, Assignment: IdAssignment, Owner> Owned<'a, Assignment, Owner, (), ()> {
		pub fn new(id_assignment: Assignment, owner: &'a Owner) -> Self {
			Self { id_assignment, owner, config: &(), witness: &() }
		}
	}
	impl<'a, Assignment: IdAssignment, Owner, Config> Owned<'a, Assignment, Owner, Config, ()> {
		pub fn new_configured(
			id_assignment: Assignment,
			owner: &'a Owner,
			config: &'a Config,
		) -> Self {
			Self { id_assignment, owner, config, witness: &() }
		}
	}
	impl<'a, Assignment: IdAssignment, Owner, Config, Witness> CreateStrategy
		for Owned<'a, Assignment, Owner, Config, Witness>
	{
		type Success = Assignment::ReportedId;
	}

	/// The `Adminable` is a [`"create" strategy`](CreateStrategy).
	///
	/// It accepts:
	/// * The [ID assignment](IdAssignment) approach
	/// * The `owner`
	/// * The `admin`
	/// * The optional `config`
	/// * The optional creation `witness`
	///
	/// The [`Success`](CreateStrategy::Success) will contain
	/// the [reported ID](IdAssignment::ReportedId) of the ID assignment approach.
	pub struct Adminable<'a, Assignment: IdAssignment, Account, Config = (), Witness = ()> {
		pub id_assignment: Assignment,
		pub owner: &'a Account,
		pub admin: &'a Account,
		pub config: &'a Config,
		pub witness: &'a Witness,
	}
	impl<'a, Assignment: IdAssignment, Account> Adminable<'a, Assignment, Account, (), ()> {
		pub fn new(id_assignment: Assignment, owner: &'a Account, admin: &'a Account) -> Self {
			Self { id_assignment, owner, admin, config: &(), witness: &() }
		}
	}
	impl<'a, Assignment: IdAssignment, Account, Config> Adminable<'a, Assignment, Account, Config, ()> {
		pub fn new_configured(
			id_assignment: Assignment,
			owner: &'a Account,
			admin: &'a Account,
			config: &'a Config,
		) -> Self {
			Self { id_assignment, owner, admin, config, witness: &() }
		}
	}
	impl<'a, Assignment: IdAssignment, Account, Config, Witness> CreateStrategy
		for Adminable<'a, Assignment, Account, Config, Witness>
	{
		type Success = Assignment::ReportedId;
	}

	/// The `JustTo` is a [`transfer strategy`](TransferStrategy).
	///
	/// It accepts the target of the transfer,
	/// i.e., who will become the asset's owner after the transfer.
	pub struct JustTo<'a, Owner>(pub &'a Owner);
	impl<'a, Owner> TransferStrategy for JustTo<'a, Owner> {}

	/// The `FromTo` is a [`transfer strategy`](TransferStrategy).
	///
	/// It accepts two parameters: `from` and `to` whom the asset should be transferred.
	pub struct FromTo<'a, Owner>(pub &'a Owner, pub &'a Owner);
	impl<'a, Owner> TransferStrategy for FromTo<'a, Owner> {}

	/// The `JustDestroy` is a [`destroy strategy`](DestroyStrategy).
	///
	/// It represents an "unchecked" destruction of the asset.
	pub struct JustDestroy;
	impl DestroyStrategy for JustDestroy {
		type Success = ();
	}

	/// The `IfOwnedBy` is a [`destroy strategy`](DestroyStrategy).
	///
	/// It accepts a possible owner of the asset.
	/// If the provided entity owns the asset, it will be destroyed.
	pub struct IfOwnedBy<'a, Owner>(pub &'a Owner);
	impl<'a, Owner> DestroyStrategy for IfOwnedBy<'a, Owner> {
		type Success = ();
	}

	/// The `WithWitness` is a [`destroy strategy`](DestroyStrategy).
	///
	/// It accepts a `Witness` to destroy an asset.
	/// It will also return a `Witness` value upon destruction.
	pub struct WithWitness<'a, Witness>(pub &'a Witness);
	impl<'a, Witness> DestroyStrategy for WithWitness<'a, Witness> {
		type Success = Witness;
	}

	/// The `IfOwnedByWithWitness` is a [`destroy strategy`](DestroyStrategy).
	///
	/// It is a combination of the [`IfOwnedBy`] and the [`WithWitness`] strategies.
	pub struct IfOwnedByWithWitness<'a, Owner, Witness> {
		pub owner: &'a Owner,
		pub witness: &'a Witness,
	}
	impl<'a, Owner, Witness> DestroyStrategy for IfOwnedByWithWitness<'a, Owner, Witness> {
		type Success = Witness;
	}
}
