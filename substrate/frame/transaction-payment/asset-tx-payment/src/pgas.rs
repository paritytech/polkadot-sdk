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
//! matches `PgasId::get()`, it withdraws the fee directly from PGAS via [`Config::Fungibles`] (no
//! swap) and burns the consumed portion by dropping the credit; any unused portion is refunded.
//! For any other asset it delegates to `Inner`.
//!
//! Wired into a runtime by setting
//! `pallet_asset_tx_payment::Config::OnChargeAssetTransaction` to
//! `PgasOnChargeAssetTransaction<PgasId, FungiblesAdapter<...>>`. The
//! [`ChargeAssetTxPayment`](crate::ChargeAssetTxPayment) extension auto-routes signed transactions
//! that leave the asset unspecified to PGAS when the signer holds enough; explicit `Some(PgasId)`
//! is also honored.

use super::{AssetBalanceOf, AssetIdOf, BalanceOf, Config};
use codec::{DecodeWithMemTracking, FullCodec};
use core::{fmt::Debug, marker::PhantomData};
use frame_support::{
	traits::{
		tokens::{
			fungibles::{Balanced, Credit, Inspect},
			Fortitude, Precision, Preservation, WithdrawConsequence,
		},
		Get,
	},
	unsigned::TransactionValidityError,
};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{DispatchInfoOf, MaybeSerializeDeserialize, PostDispatchInfoOf},
	transaction_validity::InvalidTransaction,
};

use crate::OnChargeAssetTransaction;

/// [`OnChargeAssetTransaction`] adapter that intercepts PGAS fee payments (withdraw + burn) and
/// delegates to `Inner` for any other asset. Both paths produce the same
/// `Credit<T::AccountId, T::Fungibles>` so `correct_and_deposit_fee` can dispatch on the credit's
/// asset id.
pub struct PgasOnChargeAssetTransaction<PgasId, Inner>(PhantomData<(PgasId, Inner)>);

impl<T, PgasId, Inner> OnChargeAssetTransaction<T> for PgasOnChargeAssetTransaction<PgasId, Inner>
where
	T: Config,
	PgasId: Get<Option<AssetIdOf<T>>>,
	Inner: OnChargeAssetTransaction<
		T,
		Balance = BalanceOf<T>,
		AssetId = AssetIdOf<T>,
		LiquidityInfo = Credit<T::AccountId, T::Fungibles>,
	>,
	AssetBalanceOf<T>: From<BalanceOf<T>>,
	AssetIdOf<T>: FullCodec
		+ DecodeWithMemTracking
		+ Clone
		+ MaybeSerializeDeserialize
		+ Debug
		+ Default
		+ Eq
		+ TypeInfo,
{
	type AssetId = AssetIdOf<T>;
	type Balance = BalanceOf<T>;
	type LiquidityInfo = Credit<T::AccountId, T::Fungibles>;

	fn withdraw_fee(
		who: &T::AccountId,
		call: &T::RuntimeCall,
		dispatch_info: &DispatchInfoOf<T::RuntimeCall>,
		asset_id: Self::AssetId,
		fee: Self::Balance,
		tip: Self::Balance,
	) -> Result<Self::LiquidityInfo, TransactionValidityError> {
		if PgasId::get().as_ref() == Some(&asset_id) {
			// PGAS is pegged 1:1 with the native fee, so no swap is needed. `Expendable` lets the
			// caller fully drain their PGAS on the final tx (PGAS is a sufficient asset, so reaping
			// on payment is safe).
			<T::Fungibles as Balanced<T::AccountId>>::withdraw(
				asset_id,
				who,
				fee.into(),
				Precision::Exact,
				Preservation::Expendable,
				Fortitude::Polite,
			)
			.map_err(|_| InvalidTransaction::Payment.into())
		} else {
			Inner::withdraw_fee(who, call, dispatch_info, asset_id, fee, tip)
		}
	}

	fn can_withdraw_fee(
		who: &T::AccountId,
		call: &T::RuntimeCall,
		dispatch_info: &DispatchInfoOf<T::RuntimeCall>,
		asset_id: Self::AssetId,
		fee: Self::Balance,
		tip: Self::Balance,
	) -> Result<(), TransactionValidityError> {
		if PgasId::get().as_ref() == Some(&asset_id) {
			match <T::Fungibles as Inspect<T::AccountId>>::can_withdraw(asset_id, who, fee.into()) {
				WithdrawConsequence::Success | WithdrawConsequence::ReducedToZero(_) => Ok(()),
				_ => Err(InvalidTransaction::Payment.into()),
			}
		} else {
			Inner::can_withdraw_fee(who, call, dispatch_info, asset_id, fee, tip)
		}
	}

	fn correct_and_deposit_fee(
		who: &T::AccountId,
		dispatch_info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &PostDispatchInfoOf<T::RuntimeCall>,
		corrected_fee: Self::Balance,
		tip: Self::Balance,
		already_withdrawn: Self::LiquidityInfo,
	) -> Result<(AssetBalanceOf<T>, AssetBalanceOf<T>), TransactionValidityError> {
		if PgasId::get().as_ref() == Some(&already_withdrawn.asset()) {
			let (fee_credit, refund_credit) = already_withdrawn.split(corrected_fee.into());
			// If resolve fails the refund credit is dropped, which burns it. That matches the
			// PGAS "burn on fee" semantics so we don't need to recover from it.
			let _ = <T::Fungibles as Balanced<T::AccountId>>::resolve(who, refund_credit);
			// Drop burns the fee via `pallet_assets::OnDropCredit`.
			drop(fee_credit);
			Ok((corrected_fee.into(), tip.into()))
		} else {
			Inner::correct_and_deposit_fee(
				who,
				dispatch_info,
				post_info,
				corrected_fee,
				tip,
				already_withdrawn,
			)
		}
	}
}
