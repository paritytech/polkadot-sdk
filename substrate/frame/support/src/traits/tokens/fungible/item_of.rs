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

//! Adapter to use `fungibles::*` implementations as `fungible::*`.

use super::*;
use crate::traits::{
	fungible::imbalance,
	tokens::{
		fungibles, DepositConsequence, Fortitude, Precision, Preservation, Provenance, Restriction,
		WithdrawConsequence,
	},
};
use sp_core::Get;
use sp_runtime::{DispatchError, DispatchResult};

// Redirects `fungible` function to the `fungibles` equivalent with the proper AssetId.
macro_rules! redirect {
    ( $(
        fn $fn_name:ident (
            $(
                $arg_name:ident : $arg_ty:ty
            ),* $(,)?
        ) $(-> $fn_out:ty)?;
    )+) => {
        $(
            fn $fn_name($($arg_name:$arg_ty),*) $(-> $fn_out)? {
                F::$fn_name(A::get(), $($arg_name),*)
            }
        )+
    };
}

/// Convert a `fungibles` trait implementation into a `fungible` trait implementation by identifying
/// a single item.
pub struct ItemOf<
	F: fungibles::Inspect<AccountId>,
	A: Get<<F as fungibles::Inspect<AccountId>>::AssetId>,
	AccountId,
>(sp_std::marker::PhantomData<(F, A, AccountId)>);

impl<
		F: fungibles::Inspect<AccountId>,
		A: Get<<F as fungibles::Inspect<AccountId>>::AssetId>,
		AccountId,
	> Inspect<AccountId> for ItemOf<F, A, AccountId>
{
	type Balance = <F as fungibles::Inspect<AccountId>>::Balance;

	redirect!(
		fn total_issuance() -> Self::Balance;
		fn active_issuance() -> Self::Balance;
		fn minimum_balance() -> Self::Balance;
		fn balance(who: &AccountId) -> Self::Balance;
		fn total_balance(who: &AccountId) -> Self::Balance;
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
	);
}

impl<
		F: fungibles::InspectHold<AccountId>,
		A: Get<<F as fungibles::Inspect<AccountId>>::AssetId>,
		AccountId,
	> InspectHold<AccountId> for ItemOf<F, A, AccountId>
{
	type Reason = F::Reason;

	redirect!(
		fn reducible_total_balance_on_hold(who: &AccountId, force: Fortitude) -> Self::Balance;
		fn hold_available(reason: &Self::Reason, who: &AccountId) -> bool;
		fn total_balance_on_hold(who: &AccountId) -> Self::Balance;
		fn balance_on_hold(reason: &Self::Reason, who: &AccountId) -> Self::Balance;
		fn can_hold(reason: &Self::Reason, who: &AccountId, amount: Self::Balance) -> bool;
		fn ensure_can_hold(
			reason: &Self::Reason,
			who: &AccountId,
			amount: Self::Balance,
		) -> DispatchResult;
	);
}

impl<
		F: fungibles::InspectFreeze<AccountId>,
		A: Get<<F as fungibles::Inspect<AccountId>>::AssetId>,
		AccountId,
	> InspectFreeze<AccountId> for ItemOf<F, A, AccountId>
{
	type Id = F::Id;

	redirect!(
		fn balance_frozen(id: &Self::Id, who: &AccountId) -> Self::Balance;
		fn balance_freezable(who: &AccountId) -> Self::Balance;
		fn can_freeze(id: &Self::Id, who: &AccountId) -> bool;
	);
}

impl<
		F: fungibles::Unbalanced<AccountId>,
		A: Get<<F as fungibles::Inspect<AccountId>>::AssetId>,
		AccountId,
	> Unbalanced<AccountId> for ItemOf<F, A, AccountId>
{
	fn handle_dust(dust: regular::Dust<AccountId, Self>)
	where
		Self: Sized,
	{
		<F as fungibles::Unbalanced<AccountId>>::handle_dust(fungibles::Dust(A::get(), dust.0))
	}

	redirect!(
		fn write_balance(
			who: &AccountId,
			amount: Self::Balance,
		) -> Result<Option<Self::Balance>, DispatchError>;
		fn set_total_issuance(amount: Self::Balance) -> ();
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
		fn handle_raw_dust(amount: Self::Balance);
		fn deactivate(amount: Self::Balance);
		fn reactivate(amount: Self::Balance);
	);
}

impl<
		F: fungibles::UnbalancedHold<AccountId>,
		A: Get<<F as fungibles::Inspect<AccountId>>::AssetId>,
		AccountId,
	> UnbalancedHold<AccountId> for ItemOf<F, A, AccountId>
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

impl<
		F: fungibles::Mutate<AccountId>,
		A: Get<<F as fungibles::Inspect<AccountId>>::AssetId>,
		AccountId: Eq,
	> Mutate<AccountId> for ItemOf<F, A, AccountId>
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

impl<
		F: fungibles::MutateHold<AccountId>,
		A: Get<<F as fungibles::Inspect<AccountId>>::AssetId>,
		AccountId,
	> MutateHold<AccountId> for ItemOf<F, A, AccountId>
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
			preservation: Preservation,
			force: Fortitude,
		) -> Result<Self::Balance, DispatchError>;
		fn burn_all_held(
			reason: &Self::Reason,
			who: &AccountId,
			precision: Precision,
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

impl<
		F: fungibles::MutateFreeze<AccountId>,
		A: Get<<F as fungibles::Inspect<AccountId>>::AssetId>,
		AccountId,
	> MutateFreeze<AccountId> for ItemOf<F, A, AccountId>
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

pub struct ConvertImbalanceDropHandler<AccountId, Balance, AssetIdType, AssetId, Handler>(
	sp_std::marker::PhantomData<(AccountId, Balance, AssetIdType, AssetId, Handler)>,
);

impl<
		AccountId,
		Balance,
		AssetIdType,
		AssetId: Get<AssetIdType>,
		Handler: crate::traits::fungibles::HandleImbalanceDrop<AssetIdType, Balance>,
	> HandleImbalanceDrop<Balance>
	for ConvertImbalanceDropHandler<AccountId, Balance, AssetIdType, AssetId, Handler>
{
	fn handle(amount: Balance) {
		Handler::handle(AssetId::get(), amount)
	}
}

impl<
		F: fungibles::Inspect<AccountId>
			+ fungibles::Unbalanced<AccountId>
			+ fungibles::Balanced<AccountId>,
		A: Get<<F as fungibles::Inspect<AccountId>>::AssetId>,
		AccountId,
	> Balanced<AccountId> for ItemOf<F, A, AccountId>
{
	type OnDropDebt =
		ConvertImbalanceDropHandler<AccountId, Self::Balance, F::AssetId, A, F::OnDropDebt>;
	type OnDropCredit =
		ConvertImbalanceDropHandler<AccountId, Self::Balance, F::AssetId, A, F::OnDropCredit>;

	fn deposit(
		who: &AccountId,
		value: Self::Balance,
		precision: Precision,
	) -> Result<Debt<AccountId, Self>, DispatchError> {
		<F as fungibles::Balanced<AccountId>>::deposit(A::get(), who, value, precision)
			.map(imbalance::from_fungibles)
	}
	fn issue(amount: Self::Balance) -> Credit<AccountId, Self> {
		let credit = <F as fungibles::Balanced<AccountId>>::issue(A::get(), amount);
		imbalance::from_fungibles(credit)
	}
	fn pair(amount: Self::Balance) -> (Debt<AccountId, Self>, Credit<AccountId, Self>) {
		let (a, b) = <F as fungibles::Balanced<AccountId>>::pair(A::get(), amount);
		(imbalance::from_fungibles(a), imbalance::from_fungibles(b))
	}
	fn rescind(amount: Self::Balance) -> Debt<AccountId, Self> {
		let debt = <F as fungibles::Balanced<AccountId>>::rescind(A::get(), amount);
		imbalance::from_fungibles(debt)
	}
	fn resolve(
		who: &AccountId,
		credit: Credit<AccountId, Self>,
	) -> Result<(), Credit<AccountId, Self>> {
		let credit = fungibles::imbalance::from_fungible(credit, A::get());
		<F as fungibles::Balanced<AccountId>>::resolve(who, credit)
			.map_err(imbalance::from_fungibles)
	}
	fn settle(
		who: &AccountId,
		debt: Debt<AccountId, Self>,
		preservation: Preservation,
	) -> Result<Credit<AccountId, Self>, Debt<AccountId, Self>> {
		let debt = fungibles::imbalance::from_fungible(debt, A::get());
		<F as fungibles::Balanced<AccountId>>::settle(who, debt, preservation).map_or_else(
			|d| Err(imbalance::from_fungibles(d)),
			|c| Ok(imbalance::from_fungibles(c)),
		)
	}
	fn withdraw(
		who: &AccountId,
		value: Self::Balance,
		precision: Precision,
		preservation: Preservation,
		force: Fortitude,
	) -> Result<Credit<AccountId, Self>, DispatchError> {
		<F as fungibles::Balanced<AccountId>>::withdraw(
			A::get(),
			who,
			value,
			precision,
			preservation,
			force,
		)
		.map(imbalance::from_fungibles)
	}

	redirect!(
		fn done_rescind(amount: Self::Balance);
		fn done_issue(amount: Self::Balance);
		fn done_deposit(who: &AccountId, amount: Self::Balance);
		fn done_withdraw(who: &AccountId, amount: Self::Balance);
	);
}

impl<
		F: fungibles::BalancedHold<AccountId>,
		A: Get<<F as fungibles::Inspect<AccountId>>::AssetId>,
		AccountId,
	> BalancedHold<AccountId> for ItemOf<F, A, AccountId>
{
	fn slash(
		reason: &Self::Reason,
		who: &AccountId,
		amount: Self::Balance,
	) -> (Credit<AccountId, Self>, Self::Balance) {
		let (credit, amount) =
			<F as fungibles::BalancedHold<AccountId>>::slash(A::get(), reason, who, amount);
		(imbalance::from_fungibles(credit), amount)
	}

	redirect!(
		fn done_slash(reason: &Self::Reason, who: &AccountId, amount: Self::Balance);
	);
}

#[test]
fn test() {}
