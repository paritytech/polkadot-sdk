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
use crate::{Config, Pallet, TxPaymentCredit, LOG_TARGET};

use codec::{DecodeWithMemTracking, FullCodec, MaxEncodedLen};
use core::marker::PhantomData;
use frame_support::{
	traits::{
		fungible::{Balanced, Credit, Inspect},
		tokens::{Precision, WithdrawConsequence},
		Currency, ExistenceRequirement, Imbalance, NoDrop, OnUnbalanced, SuppressedDrop,
		WithdrawReasons,
	},
	unsigned::TransactionValidityError,
};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{CheckedSub, DispatchInfoOf, PostDispatchInfoOf, Saturating, Zero},
	transaction_validity::InvalidTransaction,
};

type NegativeImbalanceOf<C, T> =
	<C as Currency<<T as frame_system::Config>::AccountId>>::NegativeImbalance;

/// Handle withdrawing, refunding and depositing of transaction fees.
pub trait OnChargeTransaction<T: Config>: TxCreditHold<T> {
	/// The underlying integer type in which fees are calculated.
	type Balance: frame_support::traits::tokens::Balance;

	type LiquidityInfo: Default;

	/// Before the transaction is executed the payment of the transaction fees
	/// need to be secured.
	///
	/// Returns the tip credit
	fn withdraw_fee(
		who: &T::AccountId,
		call: &T::RuntimeCall,
		dispatch_info: &DispatchInfoOf<T::RuntimeCall>,
		fee_with_tip: Self::Balance,
		tip: Self::Balance,
	) -> Result<Self::LiquidityInfo, TransactionValidityError>;

	/// Check if the predicted fee from the transaction origin can be withdrawn.
	fn can_withdraw_fee(
		who: &T::AccountId,
		call: &T::RuntimeCall,
		dispatch_info: &DispatchInfoOf<T::RuntimeCall>,
		fee_with_tip: Self::Balance,
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
		corrected_fee_with_tip: Self::Balance,
		tip: Self::Balance,
		liquidity_info: Self::LiquidityInfo,
	) -> Result<(), TransactionValidityError>;

	#[cfg(feature = "runtime-benchmarks")]
	fn endow_account(who: &T::AccountId, amount: Self::Balance);

	#[cfg(feature = "runtime-benchmarks")]
	fn minimum_balance() -> Self::Balance;
}

/// Needs to be implemented for every [`OnChargeTransaction`].
///
/// Cannot be added to `OnChargeTransaction` directly as this would
/// cause cycles in trait resolution.
pub trait TxCreditHold<T: Config> {
	/// The credit that is used to represent the withdrawn transaction fees.
	///
	/// The pallet will put this into a temporary storage item in order to
	/// make it available to other pallets during tx application.
	///
	/// Is only used within a transaction. Hence changes to the encoding of this
	/// type **won't** require a storage migration.
	///
	/// Set to `()` if your `OnChargeTransaction` impl does not store the credit.
	type Credit: FullCodec + DecodeWithMemTracking + MaxEncodedLen + TypeInfo + SuppressedDrop;
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
	T::OnChargeTransaction: TxCreditHold<T, Credit = NoDrop<Credit<T::AccountId, F>>>,
	F: Balanced<T::AccountId> + 'static,
	OU: OnUnbalanced<<Self::Credit as SuppressedDrop>::Inner>,
{
	type LiquidityInfo = Option<<Self::Credit as SuppressedDrop>::Inner>;
	type Balance = <F as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

	fn withdraw_fee(
		who: &<T>::AccountId,
		_call: &<T>::RuntimeCall,
		_dispatch_info: &DispatchInfoOf<<T>::RuntimeCall>,
		fee_with_tip: Self::Balance,
		tip: Self::Balance,
	) -> Result<Self::LiquidityInfo, TransactionValidityError> {
		if fee_with_tip.is_zero() {
			return Ok(None)
		}

		let credit = F::withdraw(
			who,
			fee_with_tip,
			Precision::Exact,
			frame_support::traits::tokens::Preservation::Preserve,
			frame_support::traits::tokens::Fortitude::Polite,
		)
		.map_err(|_| InvalidTransaction::Payment)?;

		let (tip_credit, inclusion_fee) = credit.split(tip);

		<Pallet<T>>::deposit_txfee(inclusion_fee);

		Ok(Some(tip_credit))
	}

	fn can_withdraw_fee(
		who: &T::AccountId,
		_call: &T::RuntimeCall,
		_dispatch_info: &DispatchInfoOf<T::RuntimeCall>,
		fee_with_tip: Self::Balance,
		_tip: Self::Balance,
	) -> Result<(), TransactionValidityError> {
		if fee_with_tip.is_zero() {
			return Ok(())
		}

		match F::can_withdraw(who, fee_with_tip) {
			WithdrawConsequence::Success => Ok(()),
			_ => Err(InvalidTransaction::Payment.into()),
		}
	}

	fn correct_and_deposit_fee(
		who: &<T>::AccountId,
		_dispatch_info: &DispatchInfoOf<<T>::RuntimeCall>,
		_post_info: &PostDispatchInfoOf<<T>::RuntimeCall>,
		corrected_fee_with_tip: Self::Balance,
		tip: Self::Balance,
		tip_credit: Self::LiquidityInfo,
	) -> Result<(), TransactionValidityError> {
		let corrected_fee = corrected_fee_with_tip.saturating_sub(tip);

		let remaining_credit = <TxPaymentCredit<T>>::take()
			.map(|stored_credit| stored_credit.into_inner())
			.unwrap_or_default();

		// If pallets take away too much it makes the transaction invalid. They need to make
		// sure that this does not happen. We do not invalide the transaction because we already
		// executed it and we rather collect too little fees than none at all.
		if remaining_credit.peek() < corrected_fee {
			log::error!(target: LOG_TARGET, "Not enough balance on hold to pay tx fees. This is a bug.");
		}

		// skip refund if account was killed by the tx
		let fee_credit = if frame_system::Pallet::<T>::account_exists(who) {
			let (mut fee_credit, refund_credit) = remaining_credit.split(corrected_fee);
			// resolve might fail if refund is below the ed and account
			// is kept alive by other providers
			if !refund_credit.peek().is_zero() {
				if let Err(not_refunded) = F::resolve(who, refund_credit) {
					fee_credit.subsume(not_refunded);
				}
			}
			fee_credit
		} else {
			remaining_credit
		};

		OU::on_unbalanceds(Some(fee_credit).into_iter().chain(tip_credit));

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

impl<T, F, OU> TxCreditHold<T> for FungibleAdapter<F, OU>
where
	T: Config,
	F: Balanced<T::AccountId> + 'static,
{
	type Credit = NoDrop<Credit<<T as frame_system::Config>::AccountId, F>>;
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
		fee_with_tip: Self::Balance,
		tip: Self::Balance,
	) -> Result<Self::LiquidityInfo, TransactionValidityError> {
		if fee_with_tip.is_zero() {
			return Ok(None)
		}

		let withdraw_reason = if tip.is_zero() {
			WithdrawReasons::TRANSACTION_PAYMENT
		} else {
			WithdrawReasons::TRANSACTION_PAYMENT | WithdrawReasons::TIP
		};

		match C::withdraw(who, fee_with_tip, withdraw_reason, ExistenceRequirement::KeepAlive) {
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
		fee_with_tip: Self::Balance,
		tip: Self::Balance,
	) -> Result<(), TransactionValidityError> {
		if fee_with_tip.is_zero() {
			return Ok(())
		}

		let withdraw_reason = if tip.is_zero() {
			WithdrawReasons::TRANSACTION_PAYMENT
		} else {
			WithdrawReasons::TRANSACTION_PAYMENT | WithdrawReasons::TIP
		};

		let new_balance = C::free_balance(who)
			.checked_sub(&fee_with_tip)
			.ok_or(InvalidTransaction::Payment)?;
		C::ensure_can_withdraw(who, fee_with_tip, withdraw_reason, new_balance)
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
		corrected_fee_with_tip: Self::Balance,
		tip: Self::Balance,
		already_withdrawn: Self::LiquidityInfo,
	) -> Result<(), TransactionValidityError> {
		if let Some(paid) = already_withdrawn {
			// Calculate how much refund we should return
			let refund_amount = paid.peek().saturating_sub(corrected_fee_with_tip);
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

#[allow(deprecated)]
impl<T: Config, C, OU> TxCreditHold<T> for CurrencyAdapter<C, OU> {
	type Credit = ();
}
