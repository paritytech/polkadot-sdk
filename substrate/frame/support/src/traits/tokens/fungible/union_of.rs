// This file is part of Substrate.

// Copyright (Criterion) Parity Technologies (UK) Ltd.
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

//! Types to combine some `fungible::*` and `fungibles::*` implementations into one union
//! `fungibles::*` implementation.

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::traits::{
	tokens::{
		fungible, fungibles, AssetId, DepositConsequence, Fortitude, Imbalance as ImbalanceT,
		Precision, Preservation, Provenance, Restriction, WithdrawConsequence,
	},
	AccountTouch, ContainsPair,
};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::Convert,
	DispatchError, DispatchResult, Either,
	Either::{Left, Right},
	RuntimeDebug,
};
use sp_std::cmp::Ordering;

/// The `NativeOrWithId` enum classifies an asset as either `Native` to the current chain or as an
/// asset with a specific ID.
#[derive(Decode, Encode, Default, MaxEncodedLen, TypeInfo, Clone, RuntimeDebug, Eq)]
pub enum NativeOrWithId<AssetId>
where
	AssetId: Ord,
{
	/// Represents the native asset of the current chain.
	///
	/// E.g., DOT for the Polkadot Asset Hub.
	#[default]
	Native,
	/// Represents an asset identified by its underlying `AssetId`.
	WithId(AssetId),
}
impl<AssetId: Ord> From<AssetId> for NativeOrWithId<AssetId> {
	fn from(asset: AssetId) -> Self {
		Self::WithId(asset)
	}
}
impl<AssetId: Ord> Ord for NativeOrWithId<AssetId> {
	fn cmp(&self, other: &Self) -> Ordering {
		match (self, other) {
			(Self::Native, Self::Native) => Ordering::Equal,
			(Self::Native, Self::WithId(_)) => Ordering::Less,
			(Self::WithId(_), Self::Native) => Ordering::Greater,
			(Self::WithId(id1), Self::WithId(id2)) => <AssetId as Ord>::cmp(id1, id2),
		}
	}
}
impl<AssetId: Ord> PartialOrd for NativeOrWithId<AssetId> {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(<Self as Ord>::cmp(self, other))
	}
}
impl<AssetId: Ord> PartialEq for NativeOrWithId<AssetId> {
	fn eq(&self, other: &Self) -> bool {
		self.cmp(other) == Ordering::Equal
	}
}

/// Criterion for [`UnionOf`] where a set for [`NativeOrWithId::Native`] asset located from the left
/// and for [`NativeOrWithId::WithId`] from the right.
pub struct NativeFromLeft;
impl<AssetId: Ord> Convert<NativeOrWithId<AssetId>, Either<(), AssetId>> for NativeFromLeft {
	fn convert(asset: NativeOrWithId<AssetId>) -> Either<(), AssetId> {
		match asset {
			NativeOrWithId::Native => Either::Left(()),
			NativeOrWithId::WithId(id) => Either::Right(id),
		}
	}
}

/// Type to combine some `fungible::*` and `fungibles::*` implementations into one union
/// `fungibles::*` implementation.
///
/// ### Generic Parameters:
/// - `Left` is `fungible::*` implementation that is incorporated into the resulting union.
/// - `Right` is `fungibles::*` implementation that is incorporated into the resulting union.
/// - `Criterion` determines whether the `AssetKind` belongs to the `Left` or `Right` set.
/// - `AssetKind` is a superset type encompassing asset kinds from `Left` and `Right` sets.
/// - `AccountId` is an account identifier type.
pub struct UnionOf<Left, Right, Criterion, AssetKind, AccountId>(
	sp_std::marker::PhantomData<(Left, Right, Criterion, AssetKind, AccountId)>,
);

impl<
		Left: fungible::Inspect<AccountId>,
		Right: fungibles::Inspect<AccountId, Balance = Left::Balance>,
		Criterion: Convert<AssetKind, Either<(), Right::AssetId>>,
		AssetKind: AssetId,
		AccountId,
	> fungibles::Inspect<AccountId> for UnionOf<Left, Right, Criterion, AssetKind, AccountId>
{
	type AssetId = AssetKind;
	type Balance = Left::Balance;

	fn total_issuance(asset: Self::AssetId) -> Self::Balance {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::Inspect<AccountId>>::total_issuance(),
			Right(a) => <Right as fungibles::Inspect<AccountId>>::total_issuance(a),
		}
	}
	fn active_issuance(asset: Self::AssetId) -> Self::Balance {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::Inspect<AccountId>>::active_issuance(),
			Right(a) => <Right as fungibles::Inspect<AccountId>>::active_issuance(a),
		}
	}
	fn minimum_balance(asset: Self::AssetId) -> Self::Balance {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::Inspect<AccountId>>::minimum_balance(),
			Right(a) => <Right as fungibles::Inspect<AccountId>>::minimum_balance(a),
		}
	}
	fn balance(asset: Self::AssetId, who: &AccountId) -> Self::Balance {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::Inspect<AccountId>>::balance(who),
			Right(a) => <Right as fungibles::Inspect<AccountId>>::balance(a, who),
		}
	}
	fn total_balance(asset: Self::AssetId, who: &AccountId) -> Self::Balance {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::Inspect<AccountId>>::total_balance(who),
			Right(a) => <Right as fungibles::Inspect<AccountId>>::total_balance(a, who),
		}
	}
	fn reducible_balance(
		asset: Self::AssetId,
		who: &AccountId,
		preservation: Preservation,
		force: Fortitude,
	) -> Self::Balance {
		match Criterion::convert(asset) {
			Left(()) =>
				<Left as fungible::Inspect<AccountId>>::reducible_balance(who, preservation, force),
			Right(a) => <Right as fungibles::Inspect<AccountId>>::reducible_balance(
				a,
				who,
				preservation,
				force,
			),
		}
	}
	fn can_deposit(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
		provenance: Provenance,
	) -> DepositConsequence {
		match Criterion::convert(asset) {
			Left(()) =>
				<Left as fungible::Inspect<AccountId>>::can_deposit(who, amount, provenance),
			Right(a) =>
				<Right as fungibles::Inspect<AccountId>>::can_deposit(a, who, amount, provenance),
		}
	}
	fn can_withdraw(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
	) -> WithdrawConsequence<Self::Balance> {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::Inspect<AccountId>>::can_withdraw(who, amount),
			Right(a) => <Right as fungibles::Inspect<AccountId>>::can_withdraw(a, who, amount),
		}
	}
	fn asset_exists(asset: Self::AssetId) -> bool {
		match Criterion::convert(asset) {
			Left(()) => true,
			Right(a) => <Right as fungibles::Inspect<AccountId>>::asset_exists(a),
		}
	}
}

impl<
		Left: fungible::InspectHold<AccountId>,
		Right: fungibles::InspectHold<AccountId, Balance = Left::Balance, Reason = Left::Reason>,
		Criterion: Convert<AssetKind, Either<(), Right::AssetId>>,
		AssetKind: AssetId,
		AccountId,
	> fungibles::InspectHold<AccountId> for UnionOf<Left, Right, Criterion, AssetKind, AccountId>
{
	type Reason = Left::Reason;

	fn reducible_total_balance_on_hold(
		asset: Self::AssetId,
		who: &AccountId,
		force: Fortitude,
	) -> Self::Balance {
		match Criterion::convert(asset) {
			Left(()) =>
				<Left as fungible::InspectHold<AccountId>>::reducible_total_balance_on_hold(
					who, force,
				),
			Right(a) =>
				<Right as fungibles::InspectHold<AccountId>>::reducible_total_balance_on_hold(
					a, who, force,
				),
		}
	}
	fn hold_available(asset: Self::AssetId, reason: &Self::Reason, who: &AccountId) -> bool {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::InspectHold<AccountId>>::hold_available(reason, who),
			Right(a) =>
				<Right as fungibles::InspectHold<AccountId>>::hold_available(a, reason, who),
		}
	}
	fn total_balance_on_hold(asset: Self::AssetId, who: &AccountId) -> Self::Balance {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::InspectHold<AccountId>>::total_balance_on_hold(who),
			Right(a) => <Right as fungibles::InspectHold<AccountId>>::total_balance_on_hold(a, who),
		}
	}
	fn balance_on_hold(
		asset: Self::AssetId,
		reason: &Self::Reason,
		who: &AccountId,
	) -> Self::Balance {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::InspectHold<AccountId>>::balance_on_hold(reason, who),
			Right(a) =>
				<Right as fungibles::InspectHold<AccountId>>::balance_on_hold(a, reason, who),
		}
	}
	fn can_hold(
		asset: Self::AssetId,
		reason: &Self::Reason,
		who: &AccountId,
		amount: Self::Balance,
	) -> bool {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::InspectHold<AccountId>>::can_hold(reason, who, amount),
			Right(a) =>
				<Right as fungibles::InspectHold<AccountId>>::can_hold(a, reason, who, amount),
		}
	}
}

impl<
		Left: fungible::InspectFreeze<AccountId>,
		Right: fungibles::InspectFreeze<AccountId, Balance = Left::Balance, Id = Left::Id>,
		Criterion: Convert<AssetKind, Either<(), Right::AssetId>>,
		AssetKind: AssetId,
		AccountId,
	> fungibles::InspectFreeze<AccountId> for UnionOf<Left, Right, Criterion, AssetKind, AccountId>
{
	type Id = Left::Id;
	fn balance_frozen(asset: Self::AssetId, id: &Self::Id, who: &AccountId) -> Self::Balance {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::InspectFreeze<AccountId>>::balance_frozen(id, who),
			Right(a) => <Right as fungibles::InspectFreeze<AccountId>>::balance_frozen(a, id, who),
		}
	}
	fn balance_freezable(asset: Self::AssetId, who: &AccountId) -> Self::Balance {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::InspectFreeze<AccountId>>::balance_freezable(who),
			Right(a) => <Right as fungibles::InspectFreeze<AccountId>>::balance_freezable(a, who),
		}
	}
	fn can_freeze(asset: Self::AssetId, id: &Self::Id, who: &AccountId) -> bool {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::InspectFreeze<AccountId>>::can_freeze(id, who),
			Right(a) => <Right as fungibles::InspectFreeze<AccountId>>::can_freeze(a, id, who),
		}
	}
}

impl<
		Left: fungible::Unbalanced<AccountId>,
		Right: fungibles::Unbalanced<AccountId, Balance = Left::Balance>,
		Criterion: Convert<AssetKind, Either<(), Right::AssetId>>,
		AssetKind: AssetId,
		AccountId,
	> fungibles::Unbalanced<AccountId> for UnionOf<Left, Right, Criterion, AssetKind, AccountId>
{
	fn handle_dust(dust: fungibles::Dust<AccountId, Self>)
	where
		Self: Sized,
	{
		match Criterion::convert(dust.0) {
			Left(()) =>
				<Left as fungible::Unbalanced<AccountId>>::handle_dust(fungible::Dust(dust.1)),
			Right(a) =>
				<Right as fungibles::Unbalanced<AccountId>>::handle_dust(fungibles::Dust(a, dust.1)),
		}
	}
	fn write_balance(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
	) -> Result<Option<Self::Balance>, DispatchError> {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::Unbalanced<AccountId>>::write_balance(who, amount),
			Right(a) => <Right as fungibles::Unbalanced<AccountId>>::write_balance(a, who, amount),
		}
	}
	fn set_total_issuance(asset: Self::AssetId, amount: Self::Balance) -> () {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::Unbalanced<AccountId>>::set_total_issuance(amount),
			Right(a) => <Right as fungibles::Unbalanced<AccountId>>::set_total_issuance(a, amount),
		}
	}
	fn decrease_balance(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
		precision: Precision,
		preservation: Preservation,
		force: Fortitude,
	) -> Result<Self::Balance, DispatchError> {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::Unbalanced<AccountId>>::decrease_balance(
				who,
				amount,
				precision,
				preservation,
				force,
			),
			Right(a) => <Right as fungibles::Unbalanced<AccountId>>::decrease_balance(
				a,
				who,
				amount,
				precision,
				preservation,
				force,
			),
		}
	}
	fn increase_balance(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
		precision: Precision,
	) -> Result<Self::Balance, DispatchError> {
		match Criterion::convert(asset) {
			Left(()) =>
				<Left as fungible::Unbalanced<AccountId>>::increase_balance(who, amount, precision),
			Right(a) => <Right as fungibles::Unbalanced<AccountId>>::increase_balance(
				a, who, amount, precision,
			),
		}
	}
}

impl<
		Left: fungible::UnbalancedHold<AccountId>,
		Right: fungibles::UnbalancedHold<AccountId, Balance = Left::Balance, Reason = Left::Reason>,
		Criterion: Convert<AssetKind, Either<(), Right::AssetId>>,
		AssetKind: AssetId,
		AccountId,
	> fungibles::UnbalancedHold<AccountId> for UnionOf<Left, Right, Criterion, AssetKind, AccountId>
{
	fn set_balance_on_hold(
		asset: Self::AssetId,
		reason: &Self::Reason,
		who: &AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::UnbalancedHold<AccountId>>::set_balance_on_hold(
				reason, who, amount,
			),
			Right(a) => <Right as fungibles::UnbalancedHold<AccountId>>::set_balance_on_hold(
				a, reason, who, amount,
			),
		}
	}
	fn decrease_balance_on_hold(
		asset: Self::AssetId,
		reason: &Self::Reason,
		who: &AccountId,
		amount: Self::Balance,
		precision: Precision,
	) -> Result<Self::Balance, DispatchError> {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::UnbalancedHold<AccountId>>::decrease_balance_on_hold(
				reason, who, amount, precision,
			),
			Right(a) => <Right as fungibles::UnbalancedHold<AccountId>>::decrease_balance_on_hold(
				a, reason, who, amount, precision,
			),
		}
	}
	fn increase_balance_on_hold(
		asset: Self::AssetId,
		reason: &Self::Reason,
		who: &AccountId,
		amount: Self::Balance,
		precision: Precision,
	) -> Result<Self::Balance, DispatchError> {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::UnbalancedHold<AccountId>>::increase_balance_on_hold(
				reason, who, amount, precision,
			),
			Right(a) => <Right as fungibles::UnbalancedHold<AccountId>>::increase_balance_on_hold(
				a, reason, who, amount, precision,
			),
		}
	}
}

impl<
		Left: fungible::Mutate<AccountId>,
		Right: fungibles::Mutate<AccountId, Balance = Left::Balance>,
		Criterion: Convert<AssetKind, Either<(), Right::AssetId>>,
		AssetKind: AssetId,
		AccountId,
	> fungibles::Mutate<AccountId> for UnionOf<Left, Right, Criterion, AssetKind, AccountId>
{
	fn mint_into(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
	) -> Result<Self::Balance, DispatchError> {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::Mutate<AccountId>>::mint_into(who, amount),
			Right(a) => <Right as fungibles::Mutate<AccountId>>::mint_into(a, who, amount),
		}
	}
	fn burn_from(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
		precision: Precision,
		force: Fortitude,
	) -> Result<Self::Balance, DispatchError> {
		match Criterion::convert(asset) {
			Left(()) =>
				<Left as fungible::Mutate<AccountId>>::burn_from(who, amount, precision, force),
			Right(a) =>
				<Right as fungibles::Mutate<AccountId>>::burn_from(a, who, amount, precision, force),
		}
	}
	fn shelve(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
	) -> Result<Self::Balance, DispatchError> {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::Mutate<AccountId>>::shelve(who, amount),
			Right(a) => <Right as fungibles::Mutate<AccountId>>::shelve(a, who, amount),
		}
	}
	fn restore(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
	) -> Result<Self::Balance, DispatchError> {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::Mutate<AccountId>>::restore(who, amount),
			Right(a) => <Right as fungibles::Mutate<AccountId>>::restore(a, who, amount),
		}
	}
	fn transfer(
		asset: Self::AssetId,
		source: &AccountId,
		dest: &AccountId,
		amount: Self::Balance,
		preservation: Preservation,
	) -> Result<Self::Balance, DispatchError> {
		match Criterion::convert(asset) {
			Left(()) =>
				<Left as fungible::Mutate<AccountId>>::transfer(source, dest, amount, preservation),
			Right(a) => <Right as fungibles::Mutate<AccountId>>::transfer(
				a,
				source,
				dest,
				amount,
				preservation,
			),
		}
	}

	fn set_balance(asset: Self::AssetId, who: &AccountId, amount: Self::Balance) -> Self::Balance {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::Mutate<AccountId>>::set_balance(who, amount),
			Right(a) => <Right as fungibles::Mutate<AccountId>>::set_balance(a, who, amount),
		}
	}
}

impl<
		Left: fungible::MutateHold<AccountId>,
		Right: fungibles::MutateHold<AccountId, Balance = Left::Balance, Reason = Left::Reason>,
		Criterion: Convert<AssetKind, Either<(), Right::AssetId>>,
		AssetKind: AssetId,
		AccountId,
	> fungibles::MutateHold<AccountId> for UnionOf<Left, Right, Criterion, AssetKind, AccountId>
{
	fn hold(
		asset: Self::AssetId,
		reason: &Self::Reason,
		who: &AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::MutateHold<AccountId>>::hold(reason, who, amount),
			Right(a) => <Right as fungibles::MutateHold<AccountId>>::hold(a, reason, who, amount),
		}
	}
	fn release(
		asset: Self::AssetId,
		reason: &Self::Reason,
		who: &AccountId,
		amount: Self::Balance,
		precision: Precision,
	) -> Result<Self::Balance, DispatchError> {
		match Criterion::convert(asset) {
			Left(()) =>
				<Left as fungible::MutateHold<AccountId>>::release(reason, who, amount, precision),
			Right(a) => <Right as fungibles::MutateHold<AccountId>>::release(
				a, reason, who, amount, precision,
			),
		}
	}
	fn burn_held(
		asset: Self::AssetId,
		reason: &Self::Reason,
		who: &AccountId,
		amount: Self::Balance,
		precision: Precision,
		force: Fortitude,
	) -> Result<Self::Balance, DispatchError> {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::MutateHold<AccountId>>::burn_held(
				reason, who, amount, precision, force,
			),
			Right(a) => <Right as fungibles::MutateHold<AccountId>>::burn_held(
				a, reason, who, amount, precision, force,
			),
		}
	}
	fn transfer_on_hold(
		asset: Self::AssetId,
		reason: &Self::Reason,
		source: &AccountId,
		dest: &AccountId,
		amount: Self::Balance,
		precision: Precision,
		mode: Restriction,
		force: Fortitude,
	) -> Result<Self::Balance, DispatchError> {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::MutateHold<AccountId>>::transfer_on_hold(
				reason, source, dest, amount, precision, mode, force,
			),
			Right(a) => <Right as fungibles::MutateHold<AccountId>>::transfer_on_hold(
				a, reason, source, dest, amount, precision, mode, force,
			),
		}
	}
	fn transfer_and_hold(
		asset: Self::AssetId,
		reason: &Self::Reason,
		source: &AccountId,
		dest: &AccountId,
		amount: Self::Balance,
		precision: Precision,
		preservation: Preservation,
		force: Fortitude,
	) -> Result<Self::Balance, DispatchError> {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::MutateHold<AccountId>>::transfer_and_hold(
				reason,
				source,
				dest,
				amount,
				precision,
				preservation,
				force,
			),
			Right(a) => <Right as fungibles::MutateHold<AccountId>>::transfer_and_hold(
				a,
				reason,
				source,
				dest,
				amount,
				precision,
				preservation,
				force,
			),
		}
	}
}

impl<
		Left: fungible::MutateFreeze<AccountId>,
		Right: fungibles::MutateFreeze<AccountId, Balance = Left::Balance, Id = Left::Id>,
		Criterion: Convert<AssetKind, Either<(), Right::AssetId>>,
		AssetKind: AssetId,
		AccountId,
	> fungibles::MutateFreeze<AccountId> for UnionOf<Left, Right, Criterion, AssetKind, AccountId>
{
	fn set_freeze(
		asset: Self::AssetId,
		id: &Self::Id,
		who: &AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::MutateFreeze<AccountId>>::set_freeze(id, who, amount),
			Right(a) =>
				<Right as fungibles::MutateFreeze<AccountId>>::set_freeze(a, id, who, amount),
		}
	}
	fn extend_freeze(
		asset: Self::AssetId,
		id: &Self::Id,
		who: &AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::MutateFreeze<AccountId>>::extend_freeze(id, who, amount),
			Right(a) =>
				<Right as fungibles::MutateFreeze<AccountId>>::extend_freeze(a, id, who, amount),
		}
	}
	fn thaw(asset: Self::AssetId, id: &Self::Id, who: &AccountId) -> DispatchResult {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::MutateFreeze<AccountId>>::thaw(id, who),
			Right(a) => <Right as fungibles::MutateFreeze<AccountId>>::thaw(a, id, who),
		}
	}
}

pub struct ConvertImbalanceDropHandler<
	Left,
	Right,
	Criterion,
	AssetKind,
	Balance,
	AssetId,
	AccountId,
>(sp_std::marker::PhantomData<(Left, Right, Criterion, AssetKind, Balance, AssetId, AccountId)>);

impl<
		Left: fungible::HandleImbalanceDrop<Balance>,
		Right: fungibles::HandleImbalanceDrop<AssetId, Balance>,
		Criterion: Convert<AssetKind, Either<(), AssetId>>,
		AssetKind,
		Balance,
		AssetId,
		AccountId,
	> fungibles::HandleImbalanceDrop<AssetKind, Balance>
	for ConvertImbalanceDropHandler<Left, Right, Criterion, AssetKind, Balance, AssetId, AccountId>
{
	fn handle(asset: AssetKind, amount: Balance) {
		match Criterion::convert(asset) {
			Left(()) => Left::handle(amount),
			Right(a) => Right::handle(a, amount),
		}
	}
}

impl<
		Left: fungible::Balanced<AccountId>,
		Right: fungibles::Balanced<AccountId, Balance = Left::Balance>,
		Criterion: Convert<AssetKind, Either<(), Right::AssetId>>,
		AssetKind: AssetId,
		AccountId,
	> fungibles::Balanced<AccountId> for UnionOf<Left, Right, Criterion, AssetKind, AccountId>
{
	type OnDropDebt = ConvertImbalanceDropHandler<
		Left::OnDropDebt,
		Right::OnDropDebt,
		Criterion,
		AssetKind,
		Left::Balance,
		Right::AssetId,
		AccountId,
	>;
	type OnDropCredit = ConvertImbalanceDropHandler<
		Left::OnDropCredit,
		Right::OnDropCredit,
		Criterion,
		AssetKind,
		Left::Balance,
		Right::AssetId,
		AccountId,
	>;

	fn deposit(
		asset: Self::AssetId,
		who: &AccountId,
		value: Self::Balance,
		precision: Precision,
	) -> Result<fungibles::Debt<AccountId, Self>, DispatchError> {
		match Criterion::convert(asset.clone()) {
			Left(()) => <Left as fungible::Balanced<AccountId>>::deposit(who, value, precision)
				.map(|inner_debt| {
					let debt = fungibles::Imbalance::new(asset, inner_debt.peek());
					fungible::Imbalance::forget(inner_debt);
					debt
				}),
			Right(a) => <Right as fungibles::Balanced<AccountId>>::deposit(
				a, who, value, precision,
			)
			.map(|inner_debt| {
				let debt = fungibles::Imbalance::new(asset, inner_debt.peek());
				fungibles::Imbalance::forget(inner_debt);
				debt
			}),
		}
	}
	fn issue(asset: Self::AssetId, amount: Self::Balance) -> fungibles::Credit<AccountId, Self> {
		match Criterion::convert(asset.clone()) {
			Left(()) => {
				let inner_credit = <Left as fungible::Balanced<AccountId>>::issue(amount);
				let credit = fungibles::Imbalance::new(asset, inner_credit.peek());
				fungible::Imbalance::forget(inner_credit);
				credit
			},
			Right(a) => {
				let inner_credit = <Right as fungibles::Balanced<AccountId>>::issue(a, amount);
				let credit = fungibles::Imbalance::new(asset, inner_credit.peek());
				fungibles::Imbalance::forget(inner_credit);
				credit
			},
		}
	}
	fn pair(
		asset: Self::AssetId,
		amount: Self::Balance,
	) -> (fungibles::Debt<AccountId, Self>, fungibles::Credit<AccountId, Self>) {
		match Criterion::convert(asset.clone()) {
			Left(()) => {
				let (inner_a, inner_b) = <Left as fungible::Balanced<AccountId>>::pair(amount);
				let (a, b) = (
					fungibles::Imbalance::new(asset.clone(), inner_a.peek()),
					fungibles::Imbalance::new(asset, inner_b.peek()),
				);
				fungible::Imbalance::forget(inner_a);
				fungible::Imbalance::forget(inner_b);
				(a, b)
			},
			Right(a) => {
				let (inner_a, inner_b) = <Right as fungibles::Balanced<AccountId>>::pair(a, amount);
				let (a, b) = (
					fungibles::Imbalance::new(asset.clone(), inner_a.peek()),
					fungibles::Imbalance::new(asset, inner_b.peek()),
				);
				fungibles::Imbalance::forget(inner_a);
				fungibles::Imbalance::forget(inner_b);
				(a, b)
			},
		}
	}
	fn rescind(asset: Self::AssetId, amount: Self::Balance) -> fungibles::Debt<AccountId, Self> {
		match Criterion::convert(asset.clone()) {
			Left(()) => {
				let inner_debt = <Left as fungible::Balanced<AccountId>>::rescind(amount);
				let debt = fungibles::Imbalance::new(asset, inner_debt.peek());
				fungible::Imbalance::forget(inner_debt);
				debt
			},
			Right(a) => {
				let inner_debt = <Right as fungibles::Balanced<AccountId>>::rescind(a, amount);
				let debt = fungibles::Imbalance::new(asset, inner_debt.peek());
				fungibles::Imbalance::forget(inner_debt);
				debt
			},
		}
	}
	fn resolve(
		who: &AccountId,
		credit: fungibles::Credit<AccountId, Self>,
	) -> Result<(), fungibles::Credit<AccountId, Self>> {
		let asset = credit.asset();
		match Criterion::convert(asset.clone()) {
			Left(()) => {
				let inner_credit = fungible::Imbalance::new(credit.peek());
				fungibles::Imbalance::forget(credit);
				<Left as fungible::Balanced<AccountId>>::resolve(who, inner_credit).map_err(
					|inner_credit| {
						let credit = fungibles::Imbalance::new(asset, inner_credit.peek());
						fungible::Imbalance::forget(inner_credit);
						credit
					},
				)
			},
			Right(a) => {
				let inner_credit = fungibles::Imbalance::new(a, credit.peek());
				fungibles::Imbalance::forget(credit);
				<Right as fungibles::Balanced<AccountId>>::resolve(who, inner_credit).map_err(
					|inner_credit| {
						let credit = fungibles::Imbalance::new(asset, inner_credit.peek());
						fungibles::Imbalance::forget(inner_credit);
						credit
					},
				)
			},
		}
	}
	fn settle(
		who: &AccountId,
		debt: fungibles::Debt<AccountId, Self>,
		preservation: Preservation,
	) -> Result<fungibles::Credit<AccountId, Self>, fungibles::Debt<AccountId, Self>> {
		let asset = debt.asset();
		match Criterion::convert(asset.clone()) {
			Left(()) => {
				let inner_debt = fungible::Imbalance::new(debt.peek());
				fungibles::Imbalance::forget(debt);
				match <Left as fungible::Balanced<AccountId>>::settle(who, inner_debt, preservation)
				{
					Ok(inner_credit) => {
						let credit = fungibles::Imbalance::new(asset, inner_credit.peek());
						fungible::Imbalance::forget(inner_credit);
						Ok(credit)
					},
					Err(inner_debt) => {
						let debt = fungibles::Imbalance::new(asset, inner_debt.peek());
						fungible::Imbalance::forget(inner_debt);
						Err(debt)
					},
				}
			},
			Right(a) => {
				let inner_debt = fungibles::Imbalance::new(a, debt.peek());
				fungibles::Imbalance::forget(debt);
				match <Right as fungibles::Balanced<AccountId>>::settle(
					who,
					inner_debt,
					preservation,
				) {
					Ok(inner_credit) => {
						let credit = fungibles::Imbalance::new(asset, inner_credit.peek());
						fungibles::Imbalance::forget(inner_credit);
						Ok(credit)
					},
					Err(inner_debt) => {
						let debt = fungibles::Imbalance::new(asset, inner_debt.peek());
						fungibles::Imbalance::forget(inner_debt);
						Err(debt)
					},
				}
			},
		}
	}
	fn withdraw(
		asset: Self::AssetId,
		who: &AccountId,
		value: Self::Balance,
		precision: Precision,
		preservation: Preservation,
		force: Fortitude,
	) -> Result<fungibles::Credit<AccountId, Self>, DispatchError> {
		match Criterion::convert(asset.clone()) {
			Left(()) => <Left as fungible::Balanced<AccountId>>::withdraw(
				who,
				value,
				precision,
				preservation,
				force,
			)
			.map(|inner_credit| {
				let credit = fungibles::Imbalance::new(asset, inner_credit.peek());
				fungible::Imbalance::forget(inner_credit);
				credit
			}),
			Right(a) => <Right as fungibles::Balanced<AccountId>>::withdraw(
				a,
				who,
				value,
				precision,
				preservation,
				force,
			)
			.map(|inner_credit| {
				let credit = fungibles::Imbalance::new(asset, inner_credit.peek());
				fungibles::Imbalance::forget(inner_credit);
				credit
			}),
		}
	}
}

impl<
		Left: fungible::BalancedHold<AccountId>,
		Right: fungibles::BalancedHold<AccountId, Balance = Left::Balance, Reason = Left::Reason>,
		Criterion: Convert<AssetKind, Either<(), Right::AssetId>>,
		AssetKind: AssetId,
		AccountId,
	> fungibles::BalancedHold<AccountId> for UnionOf<Left, Right, Criterion, AssetKind, AccountId>
{
	fn slash(
		asset: Self::AssetId,
		reason: &Self::Reason,
		who: &AccountId,
		amount: Self::Balance,
	) -> (fungibles::Credit<AccountId, Self>, Self::Balance) {
		match Criterion::convert(asset.clone()) {
			Left(()) => {
				let (inner_credit, amount) =
					<Left as fungible::BalancedHold<AccountId>>::slash(reason, who, amount);
				let credit = fungibles::Imbalance::new(asset, inner_credit.peek());
				fungible::Imbalance::forget(inner_credit);
				(credit, amount)
			},
			Right(a) => {
				let (inner_credit, amount) =
					<Right as fungibles::BalancedHold<AccountId>>::slash(a, reason, who, amount);
				let credit = fungibles::Imbalance::new(asset, inner_credit.peek());
				fungibles::Imbalance::forget(inner_credit);
				(credit, amount)
			},
		}
	}
}

impl<
		Left: fungible::Inspect<AccountId>,
		Right: fungibles::Inspect<AccountId, Balance = Left::Balance> + fungibles::Create<AccountId>,
		Criterion: Convert<AssetKind, Either<(), Right::AssetId>>,
		AssetKind: AssetId,
		AccountId,
	> fungibles::Create<AccountId> for UnionOf<Left, Right, Criterion, AssetKind, AccountId>
{
	fn create(
		asset: AssetKind,
		admin: AccountId,
		is_sufficient: bool,
		min_balance: Self::Balance,
	) -> DispatchResult {
		match Criterion::convert(asset) {
			// no-op for `Left` since `Create` trait is not defined within `fungible::*`.
			Left(()) => Ok(()),
			Right(a) => <Right as fungibles::Create<AccountId>>::create(
				a,
				admin,
				is_sufficient,
				min_balance,
			),
		}
	}
}

impl<
		Left: fungible::Inspect<AccountId>,
		Right: fungibles::Inspect<AccountId>
			+ AccountTouch<
				Right::AssetId,
				AccountId,
				Balance = <Left as fungible::Inspect<AccountId>>::Balance,
			>,
		Criterion: Convert<AssetKind, Either<(), Right::AssetId>>,
		AssetKind: AssetId,
		AccountId,
	> AccountTouch<AssetKind, AccountId> for UnionOf<Left, Right, Criterion, AssetKind, AccountId>
{
	type Balance = <Left as fungible::Inspect<AccountId>>::Balance;

	fn deposit_required(asset: AssetKind) -> Self::Balance {
		match Criterion::convert(asset) {
			Left(()) => <Left as fungible::Inspect<AccountId>>::minimum_balance(),
			Right(a) => <Right as AccountTouch<Right::AssetId, AccountId>>::deposit_required(a),
		}
	}

	fn touch(asset: AssetKind, who: AccountId, depositor: AccountId) -> DispatchResult {
		match Criterion::convert(asset) {
			// no-op for `Left` since `AccountTouch` trait is not defined within `fungible::*`.
			Left(()) => Ok(()),
			Right(a) =>
				<Right as AccountTouch<Right::AssetId, AccountId>>::touch(a, who, depositor),
		}
	}
}

impl<
		Left: fungible::Inspect<AccountId>,
		Right: fungibles::Inspect<AccountId, Balance = Left::Balance>
			+ ContainsPair<Right::AssetId, AccountId>,
		Criterion: Convert<AssetKind, Either<(), Right::AssetId>>,
		AssetKind: AssetId,
		AccountId,
	> ContainsPair<AssetKind, AccountId> for UnionOf<Left, Right, Criterion, AssetKind, AccountId>
{
	fn contains(asset: &AssetKind, who: &AccountId) -> bool {
		match Criterion::convert(asset.clone()) {
			Left(()) =>
				<Left as fungible::Inspect<AccountId>>::total_balance(who) >=
					<Left as fungible::Inspect<AccountId>>::minimum_balance(),
			Right(a) => <Right as ContainsPair<Right::AssetId, AccountId>>::contains(&a, who),
		}
	}
}
