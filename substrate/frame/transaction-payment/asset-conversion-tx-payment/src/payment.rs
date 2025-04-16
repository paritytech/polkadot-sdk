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
		fungibles,
		tokens::{Balance, Fortitude, Precision, Preservation, WithdrawConsequence},
		Defensive, OnUnbalanced, SameOrOther,
	},
	unsigned::TransactionValidityError,
};
use pallet_asset_conversion::{QuotePrice, SwapCredit};
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
	F: fungibles::Balanced<T::AccountId, Balance = BalanceOf<T>, AssetId = T::AssetId>,
	S: SwapCredit<
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
		_tip: Self::Balance,
	) -> Result<Self::LiquidityInfo, TransactionValidityError> {
		if asset_id == A::get() {
			// The `asset_id` is the target asset, we do not need to swap.
			let fee_credit = F::withdraw(
				asset_id.clone(),
				who,
				fee,
				Precision::Exact,
				Preservation::Preserve,
				Fortitude::Polite,
			)
			.map_err(|_| InvalidTransaction::Payment)?;

			return Ok((fee_credit, fee));
		}

		// Quote the amount of the `asset_id` needed to pay the fee in the asset `A`.
		let asset_fee =
			S::quote_price_tokens_for_exact_tokens(asset_id.clone(), A::get(), fee, true)
				.ok_or(InvalidTransaction::Payment)?;

		// Withdraw the `asset_id` credit for the swap.
		let asset_fee_credit = F::withdraw(
			asset_id.clone(),
			who,
			asset_fee,
			Precision::Exact,
			Preservation::Preserve,
			Fortitude::Polite,
		)
		.map_err(|_| InvalidTransaction::Payment)?;

		let (fee_credit, change) = match S::swap_tokens_for_exact_tokens(
			vec![asset_id, A::get()],
			asset_fee_credit,
			fee,
		) {
			Ok((fee_credit, change)) => (fee_credit, change),
			Err((credit_in, _)) => {
				defensive!("Fee swap should pass for the quoted amount");
				let _ = F::resolve(who, credit_in).defensive_proof("Should resolve the credit");
				return Err(InvalidTransaction::Payment.into())
			},
		};

		// Since the exact price for `fee` has been quoted, the change should be zero.
		ensure!(change.peek().is_zero(), InvalidTransaction::Payment);

		Ok((fee_credit, asset_fee))
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
			match F::can_withdraw(asset_id.clone(), who, fee) {
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

		let asset_fee =
			S::quote_price_tokens_for_exact_tokens(asset_id.clone(), A::get(), fee, true)
				.ok_or(InvalidTransaction::Payment)?;

		// Ensure we can withdraw enough `asset_id` for the swap.
		match F::can_withdraw(asset_id.clone(), who, asset_fee) {
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
		already_withdrawn: Self::LiquidityInfo,
	) -> Result<BalanceOf<T>, TransactionValidityError> {
		let (fee_paid, initial_asset_consumed) = already_withdrawn;
		let refund_amount = fee_paid.peek().saturating_sub(corrected_fee);
		let (fee_in_asset, adjusted_paid) = if refund_amount.is_zero() ||
			F::total_balance(asset_id.clone(), who).is_zero()
		{
			// Nothing to refund or the account was removed be the dispatched function.
			(initial_asset_consumed, fee_paid)
		} else if asset_id == A::get() {
			// The `asset_id` is the target asset, we do not need to swap.
			let (refund, fee_paid) = fee_paid.split(refund_amount);
			if let Err(refund) = F::resolve(who, refund) {
				let fee_paid = fee_paid.merge(refund).map_err(|_| {
					defensive!("`fee_paid` and `refund` are credits of the same asset.");
					InvalidTransaction::Payment
				})?;
				(initial_asset_consumed, fee_paid)
			} else {
				(fee_paid.peek().saturating_sub(refund_amount), fee_paid)
			}
		} else {
			// Check if the refund amount can be swapped back into the asset used by `who` for fee
			// payment.
			let refund_asset_amount = S::quote_price_exact_tokens_for_tokens(
				A::get(),
				asset_id.clone(),
				refund_amount,
				true,
			)
			// No refund given if it cannot be swapped back.
			.unwrap_or(Zero::zero());

			let debt = if refund_asset_amount.is_zero() {
				fungibles::Debt::<T::AccountId, F>::zero(asset_id.clone())
			} else {
				// Deposit the refund before the swap to ensure it can be processed.
				match F::deposit(asset_id.clone(), &who, refund_asset_amount, Precision::BestEffort)
				{
					Ok(debt) => debt,
					// No refund given since it cannot be deposited.
					Err(_) => fungibles::Debt::<T::AccountId, F>::zero(asset_id.clone()),
				}
			};

			if debt.peek().is_zero() {
				// No refund given.
				(initial_asset_consumed, fee_paid)
			} else {
				let (refund, adjusted_paid) = fee_paid.split(refund_amount);
				match S::swap_exact_tokens_for_tokens(
					vec![A::get(), asset_id],
					refund,
					Some(refund_asset_amount),
				) {
					Ok(refund_asset) => {
						match refund_asset.offset(debt) {
							Ok(SameOrOther::None) => {},
							// This arm should never be reached, as the  amount of `debt` is
							// expected to be exactly equal to the amount of `refund_asset` credit.
							_ => {
								defensive!("Debt should be equal to the refund credit");
								return Err(InvalidTransaction::Payment.into())
							},
						};
						(
							initial_asset_consumed.saturating_sub(refund_asset_amount.into()),
							adjusted_paid,
						)
					},
					// The error should not occur since swap was quoted before.
					Err((refund, _)) => {
						defensive!("Refund swap should pass for the quoted amount");
						match F::settle(who, debt, Preservation::Expendable) {
							Ok(dust) => ensure!(dust.peek().is_zero(), InvalidTransaction::Payment),
							// The error should not occur as the `debt` was just withdrawn above.
							Err(_) => {
								defensive!("Should settle the debt");
								return Err(InvalidTransaction::Payment.into())
							},
						};
						let adjusted_paid = adjusted_paid.merge(refund).map_err(|_| {
							// The error should never occur since `adjusted_paid` and `refund` are
							// credits of the same asset.
							InvalidTransaction::Payment
						})?;
						(initial_asset_consumed, adjusted_paid)
					},
				}
			}
		};

		// Handle the imbalance (fee and tip separately).
		let (tip, fee) = adjusted_paid.split(tip);
		OU::on_unbalanceds(Some(fee).into_iter().chain(Some(tip)));
		Ok(fee_in_asset)
	}
}
