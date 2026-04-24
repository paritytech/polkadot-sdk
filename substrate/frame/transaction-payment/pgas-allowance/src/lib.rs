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

//! # PGAS Fee Payment
//!
//! Provides:
//!
//! - [`PgasOnChargeAssetTransaction`]: an [`OnChargeAssetTransaction`] adapter that withdraws and
//!   burns PGAS when it is the fee asset, and delegates to an inner adapter for any other asset.
//! - [`ChargeFeeWithPgas`]: a transaction extension that wraps
//!   [`ChargeAssetTxPayment`](pallet_asset_conversion_tx_payment::ChargeAssetTxPayment) and
//!   auto-routes fee payment to PGAS when a signed transaction leaves the asset unspecified but the
//!   call matches [`Config::CallFilter`] and the signer holds enough PGAS. When the user specifies
//!   PGAS explicitly the path is taken regardless of the filter. Otherwise the extension behaves
//!   exactly like `ChargeAssetTxPayment`.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use codec::{Decode, DecodeWithMemTracking, Encode};
use core::marker::PhantomData;
use frame_support::{
	dispatch::{DispatchInfo, DispatchResult, PostDispatchInfo},
	pallet_prelude::TransactionSource,
	traits::{
		Contains, Get,
		tokens::{
			Fortitude, Precision, Preservation, WithdrawConsequence,
			fungibles::{self, Credit, Inspect},
		},
	},
	unsigned::TransactionValidityError,
	weights::Weight,
};
use frame_system::pallet_prelude::OriginFor;
use pallet_asset_conversion_tx_payment::{
	ChargeAssetTxPayment, OnChargeAssetTransaction, Pre as InnerPre, Val as InnerVal,
};
use scale_info::{StaticTypeInfo, TypeInfo};
use sp_runtime::{
	traits::{
		AsSystemOriginSigner, DispatchInfoOf, Dispatchable, Implication, PostDispatchInfoOf,
		TransactionExtension, ValidateResult,
	},
	transaction_validity::InvalidTransaction,
};

pub use pallet::*;
pub use weights::WeightInfo;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod weights;

/// Native balance type, as seen by `pallet_transaction_payment`.
pub type BalanceOf<T> =
	<<T as pallet_transaction_payment::Config>::OnChargeTransaction as
		pallet_transaction_payment::OnChargeTransaction<T>>::Balance;

/// Asset identifier type configured in `pallet_asset_conversion_tx_payment`.
pub type AssetIdOf<T> = <T as pallet_asset_conversion_tx_payment::Config>::AssetId;

/// Helper used by the extension benchmarks to endow the caller with PGAS.
#[cfg(feature = "runtime-benchmarks")]
pub trait BenchmarkHelperTrait<AccountId, AssetId, Balance> {
	/// Mint `amount` of PGAS to `who`.
	fn mint_pgas(who: &AccountId, asset_id: AssetId, amount: Balance);
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_asset_conversion_tx_payment::Config {
		/// The PGAS asset id.
		type PgasId: Get<AssetIdOf<Self>>;

		/// Filter deciding which calls auto-route to PGAS when the transaction leaves the asset
		/// unspecified.
		type CallFilter: Contains<<Self as frame_system::Config>::RuntimeCall>;

		/// Fungibles registry used to inspect PGAS balances at validate time.
		type Fungibles: fungibles::Inspect<
				Self::AccountId,
				AssetId = AssetIdOf<Self>,
				Balance = BalanceOf<Self>,
			>;

		/// Weight information for the extension.
		type WeightInfo: WeightInfo;

		/// Helper used by the extension benchmarks to endow the caller with enough PGAS to cover
		/// the fee.
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: crate::BenchmarkHelperTrait<Self::AccountId, AssetIdOf<Self>, BalanceOf<Self>>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);
}

// -- `OnChargeAssetTransaction` adapter ---------------------------------------------------------

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
	T: pallet_asset_conversion_tx_payment::Config,
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

// -- `ChargeFeeWithPgas` extension --------------------------------------------------------------

/// Transaction extension wrapping [`ChargeAssetTxPayment`]. When a signed call has no asset
/// specified, passes [`Config::CallFilter`] and the signer holds enough PGAS, the extension
/// substitutes [`Config::PgasId`] as the fee asset; otherwise it delegates to the inner extension
/// unchanged.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq)]
pub struct ChargeFeeWithPgas<T: Config> {
	#[codec(compact)]
	tip: BalanceOf<T>,
	asset_id: Option<AssetIdOf<T>>,
	/// When set, the PGAS routing is disabled and the extension behaves exactly like
	/// `ChargeAssetTxPayment`. Codec-skipped: this flag is only populated by runtime code
	/// (e.g. the Ethereum transaction pipeline) and never comes from the wire.
	#[codec(skip)]
	skip_pgas: bool,
}

impl<T: Config> ChargeFeeWithPgas<T> {
	/// Build an extension mirroring [`ChargeAssetTxPayment::from`] but with PGAS auto-routing.
	pub fn from(tip: BalanceOf<T>, asset_id: Option<AssetIdOf<T>>) -> Self {
		Self { tip, asset_id, skip_pgas: false }
	}

	/// Build an extension that never auto-routes to PGAS. Used by pipelines (Ethereum) where the
	/// originator cannot be assumed to hold PGAS.
	pub fn new_skip_pgas(tip: BalanceOf<T>, asset_id: Option<AssetIdOf<T>>) -> Self {
		Self { tip, asset_id, skip_pgas: true }
	}

	/// Decide which asset to charge for this transaction.
	fn effective_asset_id(
		&self,
		who: &T::AccountId,
		call: &T::RuntimeCall,
		fee: BalanceOf<T>,
	) -> Option<AssetIdOf<T>> {
		if self.skip_pgas || self.asset_id.is_some() || !T::CallFilter::contains(call) {
			return self.asset_id.clone();
		}
		let pgas_id = T::PgasId::get();
		let pgas_balance = T::Fungibles::reducible_balance(
			pgas_id.clone(),
			who,
			Preservation::Expendable,
			Fortitude::Polite,
		);
		if pgas_balance >= fee { Some(pgas_id) } else { None }
	}
}

/// Present the same metadata as [`ChargeAssetTxPayment`] so clients (wallets, indexers) cannot
/// distinguish the two on the wire. The wire-level encoding must stay in lockstep with
/// `ChargeAssetTxPayment<T>`: `(Compact<Balance>, Option<AssetId>)`.
impl<T: Config> TypeInfo for ChargeFeeWithPgas<T>
where
	ChargeAssetTxPayment<T>: StaticTypeInfo,
{
	type Identity = ChargeAssetTxPayment<T>;
	fn type_info() -> scale_info::Type {
		<ChargeAssetTxPayment<T> as TypeInfo>::type_info()
	}
}

impl<T: Config> core::fmt::Debug for ChargeFeeWithPgas<T> {
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		write!(f, "ChargeFeeWithPgas<{:?}, {:?}>", self.tip, self.asset_id.encode())
	}
	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut core::fmt::Formatter) -> core::fmt::Result {
		Ok(())
	}
}

/// Info passed from `validate` to `prepare`. Carries the asset id the extension settled on (the
/// runtime-level decision) so `prepare` doesn't have to reach for the balance again.
pub enum Val<T: pallet_asset_conversion_tx_payment::Config> {
	/// Either the PGAS or the native path (matches the inner extension's `Charge`/`NoCharge`
	/// result through `inner`).
	Charge { asset_id: Option<AssetIdOf<T>>, inner: InnerVal<T> },
}

/// Info passed from `prepare` to `post_dispatch_details`. Tracks both the inner `Pre` and the
/// weight this extension reserved so we can refund the unused portion ourselves (the inner refund
/// formula assumes the inner's own reservation).
pub enum Pre<T: pallet_asset_conversion_tx_payment::Config> {
	Charge { asset_id: Option<AssetIdOf<T>>, inner: InnerPre<T>, reserved: Weight },
}

impl<T: Config + Send + Sync> TransactionExtension<T::RuntimeCall> for ChargeFeeWithPgas<T>
where
	T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
	BalanceOf<T>: Send + Sync + From<u64>,
	AssetIdOf<T>: Send + Sync,
	<T::RuntimeCall as Dispatchable>::RuntimeOrigin: AsSystemOriginSigner<T::AccountId> + Clone,
	ChargeAssetTxPayment<T>: StaticTypeInfo,
{
	const IDENTIFIER: &'static str =
		<ChargeAssetTxPayment<T> as TransactionExtension<T::RuntimeCall>>::IDENTIFIER;
	type Implicit = <ChargeAssetTxPayment<T> as TransactionExtension<T::RuntimeCall>>::Implicit;
	type Val = Val<T>;
	type Pre = Pre<T>;

	fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
		ChargeAssetTxPayment::<T>::from(self.tip, self.asset_id.clone()).implicit()
	}

	fn metadata() -> alloc::vec::Vec<sp_runtime::traits::TransactionExtensionMetadata> {
		<ChargeAssetTxPayment<T> as TransactionExtension<T::RuntimeCall>>::metadata()
	}

	fn weight(&self, _call: &T::RuntimeCall) -> Weight {
		// Worst case: the PGAS balance read ran and `ChargeAssetTxPayment` took its asset path.
		<T as Config>::WeightInfo::charge_pgas().max(<T as Config>::WeightInfo::charge_pgas_skip())
	}

	fn validate(
		&self,
		origin: OriginFor<T>,
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
		self_implicit: Self::Implicit,
		inherited_implication: &impl Implication,
		source: TransactionSource,
	) -> ValidateResult<Self::Val, T::RuntimeCall> {
		let effective = if let Some(who) = origin.as_system_origin_signer() {
			let fee =
				pallet_transaction_payment::Pallet::<T>::compute_fee(len as u32, info, self.tip);
			self.effective_asset_id(who, call, fee)
		} else {
			self.asset_id.clone()
		};

		let inner = ChargeAssetTxPayment::<T>::from(self.tip, effective.clone());
		let (validity, inner_val, origin) = inner.validate(
			origin,
			call,
			info,
			len,
			self_implicit,
			inherited_implication,
			source,
		)?;
		Ok((validity, Val::Charge { asset_id: effective, inner: inner_val }, origin))
	}

	fn prepare(
		self,
		val: Self::Val,
		origin: &OriginFor<T>,
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		let reserved = <Self as TransactionExtension<T::RuntimeCall>>::weight(&self, call);
		let Val::Charge { asset_id, inner } = val;
		let inner_ext = ChargeAssetTxPayment::<T>::from(self.tip, asset_id.clone());
		let inner_pre = inner_ext.prepare(inner, origin, call, info, len)?;
		Ok(Pre::Charge { asset_id, inner: inner_pre, reserved })
	}

	fn post_dispatch_details(
		pre: Self::Pre,
		info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &PostDispatchInfoOf<T::RuntimeCall>,
		len: usize,
		result: &DispatchResult,
	) -> Result<Weight, TransactionValidityError> {
		let Pre::Charge { asset_id, inner, reserved } = pre;
		// Delegate for the fee-correction side effects (refund/burn/deposit). The inner's own
		// weight refund is relative to its own reservation, which is never part of ours, so we
		// discard it.
		let _ = <ChargeAssetTxPayment<T> as TransactionExtension<T::RuntimeCall>>::post_dispatch_details(
			inner, info, post_info, len, result,
		)?;
		let actual_path = if asset_id.is_some() {
			<T as Config>::WeightInfo::charge_pgas()
		} else {
			<T as Config>::WeightInfo::charge_pgas_skip()
		};
		Ok(reserved.saturating_sub(actual_path))
	}
}
