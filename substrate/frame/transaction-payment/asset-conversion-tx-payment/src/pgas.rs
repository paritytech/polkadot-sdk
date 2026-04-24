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

//! # PGAS fee-payment adapter
//!
//! [`PgasOnChargeAssetTransaction`] is an [`OnChargeAssetTransaction`] adapter. When the fee asset
//! matches [`PgasId`](frame_support::traits::Get), it withdraws the fee directly from PGAS (no
//! swap) and burns the consumed portion by dropping the credit; any unused portion is refunded.
//! For any other asset it delegates to `Inner`.
//!
//! Wired into a runtime by setting
//! `pallet_asset_conversion_tx_payment::Config::OnChargeAssetTransaction` to
//! `PgasOnChargeAssetTransaction<PgasId, Fungibles, SwapAssetAdapter<...>>`. Users opt into PGAS
//! payment by specifying `Some(PGAS)` as the fee asset on `ChargeAssetTxPayment`.

use super::{BalanceOf, Config};
use core::marker::PhantomData;
use frame_support::{
	traits::{
		tokens::{
			fungibles::{self, Credit},
			Fortitude, Precision, Preservation, WithdrawConsequence,
		},
		Get,
	},
	unsigned::TransactionValidityError,
};
use sp_runtime::{
	traits::{DispatchInfoOf, PostDispatchInfoOf},
	transaction_validity::InvalidTransaction,
};

use crate::OnChargeAssetTransaction;

/// Liquidity info produced by [`PgasOnChargeAssetTransaction::withdraw_fee`]: either PGAS (held as
/// a fungibles credit ready to be split and burned in `correct_and_deposit_fee`) or whatever the
/// inner adapter produces for non-PGAS assets.
pub enum PgasLiquidityInfo<PgasInfo, InnerInfo> {
	/// Fee was withdrawn as PGAS.
	Pgas(PgasInfo),
	/// Fee handling was delegated to the inner adapter.
	Other(InnerInfo),
}

/// [`OnChargeAssetTransaction`] adapter that intercepts PGAS fee payments (withdraw + burn) and
/// delegates to `Inner` for any other asset.
pub struct PgasOnChargeAssetTransaction<PgasId, F, Inner>(PhantomData<(PgasId, F, Inner)>);

impl<T, PgasId, F, Inner> OnChargeAssetTransaction<T>
	for PgasOnChargeAssetTransaction<PgasId, F, Inner>
where
	T: Config,
	PgasId: Get<T::AssetId>,
	F: fungibles::Balanced<T::AccountId, Balance = BalanceOf<T>, AssetId = T::AssetId>,
	Inner: OnChargeAssetTransaction<T, Balance = BalanceOf<T>, AssetId = T::AssetId>,
{
	type AssetId = T::AssetId;
	type Balance = BalanceOf<T>;
	type LiquidityInfo = PgasLiquidityInfo<Credit<T::AccountId, F>, Inner::LiquidityInfo>;

	fn withdraw_fee(
		who: &T::AccountId,
		call: &T::RuntimeCall,
		dispatch_info: &DispatchInfoOf<T::RuntimeCall>,
		asset_id: Self::AssetId,
		fee: Self::Balance,
		tip: Self::Balance,
	) -> Result<Self::LiquidityInfo, TransactionValidityError> {
		if asset_id == PgasId::get() {
			// PGAS is pegged 1:1 with the native fee, so no swap is needed. `Expendable` lets the
			// caller fully drain their PGAS on the final tx (PGAS is a sufficient asset, so reaping
			// on payment is safe).
			let credit = F::withdraw(
				asset_id,
				who,
				fee,
				Precision::Exact,
				Preservation::Expendable,
				Fortitude::Polite,
			)
			.map_err(|_| InvalidTransaction::Payment)?;
			Ok(PgasLiquidityInfo::Pgas(credit))
		} else {
			Inner::withdraw_fee(who, call, dispatch_info, asset_id, fee, tip)
				.map(PgasLiquidityInfo::Other)
		}
	}

	fn can_withdraw_fee(
		who: &T::AccountId,
		asset_id: Self::AssetId,
		fee: Self::Balance,
	) -> Result<(), TransactionValidityError> {
		if asset_id == PgasId::get() {
			match F::can_withdraw(asset_id, who, fee) {
				WithdrawConsequence::Success | WithdrawConsequence::ReducedToZero(_) => Ok(()),
				_ => Err(InvalidTransaction::Payment.into()),
			}
		} else {
			Inner::can_withdraw_fee(who, asset_id, fee)
		}
	}

	fn correct_and_deposit_fee(
		who: &T::AccountId,
		dispatch_info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &PostDispatchInfoOf<T::RuntimeCall>,
		corrected_fee: Self::Balance,
		tip: Self::Balance,
		asset_id: Self::AssetId,
		already_withdrawn: Self::LiquidityInfo,
	) -> Result<BalanceOf<T>, TransactionValidityError> {
		match already_withdrawn {
			PgasLiquidityInfo::Pgas(credit) => {
				let (fee_credit, refund_credit) = credit.split(corrected_fee);
				// If resolve fails the refund credit is dropped, which burns it. That matches the
				// PGAS "burn on fee" semantics so we don't need to recover from it.
				let _ = F::resolve(who, refund_credit);
				// Drop burns the fee via `pallet_assets::OnDropCredit`.
				drop(fee_credit);
				Ok(corrected_fee)
			},
			PgasLiquidityInfo::Other(inner_info) => Inner::correct_and_deposit_fee(
				who,
				dispatch_info,
				post_info,
				corrected_fee,
				tip,
				asset_id,
				inner_info,
			),
		}
	}
}
