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

//! # Asset Conversion Transaction Payment Pallet
//!
//! This pallet allows runtimes that include it to pay for transactions in assets other than the
//! chain's native asset.
//!
//! ## Overview
//!
//! This pallet provides a `TransactionExtension` with an optional `AssetId` that specifies the
//! asset to be used for payment (defaulting to the native token on `None`). It expects an
//! [`OnChargeAssetTransaction`] implementation analogous to [`pallet-transaction-payment`]. The
//! included [`SwapAssetAdapter`] (implementing [`OnChargeAssetTransaction`]) determines the
//! fee amount by converting the fee calculated by [`pallet-transaction-payment`] in the native
//! asset into the amount required of the specified asset.
//!
//! ## Pallet API
//!
//! This pallet does not have any dispatchable calls or storage. It wraps FRAME's Transaction
//! Payment pallet and functions as a replacement. This means you should include both pallets in
//! your `construct_runtime` macro, but only include this pallet's [`TransactionExtension`]
//! ([`ChargeAssetTxPayment`]).
//!
//! ## Terminology
//!
//! - Native Asset or Native Currency: The asset that a chain considers native, as in its default
//!   for transaction fee payment, deposits, inflation, etc.
//! - Other assets: Other assets that may exist on chain, for example under the Assets pallet.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use codec::{Decode, Encode};
use frame_support::{
	dispatch::{DispatchInfo, DispatchResult, PostDispatchInfo},
	pallet_prelude::TransactionSource,
	traits::IsType,
	DefaultNoBound,
};
use pallet_transaction_payment::{ChargeTransactionPayment, OnChargeTransaction};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{
		AsSystemOriginSigner, DispatchInfoOf, Dispatchable, PostDispatchInfoOf, RefundWeight,
		TransactionExtension, ValidateResult, Zero,
	},
	transaction_validity::{InvalidTransaction, TransactionValidityError, ValidTransaction},
};

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

mod payment;
use frame_support::{pallet_prelude::Weight, traits::tokens::AssetId};
pub use payment::*;
pub use weights::WeightInfo;

/// Balance type alias for balances of the chain's native asset.
pub(crate) type BalanceOf<T> = <OnChargeTransactionOf<T> as OnChargeTransaction<T>>::Balance;

/// Type aliases used for interaction with `OnChargeTransaction`.
pub(crate) type OnChargeTransactionOf<T> =
	<T as pallet_transaction_payment::Config>::OnChargeTransaction;

/// Liquidity info type alias for the chain's native asset.
pub(crate) type NativeLiquidityInfoOf<T> =
	<OnChargeTransactionOf<T> as OnChargeTransaction<T>>::LiquidityInfo;

/// Liquidity info type alias for the chain's assets.
pub(crate) type AssetLiquidityInfoOf<T> =
	<<T as Config>::OnChargeAssetTransaction as OnChargeAssetTransaction<T>>::LiquidityInfo;

/// Used to pass the initial payment info from pre- to post-dispatch.
#[derive(Encode, Decode, DefaultNoBound, TypeInfo)]
pub enum InitialPayment<T: Config> {
	/// No initial fee was paid.
	#[default]
	Nothing,
	/// The initial fee was paid in the native currency.
	Native(NativeLiquidityInfoOf<T>),
	/// The initial fee was paid in an asset.
	Asset((T::AssetId, AssetLiquidityInfoOf<T>)),
}

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_transaction_payment::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// The asset ID type that can be used for transaction payments in addition to a
		/// native asset.
		type AssetId: AssetId;
		/// The actual transaction charging logic that charges the fees.
		type OnChargeAssetTransaction: OnChargeAssetTransaction<
			Self,
			Balance = BalanceOf<Self>,
			AssetId = Self::AssetId,
		>;
		/// The weight information of this pallet.
		type WeightInfo: WeightInfo;
		#[cfg(feature = "runtime-benchmarks")]
		/// Benchmark helper
		type BenchmarkHelper: BenchmarkHelperTrait<
			Self::AccountId,
			Self::AssetId,
			<<Self as Config>::OnChargeAssetTransaction as OnChargeAssetTransaction<Self>>::AssetId,
		>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[cfg(feature = "runtime-benchmarks")]
	/// Helper trait to benchmark the `ChargeAssetTxPayment` transaction extension.
	pub trait BenchmarkHelperTrait<AccountId, FunAssetIdParameter, AssetIdParameter> {
		/// Returns the `AssetId` to be used in the liquidity pool by the benchmarking code.
		fn create_asset_id_parameter(id: u32) -> (FunAssetIdParameter, AssetIdParameter);
		/// Create a liquidity pool for a given asset and sufficiently endow accounts to benchmark
		/// the extension.
		fn setup_balances_and_pool(asset_id: FunAssetIdParameter, account: AccountId);
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A transaction fee `actual_fee`, of which `tip` was added to the minimum inclusion fee,
		/// has been paid by `who` in an asset `asset_id`.
		AssetTxFeePaid {
			who: T::AccountId,
			actual_fee: BalanceOf<T>,
			tip: BalanceOf<T>,
			asset_id: T::AssetId,
		},
		/// A swap of the refund in native currency back to asset failed.
		AssetRefundFailed { native_amount_kept: BalanceOf<T> },
	}
}

/// Require payment for transaction inclusion and optionally include a tip to gain additional
/// priority in the queue.
///
/// Wraps the transaction logic in [`pallet_transaction_payment`] and extends it with assets.
/// An asset ID of `None` falls back to the underlying transaction payment logic via the native
/// currency.
///
/// Transaction payments are processed using different handlers based on the asset type:
/// - Payments with a native asset are charged by
///   [pallet_transaction_payment::Config::OnChargeTransaction].
/// - Payments with other assets are charged by [Config::OnChargeAssetTransaction].
#[derive(Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct ChargeAssetTxPayment<T: Config> {
	#[codec(compact)]
	tip: BalanceOf<T>,
	asset_id: Option<T::AssetId>,
}

impl<T: Config> ChargeAssetTxPayment<T>
where
	T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
{
	/// Utility constructor. Used only in client/factory code.
	pub fn from(tip: BalanceOf<T>, asset_id: Option<T::AssetId>) -> Self {
		Self { tip, asset_id }
	}

	/// Fee withdrawal logic that dispatches to either [`Config::OnChargeAssetTransaction`] or
	/// [`pallet_transaction_payment::Config::OnChargeTransaction`].
	fn withdraw_fee(
		&self,
		who: &T::AccountId,
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		fee: BalanceOf<T>,
	) -> Result<(BalanceOf<T>, InitialPayment<T>), TransactionValidityError> {
		debug_assert!(self.tip <= fee, "tip should be included in the computed fee");
		if fee.is_zero() {
			Ok((fee, InitialPayment::Nothing))
		} else if let Some(asset_id) = &self.asset_id {
			T::OnChargeAssetTransaction::withdraw_fee(
				who,
				call,
				info,
				asset_id.clone(),
				fee,
				self.tip,
			)
			.map(|payment| (fee, InitialPayment::Asset((asset_id.clone(), payment))))
		} else {
			T::OnChargeTransaction::withdraw_fee(who, call, info, fee, self.tip)
				.map(|payment| (fee, InitialPayment::Native(payment)))
		}
	}

	/// Fee withdrawal logic dry-run that dispatches to either `OnChargeAssetTransaction` or
	/// `OnChargeTransaction`.
	fn can_withdraw_fee(
		&self,
		who: &T::AccountId,
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		fee: BalanceOf<T>,
	) -> Result<(), TransactionValidityError> {
		debug_assert!(self.tip <= fee, "tip should be included in the computed fee");
		if fee.is_zero() {
			Ok(())
		} else if let Some(asset_id) = &self.asset_id {
			T::OnChargeAssetTransaction::can_withdraw_fee(who, asset_id.clone(), fee.into())
		} else {
			<OnChargeTransactionOf<T> as OnChargeTransaction<T>>::can_withdraw_fee(
				who, call, info, fee, self.tip,
			)
			.map_err(|_| -> TransactionValidityError { InvalidTransaction::Payment.into() })
		}
	}
}

impl<T: Config> core::fmt::Debug for ChargeAssetTxPayment<T> {
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		write!(f, "ChargeAssetTxPayment<{:?}, {:?}>", self.tip, self.asset_id.encode())
	}
	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut core::fmt::Formatter) -> core::fmt::Result {
		Ok(())
	}
}

/// The info passed between the validate and prepare steps for the `ChargeAssetTxPayment` extension.
pub enum Val<T: Config> {
	Charge {
		tip: BalanceOf<T>,
		// who paid the fee
		who: T::AccountId,
		// transaction fee
		fee: BalanceOf<T>,
	},
	NoCharge,
}

/// The info passed between the prepare and post-dispatch steps for the `ChargeAssetTxPayment`
/// extension.
pub enum Pre<T: Config> {
	Charge {
		tip: BalanceOf<T>,
		// who paid the fee
		who: T::AccountId,
		// imbalance resulting from withdrawing the fee
		initial_payment: InitialPayment<T>,
		// weight used by the extension
		weight: Weight,
	},
	NoCharge {
		// weight initially estimated by the extension, to be refunded
		refund: Weight,
	},
}

impl<T: Config> TransactionExtension<T::RuntimeCall> for ChargeAssetTxPayment<T>
where
	T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
	BalanceOf<T>: Send + Sync + From<u64>,
	T::AssetId: Send + Sync,
	<T::RuntimeCall as Dispatchable>::RuntimeOrigin: AsSystemOriginSigner<T::AccountId> + Clone,
{
	const IDENTIFIER: &'static str = "ChargeAssetTxPayment";
	type Implicit = ();
	type Val = Val<T>;
	type Pre = Pre<T>;

	fn weight(&self, _: &T::RuntimeCall) -> Weight {
		if self.asset_id.is_some() {
			<T as Config>::WeightInfo::charge_asset_tx_payment_asset()
		} else {
			<T as Config>::WeightInfo::charge_asset_tx_payment_native()
		}
	}

	fn validate(
		&self,
		origin: <T::RuntimeCall as Dispatchable>::RuntimeOrigin,
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Encode,
		_source: TransactionSource,
	) -> ValidateResult<Self::Val, T::RuntimeCall> {
		let Some(who) = origin.as_system_origin_signer() else {
			return Ok((ValidTransaction::default(), Val::NoCharge, origin))
		};
		// Non-mutating call of `compute_fee` to calculate the fee used in the transaction priority.
		let fee = pallet_transaction_payment::Pallet::<T>::compute_fee(len as u32, info, self.tip);
		self.can_withdraw_fee(&who, call, info, fee)?;
		let priority = ChargeTransactionPayment::<T>::get_priority(info, len, self.tip, fee);
		let validity = ValidTransaction { priority, ..Default::default() };
		let val = Val::Charge { tip: self.tip, who: who.clone(), fee };
		Ok((validity, val, origin))
	}

	fn prepare(
		self,
		val: Self::Val,
		_origin: &<T::RuntimeCall as Dispatchable>::RuntimeOrigin,
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		_len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		match val {
			Val::Charge { tip, who, fee } => {
				// Mutating call of `withdraw_fee` to actually charge for the transaction.
				let (_fee, initial_payment) = self.withdraw_fee(&who, call, info, fee)?;
				Ok(Pre::Charge { tip, who, initial_payment, weight: self.weight(call) })
			},
			Val::NoCharge => Ok(Pre::NoCharge { refund: self.weight(call) }),
		}
	}

	fn post_dispatch_details(
		pre: Self::Pre,
		info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &PostDispatchInfoOf<T::RuntimeCall>,
		len: usize,
		_result: &DispatchResult,
	) -> Result<Weight, TransactionValidityError> {
		let (tip, who, initial_payment, extension_weight) = match pre {
			Pre::Charge { tip, who, initial_payment, weight } =>
				(tip, who, initial_payment, weight),
			Pre::NoCharge { refund } => {
				// No-op: Refund everything
				return Ok(refund)
			},
		};

		match initial_payment {
			InitialPayment::Native(already_withdrawn) => {
				// Take into account the weight used by this extension before calculating the
				// refund.
				let actual_ext_weight = <T as Config>::WeightInfo::charge_asset_tx_payment_native();
				let unspent_weight = extension_weight.saturating_sub(actual_ext_weight);
				let mut actual_post_info = *post_info;
				actual_post_info.refund(unspent_weight);
				let actual_fee = pallet_transaction_payment::Pallet::<T>::compute_actual_fee(
					len as u32,
					info,
					&actual_post_info,
					tip,
				);
				T::OnChargeTransaction::correct_and_deposit_fee(
					&who,
					info,
					&actual_post_info,
					actual_fee,
					tip,
					already_withdrawn,
				)?;
				pallet_transaction_payment::Pallet::<T>::deposit_fee_paid_event(
					who, actual_fee, tip,
				);
				Ok(unspent_weight)
			},
			InitialPayment::Asset((asset_id, already_withdrawn)) => {
				// Take into account the weight used by this extension before calculating the
				// refund.
				let actual_ext_weight = <T as Config>::WeightInfo::charge_asset_tx_payment_asset();
				let unspent_weight = extension_weight.saturating_sub(actual_ext_weight);
				let mut actual_post_info = *post_info;
				actual_post_info.refund(unspent_weight);
				let actual_fee = pallet_transaction_payment::Pallet::<T>::compute_actual_fee(
					len as u32,
					info,
					&actual_post_info,
					tip,
				);
				let converted_fee = T::OnChargeAssetTransaction::correct_and_deposit_fee(
					&who,
					info,
					&actual_post_info,
					actual_fee,
					tip,
					asset_id.clone(),
					already_withdrawn,
				)?;

				Pallet::<T>::deposit_event(Event::<T>::AssetTxFeePaid {
					who,
					actual_fee: converted_fee,
					tip,
					asset_id,
				});

				Ok(unspent_weight)
			},
			InitialPayment::Nothing => {
				// `actual_fee` should be zero here for any signed extrinsic. It would be
				// non-zero here in case of unsigned extrinsics as they don't pay fees but
				// `compute_actual_fee` is not aware of them. In both cases it's fine to just
				// move ahead without adjusting the fee, though, so we do nothing.
				debug_assert!(tip.is_zero(), "tip should be zero if initial fee was zero.");
				Ok(extension_weight
					.saturating_sub(<T as Config>::WeightInfo::charge_asset_tx_payment_zero()))
			},
		}
	}
}
