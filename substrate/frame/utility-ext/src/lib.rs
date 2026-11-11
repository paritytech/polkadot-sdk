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
//! Batches of calls, each dispatched with its own origin, in the spirit of `pallet_utility`:
//! [`Pallet::batch_multi_origin`] and [`Pallet::force_batch_multi_origin`].
//!
//! ## Overview
//!
//! A multi-origin batch is a regular signed transaction of its submitter whose call carries the
//! submitter's nonce and a list of items. Each item is a general transaction on its own: a call
//! plus an inner [`TransactionExtension`] pipeline ([`Config::InnerExtension`]) which authorizes
//! the item's origin and does its transaction extension bookkeeping (nonce, mortality, etc.). The
//! call applies the items in sequence as every item runs its pipeline's `validate` and `prepare`,
//! then it dispatches the call itself, then runs its pipeline's `post_dispatch` before the next
//! item starts, as if it were a transaction of its own included right after the previous item (but
//! items cannot be pulled out of a batch and ran as standalong transactions). Items cannot nest
//! multi-origin batch calls and can only dispatch calls allowed by
//! [`Config::MultiOriginCallFilter`].
//!
//! An item rejected by its pipeline is invalid and is not applied at all, like an invalid
//! transaction. An item whose call fails keeps its pipeline's effects (nonce) while the call's
//! effects are reverted, like a failed transaction. An item whose pipeline fails in `post_dispatch`
//! is rolled back after its call ran. `batch_multi_origin` stops at the first item which is invalid
//! or fails and keeps the items before it. `force_batch_multi_origin` tries to apply every item
//! regardless of some failures. The outcome is reported in events.
//!
//! The submitter pays for the whole batch up front like for any transaction. The fee covers the
//! overhead of processing the batch itself, the items' pipelines and calls and the transaction's
//! length, through the runtime's fee payment extension. Each item then pays for itself through its
//! inner pipeline like a standalone transaction. An individual signed item is charged by the chosen
//! fee payment extension, an item with another origin is charged as the runtime charges such
//! transactions (the extension that authorized the origin should also make sure it is rate limited
//! to prevent spam). The pallet's [`MultiOriginBatch`] extension wraps the fee payment extension so
//! that it refunds the submitter the weight and bytes of every item whose pipeline ran to
//! completion, whether its call succeeded or failed. The submitter keeps paying for the other
//! items' bytes and for the weight they used because if an item failed in the extension logic,
//! either during validation, preparation or post dispatch, the effects are rolled back along with
//! any fee payment or rate limited done, but the logic is still part of the block and consumes
//! blockspace, so someone must pay for it (the submitter). While the items run, the submitter's
//! withdrawn fee is set aside ([`Config::FeeCredit`]) so that the items' fee payment extensions
//! settle with their own.
//!
//! Items are signed over the [`ItemImplication`] of their batch, which binds them to the submitter,
//! the submitter's nonce, the batch mode, their position and the other items, so an item cannot be
//! applied in another batch or taken out of the batch and ran as a standalone transaction. The call
//! checks that `nonce + 1` is the submitter's current nonce, i.e. that `nonce` is the one
//! `CheckNonce` just bumped, so the runtime's pipeline must contain `CheckNonce`. Like any
//! transaction pipeline, the item pipeline must bring its own replay protection, e.g. `CheckNonce`.
//!
//! Items are not validated when the batch enters the transaction pool, only the submitter's
//! transaction is. A batch with invalid items is therefore included and paid for by the submitter,
//! while an item signer can invalidate their item at no cost by using their nonce before the batch
//! is included. Submitters should only batch items of parties they trust.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod benchmarking;
pub mod extension;
mod tests;
pub mod weights;

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{
	dispatch::{
		DispatchClass, DispatchInfo, GetDispatchInfo, PostDispatchInfo, WithPostDispatchInfo,
	},
	storage::{transactional::with_transaction_opaque_err, TransactionOutcome},
	traits::{Contains, Imbalance, IsSubType, OriginTrait, SuppressedDrop},
	BoundedVec,
};
use pallet_transaction_payment::{OnChargeTransaction, TxCreditHold};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{
		AsTransactionAuthorizedOrigin, CheckedAdd, Dispatchable,
		ExtensionPostDispatchWeightHandler, Hash, One, TransactionExtension, TxBaseImplication,
	},
	transaction_validity::{InvalidTransaction, TransactionSource, TransactionValidityError},
	DispatchError, Weight,
};
pub use weights::WeightInfo;

pub use extension::MultiOriginBatch;
pub use pallet::*;

type RuntimeCallOf<T> = <T as frame_system::Config>::RuntimeCall;
type RuntimeOriginOf<T> = <T as frame_system::Config>::RuntimeOrigin;
type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
type NonceOf<T> = <T as frame_system::Config>::Nonce;
type HashOf<T> = <T as frame_system::Config>::Hash;
type PalletsOriginOf<T> = <RuntimeOriginOf<T> as OriginTrait>::PalletsOrigin;

/// How a multi-origin batch handles an item which is invalid or whose call fails.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Clone,
	Copy,
	PartialEq,
	Eq,
	Debug,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum BatchMode {
	/// [`Pallet::batch_multi_origin`].
	StopAtFailure,
	/// [`Pallet::force_batch_multi_origin`].
	All,
}

/// Why an item of a multi-origin batch was not applied successfully.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub enum ItemError {
	/// The item's pipeline rejected it.
	Invalid(TransactionValidityError),
	/// The item's call failed.
	Failed(DispatchError),
}

/// The weight and bytes of the items of a batch whose pipeline ran to completion.
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct Share {
	pub weight: Weight,
	pub len: u32,
}

/// An item of a multi-origin batch: a general transaction without preamble, validated with the
/// [`ItemImplication`] of its batch.
#[derive(Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, TypeInfo, Clone, Debug)]
pub struct MultiOriginItem<Call, Extension, Origin> {
	/// The call to dispatch.
	pub call: Call,
	/// The pipeline authorizing `origin`.
	pub extension: Extension,
	/// The origin `extension` must authorize.
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

/// The items of a multi-origin batch for the given config.
pub type MultiOriginItemsOf<T> =
	BoundedVec<MultiOriginItemOf<T>, <T as Config>::MaxMultiOriginBatch>;

/// Marker that leads the encoding of [`ItemImplication`], so that it does not encode like the
/// implication of a regular transaction, `(extension_version, call)`, for any realistic extension
/// version and pallet index.
pub const ITEM_IMPLICATION_PREFIX: &[u8; 8] = b"_mbatch_";

/// The base implication of an item, in place of the usual `(extension_version, call)`, binding
/// it to its batch. Encodes as [`ITEM_IMPLICATION_PREFIX`] followed by its fields in order.
///
/// The authorizing extension at the head of the item pipeline sees it in `validate` followed by
/// the explicit and the implicit data of the extensions after it.
pub struct ItemImplication<'a, AccountId, Nonce, Hash> {
	/// The mode of the batch.
	pub mode: BatchMode,
	/// The account submitting the batch.
	pub submitter: &'a AccountId,
	/// The nonce of the submitter's batch transaction.
	pub nonce: Nonce,
	/// The position of the item in the batch.
	pub index: u32,
	/// [`Pallet::items_hash`](crate::Pallet::items_hash) of the batch.
	pub items_hash: &'a Hash,
}

impl<'a, AccountId: Encode, Nonce: Encode, Hash: Encode> Encode
	for ItemImplication<'a, AccountId, Nonce, Hash>
{
	fn encode_to<O: codec::Output + ?Sized>(&self, dest: &mut O) {
		dest.write(ITEM_IMPLICATION_PREFIX);
		self.mode.encode_to(dest);
		self.submitter.encode_to(dest);
		self.nonce.encode_to(dest);
		self.index.encode_to(dest);
		self.items_hash.encode_to(dest);
	}
}

/// The transient credit in which the fee payment extension keeps the transaction's withdrawn fee.
pub trait FeeCredit {
	type Credit;
	/// Take the credit of the transaction, if any.
	fn take() -> Option<Self::Credit>;
	/// Put it back.
	fn restore(credit: Self::Credit);
}

/// For fee payment extensions keeping no such credit.
impl FeeCredit for () {
	type Credit = ();
	fn take() -> Option<()> {
		None
	}
	fn restore(_: ()) {}
}

type OnChargeOf<T> = <T as pallet_transaction_payment::Config>::OnChargeTransaction;
type TxCreditOf<T> = <<OnChargeOf<T> as TxCreditHold<T>>::Credit as SuppressedDrop>::Inner;
type TxBalanceOf<T> = <OnChargeOf<T> as OnChargeTransaction<T>>::Balance;

impl<T: pallet_transaction_payment::Config> FeeCredit for pallet_transaction_payment::Pallet<T>
where
	TxCreditOf<T>: Imbalance<TxBalanceOf<T>>,
{
	type Credit = TxCreditOf<T>;
	fn take() -> Option<Self::Credit> {
		Self::withdraw_txfee(Self::remaining_txfee::<TxBalanceOf<T>>())
	}
	fn restore(credit: Self::Credit) {
		Self::deposit_txfee(credit)
	}
}

/// Builds the items the benchmarks dispatch, see the [`benchmarking`] module docs.
#[cfg(feature = "runtime-benchmarks")]
pub trait BenchmarkHelper<T: Config> {
	/// Set up an origin whose item pipeline accepts, and which can pay for, up to
	/// `MaxMultiOriginBatch` `frame_system::remark` items of any length with the nonces 0, 1,
	/// ..., and return it.
	fn setup() -> PalletsOriginOf<T>;

	/// An item of that origin dispatching `call` with `nonce`, authorized over `implication`, the
	/// encoded [`ItemImplication`] of the item.
	fn item(
		call: RuntimeCallOf<T>,
		nonce: NonceOf<T>,
		implication: Vec<u8>,
	) -> MultiOriginItemOf<T>;
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
		frame_system::Config<
		RuntimeCall: Dispatchable<
			Info = DispatchInfo,
			PostInfo = PostDispatchInfo,
			RuntimeOrigin = <Self as frame_system::Config>::RuntimeOrigin,
		> + IsSubType<Call<Self>>,
	>
	{
		/// The transaction extension pipeline carried by every item of a multi-origin batch.
		///
		/// The runtime's pipeline without the extensions acting on the transaction as a whole
		/// (`CheckWeight`, `WeightReclaim`), the fee payment extension included. As the
		/// [`ItemImplication`] binds items to their batch but not to the chain, it is recommended
		/// to include `CheckGenesis`, `CheckSpecVersion`, `CheckTxVersion` and `CheckEra`. The
		/// extensions verifying signatures should come first, see [`ItemImplication`].
		///
		/// Items are validated with [`TransactionSource::External`] and their encoded length. The
		/// validity their pipeline returns is dropped: `provides` tags do not deduplicate items,
		/// so authorized calls must check their state in `authorize`.
		///
		/// The inner extension is part of the pallet's metadata, which, with the
		/// [`ItemImplication`] encoding, is what wallets need to build items.
		type InnerExtension: TransactionExtension<RuntimeCallOf<Self>> + TypeInfo;

		/// The [`FeeCredit`] of the fee payment extension, e.g.
		/// `pallet_transaction_payment::Pallet`.
		type FeeCredit: FeeCredit;

		/// Builds the items the benchmarks dispatch.
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: BenchmarkHelper<Self>;

		/// The maximum number of items in a multi-origin batch.
		#[pallet::constant]
		type MaxMultiOriginBatch: Get<u32>;

		/// The calls an item of a multi-origin batch may dispatch.
		///
		/// Calls dispatched with a fresh origin generated within another call, like with
		/// `pallet_proxy::proxy` or `pallet_sudo::sudo_as`, escape the filter put on the item's
		/// origin for nested calls.
		type MultiOriginCallFilter: Contains<RuntimeCallOf<Self>>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	/// The [`Share`] of the last batch, consumed by [`MultiOriginBatch`] in `post_dispatch` when
	/// the batch is the transaction's call. A batch dispatched any other way leaves it behind
	/// until the next batch transaction clears it in `prepare`. This is transient storage and
	/// doesn't make it into the proof size.
	#[pallet::storage]
	#[pallet::whitelist_storage]
	pub type RefundableShare<T: Config> = StorageValue<_, Share, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event {
		/// The item at `index` was applied successfully.
		ItemCompleted { index: u32 },
		/// The item at `index` of a `force_batch_multi_origin` call was not applied successfully.
		ItemFailed { index: u32, error: ItemError },
		/// A `batch_multi_origin` call stopped at the item at `index`.
		BatchInterrupted { index: u32, error: ItemError },
		/// Every item was applied successfully.
		BatchCompleted,
		/// Every item of a `force_batch_multi_origin` call was applied, some unsuccessfully.
		BatchCompletedWithErrors,
	}

	#[pallet::error]
	pub enum Error<T> {
		/// `nonce + 1` is not the submitter's nonce.
		BadNonce,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Dispatch `items`, stopping at the first which is invalid or fails, like
		/// `pallet_utility::batch`. See the crate docs.
		#[pallet::call_index(0)]
		#[pallet::weight((Pallet::<T>::batch_weight(items), DispatchClass::Normal))]
		pub fn batch_multi_origin(
			origin: OriginFor<T>,
			items: MultiOriginItemsOf<T>,
			nonce: NonceOf<T>,
		) -> DispatchResultWithPostInfo {
			Self::do_batch(origin, items, nonce, BatchMode::StopAtFailure)
		}

		/// Like [`Pallet::batch_multi_origin`], but going on with the next item when an item is
		/// invalid or its call fails, like `pallet_utility::force_batch`.
		#[pallet::call_index(1)]
		#[pallet::weight((Pallet::<T>::batch_weight(items), DispatchClass::Normal))]
		pub fn force_batch_multi_origin(
			origin: OriginFor<T>,
			items: MultiOriginItemsOf<T>,
			nonce: NonceOf<T>,
		) -> DispatchResultWithPostInfo {
			Self::do_batch(origin, items, nonce, BatchMode::All)
		}
	}

	impl<T: Config> Pallet<T> {
		/// Whether `call` is a multi-origin batch call.
		pub(crate) fn is_batch(call: &RuntimeCallOf<T>) -> bool {
			<RuntimeCallOf<T> as IsSubType<Call<T>>>::is_sub_type(call).is_some()
		}

		/// Whether `call` may be dispatched by an item, directly or nested.
		fn item_call_allowed(call: &RuntimeCallOf<T>) -> bool {
			<T as Config>::MultiOriginCallFilter::contains(call) && !Self::is_batch(call)
		}

		/// The hash of the SCALE encoded list of the `(call, origin)` pairs of `items`.
		pub fn items_hash<'a>(
			items: impl IntoIterator<Item = (&'a RuntimeCallOf<T>, &'a PalletsOriginOf<T>)>,
		) -> HashOf<T> {
			<T as frame_system::Config>::Hashing::hash_of(&items.into_iter().collect::<Vec<_>>())
		}

		/// The weight of the pallet's own work for a batch of `count` items encoding to `bytes`.
		pub fn overhead_weight(count: u32, bytes: u32) -> Weight {
			<T as Config>::WeightInfo::batch_multi_origin(count, bytes)
		}

		/// The declared weight of a batch of `items`: the overhead and the items' pipelines and
		/// calls.
		pub fn batch_weight(items: &MultiOriginItemsOf<T>) -> Weight {
			let overhead = Self::overhead_weight(items.len() as u32, items.encoded_size() as u32);
			items.iter().fold(overhead, |weight, item| {
				weight
					.saturating_add(item.extension.weight(&item.call))
					.saturating_add(item.call.get_dispatch_info().call_weight)
			})
		}

		fn do_batch(
			origin: OriginFor<T>,
			items: MultiOriginItemsOf<T>,
			nonce: NonceOf<T>,
			mode: BatchMode,
		) -> DispatchResultWithPostInfo {
			let overhead = Self::overhead_weight(items.len() as u32, items.encoded_size() as u32);
			let submitter = ensure_signed(origin).map_err(|e| e.with_weight(overhead))?;
			if nonce.checked_add(&One::one()) !=
				Some(frame_system::Pallet::<T>::account_nonce(&submitter))
			{
				return Err(Error::<T>::BadNonce.with_weight(overhead));
			}
			let items_hash = Self::items_hash(items.iter().map(|item| (&item.call, &item.origin)));

			let submitter_fee = <T as Config>::FeeCredit::take();
			let mut weight = Weight::zero();
			let mut refundable = Share::default();
			let mut has_error = false;
			for (index, item) in items.into_iter().enumerate() {
				let index = index as u32;
				let implication = ItemImplication {
					mode,
					submitter: &submitter,
					nonce,
					index,
					items_hash: &items_hash,
				};
				let len = item.encoded_size();
				let (item_weight, outcome) = Self::apply_item(item, len, &implication);
				weight = weight.saturating_add(item_weight);
				if !matches!(outcome, Err(ItemError::Invalid(_))) {
					refundable.weight = refundable.weight.saturating_add(item_weight);
					refundable.len = refundable.len.saturating_add(len as u32);
				}
				let Err(error) = outcome else {
					Self::deposit_event(Event::ItemCompleted { index });
					continue;
				};
				has_error = true;
				match mode {
					BatchMode::StopAtFailure => {
						Self::deposit_event(Event::BatchInterrupted { index, error });
						break;
					},
					BatchMode::All => Self::deposit_event(Event::ItemFailed { index, error }),
				}
			}
			if let Some(credit) = submitter_fee {
				<T as Config>::FeeCredit::restore(credit);
			}
			if !has_error {
				Self::deposit_event(Event::BatchCompleted);
			} else if mode == BatchMode::All {
				Self::deposit_event(Event::BatchCompletedWithErrors);
			}
			if refundable != Share::default() {
				RefundableShare::<T>::put(refundable);
			}
			Ok(Some(overhead.saturating_add(weight)).into())
		}

		/// Apply an item of `len` bytes in a storage transaction rolled back if the item is
		/// invalid. Returns the weight the item used and its outcome.
		fn apply_item(
			item: MultiOriginItemOf<T>,
			len: usize,
			implication: &ItemImplication<AccountIdOf<T>, NonceOf<T>, HashOf<T>>,
		) -> (Weight, Result<(), ItemError>) {
			let mut info = item.call.get_dispatch_info();
			info.extension_weight = item.extension.weight(&item.call);

			let result = with_transaction_opaque_err(|| {
				match Self::run_item(item, len, implication, &info) {
					Err(err) => TransactionOutcome::Rollback(Err(err)),
					Ok(res) => TransactionOutcome::Commit(Ok(res)),
				}
			})
			.unwrap_or(Err((info.extension_weight, InvalidTransaction::ExhaustsResources.into())));

			match result {
				Ok((weight, result)) => (weight, result.map_err(ItemError::Failed)),
				Err((weight, err)) => (weight, Err(ItemError::Invalid(err))),
			}
		}

		/// Run an item through its pipeline and dispatch its call. Returns the actual weight of
		/// the item and its result, or the weight it used and why it is invalid.
		fn run_item(
			item: MultiOriginItemOf<T>,
			len: usize,
			implication: &ItemImplication<AccountIdOf<T>, NonceOf<T>, HashOf<T>>,
			info: &DispatchInfo,
		) -> Result<(Weight, Result<(), DispatchError>), (Weight, TransactionValidityError)> {
			let MultiOriginItem { call, extension, origin: expected } = item;
			let before_dispatch = |err: TransactionValidityError| (info.extension_weight, err);
			if !Self::item_call_allowed(&call) {
				return Err(before_dispatch(InvalidTransaction::Call.into()));
			}
			let implication = TxBaseImplication(implication);
			let (_, val, origin) = extension
				.validate(
					frame_system::RawOrigin::None.into(),
					&call,
					info,
					len,
					extension.implicit().map_err(before_dispatch)?,
					&implication,
					TransactionSource::External,
				)
				.map_err(before_dispatch)?;
			if !origin.is_transaction_authorized() {
				return Err(before_dispatch(InvalidTransaction::UnknownOrigin.into()));
			}
			if origin.caller() != &expected {
				return Err(before_dispatch(InvalidTransaction::BadSigner.into()));
			}
			let pre = extension.prepare(val, &origin, &call, info, len).map_err(before_dispatch)?;

			let mut origin: RuntimeOriginOf<T> = origin;
			origin.add_filter(Self::item_call_allowed);
			let mut res = call.dispatch(origin);
			let pd_res = res.map(|_| ()).map_err(|e| e.error);
			let post_info = match &mut res {
				Ok(info) => info,
				Err(err) => &mut err.post_info,
			};
			post_info.set_extension_weight(info);
			<T as Config>::InnerExtension::post_dispatch(pre, info, post_info, len, &pd_res)
				.map_err(|err| (info.total_weight(), err))?;
			Ok((post_info.calc_actual_weight(info), pd_res))
		}
	}
}
