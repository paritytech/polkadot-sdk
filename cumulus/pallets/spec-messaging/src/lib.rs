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

//! # Speculative Messaging pallet (sender half)
//!
//! Accumulates a parachain's OUTBOUND Speculative Messaging streams and commits to them with a
//! single [`StreamsRoot`] per block, emitted as the `Provides` UMP signal (via
//! [`ProvideConsumptionRecord`]) and deposited as an [`SPMS_ENGINE_ID`] digest.
//!
//! Each stream is an append-only MMR; its frontier ([`MmrFrontier`]) is the only long-lived state.
//! The [`StreamsRoot`] is the root of the stream commitment tree over `{stream -> stream root}`; it
//! is recomputed from the frontiers at the end-of-block fold via the canonical
//! [`streams_root`](cumulus_primitives_spec_messaging::streams_root::streams_root), so the pallet
//! never stores a tree — the frontiers determine it.
//!
//! ## Block lifecycle
//!
//! - Messages of block `N` are appended to [`OutboundMessages`] (per-stream, this-block only) via
//!   [`Pallet::append_to_stream`]. The stored frontier is untouched during the block, so a message
//!   position is stable from the moment of the send.
//! - `on_finalize` of block `N` folds this block's sends into a transient root and memoizes it in
//!   [`BlockStreamsRoot`] ([`Pallet::commit_streams_root`]); idle blocks touch nothing and emit
//!   nothing.
//! - `on_initialize` of block `N+1` drains [`OutboundMessages`] into the persistent
//!   [`OutboundFrontier`] and clears the previous block's memo — one atomic step.
//!
//! Messages must be appended before this pallet's `on_finalize` runs; a payload appended by a later
//! `on_finalize` hook would miss the fold (the "append before `on_finalize`" ordering rule, which
//! covers parachain-system's `on_finalize` too).
//!
//! This is the SENDER half only: the receiver (messaging inherent, consumption record), the channel
//! lifecycle, and the XCM router land in later PRs. [`Pallet::consumption_record`] therefore
//! returns empty here.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{collections::BTreeMap, vec::Vec};
use codec::Encode;
use cumulus_primitives_spec_messaging::{
	leaf_hash, streams_root::streams_root, ConsumptionRecord, MessagePosition, MmrFrontier,
	ProvideConsumptionRecord, StreamId, StreamsRoot, LEAF_VERSION, SPMS_ENGINE_ID,
};
use frame_support::BoundedVec;
use polkadot_core_primitives::Hash;
use sp_runtime::generic::DigestItem;

pub use pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Hard per-message payload size bound, in bytes — a consensus constant of this chain's
		/// streams.
		#[pallet::constant]
		type MaxMsgLen: Get<u32>;

		/// Per-stream, per-block cap on appended messages — with [`Config::MaxMsgLen`] the hard,
		/// consensus-side backpressure of the transport.
		#[pallet::constant]
		type MaxMessagesPerBlock: Get<u32>;
	}

	/// Per-stream MMR frontiers (peaks + leaf count) — the only long-lived sender state. Throughout
	/// a block this reflects state as of the PREVIOUS block: this block's sends are appended at the
	/// next block's `on_initialize`. The stream commitment tree is recomputed from these frontiers;
	/// no tree is stored.
	#[pallet::storage]
	pub type OutboundFrontier<T: Config> =
		StorageMap<_, Twox64Concat, StreamId, MmrFrontier, ValueQuery>;

	/// Messages sent in THIS block only, per stream, kept for node-side extraction (which reads
	/// them in this block's state). Message `i` sits at position
	/// `OutboundFrontier[stream].leaf_count + i`; appends are O(1) so gaps are unrepresentable.
	#[pallet::storage]
	pub type OutboundMessages<T: Config> = StorageMap<
		_,
		Twox64Concat,
		StreamId,
		BoundedVec<BoundedVec<u8, T::MaxMsgLen>, T::MaxMessagesPerBlock>,
		ValueQuery,
	>;

	/// The [`StreamsRoot`] committed by the block being executed — the end-of-block fold's memo,
	/// feeding the `Provides` UMP signal. Transient: set by [`Pallet::commit_streams_root`] iff
	/// this block touched a stream, cleared at the next `on_initialize`.
	#[pallet::storage]
	pub type BlockStreamsRoot<T: Config> = StorageValue<_, StreamsRoot, OptionQuery>;

	#[pallet::error]
	pub enum Error<T> {
		/// A payload exceeded [`Config::MaxMsgLen`].
		MessageTooBig,
		/// A stream already holds [`Config::MaxMessagesPerBlock`] sends this block.
		TooManyMessages,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			// The previous block's fold memo dies with its block.
			BlockStreamsRoot::<T>::kill();

			// Bump the frontiers with the previous block's sends and clear the per-block vecs — one
			// atomic step. From here on the stored frontiers reflect everything sent up to and
			// including the previous block.
			//
			// TODO: benchmark. DbWeight-based estimate for now.
			let mut weight = T::DbWeight::get().reads_writes(1, 1);
			for (stream, messages) in OutboundMessages::<T>::drain() {
				let mut frontier = OutboundFrontier::<T>::get(stream);
				for payload in &messages {
					frontier.append(leaf_hash(LEAF_VERSION, payload));
				}
				OutboundFrontier::<T>::insert(stream, frontier);
				weight.saturating_accrue(T::DbWeight::get().reads_writes(2, 2));
			}
			weight
		}

		fn on_finalize(_n: BlockNumberFor<T>) {
			let _ = Self::commit_streams_root();
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Append `payload` to `stream`'s outbound MMR, returning its stable position.
	///
	/// Enforces only the consensus hard caps ([`Config::MaxMsgLen`],
	/// [`Config::MaxMessagesPerBlock`]); channel phase and credit gating happen in the caller (the
	/// channel layer, later PR). On error, state is untouched.
	pub fn append_to_stream(
		stream: StreamId,
		payload: Vec<u8>,
	) -> Result<MessagePosition, Error<T>> {
		let payload: BoundedVec<u8, T::MaxMsgLen> =
			payload.try_into().map_err(|_| Error::<T>::MessageTooBig)?;

		// The stored frontier holds state as of the previous block all block long, so the position
		// is stable from here on.
		let index = OutboundMessages::<T>::decode_len(stream).unwrap_or(0) as u64;
		OutboundMessages::<T>::try_append(stream, payload)
			.map_err(|()| Error::<T>::TooManyMessages)?;

		Ok(MessagePosition(OutboundFrontier::<T>::get(stream).leaf_count() + index))
	}

	/// Fold this block's sends into the [`StreamsRoot`], memoize it in [`BlockStreamsRoot`], and
	/// deposit the [`SPMS_ENGINE_ID`] digest. Idempotent via the memo. Returns `None` on idle
	/// blocks (nothing touched), so an unchanged root is never re-emitted.
	pub fn commit_streams_root() -> Option<StreamsRoot> {
		if let Some(root) = BlockStreamsRoot::<T>::get() {
			// The fold already ran in this block.
			return Some(root);
		}

		// The commitment tree covers every stream ever touched: start from the persisted frontiers,
		// then overlay this block's sends. A stream first touched this block has no persisted
		// frontier yet (`OutboundFrontier::iter` skips it), so the overlay is the only source and
		// there is no double count; a stream touched again overwrites its prior entry.
		//
		// TODO(spec-msg): this recomputes over ALL (eternal) streams every touched block — O(S),
		// growing without bound. Replace with the incremental updater tracked on `streams_root`.
		let mut entries: BTreeMap<StreamId, Hash> = BTreeMap::new();
		for (stream, frontier) in OutboundFrontier::<T>::iter() {
			entries.insert(stream, frontier.root().0);
		}

		let mut touched = false;
		for (stream, messages) in OutboundMessages::<T>::iter() {
			let mut frontier = OutboundFrontier::<T>::get(stream);
			for payload in &messages {
				frontier.append(leaf_hash(LEAF_VERSION, payload));
			}
			entries.insert(stream, frontier.root().0);
			touched = true;
		}
		if !touched {
			return None;
		}

		let root = streams_root(&entries)
			.expect("at least one stream was just folded, so the tree is non-empty; qed");
		BlockStreamsRoot::<T>::put(root);
		frame_system::Pallet::<T>::deposit_log(DigestItem::Consensus(
			SPMS_ENGINE_ID,
			root.encode(),
		));

		Some(root)
	}

	/// The [`StreamsRoot`] committed by the current block, if the fold ran and a stream was
	/// touched.
	pub fn current_streams_root() -> Option<StreamsRoot> {
		BlockStreamsRoot::<T>::get()
	}

	/// THIS block's sends, per touched stream, in canonical [`StreamId`] order — what a collator
	/// extracts for delivery (block `N`'s sends live in block `N`'s state). Payload `i` of a
	/// stream's vec sits at position `OutboundFrontier[stream].leaf_count + i`. Empty on idle
	/// blocks.
	pub fn outbound_messages() -> Vec<(StreamId, Vec<Vec<u8>>)> {
		let mut messages: Vec<(StreamId, Vec<Vec<u8>>)> = OutboundMessages::<T>::iter()
			.map(|(stream, payloads)| {
				(stream, payloads.into_iter().map(BoundedVec::into_inner).collect())
			})
			.collect();
		messages.sort_by_key(|(stream, _)| *stream);
		messages
	}

	/// The block's consumption record. Empty in the sender-only pallet; the receiver half (a later
	/// PR) fills it from the messaging inherent.
	pub fn consumption_record() -> ConsumptionRecord {
		ConsumptionRecord::default()
	}
}

impl<T: Config> ProvideConsumptionRecord for Pallet<T> {
	fn provides_root() -> Option<StreamsRoot> {
		Self::commit_streams_root()
	}

	fn consumption_record() -> ConsumptionRecord {
		Self::consumption_record()
	}
}
