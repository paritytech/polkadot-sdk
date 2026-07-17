// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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
//! Both halves of the parachain-side transport (design v0.5).
//!
//! **Sender side**: accumulates this parachain's outbound message streams —
//! every sent payload becomes a leaf in its stream's append-only MMR, and a
//! block that touches at least one stream commits to *all* streams with a
//! single hash — the [`StreamsRoot`], root of the stream commitment tree (a
//! binary compact trie keyed by the canonical [`StreamId`] encoding, leaves
//! = the streams' MMR roots). Payloads themselves travel off-chain between
//! collators; the chain only ever commits to hashes.
//!
//! **Receiver side**: consumes other chains' streams via the messaging
//! inherent ([`Pallet::enact_messages`]) — fetched payloads in, no roots of
//! any kind and no relay state reads: the runtime verifies by
//! *recomputation* only, and binding the results to committed sender roots
//! is the PVF's job (consumption record + POV lifts).
//!
//! ## Sender lifecycle
//!
//! - [`Pallet::append_to_stream`] pushes a payload onto [`OutboundMessages`], the per-stream vec of
//!   THIS block's sends (host-side append, O(1) per send). The stored frontier is never touched
//!   mid-block, so `position = frontier.leaf_count + index` holds unchanged for the whole block,
//!   including finalization.
//! - At block end (`on_finalize`) the touched streams' new MMR roots are computed *transiently*
//!   (stored frontier + this block's leaves, in memory) and folded into the stored commitment tree
//!   — one tree-path write per touched stream. The resulting [`StreamsRoot`] is memoized for the
//!   `Provides` UMP signal emission and deposited as the `DigestItem::Consensus(SPMS_ENGINE_ID,
//!   root)` header digest, at most one per header. An idle block folds nothing, emits nothing and
//!   deposits nothing.
//! - At the NEXT block's `on_initialize` the pending messages are hashed (`hash_leaf`, current
//!   `LEAF_VERSION`) into the frontiers and [`OutboundMessages`] is cleared — one atomic step.
//!   Block N's messages therefore stay readable in block N's state, where node-side extraction (the
//!   runtime API) reads them — never storage directly.
//!
//! The commitment tree's node storage ([`TreeNodes`], [`TreeRoot`]) is a
//! rebuildable cache — the frontiers determine the whole tree — kept because
//! incremental path updates need the sibling hashes (O(k·log S) per block
//! for k touched streams among S).
//!
//! Messages must be appended before this pallet's `on_finalize` runs: a
//! payload appended by a later `on_finalize` hook would miss the fold and
//! silently shift every subsequent position. Order such pallets before this
//! one in `construct_runtime`.
//!
//! Channel state and the credit-gated `send` live one layer up (channels &
//! flow control); this pallet provides the internal append primitive they
//! call, and its per-block caps ([`Config::MaxMessagesPerBlock`],
//! [`Config::MaxMsgLen`]) are the hard, consensus-side backpressure
//! underneath the advisory window grants.
//!
//! The channel layer's outbound entry points already live here in skeletal
//! form: [`Pallet::send`]/[`Pallet::can_send`] wrap payloads as
//! [`SpecMsgKind::Data`] leaves on the channel's data stream (this is what
//! the XCM router, [`SpecMsgRouter`], calls). Until the full lifecycle
//! machinery lands, channel phase is a placeholder flag
//! ([`OpenOutboundChannels`]) and the advisory credit window is not yet
//! enforced — the per-block caps are the only backpressure.
//!
//! ## Receiver lifecycle
//!
//! The messaging inherent (identifier `specmsg0`) carries at most one item
//! per consumed stream: ordered channel payloads, and register head reads
//! (latest ack-stream leaf + MMR inclusion proof). Per channel item the
//! payloads are hashed (`hash_leaf`, current [`LEAF_VERSION`]) and appended
//! to the stream's [`InboundFrontier`]; order and count need no explicit
//! check — any deviation yields a different endpoint, which no lift can
//! bind (PVF-enforced). Per register read the inclusion proof pins position
//! and peaks; the yielded root is *derived, never declared*. Every touched
//! stream writes one [`Interval`] to the transient [`ConsumptionOutbox`] —
//! the block's consumption record, which the `validate_block` wrapper
//! stitches across the bundle and lifts to committed sender roots. The
//! consumption boundary is free: a block may stop anywhere (weight/POV
//! budget), the lift's extension proof covers the unconsumed tail; an
//! absent inherent is a valid block that consumes nothing.
//!
//! No-skip is an STF rule: the runtime offers no payload-free path to
//! advance a frontier, and an unparseable payload on an ordered stream is
//! consumed and dropped ([`Event::PayloadDropped`]) — skipping is illegal,
//! stalling would brick the channel. Invalid inherent items (unknown or
//! duplicate stream, oversized payload, structural proof defects, exceeded
//! caps) are rejected wholesale ([`Event::ItemRejected`]): nothing of the
//! item is consumed, the block stays valid — an honest collator's
//! pre-verification never produces them.
//!
//! Lifts are attached by the submitter *outside* block execution, so the
//! STF charges the worst case up front as `proof_size` weight: per touched
//! stream the lift reservation ([`LIFT_RESERVATION_BYTES`]), per read
//! context the advance-proof bytes ([`ADVANCE_PROOF_RESERVATION_BYTES`]) —
//! bounded by the hard per-block caps [`Config::MaxTouchedStreams`] and
//! [`Config::MaxContextGaps`] (which also keep the record's sources within
//! the relay's `MAX_COMMITMENT_ENTRIES`).
//!
//! Like the outbound side, inbound channel state is placeholder-grade until
//! the lifecycle machinery lands: [`ConsumedStreams`] is the manually armed
//! consumed-stream set, lifecycle signals and register contents are
//! verified, then dropped. Consumed [`SpecMsgKind::Data`] payloads go to
//! [`Config::DataHandler`] — runtimes wire [`EnqueueToXcmQueue`] there to
//! forward them into the message queue for XCM execution, under an origin
//! (`SpecMsg(source)`) indistinguishable from the HRMP one.
//!
//! ## Runtime API
//!
//! This pallet backs the `SpecMsgApi` node–runtime boundary (declared in
//! `cumulus-primitives-core`, implemented by the runtime as one-line
//! delegations): [`Pallet::outbound_messages`], [`Pallet::consumed_streams`],
//! [`Pallet::out_channels`], [`Pallet::in_channels`] and
//! [`Pallet::consumption_record`] — the latter dual-called, by the node via
//! API dispatch and by the `validate_block` wrapper directly in-wasm
//! ([`ProvideUmpSignals`]).

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod xcm_router;

use alloc::{
	collections::{BTreeMap, BTreeSet},
	vec::Vec,
};
use codec::{Decode, DecodeAll, DecodeWithMemTracking, Encode, MaxEncodedLen};
use cumulus_primitives_core::{
	relay_chain::{UMPSignal, MAX_COMMITMENT_ENTRIES},
	ParaId,
};
use cumulus_primitives_spec_messaging::{
	hash_leaf,
	tree::{bit_at, first_diff_bit, tree_inner_hash, tree_leaf_hash, KEY_BITS},
	ChannelId, ConsumedStream, ConsumptionRecord, InChannelState, Interval, MessagePosition,
	MmrFrontier, MmrInclusionProof, OutChannelState, ProvideUmpSignals, Register, SpecHasher,
	SpecMsgInherentData, SpecMsgKind, SpecMsgSignal, StreamId, StreamsRoot, LEAF_VERSION,
	SPMS_ENGINE_ID, STREAM_ID_LEN,
};
use polkadot_core_primitives::Hash;
use scale_info::TypeInfo;
use sp_runtime::generic::DigestItem;

pub use pallet::*;
pub use xcm_router::SpecMsgRouter;

/// Worst-case POV bytes of one stream's requires lift, charged as
/// `proof_size` weight per stream the messaging inherent touches (v0.5 §PoV
/// Weight Reservation). The steady-state lift is a bare tree proof (~300 B);
/// the reservation covers the worst case — a full tree path plus a maximal
/// extension proof — because the lift is attached by the submitter outside
/// block execution and the block must fit at authoring time.
pub const LIFT_RESERVATION_BYTES: u64 = 4 * 1024;

/// Worst-case POV bytes of one advance proof, charged as `proof_size`
/// weight per read-context gap: a register/event read pins its context
/// freely, so each read can open one gap in the bundle's interval chain
/// that a POV-carried advance proof must cover. (Channel streams never gap
/// — consumption is a stored frontier every block continues from.)
pub const ADVANCE_PROOF_RESERVATION_BYTES: u64 = 2 * 1024;

/// Sink for in-order consumed [`SpecMsgKind::Data`] payloads of inbound
/// channel streams.
///
/// The XCM-queue forwarding wires in here: [`EnqueueToXcmQueue`] is the
/// canonical implementation (enqueue under `SpecMsg(source)`). The `()`
/// implementation drops the payloads — transport-only consumption: the
/// frontier and the consumption record advance regardless, because
/// consuming a message and acting on it are separate layers.
pub trait OnSpecMsgData {
	/// One `Data` payload, consumed in order at `position` of the stream
	/// `(source, stream)`.
	fn on_data(source: ParaId, stream: StreamId, position: MessagePosition, data: Vec<u8>);
}

impl OnSpecMsgData for () {
	fn on_data(_: ParaId, _: StreamId, _: MessagePosition, _: Vec<u8>) {}
}

/// [`OnSpecMsgData`] implementation forwarding every consumed payload into
/// the runtime's message queue for XCM execution — the receiving end of the
/// [`SpecMsgRouter`]'s envelope: on an XCM channel the `Data` payload is
/// exactly the SCALE-encoded `VersionedXcm`, which is what the queue's
/// `ProcessXcmMessage` processor expects, so the bytes are enqueued
/// verbatim, mirroring the XCMP enqueue path.
///
/// The queue book is keyed by the SOURCE para alone — all of one source's
/// XCM channels feed one book — under
/// `AggregateMessageOrigin::SpecMsg(source)`, which the runtime supplies by
/// wiring `Queue` as (see `ParaIdToSpecMsg` next to `ParaIdToSibling`):
///
/// ```ignore
/// type DataHandler = EnqueueToXcmQueue<
/// 	TransformOrigin<MessageQueue, AggregateMessageOrigin, ParaId, ParaIdToSpecMsg>,
/// >;
/// ```
///
/// `SpecMsg(source)` converts to the very `Location` that `Sibling(source)`
/// converts to, so the executor, barriers and every downstream filter see an
/// origin identical to the HRMP one — no `XcmConfig` changes anywhere, and
/// no XCM program can distinguish the transport.
///
/// The runtime MUST keep the queue's `MaxMessageLen` (derived from
/// `pallet-message-queue`'s `HeapSize`) at least [`Config::MaxMsgLen`]:
/// consumed payloads are bounded by the latter, so every payload then fits;
/// one that does not is dropped defensively.
///
/// Until the channels layer demultiplexes userspace protocols per channel,
/// this forwards ALL consumed `Data` payloads — in MVP every consumed
/// channel is an XCM channel.
pub struct EnqueueToXcmQueue<Queue>(core::marker::PhantomData<Queue>);

impl<Queue> OnSpecMsgData for EnqueueToXcmQueue<Queue>
where
	Queue: frame_support::traits::EnqueueMessage<ParaId>,
{
	fn on_data(source: ParaId, _stream: StreamId, _position: MessagePosition, data: Vec<u8>) {
		let Ok(data) = frame_support::BoundedSlice::try_from(&data[..]) else {
			frame_support::defensive!(
				"consumed spec-msg payload exceeds the queue's `MaxMessageLen`; dropped \
				 (`MaxMessageLen` must be at least `MaxMsgLen`)"
			);
			return;
		};
		Queue::enqueue_message(data, source);
	}
}

/// Why a messaging-inherent item was rejected (see
/// [`Event::ItemRejected`]). Rejection is per item and total: nothing of a
/// rejected item is consumed, no interval is recorded for it.
#[derive(Clone, Copy, Encode, Decode, DecodeWithMemTracking, TypeInfo, Debug, Eq, PartialEq)]
pub enum RejectReason {
	/// The stream is not one this runtime consumes: not listed in
	/// [`ConsumedStreams`] (unknown, unaccepted or suspended channel), or
	/// the wrong stream kind for the item (ordered consumption is defined
	/// on `Channel` streams, head reads on `Ack` streams).
	UnknownStream,
	/// A second item for the same stream — the inherent carries at most
	/// one item per stream.
	DuplicateStream,
	/// A channel item without payloads: an empty interval would waste a
	/// touched-stream slot and a lift on advancing nothing.
	EmptyItem,
	/// A payload exceeds [`Config::MaxMsgLen`], the consensus hard
	/// per-message size bound.
	OversizedPayload,
	/// The [`Config::MaxTouchedStreams`] cap is exhausted.
	TooManyStreams,
	/// The [`Config::MaxContextGaps`] cap is exhausted.
	TooManyGaps,
	/// The head read's inclusion proof is structurally invalid. (A proof
	/// with merely wrong hashes is not detectable here — it yields a
	/// frontier no lift can bind, failing the candidate in the PVF.)
	InvalidProof,
	/// The head read's leaf does not decode as a [`Register`].
	BadRegister,
}

/// Storage key of one commitment-tree node: the key prefix it governs.
///
/// An inner node branching at bit `b` governs all stream keys sharing bits
/// `[0, b)` — its key is `len = b` plus exactly those bits. A leaf's key is
/// the stream's full canonical encoding with `len = KEY_BITS`. The mapping
/// is injective over the canonical trie, and stable: inserting a new branch
/// *above* a node never re-keys the node or its subtree.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	Debug,
	Eq,
	PartialEq,
)]
pub struct NodeKey {
	/// Number of significant prefix bits, MSB-first; [`KEY_BITS`] = leaf.
	pub len: u8,
	/// The prefix bits; bits at index `>= len` are zero.
	pub prefix: [u8; STREAM_ID_LEN],
}

impl NodeKey {
	/// The leaf node key of a full stream key.
	fn leaf(key: [u8; STREAM_ID_LEN]) -> Self {
		Self { len: KEY_BITS, prefix: key }
	}

	/// The inner-node key branching at `bit` on `key`'s path.
	fn inner(key: &[u8; STREAM_ID_LEN], bit: u8) -> Self {
		Self { len: bit, prefix: mask_prefix(key, bit) }
	}

	/// `true` iff this key identifies a leaf.
	fn is_leaf(&self) -> bool {
		self.len == KEY_BITS
	}

	/// First bit in `[0, len)` at which `key` diverges from this node's
	/// prefix; `None` if `key` extends the prefix.
	fn divergence(&self, key: &[u8; STREAM_ID_LEN]) -> Option<u8> {
		first_diff_bit(key, &self.prefix).filter(|bit| *bit < self.len)
	}
}

/// Zeroes all bits of `key` at index `>= len`.
fn mask_prefix(key: &[u8; STREAM_ID_LEN], len: u8) -> [u8; STREAM_ID_LEN] {
	let mut prefix = [0u8; STREAM_ID_LEN];
	let full = (len / 8) as usize;
	prefix[..full].copy_from_slice(&key[..full]);
	let partial = len % 8;
	if partial != 0 {
		prefix[full] = key[full] & (0xFF << (8 - partial));
	}
	prefix
}

/// Reference to a commitment-tree node together with its subtree hash.
///
/// Child hashes are stored inline in the parent (and the root's in
/// [`TreeRoot`]): walking down a stream's path collects every sibling hash
/// the bottom-up recomputation needs, without visiting sibling subtrees.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	Debug,
	Eq,
	PartialEq,
)]
pub struct TreeChild {
	/// The child node's storage key ([`NodeKey::is_leaf`] = leaf).
	pub key: NodeKey,
	/// The child's subtree hash — `tree_leaf_hash` for leaves,
	/// `tree_inner_hash` for inner nodes.
	pub hash: Hash,
}

/// A stored inner node of the commitment tree. Its branch bit is its own
/// storage key's `len`; children with bit = 0 are left. Leaves have no
/// stored node — a leaf is fully described by its parent's [`TreeChild`]
/// (or by [`TreeRoot`] in a single-stream tree).
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	Debug,
	Eq,
	PartialEq,
)]
pub struct InnerNode {
	/// The subtree of keys with the branch bit = 0.
	pub left: TreeChild,
	/// The subtree of keys with the branch bit = 1.
	pub right: TreeChild,
}

impl InnerNode {
	fn child(&self, side: u8) -> &TreeChild {
		if side == 0 {
			&self.left
		} else {
			&self.right
		}
	}

	fn child_mut(&mut self, side: u8) -> &mut TreeChild {
		if side == 0 {
			&mut self.left
		} else {
			&mut self.right
		}
	}

	/// This node's hash; `bit` is its branch bit (== its node key's `len`).
	fn node_hash(&self, bit: u8) -> Hash {
		tree_inner_hash(bit, &self.left.hash, &self.right.hash)
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event>> {
		/// Hard per-message payload size bound, in bytes — a consensus
		/// constant of this chain's streams (the advisory
		/// `WindowGrant::max_message_size` rides on top of it).
		#[pallet::constant]
		type MaxMsgLen: Get<u32>;

		/// Per-stream, per-block cap on appended messages — with
		/// [`Config::MaxMsgLen`] the hard, consensus-side backpressure of
		/// the transport.
		#[pallet::constant]
		type MaxMessagesPerBlock: Get<u32>;

		/// Per-block cap on streams the messaging inherent may touch.
		/// Bounds both the `proof_size` lift reservation and — since every
		/// record source carries at least one stream — the consumption
		/// record's sources to at most `MAX_COMMITMENT_ENTRIES` (enforced
		/// by [`Hooks::integrity_test`]).
		#[pallet::constant]
		type MaxTouchedStreams: Get<u32>;

		/// Per-block cap on read-context gaps (register/event reads, which
		/// pick their read context freely and can each open one gap in the
		/// bundle's interval chain). Bounds the `proof_size` advance-proof
		/// reservation.
		#[pallet::constant]
		type MaxContextGaps: Get<u32>;

		/// Where consumed [`SpecMsgKind::Data`] payloads are handed for
		/// execution — the XCM-queue forwarding seam. `()` drops them.
		type DataHandler: OnSpecMsgData;
	}

	/// Per-stream MMR frontiers (peaks + leaf count) — the only long-lived
	/// sender accumulator state. Throughout a block this reflects state as
	/// of the PREVIOUS block: this block's sends are appended at the next
	/// block's `on_initialize`. Roots are computed on demand by bagging;
	/// no stream root is ever stored.
	#[pallet::storage]
	pub type OutboundFrontier<T: Config> =
		StorageMap<_, Twox64Concat, StreamId, MmrFrontier, ValueQuery>;

	/// Messages sent in THIS block only, per stream, kept for node-side
	/// extraction via the runtime API (which reads them in this block's
	/// state). Message `i` sits at position `OutboundFrontier[stream]
	/// .leaf_count + i`; appends go through the `append` host function, so
	/// a send is O(1) and gaps are unrepresentable.
	#[pallet::storage]
	pub type OutboundMessages<T: Config> = StorageMap<
		_,
		Twox64Concat,
		StreamId,
		BoundedVec<BoundedVec<u8, T::MaxMsgLen>, T::MaxMessagesPerBlock>,
		ValueQuery,
	>;

	/// Stream commitment tree inner nodes — a rebuildable cache (the
	/// frontiers determine the whole tree), kept because incremental path
	/// updates need the sibling hashes. Node format per
	/// `cumulus_primitives_spec_messaging::tree`; layout per [`NodeKey`].
	#[pallet::storage]
	pub type TreeNodes<T: Config> = StorageMap<_, Twox64Concat, NodeKey, InnerNode, OptionQuery>;

	/// The commitment tree's root node; `None` until the first stream is
	/// ever touched. Part of the same rebuildable cache as [`TreeNodes`].
	/// Its `hash` is the [`StreamsRoot`] as of the last fold.
	#[pallet::storage]
	pub type TreeRoot<T: Config> = StorageValue<_, TreeChild, OptionQuery>;

	/// The [`StreamsRoot`] committed by the block being executed — the
	/// end-of-block fold's memo, feeding the `Provides` UMP signal.
	/// Transient: set by [`Pallet::commit_streams_root`] iff this block
	/// touched a stream, cleared at the next `on_initialize`.
	#[pallet::storage]
	pub type BlockStreamsRoot<T: Config> = StorageValue<_, StreamsRoot, OptionQuery>;

	/// Outbound channels currently considered in phase `Open`.
	///
	/// PLACEHOLDER for the channel lifecycle machinery (channels & flow
	/// control), which will replace this bare flag with phases derived from
	/// the `OpenChannel` handshake and the peer's register. Until then,
	/// inserting a key here is the manual arming step; a channel absent
	/// here refuses [`Pallet::send`] and makes the XCM router fall through.
	#[pallet::storage]
	pub type OpenOutboundChannels<T: Config> =
		StorageMap<_, Twox64Concat, ChannelId, (), OptionQuery>;

	/// Consumption frontier per consumed channel stream, keyed by the full
	/// stream key `(sender, stream id)` — a chain may consume several
	/// streams of one sender. Position (= leaf count) and the root built
	/// against (bag the peaks) are both derived — never stored.
	#[pallet::storage]
	pub type InboundFrontier<T: Config> =
		StorageMap<_, Twox64Concat, (ParaId, StreamId), MmrFrontier, ValueQuery>;

	/// The full stream keys this runtime currently consumes: channel data
	/// streams of open, non-suspended inbound channels and the ack
	/// registers of its own outbound channels. The messaging inherent
	/// rejects items for streams absent here (the API contract: every
	/// channel item the inherent carries is for a stream
	/// [`Pallet::consumed_streams`] lists, every register read for an ack
	/// register that follows from [`Pallet::out_channels`]).
	///
	/// PLACEHOLDER for the channel lifecycle machinery (channels & flow
	/// control), which will derive this set from the `InChannels` /
	/// `OutChannels` phases instead of storing it. Until then, inserting a
	/// key here is the manual arming step, mirroring
	/// [`OpenOutboundChannels`] on the sender side.
	#[pallet::storage]
	pub type ConsumedStreams<T: Config> =
		StorageMap<_, Twox64Concat, (ParaId, StreamId), (), OptionQuery>;

	/// Transient outbox of this block's consumption intervals — one item
	/// per stream the messaging inherent touched, in processing order (the
	/// same storage family as parachain-system's `UpwardMessages`: written
	/// this block, grouped/sorted at read time by
	/// [`Pallet::consumption_record`], cleared at the next
	/// `on_initialize`).
	///
	/// Unbounded like parachain-system's `UpwardMessages`: transient, and
	/// [`Config::MaxTouchedStreams`] (which also bounds record sources to
	/// `MAX_COMMITMENT_ENTRIES`) is the real bound.
	#[pallet::storage]
	#[pallet::unbounded]
	pub type ConsumptionOutbox<T: Config> =
		StorageValue<_, Vec<(ParaId, StreamId, Interval)>, ValueQuery>;

	#[pallet::error]
	#[derive(PartialEq)]
	pub enum Error<T> {
		/// The payload exceeds [`Config::MaxMsgLen`], the consensus hard
		/// per-message size bound.
		MessageTooBig,
		/// The stream already carries [`Config::MaxMessagesPerBlock`]
		/// messages in this block.
		TooManyMessages,
		/// The outbound channel is not in phase `Open`.
		ChannelNotOpen,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event {
		/// An unparseable payload on an ordered stream was consumed and
		/// dropped: skipping is illegal (the frontier admits no gaps) and
		/// stalling would brick the channel — the version machinery of the
		/// channel lifecycle exists to make this near-impossible.
		PayloadDropped {
			/// The stream's sender.
			source: ParaId,
			/// The stream the payload was consumed from.
			stream: StreamId,
			/// The dropped payload's position in the stream.
			position: MessagePosition,
		},
		/// A messaging-inherent item was rejected; nothing of it was
		/// consumed. An honest collator's pre-verification never produces
		/// a rejectable item, so this signals a buggy or malicious block
		/// author — the block itself stays valid (a rejected item consumes
		/// nothing and needs no lift).
		ItemRejected {
			/// The item's named source chain.
			source: ParaId,
			/// The item's named stream.
			stream: StreamId,
			/// Why the item was rejected.
			reason: RejectReason,
		},
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			// TODO: benchmark; DbWeight-based estimate for now, and the
			// unaccounted `on_finalize` fold (O(k·log S) node writes) needs
			// to be charged here once weights land.
			let mut weight = T::DbWeight::get().reads_writes(1, 1);

			// The previous block's fold memo and consumption outbox die
			// with their block.
			BlockStreamsRoot::<T>::kill();
			ConsumptionOutbox::<T>::kill();

			// Bump the frontiers with the previous block's sends and clear
			// the per-block vecs — one atomic step. From here on the stored
			// frontiers reflect everything sent up to and including the
			// previous block.
			for (stream, messages) in OutboundMessages::<T>::drain() {
				let mut frontier = OutboundFrontier::<T>::get(stream);
				for payload in &messages {
					frontier.append_leaf(hash_leaf::<SpecHasher>(LEAF_VERSION, payload));
				}
				OutboundFrontier::<T>::insert(stream, frontier);
				weight.saturating_accrue(T::DbWeight::get().reads_writes(2, 2));
			}

			weight
		}

		fn on_finalize(_n: BlockNumberFor<T>) {
			Self::commit_streams_root();
		}

		fn integrity_test() {
			// Every record source carries at least one touched stream, so
			// this cap is what keeps the PVF-synthesized `RequiresSet`
			// constructible for every valid block.
			assert!(
				T::MaxTouchedStreams::get() <= MAX_COMMITMENT_ENTRIES,
				"`MaxTouchedStreams` must not exceed `MAX_COMMITMENT_ENTRIES`: the consumption \
				 record's sources could not fit the synthesized `RequiresSet`",
			);
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// The messaging inherent: consume fetched payloads by
		/// recomputation and write the block's consumption record.
		///
		/// Carries no roots of any kind and reads no relay state. Items are
		/// validated and consumed independently; an invalid item is
		/// rejected wholesale with [`Event::ItemRejected`] and the dispatch
		/// still succeeds (the inherent is `Mandatory` — it must never
		/// fail). The weight charges each stream's worst-case lift bytes
		/// and each read's advance-proof bytes as `proof_size` up front;
		/// see the module docs.
		#[pallet::call_index(0)]
		#[pallet::weight((enact_messages_weight::<T>(data), DispatchClass::Mandatory))]
		pub fn enact_messages(origin: OriginFor<T>, data: SpecMsgInherentData) -> DispatchResult {
			ensure_none(origin)?;

			// Streams already touched by this inherent — at most one item
			// per stream, checked across channel items and register reads.
			let mut touched = BTreeSet::new();
			let mut gaps = 0u32;

			for (source, stream, payloads) in data.messages {
				if let Err(reason) =
					Self::consume_channel_item(&mut touched, source, stream, &payloads)
				{
					Self::deposit_event(Event::ItemRejected { source, stream, reason });
				}
			}
			for (source, stream, payload, proof) in data.register_reads {
				if let Err(reason) = Self::consume_register_read(
					&mut touched,
					&mut gaps,
					source,
					stream,
					&payload,
					&proof,
				) {
					Self::deposit_event(Event::ItemRejected { source, stream, reason });
				}
			}

			Ok(())
		}
	}

	/// Weight of [`Pallet::enact_messages`].
	///
	/// `ref_time`: DbWeight-based estimate (TODO: benchmark, together with
	/// the pallet's other weights). `proof_size`: the PoV reservation the
	/// design mandates — lifts and advance proofs are attached by the
	/// submitter outside block execution, so the STF charges the worst case
	/// at authoring time: [`LIFT_RESERVATION_BYTES`] per named stream,
	/// [`ADVANCE_PROOF_RESERVATION_BYTES`] per read context. Charged for
	/// rejected items too (pre-dispatch weight is the worst case).
	fn enact_messages_weight<T: Config>(data: &SpecMsgInherentData) -> Weight {
		let streams = (data.messages.len() + data.register_reads.len()) as u64;
		let payloads: u64 = data.messages.iter().map(|(_, _, p)| p.len() as u64).sum();

		// Per stream: consumed-set read, frontier read + write, outbox
		// append; per payload: leaf hashing and the handler, estimated as
		// one read until benchmarks land.
		T::DbWeight::get()
			.reads_writes(2 * streams + payloads, 2 * streams)
			.saturating_add(Weight::from_parts(
				0,
				streams.saturating_mul(LIFT_RESERVATION_BYTES).saturating_add(
					(data.register_reads.len() as u64)
						.saturating_mul(ADVANCE_PROOF_RESERVATION_BYTES),
				),
			))
	}

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = MakeFatalError<()>;
		const INHERENT_IDENTIFIER: InherentIdentifier =
			cumulus_primitives_spec_messaging::INHERENT_IDENTIFIER;

		fn create_inherent(data: &InherentData) -> Option<Self::Call> {
			let data = data
				.get_data::<SpecMsgInherentData>(&Self::INHERENT_IDENTIFIER)
				.ok()
				.flatten()?;
			// An absent inherent is a valid block that consumes nothing —
			// nothing to fetch must not cost an extrinsic.
			(!data.is_empty()).then(|| Call::enact_messages { data })
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::enact_messages { .. })
		}
	}

	impl<T: Config> Pallet<T> {
		/// Appends one payload (already SCALE-encoded `SpecMsgKind`) to this
		/// block's vec for `stream` and returns the message's position in
		/// the stream's MMR.
		///
		/// This is the internal primitive under the channel layer's `send`;
		/// it enforces only the consensus hard caps
		/// ([`Config::MaxMsgLen`], [`Config::MaxMessagesPerBlock`]) —
		/// channel phase and credit gating happen in the caller. On error,
		/// state is untouched.
		pub fn append_to_stream(
			stream: StreamId,
			payload: Vec<u8>,
		) -> Result<MessagePosition, Error<T>> {
			let payload: BoundedVec<u8, T::MaxMsgLen> =
				payload.try_into().map_err(|_| Error::<T>::MessageTooBig)?;

			// The stored frontier holds state as of the previous block all
			// block long, so the position is stable from here on.
			let index = OutboundMessages::<T>::decode_len(stream).unwrap_or(0) as u64;
			OutboundMessages::<T>::try_append(stream, payload)
				.map_err(|()| Error::<T>::TooManyMessages)?;

			Ok(MessagePosition(OutboundFrontier::<T>::get(stream).leaf_count + index))
		}

		/// The data stream carrying an OUTBOUND channel's messages: same
		/// discriminators, `Channel` kind, addressed to the peer. (Inbound
		/// channels live in the peer's key space and have no stream here.)
		pub fn outbound_stream(channel: &ChannelId) -> StreamId {
			StreamId::Channel { recipient: channel.peer, domain: channel.domain, num: channel.num }
		}

		/// Whether the outbound channel is in phase `Open`.
		///
		/// PLACEHOLDER semantics until the channel lifecycle machinery
		/// lands: reads the bare [`OpenOutboundChannels`] flag instead of
		/// deriving the phase from the handshake and the peer's register.
		pub fn is_outbound_channel_open(channel: &ChannelId) -> bool {
			OpenOutboundChannels::<T>::contains_key(channel)
		}

		/// Whether the channel layer would currently accept a
		/// [`Pallet::send`] of `data_len` payload bytes: the encoded
		/// [`SpecMsgKind::Data`] leaf must fit [`Config::MaxMsgLen`] and the
		/// channel stream's per-block vec must have room. Side-effect free —
		/// this is the fail-fast check the XCM router runs at `validate`.
		///
		/// TODO(channels & flow control): also gate on the peer's advisory
		/// credit window (`WindowGrant` beyond its watermark) here.
		pub fn can_send(channel: &ChannelId, data_len: usize) -> Result<(), Error<T>> {
			// The leaf payload is the SCALE-encoded `SpecMsgKind::Data`:
			// 1-byte variant tag + compact length prefix + the data bytes.
			let data_len32 = u32::try_from(data_len).unwrap_or(u32::MAX);
			let payload_len = (data_len as u64)
				.saturating_add(1)
				.saturating_add(codec::Compact(data_len32).encoded_size() as u64);
			frame_support::ensure!(
				payload_len <= u64::from(T::MaxMsgLen::get()),
				Error::<T>::MessageTooBig
			);

			let stream = Self::outbound_stream(channel);
			let queued = OutboundMessages::<T>::decode_len(stream).unwrap_or(0);
			frame_support::ensure!(
				queued < T::MaxMessagesPerBlock::get() as usize,
				Error::<T>::TooManyMessages
			);
			Ok(())
		}

		/// The channel layer's outbound `send`: wraps `data` as a
		/// [`SpecMsgKind::Data`] leaf — on the designated XCM channel the
		/// data is exactly a SCALE-encoded `VersionedXcm`, no extra framing
		/// — and appends it to the channel's data stream, returning the
		/// message's position. On error, state is untouched.
		///
		/// TODO(channels & flow control): debit the advisory credit window
		/// here; today only the placeholder open flag and the consensus
		/// hard caps of [`Pallet::append_to_stream`] gate the send.
		pub fn send(channel: ChannelId, data: Vec<u8>) -> Result<MessagePosition, Error<T>> {
			frame_support::ensure!(
				Self::is_outbound_channel_open(&channel),
				Error::<T>::ChannelNotOpen
			);
			Self::append_to_stream(
				Self::outbound_stream(&channel),
				SpecMsgKind::Data(data).encode(),
			)
		}

		/// Runs the end-of-block commitment fold — idempotent, at most once
		/// per block: computes every touched stream's new MMR root
		/// *transiently* (stored frontier + this block's leaves, in
		/// memory), folds the roots into the stored commitment tree (one
		/// path write per touched stream), memoizes the resulting
		/// [`StreamsRoot`] and deposits the header digest.
		///
		/// Returns `None` on idle blocks: nothing folded, nothing
		/// memoized, nothing deposited. This is what the `Provides` UMP
		/// signal emission hook calls — an unchanged root is never
		/// re-emitted.
		pub fn commit_streams_root() -> Option<StreamsRoot> {
			if let Some(root) = BlockStreamsRoot::<T>::get() {
				// The fold already ran in this block.
				return Some(root);
			}

			let mut touched = false;
			for (stream, messages) in OutboundMessages::<T>::iter() {
				let mut frontier = OutboundFrontier::<T>::get(stream);
				for payload in &messages {
					frontier.append_leaf(hash_leaf::<SpecHasher>(LEAF_VERSION, payload));
				}
				Self::upsert_tree_leaf(&stream, tree_leaf_hash(&stream, &frontier.root()));
				touched = true;
			}
			if !touched {
				return None;
			}

			let root = TreeRoot::<T>::get()
				.map(|node| StreamsRoot(node.hash))
				.expect("at least one leaf was just folded, so the tree is non-empty; qed");
			BlockStreamsRoot::<T>::put(root);
			frame_system::Pallet::<T>::deposit_log(DigestItem::Consensus(
				SPMS_ENGINE_ID,
				root.encode(),
			));

			Some(root)
		}

		/// The [`StreamsRoot`] committed by the current block, if the fold
		/// ran and a stream was touched.
		pub fn current_streams_root() -> Option<StreamsRoot> {
			BlockStreamsRoot::<T>::get()
		}

		/// The block's consumption record: the transient outbox grouped by
		/// source, per source sorted by [`StreamId`] (canonical encoding
		/// order) — the shape the `validate_block` wrapper stitches and the
		/// `consumption_record()` runtime API serves. One definition, two
		/// callers: the node reaches it via API dispatch, the wrapper calls
		/// it directly in-wasm — the records must be byte-identical.
		pub fn consumption_record() -> ConsumptionRecord {
			let mut record = ConsumptionRecord::default();
			for (source, stream, interval) in ConsumptionOutbox::<T>::get() {
				record.entries.entry(source).or_default().push((stream, interval));
			}
			for streams in record.entries.values_mut() {
				streams.sort_by_key(|(stream, _)| *stream);
			}
			record
		}

		/// THIS block's sends, per touched stream — what the
		/// `outbound_messages()` runtime API serves and a collator extracts
		/// for delivery and appends to its archive, calling at the built
		/// block (block N's sends live in block N's state; see the sender
		/// lifecycle in the module docs). Entries in canonical [`StreamId`]
		/// order; payload `i` of a stream's vec sits at position
		/// `OutboundFrontier[stream].leaf_count + i`. Empty on idle blocks.
		pub fn outbound_messages() -> Vec<(StreamId, Vec<Vec<u8>>)> {
			let mut messages: Vec<(StreamId, Vec<Vec<u8>>)> = OutboundMessages::<T>::iter()
				.map(|(stream, payloads)| {
					(stream, payloads.into_iter().map(BoundedVec::into_inner).collect())
				})
				.collect();
			messages.sort_by_key(|(stream, _)| *stream);
			messages
		}

		/// Everything this runtime currently consumes, grouped by source
		/// with per-stream resume cursors (= inbound frontier leaf counts) —
		/// what the `consumed_streams()` runtime API serves: the own
		/// collators fetch what is listed, from the cursor on, and stop
		/// fetching what is omitted. Per source in canonical [`StreamId`]
		/// order. Ack registers are deliberately absent (no resume state;
		/// which registers to read follows from [`Pallet::out_channels`]).
		///
		/// PLACEHOLDER derivation until the channel lifecycle machinery
		/// lands: projects the manually armed [`ConsumedStreams`] set —
		/// suspension will express itself as omission here.
		pub fn consumed_streams() -> BTreeMap<ParaId, Vec<ConsumedStream>> {
			let mut grouped = BTreeMap::<ParaId, Vec<(StreamId, ConsumedStream)>>::new();
			for (source, stream) in ConsumedStreams::<T>::iter_keys() {
				let cursor =
					MessagePosition(InboundFrontier::<T>::get((source, stream)).leaf_count);
				if let Some(consumed) = ConsumedStream::project(&stream, cursor) {
					grouped.entry(source).or_default().push((stream, consumed));
				}
			}
			grouped
				.into_iter()
				.map(|(source, mut streams)| {
					streams.sort_by_key(|(stream, _)| *stream);
					(source, streams.into_iter().map(|(_, consumed)| consumed).collect())
				})
				.collect()
		}

		/// Channel views, outbound direction — what the `out_channels()`
		/// runtime API serves: standing for authoring decisions and, via the
		/// keys, which ack registers the own collators read.
		///
		/// PLACEHOLDER derivation until the channel lifecycle machinery
		/// lands: every armed [`OpenOutboundChannels`] key maps to the
		/// default state (version-0 announcement, no register read yet) —
		/// the view's shape is the API contract, the lifecycle fills in the
		/// fields.
		pub fn out_channels() -> BTreeMap<ChannelId, OutChannelState> {
			OpenOutboundChannels::<T>::iter_keys()
				.map(|channel| (channel, OutChannelState::default()))
				.collect()
		}

		/// Channel views, inbound direction — what the `in_channels()`
		/// runtime API serves: which channels are due a register publish
		/// check, suspension standing, diagnostics.
		///
		/// PLACEHOLDER derivation until the channel lifecycle machinery
		/// lands: every armed channel data stream in [`ConsumedStreams`]
		/// maps to the default state (no register published yet, version-0
		/// peer announcement, not suspended), mirroring
		/// [`Pallet::out_channels`].
		pub fn in_channels() -> BTreeMap<ChannelId, InChannelState> {
			ConsumedStreams::<T>::iter_keys()
				.filter_map(|(source, stream)| match stream {
					StreamId::Channel { domain, num, .. } => {
						Some((ChannelId { peer: source, domain, num }, InChannelState::default()))
					},
					_ => None,
				})
				.collect()
		}

		/// Consumes one channel item of the messaging inherent: hashes the
		/// payloads into the stream's [`InboundFrontier`] in order and
		/// records the stream's [`Interval`]. On `Err` nothing was
		/// consumed.
		///
		/// Order and count of the payloads need no explicit check — any
		/// deviation yields a different frontier endpoint, which no lift
		/// can bind (PVF-enforced).
		fn consume_channel_item(
			touched: &mut BTreeSet<(ParaId, StreamId)>,
			source: ParaId,
			stream: StreamId,
			payloads: &[Vec<u8>],
		) -> Result<(), RejectReason> {
			// Ordered consumption is defined on channel data streams only.
			ensure!(matches!(stream, StreamId::Channel { .. }), RejectReason::UnknownStream);
			ensure!(
				ConsumedStreams::<T>::contains_key((source, stream)),
				RejectReason::UnknownStream
			);
			ensure!(!touched.contains(&(source, stream)), RejectReason::DuplicateStream);
			ensure!(!payloads.is_empty(), RejectReason::EmptyItem);
			ensure!(
				payloads.iter().all(|p| p.len() <= T::MaxMsgLen::get() as usize),
				RejectReason::OversizedPayload
			);
			// The caps count accepted items only: a rejected item consumes
			// neither a slot nor state.
			ensure!(
				(touched.len() as u32) < T::MaxTouchedStreams::get(),
				RejectReason::TooManyStreams
			);
			touched.insert((source, stream));

			let mut frontier = InboundFrontier::<T>::get((source, stream));
			let start = frontier.root();
			for payload in payloads {
				let position = MessagePosition(frontier.leaf_count);
				frontier.append_leaf(hash_leaf::<SpecHasher>(LEAF_VERSION, payload));

				match SpecMsgKind::decode_all(&mut &payload[..]) {
					Ok(SpecMsgKind::Data(data)) => {
						T::DataHandler::on_data(source, stream, position, data)
					},
					Ok(SpecMsgKind::Signal(signal)) => Self::apply_signal(source, stream, signal),
					// Consume-and-drop: skipping is illegal, stalling
					// would brick the channel.
					Err(_) => {
						Self::deposit_event(Event::PayloadDropped { source, stream, position })
					},
				}
			}
			InboundFrontier::<T>::insert((source, stream), &frontier);
			ConsumptionOutbox::<T>::append((source, stream, Interval { start, end: frontier }));
			Ok(())
		}

		/// Consumes one register head read of the messaging inherent: the
		/// inclusion proof pins the read's position and peaks (the yielded
		/// root is derived, never declared), and the read context is
		/// recorded as `Interval { start: root(end), end }` — nothing
		/// advances. On `Err` nothing was consumed.
		fn consume_register_read(
			touched: &mut BTreeSet<(ParaId, StreamId)>,
			gaps: &mut u32,
			source: ParaId,
			stream: StreamId,
			payload: &[u8],
			proof: &MmrInclusionProof,
		) -> Result<(), RejectReason> {
			// Head reads are defined on ack streams (broadcast event
			// streams share the discipline but are post-MVP).
			ensure!(matches!(stream, StreamId::Ack { .. }), RejectReason::UnknownStream);
			ensure!(
				ConsumedStreams::<T>::contains_key((source, stream)),
				RejectReason::UnknownStream
			);
			ensure!(!touched.contains(&(source, stream)), RejectReason::DuplicateStream);

			let leaf = hash_leaf::<SpecHasher>(LEAF_VERSION, payload);
			let (position, frontier) =
				proof.verify_head(leaf).map_err(|_| RejectReason::InvalidProof)?;
			let register =
				Register::decode_all(&mut &payload[..]).map_err(|_| RejectReason::BadRegister)?;

			ensure!(
				(touched.len() as u32) < T::MaxTouchedStreams::get(),
				RejectReason::TooManyStreams
			);
			ensure!(*gaps < T::MaxContextGaps::get(), RejectReason::TooManyGaps);
			touched.insert((source, stream));
			*gaps += 1;

			Self::apply_register_read(source, stream, position, register);
			ConsumptionOutbox::<T>::append((
				source,
				stream,
				Interval { start: frontier.root(), end: frontier },
			));
			Ok(())
		}

		/// Applies an in-band lifecycle signal consumed from `source`'s
		/// channel stream.
		///
		/// TODO(channels & flow control): the inbound lifecycle machinery
		/// lands here — `OpenChannel` into the inbound channel state,
		/// half-close, version upgrades. Until then signals are consumed
		/// (the frontier advances; skipping is illegal) and dropped.
		fn apply_signal(_source: ParaId, _stream: StreamId, _signal: SpecMsgSignal) {}

		/// Applies a verified register head read to the outbound channel's
		/// state.
		///
		/// TODO(channels & flow control): update the outbound channel state
		/// monotonic-safely (a register regressing `up_to` or `version` is
		/// ignored, `position` orders competing reads), unlock the credit
		/// window and the pruning watermark. Until then the read only
		/// records its context interval.
		fn apply_register_read(
			_source: ParaId,
			_stream: StreamId,
			_position: MessagePosition,
			_register: Register,
		) {
		}

		/// Upserts one stream's leaf hash into the stored commitment tree
		/// and recomputes the hashes along its path — O(log S) node
		/// reads/writes, everything off the path untouched.
		fn upsert_tree_leaf(id: &StreamId, leaf_hash: Hash) {
			let key = id.to_bytes();
			let leaf = TreeChild { key: NodeKey::leaf(key), hash: leaf_hash };

			let Some(root) = TreeRoot::<T>::get() else {
				// First stream ever: the single leaf is the whole tree.
				TreeRoot::<T>::put(leaf);
				return;
			};

			// Walk down to the leaf's slot, remembering the traversed inner
			// nodes and which side the path took.
			let mut path: Vec<(NodeKey, InnerNode, u8)> = Vec::new();
			let mut cursor = root;
			let mut updated = loop {
				if let Some(bit) = cursor.key.divergence(&key) {
					// `key` splits off the cursor's subtree: a fresh inner
					// node branching at the first differing bit adopts the
					// subtree and the new leaf. The subtree keeps its node
					// key — prefixes never change on insertion above.
					let node_key = NodeKey::inner(&key, bit);
					let node = if bit_at(&key, bit) == 0 {
						InnerNode { left: leaf, right: cursor }
					} else {
						InnerNode { left: cursor, right: leaf }
					};
					TreeNodes::<T>::insert(node_key, node);
					break TreeChild { key: node_key, hash: node.node_hash(bit) };
				}
				if cursor.key.is_leaf() {
					// Exact key match: the stream is already in the tree;
					// its entry gets the new hash.
					break leaf;
				}
				let node = TreeNodes::<T>::get(cursor.key).expect(
					"TreeRoot and stored child pointers only reference existing inner nodes; qed",
				);
				let side = bit_at(&key, cursor.key.len);
				let next = *node.child(side);
				path.push((cursor.key, node, side));
				cursor = next;
			};

			// Recompute the path bottom-up: each traversed node gets the
			// updated child (whose pointer may now be a fresh inner node)
			// and a recomputed hash, using the untouched siblings in place.
			while let Some((node_key, mut node, side)) = path.pop() {
				*node.child_mut(side) = updated;
				TreeNodes::<T>::insert(node_key, node);
				updated = TreeChild { key: node_key, hash: node.node_hash(node_key.len) };
			}
			TreeRoot::<T>::put(updated);
		}
	}
}

/// Hook for `cumulus-pallet-parachain-system` (`Config::UmpSignalSource`):
/// sources the `Provides` UMP signal from the end-of-block fold and the
/// consumption record for the `validate_block` wrapper's `Requires`
/// synthesis.
///
/// `provides_root` runs the fold if this block's `on_finalize` ordering has
/// not yet (idempotent via the `BlockStreamsRoot` memo) — but a payload
/// appended *after* the first fold of a block still misses it; the "append
/// before `on_finalize`" ordering rule (see the pallet doc) now covers
/// parachain-system's `on_finalize` as well.
impl<T: Config> ProvideUmpSignals for Pallet<T> {
	fn provides_root() -> Option<UMPSignal> {
		Self::commit_streams_root().map(UMPSignal::Provides)
	}

	fn consumption_record() -> ConsumptionRecord {
		Pallet::<T>::consumption_record()
	}
}
