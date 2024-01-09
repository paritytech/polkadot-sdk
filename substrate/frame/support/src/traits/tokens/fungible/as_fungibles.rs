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

//! Adapter to use `fungible:*` implementations as `fungibles::*`.

use super::{imbalance::from_fungibles, *};
use crate::traits::tokens::{
	fungible, fungibles, fungibles::imbalance::from_fungible, DepositConsequence, Fortitude,
	Precision, Preservation, Provenance, Restriction, WithdrawConsequence,
};
use sp_runtime::{DispatchError, DispatchResult};

/// Redirects `fungibles` function to the `fungible` equivalent without the AssetId argument.
macro_rules! redirect {
    ( $(
        fn $fn_name:ident (
            $(
                $arg_name:ident : $arg_ty:ty
            ),* $(,)?
        ) $(-> $fn_out:ty)?;
    )+) => {
        $(
            fn $fn_name((): Self::AssetId, $($arg_name:$arg_ty),*) $(-> $fn_out)? {
                F::$fn_name($($arg_name),*)
            }
        )+
    };
}

pub struct ConvertHandleImbalanceDrop<H>(PhantomData<H>);

impl<B, H: fungible::HandleImbalanceDrop<B>> fungibles::HandleImbalanceDrop<(), B>
	for ConvertHandleImbalanceDrop<H>
{
	fn handle((): (), amount: B) {
		H::handle(amount)
	}
}

/// A wrapper to use a `fungible` as a `fungibles` with a single asset represented by `()`.
pub struct AsFungibles<F, AccountId>(PhantomData<(F, AccountId)>);

impl<AccountId, F: fungible::Inspect<AccountId>> fungibles::Inspect<AccountId>
	for AsFungibles<F, AccountId>
{
	type AssetId = ();
	type Balance = F::Balance;

	redirect!(
		fn total_issuance() -> Self::Balance;
		fn minimum_balance() -> Self::Balance;
		fn total_balance(who: &AccountId) -> Self::Balance;
		fn balance(who: &AccountId) -> Self::Balance;
		fn reducible_balance(
			who: &AccountId,
			preservation: Preservation,
			force: Fortitude,
		) -> Self::Balance;
		fn can_deposit(
			who: &AccountId,
			amount: Self::Balance,
			provenance: Provenance,
		) -> DepositConsequence;
		fn can_withdraw(
			who: &AccountId,
			amount: Self::Balance,
		) -> WithdrawConsequence<Self::Balance>;
		fn active_issuance() -> Self::Balance;
	);

	fn asset_exists((): Self::AssetId) -> bool {
		true
	}
}

impl<AccountId, F: fungible::Unbalanced<AccountId>> fungibles::Unbalanced<AccountId>
	for AsFungibles<F, AccountId>
{
	redirect!(
		fn write_balance(
			who: &AccountId,
			amount: Self::Balance,
		) -> Result<Option<Self::Balance>, DispatchError>;
		fn set_total_issuance(amount: Self::Balance);
		fn handle_raw_dust(amount: Self::Balance);
		fn decrease_balance(
			who: &AccountId,
			amount: Self::Balance,
			precision: Precision,
			preservation: Preservation,
			force: Fortitude,
		) -> Result<Self::Balance, DispatchError>;
		fn increase_balance(
			who: &AccountId,
			amount: Self::Balance,
			precision: Precision,
		) -> Result<Self::Balance, DispatchError>;
		fn deactivate(amount: Self::Balance);
		fn reactivate(amount: Self::Balance);
	);

	fn handle_dust(fungibles::Dust((), dust): fungibles::Dust<AccountId, Self>) {
		F::handle_dust(fungible::Dust(dust))
	}
}

impl<AccountId: Eq, F: fungible::Mutate<AccountId>> fungibles::Mutate<AccountId>
	for AsFungibles<F, AccountId>
{
	redirect!(
		fn mint_into(
			who: &AccountId,
			amount: Self::Balance,
		) -> Result<Self::Balance, DispatchError>;
		fn burn_from(
			who: &AccountId,
			amount: Self::Balance,
			precision: Precision,
			force: Fortitude,
		) -> Result<Self::Balance, DispatchError>;
		fn shelve(who: &AccountId, amount: Self::Balance) -> Result<Self::Balance, DispatchError>;
		fn restore(who: &AccountId, amount: Self::Balance) -> Result<Self::Balance, DispatchError>;
		fn transfer(
			source: &AccountId,
			dest: &AccountId,
			amount: Self::Balance,
			preservation: Preservation,
		) -> Result<Self::Balance, DispatchError>;
		fn set_balance(who: &AccountId, amount: Self::Balance) -> Self::Balance;
		fn done_mint_into(who: &AccountId, amount: Self::Balance);
		fn done_burn_from(who: &AccountId, amount: Self::Balance);
		fn done_shelve(who: &AccountId, amount: Self::Balance);
		fn done_restore(who: &AccountId, amount: Self::Balance);
		fn done_transfer(source: &AccountId, dest: &AccountId, amount: Self::Balance);
	);
}

impl<AccountId, F: fungible::Balanced<AccountId>> fungibles::Balanced<AccountId>
	for AsFungibles<F, AccountId>
{
	type OnDropDebt = ConvertHandleImbalanceDrop<F::OnDropDebt>;
	type OnDropCredit = ConvertHandleImbalanceDrop<F::OnDropCredit>;

	fn rescind((): Self::AssetId, amount: Self::Balance) -> fungibles::Debt<AccountId, Self> {
		let dept = F::rescind(amount);
		from_fungible(dept, ())
	}

	fn issue((): Self::AssetId, amount: Self::Balance) -> fungibles::Credit<AccountId, Self> {
		let credit = F::issue(amount);
		from_fungible(credit, ())
	}

	fn pair(
		(): Self::AssetId,
		amount: Self::Balance,
	) -> (fungibles::Debt<AccountId, Self>, fungibles::Credit<AccountId, Self>) {
		let (dept, credit) = F::pair(amount);
		(from_fungible(dept, ()), from_fungible(credit, ()))
	}

	fn deposit(
		(): Self::AssetId,
		who: &AccountId,
		value: Self::Balance,
		precision: Precision,
	) -> Result<fungibles::Debt<AccountId, Self>, DispatchError> {
		F::deposit(who, value, precision).map(|dept| from_fungible(dept, ()))
	}

	fn withdraw(
		(): Self::AssetId,
		who: &AccountId,
		value: Self::Balance,
		precision: Precision,
		preservation: Preservation,
		force: Fortitude,
	) -> Result<fungibles::Credit<AccountId, Self>, DispatchError> {
		F::withdraw(who, value, precision, preservation, force)
			.map(|credit| from_fungible(credit, ()))
	}

	fn resolve(
		who: &AccountId,
		credit: fungibles::Credit<AccountId, Self>,
	) -> Result<(), fungibles::Credit<AccountId, Self>> {
		F::resolve(who, from_fungibles(credit)).map_err(|credit| from_fungible(credit, ()))
	}

	fn settle(
		who: &AccountId,
		debt: fungibles::Debt<AccountId, Self>,
		preservation: Preservation,
	) -> Result<fungibles::Credit<AccountId, Self>, fungibles::Debt<AccountId, Self>> {
		F::settle(who, from_fungibles(debt), preservation)
			.map(|credit| from_fungible(credit, ()))
			.map_err(|dept| from_fungible(dept, ()))
	}

	redirect!(
		fn done_rescind(amount: Self::Balance);
		fn done_issue(amount: Self::Balance);
		fn done_deposit(who: &AccountId, amount: Self::Balance);
		fn done_withdraw(who: &AccountId, amount: Self::Balance);
	);
}

impl<AccountId, F: fungible::hold::Inspect<AccountId>> fungibles::hold::Inspect<AccountId>
	for AsFungibles<F, AccountId>
{
	type Reason = F::Reason;

	redirect!(
		fn total_balance_on_hold(who: &AccountId) -> Self::Balance;
		fn balance_on_hold(reason: &Self::Reason, who: &AccountId) -> Self::Balance;
		fn reducible_total_balance_on_hold(who: &AccountId, force: Fortitude) -> Self::Balance;
		fn hold_available(reason: &Self::Reason, who: &AccountId) -> bool;
		fn ensure_can_hold(
			reason: &Self::Reason,
			who: &AccountId,
			amount: Self::Balance,
		) -> DispatchResult;
		fn can_hold(reason: &Self::Reason, who: &AccountId, amount: Self::Balance) -> bool;
	);
}

impl<AccountId, F: fungible::hold::Unbalanced<AccountId>> fungibles::hold::Unbalanced<AccountId>
	for AsFungibles<F, AccountId>
{
	redirect!(
		fn set_balance_on_hold(
			reason: &Self::Reason,
			who: &AccountId,
			amount: Self::Balance,
		) -> DispatchResult;
		fn decrease_balance_on_hold(
			reason: &Self::Reason,
			who: &AccountId,
			amount: Self::Balance,
			precision: Precision,
		) -> Result<Self::Balance, DispatchError>;
		fn increase_balance_on_hold(
			reason: &Self::Reason,
			who: &AccountId,
			amount: Self::Balance,
			precision: Precision,
		) -> Result<Self::Balance, DispatchError>;
	);
}

impl<AccountId, F: fungible::hold::Mutate<AccountId>> fungibles::hold::Mutate<AccountId>
	for AsFungibles<F, AccountId>
{
	redirect!(
		fn hold(reason: &Self::Reason, who: &AccountId, amount: Self::Balance) -> DispatchResult;
		fn release(
			reason: &Self::Reason,
			who: &AccountId,
			amount: Self::Balance,
			precision: Precision,
		) -> Result<Self::Balance, DispatchError>;
		fn burn_held(
			reason: &Self::Reason,
			who: &AccountId,
			amount: Self::Balance,
			precision: Precision,
			force: Fortitude,
		) -> Result<Self::Balance, DispatchError>;
		fn burn_all_held(
			reason: &Self::Reason,
			who: &AccountId,
			precision: Precision,
			force: Fortitude,
		) -> Result<Self::Balance, DispatchError>;
		fn transfer_on_hold(
			reason: &Self::Reason,
			source: &AccountId,
			dest: &AccountId,
			amount: Self::Balance,
			precision: Precision,
			mode: Restriction,
			force: Fortitude,
		) -> Result<Self::Balance, DispatchError>;
		fn transfer_and_hold(
			reason: &Self::Reason,
			source: &AccountId,
			dest: &AccountId,
			amount: Self::Balance,
			precision: Precision,
			expendability: Preservation,
			force: Fortitude,
		) -> Result<Self::Balance, DispatchError>;
		fn done_hold(reason: &Self::Reason, who: &AccountId, amount: Self::Balance);
		fn done_release(reason: &Self::Reason, who: &AccountId, amount: Self::Balance);
		fn done_burn_held(reason: &Self::Reason, who: &AccountId, amount: Self::Balance);
		fn done_transfer_on_hold(
			reason: &Self::Reason,
			source: &AccountId,
			dest: &AccountId,
			amount: Self::Balance,
		);
		fn done_transfer_and_hold(
			reason: &Self::Reason,
			source: &AccountId,
			dest: &AccountId,
			transferred: Self::Balance,
		);
	);
}

impl<AccountId, F: fungible::hold::Balanced<AccountId>> fungibles::hold::Balanced<AccountId>
	for AsFungibles<F, AccountId>
{
	fn slash(
		(): Self::AssetId,
		reason: &Self::Reason,
		who: &AccountId,
		amount: Self::Balance,
	) -> (fungibles::Credit<AccountId, Self>, Self::Balance) {
		let (credit, balance) = F::slash(reason, who, amount);
		(from_fungible(credit, ()), balance)
	}

	redirect!(
		fn done_slash(reason: &Self::Reason, who: &AccountId, amount: Self::Balance);
	);
}

impl<AccountId, F: fungible::freeze::Inspect<AccountId>> fungibles::freeze::Inspect<AccountId>
	for AsFungibles<F, AccountId>
{
	type Id = F::Id;

	redirect!(
		fn balance_frozen(id: &Self::Id, who: &AccountId) -> Self::Balance;
		fn can_freeze(id: &Self::Id, who: &AccountId) -> bool;
		fn balance_freezable(who: &AccountId) -> Self::Balance;
	);
}

impl<AccountId, F: fungible::freeze::Mutate<AccountId>> fungibles::freeze::Mutate<AccountId>
	for AsFungibles<F, AccountId>
{
	redirect!(
		fn set_freeze(id: &Self::Id, who: &AccountId, amount: Self::Balance) -> DispatchResult;
		fn extend_freeze(id: &Self::Id, who: &AccountId, amount: Self::Balance) -> DispatchResult;
		fn thaw(id: &Self::Id, who: &AccountId) -> DispatchResult;
		fn set_frozen(
			id: &Self::Id,
			who: &AccountId,
			amount: Self::Balance,
			fortitude: Fortitude,
		) -> DispatchResult;
		fn ensure_frozen(
			id: &Self::Id,
			who: &AccountId,
			amount: Self::Balance,
			fortitude: Fortitude,
		) -> DispatchResult;
		fn decrease_frozen(id: &Self::Id, who: &AccountId, amount: Self::Balance)
			-> DispatchResult;
		fn increase_frozen(id: &Self::Id, who: &AccountId, amount: Self::Balance)
			-> DispatchResult;
	);
}
