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
		BuildGenesisConfig, Contains, Get,
	},
	weights::Weight,
	PalletId,
};
use frame_system::pallet_prelude::OriginFor;
use pallet_transaction_payment::ChargeTransactionPayment;
use scale_info::{StaticTypeInfo, TypeInfo};
use sp_runtime::{
	traits::{
		AccountIdConversion, AsSystemOriginSigner, DispatchInfoOf, Dispatchable, Implication,
		PostDispatchInfoOf, TransactionExtension, ValidateResult, Zero,
	},
	transaction_validity::{InvalidTransaction, TransactionValidityError, ValidTransaction},
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

/// Balance type alias, sourced from `pallet-transaction-payment`.
pub type BalanceOf<T> = <<T as pallet_transaction_payment::Config>::OnChargeTransaction as
	pallet_transaction_payment::OnChargeTransaction<T>>::Balance;

/// Internal pallet id used to derive the sovereign account that owns the PGAS asset created at
/// genesis. Kept out of [`Config`] since the identifier only has to be unique within a runtime
/// and runtimes never need to customise it.
const PALLET_ID: PalletId = PalletId(*b"py/pgasa");

/// Trait used by runtimes to mint PGAS to the benchmark caller.
#[cfg(feature = "runtime-benchmarks")]
pub trait BenchmarkHelperTrait<AccountId, AssetId, Balance> {
	/// Mint `amount` of PGAS to `who`.
	fn mint_pgas(who: &AccountId, asset_id: AssetId, amount: Balance);
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_transaction_payment::Config {
		/// The asset id type used by the PGAS asset.
		type AssetId: AssetId;

		/// Access to the PGAS asset. `Balanced` exposes the imbalance API used to withdraw the
		/// reserved fee into a [`Credit`] and refund the unused portion in `post_dispatch`.
		/// `Create` is used by the pallet's `GenesisConfig` to provision the PGAS asset.
		type Assets: fungibles::Balanced<
				Self::AccountId,
				AssetId = <Self as Config>::AssetId,
				Balance = BalanceOf<Self>,
			> + fungibles::Create<Self::AccountId>;

		/// The PGAS asset id.
		type PGASAssetId: frame_support::traits::Get<<Self as Config>::AssetId>;

		/// Filter deciding which calls are eligible to be paid with PGAS. Calls that fail the
		/// filter fall through to the inner fee extension unconditionally, even if the caller
		/// holds PGAS.
		type CallFilter: Contains<<Self as frame_system::Config>::RuntimeCall>;

		/// Weight information for the extension.
		type WeightInfo: WeightInfo;

		/// Helper used by the extension benchmarks to endow the caller with enough PGAS to cover
		/// the fee. The PGAS asset itself is expected to be created by the pallet's genesis
		/// config or by the runtime's chain spec.
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: crate::BenchmarkHelperTrait<
			Self::AccountId,
			<Self as Config>::AssetId,
			BalanceOf<Self>,
		>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Genesis configuration provisions the PGAS asset so that the extension's `Create` bound is
	/// satisfied at chain start. The asset is owned by a sovereign account derived from
	/// [`PALLET_ID`] so that no user key controls it; setting `min_balance` to zero skips asset
	/// creation.
	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// Minimum balance (existential deposit) of the PGAS asset. When zero, no asset is
		/// created at genesis.
		pub min_balance: BalanceOf<T>,
		#[serde(skip)]
		pub _phantom: core::marker::PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			if self.min_balance.is_zero() {
				return;
			}
			<T::Assets as fungibles::Create<T::AccountId>>::create(
				T::PGASAssetId::get(),
				PALLET_ID.into_account_truncating(),
				true,
				self.min_balance,
			)
			.expect("PGAS asset creation failed at genesis");
		}
	}
}

/// Transaction extension that charges transaction fees in PGAS when the caller holds enough and
/// the dispatched call passes [`Config::CallFilter`]. Otherwise it delegates to the wrapped
/// extension `S`.
///
/// The wrapper is transparent from the outside: it encodes as `S` (the `PhantomData` does not
/// contribute bytes) and its [`TypeInfo`] / [`TransactionExtension::metadata`] forward to `S`.
/// Clients (wallets, block explorers) therefore see only the inner extension.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq)]
pub struct ChargePGAS<T, S> {
	inner: S,
	_phantom: core::marker::PhantomData<T>,
}

impl<T, S: StaticTypeInfo> TypeInfo for ChargePGAS<T, S> {
	type Identity = S;
	fn type_info() -> scale_info::Type {
		S::type_info()
	}
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
	/// (resolved back to `who`). `refund` carries the weight difference between what
	/// [`ChargePGAS::weight`] reserved and the full PGAS path (`charge_pgas`).
	PGAS { who: T::AccountId, credit: Credit<T::AccountId, T::Assets>, refund: Weight },
	/// Inner extension was used. `extra_refund` is the weight to return on top of whatever the
	/// inner extension refunds: zero on a filter miss (reserve matches actual cost), and the
	/// slack between the reserved `max(charge_pgas, inner_weight + charge_pgas_skip)` and the
	/// actual skip-path cost (`inner_weight + charge_pgas_skip`) on a filter-pass skip.
	Inner { inner: InnerPre, extra_refund: Weight },
}

impl<T: Config + Send + Sync, S: TransactionExtension<T::RuntimeCall>>
	TransactionExtension<T::RuntimeCall> for ChargePGAS<T, S>
where
	T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
	BalanceOf<T>: Send + Sync,
	<T as Config>::AssetId: Send + Sync,
	<T::RuntimeCall as Dispatchable>::RuntimeOrigin: AsSystemOriginSigner<T::AccountId> + Clone,
{
	// Fully transparent to decoders: forward both the identifier and the metadata so the
	// extension is indistinguishable from `S` on-chain and in wallets.
	const IDENTIFIER: &'static str = S::IDENTIFIER;
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
		let inner = self.inner.weight(call);
		let skip = <T as Config>::WeightInfo::charge_pgas_skip();
		if T::CallFilter::contains(call) {
			<T as Config>::WeightInfo::charge_pgas().max(inner.saturating_add(skip))
		} else {
			inner.saturating_add(skip)
		}
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

		let pgas = <T::Assets as fungibles::Inspect<T::AccountId>>::reducible_balance(
			T::PGASAssetId::get(),
			&who,
			Preservation::Preserve,
			Fortitude::Polite,
		);
		if pgas >= fee {
			let priority =
				ChargeTransactionPayment::<T>::get_priority(info, len, Zero::zero(), fee);
			return Ok((
				ValidTransaction { priority, ..Default::default() },
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
		let inner_weight = self.inner.weight(call);
		let charge_pgas = <T as Config>::WeightInfo::charge_pgas();
		let charge_pgas_skip = <T as Config>::WeightInfo::charge_pgas_skip();
		match val {
			Val::PGAS { who, fee } => {
				let credit = <T::Assets as fungibles::Balanced<T::AccountId>>::withdraw(
					T::PGASAssetId::get(),
					&who,
					fee,
					Precision::Exact,
					Preservation::Preserve,
					Fortitude::Polite,
				)
				.map_err(|_| InvalidTransaction::Payment)?;
				// Reserved `max(charge_pgas, inner + charge_pgas_skip)`, spending `charge_pgas`:
				// refund the slack when `inner + charge_pgas_skip` was the larger summand.
				let refund =
					inner_weight.saturating_add(charge_pgas_skip).saturating_sub(charge_pgas);
				Ok(Pre::PGAS { who, credit, refund })
			},
			Val::Inner(val) => {
				// Filter-pass skip: reserved `max(charge_pgas, inner + charge_pgas_skip)`, actual
				// cost is `inner_actual + charge_pgas_skip`. Inner already refunds `inner -
				// inner_actual`, so the extra we owe on top is `max(0, charge_pgas - inner -
				// charge_pgas_skip)`.
				// Filter-miss: reserved `inner + charge_pgas_skip`; no extra refund from us.
				let extra_refund = if T::CallFilter::contains(call) {
					charge_pgas.saturating_sub(inner_weight.saturating_add(charge_pgas_skip))
				} else {
					Weight::zero()
				};
				let inner = self.inner.prepare(val, origin, call, info, len)?;
				Ok(Pre::Inner { inner, extra_refund })
			},
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
			Pre::PGAS { who, credit, refund } => {
				let actual_fee = pallet_transaction_payment::Pallet::<T>::compute_actual_fee(
					len as u32,
					info,
					post_info,
					Zero::zero(),
				);
				// Split: keep `actual_fee` as the consumed portion (dropped below to burn), and
				// hand the remainder back to `who`. If the resolve fails (e.g. the account was
				// reaped) the refund is merged back into the consumed portion and burned too.
				let (consumed, fee_refund) = credit.split(actual_fee);
				if !fee_refund.peek().is_zero() {
					if let Err(fee_refund) =
						<T::Assets as fungibles::Balanced<T::AccountId>>::resolve(&who, fee_refund)
					{
						let _ = consumed.merge(fee_refund);
					}
				}
				Ok(refund)
			},
			Pre::Inner { inner, extra_refund } => {
				let inner_refund = S::post_dispatch_details(inner, info, post_info, len, result)?;
				Ok(inner_refund.saturating_add(extra_refund))
			},
		}
	}
}
