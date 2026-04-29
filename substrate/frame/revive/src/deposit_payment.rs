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
use frame_support::traits::{
	Get,
	fungible::{
		Balanced as _, Inspect as _, InspectHold as _, Mutate as _, MutateHold as _,
		Unbalanced as _,
	},
	tokens::{
		DepositConsequence, Fortitude, Precision, Preservation, Provenance, Restriction, fungibles,
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
	/// The tx fee hold.
	TxFee(&'a AccountId),
}

/// Payment backend used to charge storage deposits.
pub trait Deposit<T: Config>: sealed::Sealed {
	/// Mint each backend's existential deposit into `contract`.
	///
	/// Used by [`crate::exec`] when bringing a new contract account into existence.
	fn init_contract(contract: &T::AccountId) -> DispatchResult;

	/// Tear down the per-backend balance state that [`Self::init_contract`] set up.
	///
	/// Used by [`crate::exec::Stack::do_terminate`] when destroying a contract.
	fn destroy_contract(contract: &T::AccountId) -> DispatchResult;

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

	/// Refund every storage-deposit fund held on `from` to `dst`, ignoring the per-contributor
	/// caps that govern partial refunds. Used at contract termination.
	///
	/// Returns the total amount released, so the storage meter can finalise its deposit
	/// accounting.
	///
	/// # Parameters
	/// - `from`: contract whose hold is being released.
	/// - `dst`: destination of the refund. See [`Funds`].
	fn refund_all(
		from: &T::AccountId,
		dst: Funds<T::AccountId>,
	) -> Result<BalanceOf<T>, DispatchError>;

	/// Burn the native currency held on `contract` under `reason` and replace it with the same
	/// amount of PGAS, minted into `contract` and placed on hold under the same reason.
	///
	/// Only used by the v4 multi-block migration (see [`crate::migrations::v4`]) to move
	/// pre-existing native storage deposits over to PGAS. Not part of the regular charge/refund
	/// flow.
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
	/// The native ED is freshly minted and immediately
	/// [`deactivated`](frame_support::traits::fungible::Unbalanced::deactivate) so that
	/// active issuance, and therefore opengov conviction, inflation accounting, etc., is
	/// undisturbed by contract creation. The contract holds a system consumer for as long as it
	/// exists, so this minted ED is not extractable: the account cannot be reaped.
	fn init_contract(to: &T::AccountId) -> DispatchResult {
		let ed = T::Currency::minimum_balance();
		T::Currency::mint_into(to, ed)?;
		T::Currency::deactivate(ed);
		Ok(())
	}

	fn destroy_contract(contract: &T::AccountId) -> DispatchResult {
		let ed = T::Currency::minimum_balance();
		T::Currency::burn_from(
			contract,
			ed,
			Preservation::Expendable,
			Precision::BestEffort,
			Fortitude::Polite,
		)?;
		// Pair with [`Self::init_contract`]: shrink the inactive pool first so the burn only
		// nets out the mint, rather than also taking an ED off the active issuance.
		T::Currency::reactivate(ed);
		Ok(())
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

	fn refund_all(
		from: &T::AccountId,
		dst: Funds<T::AccountId>,
	) -> Result<BalanceOf<T>, DispatchError> {
		let reason = HoldReason::StorageDepositReserve;
		let amount = T::Currency::balance_on_hold(&reason.into(), from);
		if !amount.is_zero() {
			<Self as Deposit<T>>::refund_on_hold(reason, from, dst, amount)?;
		}
		Ok(amount)
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
			Preservation::Expendable,
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
	/// Mints one native ED and one PGAS ED into `to`, so the account can subsequently receive
	/// deposits in either asset without tripping existential-deposit checks. The minted native
	/// ED is [`deactivated`](frame_support::traits::fungible::Unbalanced::deactivate) so it stays
	/// outside active issuance.
	fn init_contract(to: &T::AccountId) -> DispatchResult {
		<() as Deposit<T>>::init_contract(to)?;
		<Mutator as fungibles::Mutate<T::AccountId>>::mint_into(
			Id::get(),
			to,
			<Mutator as fungibles::Inspect<T::AccountId>>::minimum_balance(Id::get()),
		)?;
		Ok(())
	}

	/// Burns the native ED minted by [`Self::init_contract`] (reactivating it first so the burn
	/// doesn't also eat into active issuance) and burns the contract's full PGAS free balance.
	/// Burning everything (rather than just the PGAS ED) ensures no PGAS lingers on a destroyed
	/// contract: any free PGAS sent to it directly, or left over from accounting, is removed.
	fn destroy_contract(contract: &T::AccountId) -> DispatchResult {
		<() as Deposit<T>>::destroy_contract(contract)?;
		let pgas_balance =
			<Mutator as fungibles::Inspect<T::AccountId>>::balance(Id::get(), contract);
		if !pgas_balance.is_zero() {
			<Mutator as fungibles::Mutate<T::AccountId>>::burn_from(
				Id::get(),
				contract,
				pgas_balance,
				Preservation::Expendable,
				Precision::BestEffort,
				Fortitude::Polite,
			)?;
		}
		Ok(())
	}

	/// Charges a deposit and places it on hold.
	///
	/// Uses PGAS when the payer has enough reducible PGAS, otherwise falls back to the native
	/// currency and records the contribution in [`NativeDepositOf`] so refunds return native up
	/// to the contributed amount. The native fallback honours [`Funds::TxFee`] by withdrawing
	/// from the txfee pool instead of the payer's free balance.
	fn charge_and_hold(
		reason: HoldReason,
		src: Funds<T::AccountId>,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		let from = match &src {
			Funds::Balance(from) | Funds::TxFee(from) => *from,
		};

		if Self::pgas_reducible_balance::<T>(from) >= amount {
			<Holder as fungibles::MutateHold<T::AccountId>>::transfer_and_hold(
				Id::get(),
				&reason.into(),
				from,
				to,
				amount,
				Precision::Exact,
				Preservation::Expendable,
				Fortitude::Polite,
			)?;
			Ok(())
		} else {
			<() as Deposit<T>>::charge_and_hold(reason, src, to, amount)?;
			Self::record_native_deposit::<T>(from, to, amount);
			Ok(())
		}
	}

	/// Refunds native currency first (capped by [`NativeDepositOf`]); any shortfall is taken from
	/// PGAS with `RefundPercent` refunded and the rest burned. When `dst` is [`Funds::TxFee`],
	/// the native portion is routed into the tx fee pool instead of the embedded account's
	/// free balance. The PGAS portion (if any) is always settled to the account embedded in
	/// `dst`.
	///
	/// Note: callers must run inside a storage layer so partial state rolls back on error.
	fn refund_on_hold(
		reason: HoldReason,
		from: &T::AccountId,
		dst: Funds<T::AccountId>,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		let to = match &dst {
			Funds::Balance(to) | Funds::TxFee(to) => *to,
		};
		let contribution = NativeDepositOf::<T>::get(from, to);
		let native_requested = amount.min(contribution);

		let native_refunded = if !native_requested.is_zero() {
			<() as Deposit<T>>::refund_on_hold(reason, from, dst, native_requested)?;
			let new_val = contribution.saturating_sub(native_requested);
			if new_val.is_zero() {
				NativeDepositOf::<T>::remove(from, to);
			} else {
				NativeDepositOf::<T>::insert(from, to, new_val);
			}
			native_requested
		} else {
			BalanceOf::<T>::zero()
		};

		let pgas_needed = amount.saturating_sub(native_refunded);
		Self::settle_pgas_refund::<T>(reason, from, to, pgas_needed)?;
		Ok(())
	}

	/// Sum of `who`'s native and PGAS balances on hold for `reason`.
	fn total_on_hold(reason: HoldReason, who: &T::AccountId) -> BalanceOf<T> {
		let native_held = <() as Deposit<T>>::total_on_hold(reason, who);
		let pgas_held = <Holder as fungibles::InspectHold<T::AccountId>>::balance_on_hold(
			Id::get(),
			&reason.into(),
			who,
		);
		native_held.saturating_add(pgas_held)
	}

	/// Refunds the full native hold to `dst` ignoring the per-contributor cap, then settles the
	/// PGAS hold via [`Self::settle_pgas_refund`] (refunding `RefundPercent` to `dst` and burning
	/// the rest). The native cap only makes sense for partial refunds on a live contract; at
	/// termination there is one recipient and the contract is gone.
	///
	/// Note: callers must run inside a storage layer so partial state rolls back on error.
	fn refund_all(
		from: &T::AccountId,
		dst: Funds<T::AccountId>,
	) -> Result<BalanceOf<T>, DispatchError> {
		let reason = HoldReason::StorageDepositReserve;
		let to = match &dst {
			Funds::Balance(to) | Funds::TxFee(to) => *to,
		};
		let native = <() as Deposit<T>>::refund_all(from, dst)?;

		let pgas = <Holder as fungibles::InspectHold<T::AccountId>>::balance_on_hold(
			Id::get(),
			&reason.into(),
			from,
		);
		let pgas_refunded = if !pgas.is_zero() {
			Self::settle_pgas_refund::<T>(reason, from, to, pgas)?
		} else {
			BalanceOf::<T>::zero()
		};

		Ok(native.saturating_add(pgas_refunded))
	}

	/// Bring a pre-existing contract up to the post-[`Self::init_contract`] invariant:
	/// mint the PGAS ED into `contract`'s free balance if it is missing, then burn the native
	/// hold under `reason` and replace it with the same amount of PGAS held on `contract`.
	fn migrate_native_to_pgas(
		reason: HoldReason,
		contract: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		let pgas_ed = <Mutator as fungibles::Inspect<T::AccountId>>::minimum_balance(Id::get());
		if <Mutator as fungibles::Inspect<T::AccountId>>::balance(Id::get(), contract).is_zero() {
			<Mutator as fungibles::Mutate<T::AccountId>>::mint_into(Id::get(), contract, pgas_ed)
				.inspect_err(|err| {
				log::debug!(
					target: LOG_TARGET,
					"Failed to mint PGAS ED for contract: {err:?}",
				)
			})?;
		}

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

		<Mutator as fungibles::Mutate<T::AccountId>>::mint_into(Id::get(), contract, amount)
			.inspect_err(
				|err| log::debug!(target: LOG_TARGET, "Failed to mint to {contract:?} amount: {amount:?}: {err:?}"),
			)?;

		<Holder as fungibles::MutateHold<T::AccountId>>::hold(
			Id::get(),
			&reason.into(),
			contract,
			amount,
		)
		.inspect_err(
			|err| log::debug!(target: LOG_TARGET, "Failed to hold amount in {contract:?}: {amount:?}: {err:?}"),
		)?;
		Ok(())
	}
}

impl<Mutator, Holder, Id, RefundPercent> PGasDeposit<Mutator, Holder, Id, RefundPercent> {
	/// Refund `RefundPercent` of `amount` from `from`'s PGAS hold to `to`'s free balance and
	/// burn the rest. Returns the amount actually transferred to `to` (excludes the burned
	/// portion).
	///
	/// If crediting `to` would violate its existential deposit (e.g. `to` has no asset
	/// account and the refund would create one below ED), the refund portion is folded into
	/// the burn rather than aborting the whole refund.
	///
	/// `amount` is capped at the PGAS actually held by `from`: when a recipient with no
	/// [`NativeDepositOf`] credit triggers a refund on a contract whose deposit was paid in
	/// native, the call settles whatever PGAS is actually held instead of reverting.
	fn settle_pgas_refund<T>(
		reason: HoldReason,
		from: &T::AccountId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> Result<BalanceOf<T>, DispatchError>
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
			return Ok(BalanceOf::<T>::zero());
		}
		// Cap the amount we settle at what's actually held in PGAS. A refund recipient with
		// no `NativeDepositOf` credit on a contract whose deposit was paid in native would
		// otherwise route the full amount through PGAS and revert on `Precision::Exact`.
		let pgas_held = <Holder as fungibles::InspectHold<T::AccountId>>::balance_on_hold(
			Id::get(),
			&reason.into(),
			from,
		);
		let amount = amount.min(pgas_held);
		if amount.is_zero() {
			return Ok(BalanceOf::<T>::zero());
		}
		let refund = RefundPercent::get().mul_floor(amount);
		let mut burn = amount.saturating_sub(refund);
		let mut refunded = BalanceOf::<T>::zero();

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
				refunded = <Holder as fungibles::MutateHold<T::AccountId>>::transfer_on_hold(
					Id::get(),
					&reason.into(),
					from,
					to,
					refund,
					Precision::BestEffort,
					Restriction::Free,
					Fortitude::Polite,
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
		Ok(refunded)
	}
}
