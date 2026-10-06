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

//! # Speculative Messaging pallet
//!
//! Sender: accumulates outbound streams (per-stream MMR frontiers) and commits one [`StreamsRoot`]
//! per block, emitted as the `Provides` UMP signal ([`ProvideConsumptionRecord`]) and an
//! [`SPMS_ENGINE_ID`] digest. Receiver: consumes fetched inbound payloads through
//! [`Call::enact_messages`] by recomputation into [`InboundFrontier`], writing the block's
//! [`ConsumptionRecord`]. The inherent carries no proofs (design §10); the PoV lift binds the
//! endpoint.
//!
//! Lifecycle: sends/consumption of block `N` are staged this block; `on_finalize` folds the
//! [`StreamsRoot`]; `on_initialize` of `N+1` drains sends into the frontiers and clears transients.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{
	collections::{BTreeMap, BTreeSet},
	vec::Vec,
};
use codec::{DecodeAll, Encode};
use cumulus_primitives_spec_messaging::{
	leaf_hash, streams_root::streams_root, ChannelId, ConsumeItem, ConsumedStream,
	ConsumptionRecord, InChannelState, Interval, MessagePosition, MessagingInherentData,
	MmrFrontier, OutChannelState, Payload, ProvideConsumptionRecord, SpecMsgKind, StreamId, StreamsRoot,
	INHERENT_IDENTIFIER, LEAF_VERSION, SPMS_ENGINE_ID,
};
use frame_support::{ensure, pallet_prelude::Weight, traits::Get, BoundedVec};
use polkadot_core_primitives::Hash;
use polkadot_parachain_primitives::primitives::Id as ParaId;
use sp_runtime::generic::DigestItem;

pub use pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

/// Sink for consumed `Data` payloads. `()` drops them; the real handler lands with the XCM layer.
pub trait OnSpecMsgData {
	/// One `Data` payload, consumed in order at `position` of `(source, stream)`.
	fn on_data(source: ParaId, stream: StreamId, position: MessagePosition, data: Vec<u8>);
}

impl OnSpecMsgData for () {
	fn on_data(_: ParaId, _: StreamId, _: MessagePosition, _: Vec<u8>) {}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{
		inherent::{InherentData, InherentIdentifier, MakeFatalError, ProvideInherent},
		pallet_prelude::*,
	};
	use frame_system::pallet_prelude::*;
	use polkadot_primitives::v9::MAX_COMMITMENT_ENTRIES;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// This parachain's own id; consumed streams are addressed to it.
		type SelfParaId: Get<ParaId>;

		/// Hard per-message payload size bound.
		#[pallet::constant]
		type MaxMsgLen: Get<u32>;

		/// Per-stream, per-block cap on outbound sends.
		#[pallet::constant]
		type MaxMessagesPerBlock: Get<u32>;

		/// Per-block cap on streams the inherent may touch; `integrity_test` keeps it
		/// `<= MAX_COMMITMENT_ENTRIES`.
		#[pallet::constant]
		type MaxTouchedStreams: Get<u32>;

		/// Per-block cap on inclusion-discipline reads (`ConsumeItem::Events`).
		#[pallet::constant]
		type MaxContextGaps: Get<u32>;

		/// Sink for consumed `Data` payloads.
		type DataHandler: OnSpecMsgData;
	}

	/// Per-stream outbound MMR frontiers; reflects state as of the previous block.
	#[pallet::storage]
	pub type OutboundFrontier<T: Config> =
		StorageMap<_, Twox64Concat, StreamId, MmrFrontier, ValueQuery>;

	/// Outbound sends of this block only, per stream; drained into the frontiers next block.
	#[pallet::storage]
	pub type OutboundMessages<T: Config> = StorageMap<
		_,
		Twox64Concat,
		StreamId,
		BoundedVec<BoundedVec<u8, T::MaxMsgLen>, T::MaxMessagesPerBlock>,
		ValueQuery,
	>;

	/// This block's committed [`StreamsRoot`] (the `Provides` source); transient.
	#[pallet::storage]
	pub type BlockStreamsRoot<T: Config> = StorageValue<_, StreamsRoot, OptionQuery>;

	/// Consumption frontier per consumed inbound stream `(source, stream)`.
	#[pallet::storage]
	pub type InboundFrontier<T: Config> =
		StorageMap<_, Twox64Concat, (ParaId, StreamId), MmrFrontier, ValueQuery>;

	/// Replay guard for inclusion-discipline streams: the next read's `base` must exceed this.
	#[pallet::storage]
	pub type InboundHighwater<T: Config> =
		StorageMap<_, Twox64Concat, (ParaId, StreamId), u64, OptionQuery>;

	/// This block's consumption intervals; grouped/sorted by [`Pallet::consumption_record`],
	/// cleared next block. Bounded by [`Config::MaxTouchedStreams`].
	#[pallet::storage]
	#[pallet::unbounded]
	pub type ConsumptionOutbox<T: Config> =
		StorageValue<_, Vec<(ParaId, StreamId, Interval)>, ValueQuery>;

	#[pallet::error]
	#[derive(PartialEq, Eq)]
	pub enum Error<T> {
		/// Payload over [`Config::MaxMsgLen`].
		MessageTooBig,
		/// Stream over [`Config::MaxMessagesPerBlock`] this block.
		TooManyMessages,
		/// Wrong stream kind for the item, or addressed to another chain.
		UnknownStream,
		/// A second item for the same stream.
		DuplicateStream,
		/// A consume item with no payloads.
		EmptyItem,
		/// [`Config::MaxTouchedStreams`] exhausted.
		TooManyStreams,
		/// [`Config::MaxContextGaps`] exhausted.
		TooManyGaps,
		/// An `Events` item's `(start_peaks, base)` is not a valid frontier.
		BadFrontier,
		/// An `Events` item's `base` does not exceed the highwater (a replay).
		Replay,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			BlockStreamsRoot::<T>::kill();
			ConsumptionOutbox::<T>::kill();

			// Drain the previous block's sends into the frontiers. TODO: benchmark.
			let mut weight = T::DbWeight::get().reads_writes(2, 2);
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

		fn integrity_test() {
			assert!(
				T::MaxTouchedStreams::get() <= MAX_COMMITMENT_ENTRIES,
				"`MaxTouchedStreams` must not exceed `MAX_COMMITMENT_ENTRIES`",
			);
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Consume this block's fetched payloads by recomputation and record them.
		/// Strict-on-import: any invalid item invalidates the block.
		#[pallet::call_index(0)]
		#[pallet::weight((enact_weight::<T>(data), DispatchClass::Mandatory))]
		pub fn enact_messages(origin: OriginFor<T>, data: MessagingInherentData) -> DispatchResult {
			ensure_none(origin)?;

			let mut touched = BTreeSet::new();
			let mut gaps = 0u32;
			for (source, stream, item) in data.items {
				match item {
					ConsumeItem::Channel { payloads } => {
						Self::consume_channel_item(&mut touched, source, stream, payloads)?
					},
					ConsumeItem::Events { base, start_peaks, payloads } => {
						Self::consume_events_item(
							&mut touched,
							&mut gaps,
							source,
							stream,
							base,
							start_peaks,
							payloads,
						)?
					},
				}
			}
			Ok(())
		}
	}

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = MakeFatalError<()>;
		const INHERENT_IDENTIFIER: InherentIdentifier =
			cumulus_primitives_spec_messaging::INHERENT_IDENTIFIER;

		fn create_inherent(data: &InherentData) -> Option<Self::Call> {
			let data =
				data.get_data::<MessagingInherentData>(&INHERENT_IDENTIFIER).ok().flatten()?;
			(!data.is_empty()).then_some(Call::enact_messages { data })
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::enact_messages { .. })
		}
	}
}

/// Weight of one `enact_messages`. TODO: benchmark.
fn enact_weight<T: Config>(data: &MessagingInherentData) -> Weight {
	T::DbWeight::get()
		.reads_writes(1, 1)
		.saturating_mul(1 + data.items.len() as u64)
}

impl<T: Config> Pallet<T> {
	/// Append `payload` to `stream`'s outbound MMR, returning its stable position. Enforces only
	/// the consensus hard caps.
	pub fn append_to_stream(
		stream: StreamId,
		payload: Vec<u8>,
	) -> Result<MessagePosition, Error<T>> {
		let payload: BoundedVec<u8, T::MaxMsgLen> =
			payload.try_into().map_err(|_| Error::<T>::MessageTooBig)?;

		let index = OutboundMessages::<T>::decode_len(stream).unwrap_or(0) as u64;
		OutboundMessages::<T>::try_append(stream, payload)
			.map_err(|()| Error::<T>::TooManyMessages)?;

		Ok(MessagePosition(OutboundFrontier::<T>::get(stream).leaf_count() + index))
	}

	/// Fold this block's sends into the [`StreamsRoot`], memoize it, and deposit the digest.
	/// Idempotent; `None` on idle blocks so an unchanged root is never re-emitted.
	pub fn commit_streams_root() -> Option<StreamsRoot> {
		if let Some(root) = BlockStreamsRoot::<T>::get() {
			return Some(root);
		}

		// Root over every stream ever touched: persisted frontiers, then this block's sends
		// overlaid (a first-touch stream is not yet persisted, so no double count).
		//
		// TODO(spec-msg): O(all-streams) per touched block, growing without bound. Replace with the
		// incremental updater tracked on `streams_root`.
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

		let root = streams_root(&entries).expect("touched, so non-empty; qed");
		BlockStreamsRoot::<T>::put(root);
		frame_system::Pallet::<T>::deposit_log(DigestItem::Consensus(
			SPMS_ENGINE_ID,
			root.encode(),
		));

		Some(root)
	}

	/// This block's committed [`StreamsRoot`], if any.
	pub fn current_streams_root() -> Option<StreamsRoot> {
		BlockStreamsRoot::<T>::get()
	}

	/// This block's outbound sends per stream, in [`StreamId`] order.
	pub fn outbound_messages() -> Vec<(StreamId, Vec<Vec<u8>>)> {
		let mut messages: Vec<(StreamId, Vec<Vec<u8>>)> = OutboundMessages::<T>::iter()
			.map(|(stream, payloads)| {
				(stream, payloads.into_iter().map(BoundedVec::into_inner).collect())
			})
			.collect();
		messages.sort_by_key(|(stream, _)| *stream);
		messages
	}

	/// Append `payloads` onto the stream's stored [`InboundFrontier`] and record the [`Interval`].
	/// Order/count need no check — a deviation yields an endpoint no lift can bind.
	fn consume_channel_item(
		touched: &mut BTreeSet<(ParaId, StreamId)>,
		source: ParaId,
		stream: StreamId,
		payloads: Vec<Payload>,
	) -> Result<(), Error<T>> {
		let StreamId::Channel { recipient, .. } = stream else {
			return Err(Error::<T>::UnknownStream);
		};
		ensure!(recipient == T::SelfParaId::get(), Error::<T>::UnknownStream);
		Self::check_touch(touched, source, stream, &payloads)?;

		let mut frontier = InboundFrontier::<T>::get((source, stream));
		let start = frontier.root();
		for payload in &payloads {
			let position = MessagePosition(frontier.leaf_count());
			frontier.append(leaf_hash(LEAF_VERSION, payload));
			// Route `Data`; signals are the channel layer's. A non-`SpecMsgKind` payload is a valid
			// leaf regardless, so it is consumed-and-dropped.
			if let Ok(SpecMsgKind::Data(data)) = SpecMsgKind::decode_all(&mut &payload[..]) {
				T::DataHandler::on_data(source, stream, position, data);
			}
		}
		InboundFrontier::<T>::insert((source, stream), &frontier);
		ConsumptionOutbox::<T>::append((source, stream, Interval { start, end: frontier }));
		Ok(())
	}

	/// Rebuild the frontier from `(start_peaks, base)`, guard replay via the highwater, append
	/// `payloads`, and record the [`Interval`]. The hints are unproven; a lie binds no lift.
	fn consume_events_item(
		touched: &mut BTreeSet<(ParaId, StreamId)>,
		gaps: &mut u32,
		source: ParaId,
		stream: StreamId,
		base: MessagePosition,
		start_peaks: Vec<Hash>,
		payloads: Vec<Payload>,
	) -> Result<(), Error<T>> {
		ensure!(*gaps < T::MaxContextGaps::get(), Error::<T>::TooManyGaps);
		Self::check_touch(touched, source, stream, &payloads)?;

		if let Some(highwater) = InboundHighwater::<T>::get((source, stream)) {
			ensure!(base.0 > highwater, Error::<T>::Replay);
		}
		let mut frontier =
			MmrFrontier::from_parts(start_peaks, base.0).ok_or(Error::<T>::BadFrontier)?;
		let start = frontier.root();
		for payload in &payloads {
			frontier.append(leaf_hash(LEAF_VERSION, payload));
		}
		InboundHighwater::<T>::insert(
			(source, stream),
			base.0.saturating_add(payloads.len() as u64).saturating_sub(1),
		);
		ConsumptionOutbox::<T>::append((source, stream, Interval { start, end: frontier }));
		*gaps += 1;
		Ok(())
	}

	/// Per-item guards: not already touched, non-empty, no oversized payload, within the cap.
	fn check_touch(
		touched: &mut BTreeSet<(ParaId, StreamId)>,
		source: ParaId,
		stream: StreamId,
		payloads: &[Payload],
	) -> Result<(), Error<T>> {
		ensure!(!touched.contains(&(source, stream)), Error::<T>::DuplicateStream);
		ensure!(!payloads.is_empty(), Error::<T>::EmptyItem);
		ensure!(
			payloads.iter().all(|p| p.len() <= T::MaxMsgLen::get() as usize),
			Error::<T>::MessageTooBig
		);
		ensure!((touched.len() as u32) < T::MaxTouchedStreams::get(), Error::<T>::TooManyStreams);
		touched.insert((source, stream));
		Ok(())
	}

	/// This block's consumption grouped by source, per source in [`StreamId`] order.
	pub fn consumption_record() -> ConsumptionRecord {
		let mut record = ConsumptionRecord::default();
		for (source, stream, interval) in ConsumptionOutbox::<T>::get() {
			record.entries.entry(source).or_default().insert(stream, interval);
		}
		record
	}

	/// The inbound streams the node should fetch, per source, with their fetch cursors.
	///
	/// Empty until the channel layer lands: the wanted streams are the open, unsuspended inbound
	/// channels, and this pallet has no channel state yet. [`InboundFrontier`] cannot stand in for
	/// it, because it only holds streams consumed at least once, so a new channel would never be
	/// fetched.
	pub fn consumed_streams() -> BTreeMap<ParaId, Vec<ConsumedStream>> {
		BTreeMap::new()
	}

	/// Outbound channel views. Empty until the channel layer lands.
	pub fn out_channels() -> BTreeMap<ChannelId, OutChannelState> {
		BTreeMap::new()
	}

	/// Inbound channel views. Empty until the channel layer lands.
	pub fn in_channels() -> BTreeMap<ChannelId, InChannelState> {
		BTreeMap::new()
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
