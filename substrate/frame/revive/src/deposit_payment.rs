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
use crate::{evm::fees::InfoT as FeeInfo, BalanceOf, Config, HoldReason, NativeDepositOf};
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
	DispatchResult, Perbill,
};

mod sealed {
	use super::PGasDeposit;

	pub trait Sealed {}

	impl Sealed for () {}

	impl<Mutator, Holder, Id, RefundPercent> Sealed
		for PGasDeposit<Mutator, Holder, Id, RefundPercent>
	{
	}
}

/// Payment backend used to charge storage deposits.
pub trait Deposit<T: Config>: sealed::Sealed {
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
	fn total_on_hold(reason: HoldReason, who: &T::AccountId) -> BalanceOf<T>;

	/// Burn the native currency held on `contract` under `reason` and replace it with the same
	/// amount of PGAS, minted into `contract` and placed on hold under the same reason.
	fn migrate_native_to_pgas(
		reason: HoldReason,
		contract: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult;

	/// Record that user `from` contributed `amount` in native balance to contract `to`.
	fn record_native_deposit(from: &T::AccountId, to: &T::AccountId, amount: BalanceOf<T>) {
		NativeDepositOf::<T>::mutate(to, from, |entitlement| {
			*entitlement = entitlement.saturating_add(amount);
		});
	}
}

/// Default backend: every storage deposit charge goes through the native currency.
impl<T: Config> Deposit<T> for () {
	fn transfer(from: &T::AccountId, to: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
		T::Currency::transfer(from, to, amount, Preservation::Preserve)?;
		<Self as Deposit<T>>::record_native_deposit(from, to, amount);
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
		<Self as Deposit<T>>::record_native_deposit(from, to, amount);
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
		let contribution = NativeDepositOf::<T>::get(from, to);
		NativeDepositOf::<T>::mutate(from, to, |entitlement| {
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
		let contribution = NativeDepositOf::<T>::get(from, to);
		NativeDepositOf::<T>::mutate(from, to, |entitlement| {
			*entitlement = entitlement.saturating_sub(amount.min(contribution));
		});
		Ok(())
	}

	fn total_on_hold(reason: HoldReason, who: &T::AccountId) -> BalanceOf<T> {
		T::Currency::balance_on_hold(&reason.into(), who)
	}

	fn migrate_native_to_pgas(
		_reason: HoldReason,
		_contract: &T::AccountId,
		_amount: BalanceOf<T>,
	) -> DispatchResult {
		Ok(())
	}
}

/// PGAS-backed payment backend. Charges prefer PGAS and fall back to the native currency;
/// refunds return native first (capped by [`NativeDepositOf`]) then `RefundPercent` of the
/// PGAS portion, burning the rest.
pub struct PGasDeposit<Mutator, Holder, Id, RefundPercent>(
	PhantomData<(Mutator, Holder, Id, RefundPercent)>,
);

impl<Mutator, Holder, Id, RefundPercent> PGasDeposit<Mutator, Holder, Id, RefundPercent> {
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

impl<T, Mutator, Holder, Id, RefundPercent> Deposit<T>
	for PGasDeposit<Mutator, Holder, Id, RefundPercent>
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
	RefundPercent: Get<Perbill>,
{
	/// Pays the full `amount` in PGAS when `from`'s reducible PGAS covers it; otherwise pays
	/// in native currency and records the contribution in [`NativeDepositOf`].
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
			<Self as Deposit<T>>::record_native_deposit(from, to, amount);
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
			<Self as Deposit<T>>::record_native_deposit(from, to, amount);
			Ok(())
		}
	}

	/// Refunds DOT first (capped by [`NativeDepositOf`]); any shortfall is taken from PGAS
	/// with `RefundPercent` refunded and the rest burned.
	fn refund_on_hold(
		reason: HoldReason,
		from: &T::AccountId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		with_storage_layer(|| {
			let contribution = NativeDepositOf::<T>::get(from, to);
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
				NativeDepositOf::<T>::mutate(from, to, |entitlement| {
					*entitlement = entitlement.saturating_sub(refunded);
				});
				refunded
			} else {
				BalanceOf::<T>::zero()
			};

			let pgas_needed = amount.saturating_sub(dot_refunded);
			Self::settle_pgas_refund::<T>(reason, from, to, pgas_needed)?;
			Ok(())
		})
	}

	/// Sum of `who`'s DOT-on-hold and PGAS-on-hold for `reason`.
	fn total_on_hold(reason: HoldReason, who: &T::AccountId) -> BalanceOf<T> {
		let dot_held = T::Currency::balance_on_hold(&reason.into(), who);
		let pgas_held = <Holder as fungibles::InspectHold<T::AccountId>>::balance_on_hold(
			Id::get(),
			&reason.into(),
			who,
		);
		dot_held.saturating_add(pgas_held)
	}

	/// Collects DOT first into the tx fee pool (capped by [`NativeDepositOf`]); any shortfall
	/// is taken from PGAS with `RefundPercent` refunded and the rest burned.
	fn collect_on_hold(
		reason: HoldReason,
		from: &T::AccountId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		with_storage_layer(|| {
			let contribution = NativeDepositOf::<T>::get(from, to);
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
					NativeDepositOf::<T>::mutate(from, to, |entitlement| {
						*entitlement = entitlement.saturating_sub(released);
					});
				}
				released
			} else {
				BalanceOf::<T>::zero()
			};

			let pgas_needed = amount.saturating_sub(dot_collected);
			Self::settle_pgas_refund::<T>(reason, from, to, pgas_needed)?;
			Ok(())
		})
	}

	/// Burn the native hold at `contract` under `reason`, then mint the same amount of PGAS
	/// into `contract` and place it on hold under the same reason.
	fn migrate_native_to_pgas(
		reason: HoldReason,
		contract: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		if amount.is_zero() {
			return Ok(());
		}
		T::Currency::burn_held(
			&reason.into(),
			contract,
			amount,
			Precision::Exact,
			Fortitude::Polite,
		)?;

		<Mutator as fungibles::Mutate<T::AccountId>>::mint_into(Id::get(), contract, amount)?;
		<Holder as fungibles::MutateHold<T::AccountId>>::hold(
			Id::get(),
			&reason.into(),
			contract,
			amount,
		)?;
		Ok(())
	}
}

impl<Mutator, Holder, Id, RefundPercent> PGasDeposit<Mutator, Holder, Id, RefundPercent> {
	/// Refund `RefundPercent` of `amount` from `from`'s PGAS hold to `to`'s free balance and
	/// burn the rest.
	fn settle_pgas_refund<T>(
		reason: HoldReason,
		from: &T::AccountId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult
	where
		T: Config,
		Holder: fungibles::MutateHold<
			T::AccountId,
			Balance = BalanceOf<T>,
			AssetId = <Mutator as fungibles::Inspect<T::AccountId>>::AssetId,
		>,
		<Holder as fungibles::InspectHold<T::AccountId>>::Reason: From<HoldReason>,
		Mutator: fungibles::Inspect<T::AccountId, Balance = BalanceOf<T>>,
		Id: Get<<Mutator as fungibles::Inspect<T::AccountId>>::AssetId>,
		RefundPercent: Get<Perbill>,
	{
		if amount.is_zero() {
			return Ok(());
		}
		let refund = RefundPercent::get().mul_floor(amount);
		if !refund.is_zero() {
			<Holder as fungibles::MutateHold<T::AccountId>>::transfer_on_hold(
				Id::get(),
				&reason.into(),
				from,
				to,
				refund,
				Precision::Exact,
				Restriction::Free,
				Fortitude::Polite,
			)?;
		}
		let burn = amount.saturating_sub(refund);
		if !burn.is_zero() {
			<Holder as fungibles::MutateHold<T::AccountId>>::burn_held(
				Id::get(),
				&reason.into(),
				from,
				burn,
				Precision::Exact,
				Fortitude::Polite,
			)?;
		}
		Ok(())
	}
}
