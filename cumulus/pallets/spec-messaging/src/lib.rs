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
//! [`ConsumptionRecord`]. The inherent carries no proofs; the PoV lift binds the endpoint.
//!
//! Lifecycle: block `N` stages its sends and consumption; `on_finalize` folds the [`StreamsRoot`];
//! the first spec-msg action of `N+1` ([`Pallet::roll_over`]) drains the sends and clears the rest.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{
	collections::{BTreeMap, BTreeSet},
	vec::Vec,
};
use codec::{Decode, DecodeAll, Encode};
use cumulus_primitives_spec_messaging::{
	leaf_hash, streams_root::streams_root, ChannelId, ChannelPhase, ConsumeItem, ConsumedStream,
	ConsumptionRecord, InChannelState, Interval, MessagePosition, MessagingInherentData,
	MmrFrontier, OutChannelState, Payload, ProvideConsumptionRecord, Register, SpecMsgKind, SpecMsgSignal,
	StreamId, StreamsRoot, WindowGrant, INHERENT_IDENTIFIER, LEAF_VERSION, SPMS_ENGINE_ID,
};
use frame_support::{ensure, pallet_prelude::Weight, traits::Get, BoundedVec};
use polkadot_core_primitives::Hash;
use polkadot_parachain_primitives::primitives::Id as ParaId;
use sp_runtime::generic::DigestItem;

pub use pallet::*;

const LOG_TARGET: &str = "runtime::spec-msg";

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

/// The channel protocol version this implementation announces, in every `OpenChannel` signal and
/// every published register. `0` gates nothing yet.
pub const PROTOCOL_VERSION: u8 = 0;

/// Sender-side credit bookkeeping of one outbound channel, kept next to [`OutChannels`] so the
/// stored view stays exactly the runtime-API type.
#[derive(Clone, Encode, Decode, scale_info::TypeInfo, Debug, Default, Eq, PartialEq)]
pub struct OutChannelMeta {
	/// Stream position of the oldest in-flight (sent, unconfirmed) message, `sizes[0]`.
	pub base: MessagePosition,
	/// Encoded leaf sizes of the in-flight messages, oldest first. Every leaf on the data stream
	/// counts, `Data` and `Signal` alike. The length is the in-flight message count.
	pub sizes: Vec<u32>,
	/// Sum of `sizes`.
	pub bytes: u64,
}

impl OutChannelMeta {
	/// Account one appended leaf of `size` encoded bytes.
	fn account_send(&mut self, size: u32) {
		self.sizes.push(size);
		self.bytes = self.bytes.saturating_add(u64::from(size));
	}

	/// Release everything below the peer's watermark `up_to`. A watermark past what was sent
	/// releases everything sent, and no more: `base` never moves past the next unsent position.
	fn confirm(&mut self, up_to: MessagePosition) {
		let confirmed = up_to.0.saturating_sub(self.base.0).min(self.sizes.len() as u64);
		for size in self.sizes.drain(..confirmed as usize) {
			self.bytes = self.bytes.saturating_sub(u64::from(size));
		}
		self.base.0 = self.base.0.saturating_add(confirmed);
	}
}

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

		/// Origin allowed to open an outbound channel.
		type OpenChannelOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Origin allowed to accept an inbound channel.
		type AcceptChannelOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The send-window credit every published register grants.
		type DefaultWindowGrant: Get<WindowGrant>;
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

	/// This block's committed [`StreamsRoot`] (the `Provides` source); transient. `Some(None)` once
	/// an idle block has committed, so a later send is rejected there too.
	#[pallet::storage]
	pub type BlockStreamsRoot<T: Config> = StorageValue<_, Option<StreamsRoot>, OptionQuery>;

	/// The block whose roll-over has run, with the number of sends it drained. See
	/// [`Pallet::roll_over`].
	#[pallet::storage]
	pub type RolledOver<T: Config> = StorageValue<_, (BlockNumberFor<T>, u32), OptionQuery>;

	/// Consumption frontier per consumed inbound stream `(source, stream)`.
	#[pallet::storage]
	pub type InboundFrontier<T: Config> =
		StorageMap<_, Twox64Concat, (ParaId, StreamId), MmrFrontier, ValueQuery>;

	/// Replay guard for inclusion-discipline streams: the next read's `base` must exceed this.
	#[pallet::storage]
	pub type InboundHighwater<T: Config> =
		StorageMap<_, Twox64Concat, (ParaId, StreamId), u64, OptionQuery>;

	/// Sender side, per outbound channel. The phase is a view: `Opening` until the peer's register
	/// is first read, which is the acceptance. Entries are never removed.
	#[pallet::storage]
	pub type OutChannels<T: Config> =
		StorageMap<_, Twox64Concat, ChannelId, OutChannelState, OptionQuery>;

	/// Credit bookkeeping per outbound channel: the in-flight messages behind the credit gate.
	#[pallet::storage]
	pub type OutChannelsMeta<T: Config> =
		StorageMap<_, Twox64Concat, ChannelId, OutChannelMeta, ValueQuery>;

	/// Receiver side, per inbound channel (`peer` = the channel's sender). An entry is the
	/// acceptance; [`Pallet::consumed_streams`] lists the live ones. Entries are never removed.
	#[pallet::storage]
	pub type InChannels<T: Config> =
		StorageMap<_, Twox64Concat, ChannelId, InChannelState, OptionQuery>;

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
		/// An `Events` item's `(start_peaks, base)` is not a valid frontier, or an item's payloads
		/// would carry a frontier past `MAX_MMR_LEAF_COUNT`.
		BadFrontier,
		/// An `Events` item's `base` does not exceed the highwater (a replay).
		Replay,
		/// This block's `StreamsRoot` is already committed; a later send could not be served.
		RootCommitted,
		/// `enact_messages` already ran this block.
		AlreadyEnacted,
		/// A channel to this chain itself.
		ChannelToSelf,
		/// The outbound channel already exists.
		AlreadyOpen,
		/// The inbound channel is already accepted.
		AlreadyAccepted,
		/// The outbound channel is not `Open`: unknown, not yet accepted, or closed.
		ChannelNotOpen,
		/// The send would exceed the peer's granted window.
		NoCredit,
		/// A register read is not exactly one leaf, or does not decode as a [`Register`].
		BadRegister,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			// Usually the roll-over runs here. A pallet whose `on_initialize` runs first and sends
			// has already run it; the drain is charged here either way. TODO: benchmark.
			let drained = Self::roll_over();
			T::DbWeight::get().reads_writes(3, 3).saturating_add(
				T::DbWeight::get().reads_writes(2, 2).saturating_mul(drained.into()),
			)
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
			Self::roll_over();
			// Once per block: the record keeps one interval per stream, so a second consumption
			// of a stream would replace the first, and no lift would bind the first.
			ensure!(!ConsumptionOutbox::<T>::exists(), Error::<T>::AlreadyEnacted);

			let mut touched = BTreeSet::new();
			let mut gaps = 0u32;
			for (source, stream, item) in data.items {
				let count = match &item {
					ConsumeItem::Channel { payloads } | ConsumeItem::Events { payloads, .. } => {
						payloads.len()
					},
				};
				let result = match item {
					ConsumeItem::Channel { payloads } => {
						Self::consume_channel_item(&mut touched, source, stream, payloads)
					},
					ConsumeItem::Events { base, start_peaks, payloads } => {
						Self::consume_events_item(
							&mut touched,
							&mut gaps,
							source,
							stream,
							base,
							start_peaks.into_inner(),
							payloads,
						)
					},
				};
				// The block fails with only the error name, and its events roll back with it:
				// name the item.
				if let Err(error) = result {
					log::warn!(
						target: LOG_TARGET,
						"Invalid inherent item from {source:?} on {stream:?}: {error:?}",
					);
					return Err(error.into());
				}
				log::debug!(
					target: LOG_TARGET,
					"Consumed {count} payloads from {source:?} on {stream:?}",
				);
			}
			Ok(())
		}

		/// Open the outbound channel `(recipient, domain, num)`: append the `OpenChannel` signal
		/// to its data stream. That leaf is the only one sendable without credit. The channel
		/// stays `Opening` until the recipient accepts and its register is read; until then,
		/// nothing else can be sent.
		#[pallet::call_index(1)]
		#[pallet::weight(T::DbWeight::get().reads_writes(3, 3))]
		pub fn open_channel(
			origin: OriginFor<T>,
			recipient: ParaId,
			domain: u8,
			num: u16,
		) -> DispatchResult {
			T::OpenChannelOrigin::ensure_origin(origin)?;
			ensure!(recipient != T::SelfParaId::get(), Error::<T>::ChannelToSelf);
			let channel = ChannelId { peer: recipient, domain, num };
			ensure!(!OutChannels::<T>::contains_key(channel), Error::<T>::AlreadyOpen);

			// Anchor the in-flight window at the stream's next position: the `OpenChannel` leaf
			// appended below is its first message.
			let stream = Self::outbound_stream(&channel);
			let next = OutboundFrontier::<T>::get(stream)
				.leaf_count()
				.saturating_add(OutboundMessages::<T>::decode_len(stream).unwrap_or(0) as u64);
			OutChannelsMeta::<T>::mutate(channel, |meta| meta.base = MessagePosition(next));

			Self::send_signal(&channel, SpecMsgSignal::OpenChannel { version: PROTOCOL_VERSION })?;
			OutChannels::<T>::insert(
				channel,
				OutChannelState {
					closed_by_us: false,
					announced_version: PROTOCOL_VERSION,
					register: None,
				},
			);
			Ok(())
		}

		/// Accept the inbound channel `(sender, domain, num)`. Its data stream joins
		/// [`Pallet::consumed_streams`], and the initial register is published on our `Ack`
		/// stream: the acceptance as the sender sees it. Either order works; accepting first is
		/// pre-authorization. Rejecting is never accepting, which costs nothing.
		#[pallet::call_index(2)]
		#[pallet::weight(T::DbWeight::get().reads_writes(3, 3))]
		pub fn accept_open_channel(
			origin: OriginFor<T>,
			sender: ParaId,
			domain: u8,
			num: u16,
		) -> DispatchResult {
			T::AcceptChannelOrigin::ensure_origin(origin)?;
			ensure!(sender != T::SelfParaId::get(), Error::<T>::ChannelToSelf);
			let channel = ChannelId { peer: sender, domain, num };
			ensure!(!InChannels::<T>::contains_key(channel), Error::<T>::AlreadyAccepted);

			let mut state = InChannelState {
				published: Register {
					version: PROTOCOL_VERSION,
					up_to: MessagePosition(0),
					grant: WindowGrant::default(),
					closed: false,
				},
				peer_version: 0,
				suspended: false,
			};
			Self::publish_register(&channel, &mut state)?;
			InChannels::<T>::insert(channel, state);
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
	/// Start this block's messaging state once: forget the previous block's root and consumption,
	/// and drain its sends. Returns the number drained. Everything that writes this block's state
	/// calls it first, so a send from an earlier pallet's `on_initialize` stays this block's.
	pub fn roll_over() -> u32 {
		let now = frame_system::Pallet::<T>::block_number();
		if let Some((at, drained)) = RolledOver::<T>::get() {
			if at == now {
				return drained;
			}
		}

		BlockStreamsRoot::<T>::kill();
		ConsumptionOutbox::<T>::kill();
		let mut drained = 0u32;
		for (stream, messages) in OutboundMessages::<T>::drain() {
			let mut frontier = OutboundFrontier::<T>::get(stream);
			for payload in &messages {
				frontier.append(leaf_hash(LEAF_VERSION, payload));
				drained = drained.saturating_add(1);
			}
			OutboundFrontier::<T>::insert(stream, frontier);
		}
		RolledOver::<T>::put((now, drained));
		drained
	}

	/// Append `payload` to `stream`'s outbound MMR, returning its stable position. Enforces only
	/// the consensus hard caps.
	pub fn append_to_stream(
		stream: StreamId,
		payload: Vec<u8>,
	) -> Result<MessagePosition, Error<T>> {
		let payload: BoundedVec<u8, T::MaxMsgLen> =
			payload.try_into().map_err(|_| Error::<T>::MessageTooBig)?;

		Self::roll_over();
		// Once the root is committed (`parachain-system` does so in its `on_finalize`), a send
		// would show in `outbound_messages` but not in the root, and no node could serve it.
		ensure!(!BlockStreamsRoot::<T>::exists(), Error::<T>::RootCommitted);
		let index = OutboundMessages::<T>::decode_len(stream).unwrap_or(0) as u64;
		OutboundMessages::<T>::try_append(stream, payload)
			.map_err(|()| Error::<T>::TooManyMessages)?;

		Ok(MessagePosition(OutboundFrontier::<T>::get(stream).leaf_count() + index))
	}

	/// Fold this block's sends into the [`StreamsRoot`], memoize it, and deposit the digest.
	/// Idempotent; `None` on idle blocks so an unchanged root is never re-emitted.
	pub fn commit_streams_root() -> Option<StreamsRoot> {
		Self::roll_over();
		if let Some(committed) = BlockStreamsRoot::<T>::get() {
			return committed;
		}

		// Root over every stream ever touched: this block's sends, then the persisted frontiers
		// of the streams it did not touch. The sends go first, so an idle block reads no
		// frontier.
		//
		// TODO(spec-msg): O(all-streams) per touched block, growing without bound. Replace with the
		// incremental updater tracked on `streams_root`.
		let mut entries: BTreeMap<StreamId, Hash> = BTreeMap::new();
		for (stream, messages) in OutboundMessages::<T>::iter() {
			let mut frontier = OutboundFrontier::<T>::get(stream);
			for payload in &messages {
				frontier.append(leaf_hash(LEAF_VERSION, payload));
			}
			entries.insert(stream, frontier.root().0);
		}
		if entries.is_empty() {
			BlockStreamsRoot::<T>::put(None::<StreamsRoot>);
			return None;
		}
		for (stream, frontier) in OutboundFrontier::<T>::iter() {
			entries.entry(stream).or_insert_with(|| frontier.root().0);
		}

		let root = streams_root(&entries).expect("touched, so non-empty; qed");
		BlockStreamsRoot::<T>::put(Some(root));
		frame_system::Pallet::<T>::deposit_log(DigestItem::Consensus(
			SPMS_ENGINE_ID,
			root.encode(),
		));
		log::debug!(
			target: LOG_TARGET,
			"Committed streams root {root:?} over {} streams",
			entries.len(),
		);

		Some(root)
	}

	/// This block's committed [`StreamsRoot`], if any.
	pub fn current_streams_root() -> Option<StreamsRoot> {
		BlockStreamsRoot::<T>::get().flatten()
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

	/// Append `payloads` onto the stream's stored [`InboundFrontier`], record the [`Interval`], and
	/// republish the channel's register with the new watermark. Only streams of an accepted, live
	/// inbound channel are consumed. Order/count need no check — a deviation yields an endpoint no
	/// lift can bind.
	fn consume_channel_item(
		touched: &mut BTreeSet<(ParaId, StreamId)>,
		source: ParaId,
		stream: StreamId,
		payloads: Vec<Payload>,
	) -> Result<(), Error<T>> {
		let StreamId::Channel { recipient, domain, num } = stream else {
			return Err(Error::<T>::UnknownStream);
		};
		ensure!(recipient == T::SelfParaId::get(), Error::<T>::UnknownStream);
		let channel = ChannelId { peer: source, domain, num };
		let mut state = InChannels::<T>::get(channel).ok_or(Error::<T>::UnknownStream)?;
		ensure!(!state.suspended && !state.published.closed, Error::<T>::UnknownStream);
		Self::check_touch(touched, source, stream, &payloads)?;

		let mut frontier = InboundFrontier::<T>::get((source, stream));
		let start = frontier.root();
		for payload in &payloads {
			let position = MessagePosition(frontier.leaf_count());
			frontier
				.try_append(leaf_hash(LEAF_VERSION, payload))
				.map_err(|_| Error::<T>::BadFrontier)?;
			// Route `Data`, apply signals. A non-`SpecMsgKind` payload is a valid leaf regardless,
			// so it is consumed-and-dropped.
			match SpecMsgKind::decode_all(&mut &payload[..]) {
				Ok(SpecMsgKind::Data(data)) => {
					T::DataHandler::on_data(source, stream, position, data)
				},
				Ok(SpecMsgKind::Signal(signal)) => Self::apply_signal(&mut state, signal),
				Err(_) => {},
			}
		}
		InboundFrontier::<T>::insert((source, stream), &frontier);
		ConsumptionOutbox::<T>::append((source, stream, Interval { start, end: frontier }));

		// Consumption moved the watermark: publish it, so the sender regains credit and can prune.
		// A stream is consumed at most once per block, so this is at most one publish per channel
		// per block.
		Self::publish_register(&channel, &mut state)?;
		InChannels::<T>::insert(channel, state);
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
		// A read of the peer's `Ack` stream is a register read: it must target one of our
		// outbound channels (in any phase: a closed channel's watermark still reports consumption)
		// and be exactly one decodable `Register` leaf, the head.
		let register_read = match stream {
			StreamId::Ack { recipient, domain, num } => {
				ensure!(recipient == T::SelfParaId::get(), Error::<T>::UnknownStream);
				let channel = ChannelId { peer: source, domain, num };
				ensure!(OutChannels::<T>::contains_key(channel), Error::<T>::UnknownStream);
				let [leaf] = payloads.as_slice() else { return Err(Error::<T>::BadRegister) };
				let register =
					Register::decode_all(&mut &leaf[..]).map_err(|_| Error::<T>::BadRegister)?;
				Some((channel, register))
			},
			_ => None,
		};
		Self::check_touch(touched, source, stream, &payloads)?;

		if let Some(highwater) = InboundHighwater::<T>::get((source, stream)) {
			ensure!(base.0 > highwater, Error::<T>::Replay);
		}
		let mut frontier =
			MmrFrontier::from_parts(start_peaks, base.0).ok_or(Error::<T>::BadFrontier)?;
		let start = frontier.root();
		// The hints can put the frontier at the ceiling: a frontier past it would not decode, and
		// would read back as an empty record.
		for payload in &payloads {
			frontier
				.try_append(leaf_hash(LEAF_VERSION, payload))
				.map_err(|_| Error::<T>::BadFrontier)?;
		}
		InboundHighwater::<T>::insert(
			(source, stream),
			base.0.saturating_add(payloads.len() as u64).saturating_sub(1),
		);
		ConsumptionOutbox::<T>::append((source, stream, Interval { start, end: frontier }));
		*gaps += 1;
		// The highwater above orders reads by position, so this register is newer than any read
		// before it.
		if let Some((channel, register)) = register_read {
			Self::apply_register_read(&channel, register);
		}
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

	/// [`Config::MaxTouchedStreams`] and [`Config::MaxContextGaps`].
	pub fn consumption_caps() -> (u32, u32) {
		(T::MaxTouchedStreams::get(), T::MaxContextGaps::get())
	}

	/// This block's consumption grouped by source, per source in [`StreamId`] order.
	///
	/// # Panics
	///
	/// If the stored record does not decode. `get` would read it as empty, and an empty record
	/// emits no `Requires`, so nothing the block consumed would be checked. Every entry is
	/// written decodable, so this never fires; if it does, the candidate is invalid instead.
	pub fn consumption_record() -> ConsumptionRecord {
		let outbox = ConsumptionOutbox::<T>::get();
		assert_eq!(
			outbox.len(),
			ConsumptionOutbox::<T>::decode_len().unwrap_or(0),
			"the consumption record is written decodable; qed"
		);
		let mut record = ConsumptionRecord::default();
		for (source, stream, interval) in outbox {
			record.entries.entry(source).or_default().insert(stream, interval);
		}
		record
	}

	/// The inbound streams the node should fetch, per source in [`StreamId`] order, with their
	/// fetch cursors: every accepted channel that is neither suspended nor closed.
	pub fn consumed_streams() -> BTreeMap<ParaId, Vec<ConsumedStream>> {
		let mut grouped = BTreeMap::<ParaId, BTreeMap<StreamId, ConsumedStream>>::new();
		for (channel, state) in InChannels::<T>::iter() {
			if state.suspended || state.published.closed {
				continue;
			}
			let stream = Self::inbound_stream(&channel);
			let cursor =
				MessagePosition(InboundFrontier::<T>::get((channel.peer, stream)).leaf_count());
			if let Some(consumed) = ConsumedStream::project(&stream, cursor) {
				grouped.entry(channel.peer).or_default().insert(stream, consumed);
			}
		}
		grouped
			.into_iter()
			.map(|(source, streams)| (source, streams.into_values().collect()))
			.collect()
	}

	/// Outbound channel views.
	pub fn out_channels() -> BTreeMap<ChannelId, OutChannelState> {
		OutChannels::<T>::iter().collect()
	}

	/// Inbound channel views.
	pub fn in_channels() -> BTreeMap<ChannelId, InChannelState> {
		InChannels::<T>::iter().collect()
	}

	/// The data stream of an outbound channel: our key space, addressed to the peer.
	pub fn outbound_stream(channel: &ChannelId) -> StreamId {
		StreamId::Channel { recipient: channel.peer, domain: channel.domain, num: channel.num }
	}

	/// The data stream of an inbound channel: the peer's key space, addressed to us.
	pub fn inbound_stream(channel: &ChannelId) -> StreamId {
		StreamId::Channel {
			recipient: T::SelfParaId::get(),
			domain: channel.domain,
			num: channel.num,
		}
	}

	/// The `Ack` stream we publish an inbound channel's register on: our key space, addressed to
	/// the channel's sender.
	pub fn ack_stream(channel: &ChannelId) -> StreamId {
		StreamId::Ack { recipient: channel.peer, domain: channel.domain, num: channel.num }
	}

	/// Send `data` on an outbound channel as a [`SpecMsgKind::Data`] leaf, returning its position.
	/// The channel must be `Open` with credit left in the peer's granted window. On error, nothing
	/// changes.
	pub fn send(channel: ChannelId, data: Vec<u8>) -> Result<MessagePosition, Error<T>> {
		let state = OutChannels::<T>::get(channel).ok_or(Error::<T>::ChannelNotOpen)?;
		ensure!(state.phase() == ChannelPhase::Open, Error::<T>::ChannelNotOpen);
		Self::ensure_credit(&channel, &state)?;
		let payload = SpecMsgKind::Data(data).encode();
		let size = payload.len() as u32;
		let position = Self::append_to_stream(Self::outbound_stream(&channel), payload)?;
		OutChannelsMeta::<T>::mutate(channel, |meta| meta.account_send(size));
		Ok(position)
	}

	/// Append a lifecycle signal to an outbound channel's data stream. Signals count against the
	/// window like any message; gating is the caller's (`OpenChannel` is exempt).
	fn send_signal(
		channel: &ChannelId,
		signal: SpecMsgSignal,
	) -> Result<MessagePosition, Error<T>> {
		let payload = SpecMsgKind::Signal(signal).encode();
		let size = payload.len() as u32;
		let position = Self::append_to_stream(Self::outbound_stream(channel), payload)?;
		OutChannelsMeta::<T>::mutate(channel, |meta| meta.account_send(size));
		Ok(position)
	}

	/// The credit gate: in-flight count and bytes must both be below the peer's grant. The grant
	/// is advice; honoring it protects our archive and surfaces backpressure to the caller.
	fn ensure_credit(channel: &ChannelId, state: &OutChannelState) -> Result<(), Error<T>> {
		let grant = state.register.map(|register| register.grant).unwrap_or_default();
		let meta = OutChannelsMeta::<T>::get(channel);
		ensure!((meta.sizes.len() as u64) < u64::from(grant.max_messages), Error::<T>::NoCredit);
		ensure!(meta.bytes < grant.max_bytes, Error::<T>::NoCredit);
		Ok(())
	}

	/// Publish an inbound channel's register on our `Ack` stream: the consumption watermark, the
	/// grant (zero while suspended or closed), and the closed flag. Updates `state.published`; the
	/// caller stores `state`.
	fn publish_register(channel: &ChannelId, state: &mut InChannelState) -> Result<(), Error<T>> {
		let stream = Self::inbound_stream(channel);
		let up_to = MessagePosition(InboundFrontier::<T>::get((channel.peer, stream)).leaf_count());
		let grant = if state.suspended || state.published.closed {
			WindowGrant::default()
		} else {
			T::DefaultWindowGrant::get()
		};
		let register =
			Register { version: PROTOCOL_VERSION, up_to, grant, closed: state.published.closed };
		Self::append_to_stream(Self::ack_stream(channel), register.encode())?;
		state.published = register;
		Ok(())
	}

	/// Apply a lifecycle signal consumed from an inbound channel. `CloseChannel` needs nothing
	/// here: the sender stops, and the republish after this consumption reports the final
	/// watermark.
	fn apply_signal(state: &mut InChannelState, signal: SpecMsgSignal) {
		match signal {
			// A (re)open announces the sender's version as is; a lower value after a reopen is a
			// genuine downgrade.
			SpecMsgSignal::OpenChannel { version } => state.peer_version = version,
			// Mid-channel raises are monotonic; a lower value is ignored.
			SpecMsgSignal::Upgrade { version } => {
				state.peer_version = state.peer_version.max(version)
			},
			SpecMsgSignal::CloseChannel => {},
		}
	}

	/// Apply a register read to its outbound channel: refresh the grant and release the in-flight
	/// messages below the watermark. A register whose watermark or version goes backwards breaks
	/// the protocol and is ignored; the previous read stands.
	fn apply_register_read(channel: &ChannelId, register: Register) {
		let Some(mut state) = OutChannels::<T>::get(channel) else { return };
		if let Some(previous) = state.register {
			if register.up_to < previous.up_to || register.version < previous.version {
				return;
			}
		}
		OutChannelsMeta::<T>::mutate(channel, |meta| meta.confirm(register.up_to));
		state.register = Some(register);
		OutChannels::<T>::insert(channel, state);
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
