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
//! ## Channels and flow control
//!
//! Channels are unidirectional and entirely bilateral — nothing here
//! involves the relay chain. One channel = the sender's ordered data
//! stream (`Channel{recipient, domain, num}`: userspace `Data` plus
//! lifecycle `Signal` leaves) and the receiver's register stream
//! (`Ack{recipient: sender, domain, num}`: lossy, latest-wins). The
//! receiver's entire voice is its [`Register`] — acceptance (its
//! existence), consumption watermark, advisory credit grant, close.
//!
//! - **Opening**: [`Pallet::open_channel`] creates the [`OutChannels`] entry (phase `Opening`) and
//!   emits the `OpenChannel` signal — the one message sendable without credit (no register exists
//!   yet), window-counted like everything after it. [`Pallet::accept_open_channel`] is the
//!   receiver's local, receiver-priced acceptance: the [`InChannels`] entry joins the consumed set
//!   and the initial register is published. Either order works (accept-first = pre-authorization);
//!   an unaccepted open leaves ZERO receiver state.
//! - **Flow control**: everything on the data stream counts against the peer's granted window;
//!   in-flight = count + byte sum of leaves at positions ≥ the read watermark, and [`Pallet::send`]
//!   requires in-flight strictly below the grant on both limits. Enforcement is this chain's own
//!   STF — the grant is advice; the receiver's levers are the grant itself, suspension and
//!   abandonment. Register reads arrive via the messaging inherent, monotonic (`up_to`/`version`
//!   regressions are ignored, leaf position orders competing reads); the read watermark also drives
//!   the node-side archive pruning.
//! - **Register publishing**: on acceptance, then when consumption progressed ~¼ of the granted
//!   window or the [`Config::RegisterPublishAge`] threshold expired, whichever first — through the
//!   ordinary sender machinery (one tree, one root).
//! - **Closing**: [`Pallet::close_channel`] sends the in-band `CloseChannel` (credit-gated;
//!   abandonment is the no-credit fallback); [`Pallet::close_inbound_channel`] publishes `closed:
//!   true` (grant void, `up_to` still reports consumption). Both advisory: frontiers survive,
//!   unconfirmed tails stay deliverable, [`Pallet::open_channel`] reopens over them — after a
//!   sender half-close without re-acceptance.
//! - **Suspension**: [`Pallet::suspend_inbound_channel`] is the upper-layer pause: the STF refuses
//!   the channel's messages, `consumed_streams()` omits the stream, published registers grant zero.
//!   [`Pallet::resume_inbound_channel`] republishes a real grant.
//! - **Versioning** (MVP-minimal): monotonic announcements — in-band out (`OpenChannel`/`Upgrade`),
//!   in the register back; effective = min of the two latest. MVP announces [`PROTOCOL_VERSION`] =
//!   0 everywhere and gates nothing; the machinery ships so v1 features are a payload change, not a
//!   protocol change.
//!
//! The per-block caps ([`Config::MaxMessagesPerBlock`],
//! [`Config::MaxMsgLen`]) remain the hard, consensus-side backpressure
//! underneath the advisory window grants.
//!
//! For the HRMP→spec-msg cutover the pallet carries the [`HrmpClosing`]
//! flag (set/cleared by [`Config::ChannelManagementOrigin`]): while a peer
//! is flagged, [`SpecMsgRouter`] treats the pair's still-open HRMP channel
//! as `Closed`, so new traffic diverts to spec-msg immediately while the
//! already-queued HRMP messages drain ahead of the relay-side closure —
//! drain-before-close without a traffic pause. The full per-pair cutover
//! sequence is on [`Pallet::set_hrmp_closing`].
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
//! The consumed-stream set is derived from the channel state: data streams
//! of accepted, live, non-suspended [`InChannels`] entries, plus the ack
//! registers following from [`OutChannels`]. Consumed [`SpecMsgKind::Data`]
//! payloads go to [`Config::DataHandler`] — runtimes wire
//! [`EnqueueToXcmQueue`] there to forward them into the message queue for
//! XCM execution, under an origin (`SpecMsg(source)`) indistinguishable
//! from the HRMP one; consumed signals and register reads feed the channel
//! lifecycle above.
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
	ChannelId, ChannelPhase, ConsumedStream, ConsumptionRecord, InChannelState, Interval,
	MessagePosition, MmrFrontier, MmrInclusionProof, OutChannelState, ProvideUmpSignals, Register,
	SpecHasher, SpecMsgInherentData, SpecMsgKind, SpecMsgSignal, StreamId, StreamsRoot,
	WindowGrant, LEAF_VERSION, SPMS_ENGINE_ID, STREAM_ID_LEN,
};
use polkadot_core_primitives::Hash;
use scale_info::TypeInfo;
use sp_runtime::{generic::DigestItem, Saturating};

pub use pallet::*;
pub use xcm_router::SpecMsgRouter;

/// The channel protocol version this implementation announces — in every
/// `OpenChannel` signal and every published register. MVP: 0, gating
/// nothing; the announcement machinery (monotonicity checks, min rule)
/// ships so v1 features are a payload change, not a protocol change.
pub const PROTOCOL_VERSION: u8 = 0;

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
	/// The stream is not one this runtime consumes: addressed to another
	/// chain, an unaccepted/closed/suspended inbound channel, no outbound
	/// channel matching the head read's ack stream, or the wrong stream
	/// kind for the item (ordered consumption is defined on `Channel`
	/// streams, head reads on `Ack` streams).
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

/// Sender-side flow bookkeeping of one outbound channel, kept next to the
/// [`OutChannels`] view state (the stored view value stays exactly the API
/// contract; this is pallet-internal): the in-flight window backing the
/// credit gate and the ordering position of applied register reads.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, TypeInfo, Debug, Default, Eq, PartialEq)]
pub struct OutChannelMeta {
	/// Stream position of the oldest in-flight (sent, unconfirmed)
	/// message — `sizes[0]`; everything below is confirmed by the peer's
	/// read watermark.
	pub base: MessagePosition,
	/// Encoded leaf sizes of the in-flight messages, oldest first —
	/// everything on the data stream counts (`Data` AND `Signal` leaves).
	/// The length is the in-flight message count; bounded in practice by
	/// the honored grant plus the per-block caps.
	pub sizes: Vec<u32>,
	/// Byte sum of `sizes` (maintained, not recomputed).
	pub bytes: u64,
	/// Ack-stream leaf position of the last applied register read:
	/// competing head reads across blocks are ordered by position, so an
	/// older head can never overwrite a newer application (e.g. roll a
	/// grant shrink back).
	pub read_at: Option<MessagePosition>,
}

impl OutChannelMeta {
	/// Accounts one appended leaf of `size` encoded bytes.
	fn account_send(&mut self, size: u32) {
		self.sizes.push(size);
		self.bytes = self.bytes.saturating_add(u64::from(size));
	}

	/// Releases everything below the confirmation watermark `up_to`. A
	/// watermark beyond what was sent (a peer over-claiming) merely empties
	/// the window — the grant is advice, over-crediting is the peer's own
	/// concession.
	fn confirm(&mut self, up_to: MessagePosition) {
		let confirmed = up_to.0.saturating_sub(self.base.0).min(self.sizes.len() as u64);
		for size in self.sizes.drain(..confirmed as usize) {
			self.bytes = self.bytes.saturating_sub(u64::from(size));
		}
		self.base.0 = self.base.0.max(up_to.0);
	}
}

/// Register publish bookkeeping of one inbound channel — drives the
/// publish policy: ~¼ of the granted window consumed since the last
/// publish, or the age threshold, whichever first.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	Debug,
	Default,
	Eq,
	PartialEq,
)]
pub struct InChannelMeta<BlockNumber> {
	/// Block the channel's register was last published at.
	pub published_at: BlockNumber,
	/// Data-stream leaf bytes consumed since the last publish.
	pub bytes_since: u64,
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
	use frame_support::{
		pallet_prelude::*,
		traits::{Consideration, Footprint},
	};
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event>> {
		/// This parachain's own id. Consumed streams are always addressed
		/// to the consuming chain (the uniform addressing rule), so the
		/// receiver half needs it to name what it consumes — and to refuse
		/// inherent items addressed elsewhere.
		type SelfParaId: Get<ParaId>;

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

		/// Origin allowed to manage channel state operationally: the
		/// [`HrmpClosing`] cutover flag and inbound-channel
		/// suspension/resumption. Usually root or a governance body.
		type ChannelManagementOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Origin allowed to open (and half-close) outbound channels —
		/// each open creates permanent state (the stream, its frontier,
		/// the [`OutChannels`] entry) and exposes one signal leaf toward
		/// the named peer.
		type OpenChannelOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Origin allowed to accept (and close) inbound channels.
		/// Acceptance is the receiver's local, receiver-priced decision;
		/// rejection = never executing it, which costs nothing.
		type AcceptChannelOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The optional acceptance deposit: a [`Consideration`] held from
		/// a *signed* accepting account for the permanent state the
		/// acceptance creates (the [`InChannels`] entry, the ack stream +
		/// frontier); acceptances by non-signed origins (root, governance
		/// bodies) create the state for free. Wire `()` to charge nothing.
		type AcceptConsideration: Consideration<Self::AccountId, Footprint>;

		/// The advisory credit window granted to live inbound channels'
		/// registers. Advice, not enforcement — the sender's own STF turns
		/// it into its local gate; this chain's hard backpressure stays
		/// [`Config::MaxMsgLen`] / [`Config::MaxMessagesPerBlock`].
		#[pallet::constant]
		type DefaultWindowGrant: Get<WindowGrant>;

		/// Age threshold of the register publish policy: an inbound
		/// channel with unreported consumption progress republishes its
		/// register at the latest this many blocks after the previous
		/// publish (the ~¼-window progress trigger fires earlier on busy
		/// channels; publishing every block would be sound, just
		/// pointless).
		#[pallet::constant]
		type RegisterPublishAge: Get<BlockNumberFor<Self>>;
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

	/// Sender-side state per outbound channel (`peer` = the recipient) —
	/// the `out_channels()` view. Phases are views over the fields
	/// ([`OutChannelState::phase`]): `Opening` until the peer's register
	/// is first read (its existence IS the acceptance), `Open` while
	/// neither side closed, `Closed` on either side's close. Entries are
	/// never removed: streams and frontiers are eternal, close and reopen
	/// only layer over them.
	#[pallet::storage]
	pub type OutChannels<T: Config> =
		StorageMap<_, Twox64Concat, ChannelId, OutChannelState, OptionQuery>;

	/// Sender-side flow bookkeeping per outbound channel (the in-flight
	/// window behind the credit gate, register-read ordering), split from
	/// [`OutChannels`] so the stored view value stays exactly the API
	/// contract. Unbounded: the in-flight size list is bounded in practice
	/// by the honored grant plus the per-block caps.
	#[pallet::storage]
	#[pallet::unbounded]
	pub type OutChannelsMeta<T: Config> =
		StorageMap<_, Twox64Concat, ChannelId, OutChannelMeta, ValueQuery>;

	/// Receiver-side state per inbound channel (`peer` = the channel's
	/// sender) — the `in_channels()` view. An entry's existence is the
	/// acceptance; [`Pallet::consumed_streams`] derives the consumed set
	/// from the live (non-suspended, non-closed) entries. Entries are
	/// never removed — the receiver's one obligation across close/reopen
	/// is retaining the consumption frontier.
	#[pallet::storage]
	pub type InChannels<T: Config> =
		StorageMap<_, Twox64Concat, ChannelId, InChannelState, OptionQuery>;

	/// Register publish bookkeeping per inbound channel.
	#[pallet::storage]
	pub type InChannelsMeta<T: Config> =
		StorageMap<_, Twox64Concat, ChannelId, InChannelMeta<BlockNumberFor<T>>, ValueQuery>;

	/// Acceptance deposits: the [`Config::AcceptConsideration`] ticket
	/// held for the permanent state an acceptance created, with the
	/// depositing account. Held for the channel's lifetime — the state it
	/// prices never goes away — so a re-acceptance after a receiver close
	/// never charges twice.
	#[pallet::storage]
	pub type AcceptanceTickets<T: Config> =
		StorageMap<_, Twox64Concat, ChannelId, (T::AccountId, T::AcceptConsideration), OptionQuery>;

	/// Sibling peers whose HRMP channel pair is mid HRMP→spec-msg cutover
	/// (drain-before-close): while a peer is flagged here the XCM router
	/// ([`SpecMsgRouter`]) treats the pair's HRMP channel as `Closed` even
	/// though `get_channel_status` still reports it open — new traffic
	/// diverts to spec-msg immediately while the already-queued HRMP
	/// messages keep draining through `XcmpQueue` for the remainder of the
	/// session, so the pipe is empty when the relay-side closure enacts.
	///
	/// Set via [`Pallet::set_hrmp_closing`] in the same governance batch as
	/// the relay's `hrmp.close_channel`; cleared on rollback
	/// ([`Pallet::clear_hrmp_closing`]). Once the HRMP channel is actually
	/// gone the flag is inert — the channel status reports `Closed` by
	/// itself — but it MUST be cleared before an HRMP re-open may take
	/// routing precedence again.
	#[pallet::storage]
	pub type HrmpClosing<T: Config> = StorageMap<_, Twox64Concat, ParaId, (), OptionQuery>;

	/// Consumption frontier per consumed channel stream, keyed by the full
	/// stream key `(sender, stream id)` — a chain may consume several
	/// streams of one sender. Position (= leaf count) and the root built
	/// against (bag the peaks) are both derived — never stored.
	#[pallet::storage]
	pub type InboundFrontier<T: Config> =
		StorageMap<_, Twox64Concat, (ParaId, StreamId), MmrFrontier, ValueQuery>;

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
		/// The send would exceed the peer's granted credit window
		/// (in-flight must stay strictly below the grant on both the
		/// message and the byte limit). Backpressure, not failure: capacity
		/// returns with the next register read advancing the watermark.
		NoCredit,
		/// `open_channel`: the channel is already `Opening` or `Open`.
		AlreadyOpen,
		/// `accept_open_channel`: the channel is already accepted and not
		/// receiver-closed.
		AlreadyAccepted,
		/// The channel is already closed on this side.
		AlreadyClosed,
		/// No channel state exists for the given discriminators.
		UnknownChannel,
		/// Channels to this chain itself are meaningless.
		ChannelToSelf,
		/// `suspend_inbound_channel`: already suspended.
		AlreadySuspended,
		/// `resume_inbound_channel`: not suspended.
		NotSuspended,
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
		/// An outbound channel was opened (or reopened): the `OpenChannel`
		/// signal is on the stream. `channel.peer` is the recipient.
		ChannelOpened {
			/// The opened outbound channel.
			channel: ChannelId,
		},
		/// This chain half-closed an outbound channel: the `CloseChannel`
		/// signal is on the stream, nothing further will be sent until a
		/// reopen.
		ChannelClosed {
			/// The half-closed outbound channel.
			channel: ChannelId,
		},
		/// An inbound channel was accepted (or re-accepted after a
		/// receiver-side close): its data stream joined the consumed set
		/// and the initial register is published. `channel.peer` is the
		/// channel's sender.
		ChannelAccepted {
			/// The accepted inbound channel.
			channel: ChannelId,
		},
		/// This chain closed an inbound channel: a `closed: true` register
		/// is published, the stream left the consumed set. The consumption
		/// frontier is retained for a later re-acceptance.
		InboundChannelClosed {
			/// The closed inbound channel.
			channel: ChannelId,
		},
		/// An inbound channel was suspended (paused, not closed): the STF
		/// refuses its messages, `consumed_streams()` omits the stream and
		/// the published register grants zero.
		ChannelSuspended {
			/// The suspended inbound channel.
			channel: ChannelId,
		},
		/// A suspended inbound channel was resumed: a real grant is
		/// republished, consumption restarts from the retained frontier.
		ChannelResumed {
			/// The resumed inbound channel.
			channel: ChannelId,
		},
		/// A register was published on an inbound channel's ack stream —
		/// the receiver's entire voice: acceptance, watermark, grant,
		/// close.
		RegisterPublished {
			/// The inbound channel the register speaks for.
			channel: ChannelId,
			/// The published register.
			register: Register,
		},
		/// A verified register read regressed `up_to` or `version` — a
		/// protocol violation by the peer. The read was ignored (the
		/// previous one stands); repeated violations are grounds for
		/// close or abandonment.
		RegisterRegressed {
			/// The outbound channel whose peer published the regression.
			channel: ChannelId,
		},
		/// The HRMP→spec-msg cutover flag was set: the XCM router now
		/// treats the pair's HRMP channel as `Closed`, diverting new
		/// traffic to spec-msg while the queued HRMP messages drain.
		HrmpClosingSet {
			/// The sibling whose HRMP channel pair is closing.
			peer: ParaId,
		},
		/// The cutover flag was cleared (rollback): the router prefers the
		/// HRMP channel again while one exists.
		HrmpClosingCleared {
			/// The sibling whose cutover was rolled back.
			peer: ParaId,
		},
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
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

			// Age sweep of the register publish policy: inbound channels
			// with unreported consumption progress republish at the latest
			// every `RegisterPublishAge` blocks — the backstop for channels
			// whose progress stays below the ¼-window trigger and then goes
			// quiet (the sender needs the watermark to reclaim credit and
			// prune its archive). O(inbound channels) per block — fine at
			// MVP channel counts; a due queue replaces this when channels
			// multiply.
			let age = T::RegisterPublishAge::get();
			let due: Vec<(ChannelId, InChannelState)> = InChannels::<T>::iter()
				.filter(|(channel, state)| {
					weight.saturating_accrue(T::DbWeight::get().reads(3));
					if state.suspended || state.published.closed {
						return false;
					}
					if n.saturating_sub(InChannelsMeta::<T>::get(channel).published_at) < age {
						return false;
					}
					let stream = Self::inbound_stream(channel);
					InboundFrontier::<T>::get((channel.peer, stream)).leaf_count >
						state.published.up_to.0
				})
				.collect();
			for (channel, mut state) in due {
				if Self::publish_register(&channel, &mut state).is_ok() {
					InChannels::<T>::insert(channel, state);
				}
				weight.saturating_accrue(T::DbWeight::get().reads_writes(2, 3));
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

		/// Flags the HRMP channel pair to `peer` as closing — the
		/// parachain-side flip of the per-pair HRMP→spec-msg cutover
		/// sequence (drain-before-close):
		///
		/// 1. Open the spec-msg channels in both directions (`open_channel` on each sender,
		///    `accept_open_channel` on each receiver).
		/// 2. Wait for phase `Open` in both directions — a full round-trip over the real transport,
		///    observable on-chain: the peer accepted, published its initial register, and the
		///    sender read it (credit granted).
		/// 3. Submit this call in the same governance batch as the relay's `hrmp.close_channel`:
		///    from this block on the XCM router treats the still-open HRMP channel as `Closed`, so
		///    new traffic diverts to spec-msg while the already-queued HRMP messages drain for the
		///    remainder of the session — the pipe is empty when the closure enacts. (A closure with
		///    a non-empty pipe LOSES messages: the sender's `take_outbound_messages` swallows every
		///    page buffered for a `Closed` destination, and the relay's `close_hrmp_channel` drops
		///    all undelivered contents.)
		/// 4. Verify the drain before the session boundary — scripted, not eyeballed: the sender's
		///    `OutboundXcmpMessages` / `OutboundXcmpStatus` entries for `peer` and the relay's
		///    `HrmpChannelContents` of the pair must be empty.
		///
		/// Requires the designated outbound XCM channel to `peer` to be
		/// open — the sender-side, on-chain half of step 2's gate (the full
		/// both-directions condition is operational; one side cannot prove
		/// the reverse direction). Idempotent. Cost of the drain window:
		/// the receiver services residual HRMP (`Sibling` book) and new
		/// spec-msg (`SpecMsg` book) round-robin — bounded cross-transport
		/// reordering for at most one session.
		#[pallet::call_index(1)]
		#[pallet::weight((T::DbWeight::get().reads_writes(1, 1), DispatchClass::Operational))]
		pub fn set_hrmp_closing(origin: OriginFor<T>, peer: ParaId) -> DispatchResult {
			T::ChannelManagementOrigin::ensure_origin(origin)?;
			ensure!(
				Self::is_outbound_channel_open(&xcm_router::xcm_channel(peer)),
				Error::<T>::ChannelNotOpen
			);
			HrmpClosing::<T>::insert(peer, ());
			Self::deposit_event(Event::HrmpClosingSet { peer });
			Ok(())
		}

		/// Clears the [`HrmpClosing`] flag for `peer` — the rollback path:
		/// re-open the HRMP channel (handshake or relay-governance
		/// `force_open_hrmp_channel`; enacts at a session boundary, so
		/// rollback is distinctly slower than the flip) and clear the flag,
		/// and the router prefers HRMP again the moment the channel reports
		/// open. Messages already appended to the spec-msg channel keep
		/// being consumed by the peer — retrievable until its register
		/// confirms them (the watermark pruning rule), so a rollback
		/// re-interleaves the transports once but never loses. Idempotent.
		#[pallet::call_index(2)]
		#[pallet::weight((T::DbWeight::get().reads_writes(0, 1), DispatchClass::Operational))]
		pub fn clear_hrmp_closing(origin: OriginFor<T>, peer: ParaId) -> DispatchResult {
			T::ChannelManagementOrigin::ensure_origin(origin)?;
			HrmpClosing::<T>::remove(peer);
			Self::deposit_event(Event::HrmpClosingCleared { peer });
			Ok(())
		}

		/// Opens (or reopens) the outbound channel `(recipient, domain,
		/// num)`: creates the sender-side state in phase `Opening` and
		/// emits the `OpenChannel` signal leaf — the one message sendable
		/// without credit (necessarily: no register need exist), yet
		/// window-counted like everything on the stream. Total exposure
		/// toward a dead or unwilling peer: this one tiny leaf in the own
		/// archive and tree.
		///
		/// Reopening a `Closed` channel emits the signal at the current
		/// stream position — frontiers are eternal, the unconfirmed tail
		/// stays deliverable. After a sender half-close the peer's live
		/// register still stands: no re-acceptance needed, the channel is
		/// `Open` again the moment this executes. After a receiver close
		/// it stays `Closed` until the peer re-accepts
		/// (`accept_open_channel`) and the fresh register is read.
		#[pallet::call_index(3)]
		#[pallet::weight(T::DbWeight::get().reads_writes(5, 3))]
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
			if let Some(state) = &previous {
				ensure!(state.phase() == ChannelPhase::Closed, Error::<T>::AlreadyOpen);
			} else {
				// First open ever: anchor the in-flight window at the
				// stream's next position (the `OpenChannel` leaf appended
				// below is the first window-counted message).
				let stream = Self::outbound_stream(&channel);
				let next = OutboundFrontier::<T>::get(stream)
					.leaf_count
					.saturating_add(OutboundMessages::<T>::decode_len(stream).unwrap_or(0) as u64);
				OutChannelsMeta::<T>::mutate(channel, |meta| meta.base = MessagePosition(next));
			}

			Self::send_signal(&channel, SpecMsgSignal::OpenChannel { version: PROTOCOL_VERSION })?;
			OutChannels::<T>::insert(
				channel,
				OutChannelState {
					closed_by_us: false,
					announced_version: PROTOCOL_VERSION,
					// The last register read survives the reopen: after a
					// sender half-close it still carries live credit; after
					// a receiver close it keeps the phase `Closed` until a
					// fresh register (the re-acceptance) is read.
					register: previous.and_then(|state| state.register),
				},
			);
			Self::deposit_event(Event::ChannelOpened { channel });
			Ok(())
		}

		/// Accepts (or re-accepts after a receiver-side close) the inbound
		/// channel `(sender, domain, num)` — the analog of HRMP's
		/// `hrmp_accept_open_channel`, but entirely local and priced by
		/// this chain: fees, plus [`Config::AcceptConsideration`] held
		/// from a signed acceptor for the permanent state this creates.
		/// On execution the channel's data stream joins the consumed set
		/// (it now appears in `consumed_streams()` — the own collators
		/// start fetching it) and the initial register is published on the
		/// ack stream: the sender-visible acceptance. Either handshake
		/// order works — accept-first is pre-authorization. Rejection =
		/// never executing this, which costs the receiver nothing.
		#[pallet::call_index(4)]
		#[pallet::weight(T::DbWeight::get().reads_writes(6, 5))]
		pub fn accept_open_channel(
			origin: OriginFor<T>,
			sender: ParaId,
			domain: u8,
			num: u16,
		) -> DispatchResult {
			T::AcceptChannelOrigin::ensure_origin(origin.clone())?;
			ensure!(sender != T::SelfParaId::get(), Error::<T>::ChannelToSelf);

			let channel = ChannelId { peer: sender, domain, num };
			let mut state = match InChannels::<T>::get(channel) {
				// Re-acceptance revokes a receiver-side close; anything
				// else is already accepted.
				Some(mut state) => {
					ensure!(state.published.closed, Error::<T>::AlreadyAccepted);
					state.published.closed = false;
					state
				},
				None => InChannelState::default(),
			};

			// The optional deposit for the permanent state created — held
			// from a signed acceptor, never charged twice (the state a
			// held ticket priced never went away).
			if let Ok(who) = ensure_signed(origin) {
				if !AcceptanceTickets::<T>::contains_key(channel) {
					let footprint = Footprint::from_parts(
						1,
						InChannelState::max_encoded_len()
							.saturating_add(MmrFrontier::max_encoded_len()),
					);
					let ticket = T::AcceptConsideration::new(&who, footprint)?;
					AcceptanceTickets::<T>::insert(channel, (who, ticket));
				}
			}

			Self::publish_register(&channel, &mut state)?;
			InChannels::<T>::insert(channel, state);
			Self::deposit_event(Event::ChannelAccepted { channel });
			Ok(())
		}

		/// Half-closes the outbound channel: emits the `CloseChannel`
		/// signal — in-band, window-counted and credit-gated like every
		/// post-open message — and sets `closed_by_us`. Advisory resource
		/// release, safe at any time: the frontier survives, the
		/// unconfirmed tail stays deliverable, [`Pallet::open_channel`]
		/// reopens over it. With the window exhausted (or the peer already
		/// closed) the signal cannot be *sent* — abandonment (just stop
		/// sending) is the legal fallback on both sides.
		#[pallet::call_index(5)]
		#[pallet::weight(T::DbWeight::get().reads_writes(4, 3))]
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
			// The signal is an ordinary window-counted message: it needs an
			// open phase and credit — only `OpenChannel` is exempt.
			ensure!(state.phase() == ChannelPhase::Open, Error::<T>::ChannelNotOpen);
			Self::ensure_credit(&channel, &state)?;

			Self::send_signal(&channel, SpecMsgSignal::CloseChannel)?;
			state.closed_by_us = true;
			OutChannels::<T>::insert(channel, state);
			Self::deposit_event(Event::ChannelClosed { channel });
			Ok(())
		}

		/// Receiver-side close of the inbound channel: publishes a
		/// register with `closed: true` — the grant is void, `up_to` still
		/// reports what was consumed — and stops consuming the stream (it
		/// leaves `consumed_streams()`, the STF refuses its items). The
		/// consumption frontier is retained — the receiver's one
		/// obligation — so a later re-acceptance resumes exactly where
		/// consumption stopped.
		#[pallet::call_index(6)]
		#[pallet::weight(T::DbWeight::get().reads_writes(4, 4))]
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
			Self::deposit_event(Event::InboundChannelClosed { channel });
			Ok(())
		}

		/// Suspends the inbound channel — the upper-layer off switch, a
		/// pause rather than a close. The three effects all derive from
		/// the one flag: the STF refuses the channel's messages (inherent
		/// items are rejected), `consumed_streams()` omits the stream (the
		/// own collators stop fetching), and the published register grants
		/// zero.
		#[pallet::call_index(7)]
		#[pallet::weight(T::DbWeight::get().reads_writes(4, 4))]
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
			Self::deposit_event(Event::ChannelSuspended { channel });
			Ok(())
		}

		/// Resumes a suspended inbound channel: republishes a real grant;
		/// consumption (and the own collators' fetching) restarts from the
		/// retained frontier.
		#[pallet::call_index(8)]
		#[pallet::weight(T::DbWeight::get().reads_writes(4, 4))]
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
			Self::deposit_event(Event::ChannelResumed { channel });
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

		// Per stream: channel state and frontier reads/writes, publish
		// bookkeeping and a possible register publish; per payload: leaf
		// hashing and the handler, estimated as one read until benchmarks
		// land.
		T::DbWeight::get()
			.reads_writes(4 * streams + payloads, 4 * streams)
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

		/// The data stream of an INBOUND channel: the peer's key space,
		/// addressed to this chain — the uniform addressing rule.
		pub fn inbound_stream(channel: &ChannelId) -> StreamId {
			StreamId::Channel {
				recipient: T::SelfParaId::get(),
				domain: channel.domain,
				num: channel.num,
			}
		}

		/// The ack stream this chain publishes an INBOUND channel's
		/// register on: own key space, addressed to the channel's sender.
		/// (The registers this chain READS live in the peers' key spaces,
		/// addressed to this chain.)
		pub fn ack_stream(channel: &ChannelId) -> StreamId {
			StreamId::Ack { recipient: channel.peer, domain: channel.domain, num: channel.num }
		}

		/// Whether the outbound channel is in phase `Open`: the peer's
		/// register was read (acceptance) and neither side closed. This is
		/// the on-chain gate of the HRMP cutover
		/// ([`Pallet::set_hrmp_closing`]) — a full handshake round-trip
		/// over the real transport, observable on-chain.
		pub fn is_outbound_channel_open(channel: &ChannelId) -> bool {
			OutChannels::<T>::get(channel)
				.map_or(false, |state| state.phase() == ChannelPhase::Open)
		}

		/// Whether the HRMP channel pair to `peer` is flagged mid-cutover —
		/// what makes the XCM router treat the pair's HRMP channel as
		/// `Closed` while `get_channel_status` still reports it open.
		pub fn is_hrmp_closing(peer: ParaId) -> bool {
			HrmpClosing::<T>::contains_key(peer)
		}

		/// Whether the channel layer would currently accept a
		/// [`Pallet::send`] of `data_len` payload bytes: the encoded
		/// [`SpecMsgKind::Data`] leaf must fit [`Config::MaxMsgLen`], the
		/// channel must be in phase `Open` with credit left in the peer's
		/// advisory window, and the channel stream's per-block vec must
		/// have room. Side-effect free — this is the fail-fast check the
		/// XCM router runs at `validate`.
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

			let state = OutChannels::<T>::get(channel).ok_or(Error::<T>::ChannelNotOpen)?;
			frame_support::ensure!(state.phase() == ChannelPhase::Open, Error::<T>::ChannelNotOpen);
			Self::ensure_credit(channel, &state)?;

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
		/// message's position. Gated by [`Pallet::can_send`] (phase `Open`,
		/// credit, hard caps); no hidden queueing — on error, state is
		/// untouched.
		pub fn send(channel: ChannelId, data: Vec<u8>) -> Result<MessagePosition, Error<T>> {
			Self::can_send(&channel, data.len())?;
			let payload = SpecMsgKind::Data(data).encode();
			let size = payload.len() as u32;
			let position = Self::append_to_stream(Self::outbound_stream(&channel), payload)?;
			OutChannelsMeta::<T>::mutate(channel, |meta| meta.account_send(size));
			Ok(position)
		}

		/// Appends one lifecycle signal leaf to the outbound channel's
		/// data stream and accounts it against the in-flight window —
		/// signals are ordinary window-counted messages. Credit gating is
		/// the caller's: `OpenChannel` is exempt (the lifecycle
		/// bootstrap), everything after is gated.
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

		/// The credit gate: in-flight (count and bytes) must be strictly
		/// below the peer's granted window on BOTH limits. Enforcement is
		/// this chain's own STF — the receiver's grant is advice; honoring
		/// it protects the own archive and surfaces backpressure to the
		/// caller ([`SendError::Transport`] via the XCM router). A shrunk
		/// grant only gates new sends — in-flight messages are never
		/// invalidated.
		///
		/// [`SendError::Transport`]: xcm::latest::SendError::Transport
		fn ensure_credit(channel: &ChannelId, state: &OutChannelState) -> Result<(), Error<T>> {
			let grant = state.register.map(|register| register.grant).unwrap_or_default();
			let meta = OutChannelsMeta::<T>::get(channel);
			frame_support::ensure!(
				(meta.sizes.len() as u64) < u64::from(grant.max_messages),
				Error::<T>::NoCredit
			);
			frame_support::ensure!(meta.bytes < grant.max_bytes, Error::<T>::NoCredit);
			Ok(())
		}

		/// Publishes `channel`'s register on its ack stream from the
		/// current state and consumption frontier — the receiver's entire
		/// voice as one latest-wins leaf, sent through the ordinary sender
		/// machinery (one tree, one root): watermark, grant (zero while
		/// suspended or closed), close. Updates `state.published` in place
		/// (the caller stores the state) and resets the publish
		/// bookkeeping.
		fn publish_register(
			channel: &ChannelId,
			state: &mut InChannelState,
		) -> Result<(), Error<T>> {
			let stream = Self::inbound_stream(channel);
			let up_to =
				MessagePosition(InboundFrontier::<T>::get((channel.peer, stream)).leaf_count);
			let grant = if state.suspended || state.published.closed {
				// Zero: nothing may be sent until resume/re-acceptance.
				WindowGrant::default()
			} else {
				T::DefaultWindowGrant::get()
			};
			let register = Register {
				version: PROTOCOL_VERSION,
				up_to,
				grant,
				closed: state.published.closed,
			};

			Self::append_to_stream(Self::ack_stream(channel), register.encode())?;
			state.published = register;
			InChannelsMeta::<T>::insert(
				channel,
				InChannelMeta {
					published_at: frame_system::Pallet::<T>::block_number(),
					bytes_since: 0,
				},
			);
			Self::deposit_event(Event::RegisterPublished { channel: *channel, register });
			Ok(())
		}

		/// The register publish policy, checked after consuming on an
		/// inbound channel: publish when consumption progressed ~¼ of the
		/// granted window since the last publish (messages or bytes,
		/// whichever trips first), when the age threshold expired with
		/// unreported progress, or when a consumed signal forced it. A
		/// failed publish (per-block cap on the ack stream) keeps the
		/// bookkeeping and is retried by the `on_initialize` age sweep.
		fn note_consumption(
			channel: &ChannelId,
			state: &mut InChannelState,
			up_to: MessagePosition,
			bytes: u64,
			force: bool,
		) {
			let mut meta = InChannelsMeta::<T>::get(channel);
			meta.bytes_since = meta.bytes_since.saturating_add(bytes);

			let grant = state.published.grant;
			let messages_since = up_to.0.saturating_sub(state.published.up_to.0);
			let age = frame_system::Pallet::<T>::block_number().saturating_sub(meta.published_at);
			let due = force ||
				(grant.max_messages > 0 &&
					messages_since.saturating_mul(4) >= u64::from(grant.max_messages)) ||
				(grant.max_bytes > 0 && meta.bytes_since.saturating_mul(4) >= grant.max_bytes) ||
				(age >= T::RegisterPublishAge::get() && messages_since > 0);

			if !due || Self::publish_register(channel, state).is_err() {
				InChannelsMeta::<T>::insert(channel, meta);
			}
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
		/// Derived from the channel state: the data streams of accepted
		/// [`InChannels`] entries that are neither suspended nor
		/// receiver-closed — suspension and close express themselves as
		/// omission here, which is how the own collators learn to stop
		/// fetching.
		pub fn consumed_streams() -> BTreeMap<ParaId, Vec<ConsumedStream>> {
			let mut grouped = BTreeMap::<ParaId, Vec<(StreamId, ConsumedStream)>>::new();
			for (channel, state) in InChannels::<T>::iter() {
				if state.suspended || state.published.closed {
					continue;
				}
				let stream = Self::inbound_stream(&channel);
				let cursor =
					MessagePosition(InboundFrontier::<T>::get((channel.peer, stream)).leaf_count);
				if let Some(consumed) = ConsumedStream::project(&stream, cursor) {
					grouped.entry(channel.peer).or_default().push((stream, consumed));
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
		/// runtime API serves: phase and credit/watermark standing for
		/// authoring decisions (the register's `up_to` is the node-side
		/// archive's pruning watermark) and, via the keys, which ack
		/// registers the own collators read.
		pub fn out_channels() -> BTreeMap<ChannelId, OutChannelState> {
			OutChannels::<T>::iter().collect()
		}

		/// Channel views, inbound direction — what the `in_channels()`
		/// runtime API serves: which channels are due a register publish
		/// check, suspension standing, diagnostics.
		pub fn in_channels() -> BTreeMap<ChannelId, InChannelState> {
			InChannels::<T>::iter().collect()
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
			// Ordered consumption is defined on channel data streams
			// addressed to this chain (the uniform addressing rule) whose
			// inbound channel is accepted, live and not suspended.
			let StreamId::Channel { recipient, domain, num } = stream else {
				return Err(RejectReason::UnknownStream);
			};
			ensure!(recipient == T::SelfParaId::get(), RejectReason::UnknownStream);
			let channel = ChannelId { peer: source, domain, num };
			let mut state = InChannels::<T>::get(channel).ok_or(RejectReason::UnknownStream)?;
			ensure!(!state.suspended && !state.published.closed, RejectReason::UnknownStream);
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
			let mut bytes = 0u64;
			let mut publish = false;
			for payload in payloads {
				let position = MessagePosition(frontier.leaf_count);
				frontier.append_leaf(hash_leaf::<SpecHasher>(LEAF_VERSION, payload));
				bytes = bytes.saturating_add(payload.len() as u64);

				match SpecMsgKind::decode_all(&mut &payload[..]) {
					Ok(SpecMsgKind::Data(data)) => {
						T::DataHandler::on_data(source, stream, position, data)
					},
					Ok(SpecMsgKind::Signal(signal)) => {
						publish |= Self::apply_signal(&mut state, signal)
					},
					// Consume-and-drop: skipping is illegal, stalling
					// would brick the channel.
					Err(_) => {
						Self::deposit_event(Event::PayloadDropped { source, stream, position })
					},
				}
			}
			let up_to = MessagePosition(frontier.leaf_count);
			InboundFrontier::<T>::insert((source, stream), &frontier);
			ConsumptionOutbox::<T>::append((source, stream, Interval { start, end: frontier }));
			Self::note_consumption(&channel, &mut state, up_to, bytes, publish);
			InChannels::<T>::insert(channel, state);
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
			// Head reads are defined on ack streams addressed to this
			// chain whose outbound channel exists — in ANY phase: even a
			// closed channel's watermark still reports consumption (the
			// pruning signal). Broadcast event streams share the
			// discipline but are post-MVP.
			let StreamId::Ack { recipient, domain, num } = stream else {
				return Err(RejectReason::UnknownStream);
			};
			ensure!(recipient == T::SelfParaId::get(), RejectReason::UnknownStream);
			let channel = ChannelId { peer: source, domain, num };
			ensure!(OutChannels::<T>::contains_key(channel), RejectReason::UnknownStream);
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

			Self::apply_register_read(&channel, position, register);
			ConsumptionOutbox::<T>::append((
				source,
				stream,
				Interval { start: frontier.root(), end: frontier },
			));
			Ok(())
		}

		/// Applies an in-band lifecycle signal consumed from an inbound
		/// channel's data stream. Returns whether the register should be
		/// published right away.
		fn apply_signal(state: &mut InChannelState, signal: SpecMsgSignal) -> bool {
			match signal {
				// (Re)open announcement. The version is absolute, not
				// clamped: announcing lower than before is exactly how
				// genuine downgrades happen (close + reopen).
				SpecMsgSignal::OpenChannel { version } => {
					state.peer_version = version;
					false
				},
				// Mid-channel raises are monotonic; a lower value is
				// invalid and ignored.
				SpecMsgSignal::Upgrade { version } => {
					if version > state.peer_version {
						state.peer_version = version;
					}
					false
				},
				// The peer sends nothing further until a reopen: report
				// the final watermark right away so its archive can prune
				// to the end and a reopen starts fully credited.
				SpecMsgSignal::CloseChannel => true,
			}
		}

		/// Applies a verified register head read to the outbound channel's
		/// state: unlocks/refreshes the credit window, releases the
		/// in-flight prefix below the watermark and hands the node-side
		/// archive its pruning watermark (via the `out_channels()` view).
		///
		/// Monotonic-safe: a register regressing `up_to` or `version` is a
		/// protocol violation and ignored ([`Event::RegisterRegressed`];
		/// the previous read stands — grounds for close or abandonment),
		/// and competing head reads are ordered by leaf position, so a
		/// stale head (e.g. replaying a since-shrunk grant) is ignored
		/// silently.
		fn apply_register_read(channel: &ChannelId, position: MessagePosition, register: Register) {
			let Some(mut state) = OutChannels::<T>::get(channel) else {
				// The consume gate checked existence; qed-adjacent, but a
				// missing entry only means nothing to apply to.
				return;
			};
			let mut meta = OutChannelsMeta::<T>::get(channel);
			if meta.read_at.map_or(false, |last| position <= last) {
				return;
			}
			if let Some(previous) = state.register {
				if register.up_to < previous.up_to || register.version < previous.version {
					Self::deposit_event(Event::RegisterRegressed { channel: *channel });
					return;
				}
			}

			meta.read_at = Some(position);
			meta.confirm(register.up_to);
			state.register = Some(register);
			OutChannels::<T>::insert(channel, state);
			OutChannelsMeta::<T>::insert(channel, meta);
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
