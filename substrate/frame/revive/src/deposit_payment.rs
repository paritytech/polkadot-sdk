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
//! Storage deposits can be backed by the native currency or by PGAS.
//! Runtimes without PGAS leave the default `()` binding,
//! which always uses the native currency.
use crate::{evm::fees::InfoT as FeeInfo, BalanceOf, Config, DotByContractUser, HoldReason};
use core::marker::PhantomData;
use frame_support::{
	storage::with_storage_layer,
	traits::{
		fungible::{Balanced as _, InspectHold as _, Mutate as _, MutateHold as _},
		tokens::{fungibles, Fortitude, Precision, Preservation, Restriction},
		Get,
	},
};
use sp_runtime::{
	traits::{Saturating, Zero},
	DispatchResult,
};

/// Payment backend used to charge storage deposits.
pub trait Deposit<T: Config> {
	/// Transfer `amount` from `from` to `to` to back a storage deposit.
	fn transfer(from: &T::AccountId, to: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult;

	/// Transfer `amount` from `from` to `to` and place it on hold under `reason`.
	fn transfer_and_hold(
		reason: HoldReason,
		from: &T::AccountId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult;

	/// Refund `amount` of held funds from contract `from` to user `to`'s free balance.
	fn refund_on_hold(
		reason: HoldReason,
		from: &T::AccountId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult;

	/// Collect `amount` of held funds from contract `from` back into the tx fee pool.
	fn collect_on_hold(
		reason: HoldReason,
		from: &T::AccountId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult;

	/// Total amount held for `who` under `reason`.
	fn total_on_hold(reason: HoldReason, who: &T::AccountId) -> BalanceOf<T> {
		T::Currency::balance_on_hold(&reason.into(), who)
	}

	/// Record that user `from` contributed `amount` in native balance to contract `to`.
	fn record_dot_contribution(from: &T::AccountId, to: &T::AccountId, amount: BalanceOf<T>) {
		DotByContractUser::<T>::mutate(to, from, |entitlement| {
			*entitlement = entitlement.saturating_add(amount);
		});
	}
}

/// Default backend: every storage deposit charge goes through the native currency.
impl<T: Config> Deposit<T> for () {
	fn transfer(from: &T::AccountId, to: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
		T::Currency::transfer(from, to, amount, Preservation::Preserve)?;
		<Self as Deposit<T>>::record_dot_contribution(from, to, amount);
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
		<Self as Deposit<T>>::record_dot_contribution(from, to, amount);
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

/// PGAS-backed payment backend.
///
/// Charges prefer PGAS when the payer has enough reducible PGAS and fall back to the native
/// currency otherwise; native-currency contributions are recorded in [`DotByContractUser`].
/// Refunds and collects try the native side first, capped by the user's tracked
/// [`DotByContractUser`] entitlement, and pull any remainder from PGAS.
pub struct PGasDeposit<Mutator, Holder, Id>(PhantomData<(Mutator, Holder, Id)>);

impl<Mutator, Holder, Id> PGasDeposit<Mutator, Holder, Id> {
	fn pgas_reducible_balance<T>(who: &T::AccountId) -> BalanceOf<T>
	where
		T: Config,
		Mutator: fungibles::Inspect<T::AccountId, Balance = BalanceOf<T>>,
		Id: Get<<Mutator as fungibles::Inspect<T::AccountId>>::AssetId>,
	{
		<Mutator as fungibles::Inspect<T::AccountId>>::reducible_balance(
			Id::get(),
			who,
			Preservation::Preserve,
			Fortitude::Polite,
		)
	}
}

impl<T, Mutator, Holder, Id> Deposit<T> for PGasDeposit<Mutator, Holder, Id>
where
	T: Config,
	Mutator: fungibles::Mutate<T::AccountId, Balance = BalanceOf<T>>,
	Holder: fungibles::MutateHold<
		T::AccountId,
		Balance = BalanceOf<T>,
		AssetId = <Mutator as fungibles::Inspect<T::AccountId>>::AssetId,
	>,
	<Holder as fungibles::InspectHold<T::AccountId>>::Reason: From<HoldReason>,
	Id: Get<<Mutator as fungibles::Inspect<T::AccountId>>::AssetId>,
{
	/// Pays the full `amount` in PGAS when `from`'s reducible PGAS covers it; otherwise pays
	/// in native currency and records the contribution in [`DotByContractUser`].
	fn transfer(from: &T::AccountId, to: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
		if Self::pgas_reducible_balance::<T>(from) >= amount {
			<Mutator as fungibles::Mutate<T::AccountId>>::transfer(
				Id::get(),
				from,
				to,
				amount,
				Preservation::Preserve,
			)
			.map(|_| ())
		} else {
			T::Currency::transfer(from, to, amount, Preservation::Preserve)?;
			<Self as Deposit<T>>::record_dot_contribution(from, to, amount);
			Ok(())
		}
	}

	/// Same asset-selection rule as [`Self::transfer`], applied to a transfer-and-hold.
	fn transfer_and_hold(
		reason: HoldReason,
		from: &T::AccountId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		if Self::pgas_reducible_balance::<T>(from) >= amount {
			<Holder as fungibles::MutateHold<T::AccountId>>::transfer_and_hold(
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
			<Self as Deposit<T>>::record_dot_contribution(from, to, amount);
			Ok(())
		}
	}

	/// Refunds DOT first, capped by `DotByContractUser[from][to]` (the user's tracked native
	/// contribution) and by the contract's actual DOT-on-hold (`Precision::BestEffort`, so a
	/// short hold does not fail). The DOT actually released is subtracted from
	/// [`DotByContractUser`], and any shortfall is covered by a `Precision::Exact` PGAS
	/// `transfer_on_hold`. The aggregate refunded is always exactly `amount`.
	///
	/// Atomic: on any sub-op failure all state changes are rolled back.
	fn refund_on_hold(
		reason: HoldReason,
		from: &T::AccountId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		with_storage_layer(|| {
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
				<Holder as fungibles::MutateHold<T::AccountId>>::transfer_on_hold(
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
		})
	}

	/// Sum of the contract's DOT-on-hold and PGAS-on-hold for `reason`.
	fn total_on_hold(reason: HoldReason, who: &T::AccountId) -> BalanceOf<T> {
		let dot_held = T::Currency::balance_on_hold(&reason.into(), who);
		let pgas_held = <Holder as fungibles::InspectHold<T::AccountId>>::balance_on_hold(
			Id::get(),
			&reason.into(),
			who,
		);
		dot_held.saturating_add(pgas_held)
	}

	/// Collects DOT first into the tx fee pool, capped like [`Self::refund_on_hold`]
	/// (`DotByContractUser[from][to]` and `Precision::BestEffort` on release), then withdraws
	/// the released amount and deposits it via [`FeeInfo::deposit_txfee`]. Any remainder
	/// cannot be fed to the DOT fee pool, so it is released from PGAS and transferred to
	/// `to`'s free PGAS balance with `Precision::Exact`.
	///
	/// Atomic: on any sub-op failure all state changes are rolled back.
	fn collect_on_hold(
		reason: HoldReason,
		from: &T::AccountId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		with_storage_layer(|| {
			let contribution = DotByContractUser::<T>::get(from, to);
			let dot_requested = amount.min(contribution);

			let dot_collected = if !dot_requested.is_zero() {
				let released = T::Currency::release(
					&reason.into(),
					from,
					dot_requested,
					Precision::BestEffort,
				)?;
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
				<Holder as fungibles::MutateHold<T::AccountId>>::transfer_on_hold(
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
		})
	}
}
