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

//! # Asset Conversion Storage Deposit
//!
//! Allows storage deposits to be paid in non-native fungible assets.
//!
//! ## Overview
//!
//! Storage deposits are refundable collateral locked when on-chain state is created. They
//! traditionally require the chain's native token. This pallet lifts that restriction by providing:
//!
//! - [`FungiblesHoldConsideration`]: a [`Consideration`] implementation that holds a non-native
//!   fungible asset directly (same nominal amount as the native deposit). A DOT/KSM liquidity pool
//!   must exist for the chosen asset in `pallet-asset-conversion`; no conversion is performed.
//!
//! - [`ChargeAssetStorageDeposit`]: a [`TransactionExtension`] that lets signers declare their
//!   preferred deposit asset via an optional `AssetKind` field. The extension records the choice in
//!   [`DepositAsset`] storage before the call is dispatched and clears it afterwards.
//!
//! ## Usage
//!
//! 1. Include this pallet in your `construct_runtime!`.
//! 2. Wire [`FungiblesHoldConsideration`] as the `Consideration` type in pallets that create
//!    storage deposits (replacing e.g. `fungible::HoldConsideration`).
//! 3. Add [`ChargeAssetStorageDeposit`] to the signed-extension pipeline.
//!
//! If no asset is specified in the extension (or for unsigned transactions), behaviour is identical
//! to the native [`fungible::HoldConsideration`].

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use core::marker::PhantomData;
use frame_support::{
	ensure,
	pallet_prelude::Weight,
	traits::{
		fungibles,
		tokens::Precision,
		Consideration, Footprint,
	},
	CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound,
};
use pallet_asset_conversion::{PoolLocator, Pools};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{Convert, DispatchInfoOf, Dispatchable, Get, PostDispatchInfoOf, TransactionExtension,
		Zero},
	transaction_validity::{TransactionValidityError, ValidTransaction},
};

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::composite_enum]
	pub enum HoldReason {
		StorageDeposit,
	}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The asset identifier type — must match `pallet-asset-conversion`'s `AssetKind`.
		type AssetKind: Parameter + MaxEncodedLen + PartialEq + Send + Sync + 'static;

		/// Multi-asset fungibles that support holds. Must use the same `AssetId` as
		/// `AssetKind`.
		type Assets: fungibles::hold::Mutate<Self::AccountId, AssetId = Self::AssetKind>
			+ fungibles::Inspect<Self::AccountId, AssetId = Self::AssetKind>;

		/// The asset-conversion pallet instance used to verify pool existence.
		type AssetConversion: pallet_asset_conversion::Config<AssetKind = Self::AssetKind>;

		/// The native asset identifier. Used to verify that a DOT/<asset> pool exists and as the
		/// fallback when no preferred asset is specified.
		type NativeAsset: Get<Self::AssetKind>;
	}

	/// The asset preferred by the current transaction for storage deposit collateral.
	///
	/// Set by [`ChargeAssetStorageDeposit`] in its `prepare` step and cleared in
	/// `post_dispatch_details`. When `None`, deposits fall back to the native asset.
	#[pallet::storage]
	pub type DepositAsset<T: Config> = StorageValue<_, T::AssetKind, OptionQuery>;

	#[pallet::error]
	pub enum Error<T> {
		/// No liquidity pool exists between the native asset and the requested deposit asset.
		NoPoolForDepositAsset,
		/// The pair of assets is not a valid pool pair in `pallet-asset-conversion`.
		InvalidAssetPair,
	}
}

// ---------------------------------------------------------------------------
// FungiblesHoldConsideration
// ---------------------------------------------------------------------------

/// A [`Consideration`] that holds a non-native fungible asset as storage deposit collateral.
///
/// On creation, reads the preferred asset from [`DepositAsset`] storage (written by
/// [`ChargeAssetStorageDeposit`] before dispatch). If an asset other than the native token is
/// found, the existence of a liquidity pool for that asset in `pallet-asset-conversion` is
/// verified; the hold then targets that asset. The held amount equals the deposit amount that
/// would be charged for the same footprint in the native token — no price conversion is performed.
///
/// On release ([`drop`](Consideration::drop)), the exact amount stored in the ticket is returned
/// to the account, regardless of any price changes since the deposit was created.
///
/// # Type parameters
///
/// - `T` — the pallet's [`Config`], providing `AssetKind`, `Assets`, `AssetConversion`, and
///   `NativeAsset`.
/// - `R` — supplies the hold [`Reason`](fungibles::hold::Inspect::Reason).
/// - `D` — converts a [`Footprint`] to a [`Balance`] (e.g. [`LinearStoragePrice`]).
/// - `Fp` — the footprint type (defaults to [`Footprint`]).
///
/// [`LinearStoragePrice`]: frame_support::traits::LinearStoragePrice
#[derive(
	CloneNoBound,
	EqNoBound,
	PartialEqNoBound,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
	DebugNoBound,
)]
#[scale_info(skip_type_params(T, R, D, Fp))]
#[codec(mel_bound())]
pub struct FungiblesHoldConsideration<T, R, D, Fp = Footprint>(
	/// Asset being held.
	T::AssetKind,
	/// Amount held.
	<T::Assets as fungibles::Inspect<T::AccountId>>::Balance,
	PhantomData<fn() -> (R, D, Fp)>,
)
where
	T: Config;

impl<T, R, D, Fp> Consideration<T::AccountId, Fp> for FungiblesHoldConsideration<T, R, D, Fp>
where
	T: Config,
	R: 'static + Get<<T::Assets as fungibles::hold::Inspect<T::AccountId>>::Reason>,
	D: 'static + Convert<Fp, <T::Assets as fungibles::Inspect<T::AccountId>>::Balance>,
	Fp: 'static,
{
	fn new(who: &T::AccountId, footprint: Fp) -> Result<Self, sp_runtime::DispatchError> {
		let asset_id = DepositAsset::<T>::get().unwrap_or_else(T::NativeAsset::get);

		// For non-native assets, verify a pool exists with the native token.
		if asset_id != T::NativeAsset::get() {
			let pool_id =
				<T::AssetConversion as pallet_asset_conversion::Config>::PoolLocator::pool_id(
					&T::NativeAsset::get(),
					&asset_id,
				)
				.map_err(|_| Error::<T>::InvalidAssetPair)?;

			ensure!(
				Pools::<T::AssetConversion>::contains_key(&pool_id),
				Error::<T>::NoPoolForDepositAsset,
			);
		}

		use fungibles::hold::Mutate as FungiblesHoldMutate;
		let amount = D::convert(footprint);
		if !amount.is_zero() {
			<T::Assets as FungiblesHoldMutate<T::AccountId>>::hold(
				asset_id.clone(),
				&R::get(),
				who,
				amount,
			)?;
		}
		Ok(Self(asset_id, amount, PhantomData))
	}

	fn update(self, who: &T::AccountId, footprint: Fp) -> Result<Self, sp_runtime::DispatchError> {
		use fungibles::hold::Mutate as FungiblesHoldMutate;
		let new_amount = D::convert(footprint);
		match self.1.cmp(&new_amount) {
			core::cmp::Ordering::Greater => {
				<T::Assets as FungiblesHoldMutate<T::AccountId>>::release(
					self.0.clone(),
					&R::get(),
					who,
					self.1 - new_amount,
					Precision::BestEffort,
				)?;
			},
			core::cmp::Ordering::Less => {
				<T::Assets as FungiblesHoldMutate<T::AccountId>>::hold(
					self.0.clone(),
					&R::get(),
					who,
					new_amount - self.1,
				)?;
			},
			core::cmp::Ordering::Equal => {},
		}
		Ok(Self(self.0, new_amount, PhantomData))
	}

	fn drop(self, who: &T::AccountId) -> Result<(), sp_runtime::DispatchError> {
		use fungibles::hold::Mutate as FungiblesHoldMutate;
		if !self.1.is_zero() {
			<T::Assets as FungiblesHoldMutate<T::AccountId>>::release(
				self.0,
				&R::get(),
				who,
				self.1,
				Precision::BestEffort,
			)
			.map(|_| ())
		} else {
			Ok(())
		}
	}

	fn burn(self, who: &T::AccountId) {
		use fungibles::hold::Mutate as FungiblesHoldMutate;
		if !self.1.is_zero() {
			let _ = <T::Assets as FungiblesHoldMutate<T::AccountId>>::burn_held(
				self.0,
				&R::get(),
				who,
				self.1,
				Precision::BestEffort,
				frame_support::traits::tokens::Fortitude::Force,
			);
		}
	}
}

// ---------------------------------------------------------------------------
// ChargeAssetStorageDeposit — TransactionExtension
// ---------------------------------------------------------------------------

/// A [`TransactionExtension`] that records the signer's preferred deposit asset before dispatch.
///
/// Include this extension in the signed-extension pipeline alongside any fee-payment extensions.
/// The `asset_id` field is `None` by default, which leaves deposit behaviour unchanged (native
/// token). When `Some(id)` is provided, [`DepositAsset`] is set before the call is executed so
/// that [`FungiblesHoldConsideration`] can read it.
///
/// The storage entry is always cleared in [`post_dispatch_details`] regardless of the outcome of
/// the call, including on error.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct ChargeAssetStorageDeposit<T: Config> {
	pub asset_id: Option<T::AssetKind>,
}

impl<T: Config> ChargeAssetStorageDeposit<T> {
	pub fn new(asset_id: Option<T::AssetKind>) -> Self {
		Self { asset_id }
	}
}

impl<T: Config> core::fmt::Debug for ChargeAssetStorageDeposit<T> {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		write!(f, "ChargeAssetStorageDeposit")
	}
}

impl<T: Config> TransactionExtension<T::RuntimeCall> for ChargeAssetStorageDeposit<T>
where
	T::RuntimeCall: Dispatchable<Info = frame_support::dispatch::DispatchInfo>,
	<T::RuntimeCall as Dispatchable>::RuntimeOrigin:
		sp_runtime::traits::AsSystemOriginSigner<T::AccountId> + Clone,
{
	const IDENTIFIER: &'static str = "ChargeAssetStorageDeposit";
	type Implicit = ();
	type Val = ();
	type Pre = Option<T::AssetKind>;

	fn weight(&self, _call: &T::RuntimeCall) -> Weight {
		Weight::zero()
	}

	fn validate(
		&self,
		origin: <T::RuntimeCall as Dispatchable>::RuntimeOrigin,
		_call: &T::RuntimeCall,
		_info: &DispatchInfoOf<T::RuntimeCall>,
		_len: usize,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Encode,
		_source: sp_runtime::transaction_validity::TransactionSource,
	) -> sp_runtime::traits::ValidateResult<Self::Val, T::RuntimeCall> {
		Ok((ValidTransaction::default(), (), origin))
	}

	fn prepare(
		self,
		_val: Self::Val,
		_origin: &<T::RuntimeCall as Dispatchable>::RuntimeOrigin,
		_call: &T::RuntimeCall,
		_info: &DispatchInfoOf<T::RuntimeCall>,
		_len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		// Write the preferred asset into storage so FungiblesHoldConsideration can read it
		// during the call dispatch (which happens after prepare returns).
		if let Some(ref asset) = self.asset_id {
			DepositAsset::<T>::set(Some(asset.clone()));
		}
		Ok(self.asset_id)
	}

	fn post_dispatch_details(
		pre: Self::Pre,
		_info: &DispatchInfoOf<T::RuntimeCall>,
		_post_info: &PostDispatchInfoOf<T::RuntimeCall>,
		_len: usize,
		_result: &sp_runtime::DispatchResult,
	) -> Result<Weight, TransactionValidityError> {
		// Always clear — whether or not an asset was set — to prevent any cross-extrinsic leakage.
		if pre.is_some() {
			DepositAsset::<T>::kill();
		}
		Ok(Weight::zero())
	}
}
