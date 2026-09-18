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

//! # Utility Extension Pallet
//!
//! Extends [`pallet_utility`] with [`Pallet::batch_multi_origin`]: an atomic batch of calls, each
//! dispatched with its own origin. The pallet's [`Config`] extends [`pallet_utility::Config`] and
//! reuses its types.
//!
//! ## Overview
//!
//! Each item of a multi-origin batch is a general transaction on its own: a call plus an inner
//! [`TransactionExtension`] pipeline ([`Config::InnerExtension`]) which authorizes the item's
//! origin and does its transaction extension bookkeeping (nonce, fee payment, etc.). The batch is a
//! general transaction authorized by the [`MultiOriginBatch`] extension, which applies every item
//! in sequence and hands the authorized origins over to the call. Items are signed over the
//! [`ItemImplication`] of their batch and cannot be applied elsewhere.
//!
//! All items are prepared before the calls are dispatched and post dispatched after them. Calls
//! relying on transient per-transaction state kept by extensions must be kept out of batches with
//! [`Config::MultiOriginCallFilter`].
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! * `batch_multi_origin` - Atomically dispatch multiple calls, each from its own origin.

#![cfg_attr(not(feature = "std"), no_std)]

mod benchmarking;
pub mod extension;
mod tests;
pub mod weights;

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode};
use frame_support::{
	dispatch::{
		extract_actual_weight, DispatchClass, DispatchInfo, GetDispatchInfo, PostDispatchInfo,
	},
	traits::{Contains, IsSubType, OriginTrait},
	BoundedVec,
};
use scale_info::TypeInfo;
use sp_runtime::traits::{BadOrigin, Dispatchable, TransactionExtension};
pub use weights::WeightInfo;

pub use extension::{
	ItemImplication, MultiOriginBatch, MultiOriginBatchMarker, ITEM_EXTENSION_VERSION,
};
pub use pallet::*;

type RuntimeCallOf<T> = <T as frame_system::Config>::RuntimeCall;
type PalletsOriginOf<T> = <T as pallet_utility::Config>::PalletsOrigin;

/// An item of a [`Pallet::batch_multi_origin`] call.
///
/// A general transaction without preamble: a call plus the inner extension pipeline which
/// authorizes it, validated with the [`ItemImplication`] of its batch.
#[derive(Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, TypeInfo, Clone, Debug)]
pub struct MultiOriginItem<Call, Extension, Origin> {
	/// The call to dispatch with the origin authorized by `extension`.
	pub call: Call,
	/// The inner transaction extension pipeline of this item.
	pub extension: Extension,
	/// The origin `extension` must authorize.
	///
	/// This is part of the call so therefore it gets included in the inherited implication used by
	/// all other signers when running their transaction extension pipelines. This ensures each
	/// call runs with the expected origin.
	pub origin: Origin,
}

impl<Call, Extension, Origin> MultiOriginItem<Call, Extension, Origin> {
	/// Create a new multi-origin batch item.
	pub fn new(call: Call, extension: Extension, origin: Origin) -> Self {
		Self { call, extension, origin }
	}
}

/// The [`MultiOriginItem`] for the given config.
pub type MultiOriginItemOf<T> =
	MultiOriginItem<RuntimeCallOf<T>, <T as Config>::InnerExtension, PalletsOriginOf<T>>;

/// The items of a [`Pallet::batch_multi_origin`] call for the given config.
pub type MultiOriginItemsOf<T> =
	BoundedVec<MultiOriginItemOf<T>, <T as Config>::MaxMultiOriginBatch>;

/// Builds the items the benchmarks dispatch.
#[cfg(feature = "runtime-benchmarks")]
pub trait BenchmarkHelper<T: Config> {
	/// An item dispatching `call`. Its pipeline is never run, so it need not authorize anyone.
	fn item(call: RuntimeCallOf<T>) -> MultiOriginItemOf<T>;
}

#[cfg(feature = "runtime-benchmarks")]
impl<T: Config> BenchmarkHelper<T> for ()
where
	<T as Config>::InnerExtension: Default,
{
	fn item(call: RuntimeCallOf<T>) -> MultiOriginItemOf<T> {
		let origin: <T as frame_system::Config>::RuntimeOrigin =
			frame_system::RawOrigin::None.into();
		MultiOriginItem::new(
			call,
			Default::default(),
			PalletsOriginOf::<T>::from(origin.into_caller()),
		)
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Configuration trait.
	#[pallet::config]
	pub trait Config:
		pallet_utility::Config
		+ frame_system::Config<
			RuntimeCall: Dispatchable<
				Info = DispatchInfo,
				PostInfo = PostDispatchInfo,
				RuntimeOrigin = <Self as frame_system::Config>::RuntimeOrigin,
			> + IsSubType<Call<Self>>
			                 + From<Call<Self>>,
			RuntimeOrigin: From<Origin>
			                   + Into<Result<Origin, <Self as frame_system::Config>::RuntimeOrigin>>,
		>
	{
		/// The overarching event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The transaction extension pipeline carried by every item of a
		/// [`Pallet::batch_multi_origin`] call.
		///
		/// It authorizes the item's origin and does the item's own bookkeeping (nonce, fee payment,
		/// etc.). It should be configured as a subset of the runtime's pipeline, but it must not
		/// include extensions which act on the transaction as a whole, such as `CheckWeight` or
		/// `WeightReclaim`, and must not include [`MultiOriginBatch`] itself (to disallow recursion
		/// of batching). It should also include [`MultiOriginBatchMarker`], after the extension
		/// which verifies the signature so that it is part of the signed payload, to keep item
		/// signatures distinct from those of regular transactions.
		///
		/// WARNING: The call is dispatched with the `PalletsOrigin` of the authorized origin, which
		/// means call filters the pipeline attaches to the origin are not carried over.
		type InnerExtension: TransactionExtension<RuntimeCallOf<Self>>;

		/// Builds the items the benchmarks dispatch.
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: BenchmarkHelper<Self>;

		/// The maximum number of items in a [`Pallet::batch_multi_origin`] call.
		///
		/// As a guideline for runtime devs, this should not be greater than 8 because there are
		/// iterations on these calls in validation and also the priority of the outer call will be
		/// the minimum of the calls inside, so the greater the max number here, the more distortion
		/// we see in the blocks and tx pool.
		#[pallet::constant]
		type MaxMultiOriginBatch: Get<u32>;

		/// The calls an item of a [`Pallet::batch_multi_origin`] call may dispatch, directly or
		/// nested in other calls.
		///
		/// Items run their inner transaction extension pipelines back to back, all `prepare`s
		/// before the calls and all `post_dispatch`es after them. Calls which rely on transient
		/// per-transaction state kept by those extensions (e.g. the Ethereum transaction calls of
		/// `pallet_revive` drawing storage deposits from the fee pot of
		/// `pallet_transaction_payment`) would act on another item's state and must be filtered out
		/// here.
		///
		/// The filter is enforced as follows:
		/// - The item's call is checked against it in `validate`, before anything runs.
		/// - Calls nested in it are checked at dispatch, through a filter attached to the item's
		///   origin.
		/// - That origin filter is lost when a call dispatches its inner call with a fresh origin,
		///   e.g. `pallet_proxy::proxy` or `pallet_sudo::sudo_as`. Calls nested that way are not
		///   filtered.
		type MultiOriginCallFilter: Contains<RuntimeCallOf<Self>>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	/// The origins this pallet can dispatch with.
	#[pallet::origin]
	#[derive(
		PartialEq, Eq, Clone, Encode, Decode, DecodeWithMemTracking, Debug, TypeInfo, MaxEncodedLen,
	)]
	pub enum Origin {
		/// A [`Pallet::batch_multi_origin`] call authorized by the [`MultiOriginBatch`]
		/// transaction extension. The origins of the items are in [`MultiOrigins`].
		MultiOriginBatch,
	}

	/// The origins of the items of the [`Pallet::batch_multi_origin`] call being applied, in order.
	///
	/// This storage item is transient and does not persist in the final state. It is set by
	/// [`MultiOriginBatch`] in `prepare`, consumed by the call at dispatch and cleared by
	/// [`MultiOriginBatch`] in `post_dispatch` if the call failed, so it never outlives the
	/// transaction which set it.
	#[pallet::storage]
	#[pallet::unbounded]
	pub type MultiOrigins<T: Config> = StorageValue<_, Vec<PalletsOriginOf<T>>, OptionQuery>;

	/// The post dispatch info of each item of the [`Pallet::batch_multi_origin`] call just
	/// applied, in order.
	///
	/// This storage item is transient and does not persist in the final state. It is set by the
	/// call when it succeeds and consumed by [`MultiOriginBatch`] in `post_dispatch` to run the
	/// items' inner pipelines with their actual weights.
	#[pallet::storage]
	#[pallet::unbounded]
	pub type MultiOriginPostInfos<T: Config> = StorageValue<_, Vec<PostDispatchInfo>, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event {
		/// A single item within a multi-origin batch has completed with no error.
		ItemCompleted,
		/// A multi-origin batch completed fully with no error.
		BatchCompleted,
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The origins of a multi-origin batch were not set by the transaction extension.
		MissingOrigins,
		/// The number of origins set for a multi-origin batch differs from its number of items.
		OriginCountMismatch,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Atomically dispatch a batch of calls, each with its own origin.
		///
		/// Each item is a general transaction on its own: a call plus an inner transaction
		/// extension pipeline ([`Config::InnerExtension`]) which authorizes the item's origin and
		/// does its bookkeeping (nonce, fee payment, etc.). Items are applied in sequence. The
		/// batch must not be empty.
		///
		/// The dispatch origin for this call must be [`Origin::MultiOriginBatch`], which only the
		/// [`MultiOriginBatch`] transaction extension provides: the call must be submitted as a
		/// general transaction with no other authorization. The extension runs every item's
		/// pipeline and hands the authorized origins over to this call.
		///
		/// Like `pallet_utility::batch_all`, the whole call fails and rolls back if any item
		/// fails. Items cannot nest another `batch_multi_origin` call and can only dispatch calls
		/// allowed by [`Config::MultiOriginCallFilter`]. This batch can never be a future
		/// transaction because an item whose nonce is not yet current invalidates the batch
		/// instead of queuing it.
		#[pallet::call_index(0)]
		#[pallet::weight({
			let (dispatch_weight, dispatch_class) = Pallet::<T>::weight_and_dispatch_class(
				items.iter().map(|item| item.call.get_dispatch_info()),
			);
			let dispatch_weight = dispatch_weight
				.saturating_add(<T as Config>::WeightInfo::batch_multi_origin(items.len() as u32));
			(dispatch_weight, dispatch_class)
		})]
		pub fn batch_multi_origin(
			origin: OriginFor<T>,
			items: MultiOriginItemsOf<T>,
		) -> DispatchResultWithPostInfo {
			Self::ensure_multi_origin_batch(origin)?;
			let origins = MultiOrigins::<T>::take().ok_or(Error::<T>::MissingOrigins)?;
			ensure!(origins.len() == items.len(), Error::<T>::OriginCountMismatch);

			let items_len = items.len();
			// Track the actual weight of each of the item calls.
			let mut weight = Weight::zero();
			let mut post_infos = Vec::with_capacity(items_len);
			for (index, (item, pallets_origin)) in items.into_iter().zip(origins).enumerate() {
				let info = item.call.get_dispatch_info();
				let mut origin: OriginFor<T> = pallets_origin.into();
				// Don't allow items to nest `batch_multi_origin` calls nor to dispatch calls the
				// runtime keeps out of multi-origin batches.
				origin.add_filter(move |c: &RuntimeCallOf<T>| {
					<T as Config>::MultiOriginCallFilter::contains(c) &&
						!matches!(c.is_sub_type(), Some(Call::batch_multi_origin { .. }))
				});
				let result = item.call.dispatch(origin);
				// Add the weight of this call.
				weight = weight.saturating_add(extract_actual_weight(&result, &info));
				let post_info = result.map_err(|mut err| {
					// Take the weight of this function itself into account.
					let base_weight = <T as Config>::WeightInfo::batch_multi_origin(
						index.saturating_add(1) as u32,
					);
					// Return the actual used weight + base_weight of this call.
					err.post_info = Some(base_weight.saturating_add(weight)).into();
					err
				})?;
				post_infos.push(post_info);
				Self::deposit_event(Event::ItemCompleted);
			}
			MultiOriginPostInfos::<T>::put(post_infos);
			Self::deposit_event(Event::BatchCompleted);
			let base_weight = <T as Config>::WeightInfo::batch_multi_origin(items_len as u32);
			Ok(Some(base_weight.saturating_add(weight)).into())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Ensure `origin` is [`Origin::MultiOriginBatch`].
		pub fn ensure_multi_origin_batch(origin: OriginFor<T>) -> Result<(), BadOrigin> {
			let origin: Result<Origin, OriginFor<T>> = origin.into();
			match origin {
				Ok(Origin::MultiOriginBatch) => Ok(()),
				_ => Err(BadOrigin),
			}
		}

		/// The items of `call` if it is a [`Pallet::batch_multi_origin`] call.
		pub fn multi_origin_items(call: &RuntimeCallOf<T>) -> Option<&MultiOriginItemsOf<T>> {
			match call.is_sub_type() {
				Some(Call::batch_multi_origin { items }) => Some(items),
				_ => None,
			}
		}

		/// The weight of an item's inner pipeline, which runs once in the dry run of
		/// `MultiOriginBatch::validate` and once again in its `prepare`.
		pub fn multi_origin_item_extension_weight(item: &MultiOriginItemOf<T>) -> Weight {
			item.extension.weight(&item.call).saturating_mul(2)
		}

		/// The dispatch info and encoded length each item pays for, given those of the whole
		/// transaction. Each transaction will pay for its own weight but there is still a small
		/// overhead associated with the outer call. This is divided equally among the calls and
		/// it's only relevant for fee payment reasons.
		pub fn multi_origin_item_infos(
			items: &MultiOriginItemsOf<T>,
			info: &DispatchInfo,
			len: usize,
		) -> Vec<(DispatchInfo, usize)> {
			let Some(count) = core::num::NonZeroU64::new(items.len() as u64) else {
				return Vec::new();
			};
			let mut infos: Vec<(DispatchInfo, usize)> = items
				.iter()
				.map(|item| {
					let mut info = item.call.get_dispatch_info();
					info.extension_weight = Self::multi_origin_item_extension_weight(item);
					(info, item.encoded_size())
				})
				.collect();

			let (accounted_weight, accounted_len) =
				infos.iter().fold((Weight::zero(), 0usize), |(weight, len), (info, item_len)| {
					(weight.saturating_add(info.total_weight()), len.saturating_add(*item_len))
				});
			let overhead_weight = info.total_weight().saturating_sub(accounted_weight);
			let overhead_len = len.saturating_sub(accounted_len);
			let share_weight = overhead_weight.div(count.get());
			let share_len = overhead_len / count.get() as usize;
			for (info, item_len) in infos.iter_mut() {
				info.extension_weight = info.extension_weight.saturating_add(share_weight);
				*item_len = item_len.saturating_add(share_len);
			}
			if let Some((first, first_len)) = infos.first_mut() {
				first.extension_weight = first.extension_weight.saturating_add(
					overhead_weight.saturating_sub(share_weight.saturating_mul(count.get())),
				);
				*first_len = first_len
					.saturating_add(overhead_len.saturating_sub(share_len * count.get() as usize));
			}
			infos
		}

		/// Get the accumulated `weight` and the dispatch class for the given dispatch infos.
		fn weight_and_dispatch_class(
			dispatch_infos: impl ExactSizeIterator<Item = DispatchInfo>,
		) -> (Weight, DispatchClass) {
			if dispatch_infos.len() == 0 {
				return (Weight::zero(), DispatchClass::Normal);
			}

			dispatch_infos.fold(
				(Weight::zero(), DispatchClass::Operational),
				|(total_weight, dispatch_class): (Weight, DispatchClass), di| {
					(
						total_weight.saturating_add(di.call_weight),
						// If not all are `Operational`, we want to use `DispatchClass::Normal`.
						if di.class == DispatchClass::Normal { di.class } else { dispatch_class },
					)
				},
			)
		}
	}
}
