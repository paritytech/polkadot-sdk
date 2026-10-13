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
	ConsumptionRecord, InChannelState, Interval, MMRExtensionProof, MessagePosition,
	MessagingInherentData, MmrFrontier, OutChannelState, Payload, ProvideConsumptionRecord, Register,
	SpecMsgKind, SpecMsgSignal, StreamId, StreamsRoot, WindowGrant, INHERENT_IDENTIFIER,
	LEAF_VERSION, SPMS_ENGINE_ID,
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
	/// Position on the peer's `Ack` stream of the last applied register. The lift binds it and
	/// the stream only grows, so an older register is never applied over a newer one.
	pub read_at: Option<MessagePosition>,
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
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
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

		/// Origin allowed to accept and close an inbound channel. Must be privileged: acceptance
		/// creates permanent state this pallet does not price.
		type AcceptChannelOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Origin allowed to suspend and resume an inbound channel, and to skip a stalled one
		/// ahead ([`Pallet::skip_inbound_stream`]).
		type ChannelManagementOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The send-window credit every published register grants.
		type DefaultWindowGrant: Get<WindowGrant>;

		/// Local cap on each outbound channel's in-flight messages and bytes, applied on top of
		/// the peer's grant. It bounds the per-channel bookkeeping and the archive's unconfirmed
		/// tail whatever the peer grants.
		type MaxInFlight: Get<WindowGrant>;
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

	/// Whether `enact_messages` ran this block; cleared next block.
	#[pallet::storage]
	pub type Enacted<T: Config> = StorageValue<_, (), OptionQuery>;

	/// This block's consumption intervals; grouped/sorted by [`Pallet::consumption_record`],
	/// cleared next block. Bounded by [`Config::MaxTouchedStreams`].
	#[pallet::storage]
	#[pallet::unbounded]
	pub type ConsumptionOutbox<T: Config> =
		StorageValue<_, Vec<(ParaId, StreamId, Interval)>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Governance moved an inbound channel's consumption frontier ahead without consuming the
		/// leaves in between: `from..to` were never delivered.
		StreamSkipped { channel: ChannelId, from: MessagePosition, to: MessagePosition },
		/// A consumed leaf did not decode as a `SpecMsgKind`. It is consumed anyway (it is a
		/// valid leaf) and dropped.
		UndecodableLeaf { channel: ChannelId, position: MessagePosition },
		/// A consumed leaf exceeds [`Config::MaxMsgLen`]: the sender ignored our grant's
		/// `max_message_size`. It is consumed anyway and dropped, so a peer cannot stall the
		/// channel or fail the block.
		OversizedLeaf { channel: ChannelId, position: MessagePosition },
		/// An inherent item was not consumed: earlier in the block, after the collator planned
		/// it, the channel was suspended or closed, or its stream skipped (a scheduled call).
		ItemDropped { channel: ChannelId },
	}

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
		/// An `Events` item's `(start_peaks, base)`, or a skip's claimed frontier, is not a valid
		/// frontier, or an item's payloads would carry a frontier past `MAX_MMR_LEAF_COUNT`.
		BadFrontier,
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
		/// No such channel.
		UnknownChannel,
		/// The channel is already closed from this side.
		AlreadyClosed,
		/// The inbound channel is already suspended.
		AlreadySuspended,
		/// The inbound channel is not suspended.
		NotSuspended,
		/// The stream was already consumed this block.
		StreamTouched,
		/// The claimed frontier is not ahead of ours, or the extension proof does not extend ours
		/// to it.
		BadExtension,
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
			assert!(
				T::DefaultWindowGrant::get().max_message_size <= T::MaxMsgLen::get(),
				"`DefaultWindowGrant::max_message_size` must not exceed `MaxMsgLen`",
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
			ensure!(!Enacted::<T>::exists(), Error::<T>::AlreadyEnacted);
			Enacted::<T>::put(());

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
						.map(|()| true)
					},
				};
				// The block fails with only the error name, and its events roll back with it:
				// name the item.
				let consumed = result.map_err(|error| {
					log::warn!(
						target: LOG_TARGET,
						"Invalid inherent item from {source:?} on {stream:?}: {error:?}",
					);
					error
				})?;
				if consumed {
					log::debug!(
						target: LOG_TARGET,
						"Consumed {count} payloads from {source:?} on {stream:?}",
					);
				}
			}
			Ok(())
		}

		/// Open the outbound channel `(recipient, domain, num)` by appending `OpenChannel`, the
		/// only leaf sendable without credit. It stays `Opening` until the recipient's register
		/// is read. A reopen after our own close is `Open` at once; after the peer's close it
		/// waits for a new register.
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
			let previous = OutChannels::<T>::get(channel);
			match &previous {
				Some(state) => {
					ensure!(state.phase() == ChannelPhase::Closed, Error::<T>::AlreadyOpen)
				},
				None => {
					// First open: anchor the in-flight window at the stream's next position. The
					// `OpenChannel` leaf appended below is its first message.
					let stream = Self::outbound_stream(&channel);
					let next = OutboundFrontier::<T>::get(stream).leaf_count().saturating_add(
						OutboundMessages::<T>::decode_len(stream).unwrap_or(0) as u64,
					);
					OutChannelsMeta::<T>::mutate(channel, |meta| meta.base = MessagePosition(next));
				},
			}

			Self::send_signal(&channel, SpecMsgSignal::OpenChannel { version: PROTOCOL_VERSION })?;
			OutChannels::<T>::insert(
				channel,
				OutChannelState {
					closed_by_us: false,
					announced_version: PROTOCOL_VERSION,
					// The last register survives a reopen: after our close it still carries live
					// credit; after the peer's close it keeps the channel `Closed` until a fresh
					// register is read.
					register: previous.and_then(|state| state.register),
				},
			);
			Ok(())
		}

		/// Accept the inbound channel `(sender, domain, num)`, or re-accept it after we closed it.
		/// Its data stream joins [`Pallet::consumed_streams`], and a register is published on our
		/// `Ack` stream: the acceptance as the sender sees it. Either order works; accepting first
		/// is pre-authorization. Rejecting is never accepting, which costs nothing.
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
			let mut state = match InChannels::<T>::get(channel) {
				// Only a channel we closed can be accepted again.
				Some(mut state) => {
					ensure!(state.published.closed, Error::<T>::AlreadyAccepted);
					state.published.closed = false;
					state
				},
				None => InChannelState {
					published: Register {
						version: PROTOCOL_VERSION,
						up_to: MessagePosition(0),
						grant: WindowGrant::default(),
						closed: false,
					},
					peer_version: 0,
					suspended: false,
				},
			};
			Self::publish_register(&channel, &mut state)?;
			InChannels::<T>::insert(channel, state);
			Ok(())
		}

		/// Close our side of the outbound channel: append `CloseChannel` and stop sending. The
		/// signal is an ordinary message, so it needs an `Open` channel and credit. Closing is
		/// advisory and safe at any time; [`Pallet::open_channel`] reopens over the same stream.
		/// With no credit left, just stop sending: abandonment needs no signal.
		#[pallet::call_index(3)]
		#[pallet::weight(T::DbWeight::get().reads_writes(3, 3))]
		pub fn close_channel(
			origin: OriginFor<T>,
			recipient: ParaId,
			domain: u8,
			num: u16,
		) -> DispatchResult {
			T::OpenChannelOrigin::ensure_origin(origin)?;
			let channel = ChannelId { peer: recipient, domain, num };
			let mut state = OutChannels::<T>::get(channel).ok_or(Error::<T>::UnknownChannel)?;
			ensure!(!state.closed_by_us, Error::<T>::AlreadyClosed);
			ensure!(state.phase() == ChannelPhase::Open, Error::<T>::ChannelNotOpen);
			let size = SpecMsgKind::Signal(SpecMsgSignal::CloseChannel).encoded_size();
			Self::ensure_credit(&channel, &state, size)?;
			Self::send_signal(&channel, SpecMsgSignal::CloseChannel)?;
			state.closed_by_us = true;
			OutChannels::<T>::insert(channel, state);
			Ok(())
		}

		/// Close the inbound channel from our side: publish a register with `closed` set (no
		/// grant; `up_to` still reports what we consumed) and stop consuming it. The frontier is
		/// kept, so [`Pallet::accept_open_channel`] later resumes where consumption stopped.
		#[pallet::call_index(4)]
		#[pallet::weight(T::DbWeight::get().reads_writes(3, 3))]
		pub fn close_inbound_channel(
			origin: OriginFor<T>,
			sender: ParaId,
			domain: u8,
			num: u16,
		) -> DispatchResult {
			T::AcceptChannelOrigin::ensure_origin(origin)?;
			let channel = ChannelId { peer: sender, domain, num };
			let mut state = InChannels::<T>::get(channel).ok_or(Error::<T>::UnknownChannel)?;
			ensure!(!state.published.closed, Error::<T>::AlreadyClosed);
			state.published.closed = true;
			Self::publish_register(&channel, &mut state)?;
			InChannels::<T>::insert(channel, state);
			Ok(())
		}

		/// Suspend the inbound channel: a pause, not a close. Consumption is refused,
		/// [`Pallet::consumed_streams`] omits the stream, and the published register grants
		/// zero. All state stays.
		#[pallet::call_index(5)]
		#[pallet::weight(T::DbWeight::get().reads_writes(3, 3))]
		pub fn suspend_inbound_channel(
			origin: OriginFor<T>,
			sender: ParaId,
			domain: u8,
			num: u16,
		) -> DispatchResult {
			T::ChannelManagementOrigin::ensure_origin(origin)?;
			let channel = ChannelId { peer: sender, domain, num };
			let mut state = InChannels::<T>::get(channel).ok_or(Error::<T>::UnknownChannel)?;
			ensure!(!state.suspended, Error::<T>::AlreadySuspended);
			state.suspended = true;
			Self::publish_register(&channel, &mut state)?;
			InChannels::<T>::insert(channel, state);
			Ok(())
		}

		/// Resume a suspended inbound channel: republish a real grant. Consumption restarts from
		/// the kept frontier.
		#[pallet::call_index(6)]
		#[pallet::weight(T::DbWeight::get().reads_writes(3, 3))]
		pub fn resume_inbound_channel(
			origin: OriginFor<T>,
			sender: ParaId,
			domain: u8,
			num: u16,
		) -> DispatchResult {
			T::ChannelManagementOrigin::ensure_origin(origin)?;
			let channel = ChannelId { peer: sender, domain, num };
			let mut state = InChannels::<T>::get(channel).ok_or(Error::<T>::UnknownChannel)?;
			ensure!(state.suspended, Error::<T>::NotSuspended);
			state.suspended = false;
			Self::publish_register(&channel, &mut state)?;
			InChannels::<T>::insert(channel, state);
			Ok(())
		}

		/// Stall recovery: move an inbound frontier to `(peaks, leaf_count)` past payloads that
		/// cannot be fetched; the skipped leaves are lost. `extension` must extend our frontier to
		/// exactly that root, so a skip only moves forward. Whether the sender committed that
		/// frontier is checked only by the candidate's lift: take the peaks from a committed root,
		/// or no block with this call can be included. Recorded as an [`Interval`] like any
		/// consumption.
		#[pallet::call_index(7)]
		#[pallet::weight(T::DbWeight::get().reads_writes(4, 4))]
		pub fn skip_inbound_stream(
			origin: OriginFor<T>,
			sender: ParaId,
			domain: u8,
			num: u16,
			peaks: Vec<Hash>,
			leaf_count: u64,
			extension: MMRExtensionProof,
		) -> DispatchResult {
			T::ChannelManagementOrigin::ensure_origin(origin)?;
			let channel = ChannelId { peer: sender, domain, num };
			let mut state = InChannels::<T>::get(channel).ok_or(Error::<T>::UnknownChannel)?;
			let stream = Self::inbound_stream(&channel);
			let outbox = ConsumptionOutbox::<T>::get();
			// One interval per stream per block: a stream the inherent or an earlier skip consumed
			// is already here. A skip before the inherent makes the inherent drop that item.
			ensure!(
				!outbox.iter().any(|(source, s, _)| *source == sender && *s == stream),
				Error::<T>::StreamTouched
			);
			ensure!(
				(outbox.len() as u32) < T::MaxTouchedStreams::get(),
				Error::<T>::TooManyStreams
			);

			let new = MmrFrontier::from_parts(peaks, leaf_count).ok_or(Error::<T>::BadFrontier)?;
			let old = InboundFrontier::<T>::get((sender, stream));
			ensure!(
				leaf_count > old.leaf_count() && extension.leaf_count == leaf_count,
				Error::<T>::BadExtension
			);
			let root = extension.verify(&old).map_err(|_| Error::<T>::BadExtension)?;
			ensure!(root == new.root(), Error::<T>::BadExtension);

			let (from, to) = (MessagePosition(old.leaf_count()), MessagePosition(leaf_count));
			InboundFrontier::<T>::insert((sender, stream), &new);
			ConsumptionOutbox::<T>::append((
				sender,
				stream,
				Interval { start: old.root(), end: new },
			));
			Self::publish_register(&channel, &mut state)?;
			InChannels::<T>::insert(channel, state);
			Self::deposit_event(Event::StreamSkipped { channel, from, to });
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
		Enacted::<T>::kill();
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
	///
	/// Crate-internal: it skips the channel and credit gates, and would let a caller forge `Signal`
	/// leaves. Other code sends through [`Pallet::send`] (design § Message Kinds).
	pub(crate) fn append_to_stream(
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
	/// lift can bind. Returns `false` if the item was dropped.
	fn consume_channel_item(
		touched: &mut BTreeSet<(ParaId, StreamId)>,
		source: ParaId,
		stream: StreamId,
		payloads: Vec<Payload>,
	) -> Result<bool, Error<T>> {
		let StreamId::Channel { recipient, domain, num } = stream else {
			return Err(Error::<T>::UnknownStream);
		};
		ensure!(recipient == T::SelfParaId::get(), Error::<T>::UnknownStream);
		let channel = ChannelId { peer: source, domain, num };
		let mut state = InChannels::<T>::get(channel).ok_or(Error::<T>::UnknownStream)?;
		Self::check_touch(touched, source, stream, &payloads)?;
		// The collator planned on the parent's state. A scheduled call in `on_initialize` may have
		// suspended or closed the channel, or skipped its stream, since: drop the item rather than
		// fail the block, which every retry would rebuild the same way. Nothing is recorded, so
		// nothing needs a lift.
		let skipped = ConsumptionOutbox::<T>::get()
			.iter()
			.any(|(s, t, _)| *s == source && *t == stream);
		if state.suspended || state.published.closed || skipped {
			Self::deposit_event(Event::ItemDropped { channel });
			return Ok(false);
		}

		let mut frontier = InboundFrontier::<T>::get((source, stream));
		let start = frontier.root();
		for payload in &payloads {
			let position = MessagePosition(frontier.leaf_count());
			frontier
				.try_append(leaf_hash(LEAF_VERSION, payload))
				.map_err(|_| Error::<T>::BadFrontier)?;
			if payload.len() > T::MaxMsgLen::get() as usize {
				Self::deposit_event(Event::OversizedLeaf { channel, position });
				continue;
			}
			// Route `Data`, apply signals. A non-`SpecMsgKind` payload is a valid leaf regardless,
			// so it is consumed, dropped and reported.
			match SpecMsgKind::decode_all(&mut &payload[..]) {
				Ok(SpecMsgKind::Data(data)) => {
					T::DataHandler::on_data(source, stream, position, data)
				},
				Ok(SpecMsgKind::Signal(signal)) => Self::apply_signal(&mut state, signal),
				Err(_) => Self::deposit_event(Event::UndecodableLeaf { channel, position }),
			}
		}
		InboundFrontier::<T>::insert((source, stream), &frontier);
		ConsumptionOutbox::<T>::append((source, stream, Interval { start, end: frontier }));

		// Consumption moved the watermark: publish it, so the sender regains credit and can prune.
		// A stream is consumed at most once per block, so this is at most one publish per channel
		// per block.
		Self::publish_register(&channel, &mut state)?;
		InChannels::<T>::insert(channel, state);
		Ok(true)
	}

	/// Consume a register read: the head `Register` of the peer's `Ack` stream for one of our
	/// outbound channels, in any phase. Rebuild the frontier from the unproven hints (a lie binds
	/// no lift), record the [`Interval`] and apply the register. A read is never rejected for its
	/// position; [`Pallet::apply_register_read`] ignores an older one. Any other `Events` stream
	/// invalidates the block.
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
		let StreamId::Ack { recipient, domain, num } = stream else {
			return Err(Error::<T>::UnknownStream);
		};
		ensure!(recipient == T::SelfParaId::get(), Error::<T>::UnknownStream);
		let channel = ChannelId { peer: source, domain, num };
		ensure!(OutChannels::<T>::contains_key(channel), Error::<T>::UnknownStream);
		let [leaf] = payloads.as_slice() else { return Err(Error::<T>::BadRegister) };
		let register = Register::decode_all(&mut &leaf[..]).map_err(|_| Error::<T>::BadRegister)?;
		Self::check_touch(touched, source, stream, &payloads)?;

		let mut frontier =
			MmrFrontier::from_parts(start_peaks, base.0).ok_or(Error::<T>::BadFrontier)?;
		// The hints can put the frontier at the ceiling: a frontier past it would not decode, and
		// would read back as an empty record.
		frontier
			.try_append(leaf_hash(LEAF_VERSION, leaf))
			.map_err(|_| Error::<T>::BadFrontier)?;
		// A read advances nothing: the interval starts and ends at the context it was read
		// against. Two blocks that read the same head then chain with no gap, and a gap between
		// two heads is a forward extension to the later context, which the lift can prove.
		let start = frontier.root();
		ConsumptionOutbox::<T>::append((source, stream, Interval { start, end: frontier }));
		*gaps += 1;
		Self::apply_register_read(&channel, base, register);
		Ok(())
	}

	/// Per-item guards: not already touched, non-empty, within the cap.
	fn check_touch(
		touched: &mut BTreeSet<(ParaId, StreamId)>,
		source: ParaId,
		stream: StreamId,
		payloads: &[Payload],
	) -> Result<(), Error<T>> {
		ensure!(!touched.contains(&(source, stream)), Error::<T>::DuplicateStream);
		ensure!(!payloads.is_empty(), Error::<T>::EmptyItem);
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
		let payload = SpecMsgKind::Data(data).encode();
		Self::ensure_credit(&channel, &state, payload.len())?;
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

	/// The credit gate: in-flight count and bytes must both be below the peer's grant and
	/// [`Config::MaxInFlight`], and a leaf of `size` encoded bytes must fit the grant's
	/// `max_message_size`, since the peer drops a bigger one. The grant is advice; honoring it
	/// protects our archive and surfaces backpressure to the caller.
	fn ensure_credit(
		channel: &ChannelId,
		state: &OutChannelState,
		size: usize,
	) -> Result<(), Error<T>> {
		let grant = state.register.map(|register| register.grant).unwrap_or_default();
		ensure!(size as u64 <= u64::from(grant.max_message_size), Error::<T>::MessageTooBig);
		let cap = T::MaxInFlight::get();
		let meta = OutChannelsMeta::<T>::get(channel);
		ensure!(
			(meta.sizes.len() as u64) < u64::from(grant.max_messages.min(cap.max_messages)),
			Error::<T>::NoCredit
		);
		ensure!(meta.bytes < grant.max_bytes.min(cap.max_bytes), Error::<T>::NoCredit);
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

	/// Apply a register read at `position` on the peer's `Ack` stream: refresh the grant and
	/// release in-flight messages below the watermark. A register older than the last applied one,
	/// or whose watermark or version goes backwards, is ignored. The position check stops an older
	/// grant or `closed` flag from replacing a newer one.
	fn apply_register_read(channel: &ChannelId, position: MessagePosition, register: Register) {
		let Some(mut state) = OutChannels::<T>::get(channel) else { return };
		if let Some(previous) = state.register {
			if register.up_to < previous.up_to || register.version < previous.version {
				return;
			}
		}
		let mut meta = OutChannelsMeta::<T>::get(channel);
		if meta.read_at.is_some_and(|read_at| position < read_at) {
			return;
		}
		meta.read_at = Some(position);
		meta.confirm(register.up_to);
		OutChannelsMeta::<T>::insert(channel, meta);
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
