//! This modules contains the common asset ops strategies.

use super::*;
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;

/// The `CheckState` is a strategy that accepts an `Inspect` value and the `Inner` strategy.
///
/// It is meant to be used when the asset state check should be performed
/// in addition to the `Inner` strategy.
/// The inspected state must be equal to the provided value.
///
/// The `CheckState` implements all potentially state-mutating strategies that the `Inner`
/// implements.
pub struct CheckState<Inspect: InspectStrategy, Inner = Unchecked>(pub Inspect::Value, pub Inner);
impl<Inspect: InspectStrategy> CheckState<Inspect, Unchecked> {
	pub fn expect(expected: Inspect::Value) -> Self {
		Self(expected, Unchecked)
	}
}
impl<Inspect: InspectStrategy, Inner> CheckState<Inspect, Inner> {
	pub fn new(expected: Inspect::Value, inner: Inner) -> Self {
		Self(expected, inner)
	}
}
impl<Inspect: InspectStrategy, Inner: UpdateStrategy> UpdateStrategy
	for CheckState<Inspect, Inner>
{
	type Update<'u> = Inner::Update<'u>;
	type Success = Inner::Success;
}
impl<Inspect: InspectStrategy, Inner: CreateStrategy> CreateStrategy
	for CheckState<Inspect, Inner>
{
	type Success = Inner::Success;
}
impl<Inspect: InspectStrategy, Inner: TransferStrategy> TransferStrategy
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
/// in addition to the `Inner` strategy.
///
/// The `CheckOrigin` implements all potentially state-mutating strategies that the `Inner`
/// implements.
pub struct CheckOrigin<RuntimeOrigin, Inner = Unchecked>(pub RuntimeOrigin, pub Inner);
impl<RuntimeOrigin> CheckOrigin<RuntimeOrigin, Unchecked> {
	pub fn expect(origin: RuntimeOrigin) -> Self {
		Self(origin, Unchecked)
	}
}
impl<RuntimeOrigin, Inner: UpdateStrategy> UpdateStrategy for CheckOrigin<RuntimeOrigin, Inner> {
	type Update<'u> = Inner::Update<'u>;
	type Success = Inner::Success;
}
impl<RuntimeOrigin, Inner: CreateStrategy> CreateStrategy for CheckOrigin<RuntimeOrigin, Inner> {
	type Success = Inner::Success;
}
impl<RuntimeOrigin, Inner: TransferStrategy> TransferStrategy
	for CheckOrigin<RuntimeOrigin, Inner>
{
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

/// The Unchecked represents the simplest state-mutating strategy,
/// which doesn't require additional checks to perform the operation.
///
/// It can be used as the following strategies:
/// * [`destroy strategy`](DestroyStrategy)
/// * [`stash strategy`](StashStrategy)
/// * [`restore strategy`](RestoreStrategy)
pub struct Unchecked;
impl DestroyStrategy for Unchecked {
	type Success = ();
}
impl StashStrategy for Unchecked {
	type Success = ();
}
impl RestoreStrategy for Unchecked {
	type Success = ();
}

/// This is a simple transfer and restore strategy
/// which unconditionally transfers / restores the asset to the beneficiary account.
pub struct To<AccountId>(pub AccountId);
impl<AccountId> TransferStrategy for To<AccountId> {
	type Success = ();
}
impl<AccountId> RestoreStrategy for To<AccountId> {
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
	type Update<'u> = Option<&'u [u8]>;
	type Success = ();
}

/// The `Ownership` [inspect](InspectStrategy) strategy allows getting the
/// owner of an asset.
pub struct Ownership<Owner>(PhantomData<Owner>);
impl<Owner> Default for Ownership<Owner> {
	fn default() -> Self {
		Self(PhantomData)
	}
}
impl<Owner> InspectStrategy for Ownership<Owner> {
	type Value = Owner;
}

/// The operation implementation must check
/// if the given account owns the asset and act according to the inner strategy.
pub type IfOwnedBy<Owner, Inner = Unchecked> = CheckState<Ownership<Owner>, Inner>;

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
	type Update<'u> = bool;
	type Success = ();
}

/// The `CanTransfer` strategy represents the ability to transfer an asset.
/// It is both an [inspect](InspectStrategy) and [update](UpdateStrategy)
/// strategy.
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
impl<Condition> InspectStrategy for CanTransfer<Condition> {
	type Value = bool;
}
impl<Condition> UpdateStrategy for CanTransfer<Condition> {
	type Update<'u> = bool;
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
	type Update<'u> = bool;
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
	pub fn new(owner: Owner, id_assignment: Assignment) -> Self {
		Self { id_assignment, owner, config: (), witness: () }
	}
}
impl<Owner, Assignment: IdAssignment, Config> Owned<Owner, Assignment, Config, ()> {
	pub fn new_configured(owner: Owner, id_assignment: Assignment, config: Config) -> Self {
		Self { id_assignment, owner, config, witness: () }
	}
}
impl<Owner, Assignment: IdAssignment, Config, Witness> CreateStrategy
	for Owned<Owner, Assignment, Config, Witness>
{
	type Success = Assignment::ReportedId;
}

/// The `WithAdmin` is a [`"create" strategy`](CreateStrategy).
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
pub struct WithAdmin<Account, Assignment: IdAssignment, Config = (), Witness = ()> {
	pub owner: Account,
	pub admin: Account,
	pub id_assignment: Assignment,
	pub config: Config,
	pub witness: Witness,
}
impl<Account, Assignment: IdAssignment> WithAdmin<Account, Assignment, (), ()> {
	pub fn new(owner: Account, admin: Account, id_assignment: Assignment) -> Self {
		Self { id_assignment, owner, admin, config: (), witness: () }
	}
}
impl<Account, Assignment: IdAssignment, Config> WithAdmin<Account, Assignment, Config, ()> {
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
	for WithAdmin<Account, Assignment, Config, Witness>
{
	type Success = Assignment::ReportedId;
}

/// The `WithWitness` is a [`destroy strategy`](DestroyStrategy).
///
/// It accepts a `Witness` to destroy an asset.
/// It will also return a `Witness` value upon destruction.
pub struct WithWitness<Witness>(pub Witness);
impl<Witness> DestroyStrategy for WithWitness<Witness> {
	type Success = Witness;
}
