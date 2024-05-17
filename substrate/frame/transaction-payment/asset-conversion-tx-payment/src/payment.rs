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

use frame_support::{
	ensure,
	traits::{
		fungible::Inspect,
		fungibles,
		fungibles::Inspect as FungiblesInspect,
		tokens::{Balance, Fortitude, Precision, Preservation},
		Defensive, SameOrOther,
	},
	unsigned::TransactionValidityError,
};
use pallet_asset_conversion::{Pallet as AssetConversion, Swap, SwapCredit};
use sp_runtime::{
	traits::{DispatchInfoOf, Get, PostDispatchInfoOf, Zero},
	transaction_validity::InvalidTransaction,
	Saturating,
};
use sp_std::marker::PhantomData;

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
	) -> Result<
		(LiquidityInfoOf<T>, Self::LiquidityInfo, AssetBalanceOf<T>),
		TransactionValidityError,
	>;

	/// Refund any overpaid fees and deposit the corrected amount.
	/// The actual fee gets calculated once the transaction is executed.
	///
	/// Note: The `fee` already includes the `tip`.
	///
	/// Returns the fee and tip in the asset used for payment as (fee, tip).
	fn correct_and_deposit_fee(
		who: &T::AccountId,
		dispatch_info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &PostDispatchInfoOf<T::RuntimeCall>,
		corrected_fee: Self::Balance,
		tip: Self::Balance,
		fee_paid: LiquidityInfoOf<T>,
		received_exchanged: Self::LiquidityInfo,
		asset_id: Self::AssetId,
		initial_asset_consumed: AssetBalanceOf<T>,
	) -> Result<AssetBalanceOf<T>, TransactionValidityError>;
}

/// Implements the asset transaction for a balance to asset converter (implementing [`Swap`]).
///
/// The converter is given the complete fee in terms of the asset used for the transaction.
pub struct AssetConversionAdapter<C, CON, N>(PhantomData<(C, CON, N)>);

/// Default implementation for a runtime instantiating this pallet, an asset to native swapper.
impl<T, C, CON, N> OnChargeAssetTransaction<T> for AssetConversionAdapter<C, CON, N>
where
	N: Get<CON::AssetKind>,
	T: Config,
	C: Inspect<<T as frame_system::Config>::AccountId>,
	CON: Swap<T::AccountId, Balance = BalanceOf<T>, AssetKind = T::AssetKind>,
	BalanceOf<T>: Into<AssetBalanceOf<T>>,
	T::AssetKind: From<AssetIdOf<T>>,
	BalanceOf<T>: IsType<<C as Inspect<<T as frame_system::Config>::AccountId>>::Balance>,
{
	type Balance = BalanceOf<T>;
	type AssetId = AssetIdOf<T>;
	type LiquidityInfo = BalanceOf<T>;

	/// Swap & withdraw the predicted fee from the transaction origin.
	///
	/// Note: The `fee` already includes the `tip`.
	///
	/// Returns the total amount in native currency received by exchanging the `asset_id` and the
	/// amount in native currency used to pay the fee.
	fn withdraw_fee(
		who: &T::AccountId,
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		asset_id: Self::AssetId,
		fee: BalanceOf<T>,
		tip: BalanceOf<T>,
	) -> Result<
		(LiquidityInfoOf<T>, Self::LiquidityInfo, AssetBalanceOf<T>),
		TransactionValidityError,
	> {
		// convert the asset into native currency
		let ed = C::minimum_balance();
		let native_asset_required =
			if C::balance(&who) >= ed.saturating_add(fee.into()) { fee } else { fee + ed.into() };

		let asset_consumed = CON::swap_tokens_for_exact_tokens(
			who.clone(),
			vec![asset_id.into(), N::get()],
			native_asset_required,
			None,
			who.clone(),
			true,
		)
		.map_err(|_| TransactionValidityError::from(InvalidTransaction::Payment))?;

		ensure!(asset_consumed > Zero::zero(), InvalidTransaction::Payment);

		// charge the fee in native currency
		<T::OnChargeTransaction>::withdraw_fee(who, call, info, fee, tip)
			.map(|r| (r, native_asset_required, asset_consumed.into()))
	}

	/// Correct the fee and swap the refund back to asset.
	///
	/// Note: The `corrected_fee` already includes the `tip`.
	/// Note: Is the ED wasn't needed, the `received_exchanged` will be equal to `fee_paid`, or
	/// `fee_paid + ed` otherwise.
	fn correct_and_deposit_fee(
		who: &T::AccountId,
		dispatch_info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &PostDispatchInfoOf<T::RuntimeCall>,
		corrected_fee: BalanceOf<T>,
		tip: BalanceOf<T>,
		fee_paid: LiquidityInfoOf<T>,
		received_exchanged: Self::LiquidityInfo,
		asset_id: Self::AssetId,
		initial_asset_consumed: AssetBalanceOf<T>,
	) -> Result<AssetBalanceOf<T>, TransactionValidityError> {
		// Refund the native asset to the account that paid the fees (`who`).
		// The `who` account will receive the "fee_paid - corrected_fee" refund.
		<T::OnChargeTransaction>::correct_and_deposit_fee(
			who,
			dispatch_info,
			post_info,
			corrected_fee,
			tip,
			fee_paid,
		)?;

		// calculate the refund in native asset, to swap back to the desired `asset_id`
		let swap_back = received_exchanged.saturating_sub(corrected_fee);
		let mut asset_refund = Zero::zero();
		if !swap_back.is_zero() {
			// If this fails, the account might have dropped below the existential balance or there
			// is not enough liquidity left in the pool. In that case we don't throw an error and
			// the account will keep the native currency.
			match CON::swap_exact_tokens_for_tokens(
				who.clone(), // we already deposited the native to `who`
				vec![
					N::get(),        // we provide the native
					asset_id.into(), // we want asset_id back
				],
				swap_back,   // amount of the native asset to convert to `asset_id`
				None,        // no minimum amount back
				who.clone(), // we will refund to `who`
				false,       // no need to keep alive
			)
			.ok()
			{
				Some(acquired) => {
					asset_refund = acquired
						.try_into()
						.map_err(|_| TransactionValidityError::from(InvalidTransaction::Payment))?;
				},
				None => {
					Pallet::<T>::deposit_event(Event::<T>::AssetRefundFailed {
						native_amount_kept: swap_back,
					});
				},
			}
		}

		let actual_paid = initial_asset_consumed.saturating_sub(asset_refund);
		Ok(actual_paid)
	}
}

use super::pallet as pallet_asset_conversion_tx_payment;

/// Implements [`OnChargeAssetTransaction`] for [`pallet_asset_conversion_tx_payment`], where
/// the asset class used to pay the fee is defined with the `A` type parameter (eg. DOT
/// location) and accessed via the type implementing the [`frame_support::traits::fungibles`]
/// trait.
pub struct SwapCreditAdapter<A, S>(PhantomData<(A, S)>);
impl<A, S, T> OnChargeAssetTransaction<T> for SwapCreditAdapter<A, S>
where
	A: Get<S::AssetKind>,
	S: SwapCredit<
		T::AccountId,
		Balance = T::Balance,
		AssetKind = T::AssetKind,
		Credit = fungibles::Credit<T::AccountId, T::Assets>,
	>,

	T: pallet_asset_conversion_tx_payment::Config,
	T::Fungibles: fungibles::Inspect<T::AccountId, Balance = T::Balance, AssetId = T::AssetKind>,
	T::OnChargeTransaction:
		OnChargeTransaction<T, Balance = T::Balance, LiquidityInfo = Option<S::Credit>>,
{
	type AssetId = T::AssetKind;
	type Balance = T::Balance;
	type LiquidityInfo = T::Balance;

	fn withdraw_fee(
		who: &<T>::AccountId,
		_call: &<T>::RuntimeCall,
		_dispatch_info: &DispatchInfoOf<<T>::RuntimeCall>,
		asset_id: Self::AssetId,
		fee: Self::Balance,
		_tip: Self::Balance,
	) -> Result<(LiquidityInfoOf<T>, Self::LiquidityInfo, T::Balance), TransactionValidityError> {
		let asset_fee = AssetConversion::<T>::quote_price_tokens_for_exact_tokens(
			asset_id.clone(),
			A::get(),
			fee,
			true,
		)
		.ok_or(InvalidTransaction::Payment)?;

		let asset_fee_credit = T::Assets::withdraw(
			asset_id.clone(),
			who,
			asset_fee,
			Precision::Exact,
			Preservation::Preserve,
			Fortitude::Polite,
		)
		.map_err(|_| TransactionValidityError::from(InvalidTransaction::Payment))?;

		let (fee_credit, change) = match S::swap_tokens_for_exact_tokens(
			vec![asset_id, A::get()],
			asset_fee_credit,
			fee,
		) {
			Ok((fee_credit, change)) => (fee_credit, change),
			Err((credit_in, _)) => {
				let _ = T::Assets::resolve(who, credit_in).defensive();
				return Err(InvalidTransaction::Payment.into())
			},
		};

		ensure!(change.peek().is_zero(), InvalidTransaction::Payment);

		Ok((Some(fee_credit), fee, asset_fee))
	}
	fn correct_and_deposit_fee(
		who: &<T>::AccountId,
		dispatch_info: &DispatchInfoOf<<T>::RuntimeCall>,
		post_info: &PostDispatchInfoOf<<T>::RuntimeCall>,
		corrected_fee: Self::Balance,
		tip: Self::Balance,
		fee_paid: LiquidityInfoOf<T>,
		_received_exchanged: Self::LiquidityInfo,
		asset_id: Self::AssetId,
		initial_asset_consumed: T::Balance,
	) -> Result<T::Balance, TransactionValidityError> {
		let Some(fee_paid) = fee_paid else {
			return Ok(Zero::zero());
		};
		// Try to refund if the fee paid is more than the corrected fee and the account was not
		// removed by the dispatched function.
		let (fee, fee_in_asset) = if fee_paid.peek() > corrected_fee &&
			!T::Assets::total_balance(asset_id.clone(), who).is_zero()
		{
			let refund_amount = fee_paid.peek().saturating_sub(corrected_fee);
			// Check if the refund amount can be swapped back into the asset used by `who` for
			// fee payment.
			let refund_asset_amount = AssetConversion::<T>::quote_price_exact_tokens_for_tokens(
				A::get(),
				asset_id.clone(),
				refund_amount,
				true,
			)
			// No refund given if it cannot be swapped back.
			.unwrap_or(Zero::zero());

			// Deposit the refund before the swap to ensure it can be processed.
			let debt = match T::Assets::deposit(
				asset_id.clone(),
				&who,
				refund_asset_amount,
				Precision::BestEffort,
			) {
				Ok(debt) => debt,
				// No refund given since it cannot be deposited.
				Err(_) => fungibles::Debt::<T::AccountId, T::Assets>::zero(asset_id.clone()),
			};

			if debt.peek().is_zero() {
				// No refund given.
				(fee_paid, initial_asset_consumed)
			} else {
				let (refund, fee_paid) = fee_paid.split(refund_amount);
				match S::swap_exact_tokens_for_tokens(
					vec![A::get(), asset_id],
					refund,
					Some(refund_asset_amount),
				) {
					Ok(refund_asset) => {
						match refund_asset.offset(debt) {
							Ok(SameOrOther::None) => {},
							// This arm should never be reached, as the  amount of `debt` is
							// expected to be exactly equal to the amount of `refund_asset`
							// credit.
							_ => return Err(InvalidTransaction::Payment.into()),
						};
						(
							fee_paid,
							initial_asset_consumed.saturating_sub(refund_asset_amount.into()),
						)
					},
					// The error should not occur since swap was quoted before.
					Err((refund, _)) => {
						match T::Assets::settle(who, debt, Preservation::Expendable) {
							Ok(dust) => ensure!(dust.peek().is_zero(), InvalidTransaction::Payment),
							// The error should not occur as the `debt` was just withdrawn
							// above.
							Err(_) => return Err(InvalidTransaction::Payment.into()),
						};
						let fee_paid = fee_paid.merge(refund).map_err(|_| {
							// The error should never occur since `fee_paid` and `refund` are
							// credits of the same asset.
							TransactionValidityError::from(InvalidTransaction::Payment)
						})?;
						(fee_paid, initial_asset_consumed)
					},
				}
			}
		} else {
			(fee_paid, initial_asset_consumed)
		};

		// Refund is already processed.
		let corrected_fee = fee.peek();
		// Deposit fee.
		T::OnChargeTransaction::correct_and_deposit_fee(
			who,
			dispatch_info,
			post_info,
			corrected_fee,
			tip,
			Some(fee),
		)
		.map(|_| fee_in_asset)
	}
}
