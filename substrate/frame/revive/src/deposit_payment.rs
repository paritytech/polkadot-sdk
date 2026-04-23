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
use crate::{
	BalanceOf, Config, HoldReason, LOG_TARGET, NativeDepositOf, evm::fees::InfoT as FeeInfo,
};
use core::marker::PhantomData;
use frame_support::{
	storage::with_storage_layer,
	traits::{
		Get,
		fungible::{Balanced as _, Inspect as _, InspectHold as _, Mutate as _, MutateHold as _},
		tokens::{
			DepositConsequence, Fortitude, Precision, Preservation, Provenance, Restriction,
			fungibles,
		},
	},
};
use sp_runtime::{
	DispatchError, DispatchResult, Perbill, TokenError,
	traits::{Saturating, Zero},
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

/// Identifies where the native side of a storage deposit lives.
///
/// Charges treat it as the source; refunds treat it as the destination.
pub enum Funds<'a, AccountId> {
	/// The free balance of the given account.
	Balance(&'a AccountId),
	/// The tx fee pool. The embedded account is the origin, used for deposit attribution
	/// and, on refund, as the destination of any PGAS portion.
	TxFee(&'a AccountId),
}

/// Payment backend used to charge storage deposits.
pub trait Deposit<T: Config>: sealed::Sealed {
	/// Charge the existential deposit to bring `to`'s account into existence.
	///
	/// Returns the amount charged.
	///
	/// # Parameters
	/// - `src`: source of the ED. See [`Funds`].
	/// - `to`: account to bring into existence.
	fn charge_ed(
		src: Funds<T::AccountId>,
		to: &T::AccountId,
	) -> Result<BalanceOf<T>, DispatchError>;

	/// Charge `amount` from `src` to `to` and place it on hold under `reason`.
	///
	/// # Parameters
	/// - `reason`: hold reason to place the charge under.
	/// - `src`: source of the charge. See [`Funds`].
	/// - `to`: account on which the hold is placed.
	/// - `amount`: amount to charge.
	fn charge_and_hold(
		reason: HoldReason,
		src: Funds<T::AccountId>,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult;

	/// Refund `amount` of held funds from contract `from`.
	///
	/// The PGAS portion (if any) is always settled to the account embedded in `dst` under
	/// `PGasDeposit`'s `RefundPercent`.
	///
	/// # Parameters
	/// - `reason`: hold reason the funds were placed under.
	/// - `from`: contract whose hold is being released.
	/// - `dst`: destination of the refund. See [`Funds`]. Also the attribution key used to cap the
	///   native portion via [`NativeDepositOf`].
	/// - `amount`: amount to refund.
	fn refund_on_hold(
		reason: HoldReason,
		from: &T::AccountId,
		dst: Funds<T::AccountId>,
		amount: BalanceOf<T>,
	) -> DispatchResult;

	/// Total amount held for `who` under `reason`.
	///
	/// # Parameters
	/// - `reason`: hold reason to query.
	/// - `who`: account whose held balance is returned.
	fn total_on_hold(reason: HoldReason, who: &T::AccountId) -> BalanceOf<T>;

	/// Burn the native currency held on `contract` under `reason` and replace it with the same
	/// amount of PGAS, minted into `contract` and placed on hold under the same reason.
	///
	/// # Parameters
	/// - `reason`: hold reason whose balance is being migrated.
	/// - `contract`: account holding the funds to migrate.
	/// - `amount`: amount to migrate from native to PGAS.
	fn migrate_native_to_pgas(
		reason: HoldReason,
		contract: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult;
}

/// Default backend: every storage deposit charge goes through the native currency.
impl<T: Config> Deposit<T> for () {
	fn charge_ed(
		src: Funds<T::AccountId>,
		to: &T::AccountId,
	) -> Result<BalanceOf<T>, DispatchError> {
		let ed = T::Currency::minimum_balance();
		match src {
			Funds::Balance(from) => {
				T::Currency::transfer(from, to, ed, Preservation::Preserve)?;
			},
			Funds::TxFee(_) => {
				let credit = T::FeeInfo::withdraw_txfee(ed)
					.ok_or(DispatchError::Token(TokenError::FundsUnavailable))?;
				T::Currency::resolve(to, credit)
					.map_err(|_| DispatchError::Token(TokenError::FundsUnavailable))?;
			},
		}
		Ok(ed)
	}

	fn charge_and_hold(
		reason: HoldReason,
		src: Funds<T::AccountId>,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		match src {
			Funds::Balance(from) => {
				T::Currency::transfer_and_hold(
					&reason.into(),
					from,
					to,
					amount,
					Precision::Exact,
					Preservation::Preserve,
					Fortitude::Polite,
				)?;
			},
			Funds::TxFee(_) => {
				let credit = T::FeeInfo::withdraw_txfee(amount)
					.ok_or(DispatchError::Token(TokenError::FundsUnavailable))?;
				T::Currency::resolve(to, credit)
					.map_err(|_| DispatchError::Token(TokenError::FundsUnavailable))?;
				T::Currency::hold(&reason.into(), to, amount)?;
			},
		}
		Ok(())
	}

	fn refund_on_hold(
		reason: HoldReason,
		from: &T::AccountId,
		dst: Funds<T::AccountId>,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		match dst {
			Funds::Balance(to) => {
				T::Currency::transfer_on_hold(
					&reason.into(),
					from,
					to,
					amount,
					Precision::Exact,
					Restriction::Free,
					Fortitude::Polite,
				)?;
			},
			Funds::TxFee(_) => {
				let released =
					T::Currency::release(&reason.into(), from, amount, Precision::Exact)?;
				let credit = T::Currency::withdraw(
					from,
					released,
					Precision::Exact,
					Preservation::Preserve,
					Fortitude::Polite,
				)?;
				T::FeeInfo::deposit_txfee(credit);
			},
		}
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

	/// Record that user `from` contributed `amount` in native balance to contract `to`.
	/// Read by [`Self::refund_on_hold`] to cap the native portion of refunds.
	fn record_native_deposit<T: Config>(
		from: &T::AccountId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) {
		NativeDepositOf::<T>::mutate(to, from, |entitlement| {
			*entitlement = entitlement.saturating_add(amount);
		});
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
	/// Picks the asset and amount internally: uses the PGAS asset's ED when the payer has enough
	/// reducible PGAS, otherwise falls back to the native currency ED and records the
	/// contribution in [`NativeDepositOf`]. Returns the amount actually charged.
	///
	/// When `src` is [`Funds::TxFee`] (eth-tx dispatch), the native ED is withdrawn from the
	/// txfee pool regardless of the payer's PGAS balance.
	fn charge_ed(
		src: Funds<T::AccountId>,
		to: &T::AccountId,
	) -> Result<BalanceOf<T>, DispatchError> {
		match src {
			Funds::TxFee(origin) => {
				let native_ed = T::Currency::minimum_balance();
				let credit = T::FeeInfo::withdraw_txfee(native_ed)
					.ok_or(DispatchError::Token(TokenError::FundsUnavailable))?;
				T::Currency::resolve(to, credit)
					.map_err(|_| DispatchError::Token(TokenError::FundsUnavailable))?;
				Self::record_native_deposit::<T>(origin, to, native_ed);
				Ok(native_ed)
			},
			Funds::Balance(from) => {
				let pgas_ed =
					<Mutator as fungibles::Inspect<T::AccountId>>::minimum_balance(Id::get());
				if Self::pgas_reducible_balance::<T>(from) >= pgas_ed {
					<Mutator as fungibles::Mutate<T::AccountId>>::transfer(
						Id::get(),
						from,
						to,
						pgas_ed,
						Preservation::Preserve,
					)?;
					Ok(pgas_ed)
				} else {
					let native_ed = T::Currency::minimum_balance();
					T::Currency::transfer(from, to, native_ed, Preservation::Preserve)?;
					Self::record_native_deposit::<T>(from, to, native_ed);
					Ok(native_ed)
				}
			},
		}
	}

	/// Same asset-selection rule as [`Self::charge_ed`], applied to a transfer-and-hold.
	///
	/// The PGAS branch writes directly to the payer's free balance and the contract's hold
	/// storage, so the contract never needs a `pallet_assets::Account` entry. This allows
	/// deposits below the PGAS existential deposit to succeed.
	///
	/// When `src` is [`Funds::TxFee`] (eth-tx dispatch), `amount` is withdrawn from the txfee
	/// pool and placed on hold at `to` regardless of the payer's PGAS balance.
	fn charge_and_hold(
		reason: HoldReason,
		src: Funds<T::AccountId>,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		let from = match src {
			Funds::TxFee(origin) => {
				let credit = T::FeeInfo::withdraw_txfee(amount)
					.ok_or(DispatchError::Token(TokenError::FundsUnavailable))?;
				T::Currency::resolve(to, credit)
					.map_err(|_| DispatchError::Token(TokenError::FundsUnavailable))?;
				T::Currency::hold(&reason.into(), to, amount)?;
				Self::record_native_deposit::<T>(origin, to, amount);
				return Ok(());
			},
			Funds::Balance(from) => from,
		};

		if Self::pgas_reducible_balance::<T>(from) >= amount {
			<Mutator as fungibles::Unbalanced<T::AccountId>>::decrease_balance(
				Id::get(),
				from,
				amount,
				Precision::Exact,
				Preservation::Preserve,
				Fortitude::Polite,
			)?;
			<Holder as fungibles::hold::Unbalanced<T::AccountId>>::increase_balance_on_hold(
				Id::get(),
				&reason.into(),
				to,
				amount,
				Precision::Exact,
			)?;
			Ok(())
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
			Self::record_native_deposit::<T>(from, to, amount);
			Ok(())
		}
	}

	/// Refunds native currency first (capped by [`NativeDepositOf`]); any shortfall is taken from
	/// PGAS with `RefundPercent` refunded and the rest burned. When `dst` is [`Funds::TxFee`],
	/// the native portion is routed into the tx fee pool instead of the embedded account's
	/// free balance.
	fn refund_on_hold(
		reason: HoldReason,
		from: &T::AccountId,
		dst: Funds<T::AccountId>,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		let (to, to_txfee) = match dst {
			Funds::Balance(to) => (to, false),
			Funds::TxFee(origin) => (origin, true),
		};
		with_storage_layer(|| {
			let contribution = NativeDepositOf::<T>::get(from, to);
			let dot_requested = amount.min(contribution);

			let dot_refunded = if !dot_requested.is_zero() {
				let refunded = if to_txfee {
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
					}
					released
				} else {
					T::Currency::transfer_on_hold(
						&reason.into(),
						from,
						to,
						dot_requested,
						Precision::BestEffort,
						Restriction::Free,
						Fortitude::Polite,
					)?
				};
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

	/// Sum of `who`'s native and PGAS balances on hold for `reason`.
	fn total_on_hold(reason: HoldReason, who: &T::AccountId) -> BalanceOf<T> {
		let dot_held = T::Currency::balance_on_hold(&reason.into(), who);
		let pgas_held = <Holder as fungibles::InspectHold<T::AccountId>>::balance_on_hold(
			Id::get(),
			&reason.into(),
			who,
		);
		dot_held.saturating_add(pgas_held)
	}

	/// Burn the native hold at `contract` under `reason`, then mint the same amount of PGAS
	/// directly onto a hold on `contract` under the same reason.
	///
	/// Writes to the holder pallet storage without creating a `pallet_assets::Account` for
	/// the contract, so no existential deposit is minted alongside. The PGAS supply is
	/// bumped by exactly `amount`; `burn_held` on refund/termination decrements it back.
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
		)
		.inspect_err(
			|err| log::debug!(target: LOG_TARGET, "Failed to burn held amount {amount:?}: {err:?}"),
		)?;

		let new_supply = <Mutator as fungibles::Inspect<T::AccountId>>::total_issuance(Id::get())
			.saturating_add(amount);
		<Mutator as fungibles::Unbalanced<T::AccountId>>::set_total_issuance(Id::get(), new_supply);
		<Holder as fungibles::hold::Unbalanced<T::AccountId>>::increase_balance_on_hold(
			Id::get(),
			&reason.into(),
			contract,
			amount,
			Precision::Exact,
		)
		.inspect_err(
			|err| log::debug!(target: LOG_TARGET, "Failed to hold amount: {amount:?}: {err:?}"),
		)?;
		Ok(())
	}
}

impl<Mutator, Holder, Id, RefundPercent> PGasDeposit<Mutator, Holder, Id, RefundPercent> {
	/// Refund `RefundPercent` of `amount` from `from`'s PGAS hold to `to`'s free balance and
	/// burn the rest.
	///
	/// If crediting `to` would violate its existential deposit (e.g. `to` has no asset
	/// account and the refund would create one below ED), the refund portion is folded into
	/// the burn rather than aborting the whole refund.
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
		Mutator: fungibles::Mutate<T::AccountId, Balance = BalanceOf<T>>,
		Id: Get<<Mutator as fungibles::Inspect<T::AccountId>>::AssetId>,
		RefundPercent: Get<Perbill>,
	{
		if amount.is_zero() {
			return Ok(());
		}
		let refund = RefundPercent::get().mul_floor(amount);
		let mut burn = amount.saturating_sub(refund);

		if !refund.is_zero() {
			let can_credit = matches!(
				<Mutator as fungibles::Inspect<T::AccountId>>::can_deposit(
					Id::get(),
					to,
					refund,
					Provenance::Extant,
				),
				DepositConsequence::Success
			);
			if can_credit {
				<Holder as fungibles::hold::Unbalanced<T::AccountId>>::decrease_balance_on_hold(
					Id::get(),
					&reason.into(),
					from,
					refund,
					Precision::Exact,
				)?;
				<Mutator as fungibles::Unbalanced<T::AccountId>>::increase_balance(
					Id::get(),
					to,
					refund,
					Precision::Exact,
				)?;
			} else {
				burn = burn.saturating_add(refund);
			}
		}

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
