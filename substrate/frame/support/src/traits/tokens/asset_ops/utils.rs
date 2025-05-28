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

use super::{
	common_strategies::{CheckOrigin, ConfigValueMarker, DeriveAndReportId, WithConfig},
	*,
};
use crate::{sp_runtime::traits::FallibleConvert, traits::EnsureOriginWithArg};

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
	M: FallibleConvert<IdA, IdB>,
	CreateOp: Create<DeriveAndReportId<IdB, ReportedId>>,
{
	fn create(
		id_assignment: DeriveAndReportId<IdA, ReportedId>,
	) -> Result<ReportedId, DispatchError> {
		let id_a = id_assignment.params;
		let id_b = M::fallible_convert(id_a)?;

		CreateOp::create(DeriveAndReportId::from(id_b))
	}
}
impl<Config, IdA, IdB, ReportedId, M, CreateOp>
	Create<WithConfig<Config, DeriveAndReportId<IdA, ReportedId>>> for MapId<IdA, IdB, M, CreateOp>
where
	Config: ConfigValueMarker,
	M: FallibleConvert<IdA, IdB>,
	CreateOp: Create<WithConfig<Config, DeriveAndReportId<IdB, ReportedId>>>,
{
	fn create(
		strategy: WithConfig<Config, DeriveAndReportId<IdA, ReportedId>>,
	) -> Result<ReportedId, DispatchError> {
		let WithConfig { config, extra: id_assignment } = strategy;
		let id_a = id_assignment.params;
		let id_b = M::fallible_convert(id_a)?;

		CreateOp::create(WithConfig::new(config, DeriveAndReportId::from(id_b)))
	}
}
impl<Id, M: FallibleConvert<Id, Op::Id>, Op: AssetDefinition> AssetDefinition
	for MapId<Id, Op::Id, M, Op>
{
	type Id = Id;
}
impl<Id, M, S, Op> Update<S> for MapId<Id, Op::Id, M, Op>
where
	M: FallibleConvert<Id, Op::Id>,
	S: UpdateStrategy,
	Op: Update<S>,
	Self::Id: Clone,
{
	fn update(
		id: &Self::Id,
		strategy: S,
		update_value: S::UpdateValue<'_>,
	) -> Result<S::Success, DispatchError> {
		let id = M::fallible_convert(id.clone())?;

		Op::update(&id, strategy, update_value)
	}
}
impl<Id, M, S, Op> Destroy<S> for MapId<Id, Op::Id, M, Op>
where
	M: FallibleConvert<Id, Op::Id>,
	S: DestroyStrategy,
	Op: Destroy<S>,
	Self::Id: Clone,
{
	fn destroy(id: &Self::Id, strategy: S) -> Result<S::Success, DispatchError> {
		let id = M::fallible_convert(id.clone())?;

		Op::destroy(&id, strategy)
	}
}
impl<Id, M, S, Op> Stash<S> for MapId<Id, Op::Id, M, Op>
where
	M: FallibleConvert<Id, Op::Id>,
	S: StashStrategy,
	Op: Stash<S>,
	Self::Id: Clone,
{
	fn stash(id: &Self::Id, strategy: S) -> Result<S::Success, DispatchError> {
		let id = M::fallible_convert(id.clone())?;

		Op::stash(&id, strategy)
	}
}
impl<Id, M, S, Op> Restore<S> for MapId<Id, Op::Id, M, Op>
where
	M: FallibleConvert<Id, Op::Id>,
	S: RestoreStrategy,
	Op: Restore<S>,
	Self::Id: Clone,
{
	fn restore(id: &Self::Id, strategy: S) -> Result<S::Success, DispatchError> {
		let id = M::fallible_convert(id.clone())?;

		Op::restore(&id, strategy)
	}
}

/// This adapter allows one to derive a [CreateStrategy] value from the ID derivation parameters
/// from the [DeriveAndReportId].
///
/// The instance will be created using the derived strategy.
pub struct DeriveStrategyThenCreate<Strategy, DeriveCfg, CreateOp>(
	PhantomData<(Strategy, DeriveCfg, CreateOp)>,
);
impl<Params, Strategy, DeriveCfg, CreateOp> Create<DeriveAndReportId<Params, Strategy::Success>>
	for DeriveStrategyThenCreate<Strategy, DeriveCfg, CreateOp>
where
	Strategy: CreateStrategy,
	DeriveCfg: FallibleConvert<Params, Strategy>,
	CreateOp: Create<Strategy>,
{
	fn create(
		id_assignment: DeriveAndReportId<Params, Strategy::Success>,
	) -> Result<Strategy::Success, DispatchError> {
		let strategy = DeriveCfg::fallible_convert(id_assignment.params)?;

		CreateOp::create(strategy)
	}
}

/// Unique instance operations that always fail.
///
/// Intended to be used to forbid certain actions.
pub struct AlwaysErrOps<Id>(PhantomData<Id>);
impl<Id> AssetDefinition for AlwaysErrOps<Id> {
	type Id = Id;
}
impl<Id, S: CreateStrategy> Create<S> for AlwaysErrOps<Id> {
	fn create(_strategy: S) -> Result<S::Success, DispatchError> {
		Err(DispatchError::BadOrigin)
	}
}
impl<Id, S: DestroyStrategy> Destroy<S> for AlwaysErrOps<Id> {
	fn destroy(_id: &Self::Id, _strategy: S) -> Result<S::Success, DispatchError> {
		Err(DispatchError::BadOrigin)
	}
}
