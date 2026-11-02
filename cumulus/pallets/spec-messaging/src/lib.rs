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
//! [`Call::enact_messages`] by recomputation into channel frontiers, writing the block's
//! [`ConsumptionRecord`]. The inherent carries no proofs; the PoV lift binds the endpoint.
//!
//! Lifecycle: block `N` stages its sends and consumption; at block end
//! [`Pallet::commit_streams_root`] publishes pending ack registers and folds the [`StreamsRoot`];
//! the first spec-msg action of `N+1`
//! ([`Pallet::roll_over`]) drains the sends and clears transient state.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{
	collections::{BTreeMap, BTreeSet},
	vec::Vec,
};
use codec::{Compact, DecodeAll, Encode, MaxEncodedLen};
use core::ops::RangeInclusive;
use cumulus_primitives_core::CumulusDigestItem;
use cumulus_primitives_spec_messaging::{
	leaf_hash_with_version, streams_root::streams_root, AppliedRegister, ChannelMessage,
	ConsumedStreams, ConsumptionRecord, InChannelId, InChannelState, Interval, LossyRead,
	MMRExtensionProof, MessagePosition, MessagingInherentData, MmrFrontier, MmrRoot, OpenSession,
	OutChannelId, OutChannelState, Payload, ProvideConsumptionRecord, Register, RegisterStatus,
	SessionParams, StreamId, StreamProof, StreamsRoot, INHERENT_IDENTIFIER, LEAF_VERSION,
	MAX_SPECULATIVE_MESSAGE_LEN, SPMS_ENGINE_ID,
};
use frame_support::{
	ensure,
	inherent::InherentData,
	pallet_prelude::Weight,
	traits::{EnsureOrigin, Get},
	BoundedVec,
};
use polkadot_core_primitives::Hash;
use polkadot_parachain_primitives::primitives::Id as ParaId;
use polkadot_primitives::v9::MAX_COMMITMENT_ENTRIES;
use sp_runtime::generic::DigestItem;

pub use pallet::*;

const LOG_TARGET: &str = "runtime::spec-msg";

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

/// Query the memory footprint of a channel's delivery queue.
pub trait QueueFootprintQuery<Id> {
	/// Number of ready pages in the channel's delivery queue.
	fn ready_pages(id: &Id) -> u32;
}

impl<Id> QueueFootprintQuery<Id> for () {
	fn ready_pages(_: &Id) -> u32 {
		0
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

		/// The in-flight gate: at most `MaxInFlight` leaves sent and unconfirmed by the peer's
		/// watermark.
		#[pallet::constant]
		type MaxInFlight: Get<u32>;

		/// Per-block cap on streams the inherent may touch; `<= MAX_COMMITMENT_ENTRIES`.
		#[pallet::constant]
		type MaxTouchedStreams: Get<u32>;

		/// Per-item cap on inbound messages consumed.
		#[pallet::constant]
		type MaxInboundMessagesPerItem: Get<u32>;

		/// Supported channel versions sent and parsed; advertised when accepting.
		#[pallet::constant]
		type SupportedVersions: Get<RangeInclusive<u8>>;

		/// Advertised maximum message size when accepting an inbound channel.
		#[pallet::constant]
		type MaxMessageSize: Get<u32>;

		/// Maximum allowed footprint (ready pages) for an inbound channel's delivery queue.
		#[pallet::constant]
		type MaxQueueFootprint: Get<u32>;

		/// Query the footprint of an inbound channel's delivery queue.
		type QueueFootprint: QueueFootprintQuery<InChannelId>;

		/// Sink for consumed `Data` payloads.
		type DataHandler: OnSpecMsgData;

		/// Origin allowed to open an outbound channel.
		type OpenChannelOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Origin allowed to accept and close an inbound channel.
		type AcceptChannelOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Origin allowed to suspend and resume an inbound channel.
		type ChannelManagementOrigin: EnsureOrigin<Self::RuntimeOrigin>;
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

	/// Sender side, per outbound channel.
	#[pallet::storage]
	pub type OutChannels<T: Config> =
		StorageMap<_, Twox64Concat, OutChannelId, OutChannelState, OptionQuery>;

	/// Receiver side, per inbound channel.
	#[pallet::storage]
	pub type InChannels<T: Config> =
		StorageMap<_, Twox64Concat, InChannelId, InChannelState, OptionQuery>;

	/// Whether `enact_messages` ran this block; cleared next block.
	#[pallet::storage]
	pub type Enacted<T: Config> = StorageValue<_, (), OptionQuery>;

	/// This block's consumption intervals; grouped/sorted by [`Pallet::consumption_record`],
	/// cleared next block. Bounded by [`Config::MaxTouchedStreams`].
	#[pallet::storage]
	#[pallet::unbounded]
	pub type ConsumptionOutbox<T: Config> =
		StorageValue<_, Vec<(ParaId, StreamId, Interval)>, ValueQuery>;

	/// Distinct source parachains touched across blocks in the candidate bundle. Cleared on index
	/// 0.
	#[pallet::storage]
	#[pallet::unbounded]
	pub type BundleSources<T: Config> = StorageValue<_, BTreeSet<ParaId>, ValueQuery>;

	/// Streams touched across blocks in the candidate bundle: a stream's first touch reserves its
	/// lift. Cleared on index 0.
	#[pallet::storage]
	#[pallet::unbounded]
	pub type BundleStreams<T: Config> = StorageValue<_, BTreeSet<(ParaId, StreamId)>, ValueQuery>;

	/// Inbound channels whose register may have changed; published at block end, and kept for the
	/// next block if the publish fails.
	#[pallet::storage]
	#[pallet::unbounded]
	pub type PendingRegisterPublishes<T: Config> =
		StorageValue<_, BTreeSet<InChannelId>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An outbound channel was opened or reopened.
		ChannelOpened { channel: OutChannelId },
		/// An inbound channel was accepted with session parameters.
		ChannelAccepted { channel: InChannelId, session: u32 },
		/// An outbound channel was closed from our side.
		ChannelClosed { channel: OutChannelId },
		/// An inbound channel was closed from our side.
		InboundChannelClosed { channel: InChannelId },
		/// An ack register was published at block end.
		RegisterPublished { channel: InChannelId, up_to: MessagePosition, status: RegisterStatus },
		/// A message was consumed in order.
		MessageConsumed { source: ParaId, stream: StreamId, position: MessagePosition },
	}

	#[pallet::error]
	#[derive(PartialEq, Eq)]
	pub enum Error<T> {
		/// Payload over [`Config::MaxMsgLen`] or session `max_message_size`.
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
		/// An item's payloads or frontier would violate MMR rules or exceed bounds.
		BadFrontier,
		/// This block's `StreamsRoot` is already committed; a later send could not be served.
		RootCommitted,
		/// `enact_messages` already ran this block.
		AlreadyEnacted,
		/// A channel to this chain itself.
		ChannelToSelf,
		/// The channel is already open or accepted.
		AlreadyOpen,
		/// The inbound channel is already accepted.
		AlreadyAccepted,
		/// The channel is not open or active in supported versions.
		NotOpen,
		/// The send would exceed the `MaxInFlight` credit gate.
		NoCredit,
		/// A register read is not exactly one leaf, or does not decode as a [`Register`].
		BadRegister,
		/// No such channel exists.
		UnknownChannel,
		/// The channel is already closed from this side.
		AlreadyClosed,
		/// Send attempted before pallet initialization (e.g. during runtime upgrade).
		NotInitialized,
		/// Session counter overflowed.
		SessionOverflow,
		/// An inherent item violated protocol admission rules.
		NotAdmitted,
		/// Inbound messages in one item exceed [`Config::MaxInboundMessagesPerItem`].
		TooManyInboundMessages,
		/// Candidate bundle exceeded `MAX_COMMITMENT_ENTRIES` distinct sources.
		MaxCommitmentEntriesExceeded,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
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
				1 <= T::MaxInFlight::get() && T::MaxInFlight::get() < T::MaxMessagesPerBlock::get(),
				"1 <= MaxInFlight < MaxMessagesPerBlock required",
			);
			assert!(
				T::MaxTouchedStreams::get() <= MAX_COMMITMENT_ENTRIES,
				"`MaxTouchedStreams` must not exceed `MAX_COMMITMENT_ENTRIES`",
			);
			assert!(
				T::MaxMessageSize::get() <= T::MaxMsgLen::get(),
				"`MaxMessageSize` must not exceed `MaxMsgLen`",
			);
			// Receivers refuse to fetch a longer leaf, which would stall its channel for good.
			assert!(
				T::MaxMsgLen::get() <= MAX_SPECULATIVE_MESSAGE_LEN,
				"`MaxMsgLen` must not exceed `MAX_SPECULATIVE_MESSAGE_LEN`",
			);
			assert!(
				!T::SupportedVersions::get().is_empty(),
				"`SupportedVersions` must not be empty",
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
			ensure!(!Enacted::<T>::exists(), Error::<T>::AlreadyEnacted);
			Enacted::<T>::put(());

			let us = T::SelfParaId::get();
			let mut budget = BundleBudget::load::<T>();

			for (channel_id, payloads) in data.channels {
				ensure!(!payloads.is_empty(), Error::<T>::EmptyItem);
				ensure!(
					payloads.len() as u32 <= T::MaxInboundMessagesPerItem::get(),
					Error::<T>::TooManyInboundMessages
				);
				let stream = channel_id.sender_data_stream(us);
				let bytes = Self::lift_reservation(&budget, stream, None)?;
				Self::consume_channel_item(channel_id, payloads)?;
				budget.record(stream, bytes);
			}

			for (channel_id, lossy) in data.registers {
				let stream = channel_id.peer_ack_stream(us);
				let skip = Self::consume_register_item(channel_id, lossy)?;
				let bytes = Self::lift_reservation(&budget, stream, skip)?;
				budget.record(stream, bytes);
			}

			// Broadcast subscriptions are not supported yet.
			ensure!(data.events.is_empty(), Error::<T>::NotAdmitted);

			BundleSources::<T>::put(&budget.sources);
			BundleStreams::<T>::put(&budget.streams);
			// Lifts are attached after the block is built, so their proof size is charged here.
			// Extra weight is outside the call's declared weight, so weight reclaim cannot refund
			// it.
			frame_system::Pallet::<T>::register_extra_weight_unchecked(
				Weight::from_parts(0, budget.reserved),
				DispatchClass::Mandatory,
			);
			Ok(())
		}

		/// Open or reopen an outbound channel to `recipient`.
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
			let channel = OutChannelId { recipient, domain, num };
			let mut state = OutChannels::<T>::get(channel).unwrap_or_default();
			ensure!(!state.want_open, Error::<T>::AlreadyOpen);
			state.want_open = true;

			let _ = Self::evaluate_hello_rule(&channel, &mut state);
			OutChannels::<T>::insert(channel, state);
			Self::deposit_event(Event::ChannelOpened { channel });
			Ok(())
		}

		/// Accept an inbound channel from `sender`, allocating or stepping its session.
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
			let channel = InChannelId { sender, domain, num };
			let mut state = InChannels::<T>::get(channel).unwrap_or_else(|| InChannelState {
				frontier: MmrFrontier::new(),
				session: 0,
				status: RegisterStatus::Closed { session: 0 },
				published: None,
				open: None,
				draining: false,
			});

			ensure!(
				!matches!(state.status, RegisterStatus::Opened) && !state.draining,
				Error::<T>::AlreadyOpen
			);

			let session = state.session.checked_add(1).ok_or(Error::<T>::SessionOverflow)?;
			let supported = T::SupportedVersions::get();
			state.session = session;
			state.status = RegisterStatus::Accepting {
				session,
				min: *supported.start(),
				max: *supported.end(),
				max_message_size: T::MaxMessageSize::get(),
			};
			state.open = None;
			state.draining = false;

			InChannels::<T>::insert(channel, state);
			PendingRegisterPublishes::<T>::mutate(|set| set.insert(channel));
			Self::deposit_event(Event::ChannelAccepted { channel, session });
			Ok(())
		}

		/// Close an outbound channel from our side.
		#[pallet::call_index(3)]
		#[pallet::weight(T::DbWeight::get().reads_writes(3, 3))]
		pub fn close_channel(
			origin: OriginFor<T>,
			recipient: ParaId,
			domain: u8,
			num: u16,
		) -> DispatchResult {
			T::OpenChannelOrigin::ensure_origin(origin)?;
			let channel = OutChannelId { recipient, domain, num };
			let mut state = OutChannels::<T>::get(channel).ok_or(Error::<T>::UnknownChannel)?;
			ensure!(state.want_open, Error::<T>::AlreadyClosed);
			state.want_open = false;

			let session = state.session;
			if state.took_part_in(session) && !state.closed(session) {
				let close = ChannelMessage::CloseChannel { session };
				Self::append_to_stream(channel.data_stream(), close.encode())?;
				state.closed_session = Some(session);
			}
			OutChannels::<T>::insert(channel, state);
			Self::deposit_event(Event::ChannelClosed { channel });
			Ok(())
		}

		/// Close an inbound channel from our side.
		#[pallet::call_index(4)]
		#[pallet::weight(T::DbWeight::get().reads_writes(3, 3))]
		pub fn close_inbound_channel(
			origin: OriginFor<T>,
			sender: ParaId,
			domain: u8,
			num: u16,
		) -> DispatchResult {
			T::AcceptChannelOrigin::ensure_origin(origin)?;
			let channel = InChannelId { sender, domain, num };
			let mut state = InChannels::<T>::get(channel).ok_or(Error::<T>::UnknownChannel)?;
			ensure!(
				!matches!(state.status, RegisterStatus::Closed { .. }) && !state.draining,
				Error::<T>::AlreadyClosed
			);

			if matches!(state.status, RegisterStatus::Opened) {
				state.status = RegisterStatus::Closed { session: state.session };
				state.draining = true;
			} else {
				state.status = RegisterStatus::Closed { session: state.session };
				state.draining = false;
				state.open = None;
			}

			InChannels::<T>::insert(channel, state);
			PendingRegisterPublishes::<T>::mutate(|set| set.insert(channel));
			Self::deposit_event(Event::InboundChannelClosed { channel });
			Ok(())
		}
	}

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = MakeFatalError<()>;
		const INHERENT_IDENTIFIER: InherentIdentifier =
			cumulus_primitives_spec_messaging::INHERENT_IDENTIFIER;

		fn create_inherent(inherent_data: &InherentData) -> Option<Self::Call> {
			let data = inherent_data
				.get_data::<MessagingInherentData>(&INHERENT_IDENTIFIER)
				.ok()
				.flatten()?;
			if data.is_empty() {
				return None;
			}

			let us = T::SelfParaId::get();
			let mut budget = BundleBudget::load::<T>();
			let mut remaining = Self::proof_budget(inherent_data);
			let mut filtered_channels = BTreeMap::new();

			for (channel_id, payloads) in data.channels {
				let Some(mut admitted) = Self::admit_channel_item(channel_id, &payloads) else {
					continue;
				};
				let stream = channel_id.sender_data_stream(us);
				let Ok(bytes) = Self::lift_reservation(&budget, stream, None) else {
					continue;
				};
				// Any prefix of an admitted prefix is admitted, so trimming to the budget is safe.
				let mut cost = bytes
					.saturating_add(channel_id.encoded_size() as u64)
					.saturating_add(Compact::<u32>::max_encoded_len() as u64);
				let mut keep = 0;
				for payload in &admitted.payloads {
					let next = cost.saturating_add(payload.encoded_size() as u64);
					if next > remaining {
						break;
					}
					cost = next;
					keep += 1;
				}
				if keep == 0 {
					continue;
				}
				admitted.payloads.truncate(keep);
				remaining -= cost;
				budget.record(stream, bytes);
				filtered_channels.insert(channel_id, admitted.payloads);
			}

			let mut filtered_registers = BTreeMap::new();
			for (channel_id, lossy) in data.registers {
				let Ok(skip) = Self::admit_register_read(channel_id, &lossy) else {
					continue;
				};
				let stream = channel_id.peer_ack_stream(us);
				let Ok(bytes) = Self::lift_reservation(&budget, stream, skip) else {
					continue;
				};
				let cost = bytes.saturating_add((channel_id, &lossy).encoded_size() as u64);
				if cost > remaining {
					continue;
				}
				remaining -= cost;
				budget.record(stream, bytes);
				filtered_registers.insert(channel_id, lossy);
			}

			let filtered = MessagingInherentData {
				channels: filtered_channels,
				registers: filtered_registers,
				events: BTreeMap::new(),
			};

			if filtered.is_empty() {
				None
			} else {
				Some(Call::enact_messages { data: filtered })
			}
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::enact_messages { .. })
		}
	}
}

/// Weight of one `enact_messages`.
fn enact_weight<T: Config>(data: &MessagingInherentData) -> Weight {
	let items_count = (data.channels.len() + data.registers.len() + data.events.len()) as u64;
	T::DbWeight::get().reads_writes(2, 2).saturating_mul(1 + items_count)
}

/// The candidate bundle's touched sources and streams, plus this inherent's touches and the proof
/// bytes it reserved for lifts.
struct BundleBudget {
	touched: u32,
	sources: BTreeSet<ParaId>,
	streams: BTreeSet<(ParaId, StreamId)>,
	reserved: u64,
}

impl BundleBudget {
	fn load<T: Config>() -> Self {
		Self {
			touched: 0,
			sources: BundleSources::<T>::get(),
			streams: BundleStreams::<T>::get(),
			reserved: 0,
		}
	}

	fn record(&mut self, stream: (ParaId, StreamId), bytes: u64) {
		self.touched += 1;
		self.sources.insert(stream.0);
		self.streams.insert(stream);
		self.reserved = self.reserved.saturating_add(bytes);
	}
}

/// Decodes to the length of an inherent's raw data, so other inherents can be sized without
/// knowing their types.
struct EncodedLen(usize);

impl codec::Decode for EncodedLen {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		Ok(Self(input.remaining_len()?.unwrap_or(0)))
	}
}

/// Result of evaluating an inbound channel item against admission rules.
struct AdmittedChannelItem {
	/// The frontier before the item: its root and leaf count.
	start: MmrRoot,
	start_leaf_count: u64,
	/// The channel state after the admitted prefix.
	state: InChannelState,
	payloads: Vec<Payload>,
	data_messages: Vec<(MessagePosition, Vec<u8>)>,
}

impl<T: Config> Pallet<T> {
	/// Start this block's messaging state once: forget the previous block's root and consumption,
	/// and drain its sends. Returns the number drained.
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

		let is_first =
			CumulusDigestItem::find_block_bundle_info(&frame_system::Pallet::<T>::digest())
				.map_or(true, |info| info.index == 0);
		if is_first {
			BundleSources::<T>::kill();
			BundleStreams::<T>::kill();
		}

		let mut drained = 0u32;
		for (stream, messages) in OutboundMessages::<T>::drain() {
			let mut frontier = OutboundFrontier::<T>::get(stream);
			for payload in &messages {
				frontier.append(leaf_hash_with_version(LEAF_VERSION, payload));
				drained = drained.saturating_add(1);
			}
			OutboundFrontier::<T>::insert(stream, frontier);
		}
		RolledOver::<T>::put((now, drained));
		drained
	}

	/// Append `payload` to `stream`'s outbound MMR, returning its position.
	pub(crate) fn append_to_stream(
		stream: StreamId,
		payload: Vec<u8>,
	) -> Result<MessagePosition, Error<T>> {
		let payload: BoundedVec<u8, T::MaxMsgLen> =
			payload.try_into().map_err(|_| Error::<T>::MessageTooBig)?;

		Self::roll_over();
		ensure!(!BlockStreamsRoot::<T>::exists(), Error::<T>::RootCommitted);
		let index = OutboundMessages::<T>::decode_len(stream).unwrap_or(0) as u64;
		OutboundMessages::<T>::try_append(stream, payload)
			.map_err(|()| Error::<T>::TooManyMessages)?;

		Ok(MessagePosition(OutboundFrontier::<T>::get(stream).leaf_count() + index))
	}

	/// Fold this block's sends into the [`StreamsRoot`], memoize it, and deposit the digest.
	pub fn commit_streams_root() -> Option<StreamsRoot> {
		Self::roll_over();
		if let Some(committed) = BlockStreamsRoot::<T>::get() {
			return committed;
		}

		Self::publish_pending_registers();

		let mut entries: BTreeMap<StreamId, Hash> = BTreeMap::new();
		for (stream, messages) in OutboundMessages::<T>::iter() {
			let mut frontier = OutboundFrontier::<T>::get(stream);
			for payload in &messages {
				frontier.append(leaf_hash_with_version(LEAF_VERSION, payload));
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
			"Block {:?}: committed streams root {root:?} over {} streams",
			frame_system::Pallet::<T>::block_number(),
			entries.len(),
		);

		Some(root)
	}

	/// This block's committed [`StreamsRoot`], if any.
	pub fn current_streams_root() -> Option<StreamsRoot> {
		BlockStreamsRoot::<T>::get().flatten()
	}

	/// Publish any pending ack registers for channels modified this block.
	fn publish_pending_registers() {
		let pending = PendingRegisterPublishes::<T>::take();
		for channel in pending {
			if let Some(mut state) = InChannels::<T>::get(channel) {
				let current = Register {
					up_to: MessagePosition(state.frontier.leaf_count()),
					status: state.status,
				};
				if state.published.as_ref() != Some(&current) {
					let stream = channel.ack_stream();
					if Self::append_to_stream(stream, current.encode()).is_ok() {
						state.published = Some(current);
						InChannels::<T>::insert(channel, state);
						Self::deposit_event(Event::RegisterPublished {
							channel,
							up_to: current.up_to,
							status: current.status,
						});
					} else {
						PendingRegisterPublishes::<T>::mutate(|p| {
							p.insert(channel);
						});
					}
				}
			}
		}
	}

	/// Send `data` on an outbound channel.
	pub fn send(channel: OutChannelId, data: Vec<u8>) -> Result<MessagePosition, Error<T>> {
		ensure!(frame_system::Pallet::<T>::execution_phase().is_some(), Error::<T>::NotInitialized);

		let state = OutChannels::<T>::get(channel).ok_or(Error::<T>::NotOpen)?;
		let params =
			state.active_session(T::SupportedVersions::get()).ok_or(Error::<T>::NotOpen)?;

		let message = ChannelMessage::Data(data);
		let payload = message.encode();
		ensure!(payload.len() <= T::MaxMsgLen::get() as usize, Error::<T>::MessageTooBig);
		ensure!(payload.len() <= params.max_message_size as usize, Error::<T>::MessageTooBig);

		let stream = channel.data_stream();
		let sent = OutboundFrontier::<T>::get(stream).leaf_count() +
			OutboundMessages::<T>::decode_len(stream).unwrap_or(0) as u64;
		let up_to = state.register.map(|r| r.register.up_to.0).unwrap_or(0);
		ensure!(sent.saturating_sub(up_to) < T::MaxInFlight::get() as u64, Error::<T>::NoCredit);

		Self::append_to_stream(stream, payload)
	}

	/// Evaluate the Hello rule for an outbound channel, emitting `Hello` if due.
	fn evaluate_hello_rule(
		channel: &OutChannelId,
		state: &mut OutChannelState,
	) -> Result<bool, Error<T>> {
		if !state.want_open {
			return Ok(false);
		}
		let Some(applied) = &state.register else {
			return Ok(false);
		};
		let RegisterStatus::Accepting { session, min, max, max_message_size } =
			applied.register.status
		else {
			return Ok(false);
		};
		if state.took_part_in(session) {
			return Ok(false);
		}
		let supported = T::SupportedVersions::get();
		let version = (*supported.end()).min(max);
		if version < (*supported.start()).max(min) {
			return Ok(false);
		}
		let stream = channel.data_stream();
		let sent = OutboundFrontier::<T>::get(stream).leaf_count() +
			OutboundMessages::<T>::decode_len(stream).unwrap_or(0) as u64;
		let up_to = applied.register.up_to.0;
		if sent.saturating_sub(up_to) >= T::MaxInFlight::get() as u64 {
			return Ok(false);
		}

		let hello = ChannelMessage::Hello { session, version };
		Self::append_to_stream(stream, hello.encode())?;
		state.open = Some(OpenSession {
			session,
			params: SessionParams { version, max_message_size },
			opened: false,
		});
		Ok(true)
	}

	/// Checks the stream and source caps for touching `stream`, and returns the proof bytes the
	/// candidate attaches for it after the block is built. `skip` is a lossy read's gap from where
	/// the previous read ended.
	fn lift_reservation(
		budget: &BundleBudget,
		stream: (ParaId, StreamId),
		skip: Option<(u64, u64)>,
	) -> Result<u64, Error<T>> {
		ensure!(budget.touched < T::MaxTouchedStreams::get(), Error::<T>::TooManyStreams);
		let new_source = !budget.sources.contains(&stream.0);
		ensure!(
			!new_source || budget.sources.len() < MAX_COMMITMENT_ENTRIES as usize,
			Error::<T>::MaxCommitmentEntriesExceeded
		);

		let mut bytes = 0usize;
		if budget.sources.is_empty() {
			bytes += Compact(MAX_COMMITMENT_ENTRIES).encoded_size();
		}
		if new_source {
			bytes += ParaId::max_encoded_len() + Compact::<u32>::max_encoded_len();
		}
		if !budget.streams.contains(&stream) {
			// One lift without advances; a bundle has at most 256 blocks, so the `advances` length
			// prefix takes 2 bytes.
			bytes += 2 + MMRExtensionProof::max_encoded_len() + StreamProof::max_encoded_len();
		} else if let Some((n1, n2)) = skip {
			bytes += MMRExtensionProof::encoded_len_for(n1, n2)
				.unwrap_or_else(MMRExtensionProof::max_encoded_len);
		}
		Ok(bytes as u64)
	}

	/// Proof size left for the messaging inherent: the block's remaining proof budget, less the
	/// raw data of the other inherents, which are applied after this one is created.
	fn proof_budget(inherent_data: &InherentData) -> u64 {
		let max = T::BlockWeights::get().max_block.proof_size();
		let used = frame_system::Pallet::<T>::block_weight().total().proof_size();
		let others: u64 = inherent_data
			.identifiers()
			.filter(|id| **id != INHERENT_IDENTIFIER)
			.filter_map(|id| inherent_data.get_data::<EncodedLen>(id).ok().flatten())
			.map(|len| len.0 as u64)
			.fold(0, u64::saturating_add);
		max.saturating_sub(used).saturating_sub(others)
	}

	/// The watching rule for an inbound channel: watched, and its delivery queue has room.
	fn is_consumed(channel: &InChannelId, state: &InChannelState) -> bool {
		state.is_watched() && T::QueueFootprint::ready_pages(channel) < T::MaxQueueFootprint::get()
	}

	/// Admit a prefix of payloads for an inbound channel.
	///
	/// Shared between `create_inherent` (which cuts before the first non-admitted payload)
	/// and `consume_channel_item` / dispatch (which requires the admitted prefix to be the
	/// whole item).
	fn admit_channel_item(
		channel_id: InChannelId,
		payloads: &[Payload],
	) -> Option<AdmittedChannelItem> {
		let mut state = InChannels::<T>::get(channel_id)?;
		if !Self::is_consumed(&channel_id, &state) {
			return None;
		}
		let start = state.frontier.root();
		let start_leaf_count = state.frontier.leaf_count();
		let max_item_msgs = T::MaxInboundMessagesPerItem::get() as usize;

		let mut admitted_payloads = Vec::new();
		let mut data_messages = Vec::new();

		for payload in payloads.iter().take(max_item_msgs) {
			if payload.len() > T::MaxMsgLen::get() as usize {
				break;
			}
			let Ok(msg) = ChannelMessage::decode_all(&mut &payload[..]) else {
				break;
			};

			let mut pending_data = None;
			let mut is_close = false;

			match msg {
				ChannelMessage::Hello { session, version } => {
					if session < state.session {
						// Stale: consumed and ignored.
					} else {
						let RegisterStatus::Accepting { session: s, min, max, max_message_size } =
							state.status
						else {
							break;
						};
						if session != s || version < min || version > max || state.open.is_some() {
							break;
						}
						state.session = session;
						state.open = Some(SessionParams { version, max_message_size });
						state.status = RegisterStatus::Opened;
					}
				},
				ChannelMessage::Data(data) => {
					if !matches!(state.status, RegisterStatus::Opened) && !state.draining {
						break;
					}
					let Some(params) = state.open.as_ref() else {
						break;
					};
					if payload.len() > params.max_message_size as usize {
						break;
					}
					pending_data = Some(data);
				},
				ChannelMessage::CloseChannel { session } => {
					if session < state.session {
						// Stale: consumed and ignored.
					} else {
						if !matches!(state.status, RegisterStatus::Opened) && !state.draining {
							break;
						}
						if session != state.session {
							break;
						}
						state.status = RegisterStatus::Closed { session };
						state.draining = false;
						state.open = None;
						is_close = true;
					}
				},
			}

			let pos = MessagePosition(state.frontier.leaf_count());
			if state
				.frontier
				.try_append(leaf_hash_with_version(LEAF_VERSION, payload))
				.is_err()
			{
				break;
			}

			if let Some(data) = pending_data {
				data_messages.push((pos, data));
			}
			admitted_payloads.push(payload.clone());

			if is_close {
				// Non-stale close finishes the item; subsequent payloads cannot be admitted.
				break;
			}
		}

		Some(AdmittedChannelItem {
			start,
			start_leaf_count,
			state,
			payloads: admitted_payloads,
			data_messages,
		})
	}

	/// Validate a register read for admission. Returns the read's gap from where the previous read
	/// ended, if there was one.
	fn admit_register_read(
		channel_id: OutChannelId,
		lossy: &LossyRead,
	) -> Result<Option<(u64, u64)>, Error<T>> {
		let Some(state) = OutChannels::<T>::get(channel_id) else {
			return Err(Error::<T>::UnknownChannel);
		};
		ensure!(state.want_open, Error::<T>::NotAdmitted);
		let [payload] = lossy.payloads.as_slice() else {
			return Err(Error::<T>::BadRegister);
		};
		let register =
			Register::decode_all(&mut &payload[..]).map_err(|_| Error::<T>::BadRegister)?;

		let mut frontier = lossy.frontier.clone();
		let read_pos = MessagePosition(frontier.leaf_count());
		frontier
			.try_append(leaf_hash_with_version(LEAF_VERSION, payload))
			.map_err(|_| Error::<T>::BadFrontier)?;

		if let Some(hw) = state.register.map(|r| r.position.0) {
			ensure!(read_pos.0 > hw, Error::<T>::NotAdmitted);
		}

		let prev_up_to = state.register.map(|r| r.register.up_to.0).unwrap_or(0);
		let stream = channel_id.data_stream();
		let sent = OutboundFrontier::<T>::get(stream).leaf_count() +
			OutboundMessages::<T>::decode_len(stream).unwrap_or(0) as u64;
		ensure!(register.up_to.0 >= prev_up_to, Error::<T>::NotAdmitted);
		ensure!(register.up_to.0 <= sent, Error::<T>::NotAdmitted);

		match register.status {
			RegisterStatus::Accepting { session, min, max, .. } => {
				ensure!(min <= max, Error::<T>::NotAdmitted);
				if session == state.session {
					let Some(prev_status) = state.register.map(|r| r.register.status) else {
						return Err(Error::<T>::NotAdmitted);
					};
					ensure!(prev_status == register.status, Error::<T>::NotAdmitted);
				} else {
					ensure!(session > state.session, Error::<T>::NotAdmitted);
				}
			},
			RegisterStatus::Opened => {
				ensure!(state.took_part_in(state.session), Error::<T>::NotAdmitted);
			},
			RegisterStatus::Closed { session } => {
				ensure!(session >= state.session, Error::<T>::NotAdmitted);
			},
		}

		Ok(state.register.map(|r| (r.position.0 + 1, read_pos.0)))
	}

	/// Consume an inbound channel item in `enact_messages`.
	fn consume_channel_item(
		channel_id: InChannelId,
		payloads: Vec<Payload>,
	) -> Result<(), Error<T>> {
		let admitted =
			Self::admit_channel_item(channel_id, &payloads).ok_or(Error::<T>::NotAdmitted)?;
		ensure!(admitted.payloads.len() == payloads.len(), Error::<T>::NotAdmitted);

		let foreign_stream = channel_id.sender_data_stream(T::SelfParaId::get());

		for (pos, data) in admitted.data_messages {
			T::DataHandler::on_data(channel_id.sender, foreign_stream.1, pos, data);
		}

		let start_pos = MessagePosition(admitted.start_leaf_count);
		for idx in 0..admitted.payloads.len() {
			Self::deposit_event(Event::MessageConsumed {
				source: channel_id.sender,
				stream: foreign_stream.1,
				position: MessagePosition(start_pos.0 + idx as u64),
			});
		}

		InChannels::<T>::insert(channel_id, &admitted.state);
		ConsumptionOutbox::<T>::append((
			channel_id.sender,
			foreign_stream.1,
			Interval { start: admitted.start, end: admitted.state.frontier },
		));
		PendingRegisterPublishes::<T>::mutate(|set| set.insert(channel_id));
		Ok(())
	}

	/// Consume a register read item in `enact_messages`. Returns the read's gap, as
	/// [`Self::admit_register_read`].
	fn consume_register_item(
		channel_id: OutChannelId,
		lossy: LossyRead,
	) -> Result<Option<(u64, u64)>, Error<T>> {
		let skip = Self::admit_register_read(channel_id, &lossy)?;
		let mut state = OutChannels::<T>::get(channel_id).ok_or(Error::<T>::UnknownChannel)?;
		let [payload] = lossy.payloads.as_slice() else {
			return Err(Error::<T>::BadRegister);
		};
		let register =
			Register::decode_all(&mut &payload[..]).map_err(|_| Error::<T>::BadRegister)?;

		let mut frontier = lossy.frontier;
		let read_pos = MessagePosition(frontier.leaf_count());
		let start = frontier.root();
		frontier
			.try_append(leaf_hash_with_version(LEAF_VERSION, payload))
			.map_err(|_| Error::<T>::BadFrontier)?;

		state.register = Some(AppliedRegister { position: read_pos, register });
		match register.status {
			RegisterStatus::Accepting { session, .. } => {
				state.session = session;
				let _ = Self::evaluate_hello_rule(&channel_id, &mut state);
			},
			RegisterStatus::Opened => {
				if let Some(ref mut o) = state.open {
					if o.session == state.session {
						o.opened = true;
					}
				}
			},
			RegisterStatus::Closed { session } => {
				state.session = session;
				if state.took_part_in(session) && !state.closed(session) {
					// Cannot fail: the gate keeps this block's sends below `MaxInFlight`, which
					// `integrity_test` keeps below `MaxMessagesPerBlock`.
					let close = ChannelMessage::CloseChannel { session };
					Self::append_to_stream(channel_id.data_stream(), close.encode())?;
					state.closed_session = Some(session);
				}
			},
		}

		OutChannels::<T>::insert(channel_id, state);
		let foreign_ack = channel_id.peer_ack_stream(T::SelfParaId::get());
		ConsumptionOutbox::<T>::append((
			foreign_ack.0,
			foreign_ack.1,
			Interval { start, end: frontier },
		));
		Ok(skip)
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

	/// The inbound streams and registers the node should fetch.
	pub fn consumed_streams() -> ConsumedStreams {
		let mut consumed = ConsumedStreams::default();
		for (channel, state) in InChannels::<T>::iter() {
			if Self::is_consumed(&channel, &state) {
				consumed.channels.insert(channel, state.frontier);
			}
		}
		for (channel, state) in OutChannels::<T>::iter() {
			if let Some(cursor) = state.register_cursor() {
				consumed.registers.insert(channel, cursor);
			}
		}
		consumed
	}

	/// Cap on streams touched per block.
	pub fn max_touched_streams() -> u32 {
		T::MaxTouchedStreams::get()
	}

	/// This block's consumption record.
	pub fn consumption_record() -> ConsumptionRecord {
		let outbox = ConsumptionOutbox::<T>::get();
		let mut record = ConsumptionRecord::default();
		for (source, stream, interval) in outbox {
			record.entries.entry(source).or_default().insert(stream, interval);
		}
		record
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
