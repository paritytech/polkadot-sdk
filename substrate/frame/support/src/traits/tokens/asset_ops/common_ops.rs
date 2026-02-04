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

use super::{common_strategies::*, *};
use crate::{
	dispatch::DispatchResult,
	sp_runtime::traits::Convert,
	traits::{misc::TypedGet, EnsureOriginWithArg},
};

/// The `UseEnsuredOrigin` is an adapter that implements all the asset ops implemented by the `Op`
/// with strategies augmented by the [CheckOrigin].
/// The Origin will be checked according to the provided `EnsureOrigin`.
pub struct UseEnsuredOrigin<EnsureOrigin, Op>(PhantomData<(EnsureOrigin, Op)>);
impl<O, E, S, Op> Create<CheckOrigin<O, S>> for UseEnsuredOrigin<E, Op>
where
	E: EnsureOriginWithArg<O, S>,
	S: CreateStrategy,
	Op: Create<S>,
{
	fn create(strategy: CheckOrigin<O, S>) -> Result<S::Success, DispatchError> {
		let CheckOrigin(origin, inner) = strategy;

		E::ensure_origin(origin, &inner)?;

		Op::create(inner)
	}
}
impl<E, Op: AssetDefinition> AssetDefinition for UseEnsuredOrigin<E, Op> {
	type Id = Op::Id;
}
impl<O, E, S, Op> Update<CheckOrigin<O, S>> for UseEnsuredOrigin<E, Op>
where
	E: EnsureOriginWithArg<O, S>,
	S: UpdateStrategy,
	Op: Update<S>,
{
	fn update(
		id: &Self::Id,
		strategy: CheckOrigin<O, S>,
		update_value: S::UpdateValue<'_>,
	) -> Result<S::Success, DispatchError> {
		let CheckOrigin(origin, inner) = strategy;

		E::ensure_origin(origin, &inner)?;

		Op::update(id, inner, update_value)
	}
}
impl<O, E, S, Op> Destroy<CheckOrigin<O, S>> for UseEnsuredOrigin<E, Op>
where
	E: EnsureOriginWithArg<O, S>,
	S: DestroyStrategy,
	Op: Destroy<S>,
{
	fn destroy(id: &Self::Id, strategy: CheckOrigin<O, S>) -> Result<S::Success, DispatchError> {
		let CheckOrigin(origin, inner) = strategy;

		E::ensure_origin(origin, &inner)?;

		Op::destroy(id, inner)
	}
}
impl<O, E, S, Op> Stash<CheckOrigin<O, S>> for UseEnsuredOrigin<E, Op>
where
	E: EnsureOriginWithArg<O, S>,
	S: StashStrategy,
	Op: Stash<S>,
{
	fn stash(id: &Self::Id, strategy: CheckOrigin<O, S>) -> Result<S::Success, DispatchError> {
		let CheckOrigin(origin, inner) = strategy;

		E::ensure_origin(origin, &inner)?;

		Op::stash(id, inner)
	}
}
impl<O, E, S, Op> Restore<CheckOrigin<O, S>> for UseEnsuredOrigin<E, Op>
where
	E: EnsureOriginWithArg<O, S>,
	S: RestoreStrategy,
	Op: Restore<S>,
{
	fn restore(id: &Self::Id, strategy: CheckOrigin<O, S>) -> Result<S::Success, DispatchError> {
		let CheckOrigin(origin, inner) = strategy;

		E::ensure_origin(origin, &inner)?;

		Op::restore(id, inner)
	}
}

/// The `MapId` is an adapter that implements all the asset ops implemented by the `Op`.
/// The adapter allows `IdA` to be used instead of `IdB` for every `Op` operation that uses `IdB` as
/// instance ID. The `IdA` value will be converted to `IdB` by the mapper `M` and supplied to the
/// `Op`'s corresponding operation implementation.
pub struct MapId<IdA, IdB, M, Op>(PhantomData<(IdA, IdB, M, Op)>);
impl<IdA, IdB, ReportedId, M, CreateOp> Create<DeriveAndReportId<IdA, ReportedId>>
	for MapId<IdA, IdB, M, CreateOp>
where
	M: Convert<IdA, Result<IdB, DispatchError>>,
	CreateOp: Create<DeriveAndReportId<IdB, ReportedId>>,
{
	fn create(
		id_assignment: DeriveAndReportId<IdA, ReportedId>,
	) -> Result<ReportedId, DispatchError> {
		let id_a = id_assignment.params;
		let id_b = M::convert(id_a)?;

		CreateOp::create(DeriveAndReportId::from(id_b))
	}
}
impl<Config, IdA, IdB, ReportedId, M, CreateOp>
	Create<WithConfig<Config, DeriveAndReportId<IdA, ReportedId>>> for MapId<IdA, IdB, M, CreateOp>
where
	Config: ConfigValueMarker,
	M: Convert<IdA, Result<IdB, DispatchError>>,
	CreateOp: Create<WithConfig<Config, DeriveAndReportId<IdB, ReportedId>>>,
{
	fn create(
		strategy: WithConfig<Config, DeriveAndReportId<IdA, ReportedId>>,
	) -> Result<ReportedId, DispatchError> {
		let WithConfig { config, extra: id_assignment } = strategy;
		let id_a = id_assignment.params;
		let id_b = M::convert(id_a)?;

		CreateOp::create(WithConfig::new(config, DeriveAndReportId::from(id_b)))
	}
}
impl<Id, M: Convert<Id, Result<Op::Id, DispatchError>>, Op: AssetDefinition> AssetDefinition
	for MapId<Id, Op::Id, M, Op>
{
	type Id = Id;
}
impl<Id, M, S, Op> Update<S> for MapId<Id, Op::Id, M, Op>
where
	M: Convert<Id, Result<Op::Id, DispatchError>>,
	S: UpdateStrategy,
	Op: Update<S>,
	Self::Id: Clone,
{
	fn update(
		id: &Self::Id,
		strategy: S,
		update_value: S::UpdateValue<'_>,
	) -> Result<S::Success, DispatchError> {
		let id = M::convert(id.clone())?;

		Op::update(&id, strategy, update_value)
	}
}
impl<Id, M, S, Op> Destroy<S> for MapId<Id, Op::Id, M, Op>
where
	M: Convert<Id, Result<Op::Id, DispatchError>>,
	S: DestroyStrategy,
	Op: Destroy<S>,
	Self::Id: Clone,
{
	fn destroy(id: &Self::Id, strategy: S) -> Result<S::Success, DispatchError> {
		let id = M::convert(id.clone())?;

		Op::destroy(&id, strategy)
	}
}
impl<Id, M, S, Op> Stash<S> for MapId<Id, Op::Id, M, Op>
where
	M: Convert<Id, Result<Op::Id, DispatchError>>,
	S: StashStrategy,
	Op: Stash<S>,
	Self::Id: Clone,
{
	fn stash(id: &Self::Id, strategy: S) -> Result<S::Success, DispatchError> {
		let id = M::convert(id.clone())?;

		Op::stash(&id, strategy)
	}
}
impl<Id, M, S, Op> Restore<S> for MapId<Id, Op::Id, M, Op>
where
	M: Convert<Id, Result<Op::Id, DispatchError>>,
	S: RestoreStrategy,
	Op: Restore<S>,
	Self::Id: Clone,
{
	fn restore(id: &Self::Id, strategy: S) -> Result<S::Success, DispatchError> {
		let id = M::convert(id.clone())?;

		Op::restore(&id, strategy)
	}
}

/// The `CombinedAssetOps` is a tool for combining
/// different implementations of `Restore`, `Update`, and `Stash` operations.
///
/// All three operations must use the same `AssetDefinition::Id`.
pub struct CombinedAssetOps<RestoreOp, UpdateOp, StashOp>(
	PhantomData<(RestoreOp, UpdateOp, StashOp)>,
);
impl<RestoreOp, UpdateOp, StashOp> AssetDefinition
	for CombinedAssetOps<RestoreOp, UpdateOp, StashOp>
where
	RestoreOp: AssetDefinition,
	UpdateOp: AssetDefinition<Id = RestoreOp::Id>,
	StashOp: AssetDefinition<Id = RestoreOp::Id>,
{
	type Id = RestoreOp::Id;
}
impl<Strategy, RestoreOp, UpdateOp, StashOp> Restore<Strategy>
	for CombinedAssetOps<RestoreOp, UpdateOp, StashOp>
where
	Strategy: RestoreStrategy,
	RestoreOp: Restore<Strategy>,
	UpdateOp: AssetDefinition<Id = RestoreOp::Id>,
	StashOp: AssetDefinition<Id = RestoreOp::Id>,
{
	fn restore(id: &Self::Id, strategy: Strategy) -> Result<Strategy::Success, DispatchError> {
		RestoreOp::restore(id, strategy)
	}
}
impl<Strategy, RestoreOp, UpdateOp, StashOp> Update<Strategy>
	for CombinedAssetOps<RestoreOp, UpdateOp, StashOp>
where
	Strategy: UpdateStrategy,
	UpdateOp: Update<Strategy>,
	RestoreOp: AssetDefinition,
	UpdateOp: AssetDefinition<Id = RestoreOp::Id>,
	StashOp: AssetDefinition<Id = RestoreOp::Id>,
{
	fn update(
		id: &Self::Id,
		strategy: Strategy,
		update: Strategy::UpdateValue<'_>,
	) -> Result<Strategy::Success, DispatchError> {
		UpdateOp::update(id, strategy, update)
	}
}
impl<Strategy, RestoreOp, UpdateOp, StashOp> Stash<Strategy>
	for CombinedAssetOps<RestoreOp, UpdateOp, StashOp>
where
	Strategy: StashStrategy,
	StashOp: Stash<Strategy>,
	RestoreOp: AssetDefinition,
	UpdateOp: AssetDefinition<Id = RestoreOp::Id>,
	StashOp: AssetDefinition<Id = RestoreOp::Id>,
{
	fn stash(id: &Self::Id, strategy: Strategy) -> Result<Strategy::Success, DispatchError> {
		StashOp::stash(id, strategy)
	}
}

/// The `StashAccountAssetOps` adds the `Stash` and `Restore` implementations to an NFT
/// engine capable of transferring a token from one account to another (i.e. implementing
/// `Update<ChangeOwnerFrom<AccountId>>`).
///
/// On stash, it will transfer the token from the current owner to the `StashAccount`.
/// On restore, it will transfer the token from the `StashAccount` to the given beneficiary.
pub struct StashAccountAssetOps<StashAccount, UpdateOp>(PhantomData<(StashAccount, UpdateOp)>);
impl<StashAccount, UpdateOp: AssetDefinition> AssetDefinition
	for StashAccountAssetOps<StashAccount, UpdateOp>
{
	type Id = UpdateOp::Id;
}
impl<StashAccount: TypedGet, UpdateOp> Update<ChangeOwnerFrom<StashAccount::Type>>
	for StashAccountAssetOps<StashAccount, UpdateOp>
where
	StashAccount::Type: 'static,
	UpdateOp: Update<ChangeOwnerFrom<StashAccount::Type>>,
{
	fn update(
		id: &Self::Id,
		strategy: ChangeOwnerFrom<StashAccount::Type>,
		update: &StashAccount::Type,
	) -> DispatchResult {
		UpdateOp::update(id, strategy, update)
	}
}
impl<StashAccount, UpdateOp> Restore<WithConfig<ConfigValue<Owner<StashAccount::Type>>>>
	for StashAccountAssetOps<StashAccount, UpdateOp>
where
	StashAccount: TypedGet,
	StashAccount::Type: 'static,
	UpdateOp: Update<ChangeOwnerFrom<StashAccount::Type>>,
{
	fn restore(
		id: &Self::Id,
		strategy: WithConfig<ConfigValue<Owner<StashAccount::Type>>>,
	) -> DispatchResult {
		let WithConfig { config: ConfigValue(beneficiary), .. } = strategy;

		UpdateOp::update(id, ChangeOwnerFrom::check(StashAccount::get()), &beneficiary)
	}
}
impl<StashAccount, UpdateOp> Stash<IfOwnedBy<StashAccount::Type>>
	for StashAccountAssetOps<StashAccount, UpdateOp>
where
	StashAccount: TypedGet,
	StashAccount::Type: 'static,
	UpdateOp: Update<ChangeOwnerFrom<StashAccount::Type>>,
{
	fn stash(id: &Self::Id, strategy: IfOwnedBy<StashAccount::Type>) -> DispatchResult {
		let CheckState(check_owner, ..) = strategy;

		UpdateOp::update(id, ChangeOwnerFrom::check(check_owner), &StashAccount::get())
	}
}

/// Unique instance operations that always fail.
///
/// Intended to be used to forbid certain actions.
pub struct DisabledOps<Id>(PhantomData<Id>);
impl<Id> AssetDefinition for DisabledOps<Id> {
	type Id = Id;
}
impl<Id, S: CreateStrategy> Create<S> for DisabledOps<Id> {
	fn create(_strategy: S) -> Result<S::Success, DispatchError> {
		Err(DispatchError::Other("Disabled"))
	}
}
impl<Id, S: DestroyStrategy> Destroy<S> for DisabledOps<Id> {
	fn destroy(_id: &Self::Id, _strategy: S) -> Result<S::Success, DispatchError> {
		Err(DispatchError::Other("Disabled"))
	}
}
