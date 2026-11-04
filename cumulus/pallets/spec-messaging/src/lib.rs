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
//! per block as an [`SPMS_ENGINE_ID`] digest; `validate_block` emits the bundle's last one as the
//! `Provides` UMP signal. Receiver: consumes fetched inbound payloads through
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
use core::{marker::PhantomData, ops::RangeInclusive};
use cumulus_primitives_core::CumulusDigestItem;
use cumulus_primitives_spec_messaging::{
	leaf_hash_with_version, streams_root::update_tree_leaf, AppliedRegister, ChannelMessage,
	ConsumedStreams, ConsumptionRecord, InChannelId, InChannelState, InnerNode, Interval,
	LossyRead, MMRExtensionProof, MessagePosition, MessagingInherentData, MmrFrontier, MmrRoot,
	NodeKey, NodeLookup, NodeStore, OpenSession, OutChannelId, OutChannelState, Payload,
	ProvideConsumptionRecord, Register, RegisterStatus, SessionParams, StreamId, StreamProof,
	StreamsRoot, TreeError, TreeRootEntry, INHERENT_IDENTIFIER, LEAF_VERSION,
	MAX_SPECULATIVE_MESSAGE_LEN, SPMS_ENGINE_ID,
};
use frame_support::{
	ensure,
	inherent::InherentData,
	pallet_prelude::Weight,
	traits::{EnsureOrigin, Get},
	BoundedBTreeSet, BoundedVec,
};
use polkadot_core_primitives::Hash;
use polkadot_parachain_primitives::primitives::Id as ParaId;
use polkadot_primitives::v9::MAX_COMMITMENT_ENTRIES;
use sp_runtime::generic::DigestItem;

pub use pallet::*;

const LOG_TARGET: &str = "runtime::spec-msg";

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod weights;
pub mod xcm_transport;

pub use weights::WeightInfo;
pub use xcm_transport::{
	xcm_channel, EnqueueToXcmQueue, QueueFootprintOf, SpecMsgRouter, XCM_CHANNEL_DOMAIN,
	XCM_CHANNEL_NUM,
};

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

/// Sink for consumed `Data` payloads. `()` drops them; [`EnqueueToXcmQueue`] executes the XCM
/// channel's payloads as XCM.
pub trait OnSpecMsgData {
	/// One `Data` payload, consumed in order at `position` of `(source, stream)`.
	fn on_data(source: ParaId, stream: StreamId, position: MessagePosition, data: Vec<u8>);

	/// Upper bound of [`Self::on_data`] for a payload of `len` bytes.
	fn on_data_weight(len: u32) -> Weight;

	/// The longest `data` this handler accepts; `integrity_test` keeps [`Config::MaxMsgLen`] within
	/// it, so no consumed payload is dropped for its size.
	fn max_len() -> u32 {
		u32::MAX
	}
}

impl OnSpecMsgData for () {
	fn on_data(_: ParaId, _: StreamId, _: MessagePosition, _: Vec<u8>) {}

	fn on_data_weight(_: u32) -> Weight {
		Weight::zero()
	}
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

		/// Per-block cap on sends across all streams, so the end-of-block fold and the next
		/// block's drain are bounded. [`Config::MaxTouchedStreams`] of it is kept for the sends the
		/// inherent causes, so earlier sends in the block cannot make the inherent fail.
		#[pallet::constant]
		type MaxSendsPerBlock: Get<u32>;

		/// Per-block cap on streams the inherent may touch; `<= MAX_COMMITMENT_ENTRIES`.
		#[pallet::constant]
		type MaxTouchedStreams: Get<u32>;

		/// Cap on distinct streams the candidate bundle may touch. It must cover the block proof
		/// size divided by a lift's reservation, so the proof size always binds first.
		#[pallet::constant]
		type MaxBundleStreams: Get<u32>;

		/// Per-item cap on inbound messages consumed.
		#[pallet::constant]
		type MaxInboundMessagesPerItem: Get<u32>;

		/// Supported channel versions sent and parsed; advertised when accepting.
		#[pallet::constant]
		type SupportedVersions: Get<RangeInclusive<u8>>;

		/// Cap on the channels one block's pass over [`OutChannels`] visits after a change of
		/// [`Self::SupportedVersions`]; at least 1.
		#[pallet::constant]
		type MaxHelloPassPerBlock: Get<u32>;

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

		/// Weight information for the calls and hooks.
		type WeightInfo: WeightInfo;

		/// Origin allowed to open an outbound channel.
		type OpenChannelOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Origin allowed to accept and close an inbound channel.
		type AcceptChannelOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Origin allowed to flag a sibling's HRMP channel as closing in favour of spec-msg.
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

	/// Leaf hashes of [`OutboundMessages`], in the same order. A payload is hashed once, at send,
	/// so the commit and the next block's drain read only hashes.
	#[pallet::storage]
	pub type OutboundLeafHashes<T: Config> =
		StorageMap<_, Twox64Concat, StreamId, BoundedVec<Hash, T::MaxMessagesPerBlock>, ValueQuery>;

	/// Sends this block, across all streams.
	#[pallet::storage]
	pub type SendsThisBlock<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// This block's committed [`StreamsRoot`] (the `Provides` source); transient. `Some(None)` once
	/// an idle block has committed, so a later send is rejected there too.
	#[pallet::storage]
	pub type BlockStreamsRoot<T: Config> = StorageValue<_, Option<StreamsRoot>, OptionQuery>;

	/// Root of the commitment-tree node cache over every outbound stream. Each commit folds the
	/// block's touched streams into it, so it must hold every stream from the first send on.
	#[pallet::storage]
	pub type TreeRoot<T: Config> = StorageValue<_, TreeRootEntry, OptionQuery>;

	/// Inner nodes of the [`TreeRoot`] trie.
	#[pallet::storage]
	pub type TreeNodes<T: Config> = StorageMap<_, Twox64Concat, NodeKey, InnerNode, OptionQuery>;

	/// The block whose roll-over has run, with the number of sends it drained. See
	/// [`Pallet::roll_over`].
	#[pallet::storage]
	pub type RolledOver<T: Config> = StorageValue<_, (BlockNumberFor<T>, u32), OptionQuery>;

	/// Sender side, per outbound channel.
	#[pallet::storage]
	pub type OutChannels<T: Config> =
		StorageMap<_, Twox64Concat, OutChannelId, OutChannelState, OptionQuery>;

	/// The [`Config::SupportedVersions`] range last seen by `on_runtime_upgrade`.
	#[pallet::storage]
	pub type SeenVersions<T: Config> = StorageValue<_, (u8, u8), OptionQuery>;

	/// The raw [`OutChannels`] key the Hello pass resumes after; empty to start over. A pass is
	/// running while this is set.
	#[pallet::storage]
	pub type HelloPassCursor<T: Config> =
		StorageValue<_, BoundedVec<u8, ConstU32<64>>, OptionQuery>;

	/// Receiver side, per inbound channel.
	#[pallet::storage]
	pub type InChannels<T: Config> =
		StorageMap<_, Twox64Concat, InChannelId, InChannelState, OptionQuery>;

	/// Siblings whose HRMP channel is being closed in favour of spec-msg. While a sibling is
	/// flagged, [`SpecMsgRouter`] treats its HRMP channel as closed: new XCM goes over spec-msg,
	/// while `XcmpQueue` keeps draining what HRMP already queued. See
	/// [`Pallet::set_hrmp_closing`].
	#[pallet::storage]
	pub type HrmpClosing<T: Config> = StorageMap<_, Twox64Concat, ParaId, (), OptionQuery>;

	/// Inbound channels whose delivery queue is paused through [`QueuePausedQuery`]. See
	/// [`Pallet::pause_inbound_channel`].
	#[pallet::storage]
	pub type PausedInbound<T: Config> = StorageMap<_, Twox64Concat, InChannelId, (), OptionQuery>;

	/// Whether `enact_messages` ran this block; cleared next block.
	#[pallet::storage]
	pub type Enacted<T: Config> = StorageValue<_, (), OptionQuery>;

	/// This block's consumption intervals; grouped/sorted by [`Pallet::consumption_record`],
	/// cleared next block. Bounded by [`Config::MaxTouchedStreams`].
	#[pallet::storage]
	#[pallet::unbounded]
	pub type ConsumptionOutbox<T: Config> =
		StorageValue<_, Vec<(ParaId, StreamId, Interval)>, ValueQuery>;

	/// Streams touched across blocks in the candidate bundle: a stream's first touch reserves its
	/// lift, and the touched sources are their parachains. Cleared on index 0. `create_inherent`
	/// reads no register whose stream is here, so its bundles need no `advances`.
	#[pallet::storage]
	pub type BundleStreams<T: Config> =
		StorageValue<_, BoundedBTreeSet<(ParaId, StreamId), T::MaxBundleStreams>, ValueQuery>;

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
		/// The block's sends reached [`Pallet::send_limit`], or [`Config::MaxSendsPerBlock`].
		TooManySends,
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
			// The calls and the inherent charge the end-of-block commit; this charges the idle one.
			let weight =
				T::WeightInfo::on_initialize(drained).saturating_add(T::WeightInfo::idle_commit());
			match Self::run_hello_pass() {
				Some((visited, hellos)) => weight
					.saturating_add(T::WeightInfo::hello_pass(visited))
					.saturating_add(commit_weight::<T>(hellos, 0)),
				None => weight,
			}
		}

		/// Only schedules the Hello pass when [`Config::SupportedVersions`] changed: sends are
		/// refused before initialization, so `on_initialize` does the work.
		fn on_runtime_upgrade() -> Weight {
			let supported = T::SupportedVersions::get();
			let now = (*supported.start(), *supported.end());
			let changed = match SeenVersions::<T>::get() {
				Some(seen) => seen != now,
				None => OutChannels::<T>::iter_keys().next().is_some(),
			};
			if changed {
				HelloPassCursor::<T>::put(BoundedVec::default());
			}
			SeenVersions::<T>::put(now);
			T::DbWeight::get().reads_writes(2, 2)
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
				T::MaxTouchedStreams::get() < T::MaxSendsPerBlock::get(),
				"`MaxTouchedStreams` must be below `MaxSendsPerBlock`",
			);
			assert!(
				T::MaxBundleStreams::get() as u64 * lift_len() as u64 >=
					T::BlockWeights::get().max_block.proof_size(),
				"`MaxBundleStreams` lifts must cover the block proof size",
			);
			assert!(
				T::MaxMessageSize::get() <= T::MaxMsgLen::get(),
				"`MaxMessageSize` must not exceed `MaxMsgLen`",
			);
			assert!(
				T::MaxMsgLen::get() <= T::DataHandler::max_len(),
				"`MaxMsgLen` must not exceed the data handler's `max_len`",
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
			assert!(
				T::MaxHelloPassPerBlock::get() >= 1,
				"`MaxHelloPassPerBlock` must be at least 1"
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

			BundleStreams::<T>::put(
				BoundedBTreeSet::try_from(budget.streams)
					.map_err(|_| Error::<T>::TooManyStreams)?,
			);
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
		#[pallet::weight(
			T::WeightInfo::open_channel().saturating_add(commit_weight::<T>(1, 0))
		)]
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

			// A Hello that cannot be sent now fails the call: no later event would re-check it.
			Self::evaluate_hello_rule(&channel, &mut state, Self::send_limit())?;
			OutChannels::<T>::insert(channel, state);
			Self::deposit_event(Event::ChannelOpened { channel });
			Ok(())
		}

		/// Accept an inbound channel from `sender`, allocating or stepping its session.
		#[pallet::call_index(2)]
		#[pallet::weight(
			T::WeightInfo::accept_open_channel()
				.saturating_add(commit_weight::<T>(0, 1))
		)]
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
		#[pallet::weight(
			T::WeightInfo::close_channel().saturating_add(commit_weight::<T>(1, 0))
		)]
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
		#[pallet::weight(
			T::WeightInfo::close_inbound_channel()
				.saturating_add(commit_weight::<T>(0, 1))
		)]
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

		/// Flag the HRMP channel to `peer` as closing: new XCM goes over spec-msg while HRMP
		/// drains. Needs our spec-msg XCM channel to `peer` open. Cutover: open spec-msg both
		/// ways, then batch this call with the relay's `hrmp.close_channel`, which takes effect
		/// next session. Clear with [`Pallet::clear_hrmp_closing`] before reopening HRMP.
		/// Idempotent. The two transports feed separate queues, so a new XCM over spec-msg can
		/// execute before an older one still queued from HRMP: where order matters, let that queue
		/// drain first.
		#[pallet::call_index(5)]
		#[pallet::weight((T::WeightInfo::set_hrmp_closing(), DispatchClass::Operational))]
		pub fn set_hrmp_closing(origin: OriginFor<T>, peer: ParaId) -> DispatchResult {
			T::ChannelManagementOrigin::ensure_origin(origin)?;
			let open = OutChannels::<T>::get(xcm_channel(peer))
				.is_some_and(|state| state.active_session(T::SupportedVersions::get()).is_some());
			ensure!(open, Error::<T>::NotOpen);
			HrmpClosing::<T>::insert(peer, ());
			Ok(())
		}

		/// Clear the [`HrmpClosing`] flag for `peer`: the router prefers HRMP again whenever an
		/// HRMP channel is open. XCM already sent over spec-msg is still delivered. Idempotent.
		#[pallet::call_index(6)]
		#[pallet::weight((T::WeightInfo::clear_hrmp_closing(), DispatchClass::Operational))]
		pub fn clear_hrmp_closing(origin: OriginFor<T>, peer: ParaId) -> DispatchResult {
			T::ChannelManagementOrigin::ensure_origin(origin)?;
			HrmpClosing::<T>::remove(peer);
			Ok(())
		}

		/// Pause the delivery queue of an inbound channel: execution stops at once, and
		/// consumption stops when the queue reaches `MaxQueueFootprint`. Idempotent.
		#[pallet::call_index(7)]
		#[pallet::weight((T::WeightInfo::pause_inbound_channel(), DispatchClass::Operational))]
		pub fn pause_inbound_channel(
			origin: OriginFor<T>,
			sender: ParaId,
			domain: u8,
			num: u16,
		) -> DispatchResult {
			T::ChannelManagementOrigin::ensure_origin(origin)?;
			PausedInbound::<T>::insert(InChannelId { sender, domain, num }, ());
			Ok(())
		}

		/// Resume the delivery queue paused by [`Pallet::pause_inbound_channel`]. Idempotent.
		#[pallet::call_index(8)]
		#[pallet::weight((T::WeightInfo::resume_inbound_channel(), DispatchClass::Operational))]
		pub fn resume_inbound_channel(
			origin: OriginFor<T>,
			sender: ParaId,
			domain: u8,
			num: u16,
		) -> DispatchResult {
			T::ChannelManagementOrigin::ensure_origin(origin)?;
			PausedInbound::<T>::remove(InChannelId { sender, domain, num });
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
				let stream = channel_id.peer_ack_stream(us);
				// One read per register per bundle; a newer register waits for the next bundle.
				// A second read can skip leaves, and the PoV then needs an advance over them,
				// which the node fetches only after importing the block: usually after the
				// collation is built. A read that skips none waits too, so no two reads of a
				// register are ever stitched.
				if budget.streams.contains(&stream) {
					continue;
				}
				let Ok(skip) = Self::admit_register_read(channel_id, &lossy) else {
					continue;
				};
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

/// The end-of-block work for `sends` sends and `publishes` register publishes. Both benchmarks'
/// bases hold the commit's fixed cost, so `commit_publishes` adds only its slope.
fn commit_weight<T: Config>(sends: u32, publishes: u32) -> Weight {
	let publishes = T::WeightInfo::commit_publishes(publishes)
		.saturating_sub(T::WeightInfo::commit_publishes(0));
	T::WeightInfo::commit_sends(sends).saturating_add(publishes)
}

/// Weight of one `enact_messages`, with the end-of-block work it causes: one register publish per
/// channel item and at most one send per register read.
fn enact_weight<T: Config>(data: &MessagingInherentData) -> Weight {
	let channels = data.channels.len() as u32;
	let registers = data.registers.len() as u32;
	let mut payloads = 0u32;
	let mut bytes = 0u32;
	let mut handler = Weight::zero();
	for payload in data.channels.values().flatten() {
		payloads = payloads.saturating_add(1);
		bytes = bytes.saturating_add(payload.len() as u32);
		handler = handler.saturating_add(T::DataHandler::on_data_weight(payload.len() as u32));
	}
	// Every benchmark's base holds the inherent's fixed cost, and the payload and byte bases one
	// channel's too: `enact_channels`, or `enact_registers` without channels, charges them once.
	let registers_weight = T::WeightInfo::enact_registers(registers);
	let enact = if channels == 0 {
		registers_weight
	} else {
		// The payload benchmark counts the messages beyond the first of each channel.
		let payloads = T::WeightInfo::enact_payloads(payloads.saturating_sub(channels))
			.saturating_sub(T::WeightInfo::enact_payloads(0));
		let bytes = T::WeightInfo::enact_bytes(bytes).saturating_sub(T::WeightInfo::enact_bytes(0));
		T::WeightInfo::enact_channels(channels)
			.saturating_add(payloads)
			.saturating_add(bytes)
			.saturating_add(registers_weight.saturating_sub(T::WeightInfo::enact_registers(0)))
	};
	enact
		.saturating_add(handler)
		.saturating_add(commit_weight::<T>(registers, channels))
}

/// The proof bytes one lift without advances reserves; a bundle has at most 256 blocks, so the
/// `advances` length prefix takes 2 bytes.
fn lift_len() -> usize {
	2 + MMRExtensionProof::max_encoded_len() + StreamProof::max_encoded_len()
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
		let streams = BundleStreams::<T>::get().into_inner();
		let sources = streams.iter().map(|(source, _)| *source).collect();
		Self { touched: 0, sources, streams, reserved: 0 }
	}

	fn record(&mut self, stream: (ParaId, StreamId), bytes: u64) {
		self.touched += 1;
		self.sources.insert(stream.0);
		self.streams.insert(stream);
		self.reserved = self.reserved.saturating_add(bytes);
	}
}

/// [`TreeNodes`] with writes buffered until [`Self::flush`], so a failed fold writes nothing and a
/// node on several touched paths is written once.
struct TreeNodeOverlay<T>(BTreeMap<NodeKey, InnerNode>, PhantomData<T>);

impl<T> Default for TreeNodeOverlay<T> {
	fn default() -> Self {
		Self(BTreeMap::new(), PhantomData)
	}
}

impl<T: Config> TreeNodeOverlay<T> {
	fn flush(self) {
		for (key, node) in self.0 {
			TreeNodes::<T>::insert(key, node);
		}
	}
}

impl<T: Config> NodeLookup for TreeNodeOverlay<T> {
	fn get_node(&self, key: &NodeKey) -> Option<InnerNode> {
		self.0.get(key).copied().or_else(|| TreeNodes::<T>::get(key))
	}
}

impl<T: Config> NodeStore for TreeNodeOverlay<T> {
	fn set_node(&mut self, key: NodeKey, node: InnerNode) {
		self.0.insert(key, node);
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
		SendsThisBlock::<T>::kill();

		let is_first =
			CumulusDigestItem::find_block_bundle_info(&frame_system::Pallet::<T>::digest())
				.map_or(true, |info| info.index == 0);
		if is_first {
			BundleStreams::<T>::kill();
		}

		let _ = OutboundMessages::<T>::clear(u32::MAX, None);
		let mut drained = 0u32;
		for (stream, hashes) in OutboundLeafHashes::<T>::drain() {
			let mut frontier = OutboundFrontier::<T>::get(stream);
			for hash in hashes {
				frontier.append(hash);
				drained = drained.saturating_add(1);
			}
			OutboundFrontier::<T>::insert(stream, frontier);
		}
		RolledOver::<T>::put((now, drained));
		drained
	}

	/// The block's send cap outside the inherent's room.
	pub fn send_limit() -> u32 {
		T::MaxSendsPerBlock::get().saturating_sub(T::MaxTouchedStreams::get())
	}

	/// Append `payload` to `stream`'s outbound MMR within [`Self::send_limit`], returning its
	/// position.
	pub(crate) fn append_to_stream(
		stream: StreamId,
		payload: Vec<u8>,
	) -> Result<MessagePosition, Error<T>> {
		Self::append_within(stream, payload, Self::send_limit())
	}

	/// [`Self::append_to_stream`] with the block's sends capped at `limit`.
	fn append_within(
		stream: StreamId,
		payload: Vec<u8>,
		limit: u32,
	) -> Result<MessagePosition, Error<T>> {
		let payload: BoundedVec<u8, T::MaxMsgLen> =
			payload.try_into().map_err(|_| Error::<T>::MessageTooBig)?;

		Self::roll_over();
		ensure!(!BlockStreamsRoot::<T>::exists(), Error::<T>::RootCommitted);
		let sends = SendsThisBlock::<T>::get();
		ensure!(sends < limit, Error::<T>::TooManySends);
		let hash = leaf_hash_with_version(LEAF_VERSION, &payload);
		let index = OutboundMessages::<T>::decode_len(stream).unwrap_or(0) as u64;
		OutboundMessages::<T>::try_append(stream, payload)
			.map_err(|()| Error::<T>::TooManyMessages)?;
		OutboundLeafHashes::<T>::try_append(stream, hash)
			.map_err(|()| Error::<T>::TooManyMessages)?;
		SendsThisBlock::<T>::put(sends + 1);

		Ok(MessagePosition(OutboundFrontier::<T>::get(stream).leaf_count() + index))
	}

	/// Fold this block's sends into the [`StreamsRoot`], memoize it, and deposit the digest.
	pub fn commit_streams_root() -> Option<StreamsRoot> {
		Self::roll_over();
		if let Some(committed) = BlockStreamsRoot::<T>::get() {
			return committed;
		}

		Self::publish_pending_registers();

		let mut touched: BTreeMap<StreamId, Hash> = BTreeMap::new();
		for (stream, hashes) in OutboundLeafHashes::<T>::iter() {
			let mut frontier = OutboundFrontier::<T>::get(stream);
			for hash in hashes {
				frontier.append(hash);
			}
			touched.insert(stream, frontier.root().0);
		}
		if touched.is_empty() {
			BlockStreamsRoot::<T>::put(None::<StreamsRoot>);
			return None;
		}

		let root = match Self::fold_into_tree(&touched) {
			Ok(root) => root,
			Err(error) => {
				// Fail closed: a root from a corrupt cache would not commit the real stream roots.
				log::error!(
					target: LOG_TARGET,
					"Block {:?}: streams root not committed: {error:?}",
					frame_system::Pallet::<T>::block_number(),
				);
				BlockStreamsRoot::<T>::put(None::<StreamsRoot>);
				return None;
			},
		};
		BlockStreamsRoot::<T>::put(Some(root));
		frame_system::Pallet::<T>::deposit_log(DigestItem::Consensus(
			SPMS_ENGINE_ID,
			root.encode(),
		));
		log::debug!(
			target: LOG_TARGET,
			"Block {:?}: committed streams root {root:?} over {} touched streams",
			frame_system::Pallet::<T>::block_number(),
			touched.len(),
		);

		Some(root)
	}

	/// Fold the touched streams' roots into the node cache and return the new [`StreamsRoot`].
	/// On error, nothing is written.
	fn fold_into_tree(touched: &BTreeMap<StreamId, Hash>) -> Result<StreamsRoot, TreeError> {
		let mut entry = TreeRoot::<T>::get();
		let mut nodes = TreeNodeOverlay::<T>::default();
		let mut root = None;
		for (stream, stream_root) in touched {
			root = Some(update_tree_leaf(&mut entry, *stream, *stream_root, &mut nodes)?);
		}
		nodes.flush();
		TreeRoot::<T>::set(entry);
		Ok(root.expect("callers pass at least one touched stream; qed"))
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
					if Self::append_within(stream, current.encode(), T::MaxSendsPerBlock::get())
						.is_ok()
					{
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

	/// Weight of [`Self::send`] for `data_len` bytes, with the end-of-block work it causes. Callers
	/// charge it: `send` is not a call.
	pub fn send_weight(data_len: u32) -> Weight {
		T::WeightInfo::send(data_len).saturating_add(commit_weight::<T>(1, 0))
	}

	/// Whether [`Self::send`] would accept `data_len` bytes on `channel` now, apart from the
	/// initialization check: the encoded `Data` leaf fits [`Config::MaxMsgLen`] and the session's
	/// `max_message_size`, the channel is open with credit left, and the stream and the block have
	/// room. No side effects.
	pub fn can_send(channel: OutChannelId, data_len: usize) -> Result<(), Error<T>> {
		// The `Data` leaf is a variant byte, the compact length and the bytes.
		let len = u32::try_from(data_len).map_err(|_| Error::<T>::MessageTooBig)?;
		let leaf_len = (data_len as u64)
			.saturating_add(1)
			.saturating_add(Compact(len).encoded_size() as u64);
		ensure!(leaf_len <= u64::from(T::MaxMsgLen::get()), Error::<T>::MessageTooBig);

		let state = OutChannels::<T>::get(channel).ok_or(Error::<T>::NotOpen)?;
		let params =
			state.active_session(T::SupportedVersions::get()).ok_or(Error::<T>::NotOpen)?;
		ensure!(leaf_len <= u64::from(params.max_message_size), Error::<T>::MessageTooBig);

		let stream = channel.data_stream();
		let sent = OutboundFrontier::<T>::get(stream).leaf_count() +
			OutboundMessages::<T>::decode_len(stream).unwrap_or(0) as u64;
		let up_to = state.register.map(|r| r.register.up_to.0).unwrap_or(0);
		ensure!(sent.saturating_sub(up_to) < T::MaxInFlight::get() as u64, Error::<T>::NoCredit);

		let queued = OutboundMessages::<T>::decode_len(stream).unwrap_or(0);
		ensure!(queued < T::MaxMessagesPerBlock::get() as usize, Error::<T>::TooManyMessages);
		ensure!(SendsThisBlock::<T>::get() < Self::send_limit(), Error::<T>::TooManySends);
		Ok(())
	}

	/// Send `data` on an outbound channel.
	pub fn send(channel: OutChannelId, data: Vec<u8>) -> Result<MessagePosition, Error<T>> {
		ensure!(frame_system::Pallet::<T>::execution_phase().is_some(), Error::<T>::NotInitialized);
		Self::can_send(channel, data.len())?;
		Self::append_to_stream(channel.data_stream(), ChannelMessage::Data(data).encode())
	}

	/// One block's step of the Hello pass, if one is scheduled: returns the channels visited and
	/// the `Hello`s sent. The cursor stays before a channel the send limit stopped.
	fn run_hello_pass() -> Option<(u32, u32)> {
		use frame_support::storage::generator::StorageMap as _;
		let cursor = HelloPassCursor::<T>::get()?;
		let mut last = if cursor.is_empty() {
			OutChannels::<T>::prefix_hash().to_vec()
		} else {
			cursor.into_inner()
		};
		let limit = Self::send_limit();
		let (mut visited, mut hellos) = (0u32, 0u32);
		let mut finished = true;
		for (channel, mut state) in OutChannels::<T>::iter_from(last.clone()) {
			if visited >= T::MaxHelloPassPerBlock::get() {
				finished = false;
				break;
			}
			visited = visited.saturating_add(1);
			match Self::evaluate_hello_rule(&channel, &mut state, limit) {
				Ok(true) => {
					OutChannels::<T>::insert(channel, state);
					hellos = hellos.saturating_add(1);
				},
				Err(Error::<T>::TooManySends) => {
					finished = false;
					break;
				},
				Ok(false) | Err(_) => {},
			}
			last = OutChannels::<T>::hashed_key_for(channel);
		}
		if finished {
			HelloPassCursor::<T>::kill();
		} else {
			let cursor = BoundedVec::try_from(last).expect("an `OutChannels` key fits; qed");
			HelloPassCursor::<T>::put(cursor);
		}
		Some((visited, hellos))
	}

	/// Evaluate the Hello rule for an outbound channel, emitting `Hello` within `limit` if due.
	fn evaluate_hello_rule(
		channel: &OutChannelId,
		state: &mut OutChannelState,
		limit: u32,
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
		Self::append_within(stream, hello.encode(), limit)?;
		state.open = Some(OpenSession {
			session,
			params: SessionParams { version, max_message_size },
			opened: false,
		});
		Ok(true)
	}

	/// Checks the stream and source caps for touching `stream`, and returns the proof bytes the
	/// candidate attaches for it after the block is built. `skip` is a lossy read's gap from where
	/// the previous read ended. It costs bytes only when the bundle already read the stream, which
	/// only a block authored elsewhere does: `create_inherent` reads a register once per bundle.
	fn lift_reservation(
		budget: &BundleBudget,
		stream: (ParaId, StreamId),
		skip: Option<(u64, u64)>,
	) -> Result<u64, Error<T>> {
		ensure!(budget.touched < T::MaxTouchedStreams::get(), Error::<T>::TooManyStreams);
		ensure!(
			budget.streams.contains(&stream) ||
				budget.streams.len() < T::MaxBundleStreams::get() as usize,
			Error::<T>::TooManyStreams
		);
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
			bytes += lift_len();
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
				let _ =
					Self::evaluate_hello_rule(&channel_id, &mut state, T::MaxSendsPerBlock::get());
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
					// `integrity_test` keeps below `MaxMessagesPerBlock`, and the inherent's
					// sends have their own room in `MaxSendsPerBlock`.
					let close = ChannelMessage::CloseChannel { session };
					Self::append_within(
						channel_id.data_stream(),
						close.encode(),
						T::MaxSendsPerBlock::get(),
					)?;
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
			record.entries.entry(source).or_default().push((stream, interval));
		}
		// No dedup: a duplicate must reach requires synthesis and fail there.
		for streams in record.entries.values_mut() {
			streams.sort_by_key(|(stream, _)| *stream);
		}
		record
	}
}

impl<T: Config> ProvideConsumptionRecord for Pallet<T> {
	fn consumption_record() -> ConsumptionRecord {
		Self::consumption_record()
	}
}
