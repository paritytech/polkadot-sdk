//! This modules contains the common asset ops strategies.

use super::*;
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;

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
	type Success = Inner::Success;
}
impl<RuntimeOrigin, Inner: CreateStrategy> CreateStrategy for WithOrigin<RuntimeOrigin, Inner> {
	type Success = Inner::Success;
}
impl<RuntimeOrigin, Inner: TransferStrategy> TransferStrategy for WithOrigin<RuntimeOrigin, Inner> {
	type Success = Inner::Success;
}
impl<RuntimeOrigin, Inner: DestroyStrategy> DestroyStrategy for WithOrigin<RuntimeOrigin, Inner> {
	type Success = Inner::Success;
}
impl<RuntimeOrigin, Inner: StashStrategy> StashStrategy for WithOrigin<RuntimeOrigin, Inner> {
	type Success = Inner::Success;
}
impl<RuntimeOrigin, Inner: RestoreStrategy> RestoreStrategy for WithOrigin<RuntimeOrigin, Inner> {
	type Success = Inner::Success;
}

/// The JustDo represents the simplest strategy,
/// which doesn't require additional checks to perform the operation.
///
/// It can be used as the following strategies:
/// * [`transfer strategy`](TransferStrategy)
/// * [`destroy strategy`](DestroyStrategy)
/// * [`stash strategy`](StashStrategy)
/// * [`restore strategy`](RestoreStrategy)
///
/// It accepts whatever parameters are set in its generic argument.
/// For instance, for an unchecked transfer,
/// this strategy may take a reference to a beneficiary account.
pub struct JustDo<Params = ()>(pub Params);
impl Default for JustDo<()> {
	fn default() -> Self {
		Self(())
	}
}
impl<Params> TransferStrategy for JustDo<Params> {
	type Success = ();
}
impl<Params> DestroyStrategy for JustDo<Params> {
	type Success = ();
}
impl<Params> StashStrategy for JustDo<Params> {
	type Success = ();
}
impl<Params> RestoreStrategy for JustDo<Params> {
	type Success = ();
}

/// The `Bytes` strategy represents raw metadata bytes.
/// It is both an [inspect](MetadataInspectStrategy) and [update](MetadataUpdateStrategy)
/// metadata strategy.
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
impl<Request> MetadataInspectStrategy for Bytes<Request> {
	type Value = Vec<u8>;
}
impl<Request> MetadataUpdateStrategy for Bytes<Request> {
	type Update<'u> = Option<&'u [u8]>;
	type Success = ();
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
impl<Condition> MetadataInspectStrategy for CanCreate<Condition> {
	type Value = bool;
}
impl<Condition> MetadataUpdateStrategy for CanCreate<Condition> {
	type Update<'u> = bool;
	type Success = ();
}

/// The `CanTransfer` strategy represents the ability to transfer an asset.
/// It is both an [inspect](MetadataInspectStrategy) and [update](MetadataUpdateStrategy)
/// metadata strategy.
///
/// * As the inspect strategy, it returns `bool`.
/// * As the update strategy, it accepts `bool`.
///
/// By default, this strategy means the ability to transfer an asset "in general".
/// However, a user can define several variants of this strategy by supplying the `Condition`
/// type. Using the `Condition` value, we are formulating the question, "Can this be transferred
/// under the given condition?". For instance, "Can **a specific user** transfer an asset of
/// **another user**?".
pub struct CanTransfer<Condition = ()>(pub Condition);
impl Default for CanTransfer<()> {
	fn default() -> Self {
		Self(())
	}
}
impl<Condition> MetadataInspectStrategy for CanTransfer<Condition> {
	type Value = bool;
}
impl<Condition> MetadataUpdateStrategy for CanTransfer<Condition> {
	type Update<'u> = bool;
	type Success = ();
}

/// The `CanDestroy` strategy represents the ability to destroy an asset.
/// It is both an [inspect](MetadataInspectStrategy) and [update](MetadataUpdateStrategy)
/// metadata strategy.
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
impl<Condition> MetadataInspectStrategy for CanDestroy<Condition> {
	type Value = bool;
}
impl<Condition> MetadataUpdateStrategy for CanDestroy<Condition> {
	type Update<'u> = bool;
	type Success = ();
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
	type Success = ();
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

/// The `Owned` is a [`"create" strategy`](CreateStrategy).
///
/// It accepts:
/// * The `owner`
/// * The [ID assignment](IdAssignment) approach
/// * The optional `config`
/// * The optional creation `witness`
///
/// The [`Success`](CreateStrategy::Success) will contain
/// the [reported ID](IdAssignment::ReportedId) of the ID assignment approach.
#[derive(RuntimeDebug, PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct Owned<Owner, Assignment: IdAssignment, Config = (), Witness = ()> {
	pub owner: Owner,
	pub id_assignment: Assignment,
	pub config: Config,
	pub witness: Witness,
}
impl<Owner, Assignment: IdAssignment> Owned<Owner, Assignment, (), ()> {
	pub fn new(id_assignment: Assignment, owner: Owner) -> Self {
		Self { id_assignment, owner, config: (), witness: () }
	}
}
impl<Owner, Assignment: IdAssignment, Config> Owned<Owner, Assignment, Config, ()> {
	pub fn new_configured(id_assignment: Assignment, owner: Owner, config: Config) -> Self {
		Self { id_assignment, owner, config, witness: () }
	}
}
impl<Owner, Assignment: IdAssignment, Config, Witness> CreateStrategy
	for Owned<Owner, Assignment, Config, Witness>
{
	type Success = Assignment::ReportedId;
}

/// The `Adminable` is a [`"create" strategy`](CreateStrategy).
///
/// It accepts:
/// * The `owner`
/// * The `admin`
/// * The [ID assignment](IdAssignment) approach
/// * The optional `config`
/// * The optional creation `witness`
///
/// The [`Success`](CreateStrategy::Success) will contain
/// the [reported ID](IdAssignment::ReportedId) of the ID assignment approach.
#[derive(RuntimeDebug, PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct Adminable<Account, Assignment: IdAssignment, Config = (), Witness = ()> {
	pub owner: Account,
	pub admin: Account,
	pub id_assignment: Assignment,
	pub config: Config,
	pub witness: Witness,
}
impl<Account, Assignment: IdAssignment> Adminable<Account, Assignment, (), ()> {
	pub fn new(id_assignment: Assignment, owner: Account, admin: Account) -> Self {
		Self { id_assignment, owner, admin, config: (), witness: () }
	}
}
impl<Account, Assignment: IdAssignment, Config> Adminable<Account, Assignment, Config, ()> {
	pub fn new_configured(
		owner: Account,
		admin: Account,
		id_assignment: Assignment,
		config: Config,
	) -> Self {
		Self { id_assignment, owner, admin, config, witness: () }
	}
}
impl<Account, Assignment: IdAssignment, Config, Witness> CreateStrategy
	for Adminable<Account, Assignment, Config, Witness>
{
	type Success = Assignment::ReportedId;
}

/// The `FromTo` is a [`transfer strategy`](TransferStrategy).
///
/// It accepts two parameters: `from` and `to` whom the asset should be transferred.
pub struct FromTo<Owner>(pub Owner, pub Owner);
impl<Owner> TransferStrategy for FromTo<Owner> {
	type Success = ();
}

/// The `IfOwnedBy` is both a [`destroy strategy`](DestroyStrategy)
/// and a [`stash strategy`](StashStrategy).
///
/// It accepts a possible owner of the asset.
/// If the provided entity owns the asset, the corresponding operation will be performed.
pub struct IfOwnedBy<Owner>(pub Owner);
impl<Owner> DestroyStrategy for IfOwnedBy<Owner> {
	type Success = ();
}
impl<Owner> StashStrategy for IfOwnedBy<Owner> {
	type Success = ();
}

/// The `IfRestorable` is a [`restore strategy`](RestoreStrategy).
///
/// It accepts whatever parameters are set in its generic argument.
/// For instance, if an asset is restorable,
/// this strategy may reference a beneficiary account,
/// which should own the asset upon restoration.
pub struct IfRestorable<Params>(pub Params);
impl<Params> RestoreStrategy for IfRestorable<Params> {
	type Success = ();
}

/// The `WithWitness` is a [`destroy strategy`](DestroyStrategy).
///
/// It accepts a `Witness` to destroy an asset.
/// It will also return a `Witness` value upon destruction.
pub struct WithWitness<Witness>(pub Witness);
impl<Witness> DestroyStrategy for WithWitness<Witness> {
	type Success = Witness;
}

/// The `IfOwnedByWithWitness` is a [`destroy strategy`](DestroyStrategy).
///
/// It is a combination of the [`IfOwnedBy`] and the [`WithWitness`] strategies.
pub struct IfOwnedByWithWitness<Owner, Witness> {
	pub owner: Owner,
	pub witness: Witness,
}
impl<Owner, Witness> DestroyStrategy for IfOwnedByWithWitness<Owner, Witness> {
	type Success = Witness;
}
