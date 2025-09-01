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

///! Traits and default implementation for paying transaction fees in assets.
use super::*;
use crate::Config;

use alloc::vec;
use core::marker::PhantomData;
use frame_support::{
	defensive, ensure,
	traits::{
		fungible::{self, MutateHold},
		fungibles,
		tokens::{Balance, Fortitude, Precision, Preservation, WithdrawConsequence},
		OnUnbalanced,
	},
	unsigned::TransactionValidityError,
};
use frame_system::Pallet as System;
use pallet_asset_conversion::{QuotePrice, Swap, SwapCredit};
use pallet_transaction_payment::HoldReason;
use sp_runtime::{
	traits::{DispatchInfoOf, Get, PostDispatchInfoOf, Zero},
	transaction_validity::InvalidTransaction,
	Saturating,
};

/// Handle withdrawing, refunding and depositing of transaction fees.
pub trait OnChargeAssetTransaction<T: Config> {
	/// The underlying integer type in which fees are calculated.
	type Balance: Balance;
	/// The type used to identify the assets used for transaction payment.
	type AssetId: AssetId;
	/// The type used to store the intermediate values between pre- and post-dispatch.
	type LiquidityInfo;

	/// Secure the payment of the transaction fees before the transaction is executed.
	///
	/// Note: The `fee` already includes the `tip`.
	fn withdraw_fee(
		who: &T::AccountId,
		call: &T::RuntimeCall,
		dispatch_info: &DispatchInfoOf<T::RuntimeCall>,
		asset_id: Self::AssetId,
		fee: Self::Balance,
		tip: Self::Balance,
	) -> Result<Self::LiquidityInfo, TransactionValidityError>;

	/// Ensure payment of the transaction fees can be withdrawn.
	///
	/// Note: The `fee` already includes the tip.
	fn can_withdraw_fee(
		who: &T::AccountId,
		asset_id: Self::AssetId,
		fee: Self::Balance,
	) -> Result<(), TransactionValidityError>;

	/// Refund any overpaid fees and deposit the corrected amount.
	/// The actual fee gets calculated once the transaction is executed.
	///
	/// Note: The `fee` already includes the `tip`.
	///
	/// Returns the amount of `asset_id` that was used for the payment.
	fn correct_and_deposit_fee(
		who: &T::AccountId,
		dispatch_info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &PostDispatchInfoOf<T::RuntimeCall>,
		corrected_fee: Self::Balance,
		tip: Self::Balance,
		asset_id: Self::AssetId,
		already_withdraw: Self::LiquidityInfo,
	) -> Result<BalanceOf<T>, TransactionValidityError>;
}

/// Means to withdraw, correct and deposit fees in the asset accepted by the system.
///
/// The type uses the [`SwapCredit`] implementation to swap the asset used by a user for the fee
/// payment for the asset accepted as a fee payment be the system.
///
/// Parameters:
/// - `A`: The asset identifier that system accepts as a fee payment (eg. native asset).
/// - `F`: The fungibles registry that can handle assets provided by user and the `A` asset.
/// - `S`: The swap implementation that can swap assets provided by user for the `A` asset.
/// - OU: The handler for withdrawn `fee` and `tip`, passed in the respective order to
///   [OnUnbalanced::on_unbalanceds].
/// - `T`: The pallet's configuration.
pub struct SwapAssetAdapter<A, F, S, OU>(PhantomData<(A, F, S, OU)>);

impl<A, F, S, OU, T> OnChargeAssetTransaction<T> for SwapAssetAdapter<A, F, S, OU>
where
	A: Get<T::AssetId>,
	F: fungibles::Balanced<T::AccountId, Balance = BalanceOf<T>, AssetId = T::AssetId>
		+ MutateHold<T::AccountId, Balance = BalanceOf<T>, Reason = T::RuntimeHoldReason>,
	S: Swap<T::AccountId, Balance = BalanceOf<T>, AssetKind = T::AssetId>
		+ SwapCredit<
			T::AccountId,
			Balance = BalanceOf<T>,
			AssetKind = T::AssetId,
			Credit = fungibles::Credit<T::AccountId, F>,
		> + QuotePrice<Balance = BalanceOf<T>, AssetKind = T::AssetId>,
	OU: OnUnbalanced<fungibles::Credit<T::AccountId, F>>,
	T: Config,
{
	type AssetId = T::AssetId;
	type Balance = BalanceOf<T>;
	type LiquidityInfo = (fungibles::Credit<T::AccountId, F>, BalanceOf<T>);

	fn withdraw_fee(
		who: &T::AccountId,
		_call: &T::RuntimeCall,
		_dispatch_info: &DispatchInfoOf<<T>::RuntimeCall>,
		asset_id: Self::AssetId,
		fee: Self::Balance,
		tip: Self::Balance,
	) -> Result<Self::LiquidityInfo, TransactionValidityError> {
		// We need to have the account stay alive even when all the free balance is sent away.
		// Otherwise the held balance is burned before we have a chance to recover it.
		<System<T>>::inc_providers(who);

		// if we are not paying in native balance we need to convert first
		let (asset_fee, fee) = if asset_id != A::get() {
			// target account needs to be brought to live
			let native_ed = <F as fungible::Inspect<T::AccountId>>::minimum_balance();
			let fee = if <F as fungible::Inspect<T::AccountId>>::balance(who).is_zero() {
				fee.saturating_add(native_ed)
			} else {
				fee
			};

			// Quote the amount of the `asset_id` needed to pay the fee in the asset `A`.
			let asset_fee =
				S::quote_price_tokens_for_exact_tokens(asset_id.clone(), A::get(), fee, true)
					.ok_or(InvalidTransaction::Payment)?;

			match <S as Swap<_>>::swap_exact_tokens_for_tokens(
				who.clone(),
				vec![asset_id.clone(), A::get()],
				asset_fee,
				Some(fee),
				who.clone(),
				true,
			) {
				Ok(fee_used) => ensure!(fee_used == fee, InvalidTransaction::Payment),
				Err(_) => {
					defensive!("Fee swap should pass for the quoted amount");
					return Err(InvalidTransaction::Payment.into())
				},
			};

			(asset_fee, fee)
		} else {
			(fee, fee)
		};

		// Pallets have no way of knowing the amount of tip. Hence they have no way
		// of making sure that they don't consume the tip. This is why we exclude it
		// from the hold.
		let tip_credit = F::withdraw(
			A::get(),
			who,
			tip,
			Precision::Exact,
			Preservation::Expendable,
			Fortitude::Polite,
		)
		.map_err(|_| TransactionValidityError::Invalid(InvalidTransaction::Payment))?;

		// Put on hold so that pallets can withdraw from it in order to pay for deposits.
		F::hold(&HoldReason::Payment.into(), who, fee.saturating_sub(tip))
			.map_err(|_| InvalidTransaction::Payment)?;

		Ok((tip_credit, asset_fee))
	}

	/// Dry run of swap & withdraw the predicted fee from the transaction origin.
	///
	/// Note: The `fee` already includes the tip.
	///
	/// Returns an error if the total amount in native currency can't be exchanged for `asset_id`.
	fn can_withdraw_fee(
		who: &T::AccountId,
		asset_id: Self::AssetId,
		fee: BalanceOf<T>,
	) -> Result<(), TransactionValidityError> {
		if asset_id == A::get() {
			// The `asset_id` is the target asset, we do not need to swap.
			match <F as fungibles::Inspect<T::AccountId>>::can_withdraw(asset_id.clone(), who, fee)
			{
				WithdrawConsequence::BalanceLow |
				WithdrawConsequence::UnknownAsset |
				WithdrawConsequence::Underflow |
				WithdrawConsequence::Overflow |
				WithdrawConsequence::Frozen =>
					return Err(TransactionValidityError::from(InvalidTransaction::Payment)),
				WithdrawConsequence::Success |
				WithdrawConsequence::ReducedToZero(_) |
				WithdrawConsequence::WouldDie => return Ok(()),
			}
		}

		// target account needs to be brought to live
		let native_ed = <F as fungible::Inspect<T::AccountId>>::minimum_balance();
		let fee = if <F as fungible::Inspect<T::AccountId>>::balance(who).is_zero() {
			fee.saturating_add(native_ed)
		} else {
			fee
		};

		let asset_fee =
			S::quote_price_tokens_for_exact_tokens(asset_id.clone(), A::get(), fee, true)
				.ok_or(InvalidTransaction::Payment)?;

		// Ensure we can withdraw enough `asset_id` for the swap.
		match <F as fungibles::Inspect<T::AccountId>>::can_withdraw(
			asset_id.clone(),
			who,
			asset_fee,
		) {
			WithdrawConsequence::BalanceLow |
			WithdrawConsequence::UnknownAsset |
			WithdrawConsequence::Underflow |
			WithdrawConsequence::Overflow |
			WithdrawConsequence::Frozen =>
				return Err(TransactionValidityError::from(InvalidTransaction::Payment)),
			WithdrawConsequence::Success |
			WithdrawConsequence::ReducedToZero(_) |
			WithdrawConsequence::WouldDie => {},
		};

		Ok(())
	}

	fn correct_and_deposit_fee(
		who: &T::AccountId,
		_dispatch_info: &DispatchInfoOf<<T>::RuntimeCall>,
		_post_info: &PostDispatchInfoOf<<T>::RuntimeCall>,
		corrected_fee: Self::Balance,
		tip: Self::Balance,
		asset_id: Self::AssetId,
		already_paid: Self::LiquidityInfo,
	) -> Result<BalanceOf<T>, TransactionValidityError> {
		let (tip_credit, asset_fee) = already_paid;
		let corrected_fee = corrected_fee.saturating_sub(tip);
		let account_dead = <System<T>>::reference_count(who) == 1;
		let available_fee = F::balance_on_hold(&HoldReason::Payment.into(), who);

		// If pallets take away too much it makes the transaction invalid. They need to make
		// sure that this does not happen.
		if available_fee < corrected_fee {
			defensive!("Not enough balance on hold to pay tx fees. This is a bug.");
			Err(TransactionValidityError::Invalid(InvalidTransaction::Payment))?;
		}

		F::release(&HoldReason::Payment.into(), who, available_fee, Precision::Exact)
			.map_err(|_| TransactionValidityError::Invalid(InvalidTransaction::Payment))?;

		let (refund_credit, payable_credit) = {
			// need to withdraw everything in one go to prevent a dusting in case the account
			// was only kept alive by the transaction fee
			let available_credit = F::withdraw(
				A::get(),
				who,
				available_fee,
				Precision::Exact,
				Preservation::Expendable,
				Fortitude::Polite,
			)
			.map_err(|_| TransactionValidityError::Invalid(InvalidTransaction::Payment))?;
			let refund = if account_dead {
				Zero::zero()
			} else {
				available_fee.saturating_sub(corrected_fee)
			};
			available_credit.split(refund)
		};

		// only the refund needs to be swapped back
		let refund_credit = if asset_id != A::get() && !refund_credit.peek().is_zero() {
			<S as SwapCredit<_>>::swap_exact_tokens_for_tokens(
				vec![A::get(), asset_id.clone()],
				refund_credit,
				None,
			)
			.map_err(|_| TransactionValidityError::Invalid(InvalidTransaction::Payment))?
		} else {
			refund_credit
		};

		let asset_refund_amount = refund_credit.peek();
		if !asset_refund_amount.is_zero() {
			F::resolve(who, refund_credit)
				.map_err(|_| TransactionValidityError::Invalid(InvalidTransaction::Payment))?;
		}

		<System<T>>::dec_providers(who).expect("We increased the provider in withdraw_fee. We assume all other providers are balanced. qed");

		OU::on_unbalanceds(Some(payable_credit).into_iter().chain(Some(tip_credit)));
		Ok(asset_fee.saturating_sub(asset_refund_amount).saturating_add(tip))
	}
}
