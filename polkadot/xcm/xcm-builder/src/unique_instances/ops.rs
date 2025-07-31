// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Utilities for redefining and auto-implementing the unique instances operations.

use core::marker::PhantomData;
use frame_support::traits::tokens::asset_ops::{
	common_strategies::{ChangeOwnerFrom, CheckState, ConfigValue, IfOwnedBy, Owner, WithConfig},
	AssetDefinition, Restore, RestoreStrategy, Stash, StashStrategy, Update, UpdateStrategy,
};
use sp_runtime::{
	traits::TypedGet,
	DispatchError, DispatchResult,
};

/// The `UniqueInstancesOps` is a tool for combining
/// different implementations of `Restore`, `Update`, and `Stash` operations
/// into one type to be used in [`UniqueInstancesAdapter`](super::adapter::UniqueInstancesAdapter).
///
/// All three operations must use the same ID for instances.
pub struct UniqueInstancesOps<RestoreOp, UpdateOp, StashOp>(
	PhantomData<(RestoreOp, UpdateOp, StashOp)>,
);
impl<RestoreOp, UpdateOp, StashOp> AssetDefinition
	for UniqueInstancesOps<RestoreOp, UpdateOp, StashOp>
where
	RestoreOp: AssetDefinition,
	UpdateOp: AssetDefinition<Id = RestoreOp::Id>,
	StashOp: AssetDefinition<Id = RestoreOp::Id>,
{
	type Id = RestoreOp::Id;
}
impl<Strategy, RestoreOp, UpdateOp, StashOp> Restore<Strategy>
	for UniqueInstancesOps<RestoreOp, UpdateOp, StashOp>
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
	for UniqueInstancesOps<RestoreOp, UpdateOp, StashOp>
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
	for UniqueInstancesOps<RestoreOp, UpdateOp, StashOp>
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

/// The `UniqueInstancesWithStashAccount` adds the `Stash` and `Restore` implementations to an NFT
/// engine capable of transferring a token from one account to another (i.e. implementing
/// `Update<ChangeOwnerFrom<AccountId>>`).
///
/// On stash, it will transfer the token from the current owner to the `StashAccount`.
/// On restore, it will transfer the token from the `StashAccount` to the given beneficiary.
pub struct UniqueInstancesWithStashAccount<StashAccount, UpdateOp>(
	PhantomData<(StashAccount, UpdateOp)>,
);
impl<StashAccount, UpdateOp: AssetDefinition> AssetDefinition
	for UniqueInstancesWithStashAccount<StashAccount, UpdateOp>
{
	type Id = UpdateOp::Id;
}
impl<StashAccount: TypedGet, UpdateOp> Update<ChangeOwnerFrom<StashAccount::Type>>
	for UniqueInstancesWithStashAccount<StashAccount, UpdateOp>
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
	for UniqueInstancesWithStashAccount<StashAccount, UpdateOp>
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
	for UniqueInstancesWithStashAccount<StashAccount, UpdateOp>
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
