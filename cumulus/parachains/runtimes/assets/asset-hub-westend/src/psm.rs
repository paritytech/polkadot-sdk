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

//! Runtime-local fungibles adapter used by PSM.

use alloc::vec::Vec;
use frame_support::traits::tokens::{
	fungibles, AssetId, DepositConsequence, Fortitude, Precision, Preservation, Provenance,
	WithdrawConsequence,
};
use sp_runtime::{
	traits::Convert,
	DispatchError, DispatchResult, Either,
	Either::{Left, Right},
};

type Inner<Left, Right, Criterion, AssetKind, AccountId> =
	frame_support::traits::fungibles::UnionOf<Left, Right, Criterion, AssetKind, AccountId>;

/// A wrapper over [`frame_support::traits::fungibles::UnionOf`] that also delegates asset-role
/// inspection without requiring a new `frame-support` release.
pub struct UnionOf<Left, Right, Criterion, AssetKind, AccountId>(
	core::marker::PhantomData<(Left, Right, Criterion, AssetKind, AccountId)>,
);

impl<Left, Right, Criterion, AssetKind, AccountId> fungibles::Inspect<AccountId>
	for UnionOf<Left, Right, Criterion, AssetKind, AccountId>
where
	Left: fungibles::Inspect<AccountId>,
	Right: fungibles::Inspect<AccountId, Balance = Left::Balance>,
	Criterion: Convert<AssetKind, Either<Left::AssetId, Right::AssetId>>,
	AssetKind: AssetId,
{
	type AssetId = AssetKind;
	type Balance = Left::Balance;

	fn total_issuance(asset: Self::AssetId) -> Self::Balance {
		<Inner<Left, Right, Criterion, AssetKind, AccountId> as fungibles::Inspect<AccountId>>::total_issuance(asset)
	}
	fn active_issuance(asset: Self::AssetId) -> Self::Balance {
		<Inner<Left, Right, Criterion, AssetKind, AccountId> as fungibles::Inspect<AccountId>>::active_issuance(asset)
	}
	fn minimum_balance(asset: Self::AssetId) -> Self::Balance {
		<Inner<Left, Right, Criterion, AssetKind, AccountId> as fungibles::Inspect<AccountId>>::minimum_balance(asset)
	}
	fn balance(asset: Self::AssetId, who: &AccountId) -> Self::Balance {
		<Inner<Left, Right, Criterion, AssetKind, AccountId> as fungibles::Inspect<AccountId>>::balance(asset, who)
	}
	fn total_balance(asset: Self::AssetId, who: &AccountId) -> Self::Balance {
		<Inner<Left, Right, Criterion, AssetKind, AccountId> as fungibles::Inspect<AccountId>>::total_balance(asset, who)
	}
	fn reducible_balance(
		asset: Self::AssetId,
		who: &AccountId,
		preservation: Preservation,
		force: Fortitude,
	) -> Self::Balance {
		<Inner<Left, Right, Criterion, AssetKind, AccountId> as fungibles::Inspect<AccountId>>::reducible_balance(asset, who, preservation, force)
	}
	fn can_deposit(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
		provenance: Provenance,
	) -> DepositConsequence {
		<Inner<Left, Right, Criterion, AssetKind, AccountId> as fungibles::Inspect<AccountId>>::can_deposit(asset, who, amount, provenance)
	}
	fn can_withdraw(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
	) -> WithdrawConsequence<Self::Balance> {
		<Inner<Left, Right, Criterion, AssetKind, AccountId> as fungibles::Inspect<AccountId>>::can_withdraw(asset, who, amount)
	}
	fn asset_exists(asset: Self::AssetId) -> bool {
		<Inner<Left, Right, Criterion, AssetKind, AccountId> as fungibles::Inspect<AccountId>>::asset_exists(asset)
	}
}

impl<Left, Right, Criterion, AssetKind, AccountId> fungibles::metadata::Inspect<AccountId>
	for UnionOf<Left, Right, Criterion, AssetKind, AccountId>
where
	Left: fungibles::Inspect<AccountId> + fungibles::metadata::Inspect<AccountId>,
	Right: fungibles::Inspect<AccountId, Balance = Left::Balance>
		+ fungibles::metadata::Inspect<AccountId>,
	Criterion: Convert<AssetKind, Either<Left::AssetId, Right::AssetId>>,
	AssetKind: AssetId,
{
	fn name(asset: Self::AssetId) -> Vec<u8> {
		<Inner<Left, Right, Criterion, AssetKind, AccountId> as fungibles::metadata::Inspect<
			AccountId,
		>>::name(asset)
	}
	fn symbol(asset: Self::AssetId) -> Vec<u8> {
		<Inner<Left, Right, Criterion, AssetKind, AccountId> as fungibles::metadata::Inspect<
			AccountId,
		>>::symbol(asset)
	}
	fn decimals(asset: Self::AssetId) -> u8 {
		<Inner<Left, Right, Criterion, AssetKind, AccountId> as fungibles::metadata::Inspect<
			AccountId,
		>>::decimals(asset)
	}
}

impl<Left, Right, Criterion, AssetKind, AccountId> fungibles::metadata::Mutate<AccountId>
	for UnionOf<Left, Right, Criterion, AssetKind, AccountId>
where
	Left: fungibles::Inspect<AccountId>
		+ fungibles::metadata::Inspect<AccountId>
		+ fungibles::metadata::Mutate<AccountId>,
	Right: fungibles::Inspect<AccountId, Balance = Left::Balance>
		+ fungibles::metadata::Inspect<AccountId>
		+ fungibles::metadata::Mutate<AccountId>,
	Criterion: Convert<AssetKind, Either<Left::AssetId, Right::AssetId>>,
	AssetKind: AssetId,
{
	fn set(
		asset: Self::AssetId,
		from: &AccountId,
		name: Vec<u8>,
		symbol: Vec<u8>,
		decimals: u8,
	) -> DispatchResult {
		match Criterion::convert(asset) {
			Left(asset) => Left::set(asset, from, name, symbol, decimals),
			Right(asset) => Right::set(asset, from, name, symbol, decimals),
		}
	}
}

impl<Left, Right, Criterion, AssetKind, AccountId> fungibles::roles::Inspect<AccountId>
	for UnionOf<Left, Right, Criterion, AssetKind, AccountId>
where
	Left: fungibles::Inspect<AccountId> + fungibles::roles::Inspect<AccountId>,
	Right: fungibles::Inspect<AccountId, Balance = Left::Balance>
		+ fungibles::roles::Inspect<AccountId>,
	Criterion: Convert<AssetKind, Either<Left::AssetId, Right::AssetId>>,
	AssetKind: AssetId,
{
	fn owner(asset: Self::AssetId) -> Option<AccountId> {
		match Criterion::convert(asset) {
			Left(asset) => Left::owner(asset),
			Right(asset) => Right::owner(asset),
		}
	}
	fn issuer(asset: Self::AssetId) -> Option<AccountId> {
		match Criterion::convert(asset) {
			Left(asset) => Left::issuer(asset),
			Right(asset) => Right::issuer(asset),
		}
	}
	fn admin(asset: Self::AssetId) -> Option<AccountId> {
		match Criterion::convert(asset) {
			Left(asset) => Left::admin(asset),
			Right(asset) => Right::admin(asset),
		}
	}
	fn freezer(asset: Self::AssetId) -> Option<AccountId> {
		match Criterion::convert(asset) {
			Left(asset) => Left::freezer(asset),
			Right(asset) => Right::freezer(asset),
		}
	}
}

impl<Left, Right, Criterion, AssetKind, AccountId> fungibles::Unbalanced<AccountId>
	for UnionOf<Left, Right, Criterion, AssetKind, AccountId>
where
	Left: fungibles::Unbalanced<AccountId>,
	Right: fungibles::Unbalanced<AccountId, Balance = Left::Balance>,
	Criterion: Convert<AssetKind, Either<Left::AssetId, Right::AssetId>>,
	AssetKind: AssetId,
{
	fn handle_dust(dust: fungibles::Dust<AccountId, Self>) {
		match Criterion::convert(dust.0) {
			Left(asset) => Left::handle_dust(fungibles::Dust(asset, dust.1)),
			Right(asset) => Right::handle_dust(fungibles::Dust(asset, dust.1)),
		}
	}
	fn write_balance(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
	) -> Result<Option<Self::Balance>, DispatchError> {
		match Criterion::convert(asset) {
			Left(asset) => Left::write_balance(asset, who, amount),
			Right(asset) => Right::write_balance(asset, who, amount),
		}
	}
	fn set_total_issuance(asset: Self::AssetId, amount: Self::Balance) {
		match Criterion::convert(asset) {
			Left(asset) => Left::set_total_issuance(asset, amount),
			Right(asset) => Right::set_total_issuance(asset, amount),
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
			Left(asset) => {
				Left::decrease_balance(asset, who, amount, precision, preservation, force)
			},
			Right(asset) => {
				Right::decrease_balance(asset, who, amount, precision, preservation, force)
			},
		}
	}
	fn increase_balance(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
		precision: Precision,
	) -> Result<Self::Balance, DispatchError> {
		match Criterion::convert(asset) {
			Left(asset) => Left::increase_balance(asset, who, amount, precision),
			Right(asset) => Right::increase_balance(asset, who, amount, precision),
		}
	}
}

impl<Left, Right, Criterion, AssetKind, AccountId> fungibles::Mutate<AccountId>
	for UnionOf<Left, Right, Criterion, AssetKind, AccountId>
where
	Left: fungibles::Mutate<AccountId>,
	Right: fungibles::Mutate<AccountId, Balance = Left::Balance>,
	Criterion: Convert<AssetKind, Either<Left::AssetId, Right::AssetId>>,
	AssetKind: AssetId,
	AccountId: Eq,
{
	fn mint_into(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
	) -> Result<Self::Balance, DispatchError> {
		match Criterion::convert(asset) {
			Left(asset) => Left::mint_into(asset, who, amount),
			Right(asset) => Right::mint_into(asset, who, amount),
		}
	}
	fn burn_from(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
		preservation: Preservation,
		precision: Precision,
		force: Fortitude,
	) -> Result<Self::Balance, DispatchError> {
		match Criterion::convert(asset) {
			Left(asset) => Left::burn_from(asset, who, amount, preservation, precision, force),
			Right(asset) => Right::burn_from(asset, who, amount, preservation, precision, force),
		}
	}
	fn shelve(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
	) -> Result<Self::Balance, DispatchError> {
		match Criterion::convert(asset) {
			Left(asset) => Left::shelve(asset, who, amount),
			Right(asset) => Right::shelve(asset, who, amount),
		}
	}
	fn restore(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
	) -> Result<Self::Balance, DispatchError> {
		match Criterion::convert(asset) {
			Left(asset) => Left::restore(asset, who, amount),
			Right(asset) => Right::restore(asset, who, amount),
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
			Left(asset) => Left::transfer(asset, source, dest, amount, preservation),
			Right(asset) => Right::transfer(asset, source, dest, amount, preservation),
		}
	}
	fn set_balance(asset: Self::AssetId, who: &AccountId, amount: Self::Balance) -> Self::Balance {
		match Criterion::convert(asset) {
			Left(asset) => Left::set_balance(asset, who, amount),
			Right(asset) => Right::set_balance(asset, who, amount),
		}
	}
}

impl<Left, Right, Criterion, AssetKind, AccountId> fungibles::Create<AccountId>
	for UnionOf<Left, Right, Criterion, AssetKind, AccountId>
where
	Left: fungibles::Inspect<AccountId> + fungibles::Create<AccountId>,
	Right: fungibles::Inspect<AccountId, Balance = Left::Balance> + fungibles::Create<AccountId>,
	Criterion: Convert<AssetKind, Either<Left::AssetId, Right::AssetId>>,
	AssetKind: AssetId,
{
	fn create(
		asset: Self::AssetId,
		admin: AccountId,
		is_sufficient: bool,
		min_balance: Self::Balance,
	) -> DispatchResult {
		match Criterion::convert(asset) {
			Left(asset) => Left::create(asset, admin, is_sufficient, min_balance),
			Right(asset) => Right::create(asset, admin, is_sufficient, min_balance),
		}
	}
}
