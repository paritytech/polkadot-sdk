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

//! # Account Footprint Pallet
//!
//! `pallet-footprint` is a central, per-account storage quota meter. It accounts for storage as
//! weighted bytes, where a [`Footprint`] of `count` items and `size` bytes costs
//! `size + count * ItemByteWeight`. An account's allowance is the sum of a personhood-provided
//! base allowance and a purchased allowance backed by one fungible hold.
//!
//! Other pallets use [`QuotaConsideration`] as their [`Consideration`] implementation. Tickets
//! charge the account on allocation and release it on cleanup, while [`UsageByReason`] records a
//! runtime-defined attribution for user interfaces. If an allowance is reduced below existing
//! usage, the account is deliberately left over quota: only new or growing allocations fail;
//! shrinking, releasing, and cleanup always remain possible. The pallet never deletes user data.
//!
//! Runtimes without personhood can configure `ClaimOrigin = EnsureNever<TokenOf<Self>>` and
//! `BaseAllowance = ()`. Such runtimes simply provide purchased allowance.

#![cfg_attr(not(feature = "std"), no_std)]

mod benchmarking;
mod mock;
mod tests;
pub mod weights;

pub use pallet::*;
pub use weights::WeightInfo;

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use core::marker::PhantomData;
use frame_support::{
	dispatch::DispatchResult,
	pallet_prelude::*,
	traits::{
		tokens::fungible::{Inspect, MutateHold},
		Consideration, EnsureOrigin, Footprint, MaybeConsideration,
	},
	CloneNoBound, DebugNoBound, DefaultNoBound, EqNoBound, PartialEqNoBound,
};
use frame_system::pallet_prelude::{ensure_signed, OriginFor};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{Member, SaturatedConversion, Saturating},
	ArithmeticError, DispatchError,
};

/// Supplies a personhood-backed base allowance and the token used to claim it.
pub trait BaseAllowanceProvider {
	/// Identifies the granted entity, such as a personhood alias. A token can be claimed by only
	/// one account at a time.
	type Token: Parameter + Member + MaxEncodedLen;

	/// Return the current base allowance in weighted bytes, or `None` when the token is no longer
	/// valid.
	fn base_allowance(token: &Self::Token) -> Option<u64>;

	/// Create a token whose [`Self::base_allowance`] is `Some` for benchmarking.
	#[cfg(feature = "runtime-benchmarks")]
	fn create_token() -> Option<Self::Token>;
}

impl BaseAllowanceProvider for () {
	type Token = ();

	fn base_allowance(_: &Self::Token) -> Option<u64> {
		None
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn create_token() -> Option<Self::Token> {
		None
	}
}

/// Base and purchased portions of an account's allowance.
#[derive(
	CloneNoBound,
	EqNoBound,
	PartialEqNoBound,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	DebugNoBound,
	DefaultNoBound,
)]
#[scale_info(skip_type_params(Token))]
#[codec(mel_bound())]
pub struct AllowanceInfo<Token: Parameter + MaxEncodedLen> {
	/// Weighted bytes granted through a claimed base allowance.
	pub base: u64,
	/// Weighted bytes purchased and backed by the pallet's fungible hold.
	pub purchased: u64,
	/// The token that owns `base`, if any.
	pub token: Option<Token>,
}

/// A [`Consideration`] ticket that charges the account's central footprint quota.
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
#[scale_info(skip_type_params(T, R))]
#[codec(mel_bound())]
pub struct QuotaConsideration<T, R>(Footprint, PhantomData<fn() -> (T, R)>);

impl<T: Config, R: 'static + Get<T::Reason>> Consideration<T::AccountId, Footprint>
	for QuotaConsideration<T, R>
{
	fn new(who: &T::AccountId, footprint: Footprint) -> Result<Self, DispatchError> {
		Pallet::<T>::charge(who, R::get(), footprint)?;
		Ok(Self(footprint, PhantomData))
	}

	fn update(self, who: &T::AccountId, footprint: Footprint) -> Result<Self, DispatchError> {
		Pallet::<T>::adjust(who, R::get(), self.0, footprint)?;
		Ok(Self(footprint, PhantomData))
	}

	fn drop(self, who: &T::AccountId) -> DispatchResult {
		Pallet::<T>::release(who, R::get(), self.0)
	}

	fn burn(self, who: &T::AccountId) {
		Pallet::<T>::burn_usage(who, R::get(), self.0);
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(who: &T::AccountId, footprint: Footprint) {
		Pallet::<T>::grant_for_benchmarks(who, footprint);
	}
}

impl<T: Config, R: 'static + Get<T::Reason>> MaybeConsideration<T::AccountId, Footprint>
	for QuotaConsideration<T, R>
{
	fn is_none(&self) -> bool {
		self.0 == Footprint::default()
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	/// Balance type used by the configured fungible currency.
	pub type BalanceOf<T> =
		<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;
	/// Token type used by the configured base-allowance provider.
	pub type TokenOf<T> = <<T as Config>::BaseAllowance as BaseAllowanceProvider>::Token;

	#[pallet::pallet]
	/// The footprint quota pallet.
	pub struct Pallet<T>(_);

	#[pallet::config]
	/// Configuration for the footprint quota pallet.
	pub trait Config: frame_system::Config {
		/// The runtime-wide hold reason into which this pallet's [`HoldReason`] is composed.
		type RuntimeHoldReason: From<HoldReason>;

		/// Fungible currency that backs purchased allowance with a single hold.
		#[cfg(not(feature = "runtime-benchmarks"))]
		type Currency: MutateHold<Self::AccountId, Reason = Self::RuntimeHoldReason>;
		/// Fungible currency that backs purchased allowance with a single hold.
		#[cfg(feature = "runtime-benchmarks")]
		type Currency: MutateHold<Self::AccountId, Reason = Self::RuntimeHoldReason>
			+ frame_support::traits::tokens::fungible::Mutate<Self::AccountId>;

		/// Runtime-defined attribution for the pallet or feature which owns a footprint.
		type Reason: Parameter + Member + Copy + MaxEncodedLen;

		/// Trie overhead, in weighted bytes, charged for every stored item.
		#[pallet::constant]
		type ItemByteWeight: Get<u64>;

		/// Price, in the configured currency, of one purchased weighted byte.
		#[pallet::constant]
		type PricePerByte: Get<BalanceOf<Self>>;

		/// Maximum number of weighted bytes an account may purchase.
		#[pallet::constant]
		type MaxPurchased: Get<u64>;

		/// Origin that proves entitlement to a base allowance and returns its token.
		type ClaimOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = TokenOf<Self>>;

		/// Provider of personhood-backed base allowances.
		type BaseAllowance: BaseAllowanceProvider;

		/// Weight information for this pallet's dispatchables.
		type WeightInfo: WeightInfo;
	}

	/// A reason for this pallet placing a hold on purchased allowance funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Funds backing purchased storage allowance.
		PurchasedAllowance,
	}

	/// Total weighted bytes currently charged to each account.
	#[pallet::storage]
	pub type Usage<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, u64, ValueQuery>;

	/// Footprint breakdown by account and runtime-defined attribution reason.
	///
	/// Entries are removed when both fields return to zero.
	#[pallet::storage]
	pub type UsageByReason<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		Twox64Concat,
		T::Reason,
		Footprint,
		ValueQuery,
	>;

	/// Base and purchased allowance assigned to each account.
	#[pallet::storage]
	pub type Allowances<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, AllowanceInfo<TokenOf<T>>, ValueQuery>;

	/// Account currently claiming each base-allowance token.
	#[pallet::storage]
	pub type Claims<T: Config> =
		StorageMap<_, Blake2_128Concat, TokenOf<T>, T::AccountId, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	/// Events emitted by the footprint quota pallet.
	pub enum Event<T: Config> {
		/// A footprint was charged to an account.
		Charged { who: T::AccountId, reason: T::Reason, footprint: Footprint },
		/// A footprint was released from an account.
		Released { who: T::AccountId, reason: T::Reason, footprint: Footprint },
		/// A consideration ticket changed its footprint.
		Adjusted {
			who: T::AccountId,
			reason: T::Reason,
			old_footprint: Footprint,
			new_footprint: Footprint,
		},
		/// A consideration ticket was burned, so its usage remains charged forever.
		UsageBurned { who: T::AccountId, reason: T::Reason, footprint: Footprint },
		/// Purchased allowance and its corresponding hold were set.
		PurchasedSet { who: T::AccountId, bytes: u64, held: BalanceOf<T> },
		/// A base allowance was claimed onto an account.
		BaseClaimed { who: T::AccountId, bytes: u64 },
		/// A current base allowance was revalidated.
		BaseRevalidated { who: T::AccountId, bytes: u64 },
		/// A no-longer-valid base allowance was revoked.
		BaseRevoked { who: T::AccountId },
	}

	#[pallet::error]
	/// Errors returned by the footprint quota pallet.
	pub enum Error<T> {
		/// The requested purchased allowance exceeds [`Config::MaxPurchased`].
		ExceedsMaxPurchased,
		/// Lowering purchased allowance would leave live usage unsupported by base plus purchase.
		AllowanceBelowUsage,
		/// The claim token currently has no base allowance.
		NoBaseAllowance,
		/// The target account already carries a different base-allowance token.
		AccountAlreadyClaimed,
		/// Existing usage on the prior account still relies on the base allowance being moved.
		BaseInUse,
		/// The target account has no base-allowance claim to revalidate.
		NoClaim,
		/// A new or growing allocation would exceed the account's allowance.
		Exhausted,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set the total purchased allowance for the signed account.
		///
		/// The corresponding hold is set to `bytes * PricePerByte`. Lowering is only permitted
		/// when the account's existing usage still fits its base allowance plus the new purchase.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::set_purchased())]
		pub fn set_purchased(origin: OriginFor<T>, bytes: u64) -> DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(bytes <= T::MaxPurchased::get(), Error::<T>::ExceedsMaxPurchased);

			let mut allowance = Allowances::<T>::get(&who);
			if bytes < allowance.purchased {
				ensure!(
					Usage::<T>::get(&who) <= allowance.base.saturating_add(bytes),
					Error::<T>::AllowanceBelowUsage
				);
			}

			let held =
				T::PricePerByte::get().saturating_mul(bytes.saturated_into::<BalanceOf<T>>());
			let reason: T::RuntimeHoldReason = HoldReason::PurchasedAllowance.into();
			T::Currency::set_on_hold(&reason, &who, held)?;

			allowance.purchased = bytes;
			Self::put_allowance(&who, allowance);
			Self::deposit_event(Event::PurchasedSet { who, bytes, held });
			Ok(())
		}

		/// Claim the base allowance proved by `origin` onto `target`.
		///
		/// A token can be claimed by one account at a time. Moving it requires the old account's
		/// usage to fit its purchased allowance alone, so the old account is never stranded with
		/// base-dependent live usage.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::claim_base())]
		pub fn claim_base(origin: OriginFor<T>, target: T::AccountId) -> DispatchResult {
			let token = T::ClaimOrigin::ensure_origin(origin)?;
			let base =
				T::BaseAllowance::base_allowance(&token).ok_or(Error::<T>::NoBaseAllowance)?;

			let mut target_allowance = Allowances::<T>::get(&target);
			if let Some(other) = target_allowance.token.as_ref() {
				ensure!(other == &token, Error::<T>::AccountAlreadyClaimed);
			}

			if let Some(old) = Claims::<T>::get(&token) {
				if old != target {
					let mut old_allowance = Allowances::<T>::get(&old);
					ensure!(
						Usage::<T>::get(&old) <= old_allowance.purchased,
						Error::<T>::BaseInUse
					);
					old_allowance.base = 0;
					old_allowance.token = None;
					Self::put_allowance(&old, old_allowance);
				}
			}

			Claims::<T>::insert(&token, &target);
			target_allowance.base = base;
			target_allowance.token = Some(token);
			Self::put_allowance(&target, target_allowance);
			Self::deposit_event(Event::BaseClaimed { who: target, bytes: base });
			Ok(())
		}

		/// Revalidate the base allowance claimed by `target`.
		///
		/// Anyone may call this. Demotion or revocation intentionally does not fail when existing
		/// usage exceeds the newly revalidated allowance; it only prevents later growth.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::revalidate_base())]
		pub fn revalidate_base(origin: OriginFor<T>, target: T::AccountId) -> DispatchResult {
			let _ = ensure_signed(origin)?;
			let mut allowance = Allowances::<T>::get(&target);
			let token = allowance.token.clone().ok_or(Error::<T>::NoClaim)?;

			match T::BaseAllowance::base_allowance(&token) {
				Some(bytes) => {
					allowance.base = bytes;
					Self::put_allowance(&target, allowance);
					Self::deposit_event(Event::BaseRevalidated { who: target, bytes });
				},
				None => {
					allowance.base = 0;
					allowance.token = None;
					Claims::<T>::remove(&token);
					Self::put_allowance(&target, allowance);
					Self::deposit_event(Event::BaseRevoked { who: target });
				},
			}

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Convert a raw footprint into weighted bytes using saturating arithmetic.
		pub(crate) fn weighted(footprint: Footprint) -> u64 {
			footprint
				.size
				.saturating_add(footprint.count.saturating_mul(T::ItemByteWeight::get()))
		}

		/// Charge `footprint` to `who` under `reason`, failing only if growth exceeds allowance.
		pub(crate) fn charge(
			who: &T::AccountId,
			reason: T::Reason,
			footprint: Footprint,
		) -> DispatchResult {
			let new_usage = Usage::<T>::get(who)
				.checked_add(Self::weighted(footprint))
				.ok_or(ArithmeticError::Overflow)?;
			ensure!(new_usage <= Self::allowance_of(who), Error::<T>::Exhausted);

			Self::put_usage(who, new_usage);
			Self::add_reason(who, reason, footprint);
			Self::deposit_event(Event::Charged { who: who.clone(), reason, footprint });
			Ok(())
		}

		/// Apply a ticket footprint update, checking only net weighted-byte growth.
		pub(crate) fn adjust(
			who: &T::AccountId,
			reason: T::Reason,
			old_footprint: Footprint,
			new_footprint: Footprint,
		) -> DispatchResult {
			let old_weighted = Self::weighted(old_footprint);
			let new_weighted = Self::weighted(new_footprint);
			let usage = Usage::<T>::get(who);

			if new_weighted > old_weighted {
				let new_usage = usage
					.checked_add(new_weighted - old_weighted)
					.ok_or(ArithmeticError::Overflow)?;
				ensure!(new_usage <= Self::allowance_of(who), Error::<T>::Exhausted);
				Self::put_usage(who, new_usage);
			} else if old_weighted > new_weighted {
				Self::put_usage(who, usage.saturating_sub(old_weighted - new_weighted));
			}

			Self::adjust_reason(who, reason, old_footprint, new_footprint);
			if old_footprint != new_footprint {
				Self::deposit_event(Event::Adjusted {
					who: who.clone(),
					reason,
					old_footprint,
					new_footprint,
				});
			}
			Ok(())
		}

		/// Release `footprint` from `who`; cleanup is always allowed and saturates defensively.
		pub(crate) fn release(
			who: &T::AccountId,
			reason: T::Reason,
			footprint: Footprint,
		) -> DispatchResult {
			let new_usage = Usage::<T>::get(who).saturating_sub(Self::weighted(footprint));
			Self::put_usage(who, new_usage);
			Self::subtract_reason(who, reason, footprint);
			Self::deposit_event(Event::Released { who: who.clone(), reason, footprint });
			Ok(())
		}

		/// Record that a consideration ticket was sacrificed without releasing its charged usage.
		///
		/// [`Consideration::burn`] represents loss to the ticket owner, so the quota remains
		/// consumed forever rather than silently returning capacity to the account.
		pub(crate) fn burn_usage(who: &T::AccountId, reason: T::Reason, footprint: Footprint) {
			Self::deposit_event(Event::UsageBurned { who: who.clone(), reason, footprint });
		}

		/// Add purchased capacity for a benchmark without taking a fungible hold.
		#[cfg(feature = "runtime-benchmarks")]
		pub(crate) fn grant_for_benchmarks(who: &T::AccountId, footprint: Footprint) {
			let mut allowance = Allowances::<T>::get(who);
			allowance.purchased = allowance.purchased.saturating_add(Self::weighted(footprint));
			Self::put_allowance(who, allowance);
		}

		fn allowance_of(who: &T::AccountId) -> u64 {
			let allowance = Allowances::<T>::get(who);
			allowance.base.saturating_add(allowance.purchased)
		}

		fn put_usage(who: &T::AccountId, usage: u64) {
			if usage == 0 {
				Usage::<T>::remove(who);
			} else {
				Usage::<T>::insert(who, usage);
			}
		}

		fn put_allowance(who: &T::AccountId, allowance: AllowanceInfo<TokenOf<T>>) {
			if allowance.base == 0 && allowance.purchased == 0 && allowance.token.is_none() {
				Allowances::<T>::remove(who);
			} else {
				Allowances::<T>::insert(who, allowance);
			}
		}

		fn add_reason(who: &T::AccountId, reason: T::Reason, footprint: Footprint) {
			if footprint == Footprint::default() {
				return;
			}

			UsageByReason::<T>::mutate_exists(who, reason, |entry| {
				let value = entry.get_or_insert(Footprint::default());
				value.count = value.count.saturating_add(footprint.count);
				value.size = value.size.saturating_add(footprint.size);
			});
		}

		fn adjust_reason(
			who: &T::AccountId,
			reason: T::Reason,
			old_footprint: Footprint,
			new_footprint: Footprint,
		) {
			if old_footprint == new_footprint {
				return;
			}

			UsageByReason::<T>::mutate_exists(who, reason, |entry| {
				let mut value = match entry.take() {
					Some(value) => value,
					None => Footprint::default(),
				};
				value.count = value
					.count
					.saturating_sub(old_footprint.count)
					.saturating_add(new_footprint.count);
				value.size = value
					.size
					.saturating_sub(old_footprint.size)
					.saturating_add(new_footprint.size);
				if value == Footprint::default() {
					*entry = None;
				} else {
					*entry = Some(value);
				}
			});
		}

		fn subtract_reason(who: &T::AccountId, reason: T::Reason, footprint: Footprint) {
			UsageByReason::<T>::mutate_exists(who, reason, |entry| {
				let Some(value) = entry.as_mut() else { return };
				value.count = value.count.saturating_sub(footprint.count);
				value.size = value.size.saturating_sub(footprint.size);
				let remove = *value == Footprint::default();
				if remove {
					*entry = None;
				}
			});
		}
	}
}
