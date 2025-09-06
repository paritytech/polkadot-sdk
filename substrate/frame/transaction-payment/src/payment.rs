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

/// ! Traits and default implementation for paying transaction fees.
use crate::{Config, HoldReason, LOG_TARGET};

use core::marker::PhantomData;
use sp_runtime::{
	traits::{CheckedSub, DispatchInfoOf, PostDispatchInfoOf, Saturating, Zero},
	transaction_validity::InvalidTransaction,
};

use frame_support::{
	traits::{
		fungible::{Balanced, Credit, Inspect, MutateHold},
		tokens::{Fortitude, Precision, Preservation, WithdrawConsequence},
		Currency, ExistenceRequirement, Imbalance, OnUnbalanced, WithdrawReasons,
	},
	unsigned::TransactionValidityError,
};
use frame_system::Pallet as System;

type NegativeImbalanceOf<C, T> =
	<C as Currency<<T as frame_system::Config>::AccountId>>::NegativeImbalance;

/// Handle withdrawing, refunding and depositing of transaction fees.
pub trait OnChargeTransaction<T: Config> {
	/// The underlying integer type in which fees are calculated.
	type Balance: frame_support::traits::tokens::Balance;

	type LiquidityInfo: Default;

	/// Before the transaction is executed the payment of the transaction fees
	/// need to be secured.
	///
	/// Note: The `fee` already includes the `tip`.
	fn withdraw_fee(
		who: &T::AccountId,
		call: &T::RuntimeCall,
		dispatch_info: &DispatchInfoOf<T::RuntimeCall>,
		fee: Self::Balance,
		tip: Self::Balance,
	) -> Result<Self::LiquidityInfo, TransactionValidityError>;

	/// Check if the predicted fee from the transaction origin can be withdrawn.
	///
	/// Note: The `fee` already includes the `tip`.
	fn can_withdraw_fee(
		who: &T::AccountId,
		call: &T::RuntimeCall,
		dispatch_info: &DispatchInfoOf<T::RuntimeCall>,
		fee: Self::Balance,
		tip: Self::Balance,
	) -> Result<(), TransactionValidityError>;

	/// After the transaction was executed the actual fee can be calculated.
	/// This function should refund any overpaid fees and optionally deposit
	/// the corrected amount.
	///
	/// Note: The `fee` already includes the `tip`.
	fn correct_and_deposit_fee(
		who: &T::AccountId,
		dispatch_info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &PostDispatchInfoOf<T::RuntimeCall>,
		corrected_fee: Self::Balance,
		tip: Self::Balance,
		already_withdrawn: Self::LiquidityInfo,
	) -> Result<(), TransactionValidityError>;

	#[cfg(feature = "runtime-benchmarks")]
	fn endow_account(who: &T::AccountId, amount: Self::Balance);

	#[cfg(feature = "runtime-benchmarks")]
	fn minimum_balance() -> Self::Balance;
}

/// Implements transaction payment for a pallet implementing the [`frame_support::traits::fungible`]
/// trait (eg. pallet_balances) using an unbalance handler (implementing
/// [`OnUnbalanced`]).
///
/// The unbalance handler is given 2 unbalanceds in [`OnUnbalanced::on_unbalanceds`]: `fee` and
/// then `tip`.
pub struct FungibleAdapter<F, OU>(PhantomData<(F, OU)>);

impl<T, F, OU> OnChargeTransaction<T> for FungibleAdapter<F, OU>
where
	T: Config,
	F: Balanced<T::AccountId> + MutateHold<T::AccountId, Reason = T::RuntimeHoldReason>,
	OU: OnUnbalanced<Credit<T::AccountId, F>>,
{
	type LiquidityInfo = Option<Credit<T::AccountId, F>>;
	type Balance = <F as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

	fn withdraw_fee(
		who: &<T>::AccountId,
		_call: &<T>::RuntimeCall,
		_dispatch_info: &DispatchInfoOf<<T>::RuntimeCall>,
		fee: Self::Balance,
		tip: Self::Balance,
	) -> Result<Self::LiquidityInfo, TransactionValidityError> {
		if fee.is_zero() {
			return Ok(None)
		}

		// We need to have the account stay alive even when all the free balance is sent away.
		// Otherwise the held balance is burned before we have a chance to recover it.
		<System<T>>::inc_providers(who);

		// Put on hold so that pallets can withdraw from it in order to pay for deposits.
		F::hold(&HoldReason::Payment.into(), who, fee.saturating_sub(tip))
			.map_err(|_| InvalidTransaction::Payment)?;

		// Pallets have no way of knowing the amount of tip. Hence they have no way
		// of making sure that they don't consume the tip. This is why we exclude it
		// from the hold.
		let tip_credit =
			F::withdraw(who, tip, Precision::Exact, Preservation::Preserve, Fortitude::Polite)
				.map_err(|_| TransactionValidityError::Invalid(InvalidTransaction::Payment))?;

		Ok(Some(tip_credit))
	}

	fn can_withdraw_fee(
		who: &T::AccountId,
		_call: &T::RuntimeCall,
		_dispatch_info: &DispatchInfoOf<T::RuntimeCall>,
		fee: Self::Balance,
		_tip: Self::Balance,
	) -> Result<(), TransactionValidityError> {
		if fee.is_zero() {
			return Ok(())
		}

		match F::can_withdraw(who, fee) {
			WithdrawConsequence::Success => Ok(()),
			_ => Err(InvalidTransaction::Payment.into()),
		}
	}

	fn correct_and_deposit_fee(
		who: &<T>::AccountId,
		_dispatch_info: &DispatchInfoOf<<T>::RuntimeCall>,
		_post_info: &PostDispatchInfoOf<<T>::RuntimeCall>,
		corrected_fee: Self::Balance,
		tip: Self::Balance,
		tip_credit: Self::LiquidityInfo,
	) -> Result<(), TransactionValidityError> {
		if let Some(tip_credit) = tip_credit {
			let corrected_fee = corrected_fee.saturating_sub(tip);
			let account_dead = <System<T>>::reference_count(who) == 1;
			let available_fee =
				F::release_all(&HoldReason::Payment.into(), who, Precision::BestEffort)
					.map_err(|_| TransactionValidityError::Invalid(InvalidTransaction::Payment))?;

			// If pallets take away too much it makes the transaction invalid. They need to make
			// sure that this does not happen.
			if available_fee < corrected_fee {
				log::error!(target: LOG_TARGET, "Not enough balance on hold to pay tx fees. This is a bug.");
				Err(TransactionValidityError::Invalid(InvalidTransaction::Payment))?;
			}

			// Do not re-create account in case it is only held alive by this pallet
			let payable_fee =
				if account_dead { available_fee } else { available_fee.min(corrected_fee) };

			let adjusted_paid = F::withdraw(
				who,
				payable_fee,
				Precision::Exact,
				Preservation::Expendable,
				Fortitude::Polite,
			)
			.map_err(|_| TransactionValidityError::Invalid(InvalidTransaction::Payment))?;

			<System<T>>::dec_providers(who).expect("We increased the provider in withdraw_fee. We assume all other providers are balanced. qed");

			OU::on_unbalanceds(Some(adjusted_paid).into_iter().chain(Some(tip_credit)));
		}

		Ok(())
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn endow_account(who: &T::AccountId, amount: Self::Balance) {
		let _ = F::deposit(who, amount, Precision::BestEffort);
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn minimum_balance() -> Self::Balance {
		F::minimum_balance()
	}
}

/// Implements the transaction payment for a pallet implementing the [`Currency`]
/// trait (eg. the pallet_balances) using an unbalance handler (implementing
/// [`OnUnbalanced`]).
///
/// The unbalance handler is given 2 unbalanceds in [`OnUnbalanced::on_unbalanceds`]: `fee` and
/// then `tip`.
#[deprecated(
	note = "Please use the fungible trait and FungibleAdapter. This struct will be removed some time after March 2024."
)]
pub struct CurrencyAdapter<C, OU>(PhantomData<(C, OU)>);

/// Default implementation for a Currency and an OnUnbalanced handler.
///
/// The unbalance handler is given 2 unbalanceds in [`OnUnbalanced::on_unbalanceds`]: `fee` and
/// then `tip`.
#[allow(deprecated)]
impl<T, C, OU> OnChargeTransaction<T> for CurrencyAdapter<C, OU>
where
	T: Config,
	C: Currency<<T as frame_system::Config>::AccountId>,
	C::PositiveImbalance: Imbalance<
		<C as Currency<<T as frame_system::Config>::AccountId>>::Balance,
		Opposite = C::NegativeImbalance,
	>,
	C::NegativeImbalance: Imbalance<
		<C as Currency<<T as frame_system::Config>::AccountId>>::Balance,
		Opposite = C::PositiveImbalance,
	>,
	OU: OnUnbalanced<NegativeImbalanceOf<C, T>>,
{
	type LiquidityInfo = Option<NegativeImbalanceOf<C, T>>;
	type Balance = <C as Currency<<T as frame_system::Config>::AccountId>>::Balance;

	/// Withdraw the predicted fee from the transaction origin.
	///
	/// Note: The `fee` already includes the `tip`.
	fn withdraw_fee(
		who: &T::AccountId,
		_call: &T::RuntimeCall,
		_info: &DispatchInfoOf<T::RuntimeCall>,
		fee: Self::Balance,
		tip: Self::Balance,
	) -> Result<Self::LiquidityInfo, TransactionValidityError> {
		if fee.is_zero() {
			return Ok(None)
		}

		let withdraw_reason = if tip.is_zero() {
			WithdrawReasons::TRANSACTION_PAYMENT
		} else {
			WithdrawReasons::TRANSACTION_PAYMENT | WithdrawReasons::TIP
		};

		match C::withdraw(who, fee, withdraw_reason, ExistenceRequirement::KeepAlive) {
			Ok(imbalance) => Ok(Some(imbalance)),
			Err(_) => Err(InvalidTransaction::Payment.into()),
		}
	}

	/// Check if the predicted fee from the transaction origin can be withdrawn.
	///
	/// Note: The `fee` already includes the `tip`.
	fn can_withdraw_fee(
		who: &T::AccountId,
		_call: &T::RuntimeCall,
		_info: &DispatchInfoOf<T::RuntimeCall>,
		fee: Self::Balance,
		tip: Self::Balance,
	) -> Result<(), TransactionValidityError> {
		if fee.is_zero() {
			return Ok(())
		}

		let withdraw_reason = if tip.is_zero() {
			WithdrawReasons::TRANSACTION_PAYMENT
		} else {
			WithdrawReasons::TRANSACTION_PAYMENT | WithdrawReasons::TIP
		};

		let new_balance =
			C::free_balance(who).checked_sub(&fee).ok_or(InvalidTransaction::Payment)?;
		C::ensure_can_withdraw(who, fee, withdraw_reason, new_balance)
			.map(|_| ())
			.map_err(|_| InvalidTransaction::Payment.into())
	}

	/// Hand the fee and the tip over to the `[OnUnbalanced]` implementation.
	/// Since the predicted fee might have been too high, parts of the fee may
	/// be refunded.
	///
	/// Note: The `corrected_fee` already includes the `tip`.
	fn correct_and_deposit_fee(
		who: &T::AccountId,
		_dispatch_info: &DispatchInfoOf<T::RuntimeCall>,
		_post_info: &PostDispatchInfoOf<T::RuntimeCall>,
		corrected_fee: Self::Balance,
		tip: Self::Balance,
		already_withdrawn: Self::LiquidityInfo,
	) -> Result<(), TransactionValidityError> {
		if let Some(paid) = already_withdrawn {
			// Calculate how much refund we should return
			let refund_amount = paid.peek().saturating_sub(corrected_fee);
			// refund to the the account that paid the fees. If this fails, the
			// account might have dropped below the existential balance. In
			// that case we don't refund anything.
			let refund_imbalance = C::deposit_into_existing(who, refund_amount)
				.unwrap_or_else(|_| C::PositiveImbalance::zero());
			// merge the imbalance caused by paying the fees and refunding parts of it again.
			let adjusted_paid = paid
				.offset(refund_imbalance)
				.same()
				.map_err(|_| TransactionValidityError::Invalid(InvalidTransaction::Payment))?;
			// Call someone else to handle the imbalance (fee and tip separately)
			let (tip, fee) = adjusted_paid.split(tip);
			OU::on_unbalanceds(Some(fee).into_iter().chain(Some(tip)));
		}
		Ok(())
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn endow_account(who: &T::AccountId, amount: Self::Balance) {
		let _ = C::deposit_creating(who, amount);
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn minimum_balance() -> Self::Balance {
		C::minimum_balance()
	}
}
