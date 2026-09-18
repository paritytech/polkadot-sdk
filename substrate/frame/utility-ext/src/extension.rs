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

//! Transaction extensions for [`Pallet::batch_multi_origin`](crate::Pallet::batch_multi_origin).
//!
//! - [`MultiOriginBatch`] goes in the runtime's pipeline and authorizes general
//!   `batch_multi_origin` transactions by applying every item.
//! - [`MultiOriginBatchMarker`] goes in the items' pipeline ([`Config::InnerExtension`]) and keeps
//!   item signatures distinct from those of regular transactions.
//! - [`ItemImplication`] is included in the implication and therefore in the signing payload.

use crate::{
	Config, MultiOriginItemsOf, MultiOriginPostInfos, MultiOrigins, Origin, Pallet, PalletsOriginOf,
};
use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode};
use core::{fmt, marker::PhantomData};
use frame_support::{
	dispatch::{DispatchInfo, PostDispatchInfo},
	pallet_prelude::TransactionSource,
	storage::{with_transaction, TransactionOutcome},
	traits::{Contains, IsType, OriginTrait},
	CloneNoBound, DefaultNoBound, EqNoBound, PartialEqNoBound,
};
use scale_info::TypeInfo;
use sp_runtime::{
	generic::ExtensionVersion,
	impl_tx_ext_default,
	traits::{
		AsTransactionAuthorizedOrigin, ExtensionPostDispatchWeightHandler, Implication,
		PostDispatchInfoOf, TransactionExtension, TxBaseImplication, ValidateResult,
	},
	transaction_validity::{InvalidTransaction, TransactionValidityError, ValidTransaction},
	DispatchError, DispatchResult, Weight,
};

type RuntimeCallOf<T> = <T as frame_system::Config>::RuntimeCall;
type RuntimeOriginOf<T> = <T as frame_system::Config>::RuntimeOrigin;
type InnerValOf<T> = <<T as Config>::InnerExtension as TransactionExtension<RuntimeCallOf<T>>>::Val;
type InnerPreOf<T> = <<T as Config>::InnerExtension as TransactionExtension<RuntimeCallOf<T>>>::Pre;

/// The inner `Pre`, dispatch info and encoded length of an item.
type ItemPreOf<T> = (InnerPreOf<T>, DispatchInfo, usize);

/// The extension version every item is applied with.
pub const ITEM_EXTENSION_VERSION: ExtensionVersion = 0;

/// Leads the encoding of [`ItemImplication`], so that it does not encode like the implication of
/// a regular transaction, `(extension_version, call)`, for any realistic extension version and
/// pallet index.
pub const ITEM_IMPLICATION_PREFIX: &[u8; 8] = b"_mbatch_";

/// The implication an item is validated with, in place of the usual `(extension_version, call)`.
///
/// It binds the item to its batch: an item cannot be applied alone, in another batch or next to
/// other parties than the ones it was signed for.
pub struct ItemImplication<'a, Call, Origin> {
	/// Always [`ITEM_EXTENSION_VERSION`].
	pub extension_version: ExtensionVersion,
	/// The position of the item in the batch.
	pub index: u32,
	/// The call and origin of all items, in order.
	pub items: &'a [(&'a Call, &'a Origin)],
}

impl<'a, Call: Encode, Origin: Encode> Encode for ItemImplication<'a, Call, Origin> {
	fn encode_to<O: codec::Output + ?Sized>(&self, dest: &mut O) {
		dest.write(ITEM_IMPLICATION_PREFIX);
		self.extension_version.encode_to(dest);
		self.index.encode_to(dest);
		self.items.encode_to(dest);
	}
}

/// Marker extension for the items of a `batch_multi_origin` call.
///
/// Carries constant implicit data so that an item's signature differs from that of a regular
/// transaction with the same call and extension data.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	CloneNoBound,
	EqNoBound,
	PartialEqNoBound,
	DefaultNoBound,
	TypeInfo,
)]
#[scale_info(skip_type_params(T))]
pub struct MultiOriginBatchMarker<T>(PhantomData<T>);

impl<T> fmt::Debug for MultiOriginBatchMarker<T> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "MultiOriginBatchMarker")
	}
}

impl<T> MultiOriginBatchMarker<T> {
	/// Create a new marker extension.
	pub fn new() -> Self {
		Self(PhantomData)
	}
}

impl<T: Config + Send + Sync> TransactionExtension<RuntimeCallOf<T>> for MultiOriginBatchMarker<T> {
	const IDENTIFIER: &'static str = "MultiOriginBatchMarker";
	type Implicit = [u8; 8];
	type Val = ();
	type Pre = ();

	fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
		Ok(*b"_mb_mrk_")
	}

	fn weight(&self, _call: &RuntimeCallOf<T>) -> Weight {
		Weight::zero()
	}

	impl_tx_ext_default!(RuntimeCallOf<T>; validate prepare);
}

/// The [`TransactionExtension::Val`] of [`MultiOriginBatch`].
pub enum Val {
	/// Not a `batch_multi_origin` transaction authorized by this extension.
	NotUsing,
	/// A `batch_multi_origin` transaction authorized by this extension.
	Using,
}

/// The [`TransactionExtension::Pre`] of [`MultiOriginBatch`].
pub enum Pre<T: Config> {
	/// Not a `batch_multi_origin` transaction authorized by this extension. Holds the unspent
	/// weight.
	NotUsing(Weight),
	/// The pre-dispatch data of every item.
	Using(Vec<ItemPreOf<T>>),
}

/// Transaction extension authorizing general `batch_multi_origin` transactions.
///
/// Every item is applied as a general transaction, validated and prepared back to back so that it
/// sees the state left by the items before it.
/// - `validate` dry runs the items in a storage transaction which is rolled back.
/// - `prepare` applies them and stores the authorized origins in
/// [`MultiOrigins`](crate::MultiOrigins) for the call.
/// - `post_dispatch` runs every item's post dispatch with the actual weight recorded in
/// [`MultiOriginPostInfos`](crate::MultiOriginPostInfos).
///
/// Items pay for the whole transaction, see [`Pallet::multi_origin_item_infos`]. The extension's
/// own work is not weighed. It handles at most `MaxMultiOriginBatch` items and its storage never
/// leaves the overlay.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	CloneNoBound,
	EqNoBound,
	PartialEqNoBound,
	DefaultNoBound,
	TypeInfo,
)]
#[scale_info(skip_type_params(T))]
pub struct MultiOriginBatch<T>(PhantomData<T>);

impl<T> fmt::Debug for MultiOriginBatch<T> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "MultiOriginBatch")
	}
}

impl<T> MultiOriginBatch<T> {
	/// Create a new extension.
	pub fn new() -> Self {
		Self(PhantomData)
	}
}

impl<T: Config + Send + Sync> MultiOriginBatch<T> {
	/// Validate and prepare every item in order. `info` and `len` are the whole transaction's.
	fn apply_items(
		items: &MultiOriginItemsOf<T>,
		info: &DispatchInfo,
		len: usize,
		source: TransactionSource,
	) -> Result<
		(ValidTransaction, Vec<ItemPreOf<T>>, Vec<PalletsOriginOf<T>>),
		TransactionValidityError,
	> {
		let commitments: Vec<_> = items.iter().map(|item| (&item.call, &item.origin)).collect();
		let mut validity = ValidTransaction::default();
		let mut priority = None;
		let mut pres = Vec::with_capacity(items.len());
		let mut origins = Vec::with_capacity(items.len());
		let item_infos = Pallet::<T>::multi_origin_item_infos(items, info, len);
		for (index, (item, (item_info, item_len))) in items.iter().zip(item_infos).enumerate() {
			let (item_validity, item_val, item_origin) = Self::validate_item(
				&commitments,
				index,
				&item.extension,
				&item_info,
				item_len,
				source,
			)?;
			if PalletsOriginOf::<T>::from_ref(item_origin.caller()) != &item.origin {
				return Err(InvalidTransaction::BadSigner.into());
			}
			let item_pre = item.extension.clone().prepare(
				item_val,
				&item_origin,
				&item.call,
				&item_info,
				item_len,
			)?;
			priority = Some(item_validity.priority.min(priority.unwrap_or(item_validity.priority)));
			validity = validity.combine_with(item_validity);
			pres.push((item_pre, item_info, item_len));
			origins.push(PalletsOriginOf::<T>::from(item_origin.into_caller()));
		}
		// We take the min here conservatively because the sum would let a batch of small items jump
		// the queue.
		validity.priority = priority.unwrap_or_default();
		Ok((validity, pres, origins))
	}

	/// Validate the item at `index` as a general transaction bound to `items`.
	fn validate_item(
		items: &[(&RuntimeCallOf<T>, &PalletsOriginOf<T>)],
		index: usize,
		extension: &<T as Config>::InnerExtension,
		item_info: &DispatchInfo,
		item_len: usize,
		source: TransactionSource,
	) -> Result<(ValidTransaction, InnerValOf<T>, RuntimeOriginOf<T>), TransactionValidityError> {
		let implication = TxBaseImplication(ItemImplication {
			extension_version: ITEM_EXTENSION_VERSION,
			index: index as u32,
			items,
		});
		let (validity, val, origin) = extension.validate(
			frame_system::RawOrigin::None.into(),
			items[index].0,
			item_info,
			item_len,
			extension.implicit()?,
			&implication,
			source,
		)?;
		if !origin.is_transaction_authorized() {
			return Err(InvalidTransaction::UnknownOrigin.into());
		}
		Ok((validity, val, origin))
	}
}

impl<T: Config + Send + Sync> TransactionExtension<RuntimeCallOf<T>> for MultiOriginBatch<T> {
	const IDENTIFIER: &'static str = "MultiOriginBatch";
	type Implicit = ();
	type Val = Val;
	type Pre = Pre<T>;

	fn weight(&self, call: &RuntimeCallOf<T>) -> Weight {
		let Some(items) = Pallet::<T>::multi_origin_items(call) else {
			return Weight::zero();
		};
		items.iter().fold(Weight::zero(), |weight, item| {
			weight.saturating_add(Pallet::<T>::multi_origin_item_extension_weight(item))
		})
	}

	fn validate(
		&self,
		origin: RuntimeOriginOf<T>,
		call: &RuntimeCallOf<T>,
		info: &DispatchInfo,
		len: usize,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Implication,
		source: TransactionSource,
	) -> ValidateResult<Self::Val, RuntimeCallOf<T>> {
		// Only general `batch_multi_origin` transactions are handled.
		let items = match Pallet::<T>::multi_origin_items(call) {
			Some(items) if !origin.is_transaction_authorized() => items,
			_ => return Ok((ValidTransaction::default(), Val::NotUsing, origin)),
		};
		if items.is_empty() {
			return Err(InvalidTransaction::Call.into());
		}
		// Nested calls are filtered at dispatch through the item's origin.
		if items
			.iter()
			.any(|item| !<T as Config>::MultiOriginCallFilter::contains(&item.call))
		{
			return Err(InvalidTransaction::Call.into());
		}

		// Dry run the items in sequence without writing anything.
		let validity = with_transaction::<_, DispatchError, _>(|| {
			let result = Self::apply_items(items, info, len, source).map(|(validity, ..)| validity);
			TransactionOutcome::Rollback(Ok(result))
		})
		.map_err(|_| InvalidTransaction::ExhaustsResources)??;

		Ok((validity, Val::Using, Origin::MultiOriginBatch.into()))
	}

	fn prepare(
		self,
		val: Self::Val,
		_origin: &RuntimeOriginOf<T>,
		call: &RuntimeCallOf<T>,
		info: &DispatchInfo,
		len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		let Val::Using = val else { return Ok(Pre::NotUsing(self.weight(call))) };
		let items = Pallet::<T>::multi_origin_items(call).ok_or(InvalidTransaction::Call)?;
		let (_, pres, origins) = Self::apply_items(items, info, len, TransactionSource::InBlock)?;
		MultiOrigins::<T>::put(origins);

		Ok(Pre::Using(pres))
	}

	fn post_dispatch_details(
		pre: Self::Pre,
		_info: &DispatchInfo,
		_post_info: &PostDispatchInfoOf<RuntimeCallOf<T>>,
		_len: usize,
		result: &DispatchResult,
	) -> Result<Weight, TransactionValidityError> {
		let pres = match pre {
			Pre::Using(pres) => pres,
			Pre::NotUsing(unspent) => return Ok(unspent),
		};
		// This is left behind if the call failed, so it needs to be cleared.
		MultiOrigins::<T>::kill();
		// The call records the items' post infos only when it succeeds. If it failed, every
		// item is settled at its worst case, like a failed regular transaction.
		let post_infos = match MultiOriginPostInfos::<T>::take() {
			Some(post_infos) if post_infos.len() == pres.len() => post_infos,
			_ => alloc::vec![PostDispatchInfo::default(); pres.len()],
		};

		let mut unspent = Weight::zero();
		for ((item_pre, info, len), mut post_info) in pres.into_iter().zip(post_infos) {
			post_info.set_extension_weight(&info);
			let before = post_info.actual_weight.unwrap_or_default();
			<T::InnerExtension as TransactionExtension<RuntimeCallOf<T>>>::post_dispatch(
				item_pre,
				&info,
				&mut post_info,
				len,
				result,
			)?;
			// What the item's pipeline refunded is unspent weight of this extension.
			let after = post_info.actual_weight.unwrap_or(before);
			unspent = unspent.saturating_add(before.saturating_sub(after));
		}

		Ok(unspent)
	}
}
