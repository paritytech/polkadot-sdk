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

//! # Gas Allowance Pallet
//!
//! Provides the [`ChargePGAS`] transaction extension. When a signed transaction dispatching a
//! call that passes [`Config::CallFilter`] is submitted by an account holding at least the
//! required fee in the PGAS asset, the fee is withdrawn as a [`fungibles::Credit`] held in the
//! extension's `Pre`. Any unused portion is refunded from that credit in `post_dispatch`; the
//! remainder is dropped, which burns the consumed fee via `OnDropCredit`. Otherwise the wrapped
//! extension `S` runs unchanged.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use codec::{Decode, DecodeWithMemTracking, Encode};
use frame_support::{
	dispatch::{DispatchInfo, DispatchResult, PostDispatchInfo},
	pallet_prelude::TransactionSource,
	traits::{
		tokens::{
			fungibles::{self, Credit},
			AssetId, Fortitude, Precision, Preservation,
		},
		Contains, Get,
	},
	weights::Weight,
};
use frame_system::pallet_prelude::OriginFor;
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{
		AsSystemOriginSigner, DispatchInfoOf, Dispatchable, Implication, PostDispatchInfoOf,
		TransactionExtension, ValidateResult, Zero,
	},
	transaction_validity::{InvalidTransaction, TransactionValidityError, ValidTransaction},
};

pub use pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

/// Balance type alias, sourced from `pallet-transaction-payment`.
pub type BalanceOf<T> = <<T as pallet_transaction_payment::Config>::OnChargeTransaction as
	pallet_transaction_payment::OnChargeTransaction<T>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_transaction_payment::Config {
		/// The asset id type used by the PGAS asset.
		type AssetId: AssetId;

		/// Access to the PGAS asset. `Balanced` exposes the imbalance API used to withdraw the
		/// reserved fee into a [`Credit`] and refund the unused portion in `post_dispatch`.
		type Assets: fungibles::Balanced<
			Self::AccountId,
			AssetId = <Self as Config>::AssetId,
			Balance = BalanceOf<Self>,
		>;

		/// The PGAS asset id.
		type PGASAssetId: frame_support::traits::Get<<Self as Config>::AssetId>;

		/// Filter deciding which calls are eligible to be paid with PGAS. Calls that fail the
		/// filter fall through to the inner fee extension unconditionally, even if the caller
		/// holds PGAS.
		type CallFilter: Contains<<Self as frame_system::Config>::RuntimeCall>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);
}

/// Transaction extension that charges transaction fees in PGAS when the caller holds enough and
/// the dispatched call passes [`Config::CallFilter`]. Otherwise it delegates to the wrapped
/// extension `S`.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct ChargePGAS<T, S> {
	inner: S,
	_phantom: core::marker::PhantomData<T>,
}

impl<T, S: Default> Default for ChargePGAS<T, S> {
	fn default() -> Self {
		Self { inner: S::default(), _phantom: core::marker::PhantomData }
	}
}

impl<T, S> ChargePGAS<T, S> {
	/// Create a new `ChargePGAS` wrapping the given inner extension.
	pub fn new(inner: S) -> Self {
		Self { inner, _phantom: core::marker::PhantomData }
	}
}

impl<T, S: core::fmt::Debug> core::fmt::Debug for ChargePGAS<T, S> {
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		write!(f, "ChargePGAS({:?})", self.inner)
	}
	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut core::fmt::Formatter) -> core::fmt::Result {
		Ok(())
	}
}

/// Info passed from `validate` to `prepare`.
pub enum Val<InnerVal, T: Config> {
	/// Caller pays with PGAS: `fee` units will be withdrawn in `prepare`.
	PGAS { who: T::AccountId, fee: BalanceOf<T> },
	/// Delegate to the inner extension.
	Inner(InnerVal),
}

/// Info passed from `prepare` to `post_dispatch`.
pub enum Pre<InnerPre, T: Config> {
	/// Fee withdrawn as a credit against the PGAS asset; `post_dispatch` splits this into the
	/// actual-fee portion (dropped, which reduces total issuance) and the refund portion
	/// (resolved back to `who`).
	PGAS { who: T::AccountId, credit: Credit<T::AccountId, T::Assets> },
	/// Inner extension was used.
	Inner(InnerPre),
}

impl<T: Config + Send + Sync, S: TransactionExtension<T::RuntimeCall>>
	TransactionExtension<T::RuntimeCall> for ChargePGAS<T, S>
where
	T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
	BalanceOf<T>: Send + Sync,
	<T as Config>::AssetId: Send + Sync,
	<T::RuntimeCall as Dispatchable>::RuntimeOrigin: AsSystemOriginSigner<T::AccountId> + Clone,
{
	const IDENTIFIER: &'static str = "ChargePGAS";
	type Implicit = S::Implicit;
	type Val = Val<S::Val, T>;
	type Pre = Pre<S::Pre, T>;

	fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
		self.inner.implicit()
	}

	fn metadata() -> alloc::vec::Vec<sp_runtime::traits::TransactionExtensionMetadata> {
		S::metadata()
	}

	fn weight(&self, call: &T::RuntimeCall) -> Weight {
		self.inner.weight(call)
	}

	fn validate(
		&self,
		origin: OriginFor<T>,
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
		self_implicit: S::Implicit,
		inherited_implication: &impl Implication,
		source: TransactionSource,
	) -> ValidateResult<Self::Val, T::RuntimeCall> {
		let Some(who) = origin.as_system_origin_signer().cloned() else {
			let (validity, val, origin) = self.inner.validate(
				origin,
				call,
				info,
				len,
				self_implicit,
				inherited_implication,
				source,
			)?;
			return Ok((validity, Val::Inner(val), origin));
		};

		if !T::CallFilter::contains(call) {
			let (validity, val, origin) = self.inner.validate(
				origin,
				call,
				info,
				len,
				self_implicit,
				inherited_implication,
				source,
			)?;
			return Ok((validity, Val::Inner(val), origin));
		}

		let fee =
			pallet_transaction_payment::Pallet::<T>::compute_fee(len as u32, info, Zero::zero());
		let pgas =
			<T::Assets as fungibles::Inspect<T::AccountId>>::balance(T::PGASAssetId::get(), &who);
		if pgas >= fee {
			return Ok((
				ValidTransaction { priority: 0, ..Default::default() },
				Val::PGAS { who, fee },
				origin,
			));
		}

		let (validity, val, origin) = self.inner.validate(
			origin,
			call,
			info,
			len,
			self_implicit,
			inherited_implication,
			source,
		)?;
		Ok((validity, Val::Inner(val), origin))
	}

	fn prepare(
		self,
		val: Self::Val,
		origin: &OriginFor<T>,
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		match val {
			Val::PGAS { who, fee } => {
				let credit = <T::Assets as fungibles::Balanced<T::AccountId>>::withdraw(
					T::PGASAssetId::get(),
					&who,
					fee,
					Precision::Exact,
					Preservation::Expendable,
					Fortitude::Polite,
				)
				.map_err(|_| InvalidTransaction::Payment)?;
				Ok(Pre::PGAS { who, credit })
			},
			Val::Inner(val) => Ok(Pre::Inner(self.inner.prepare(val, origin, call, info, len)?)),
		}
	}

	fn post_dispatch_details(
		pre: Self::Pre,
		info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &PostDispatchInfoOf<T::RuntimeCall>,
		len: usize,
		result: &DispatchResult,
	) -> Result<Weight, TransactionValidityError> {
		match pre {
			Pre::PGAS { who, credit } => {
				let actual_fee = pallet_transaction_payment::Pallet::<T>::compute_actual_fee(
					len as u32,
					info,
					post_info,
					Zero::zero(),
				);
				// Split: keep `actual_fee` as the consumed portion (dropped below to burn), and
				// hand the remainder back to `who`. If the resolve fails (e.g. the account was
				// reaped) the refund is merged back into the consumed portion and burned too.
				let (consumed, refund) = credit.split(actual_fee);
				if !refund.peek().is_zero() {
					if let Err(refund) = <T::Assets as fungibles::Balanced<T::AccountId>>::resolve(
						&who, refund,
					) {
						let _ = consumed.merge(refund);
					}
				}
				Ok(Weight::zero())
			},
			Pre::Inner(pre) => S::post_dispatch_details(pre, info, post_info, len, result),
		}
	}
}
