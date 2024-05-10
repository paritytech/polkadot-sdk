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

use frame_support::{
	ensure,
	traits::{
		fungible::{self, Inspect},
		fungibles::Inspect as FungibleInspect,
		tokens::{Balance, Imbalance, Preservation},
	},
};
use pallet_asset_conversion::Swap;
use sp_runtime::{traits::Get, Saturating};
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
	C: Inspect<<T as frame_system::Config>::AccountId>
		+ fungible::Balanced<<T as frame_system::Config>::AccountId>,
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

		let debt = if ed > C::balance(&who) {
			let (debt, credit) = C::pair(ed)
				.map_err(|_| TransactionValidityError::from(InvalidTransaction::Payment))?;
			let _ = C::resolve(who, credit)
				.map_err(|_| TransactionValidityError::from(InvalidTransaction::Payment))?;
			debt
		} else {
			fungible::Debt::<T::AccountId, C>::zero()
		};

		let asset_consumed = match CON::swap_tokens_for_exact_tokens(
			who.clone(),
			vec![asset_id.into(), N::get()],
			fee,
			None,
			who.clone(),
			true,
		) {
			Ok(consumed) => consumed,
			Err(_) => {
				let _ = C::settle(who, debt, Preservation::Expendable);
				return Err(InvalidTransaction::Payment.into());
			},
		};

		if asset_consumed.is_zero() {
			let _ = C::settle(who, debt, Preservation::Expendable);
			return Err(InvalidTransaction::Payment.into());
		}

		// charge the fee in native currency
		match <T::OnChargeTransaction>::withdraw_fee(who, call, info, fee, tip) {
			Ok(fee_credit) => {
				let credit = C::settle(who, debt, Preservation::Expendable)
					.map_err(|_| TransactionValidityError::from(InvalidTransaction::Payment))?;
				ensure!(credit.peek().is_zero(), InvalidTransaction::Payment);

				Ok((fee_credit, fee, asset_consumed.into()))
			},
			Err(e) => {
				let _ = C::settle(who, debt, Preservation::Expendable);
				Err(e)
			},
		}
	}

	/// Correct the fee and swap the refund back to asset.
	///
	/// Note: The `corrected_fee` already includes the `tip`.
	/// Note: The `received_exchanged` will be equal to `fee_paid`.
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
		let ed = C::minimum_balance();
		let ed_asset = T::Fungibles::minimum_balance(asset_id.clone());
		let final_fee = if C::balance(&who) >= ed &&
			T::Fungibles::balance(asset_id.clone(), &who) >= ed_asset
		{
			corrected_fee
		} else {
			// otherwise no refund.
			received_exchanged
		};

		// Refund the native asset to the account that paid the fees (`who`).
		// The `who` account will receive the "fee_paid - corrected_fee" refund.
		<T::OnChargeTransaction>::correct_and_deposit_fee(
			who,
			dispatch_info,
			post_info,
			final_fee,
			tip,
			fee_paid,
		)?;

		// calculate the refund in native asset, to swap back to the desired `asset_id`
		let swap_back = received_exchanged.saturating_sub(final_fee);
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
