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

//! Storage deposit payment backend.
//!
//! Storage deposits can be backed by the native currency or by PGAS. The
//! [`GasPayment`] trait abstracts the choice so the pallet does not need to
//! branch on asset type: the implementation decides whether a charge is taken
//! in PGAS or in the native currency, and records DOT-paid contributions in
//! [`DotByContractUser`]. Runtimes without PGAS leave the default `()` binding,
//! which always uses the native currency and matches pre-PGAS behaviour.
use crate::{BalanceOf, Config, DotByContractUser, HoldReason, evm::fees::InfoT as FeeInfo};
use core::marker::PhantomData;
use frame_support::traits::{
	Get,
	fungible::{Balanced as _, InspectHold as _, Mutate as _, MutateHold as _},
	tokens::{Fortitude, Precision, Preservation, Restriction, fungibles},
};
use sp_runtime::{
	DispatchResult,
	traits::{Saturating, Zero},
};

/// Payment backend used to charge storage deposits.
///
/// Implementations decide whether a charge is paid in PGAS or in the native
/// currency. When paid in native currency, the DOT contribution is recorded in
/// [`DotByContractUser`] so that refunds can later be bounded to the amount the
/// user actually paid as DOT. PGAS charges are never tracked there: they refund
/// as PGAS and can never exit as DOT.
pub trait GasPayment<T: Config> {
	/// Transfer `amount` from `from` to `to` to back a storage deposit.
	///
	/// Uses PGAS when the payer holds enough; falls back to the native currency otherwise,
	/// recording the DOT contribution in [`DotByContractUser`].
	fn transfer(from: &T::AccountId, to: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult;

	/// Transfer `amount` from `from` to `to` and place it on hold under `reason`.
	///
	/// Uses PGAS when the payer holds enough; falls back to the native currency otherwise,
	/// recording the DOT contribution in [`DotByContractUser`].
	fn transfer_and_hold(
		reason: HoldReason,
		from: &T::AccountId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult;

	/// Refund `amount` of held funds from contract `from` to user `to`'s free balance.
	///
	/// Refunds DOT first up to `DotByContractUser[from][to]` and the contract's actual
	/// DOT-on-hold for `reason`; the rest comes from PGAS. The aggregate transferred is
	/// always exactly `amount` (otherwise an error is returned and storage rolls back).
	fn refund_on_hold(
		reason: HoldReason,
		from: &T::AccountId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult;

	/// Collect `amount` of held funds from contract `from` back into the tx fee pool.
	///
	/// Releases DOT first up to `DotByContractUser[from][to]` and the actual DOT-on-hold
	/// for `reason`, withdraws it from `from`, and deposits the credit to the tx fee pool.
	/// Any remainder is released from PGAS and transferred to `to`'s free balance (PGAS
	/// cannot feed the native-currency fee pool).
	fn collect_on_hold(
		reason: HoldReason,
		from: &T::AccountId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult;

	/// Total amount held for `who` under `reason` across DOT and PGAS.
	///
	/// Used by error classifiers to distinguish "insufficient deposit" from "locks prevent
	/// release" without the caller needing to know which asset backed the deposit.
	fn total_on_hold(reason: HoldReason, who: &T::AccountId) -> BalanceOf<T> {
		T::Currency::balance_on_hold(&reason.into(), who)
	}

	/// Record that user `from` contributed `amount` in DOT to contract `to`.
	fn record_dot_contribution(from: &T::AccountId, to: &T::AccountId, amount: BalanceOf<T>) {
		DotByContractUser::<T>::mutate(to, from, |entitlement| {
			*entitlement = entitlement.saturating_add(amount);
		});
	}
}

/// Default backend: every storage deposit charge goes through the native currency.
impl<T: Config> GasPayment<T> for () {
	fn transfer(from: &T::AccountId, to: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
		T::Currency::transfer(from, to, amount, Preservation::Preserve)?;
		<Self as GasPayment<T>>::record_dot_contribution(from, to, amount);
		Ok(())
	}

	fn transfer_and_hold(
		reason: HoldReason,
		from: &T::AccountId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		T::Currency::transfer_and_hold(
			&reason.into(),
			from,
			to,
			amount,
			Precision::Exact,
			Preservation::Preserve,
			Fortitude::Polite,
		)?;
		<Self as GasPayment<T>>::record_dot_contribution(from, to, amount);
		Ok(())
	}

	fn refund_on_hold(
		reason: HoldReason,
		from: &T::AccountId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		T::Currency::transfer_on_hold(
			&reason.into(),
			from,
			to,
			amount,
			Precision::Exact,
			Restriction::Free,
			Fortitude::Polite,
		)?;
		let contribution = DotByContractUser::<T>::get(from, to);
		DotByContractUser::<T>::mutate(from, to, |entitlement| {
			*entitlement = entitlement.saturating_sub(amount.min(contribution));
		});
		Ok(())
	}

	fn collect_on_hold(
		reason: HoldReason,
		from: &T::AccountId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		let released = T::Currency::release(&reason.into(), from, amount, Precision::Exact)?;
		let credit = T::Currency::withdraw(
			from,
			released,
			Precision::Exact,
			Preservation::Preserve,
			Fortitude::Polite,
		)?;
		T::FeeInfo::deposit_txfee(credit);
		let contribution = DotByContractUser::<T>::get(from, to);
		DotByContractUser::<T>::mutate(from, to, |entitlement| {
			*entitlement = entitlement.saturating_sub(amount.min(contribution));
		});
		Ok(())
	}
}

/// PGAS-backed payment backend. Prefers PGAS when the payer holds enough and
/// falls back to the native currency otherwise.
pub struct PGasPayment<Assets, Id>(PhantomData<(Assets, Id)>);

impl<Assets, Id> PGasPayment<Assets, Id> {
	fn pgas_reducible_balance<T>(who: &T::AccountId) -> BalanceOf<T>
	where
		T: Config,
		Assets: fungibles::Inspect<T::AccountId, Balance = BalanceOf<T>>,
		Id: Get<<Assets as fungibles::Inspect<T::AccountId>>::AssetId>,
	{
		<Assets as fungibles::Inspect<T::AccountId>>::reducible_balance(
			Id::get(),
			who,
			Preservation::Preserve,
			Fortitude::Polite,
		)
	}
}

impl<T, Assets, Id> GasPayment<T> for PGasPayment<Assets, Id>
where
	T: Config,
	Assets: fungibles::Mutate<T::AccountId, Balance = BalanceOf<T>>
		+ fungibles::MutateHold<T::AccountId>,
	<Assets as fungibles::InspectHold<T::AccountId>>::Reason: From<HoldReason>,
	Id: Get<<Assets as fungibles::Inspect<T::AccountId>>::AssetId>,
{
	fn transfer(from: &T::AccountId, to: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
		if Self::pgas_reducible_balance::<T>(from) >= amount {
			<Assets as fungibles::Mutate<T::AccountId>>::transfer(
				Id::get(),
				from,
				to,
				amount,
				Preservation::Preserve,
			)
			.map(|_| ())
		} else {
			T::Currency::transfer(from, to, amount, Preservation::Preserve)?;
			<Self as GasPayment<T>>::record_dot_contribution(from, to, amount);
			Ok(())
		}
	}

	fn transfer_and_hold(
		reason: HoldReason,
		from: &T::AccountId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		if Self::pgas_reducible_balance::<T>(from) >= amount {
			<Assets as fungibles::MutateHold<T::AccountId>>::transfer_and_hold(
				Id::get(),
				&reason.into(),
				from,
				to,
				amount,
				Precision::Exact,
				Preservation::Preserve,
				Fortitude::Polite,
			)
			.map(|_| ())
		} else {
			T::Currency::transfer_and_hold(
				&reason.into(),
				from,
				to,
				amount,
				Precision::Exact,
				Preservation::Preserve,
				Fortitude::Polite,
			)?;
			<Self as GasPayment<T>>::record_dot_contribution(from, to, amount);
			Ok(())
		}
	}

	fn refund_on_hold(
		reason: HoldReason,
		from: &T::AccountId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		let contribution = DotByContractUser::<T>::get(from, to);
		let dot_requested = amount.min(contribution);

		let dot_refunded = if !dot_requested.is_zero() {
			let refunded = T::Currency::transfer_on_hold(
				&reason.into(),
				from,
				to,
				dot_requested,
				Precision::BestEffort,
				Restriction::Free,
				Fortitude::Polite,
			)?;
			DotByContractUser::<T>::mutate(from, to, |entitlement| {
				*entitlement = entitlement.saturating_sub(refunded);
			});
			refunded
		} else {
			BalanceOf::<T>::zero()
		};

		let pgas_needed = amount.saturating_sub(dot_refunded);
		if !pgas_needed.is_zero() {
			<Assets as fungibles::MutateHold<T::AccountId>>::transfer_on_hold(
				Id::get(),
				&reason.into(),
				from,
				to,
				pgas_needed,
				Precision::Exact,
				Restriction::Free,
				Fortitude::Polite,
			)?;
		}
		Ok(())
	}

	fn total_on_hold(reason: HoldReason, who: &T::AccountId) -> BalanceOf<T> {
		let dot_held = T::Currency::balance_on_hold(&reason.into(), who);
		let pgas_held = <Assets as fungibles::InspectHold<T::AccountId>>::balance_on_hold(
			Id::get(),
			&reason.into(),
			who,
		);
		dot_held.saturating_add(pgas_held)
	}

	fn collect_on_hold(
		reason: HoldReason,
		from: &T::AccountId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		let contribution = DotByContractUser::<T>::get(from, to);
		let dot_requested = amount.min(contribution);

		let dot_collected = if !dot_requested.is_zero() {
			let released =
				T::Currency::release(&reason.into(), from, dot_requested, Precision::BestEffort)?;
			if !released.is_zero() {
				let credit = T::Currency::withdraw(
					from,
					released,
					Precision::Exact,
					Preservation::Preserve,
					Fortitude::Polite,
				)?;
				T::FeeInfo::deposit_txfee(credit);
				DotByContractUser::<T>::mutate(from, to, |entitlement| {
					*entitlement = entitlement.saturating_sub(released);
				});
			}
			released
		} else {
			BalanceOf::<T>::zero()
		};

		let pgas_needed = amount.saturating_sub(dot_collected);
		if !pgas_needed.is_zero() {
			<Assets as fungibles::MutateHold<T::AccountId>>::transfer_on_hold(
				Id::get(),
				&reason.into(),
				from,
				to,
				pgas_needed,
				Precision::Exact,
				Restriction::Free,
				Fortitude::Polite,
			)?;
		}
		Ok(())
	}
}
