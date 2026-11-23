// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Statement handling to plug on top of the network service.
//!
//! This crate implements gossip-based propagation of statements between nodes, layered on the
//! substrate notifications protocol. The protocol version is negotiated per peer: `statement/2`
//! (preferred) with `statement/1` as a fallback. A node on the DHT path prefers `statement/3`
//! and falls back to both.
//!
//! ## Propagation
//!
//! - During major chain synchronization, statement gossip is paused so peers prioritize downloading
//!   blocks; it resumes automatically once the node is fully synced. Without the DHT path a peer is
//!   reconnected to recover statements missed while syncing; with it, the node re-advertises its
//!   affinity filter after `MAJOR_SYNC_SETTLE_PERIOD` and v3 peers replay them. A v1 peer cannot
//!   replay on request, so it is reconnected on either path.
//! - A propagation loop runs every second (`config::PROPAGATE_TIMEOUT`): it takes all statements
//!   added since the previous round and queues their hashes to a per-peer outbox. Each peer has at
//!   most one propagation chunk in flight at a time. When its send slot is free, statements are
//!   fetched from the store, filtered and encoded up to the maximum notification size
//!   (`config::MAX_STATEMENT_NOTIFICATION_SIZE`, ~1 MiB).
//! - Incoming statements are pushed onto a bounded validation queue
//!   (`config::MAX_PENDING_STATEMENTS`); if the queue is full, incoming statements are dropped.
//! - Peer reputation is adjusted based on statement quality (good, duplicate, invalid, flooding).
//!
//! ## Tracking received statements
//!
//! There is no per-peer record of delivered statements. A statement is propagated once, on the
//! tick after its import: the propagation pass drains the store's recent set, so no later pass
//! can pick it up again. A statement with no routing target ready to receive it is the exception:
//! it stays held until a routing target syncs it, or takes a detour through the farthest full
//! peer when it has no routing target at all. The only duplicates worth preventing are statements
//! sent back to the peers they came from.
//!
//! While a statement waits for validation, the peers it came from are recorded in
//! `pending_statements_peers`. On import they move to `recently_received_statements`, and a
//! peer that resends the statement before its tick is added there too. Propagation and
//! initial-sync chunks both skip the recorded peers, and the propagation pass clears
//! `recently_received_statements` when done.
//!
//! ## Initial sync
//!
//! A peer joining (or changing its topic affinity) receives the store's existing statements
//! through a cursor over the store's admission journal: scheduling captures the journal's
//! watermark, and bursts walk the admissions below it chunk by chunk, advancing the cursor as
//! each send is confirmed — a failed chunk is resent from the same position. The watermark also
//! splits delivery ownership between the two paths: admissions below the peer's highest
//! watermark are the sync cursor's job, and propagation skips them, so within one sync a
//! statement reaches the peer through at most one path. Duplicates stay possible at the edges —
//! an affinity-change re-sync restarts the cursor from zero and redelivers earlier admissions,
//! and a chunk whose send timed out may have arrived regardless, so its resend repeats it.
//!
//! ## Announced sync
//!
//! A full peer on `statement/3` often holds most of what a sync would send it, so its sync
//! announces short hashes ahead of the bodies. The sender walks the store's index by topic,
//! which needs no body, and announces the hashes under every topic due to the peer. The peer
//! answers with one bit for each hash, set for a statement it lacks: one neither stored, nor
//! banned, nor waiting for validation. The sender sends the bodies asked for, then announces the
//! next hashes. A sync whose announce stays unanswered for [`ANNOUNCE_ANSWER_TIMEOUT`] ends.
//!
//! The walk follows no admission watermark, so it may announce a statement that propagation
//! delivers too: the peer then holds it and asks for nothing. Announced hashes have a rate limit
//! of their own on both sides.
//!
//! ## Send scheduling
//!
//! Every peer has one send slot, shared by propagation and initial sync, so at most one chunk per
//! connection is in flight. Each chunk carries a fresh id, so the result of a send left over from
//! a previous connection cannot free the current one's slot. A completed chunk frees the slot at
//! once and the next propagation chunk follows, a failed send included: a failed propagation
//! chunk is not retried, but the rest of the backlog keeps draining, while a failed initial-sync
//! chunk is resent from the sync's cursor.
//!
//! Propagation queues hashes in a per-peer outbox and fetches, filters and encodes them only when
//! the slot is free, so a slow peer holds one encoded chunk rather than its whole backlog. An
//! outbox past `config::MAX_PROPAGATION_OUTBOX_LEN` drops its oldest hashes, since the freshest
//! statements are the ones still worth delivering. Dropped hashes are counted in
//! `undelivered_statements`.
//!
//! In-flight bytes of both kinds are held against the shared
//! `config::MAX_SEND_IN_FLIGHT_BYTES` budget. A peer that finds the budget full, or whose store
//! fetch fails, is parked once and refilled in parking order as completed sends free bytes and on
//! propagation ticks. While initial syncs are pending, propagation parks
//! `config::INITIAL_SYNC_RESERVED_BYTES` early: refills reclaim freed bytes synchronously,
//! while the timer-driven sync bursts would otherwise always find the budget full.
//!
//! A peer disconnects us once we exceed its statement rate limit. Each peer carries a mirror of
//! that quota on the send side. Propagation charges it per chunk and sends regardless, while an
//! initial sync charges it per statement as the chunk is fetched and cuts the chunk where the
//! quota runs out, so a peer draws its backlog at the rate it accepts, however often it asks.
//!
//! ## Topic affinity and light nodes
//!
//! The `statement/2` protocol lets a peer advertise which topics it cares about as a bloom filter
//! ("topic affinity"). Once a peer has an active affinity filter, only matching statements are
//! forwarded to it; when its affinity changes, newly relevant statements are re-sent. Affinity
//! advertisements are rate-limited per peer, see `config::AFFINITY_UPDATES_PER_SECOND`.
//!
//! Light-client peers on `statement/2` must advertise an affinity before receiving any statements:
//! a light V2 peer pulls only the topics it cares about instead of the full feed, and is synced
//! those statements in an initial burst on connect. Full nodes receive all statements unless they
//! opt into an affinity.
//!
//! With the DHT path enabled, the node speaks `statement/3`, which carries the `statement/2`
//! messages between two nodes on that path. Every `statement/3` peer sends its affinity filter as
//! the first message on connect, and the initial sync waits for the peer's filter, replaying the
//! statements it matches and those the peer is a DHT routing target for. A full peer that falls
//! back to `statement/2` or `statement/1` floods instead of routing, so it gets every statement
//! from the moment it connects. Propagation targets are chosen by the orchestrator, so the outbox
//! drain applies no affinity filter of its own.
//!
//! ## Usage
//!
//! - Use [`StatementHandlerPrototype::new`] to create a prototype.
//! - Pass the `NonDefaultSetConfig` returned from [`StatementHandlerPrototype::new`] to the network
//!   configuration as an extra peers set.
//! - Use [`StatementHandlerPrototype::build`] then [`StatementHandler::run`] to obtain a `Future`
//!   that processes statements.

mod affinity;
mod dht;
mod outbox;

use crate::config::*;

use affinity::AffinityFilter;
use codec::{Compact, Decode, Encode, MaxEncodedLen};
use dht::{DhtMetrics, DhtOrchestrator, RetentionHandle};
use futures::{
	channel::oneshot,
	future::{pending, FusedFuture},
	prelude::*,
	stream::FuturesUnordered,
};
use governor::{
	clock::{Clock, DefaultClock},
	middleware::NoOpMiddleware,
	state::{InMemoryState, NotKeyed},
	Quota, RateLimiter,
};
use outbox::{DeadEnd, PropagationOutboxes};
use prometheus_endpoint::{
	exponential_buckets, register, Counter, CounterVec, Gauge, GaugeVec, Histogram, HistogramOpts,
	HistogramVec, Opts, PrometheusError, Registry, U64,
};
use rand::seq::IteratorRandom;
use sc_network::{
	config::{NetworkBackendType, NetworkConfiguration, NonReservedPeerMode, SetConfig},
	error, multiaddr,
	peer_store::PeerStoreProvider,
	service::{
		traits::{NotificationEvent, NotificationService, ValidationResult},
		NotificationMetrics,
	},
	types::ProtocolName,
	utils::interval,
	Event, NetworkBackend, NetworkEventStream, NetworkPeers, NetworkStateInfo, ObservedRole,
};
use sc_network_sync::{SyncEvent, SyncEventStream};
use sc_network_types::PeerId;
use sp_runtime::{
	traits::{Block as BlockT, ConstU32},
	BoundedVec,
};
use sp_statement_store::{
	short_hash, AdmittedBatch, FilterDecision, Hash, ShortHash, Statement, StatementSource,
	StatementStore, SubmitResult, Topic, TopicIndexPosition,
};
use std::{
	collections::{hash_map::Entry, HashMap, HashSet, VecDeque},
	fmt, iter,
	num::NonZeroU32,
	pin::Pin,
	sync::Arc,
	time::{Duration, Instant},
};
use tokio::time::timeout;
pub mod config;
pub use config::{AffinityTopicsFile, DhtConfig};
#[cfg_attr(not(any(test, feature = "test-helpers")), doc(hidden))]
pub use dht::peers_topology::{DhtAffinity, PeersTopology, PeersTopologyConfig};
pub use dht::RetentionReasonMask;
#[cfg(test)]
mod test_helpers;

/// A set of statements.
pub type Statements = Vec<Statement>;

type StatementBatch = BoundedVec<Statement, ConstU32<{ MAX_STATEMENTS_PER_NOTIFICATION as u32 }>>;

type AnnouncedHashes = BoundedVec<ShortHash, ConstU32<{ MAX_ANNOUNCED_HASHES as u32 }>>;

/// One bit per announced hash, the lowest bit of a byte first.
type WantedBitmap = BoundedVec<u8, ConstU32<{ MAX_ANNOUNCED_HASHES.div_ceil(8) as u32 }>>;

/// The protocol version that was negotiated with a peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PeerProtocolVersion {
	/// V1: messages are encoded as `Vec<Statement>` (the legacy format).
	V1,
	/// V2: messages are encoded as `StatementMessage` enum (supports topic affinity).
	V2,
	/// V3: the V2 messages, between two nodes on the DHT path.
	V3,
}

impl PeerProtocolVersion {
	/// The version negotiated on a substream: `negotiated_fallback` names the fallback protocol
	/// the peer settled for, and without one the peer speaks the main protocol, `statement/3`
	/// when `dht` is on.
	fn negotiated(negotiated_fallback: Option<&str>, dht: bool) -> Self {
		match negotiated_fallback {
			Some(name) if name.ends_with(STATEMENT_PROTOCOL_V1) => PeerProtocolVersion::V1,
			Some(_) => PeerProtocolVersion::V2,
			None if dht => PeerProtocolVersion::V3,
			None => PeerProtocolVersion::V2,
		}
	}

	/// Returns the encoding envelope overhead for this protocol version.
	fn envelope_overhead(&self) -> usize {
		match self {
			PeerProtocolVersion::V1 => V1_ENVELOPE_OVERHEAD,
			PeerProtocolVersion::V2 | PeerProtocolVersion::V3 => V2_ENVELOPE_OVERHEAD,
		}
	}
}

#[derive(Debug, Encode, Decode)]
enum StatementMessage {
	#[codec(index = 0)]
	Statements(StatementBatch),
	/// Bloom filter bytes representing the topics this peer is interested in.
	#[codec(index = 1)]
	ExplicitTopicAffinity(AffinityFilter),
	/// Short hashes of statements the sender holds for this peer, sent ahead of their bodies.
	#[codec(index = 2)]
	Announce { id: u64, hashes: AnnouncedHashes },
	/// The answer to the announce `id`: a set bit asks for the body of the hash at that position.
	#[codec(index = 3)]
	Want { id: u64, wanted: WantedBitmap },
}

/// Codec variant index for `StatementMessage::Statements`, kept in sync with `#[codec(index)]`.
const STATEMENTS_VARIANT_INDEX: u8 = 0;

/// Codec variant index for `StatementMessage::ExplicitTopicAffinity`, kept in sync with
/// `#[codec(index)]`.
const AFFINITY_VARIANT_INDEX: u8 = 1;

impl StatementMessage {
	/// Encode a slice of statement references as a `StatementMessage::Statements`
	/// without cloning the statements.
	fn encode_statement_refs(statements: &[&Statement]) -> Vec<u8> {
		let mut out = Vec::new();
		STATEMENTS_VARIANT_INDEX.encode_to(&mut out);
		statements.encode_to(&mut out);
		out
	}
}

/// The bitmap answering an announce of `hashes`: a set bit marks a hash that `lacks` holds for.
fn wanted_bitmap(hashes: &[ShortHash], lacks: impl Fn(&ShortHash) -> bool) -> Vec<u8> {
	let mut bitmap = vec![0u8; hashes.len().div_ceil(8)];
	for (position, hash) in hashes.iter().enumerate() {
		if lacks(hash) {
			bitmap[position / 8] |= 1 << (position % 8);
		}
	}
	bitmap
}

/// Whether the bitmap of a `Want` asks for the announced hash at `position`.
fn is_wanted(bitmap: &[u8], position: usize) -> bool {
	bitmap.get(position / 8).is_some_and(|byte| byte & (1 << (position % 8)) != 0)
}

/// Future resolving to statement import result.
pub type StatementImportFuture = oneshot::Receiver<SubmitResult>;

mod rep {
	use sc_network::ReputationChange as Rep;
	/// Reputation change when a peer sends us any statement.
	///
	/// This forces node to verify it, thus the negative value here. Once statement is verified,
	/// reputation change should be refunded with `ANY_STATEMENT_REFUND`
	pub const ANY_STATEMENT: Rep = Rep::new(-(1 << 4), "Any statement");
	/// Reputation change when a peer sends us any statement that is not invalid.
	pub const ANY_STATEMENT_REFUND: Rep = Rep::new(1 << 4, "Any statement (refund)");
	/// Reputation change when a peer sends us an statement that we didn't know about.
	pub const GOOD_STATEMENT: Rep = Rep::new(1 << 8, "Good statement");
	/// Reputation change when a peer sends us an invalid statement.
	pub const INVALID_STATEMENT: Rep = Rep::new(-(1 << 12), "Invalid statement");
	/// Reputation change when a peer sends us a duplicate statement.
	pub const DUPLICATE_STATEMENT: Rep = Rep::new(-(1 << 7), "Duplicate statement");
	/// Reputation change when a peer floods us with statements.
	pub const STATEMENT_FLOODING: Rep = Rep::new_fatal("Statement flooding");
	/// Reputation change when a peer sends us a message we can't decode.
	pub const BAD_MESSAGE: Rep = Rep::new(-(1 << 12), "Bad statement message");
	/// Reputation change when a peer sends topic affinity updates faster than the limit.
	pub const AFFINITY_FLOODING: Rep = Rep::new(-(1 << 12), "Topic affinity flooding");
	/// Reputation change when a peer announces statement hashes faster than the limit.
	pub const ANNOUNCE_FLOODING: Rep = Rep::new(-(1 << 12), "Statement announce flooding");
}

const LOG_TARGET: &str = "statement-gossip";
/// V3 statement protocol suffix: the V2 messages between two nodes on the DHT path, so the
/// protocol a peer negotiates tells whether it routes or floods.
const STATEMENT_PROTOCOL_V3: &str = "statement/3";
/// V2 statement protocol suffix: topic affinity filters, shipped since stable2606.
const STATEMENT_PROTOCOL_V2: &str = "statement/2";
/// V1 statement protocol suffix: flood gossip without affinity filters. No breaking changes will
/// be made to it.
const STATEMENT_PROTOCOL_V1: &str = "statement/1";
/// Maximum time we wait for sending a notification to a peer.
const SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// Interval for sending statement batches during initial sync to new peers.
const INITIAL_SYNC_BURST_INTERVAL: std::time::Duration = std::time::Duration::from_millis(10);

/// Maximum admission-journal entries one initial-sync fetch visits, so that a peer whose
/// affinity matches nothing cannot make one burst walk the whole journal in the event loop.
const INITIAL_SYNC_SCAN_LIMIT: usize = 4096;
/// Most hashes one announce carries, and most rows of the index by topic that one sync burst
/// visits to collect them.
const MAX_ANNOUNCED_HASHES: usize = 4096;
/// How long a sync waits for the answer to an announce before it ends.
const ANNOUNCE_ANSWER_TIMEOUT: Duration = Duration::from_secs(30);
/// Interval for processing pending topic affinity changes from peers.
const PENDING_AFFINITIES_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);
/// Interval between sweeps that evict statement-store peers unseen for the staleness TTL, so the
/// topology converges on live peers even while discovery is quiet.
const PEER_EVICTION_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
/// Delay before re-adding a peer to the reserved set after a forced disconnect for sync recovery.
const SYNC_RECOVERY_READD_DELAY: std::time::Duration = std::time::Duration::from_secs(60);

static DHT_ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Feature-flag to switch between the legacy flood path and the DHT-targeted path.
///
/// Off by default. The first read or call to [`set_dht_enabled`] fixes the value for the
/// process lifetime.
pub fn dht_enabled() -> bool {
	*DHT_ENABLED.get_or_init(|| false)
}

/// Set the DHT gate. The first read or set fixes it, so a node binary calls this once before
/// anything reads the gate.
pub fn set_dht_enabled(enabled: bool) {
	let fixed_now = DHT_ENABLED.set(enabled).is_ok();
	debug_assert!(fixed_now, "the DHT gate is already fixed at {}", dht_enabled());
}

struct Metrics {
	propagated_statements: CounterVec<U64>,
	known_statements_received: Counter<U64>,
	skipped_oversized_statements: Counter<U64>,
	propagated_statements_chunks: HistogramVec,
	pending_statements: Gauge<U64>,
	ignored_statements: Counter<U64>,
	peers_connected: GaugeVec<U64>,
	peers_without_dht: Gauge<U64>,
	statements_received: CounterVec<U64>,
	bytes_sent_total: CounterVec<U64>,
	bytes_received_total: CounterVec<U64>,
	sent_latency_seconds: Histogram,
	initial_sync_statements_sent: CounterVec<U64>,
	initial_sync_bursts_total: CounterVec<U64>,
	initial_sync_in_flight_bytes: Gauge<U64>,
	propagation_in_flight_bytes: Gauge<U64>,
	initial_sync_peers_active: Gauge<U64>,
	initial_sync_duration_seconds: HistogramVec,
	statement_flooding_detected: Counter<U64>,
	affinity_flooding_detected: Counter<U64>,
	send_failures: CounterVec<U64>,
	undelivered_statements: CounterVec<U64>,
	announced_hashes_received: Counter<U64>,
	announced_hashes_wanted: Counter<U64>,
	announced_hashes_sent: Counter<U64>,
	announced_hashes_requested: Counter<U64>,
}

mod send_failure {
	/// The network layer rejected the send.
	pub const NETWORK: &str = "network";
	/// The send did not complete within `SEND_TIMEOUT`.
	pub const TIMEOUT: &str = "timeout";
	/// The chunk was never handed to the network because the peer had no message sink.
	pub const NO_SINK: &str = "no_sink";
	/// The peer's propagation outbox overflowed and the oldest queued hashes were dropped.
	pub const OUTBOX_FULL: &str = "outbox_full";
	/// A queued statement left the store, failed to decode or expired before its chunk went out.
	pub const MISSING_FROM_STORE: &str = "missing_from_store";
	/// The peer disconnected with statements still queued.
	pub const DISCONNECTED: &str = "disconnected";
}

/// Values for the `route` metric label. They say how we send statements to a peer.
mod peer_route {
	/// We use the DHT path and send only what the peer's filter asks for.
	pub const DHT: &str = "dht";
	/// We send the peer everything. This covers v1 peers, full v2 peers, and every peer while this
	/// node runs without the DHT path.
	pub const LEGACY: &str = "legacy";
}

mod sync_outcome {
	/// A burst found the backlog drained, so every statement reached the peer.
	pub const COMPLETED: &str = "completed";
	/// The sync ended before its backlog was drained.
	pub const ABANDONED: &str = "abandoned";
	/// The peer left an announce unanswered.
	pub const UNANSWERED: &str = "unanswered";
}

impl Metrics {
	fn register(r: &Registry) -> Result<Self, PrometheusError> {
		// A counter split by the `route` label, with every route present at zero so the series
		// exist before the first statement.
		let register_by_route =
			|opts: Opts, registry: &Registry| -> Result<CounterVec<U64>, PrometheusError> {
				let counter = register(CounterVec::new(opts, &["route"])?, registry)?;
				for route in [peer_route::DHT, peer_route::LEGACY] {
					counter.with_label_values(&[route]);
				}
				Ok(counter)
			};

		let peers_connected = register(
			GaugeVec::new(
				Opts::new(
					"substrate_sync_statement_peers_connected",
					"Number of peers connected using the statement protocol by kind",
				),
				&["kind"],
			)?,
			r,
		)?;
		peers_connected.with_label_values(&["full"]).set(0);
		peers_connected.with_label_values(&["light"]).set(0);

		Ok(Self {
			propagated_statements: register_by_route(
				Opts::new(
					"substrate_sync_propagated_statements",
					"Total statements propagated to peers, counted once per recipient (a statement sent to N peers increments by N)",
				),
				r,
			)?,
			known_statements_received: register(
				Counter::new(
					"substrate_sync_known_statement_received",
					"Number of statements received via gossiping that were already in the statement store",
				)?,
				r,
			)?,
			skipped_oversized_statements: register(
				Counter::new(
					"substrate_sync_skipped_oversized_statements",
					"Number of oversized statements that were skipped to be gossiped",
				)?,
				r,
			)?,
			propagated_statements_chunks: register(
				HistogramVec::new(
					HistogramOpts::new(
						"substrate_sync_propagated_statements_chunks",
						"Distribution of chunk sizes when sending statements, by send path. Initial \
						 sync fills every chunk to the size limit, propagation does not",
					)
					.buckets(exponential_buckets(1.0, 2.0, 14)?),
					&["kind"],
				)?,
				r,
			)?,
			peers_without_dht: register(
				Gauge::new(
					"substrate_sync_statement_peers_without_dht",
					"Number of connected peers served as nodes without the DHT path",
				)?,
				r,
			)?,
			pending_statements: register(
				Gauge::new(
					"substrate_sync_pending_statement_validations",
					"Number of pending statement validations, sampled once per propagation tick",
				)?,
				r,
			)?,
			ignored_statements: register(
				Counter::new(
					"substrate_sync_ignored_statements",
					"Number of statements ignored due to exceeding MAX_PENDING_STATEMENTS limit",
				)?,
				r,
			)?,
			peers_connected,
			statements_received: register_by_route(
				Opts::new(
					"substrate_sync_statements_received",
					"Total number of statements received from peers",
				),
				r,
			)?,
			bytes_sent_total: register_by_route(
				Opts::new(
					"substrate_sync_statement_bytes_sent_total",
					"Total bytes sent for statement protocol messages",
				),
				r,
			)?,
			bytes_received_total: register_by_route(
				Opts::new(
					"substrate_sync_statement_bytes_received_total",
					"Total bytes received for statement protocol messages (includes bytes from notifications that are later discarded — e.g. while major-syncing)",
				),
				r,
			)?,
			sent_latency_seconds: register(
				Histogram::with_opts(
					HistogramOpts::new(
						"substrate_sync_statement_sent_latency_seconds",
						"Time to send statement messages to peers",
					)
					// Buckets from 1μs to ~1s covering microsecond to millisecond range.
					.buckets(vec![0.000_001, 0.000_01, 0.000_1, 0.001, 0.01, 0.1, 1.0]),
				)?,
				r,
			)?,
			initial_sync_statements_sent: register_by_route(
				Opts::new(
					"substrate_sync_initial_sync_statements_sent",
					"Total statements sent during initial sync bursts to newly connected peers",
				),
				r,
			)?,
			initial_sync_bursts_total: register_by_route(
				Opts::new(
					"substrate_sync_initial_sync_bursts_total",
					"Total initial-sync burst rounds attempted (includes rounds that return early with no hashes left)",
				),
				r,
			)?,
			initial_sync_in_flight_bytes: register(
				Gauge::new(
					"substrate_sync_initial_sync_in_flight_bytes",
					"Encoded bytes of initial-sync chunks currently queued for sending",
				)?,
				r,
			)?,
			propagation_in_flight_bytes: register(
				Gauge::new(
					"substrate_sync_propagation_in_flight_bytes",
					"Encoded bytes of propagation chunks currently queued for sending",
				)?,
				r,
			)?,
			initial_sync_peers_active: register(
				Gauge::new(
					"substrate_sync_initial_sync_peers_active",
					"Number of peers currently being synced via initial sync",
				)?,
				r,
			)?,
			initial_sync_duration_seconds: register(
				HistogramVec::new(
					HistogramOpts::new(
						"substrate_sync_initial_sync_duration_seconds",
						"Per-peer duration of initial sync, by outcome: completed (backlog drained), abandoned (ended with statements still queued) or unanswered (the peer did not answer an announce)",
					)
					.buckets(vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0]),
					&["outcome"],
				)?,
				r,
			)?,
			statement_flooding_detected: register(
				Counter::new(
					"substrate_sync_statement_flooding_detected",
					"Number of peers disconnected for exceeding statement rate limits",
				)?,
				r,
			)?,
			affinity_flooding_detected: register(
				Counter::new(
					"substrate_sync_statement_affinity_flooding_detected",
					"Number of topic affinity updates dropped for exceeding the per-peer rate limit",
				)?,
				r,
			)?,
			send_failures: register(
				CounterVec::new(
					Opts::new(
						"substrate_sync_statement_send_failures_total",
						"Total statement sends that never reached the peer, by reason",
					),
					&["reason"],
				)?,
				r,
			)?,
			undelivered_statements: register(
				CounterVec::new(
					Opts::new(
						"substrate_sync_statement_undelivered_total",
						"Total statements whose delivery was abandoned, so the peer never received them, by reason",
					),
					&["reason"],
				)?,
				r,
			)?,
			announced_hashes_received: register(
				Counter::new(
					"substrate_sync_statement_announced_hashes_received_total",
					"Total statement hashes that peers announced to this node",
				)?,
				r,
			)?,
			announced_hashes_wanted: register(
				Counter::new(
					"substrate_sync_statement_announced_hashes_wanted_total",
					"Total announced statements this node lacked and asked for",
				)?,
				r,
			)?,
			announced_hashes_sent: register(
				Counter::new(
					"substrate_sync_statement_announced_hashes_sent_total",
					"Total statement hashes this node announced to peers",
				)?,
				r,
			)?,
			announced_hashes_requested: register(
				Counter::new(
					"substrate_sync_statement_announced_hashes_requested_total",
					"Total statements that peers asked for after this node announced them",
				)?,
				r,
			)?,
		})
	}
}

/// The main protocol name and its fallbacks, most preferred first. Only a node on the DHT path
/// offers `statement/3`.
fn protocol_names(
	genesis_hash_hex: &str,
	fork_id: Option<&str>,
	dht: bool,
) -> (String, Vec<String>) {
	let protocol = |version: &str| match fork_id {
		Some(fork_id) => format!("/{genesis_hash_hex}/{fork_id}/{version}"),
		None => format!("/{genesis_hash_hex}/{version}"),
	};
	if dht {
		(
			protocol(STATEMENT_PROTOCOL_V3),
			vec![protocol(STATEMENT_PROTOCOL_V2), protocol(STATEMENT_PROTOCOL_V1)],
		)
	} else {
		(protocol(STATEMENT_PROTOCOL_V2), vec![protocol(STATEMENT_PROTOCOL_V1)])
	}
}

/// Prototype for a [`StatementHandler`].
pub struct StatementHandlerPrototype {
	protocol_name: ProtocolName,
	notification_service: Box<dyn NotificationService>,
	/// Inbound slots for full peers, [`DhtConfig::in_peers`].
	in_peers_full: u32,
	/// Inbound slots for light peers, the node's `--in-peers-light`.
	in_peers_light: u32,
}

impl StatementHandlerPrototype {
	/// Create a new instance.
	pub fn new<
		Hash: AsRef<[u8]>,
		Block: BlockT,
		Net: NetworkBackend<Block, <Block as BlockT>::Hash>,
	>(
		genesis_hash: Hash,
		fork_id: Option<&str>,
		metrics: NotificationMetrics,
		peer_store_handle: Arc<dyn PeerStoreProvider>,
		network_config: &NetworkConfiguration,
		dht_config: Option<&DhtConfig>,
	) -> (Self, Net::NotificationProtocolConfig) {
		let genesis_hash = genesis_hash.as_ref();
		let hex = array_bytes::bytes2hex("", genesis_hash);
		let (protocol_name, fallback_names) = protocol_names(&hex, fork_id, dht_enabled());
		// The default set holds `--in-peers-light` only as part of its inbound total, so the
		// light allowance is recovered the way the sync engine recovers it.
		let default_set = &network_config.default_peers_set;
		let in_peers_light = (default_set.in_peers + default_set.out_peers)
			.saturating_sub(network_config.default_peers_set_num_full);
		let in_peers_full = dht_config.map_or(DEFAULT_IN_PEERS, |config| config.in_peers);
		// Only litep2p emits the discovery and identify events the topology learns peers from.
		if dht_enabled() && matches!(network_config.network_backend, NetworkBackendType::Libp2p) {
			log::error!(
				target: LOG_TARGET,
				"The statement DHT path learns peers only from litep2p. On the libp2p backend \
				 it dials no statement peer and sends statements only to peers that dial in, so \
				 libp2p nodes exchange none. Use `--network-backend litep2p`.",
			);
		}
		// The DHT path reserves only the peers that steering picks, every other peer holds an
		// inbound slot. The v1 path manages its peers exclusively through the reserved set:
		// it must keep non-reserved slots closed, or sync recovery's forced disconnect stops
		// disconnecting.
		let set_config = if dht_enabled() {
			SetConfig {
				in_peers: in_peers_full.saturating_add(in_peers_light),
				out_peers: DEFAULT_OUT_PEERS,
				reserved_nodes: Vec::new(),
				non_reserved_mode: NonReservedPeerMode::Accept,
			}
		} else {
			SetConfig {
				in_peers: 0,
				out_peers: 0,
				reserved_nodes: Vec::new(),
				non_reserved_mode: NonReservedPeerMode::Deny,
			}
		};
		let (config, notification_service) = Net::notification_config(
			protocol_name.clone().into(),
			fallback_names.into_iter().map(Into::into).collect(),
			MAX_STATEMENT_NOTIFICATION_SIZE,
			None,
			set_config,
			metrics,
			peer_store_handle,
		);

		(
			Self {
				protocol_name: protocol_name.into(),
				notification_service,
				in_peers_full,
				in_peers_light,
			},
			config,
		)
	}

	/// Turns the prototype into the actual handler.
	///
	/// Important: the statements handler is initially disabled and doesn't gossip statements.
	/// Gossiping is enabled when major syncing is done.
	pub fn build<
		N: NetworkPeers + NetworkEventStream + NetworkStateInfo,
		S: SyncEventStream + sp_consensus::SyncOracle,
	>(
		self,
		network: N,
		sync: S,
		statement_store: Arc<dyn StatementStore>,
		metrics_registry: Option<&Registry>,
		executor: impl Fn(Pin<Box<dyn Future<Output = ()> + Send>>) + Send,
		mut num_submission_workers: usize,
		statements_per_second: u32,
		dht_config: Option<DhtConfig>,
	) -> error::Result<StatementHandler<N, S>> {
		let sync_event_stream = sync.event_stream("statement-handler-sync");
		// Still bounded via the `MAX_PENDING_STATEMENTS` check in `on_statements`.
		let (queue_sender, queue_receiver) = async_channel::unbounded();

		if num_submission_workers == 0 {
			log::warn!(
				target: LOG_TARGET,
				"num_submission_workers is 0, defaulting to 1"
			);
			num_submission_workers = 1;
		}

		let statements_per_second = match NonZeroU32::new(statements_per_second) {
			Some(rate) => rate,
			None => {
				log::warn!(
					target: LOG_TARGET,
					"statements_per_second is 0, defaulting to {}",
					DEFAULT_STATEMENTS_PER_SECOND
				);
				NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND)
					.expect("DEFAULT_STATEMENTS_PER_SECOND is nonzero")
			},
		};

		let metrics =
			if let Some(r) = metrics_registry { Some(Metrics::register(r)?) } else { None };
		let dht_metrics = if let (true, Some(r)) = (dht_enabled(), metrics_registry) {
			Some(DhtMetrics::register(r)?)
		} else {
			None
		};

		for _ in 0..num_submission_workers {
			let store = statement_store.clone();
			let mut queue_receiver = queue_receiver.clone();
			executor(
				async move {
					loop {
						let task: Option<(Statement, oneshot::Sender<SubmitResult>)> =
							queue_receiver.next().await;
						match task {
							None => return,
							Some((statement, completion)) => {
								let result = store.submit(statement, StatementSource::Network);
								if completion.send(result).is_err() {
									log::debug!(
										target: LOG_TARGET,
										"Error sending validation completion"
									);
								}
							},
						}
					}
				}
				.boxed(),
			);
		}

		let network_event_stream = if dht_enabled() {
			network
				.event_stream("statement-handler-network")
				.filter(|event| {
					std::future::ready(matches!(
						event,
						Event::PeersDiscovered(_) | Event::PeerIdentified { .. }
					))
				})
				.boxed()
		} else {
			futures::stream::pending::<Event>().boxed()
		};
		let retention = dht_config
			.as_ref()
			.map(|cfg| RetentionHandle::new(network.local_peer_id(), cfg.replication_factor));
		let DhtConfig {
			mut affinity_topics,
			affinity_topics_file,
			bloom_false_pos_rate,
			bloom_seed,
			replication_factor,
			gossip_target,
			..
		} = dht_config.unwrap_or_default();
		affinity_topics.extend(affinity_topics_file.into_iter().flat_map(|file| file.0));
		let mut dht = DhtOrchestrator::new(
			&affinity_topics,
			bloom_seed,
			bloom_false_pos_rate,
			network.local_peer_id(),
			PeersTopologyConfig { replication_factor, gossip_target },
			self.protocol_name.clone(),
			dht_metrics,
		);
		if let Some(retention) = retention {
			dht.set_retention_handle(retention);
		}
		let propagation_outboxes =
			PropagationOutboxes::new(dht_enabled().then(|| statement_store.clone()));
		let handler = StatementHandler {
			protocol_name: self.protocol_name,
			notification_service: self.notification_service,
			propagate_timeout: (Box::pin(interval(PROPAGATE_TIMEOUT))
				as Pin<Box<dyn Stream<Item = ()> + Send>>)
				.fuse(),
			pending_statements: FuturesUnordered::new(),
			pending_statements_peers: HashMap::new(),
			short_hashes_in_validation: HashSet::new(),
			recently_received_statements: HashMap::new(),
			network,
			sync,
			sync_event_stream: sync_event_stream.fuse(),
			network_event_stream: network_event_stream.fuse(),
			peers: HashMap::new(),
			statement_store,
			queue_sender,
			statements_per_second,
			metrics,
			initial_sync_timeout: Box::pin(tokio::time::sleep(INITIAL_SYNC_BURST_INTERVAL).fuse()),
			pending_affinities_timeout: Box::pin(
				tokio::time::sleep(PENDING_AFFINITIES_INTERVAL).fuse(),
			),
			peer_eviction_timeout: if dht_enabled() {
				Box::pin(tokio::time::sleep(PEER_EVICTION_INTERVAL).fuse())
			} else {
				Box::pin(pending().fuse())
			},
			pending_initial_syncs: HashMap::new(),
			initial_sync_peer_queue: VecDeque::new(),
			next_initial_sync_id: 0,
			initial_sync_in_flight_bytes: 0,
			propagation_outboxes,
			in_flight_chunks: HashMap::new(),
			next_chunk_id: 0,
			propagation_in_flight_bytes: 0,
			parked_propagations: VecDeque::new(),
			pending_sends: FuturesUnordered::new(),
			deferred_peers: HashSet::new(),
			in_peers_full: self.in_peers_full,
			in_peers_light: self.in_peers_light,
			dropped_statements_during_sync: false,
			sync_recovery_peer: None,
			sync_recovery_readd_timeout: Box::pin(pending().fuse()),
			dht,
		};

		Ok(handler)
	}
}

/// Handler for statements. Call [`StatementHandler::run`] to start the processing.
pub struct StatementHandler<
	N: NetworkPeers + NetworkEventStream + NetworkStateInfo,
	S: SyncEventStream + sp_consensus::SyncOracle,
> {
	protocol_name: ProtocolName,
	/// Interval at which we call `propagate_statements`.
	propagate_timeout: stream::Fuse<Pin<Box<dyn Stream<Item = ()> + Send>>>,
	/// Pending statements verification tasks.
	pending_statements:
		FuturesUnordered<Pin<Box<dyn Future<Output = (Hash, Option<SubmitResult>)> + Send>>>,
	/// As multiple peers can send us the same statement, we group
	/// these peers using the statement hash while the statement is
	/// imported. This prevents that we import the same statement
	/// multiple times concurrently.
	pending_statements_peers: HashMap<Hash, HashSet<PeerId>>,
	/// Short hashes of the statements in `pending_statements_peers`, so that an announce is
	/// answered without a walk over the map.
	short_hashes_in_validation: HashSet<ShortHash>,
	/// Statements received from peers and imported since the last propagation
	/// pass, each with the peers that sent it. Propagation skips those peers,
	/// so a statement never returns to a peer it came from. Cleared after each
	/// pass.
	recently_received_statements: HashMap<Hash, HashSet<PeerId>>,
	/// Network service to use to send messages and manage peers.
	network: N,
	/// Syncing service.
	sync: S,
	/// Receiver for syncing-related events.
	sync_event_stream: stream::Fuse<Pin<Box<dyn Stream<Item = SyncEvent> + Send>>>,
	/// Receiver for network topology events.
	network_event_stream: stream::Fuse<Pin<Box<dyn Stream<Item = Event> + Send>>>,
	/// Notification service.
	notification_service: Box<dyn NotificationService>,
	// All connected peers
	peers: HashMap<PeerId, Peer>,
	statement_store: Arc<dyn StatementStore>,
	queue_sender: async_channel::Sender<(Statement, oneshot::Sender<SubmitResult>)>,
	/// Maximum statements per second per peer.
	statements_per_second: NonZeroU32,
	/// Prometheus metrics.
	metrics: Option<Metrics>,
	/// Timeout for sending next statement batch during initial sync.
	initial_sync_timeout: Pin<Box<dyn FusedFuture<Output = ()> + Send>>,
	/// Timeout for processing pending topic affinity changes.
	pending_affinities_timeout: Pin<Box<dyn FusedFuture<Output = ()> + Send>>,
	/// Fires periodically to evict statement-store peers unseen for the staleness TTL.
	peer_eviction_timeout: Pin<Box<dyn FusedFuture<Output = ()> + Send>>,
	/// Pending initial syncs per peer.
	pending_initial_syncs: HashMap<PeerId, PendingInitialSync>,
	/// Queue for round-robin processing of initial syncs.
	initial_sync_peer_queue: VecDeque<PeerId>,
	/// Next value to hand out as [`PendingInitialSync::sync_id`].
	next_initial_sync_id: u64,
	/// Encoded bytes of initial-sync chunks in `pending_sends`, held against the shared
	/// [`MAX_SEND_IN_FLIGHT_BYTES`] budget.
	initial_sync_in_flight_bytes: u64,
	/// Statement hashes queued for propagation to each peer, drained from the front as chunks
	/// are fetched, whether the fetch yields a send or not.
	propagation_outboxes: PropagationOutboxes,
	/// Id of the chunk in flight, per peer — the peer's single send slot, shared by
	/// propagation and initial sync.
	in_flight_chunks: HashMap<PeerId, u64>,
	/// Next value to hand out as [`PendingSendResult::chunk_id`].
	next_chunk_id: u64,
	/// Encoded bytes of propagation chunks in `pending_sends`, held against the shared
	/// [`MAX_SEND_IN_FLIGHT_BYTES`] budget.
	propagation_in_flight_bytes: u64,
	/// Peers whose propagation chunk was deferred because the shared byte budget was
	/// exhausted or the store fetch failed, refilled in parking order as bytes free up
	/// and on propagation ticks. A peer parks at most once.
	parked_propagations: VecDeque<PeerId>,
	/// Pending propagation sends, polled by the main event loop.
	pending_sends: PendingSends,
	/// Tracks peers that connected while major sync was active and adds them to the reserved set
	/// once sync ends
	deferred_peers: HashSet<PeerId>,
	/// Inbound slots for full peers.
	in_peers_full: u32,
	/// Inbound slots for light peers.
	in_peers_light: u32,
	/// Set to `true` when an incoming statement is dropped because `is_major_syncing()` is true
	/// and only a reconnect can recover it: any peer without the DHT path, a v1 peer with it
	dropped_statements_during_sync: bool,
	/// Peer scheduled for forced disconnect+reconnect to recover statements missed during sync
	sync_recovery_peer: Option<PeerId>,
	/// Fires when the `sync_recovery_peer` re-add delay has elapsed
	sync_recovery_readd_timeout: Pin<Box<dyn FusedFuture<Output = ()> + Send>>,
	/// Only invoked when [`dht_enabled`] returns `true`.
	dht: DhtOrchestrator,
}

/// A token bucket, measured against whatever clock it was built with.
trait TokenBucket: fmt::Debug + Send + Sync {
	/// Whether admitting `n` more cells would exceed the quota.
	fn would_exceed(&self, n: NonZeroU32) -> bool;
}

impl<C> TokenBucket for RateLimiter<NotKeyed, InMemoryState, C, NoOpMiddleware<C::Instant>>
where
	C: Clock + fmt::Debug + Send + Sync,
	C::Instant: fmt::Debug + Send + Sync,
{
	fn would_exceed(&self, n: NonZeroU32) -> bool {
		!matches!(self.check_n(n), Ok(Ok(())))
	}
}

/// Per-peer rate limiter using a token bucket algorithm.
///
/// The token bucket allows short bursts up to the per-second limit while enforcing
/// the average rate over time.
#[derive(Debug)]
struct PeerRateLimiter {
	bucket: Box<dyn TokenBucket>,
}

impl PeerRateLimiter {
	fn new(statements_per_second: NonZeroU32, burst: NonZeroU32) -> Self {
		Self::with_clock(statements_per_second, burst, &DefaultClock::default())
	}

	/// The same quota, measured against `clock`.
	fn with_clock<C>(statements_per_second: NonZeroU32, burst: NonZeroU32, clock: &C) -> Self
	where
		C: Clock + fmt::Debug + Send + Sync + 'static,
		C::Instant: fmt::Debug + Send + Sync,
	{
		let quota = Quota::per_second(statements_per_second).allow_burst(burst);
		Self { bucket: Box::new(RateLimiter::direct_with_clock(quota, clock)) }
	}

	/// The quota for hashes that one peer announces.
	fn for_received_announces() -> Self {
		Self::new(config::ANNOUNCED_HASHES_PER_SECOND, config::ANNOUNCED_HASHES_BURST)
	}

	/// The sender's mirror of [`Self::for_received_announces`]. Its burst is one announce, so
	/// announces that arrive bunched stay inside the receiver's burst.
	fn for_sent_announces() -> Self {
		let one_announce = NonZeroU32::new(MAX_ANNOUNCED_HASHES as u32)
			.expect("MAX_ANNOUNCED_HASHES is non-zero; qed");
		Self::new(config::ANNOUNCED_HASHES_PER_SECOND, one_announce)
	}

	/// The quota for `ExplicitTopicAffinity` updates from one peer.
	fn for_affinity_updates() -> Self {
		Self::new(config::AFFINITY_UPDATES_PER_SECOND, config::AFFINITY_UPDATES_BURST)
	}

	/// Check if receiving `count` more messages would exceed the rate limit.
	fn is_flooding(&self, count: usize) -> bool {
		if count > u32::MAX as usize {
			return true;
		}

		let Some(n) = NonZeroU32::new(count as u32) else {
			return false;
		};
		self.bucket.would_exceed(n)
	}
}

/// Peer information
#[cfg_attr(not(any(test, feature = "test-helpers")), doc(hidden))]
#[derive(Debug)]
pub struct Peer {
	/// Rate limiter for statement flooding protection.
	statement_rate_limiter: PeerRateLimiter,
	/// Rate limiter for `ExplicitTopicAffinity` updates, kept apart from the statement bucket
	/// so a statement burst cannot reject a filter change.
	affinity_rate_limiter: PeerRateLimiter,
	/// Rate limiter for announced hashes, kept apart from the statement bucket: a hash costs a
	/// lookup where a statement costs a validation.
	announce_rate_limiter: PeerRateLimiter,
	/// Mirror of the peer's limit on announced hashes.
	announce_budget: PeerRateLimiter,
	/// Mirror of the peer's statement rate limit. The limit is not negotiated, so the mirror
	/// assumes our quota.
	send_budget: PeerRateLimiter,
	/// Protocol version negotiated with this peer.
	protocol_version: PeerProtocolVersion,
	/// Topic affinity filter received from a v2 peer.
	/// When set, only statements matching this filter should be propagated to the peer.
	topic_affinity: Option<AffinityFilter>,
	/// Whether this peer is a light client.
	/// Light clients on V2 must set topic affinity before receiving statements.
	is_light: bool,
	/// Whether the peer opened the substream.
	inbound: bool,
	/// A pending topic affinity filter waiting to be scheduled for initial sync.
	/// Set when a new `ExplicitTopicAffinity` arrives; consumed by the main loop
	/// once any in-progress initial sync for this peer completes.
	pending_topic_affinity: Option<AffinityFilter>,
	/// The peer repeated its latest filter, so its next sync sends everything.
	full_replay_requested: bool,
	/// One past the newest admission covered by the peer's initial syncs.
	sync_watermark: u64,
}

/// Tracks pending initial sync state for a peer as a cursor over the store's admission
/// journal (statements fetched on-demand).
struct PendingInitialSync {
	/// Admission sequence number the next burst resumes fetching from.
	cursor: u64,
	/// One past the newest admission covered by this sync.
	watermark: u64,
	started_at: Instant,
	/// Identifies this scheduling, so that a chunk still in flight from a previous one can be told
	/// apart once its result arrives.
	sync_id: u64,
	/// The filter the peer advertised before the one this sync serves.
	already_served: Option<AffinityFilter>,
	/// Set when the sync announces hashes ahead of the bodies. The cursor and the watermark
	/// then stay unused.
	announce: Option<AnnounceSync>,
}

/// Progress of an initial sync that announces short hashes ahead of the bodies.
struct AnnounceSync {
	/// Row of the store's index by topic that the next announce resumes from, `None` once the
	/// walk has reached the end of the index.
	next_row: Option<TopicIndexPosition>,
	/// Number of announces made so far.
	announces: u64,
	/// The announce the peer has yet to answer.
	awaiting: Option<AwaitedAnnounce>,
	/// Statements the peer asked for, sent before the next announce.
	wanted: VecDeque<Hash>,
}

impl AnnounceSync {
	fn new() -> Self {
		Self {
			next_row: Some(TopicIndexPosition::default()),
			announces: 0,
			awaiting: None,
			wanted: VecDeque::new(),
		}
	}
}

struct AwaitedAnnounce {
	id: u64,
	/// The announced statements, in the order of their bits in the answer.
	hashes: Vec<Hash>,
	/// When the announce went out, `None` until its send is confirmed.
	sent_at: Option<Instant>,
}

enum SendOutcome {
	/// The notification was accepted by the network layer.
	Sent,
	/// The network layer rejected the send.
	NetworkError(error::Error),
	/// The send did not complete within `SEND_TIMEOUT`.
	TimedOut,
}

enum SendKind {
	Propagation,
	InitialSync {
		sync_id: u64,
		/// Cursor position the pending sync advances to once this chunk's send is confirmed.
		next_cursor: u64,
	},
	/// An announce of an initial sync.
	Announce {
		sync_id: u64,
		id: u64,
	},
	/// Statements that the peer asked for in answer to an announce.
	Wanted {
		sync_id: u64,
		/// Number of wanted hashes the pending sync drops once this chunk's send is confirmed.
		count: usize,
	},
	/// The answer to a peer's announce. It takes no send slot and no part of the byte budget: a
	/// peer's announces are rate limited, and an answer is at most 513 bytes.
	Answer,
}

impl SendKind {
	fn label(&self) -> &'static str {
		match self {
			Self::Propagation => "propagation",
			Self::InitialSync { .. } | Self::Announce { .. } | Self::Wanted { .. } => {
				"initial_sync"
			},
			Self::Answer => "announce_answer",
		}
	}
}

/// Result of an asynchronous send.
struct PendingSendResult {
	peer: PeerId,
	statement_count: usize,
	bytes_sent: u64,
	result: SendOutcome,
	kind: SendKind,
	/// The peer's `route` when the chunk was queued. Peer may be gone when the result lands.
	route: &'static str,
	/// Id of the sent chunk, propagation or initial-sync. The result is stale if the id
	/// doesn't match the peer's send slot in `in_flight_chunks`.
	chunk_id: u64,
}

/// Type alias for the pending sends future collection, this is a list of in-flight sends to peers.
type PendingSends =
	FuturesUnordered<Pin<Box<dyn Future<Output = PendingSendResult> + Send + 'static>>>;

/// Encoding overhead for V1: just the `Compact<u32>` vec length prefix (max 5 bytes).
const V1_ENVELOPE_OVERHEAD: usize = 5;

/// Encoding overhead for V2: 1 byte enum discriminant + `Compact<u32>` vec length prefix.
const V2_ENVELOPE_OVERHEAD: usize = 1 + V1_ENVELOPE_OVERHEAD;

/// Returns the maximum payload size for statement notifications given the
/// protocol envelope overhead.
fn max_statement_payload_size(envelope_overhead: usize) -> usize {
	debug_assert_eq!(
		V1_ENVELOPE_OVERHEAD,
		Compact::<u32>::max_encoded_len(),
		"V1_ENVELOPE_OVERHEAD must equal Compact::<u32>::max_encoded_len()"
	);
	MAX_STATEMENT_NOTIFICATION_SIZE as usize - envelope_overhead
}

fn unix_timestamp_secs() -> u64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap_or_default()
		.as_secs()
}

/// Fetch the next chunk of statements admitted between `cursor` and `watermark`, filtering
/// in the `admitted_statements` callback so non-matching statements are never cloned into
/// the batch.
///
/// Returns the batch and the accumulated encoded size. A size above `max_size` signals a
/// lone oversized statement: it is taken and the cursor sits past it, so the caller must
/// drop the chunk and move on.
fn fetch_admitted_chunk(
	store: &dyn StatementStore,
	recently_received_statements: &HashMap<Hash, HashSet<PeerId>>,
	pending_statements_peers: &HashMap<Hash, HashSet<PeerId>>,
	who: &PeerId,
	peer_data: &Peer,
	is_dht_target: &dyn Fn(&Statement) -> bool,
	already_served: Option<&AffinityFilter>,
	cursor: u64,
	watermark: u64,
	max_size: usize,
) -> sp_statement_store::Result<(AdmittedBatch, usize)> {
	let now = unix_timestamp_secs();
	let mut accumulated_size = 0;
	let batch = store.admitted_statements(
		cursor,
		watermark,
		INITIAL_SYNC_SCAN_LIMIT,
		&mut |hash, encoded, stmt| {
			if stmt.is_expired(now) {
				return FilterDecision::Skip;
			}
			let matches_filter =
				peer_data.topic_affinity.as_ref().is_none_or(|a| a.matches_statement(stmt));
			let newly_matched = matches_filter &&
				already_served.is_none_or(|served| !served.matches_statement(stmt));
			// A sync after a filter change skips the DHT routing targets too: the sync for the
			// previous filter delivered them.
			if !newly_matched &&
				(already_served.is_some() || peer_data.is_light || !is_dht_target(stmt))
			{
				return FilterDecision::Skip;
			}
			// The peer supplied this statement, do not send it back.
			if has_received_from(recently_received_statements, pending_statements_peers, hash, who)
			{
				return FilterDecision::Skip;
			}
			if accumulated_size > 0 && accumulated_size + encoded.len() > max_size {
				return FilterDecision::Abort;
			}
			// The cursor stays on the aborted statement, so a later burst resumes here once the
			// peer's quota has room again.
			if peer_data.send_budget.is_flooding(1) {
				return FilterDecision::Abort;
			}
			accumulated_size += encoded.len();
			FilterDecision::Take
		},
	)?;
	Ok((batch, accumulated_size))
}

struct FetchedChunk {
	statements: Vec<(Hash, Statement)>,
	/// Hashes consumed from the outbox, whether they yielded a statement or not.
	processed: usize,
	/// Consumed hashes the store yielded no live statement for: absent, corrupt or expired.
	missing: usize,
	/// Encoded size of `statements`, above the maximum only for a lone oversized statement.
	encoded_size: usize,
}

/// Fetch the next chunk of statements for a peer from the front of `hashes`, filtering in the
/// `statements_by_hashes` callback so non-matching statements are never materialized.
///
/// `filter_by_affinity` is off when the hashes were queued by the DHT orchestrator, whose
/// choice of peer is final. A `paced` chunk ends where the peer's statement quota is spent.
fn fetch_statement_chunk(
	store: &dyn StatementStore,
	recently_received_statements: &HashMap<Hash, HashSet<PeerId>>,
	pending_statements_peers: &HashMap<Hash, HashSet<PeerId>>,
	who: &PeerId,
	peer_data: &Peer,
	filter_by_affinity: bool,
	paced: bool,
	hashes: &[Hash],
	max_size: usize,
) -> sp_statement_store::Result<FetchedChunk> {
	let now = unix_timestamp_secs();
	let mut accumulated_size = 0;
	let mut admit = |hash: &Hash, encoded_size: usize, stmt: &Statement| {
		if filter_by_affinity &&
			peer_data.topic_affinity.as_ref().is_some_and(|a| !a.matches_statement(stmt))
		{
			return FilterDecision::Skip;
		}
		// The peer supplied this statement, do not send it back.
		if has_received_from(recently_received_statements, pending_statements_peers, hash, who) {
			return FilterDecision::Skip;
		}
		if accumulated_size > 0 && accumulated_size + encoded_size > max_size {
			return FilterDecision::Abort;
		}
		if paced && peer_data.send_budget.is_flooding(1) {
			return FilterDecision::Abort;
		}
		accumulated_size += encoded_size;
		FilterDecision::Take
	};
	let mut found = 0;
	let (statements, processed) =
		store.statements_by_hashes(hashes, &mut |hash, encoded, stmt| {
			if stmt.is_expired(now) {
				return FilterDecision::Skip;
			}
			let decision = admit(hash, encoded.len(), stmt);
			if !matches!(decision, FilterDecision::Abort) {
				found += 1;
			}
			decision
		})?;
	Ok(FetchedChunk {
		statements,
		processed,
		missing: processed - found,
		encoded_size: accumulated_size,
	})
}

async fn send_with_timeout<F>(send: F) -> SendOutcome
where
	F: Future<Output = Result<(), error::Error>>,
{
	match timeout(SEND_TIMEOUT, send).await {
		Ok(Ok(())) => SendOutcome::Sent,
		Ok(Err(error)) => SendOutcome::NetworkError(error),
		Err(_elapsed) => SendOutcome::TimedOut,
	}
}

/// Whether the peer sent us the statement, directly or while it was queued for
/// validation. `pending_statements_peers` covers the race where a statement is
/// drained for propagation while the peers that sent it still sit there.
fn has_received_from(
	recently_received_statements: &HashMap<Hash, HashSet<PeerId>>,
	pending_statements_peers: &HashMap<Hash, HashSet<PeerId>>,
	hash: &Hash,
	who: &PeerId,
) -> bool {
	recently_received_statements.get(hash).is_some_and(|peers| peers.contains(who)) ||
		pending_statements_peers.get(hash).is_some_and(|peers| peers.contains(who))
}

impl Peer {
	/// Only full nodes on `statement/3` sync by announce.
	fn syncs_by_announce(&self) -> bool {
		self.protocol_version == PeerProtocolVersion::V3 && !self.is_light
	}

	/// The peer floods instead of routing, so it is served like a v1 peer. A light peer on
	/// `statement/2` is served by its filter instead.
	fn without_dht(&self) -> bool {
		match self.protocol_version {
			PeerProtocolVersion::V1 => true,
			PeerProtocolVersion::V2 => !self.is_light,
			PeerProtocolVersion::V3 => false,
		}
	}

	/// Create a new peer for testing/benchmarking purposes.
	#[cfg(any(test, feature = "test-helpers"))]
	pub fn new_for_testing(statements_per_second: NonZeroU32, burst: NonZeroU32) -> Self {
		Self {
			statement_rate_limiter: PeerRateLimiter::new(statements_per_second, burst),
			affinity_rate_limiter: PeerRateLimiter::for_affinity_updates(),
			announce_rate_limiter: PeerRateLimiter::for_received_announces(),
			announce_budget: PeerRateLimiter::for_sent_announces(),
			send_budget: PeerRateLimiter::new(statements_per_second, burst),
			protocol_version: PeerProtocolVersion::V1,
			topic_affinity: None,
			is_light: false,
			inbound: true,
			pending_topic_affinity: None,
			full_replay_requested: false,
			sync_watermark: 0,
		}
	}

	/// Whether this peer is ready to receive statements.
	///
	/// Light V2 peers must set their topic affinity before receiving any statements; with the
	/// DHT path on, every `statement/3` peer must.
	fn can_receive(&self) -> bool {
		!(self.protocol_version != PeerProtocolVersion::V1 &&
			self.topic_affinity.is_none() &&
			(self.is_light || dht_enabled()))
	}

	/// Whether a statement may take a detour through this peer: it is ready to receive, and as
	/// a full node it forwards what it receives.
	fn can_forward(&self) -> bool {
		self.can_receive() && !self.is_light
	}

	/// The `route` label for this peer.
	///
	/// If the DHT path is off, we send everything to every peer, so the answer is always
	/// `without_dht`. If it is on, the peer is `dht` only after we have its filter.
	/// Until then we count it as `without_dht`.
	fn route(&self) -> &'static str {
		if dht_enabled() && !self.without_dht() && self.topic_affinity.is_some() {
			peer_route::DHT
		} else {
			peer_route::LEGACY
		}
	}

	fn kind(&self) -> &'static str {
		if self.is_light {
			"light"
		} else {
			"full"
		}
	}
}

impl<N, S> StatementHandler<N, S>
where
	N: NetworkPeers + NetworkEventStream + NetworkStateInfo,
	S: SyncEventStream + sp_consensus::SyncOracle,
{
	/// Create a new `StatementHandler` for testing/benchmarking purposes.
	#[cfg(any(test, feature = "test-helpers"))]
	pub fn new_for_testing(
		protocol_name: ProtocolName,
		notification_service: Box<dyn NotificationService>,
		propagate_timeout: stream::Fuse<Pin<Box<dyn Stream<Item = ()> + Send>>>,
		network: N,
		sync: S,
		sync_event_stream: stream::Fuse<Pin<Box<dyn Stream<Item = SyncEvent> + Send>>>,
		peers: HashMap<PeerId, Peer>,
		statement_store: Arc<dyn StatementStore>,
		queue_sender: async_channel::Sender<(Statement, oneshot::Sender<SubmitResult>)>,
		statements_per_second: NonZeroU32,
	) -> Self {
		let local_peer = network.local_peer_id();
		let dht = DhtOrchestrator::new(
			&[],
			None,
			crate::config::DEFAULT_BLOOM_FALSE_POS_RATE,
			local_peer,
			PeersTopologyConfig {
				replication_factor: crate::config::DEFAULT_REPLICATION_FACTOR,
				gossip_target: crate::config::DEFAULT_GOSSIP_TARGET,
			},
			protocol_name.clone(),
			None,
		);
		let propagation_outboxes =
			PropagationOutboxes::new(dht_enabled().then(|| statement_store.clone()));
		Self {
			protocol_name,
			notification_service,
			propagate_timeout,
			pending_statements: FuturesUnordered::new(),
			pending_statements_peers: HashMap::new(),
			short_hashes_in_validation: HashSet::new(),
			recently_received_statements: HashMap::new(),
			network,
			sync,
			sync_event_stream,
			network_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = Event> + Send>>)
				.fuse(),
			peers,
			statement_store,
			queue_sender,
			statements_per_second,
			metrics: None,
			initial_sync_timeout: Box::pin(pending().fuse()),
			pending_affinities_timeout: Box::pin(pending().fuse()),
			peer_eviction_timeout: Box::pin(pending().fuse()),
			pending_initial_syncs: HashMap::new(),
			initial_sync_peer_queue: VecDeque::new(),
			next_initial_sync_id: 0,
			initial_sync_in_flight_bytes: 0,
			propagation_outboxes,
			in_flight_chunks: HashMap::new(),
			next_chunk_id: 0,
			propagation_in_flight_bytes: 0,
			parked_propagations: VecDeque::new(),
			pending_sends: FuturesUnordered::new(),
			deferred_peers: HashSet::new(),
			in_peers_full: DEFAULT_IN_PEERS,
			in_peers_light: 0,
			dropped_statements_during_sync: false,
			sync_recovery_peer: None,
			sync_recovery_readd_timeout: Box::pin(pending().fuse()),
			dht,
		}
	}

	/// Get mutable access to pending statements for testing/benchmarking.
	#[cfg(any(test, feature = "test-helpers"))]
	pub fn pending_statements_mut(
		&mut self,
	) -> &mut FuturesUnordered<Pin<Box<dyn Future<Output = (Hash, Option<SubmitResult>)> + Send>>>
	{
		&mut self.pending_statements
	}

	/// The resolver the store should use to derive each statement's retention mask.
	pub fn retention_resolver(
		&self,
	) -> Option<Box<dyn Fn(&Statement) -> RetentionReasonMask + Send + Sync>> {
		self.dht.retention_resolver()
	}

	/// Turns the [`StatementHandler`] into a future that should run forever and not be
	/// interrupted.
	pub async fn run(mut self) {
		loop {
			futures::select_biased! {
				send_result = self.pending_sends.select_next_some() => {
					self.handle_send_result(send_result);
				},
				_ = self.propagate_timeout.next() => {
					self.propagate_statements().await;
					self.shrink_pending_statements_peers();
					self.metrics.as_ref().map(|metrics| {
						metrics.pending_statements.set(self.pending_statements.len() as u64);
					});
				},
				(hash, result) = self.pending_statements.select_next_some() => {
					self.on_statement_submit_result(hash, result);
				},
				sync_event = self.sync_event_stream.next() => {
					if let Some(sync_event) = sync_event {
						self.handle_sync_event(sync_event);
					} else {
						// Syncing has seemingly closed. Closing as well.
						return;
					}
				}
				network_event = self.network_event_stream.next() => {
					if let Some(network_event) = network_event {
						self.handle_network_event(network_event);
					} else {
						// Network event stream has seemingly closed. Closing as well.
						return;
					}
				}
				event = self.notification_service.next_event().fuse() => {
					if let Some(event) = event {
						self.handle_notification_event(event).await
					} else {
						// `Notifications` has seemingly closed. Closing as well.
						return
					}
				}
				_ = &mut self.initial_sync_timeout => {
					self.process_initial_sync_burst();
					self.initial_sync_timeout =
						Box::pin(tokio::time::sleep(INITIAL_SYNC_BURST_INTERVAL).fuse());
				},
				_ = &mut self.pending_affinities_timeout => {
					if dht_enabled() {
						// Advertise this node's filter changes before serving peers their backlog.
						let topics = self.statement_store.subscription_topics();
						self.dht.set_rpc_subscription_topics(&topics);
						if self.dht.major_sync_settled() {
							for filter in self.dht.take_filters_to_broadcast() {
								self.broadcast_local_filter(filter).await;
							}
						}

						self.dht.on_pending_affinities();
						self.dht.refresh_connections(&self.network);
					}
					self.end_unanswered_announce_syncs();
					self.process_pending_affinities();
					self.pending_affinities_timeout =
						Box::pin(tokio::time::sleep(PENDING_AFFINITIES_INTERVAL).fuse());
				},
				_ = &mut self.sync_recovery_readd_timeout => {
					self.try_readd_sync_recovery_peer();
					self.sync_recovery_readd_timeout = Box::pin(pending().fuse());
				},
				_ = &mut self.peer_eviction_timeout => {
					self.dht.evict_stale_peers();
					self.peer_eviction_timeout =
						Box::pin(tokio::time::sleep(PEER_EVICTION_INTERVAL).fuse());
				},
			}

			if self.sync.is_major_syncing() {
				if dht_enabled() {
					self.dht.on_major_sync();
				}
			} else {
				self.drain_deferred_peers();
				self.start_sync_recovery();
			}
		}
	}

	/// Handle a network topology event.
	///
	/// The network event stream is `pending()` unless `dht_enabled()`, so this runs only on the
	/// DHT path.
	fn handle_network_event(&mut self, event: Event) {
		match event {
			Event::PeersDiscovered(peers) => self.dht.on_peers_discovered(peers),
			Event::PeerIdentified { peer, supported_protocols } => self
				.dht
				.on_peer_identified(peer, supported_protocols.contains(&self.protocol_name)),
			// The stream is filtered to `PeersDiscovered` and `PeerIdentified`, so no other
			// variant reaches here.
			_ => {},
		}
	}

	/// Release excess map capacity once the map is less than a quarter full.
	fn shrink_pending_statements_peers(&mut self) {
		const MIN_RETAINED_CAPACITY: usize = 1024;
		let map = &mut self.pending_statements_peers;
		if map.capacity() > MIN_RETAINED_CAPACITY && map.capacity() / 4 > map.len() {
			map.shrink_to(MIN_RETAINED_CAPACITY);
		}
	}

	/// Record a send attempt that never reached the peer.
	fn record_send_failure(&self, reason: &str) {
		self.metrics.as_ref().map(|metrics| {
			metrics.send_failures.with_label_values(&[reason]).inc();
		});
	}

	/// Record a failed send whose statements' delivery to the peer is abandoned.
	fn record_abandoned_send(&self, reason: &str, statement_count: usize) {
		self.record_send_failure(reason);
		self.metrics.as_ref().map(|metrics| {
			metrics
				.undelivered_statements
				.with_label_values(&[reason])
				.inc_by(statement_count as u64);
		});
	}

	/// Advertise an affinity filter to every connected peer that reads one. A peer without the DHT
	/// path is skipped: it floods, and a filter would narrow what it sends.
	async fn broadcast_local_filter(&mut self, filter: AffinityFilter) {
		let encoded = StatementMessage::ExplicitTopicAffinity(filter).encode();
		let peers: Vec<PeerId> = self
			.peers
			.iter()
			.filter(|(_, peer)| !peer.without_dht())
			.map(|(peer_id, _)| *peer_id)
			.collect();
		log::debug!(
			target: LOG_TARGET,
			"Advertising the local affinity filter to {} peers",
			peers.len(),
		);
		for peer in peers {
			self.send_notification(&peer, encoded.clone()).await;
		}
	}

	/// Send this node's affinity filter to a newly connected peer.
	async fn send_local_filter(&mut self, peer: &PeerId) {
		if self.peers.get(peer).is_none_or(Peer::without_dht) {
			return;
		}
		let encoded =
			StatementMessage::ExplicitTopicAffinity(self.dht.advertised_filter()).encode();
		if !self.send_notification(peer, encoded).await {
			log::warn!(
				target: LOG_TARGET,
				"{peer}: Failed to send the local affinity filter, the peer gets no statements \
				until the local topics change",
			);
		}
	}

	/// Send `notification` to `peer`, bounded by [`SEND_TIMEOUT`], counting the bytes sent.
	///
	/// Returns `true` once the network accepts the notification; `false` on a send error or
	/// timeout, both logged at debug.
	async fn send_notification(&mut self, peer: &PeerId, notification: Vec<u8>) -> bool {
		let bytes_to_send = notification.len() as u64;
		match timeout(
			SEND_TIMEOUT,
			self.notification_service.send_async_notification(peer, notification),
		)
		.await
		{
			Ok(Ok(())) => {
				let route = self.peers.get(peer).map_or(peer_route::LEGACY, Peer::route);
				self.metrics.as_ref().map(|metrics| {
					metrics.bytes_sent_total.with_label_values(&[route]).inc_by(bytes_to_send);
				});
				true
			},
			Ok(Err(e)) => {
				log::debug!(target: LOG_TARGET, "Failed to send notification to {peer}: {e:?}");
				false
			},
			Err(e) => {
				log::debug!(target: LOG_TARGET, "Timed out sending notification to {peer}: {e:?}");
				false
			},
		}
	}

	/// Add all peers that were deferred during major sync to the reserved set
	fn drain_deferred_peers(&mut self) {
		if self.deferred_peers.is_empty() {
			return;
		}

		log::debug!(
			target: LOG_TARGET,
			"Major sync complete, adding {} deferred statement peers",
			self.deferred_peers.len(),
		);

		let addrs: HashSet<multiaddr::Multiaddr> = self
			.deferred_peers
			.drain()
			.map(|p| {
				iter::once(multiaddr::Protocol::P2p(p.into())).collect::<multiaddr::Multiaddr>()
			})
			.collect();

		if let Err(err) = self.network.add_peers_to_reserved_set(self.protocol_name.clone(), addrs)
		{
			log::warn!(target: LOG_TARGET, "Failed to add deferred peers: {err}");
		}
	}

	/// Pick one connected peer, remove it from the reserved set (forcing a disconnect), and
	/// schedule it for re-adding after `SYNC_RECOVERY_READD_DELAY`. When the peer reconnects it
	/// performs a fresh initial sync, delivering any statements that were dropped while the
	/// `is_major_syncing` guard was active.
	///
	/// With the DHT path on, only the drop from a peer without the DHT path leads here, so only
	/// such a peer is picked. Steering may hold it reserved, so it is released before its
	/// substream is closed, and the peer, which holds this node in its own reserved set,
	/// reconnects on its own.
	fn start_sync_recovery(&mut self) {
		if !self.dropped_statements_during_sync {
			return;
		}
		self.dropped_statements_during_sync = false;

		if self.sync_recovery_peer.is_some() {
			return;
		}

		let Some(&peer_id) = self
			.peers
			.iter()
			.filter(|(_, peer)| !dht_enabled() || peer.without_dht())
			.map(|(peer_id, _)| peer_id)
			.choose(&mut rand::thread_rng())
		else {
			return;
		};

		log::trace!(
			target: LOG_TARGET,
			"Major sync complete, force-reconnecting {peer_id} for statement recovery",
		);

		if dht_enabled() {
			self.dht.release_peer_without_dht(&self.network, peer_id);
			self.network.disconnect_peer(peer_id, self.protocol_name.clone());
			return;
		}

		if let Err(err) = self.network.remove_peers_from_reserved_set(
			self.protocol_name.clone(),
			iter::once(peer_id).collect(),
		) {
			log::warn!(target: LOG_TARGET, "Failed to remove peer {peer_id} for sync recovery: {err}");
			return;
		}

		self.sync_recovery_peer = Some(peer_id);
		self.sync_recovery_readd_timeout =
			Box::pin(tokio::time::sleep(SYNC_RECOVERY_READD_DELAY).fuse());
	}

	/// Re-adds the sync-recovery peer to the reserved set after the backoff window has elapsed
	fn try_readd_sync_recovery_peer(&mut self) {
		let Some(peer_id) = self.sync_recovery_peer.take() else { return };
		log::trace!(
			target: LOG_TARGET,
			"Re-adding {peer_id} to reserved set after sync recovery window",
		);
		let addr =
			iter::once(multiaddr::Protocol::P2p(peer_id.into())).collect::<multiaddr::Multiaddr>();
		if let Err(err) = self
			.network
			.add_peers_to_reserved_set(self.protocol_name.clone(), iter::once(addr).collect())
		{
			log::warn!(target: LOG_TARGET, "Failed to re-add sync recovery peer {peer_id}: {err}");
		}
	}

	/// Whether the role's inbound quota has room for one more peer.
	///
	/// The protocol controller counts the full and light slots together, so the split by role
	/// happens here, as in the sync engine. A reserved peer holds no controller slot, so it is
	/// left out of the count.
	fn inbound_slot_free(&self, role: ObservedRole) -> bool {
		let is_light = role.is_light();
		let taken = self
			.peers
			.iter()
			.filter(|(id, p)| p.inbound && p.is_light == is_light && !self.dht.is_reserved(id))
			.count();
		let limit = if is_light { self.in_peers_light } else { self.in_peers_full } as usize;
		taken < limit
	}

	/// React to peer connect/disconnect events from the sync subsystem:
	///
	/// - On connect while major-syncing: defer the peer (kept in `deferred_peers`) instead of
	///   adding it to the statement protocol's reserved set, prioritizing block download; deferred
	///   peers are flushed once syncing finishes.
	/// - On connect otherwise: add the peer to the reserved set.
	/// - On disconnect: remove the peer from the reserved set, or from the deferred set if it never
	///   joined.
	///
	/// With the DHT path on, peer steering alone edits the reserved set and a sync peer holds
	/// an ordinary slot, so the events are ignored.
	fn handle_sync_event(&mut self, event: SyncEvent) {
		if dht_enabled() {
			return;
		}
		match event {
			SyncEvent::PeerConnected { peer_id: remote, roles: _ } => {
				if self.sync.is_major_syncing() {
					log::trace!(
						target: LOG_TARGET,
						"Major sync in progress, deferring connection to {remote}",
					);
					self.deferred_peers.insert(remote);
					return;
				}
				let addr = iter::once(multiaddr::Protocol::P2p(remote.into()))
					.collect::<multiaddr::Multiaddr>();
				let result = self.network.add_peers_to_reserved_set(
					self.protocol_name.clone(),
					iter::once(addr).collect(),
				);
				if let Err(err) = result {
					log::error!(target: LOG_TARGET, "Add reserved peer failed: {}", err);
				}
			},
			SyncEvent::PeerDisconnected(remote) => {
				if self.deferred_peers.remove(&remote) {
					return;
				}
				let result = self.network.remove_peers_from_reserved_set(
					self.protocol_name.clone(),
					iter::once(remote).collect(),
				);
				if let Err(err) = result {
					log::error!(target: LOG_TARGET, "Failed to remove reserved peer: {err}");
				}
			},
		}
	}

	/// Whether a notification received while major-syncing may be processed. Statement data
	/// needs current chain state, so only a v2 affinity control passes, and only with the DHT
	/// path on. The SCALE variant tag is peeked, so a dropped batch is never decoded; a v1
	/// payload carries no tag, its first byte belongs to the batch length.
	fn allowed_during_major_sync(&self, peer: &PeerId, notification: &[u8]) -> bool {
		dht_enabled() &&
			self.peers.get(peer).is_some_and(|peer_data| {
				peer_data.protocol_version != PeerProtocolVersion::V1 &&
					notification.starts_with(&[AFFINITY_VARIANT_INDEX])
			})
	}

	/// Dispatch a notification-protocol event for the statement protocol:
	///
	/// - Validates inbound substreams by peer role.
	/// - Tracks stream open/close to maintain per-peer state.
	/// - Decodes incoming notifications: V1 peers send raw statement batches; V2 and V3 peers send
	///   a `StatementMessage` that is either a batch of statements or an `ExplicitTopicAffinity`
	///   advertisement.
	/// - Rate-limits affinity advertisements (reporting `rep::AFFINITY_FLOODING` on abuse);
	///   otherwise stores the filter as pending until applied by the main loop.
	async fn handle_notification_event(&mut self, event: NotificationEvent) {
		match event {
			NotificationEvent::ValidateInboundSubstream { peer, handshake, result_tx, .. } => {
				let role = self.network.peer_role(peer, handshake);
				// A light peer rejected below has answered identify too, so it is marked here.
				if dht_enabled() && role.is_some_and(|role| role.is_light()) {
					self.dht.on_light_peer(peer);
				}
				// Only accept peers whose role can be determined
				let result = match role {
					Some(role)
						if dht_enabled() &&
							!self.dht.is_reserved(&peer) &&
							!self.inbound_slot_free(role) =>
					{
						log::debug!(
							target: LOG_TARGET,
							"Inbound slot quota full for {role:?}, rejecting {peer}",
						);
						ValidationResult::Reject
					},
					Some(_) => ValidationResult::Accept,
					None => ValidationResult::Reject,
				};
				let _ = result_tx.send(result);
			},
			NotificationEvent::NotificationStreamOpened {
				peer,
				negotiated_fallback,
				handshake,
				direction,
			} => {
				// If negotiated_fallback is Some, the peer connected on a fallback protocol.
				// If None, the peer connected on the main protocol.
				let protocol_version =
					PeerProtocolVersion::negotiated(negotiated_fallback.as_deref(), dht_enabled());
				let Some(peer_role) = self.network.peer_role(peer, handshake) else {
					log::debug!(
						target: LOG_TARGET,
						"Peer {peer} connected but role could not be determined, ignoring"
					);
					return;
				};
				let is_light = peer_role.is_light();
				log::debug!(
					target: LOG_TARGET,
					"Peer {peer} connected with statement protocol {protocol_version:?}, role={peer_role:?}"
				);
				let burst = NonZeroU32::new(
					self.statements_per_second.get() * config::STATEMENTS_BURST_COEFFICIENT,
				)
				.expect("burst capacity is nonzero");
				let _was_in = self.peers.insert(
					peer,
					Peer {
						statement_rate_limiter: PeerRateLimiter::new(
							self.statements_per_second,
							burst,
						),
						affinity_rate_limiter: PeerRateLimiter::for_affinity_updates(),
						announce_rate_limiter: PeerRateLimiter::for_received_announces(),
						announce_budget: PeerRateLimiter::for_sent_announces(),
						send_budget: PeerRateLimiter::new(self.statements_per_second, burst),
						protocol_version,
						topic_affinity: None,
						is_light,
						inbound: direction.is_inbound(),
						pending_topic_affinity: None,
						full_replay_requested: false,
						sync_watermark: 0,
					},
				);
				debug_assert!(_was_in.is_none());

				self.metrics.as_ref().map(|metrics| {
					if let Some(peer) = self.peers.get(&peer) {
						metrics.peers_connected.with_label_values(&[peer.kind()]).inc();
					}
				});

				if dht_enabled() {
					if protocol_version == PeerProtocolVersion::V1 {
						// A V1 peer cannot decode a filter and wants everything, so it is served
						// like a peer without the DHT path and stays out of the DHT index.
						self.dht.on_peer_filter_update(peer, AffinityFilter::match_all(0));
					} else if !is_light {
						self.dht.on_substream_opened(peer);
					} else {
						// A light peer stores and routes nothing, so it stays out of the DHT index
						// and reaches the propagation plan through its filter alone.
						self.dht.on_light_peer(peer);
					}
					self.send_local_filter(&peer).await;
				}

				// Peers awaiting their affinity filter are synced by the pending-affinity tick.
				// Light peers owe the filter by protocol; full peers owe it only on `statement/3`.
				// A full `statement/2` peer floods, so with the DHT path on it is served at
				// once.
				if dht_enabled() && protocol_version == PeerProtocolVersion::V2 && !is_light {
					self.serve_without_dht(peer);
				} else if self.peers.get(&peer).is_some_and(Peer::can_receive) {
					self.schedule_initial_sync_for_peer(peer, None);
				}
				self.update_peers_without_dht();
			},
			NotificationEvent::NotificationStreamClosed { peer } => {
				if dht_enabled() {
					self.dht.on_substream_closed(peer);
				}
				let removed_peer = self.peers.remove(&peer);
				debug_assert!(removed_peer.is_some());
				self.update_peers_without_dht();

				if let Some(removed_peer) = removed_peer {
					self.metrics.as_ref().map(|metrics| {
						metrics.peers_connected.with_label_values(&[removed_peer.kind()]).dec();
					});
				}

				if let Some(pending) = self.pending_initial_syncs.remove(&peer) {
					self.record_initial_sync_completion(
						sync_outcome::ABANDONED,
						pending.started_at,
					);
				}
				self.initial_sync_peer_queue.retain(|p| *p != peer);
				let abandoned = self.propagation_outboxes.remove(&peer);
				if abandoned > 0 {
					self.record_abandoned_send(send_failure::DISCONNECTED, abandoned);
				}
				self.in_flight_chunks.remove(&peer);
			},
			NotificationEvent::NotificationReceived { peer, notification } => {
				let bytes_received = notification.len() as u64;
				self.metrics.as_ref().map(|metrics| {
					let route = self.peers.get(&peer).map_or(peer_route::LEGACY, Peer::route);
					metrics.bytes_received_total.with_label_values(&[route]).inc_by(bytes_received);
				});

				// Accept statements only when node is not major syncing
				if self.sync.is_major_syncing() &&
					!self.allowed_during_major_sync(&peer, notification.as_ref())
				{
					log::trace!(
						target: LOG_TARGET,
						"{peer}: Ignoring statements while major syncing or offline"
					);
					// With the DHT path on, a v3 peer replays on the re-advertised filter; a
					// peer without the DHT path gets no such request and is reconnected once the
					// sync ends.
					let from_peer_without_dht =
						self.peers.get(&peer).is_some_and(|peer_data| peer_data.without_dht());
					if !dht_enabled() || from_peer_without_dht {
						self.dropped_statements_during_sync = true;
					}
					return;
				}

				let Some(peer_data) = self.peers.get(&peer) else {
					log::error!(target: LOG_TARGET, "Received notification from unknown peer {peer}");
					return;
				};

				match peer_data.protocol_version {
					PeerProtocolVersion::V1 => {
						// V1 peers send raw Vec<Statement>.
						if let Ok(statements) =
							<StatementBatch as Decode>::decode(&mut notification.as_ref())
						{
							self.on_statements(peer, statements.into_inner());
						} else {
							log::debug!(
								target: LOG_TARGET,
								"Failed to decode v1 statement list from {peer}"
							);
							self.network.report_peer(peer, rep::BAD_MESSAGE);
							if dht_enabled() {
								self.dht.on_peer_misbehaved(peer);
							}
						}
					},
					PeerProtocolVersion::V2 | PeerProtocolVersion::V3 => {
						// V2 and V3 peers send StatementMessage enum.
						if let Ok(message) = StatementMessage::decode(&mut notification.as_ref()) {
							match message {
								StatementMessage::Statements(statements) => {
									self.on_statements(peer, statements.into_inner());
								},
								StatementMessage::ExplicitTopicAffinity(filter) => {
									if peer_data.affinity_rate_limiter.is_flooding(1) {
										log::debug!(
											target: LOG_TARGET,
											"Dropping rate-limited ExplicitTopicAffinity from {peer}"
										);
										self.network.report_peer(peer, rep::AFFINITY_FLOODING);
										if let Some(ref metrics) = self.metrics {
											metrics.affinity_flooding_detected.inc();
										}
										if dht_enabled() {
											self.dht.on_peer_misbehaved(peer);
										}
										return;
									}
									if dht_enabled() {
										self.dht.on_peer_filter_update(peer, filter.clone());
									}

									// Record the filter for propagation decisions, and route it
									// through the pending-affinity path so the sync replays the
									// stored statements it newly matches. Without this a late
									// subscriber sees only the statements that arrive after it
									// subscribes.
									if let Some(peer_data) = self.peers.get_mut(&peer) {
										log::debug!(
											target: LOG_TARGET,
											"Received topic affinity filter from {peer}"
										);
										// Defer both the affinity update and sync scheduling
										// to the main loop tick.
										let latest = peer_data
											.pending_topic_affinity
											.as_ref()
											.or(peer_data.topic_affinity.as_ref());
										if latest == Some(&filter) {
											peer_data.full_replay_requested = true;
										}
										peer_data.pending_topic_affinity = Some(filter);
									}
								},
								StatementMessage::Announce { id, hashes } => {
									if dht_enabled() && peer_data.syncs_by_announce() {
										self.on_announce(peer, id, hashes.into_inner());
									} else {
										self.report_bad_message(peer);
									}
								},
								StatementMessage::Want { id, wanted } => {
									if dht_enabled() && peer_data.syncs_by_announce() {
										self.on_want(peer, id, wanted.into_inner());
									} else {
										self.report_bad_message(peer);
									}
								},
							}
						} else {
							log::debug!(
								target: LOG_TARGET,
								"Failed to decode v2 statement message from {peer}"
							);
							self.network.report_peer(peer, rep::BAD_MESSAGE);
							if dht_enabled() {
								self.dht.on_peer_misbehaved(peer);
							}
						}
					},
				}
			},
		}
	}

	/// Report a message that decodes but that `peer` had no right to send.
	fn report_bad_message(&mut self, peer: PeerId) {
		log::debug!(target: LOG_TARGET, "Unexpected statement message from {peer}");
		self.network.report_peer(peer, rep::BAD_MESSAGE);
		if dht_enabled() {
			self.dht.on_peer_misbehaved(peer);
		}
	}

	/// Answer the announce `id` of `who` with the bitmap of the statements this node lacks: those
	/// neither stored, nor banned, nor waiting for validation.
	fn on_announce(&mut self, who: PeerId, id: u64, hashes: Vec<ShortHash>) {
		let Some(peer) = self.peers.get(&who) else {
			return;
		};
		if peer.announce_rate_limiter.is_flooding(hashes.len()) {
			log::debug!(target: LOG_TARGET, "Dropping rate-limited announce from {who}");
			self.network.report_peer(who, rep::ANNOUNCE_FLOODING);
			if dht_enabled() {
				self.dht.on_peer_misbehaved(who);
			}
			return;
		}

		let bitmap = wanted_bitmap(&hashes, |hash| {
			!self.statement_store.knows_short_hash(hash) &&
				!self.short_hashes_in_validation.contains(hash)
		});
		self.metrics.as_ref().map(|metrics| {
			let wanted: u32 = bitmap.iter().map(|byte| byte.count_ones()).sum();
			metrics.announced_hashes_received.inc_by(hashes.len() as u64);
			metrics.announced_hashes_wanted.inc_by(wanted.into());
		});
		let wanted = WantedBitmap::truncate_from(bitmap);
		let encoded = StatementMessage::Want { id, wanted }.encode();
		let Some(message_sink) = self.notification_service.message_sink(&who) else {
			self.record_send_failure(send_failure::NO_SINK);
			return;
		};
		let route = peer.route();
		let bytes_sent = encoded.len() as u64;
		// A fresh id that occupies no send slot, so the result frees none.
		let chunk_id = self.next_chunk_id;
		self.next_chunk_id = self.next_chunk_id.saturating_add(1);
		// The send waits for room in the peer's buffer, so it runs beside the event loop.
		self.pending_sends.push(Box::pin(async move {
			let result = send_with_timeout(message_sink.send_async_notification(encoded)).await;
			PendingSendResult {
				peer: who,
				statement_count: 0,
				bytes_sent,
				result,
				kind: SendKind::Answer,
				route,
				chunk_id,
			}
		}));
	}

	/// Take the answer of `who` to the announce `id`: the statements it asks for go out before
	/// the next announce.
	fn on_want(&mut self, who: PeerId, id: u64, wanted: Vec<u8>) {
		let awaited = self
			.pending_initial_syncs
			.get_mut(&who)
			.and_then(|pending| pending.announce.as_mut())
			.and_then(|announce| {
				let awaited = announce.awaiting.take_if(|awaited| awaited.id == id)?;
				Some((announce, awaited))
			});
		let Some((announce, awaited)) = awaited else {
			// The answer to an announce sent twice after a send timeout, or to a sync that has
			// ended since.
			log::debug!(target: LOG_TARGET, "Ignoring the answer of {who} to an announce not awaited");
			return;
		};
		if wanted.len() != awaited.hashes.len().div_ceil(8) {
			self.report_bad_message(who);
			self.end_initial_sync(who, sync_outcome::ABANDONED);
			return;
		}
		let requested = awaited
			.hashes
			.into_iter()
			.enumerate()
			.filter(|(position, _)| is_wanted(&wanted, *position))
			.map(|(_, hash)| hash);
		let wanted_before = announce.wanted.len();
		announce.wanted.extend(requested);
		let requested = announce.wanted.len() - wanted_before;
		self.metrics.as_ref().map(|metrics| {
			metrics.announced_hashes_requested.inc_by(requested as u64);
		});
		self.initial_sync_peer_queue.push_back(who);
	}

	/// End the pending initial sync of `peer` with `outcome`.
	fn end_initial_sync(&mut self, peer: PeerId, outcome: &str) {
		if let Some(pending) = self.pending_initial_syncs.remove(&peer) {
			self.record_initial_sync_completion(outcome, pending.started_at);
		}
		self.initial_sync_peer_queue.retain(|queued| *queued != peer);
	}

	/// End every sync whose announce the peer left unanswered for [`ANNOUNCE_ANSWER_TIMEOUT`].
	fn end_unanswered_announce_syncs(&mut self) {
		let unanswered: Vec<PeerId> = self
			.pending_initial_syncs
			.iter()
			.filter(|(_, pending)| {
				pending
					.announce
					.as_ref()
					.and_then(|announce| announce.awaiting.as_ref())
					.and_then(|awaited| awaited.sent_at)
					.is_some_and(|sent_at| sent_at.elapsed() >= ANNOUNCE_ANSWER_TIMEOUT)
			})
			.map(|(peer, _)| *peer)
			.collect();
		for peer in unanswered {
			log::debug!(target: LOG_TARGET, "{peer} left an announce unanswered, ending its sync");
			self.end_initial_sync(peer, sync_outcome::UNANSWERED);
		}
	}

	/// Handle a batch of statements received from a peer.
	///
	/// For the batch:
	/// - Enforces the per-peer rate limit — on abuse, disconnects the peer and reports
	///   `rep::STATEMENT_FLOODING`.
	/// - Skips statements already in the store, reporting `rep::DUPLICATE_STATEMENT` if the same
	///   peer sent it twice.
	/// - Enqueues unknown statements onto the bounded validation queue.
	/// - Drops the remaining statements in the batch if the queue is full
	///   (`MAX_PENDING_STATEMENTS`).
	#[cfg_attr(not(any(test, feature = "test-helpers")), doc(hidden))]
	pub fn on_statements(&mut self, who: PeerId, statements: Statements) {
		log::trace!(target: LOG_TARGET, "Received {} statements from {}", statements.len(), who);

		let route = self.peers.get(&who).map_or(peer_route::LEGACY, Peer::route);
		self.metrics.as_ref().map(|metrics| {
			metrics
				.statements_received
				.with_label_values(&[route])
				.inc_by(statements.len() as u64);
		});

		if let Some(ref mut peer) = self.peers.get_mut(&who) {
			if peer.statement_rate_limiter.is_flooding(statements.len()) {
				log::warn!(
					target: LOG_TARGET,
					"Peer {} exceeded statement rate limit ({} statements/sec). Disconnecting.",
					who,
					self.statements_per_second
				);

				self.network.report_peer(who, rep::STATEMENT_FLOODING);

				// Initiate peer state cleanup in the `NotificationStreamClosed` handler
				if dht_enabled() {
					self.dht.release_peer_without_dht(&self.network, who);
				}
				self.network.disconnect_peer(who, self.protocol_name.clone());

				if let Some(ref metrics) = self.metrics {
					metrics.statement_flooding_detected.inc();
				}

				return;
			}

			let mut statements_left = statements.len() as u64;
			for s in statements {
				if self.pending_statements.len() > MAX_PENDING_STATEMENTS {
					log::debug!(
						target: LOG_TARGET,
						"Ignoring {} statements that exceed `MAX_PENDING_STATEMENTS`({}) limit",
						statements_left,
						MAX_PENDING_STATEMENTS,
					);
					self.metrics.as_ref().map(|metrics| {
						metrics.ignored_statements.inc_by(statements_left);
					});
					break;
				}

				let hash = s.hash();

				if self.statement_store.has_statement(&hash) {
					self.metrics.as_ref().map(|metrics| {
						metrics.known_statements_received.inc();
					});

					// If the statement still awaits its propagation pass, record the
					// peer so the pass does not send it back. Only join an existing
					// entry, or replays would grow the map without bound. Peers that
					// sent a not yet imported statement are tracked in
					// `pending_statements_peers` and move here on import.
					if let Some(peers) = self.recently_received_statements.get_mut(&hash) {
						peers.insert(who);
					}

					// The store can already hold the statement while its validation
					// completion still awaits processing by the event loop. Join the
					// pending entry so the peer moves to `recently_received_statements`
					// on import, exactly as if the message had arrived before the
					// store insert.
					if let Some(peers) = self.pending_statements_peers.get_mut(&hash) {
						if peers.insert(who) {
							self.network.report_peer(who, rep::ANY_STATEMENT);
						} else {
							log::trace!(
								target: LOG_TARGET,
								"Already received the statement from the same peer {who}.",
							);
							self.network.report_peer(who, rep::DUPLICATE_STATEMENT);
							if dht_enabled() {
								self.dht.on_peer_misbehaved(who);
							}
						}
					}
					continue;
				}

				self.network.report_peer(who, rep::ANY_STATEMENT);

				match self.pending_statements_peers.entry(hash) {
					Entry::Vacant(entry) => {
						let (completion_sender, completion_receiver) = oneshot::channel();
						match self.queue_sender.try_send((s, completion_sender)) {
							Ok(()) => {
								self.pending_statements.push(
									async move {
										let res = completion_receiver.await;
										(hash, res.ok())
									}
									.boxed(),
								);
								entry.insert(HashSet::from_iter([who]));
								self.short_hashes_in_validation.insert(short_hash(&hash));
							},
							Err(async_channel::TrySendError::Full(_)) => {
								log::debug!(
									target: LOG_TARGET,
									"Dropped statement because validation channel is full",
								);
							},
							Err(async_channel::TrySendError::Closed(_)) => {
								log::trace!(
									target: LOG_TARGET,
									"Dropped statement because validation channel is closed",
								);
							},
						}
					},
					Entry::Occupied(mut entry) => {
						if !entry.get_mut().insert(who) {
							// Already received this from the same peer.
							self.network.report_peer(who, rep::DUPLICATE_STATEMENT);
							if dht_enabled() {
								self.dht.on_peer_misbehaved(who);
							}
						}
					},
				}

				statements_left -= 1;
			}
		}
	}

	/// Adjust the sending peer's reputation based on the outcome of importing a statement it sent.
	///
	/// Every newly received statement is first charged `rep::ANY_STATEMENT` (a small **decrease**)
	/// in [`on_statements`](Self::on_statements); this method applies the follow-up adjustment
	/// once the statement has been validated:
	///
	/// - `New` → **increase** by `rep::GOOD_STATEMENT` — a valid, previously unknown statement; the
	///   net change is positive (the reward outweighs the initial charge).
	/// - `Known` → **increase** by `rep::ANY_STATEMENT_REFUND`, which exactly cancels the initial
	///   `rep::ANY_STATEMENT` charge (net zero) — valid but already in the store.
	/// - `Invalid` → **decrease** by `rep::INVALID_STATEMENT`, a large penalty — the statement
	///   failed validation.
	/// - `KnownExpired`, `Rejected`, `InternalError` → no follow-up change, so the peer keeps the
	///   initial `rep::ANY_STATEMENT` charge.
	fn on_handle_statement_import(&mut self, who: PeerId, import: &SubmitResult) {
		if dht_enabled() {
			self.dht.on_statement_imported(who, import);
		}
		match import {
			SubmitResult::New => self.network.report_peer(who, rep::GOOD_STATEMENT),
			SubmitResult::Known => self.network.report_peer(who, rep::ANY_STATEMENT_REFUND),
			SubmitResult::KnownExpired => {},
			SubmitResult::Rejected(_) => {},
			SubmitResult::Invalid(_) => self.network.report_peer(who, rep::INVALID_STATEMENT),
			SubmitResult::InternalError(_) => {},
		}
	}

	/// Handle a completed validation task. Adjusts the reputation of every peer
	/// that sent us the statement and, if the statement awaits propagation,
	/// records those peers so it is not sent back to them.
	fn on_statement_submit_result(&mut self, hash: Hash, result: Option<SubmitResult>) {
		self.short_hashes_in_validation.remove(&short_hash(&hash));
		if let Some(peers) = self.pending_statements_peers.remove(&hash) {
			if let Some(result) = result {
				for peer in &peers {
					self.on_handle_statement_import(*peer, &result);
				}
				// `New` and `Known` mean the statement awaits the next propagation
				// pass. Remember who sent it so the pass does not send it back.
				if matches!(result, SubmitResult::New | SubmitResult::Known) {
					self.recently_received_statements.entry(hash).or_default().extend(peers);
				}
			}
		} else {
			log::warn!(target: LOG_TARGET, "Inconsistent state, no peers for pending statement!");
		}
	}

	/// Queue the given `statements` for propagation to the given `peer`.
	///
	/// Internally filters out statements the peer sent to us.
	/// For v2 peers with a topic affinity filter, also filters by topic match.
	/// Surviving hashes are appended to the peer's outbox.
	fn queue_statements_for_peer(&mut self, who: &PeerId, statements: &[(u64, Hash, Statement)]) {
		let Some(peer) = self.peers.get(who) else {
			return;
		};

		if !peer.can_receive() {
			return;
		}

		let to_send: Vec<_> = statements
			.iter()
			.filter_map(|(seq, hash, stmt)| {
				if *seq < peer.sync_watermark {
					return None;
				}
				// The peer supplied this statement, do not send it back.
				if has_received_from(
					&self.recently_received_statements,
					&self.pending_statements_peers,
					hash,
					who,
				) {
					return None;
				}
				// For v2 peers with topic affinity, filter by topic match.
				if peer.topic_affinity.as_ref().is_some_and(|a| !a.matches_statement(stmt)) {
					return None;
				}
				Some(*hash)
			})
			.collect();

		self.push_outbox_hashes(who, to_send);
	}

	/// Queue the `indices` of `statements` for each peer the orchestrator chose as a target.
	///
	/// The orchestrator already picked the peer, so neither its sync watermark nor its affinity
	/// filter applies and only statements the peer sent to us are dropped.
	fn queue_statements_for_targets(
		&mut self,
		statements: &[(u64, Hash, Statement)],
		targets: Vec<(PeerId, Vec<usize>)>,
	) {
		for (who, indices) in targets {
			let Some(peer) = self.peers.get(&who) else {
				continue;
			};

			// A peer that still owes its filter is skipped: the initial sync that follows the
			// filter resends everything skipped here.
			if !peer.can_receive() {
				continue;
			}

			let to_send: Vec<_> = indices
				.iter()
				.filter_map(|&index| {
					let (_, hash, _) = &statements[index];
					// The peer supplied this statement, do not send it back.
					if has_received_from(
						&self.recently_received_statements,
						&self.pending_statements_peers,
						hash,
						&who,
					) {
						return None;
					}
					Some(*hash)
				})
				.collect();

			self.push_outbox_hashes(&who, to_send);
		}
	}

	/// Hold the statements that no routing target can receive, so the retention sweep keeps them
	/// until [`Self::resolve_dead_ends`] finds them a peer.
	fn hold_dead_ends(&mut self, statements: &[(u64, Hash, Statement)], dead_ends: Vec<usize>) {
		for index in dead_ends {
			let (seq, hash, statement) = &statements[index];
			let senders = [&self.recently_received_statements, &self.pending_statements_peers]
				.iter()
				.filter_map(|senders| senders.get(hash))
				.flatten()
				.copied()
				.collect();
			self.propagation_outboxes.hold_dead_end(DeadEnd {
				seq: *seq,
				hash: *hash,
				topics: statement.topics().to_vec(),
				expires_at: u64::from(statement.get_expiration_timestamp_secs()),
				senders,
			});
		}
	}

	/// Release, send or detour each held dead end. The farthest peer takes the detour because its
	/// routing peers lie elsewhere in the key space.
	fn resolve_dead_ends(&mut self) {
		if self.propagation_outboxes.dead_ends().is_empty() {
			return;
		}
		let detour_peers: Vec<PeerId> = self
			.dht
			.peers_farthest_first()
			.into_iter()
			.filter(|peer| self.peers.get(peer).is_some_and(|peer| peer.can_forward()))
			.collect();
		let now = unix_timestamp_secs();
		let mut resolved = HashSet::new();
		let mut sends = Vec::new();
		for dead_end in self.propagation_outboxes.dead_ends() {
			if now >= dead_end.expires_at {
				resolved.insert(dead_end.hash);
				continue;
			}
			let synced = |peer: &PeerId| {
				!self.pending_initial_syncs.contains_key(peer) &&
					self.peers.get(peer).is_some_and(|peer| peer.sync_watermark > dead_end.seq)
			};
			// No sync of such a target will reach the statement, so it has to be sent.
			let missed_by_sync = |peer: &&PeerId| {
				!dead_end.senders.contains(peer) &&
					self.peers.get(peer).is_some_and(|peer| {
						peer.can_receive() && peer.sync_watermark <= dead_end.seq
					})
			};
			let targets = self.dht.routing_targets(&dead_end.topics);
			if self.dht.is_replica_for(&dead_end.topics) || targets.iter().any(synced) {
				resolved.insert(dead_end.hash);
			} else if targets.is_empty() {
				let detour = detour_peers.iter().find(|peer| !dead_end.senders.contains(peer));
				if let Some(peer) = detour {
					log::debug!(
						target: LOG_TARGET,
						"No routing target for {:?}, detour via {peer}",
						dead_end.hash,
					);
					sends.push((*peer, dead_end.hash));
				}
			} else {
				let missed = targets.iter().filter(missed_by_sync);
				sends.extend(missed.map(|peer| (*peer, dead_end.hash)));
			}
		}
		// The outbox takes its own hold on a hash before the dead end gives up its one.
		for (peer, hash) in sends {
			self.push_outbox_hashes(&peer, vec![hash]);
			resolved.insert(hash);
		}
		self.propagation_outboxes
			.retain_dead_ends(|dead_end| !resolved.contains(&dead_end.hash));
	}

	/// Append `hashes` to the peer's outbox and send the next chunk if its slot is free.
	fn push_outbox_hashes(&mut self, who: &PeerId, hashes: Vec<Hash>) {
		if hashes.is_empty() {
			return;
		}
		let queued = hashes.len();
		let overflow = self.propagation_outboxes.push(*who, hashes);

		log::trace!(target: LOG_TARGET, "We have {queued} statements that the peer doesn't know about");

		if overflow > 0 {
			self.record_abandoned_send(send_failure::OUTBOX_FULL, overflow);
		}
		self.try_send_next_chunk(*who);
	}

	/// Send the next propagation chunk to `who` if its send slot is free.
	///
	/// Statements are fetched, filtered and encoded only when a chunk actually goes out, so a
	/// slow peer holds one encoded chunk, not its whole backlog. Hashes whose statements left
	/// the store since they were queued are dropped.
	fn try_send_next_chunk(&mut self, who: PeerId) {
		if self.in_flight_chunks.contains_key(&who) {
			return;
		}

		loop {
			let Some(outbox) = self.propagation_outboxes.get(&who) else {
				return;
			};
			if outbox.is_empty() {
				self.propagation_outboxes.remove(&who);
				return;
			}
			let Some(peer_data) = self.peers.get(&who) else {
				self.propagation_outboxes.remove(&who);
				return;
			};
			// Admission is checked before fetching, so a saturated budget leaves the
			// outbox untouched.
			if self.send_in_flight_bytes() >= self.propagation_send_budget() {
				// A peer parks once per saturation, not once per tick, so a budget that
				// stays full does not grow the deque without bound.
				if !self.parked_propagations.contains(&who) {
					self.parked_propagations.push_back(who);
				}
				return;
			}
			let peer_version = peer_data.protocol_version;
			let max_size = max_statement_payload_size(peer_version.envelope_overhead());
			let Some(outbox) = self.propagation_outboxes.queued(&who) else {
				return;
			};
			let chunk = match fetch_statement_chunk(
				&*self.statement_store,
				&self.recently_received_statements,
				&self.pending_statements_peers,
				&who,
				peer_data,
				!dht_enabled(),
				false,
				outbox,
				max_size,
			) {
				Ok(result) => result,
				Err(e) => {
					// A store read error says nothing about the queued hashes, so the outbox
					// is kept and the peer parked: the fetch is retried when a completed send
					// or a propagation tick next refills parked peers.
					log::warn!(
						target: LOG_TARGET,
						"Failed to fetch statements for propagation to {who}, retaining {} queued hashes: {e:?}",
						outbox.len(),
					);
					if !self.parked_propagations.contains(&who) {
						self.parked_propagations.push_back(who);
					}
					return;
				},
			};
			let FetchedChunk { statements, processed, missing, encoded_size: accumulated_size } =
				chunk;

			debug_assert!(
				processed > 0,
				"a fetch from a non-empty outbox consumes at least one hash"
			);
			if processed == 0 {
				return;
			}

			// Consume the fetched hashes before the oversized check, otherwise the oversized
			// statement would be fetched again on the next iteration.
			self.propagation_outboxes.consume(&who, processed);

			if missing > 0 {
				self.record_abandoned_send(send_failure::MISSING_FROM_STORE, missing);
			}

			if accumulated_size > max_size {
				log::warn!(target: LOG_TARGET, "Statement too large, skipping");
				self.metrics.as_ref().map(|metrics| {
					metrics.skipped_oversized_statements.inc();
				});
				continue;
			}

			if statements.is_empty() {
				// Everything fetched was filtered out or pruned, but the remaining hashes
				// may still yield a chunk.
				continue;
			}

			let statement_count = statements.len();
			let send_stmts: Vec<_> = statements.iter().map(|(_, stmt)| stmt).collect();
			let encoded = match peer_version {
				PeerProtocolVersion::V1 => send_stmts.encode(),
				PeerProtocolVersion::V2 | PeerProtocolVersion::V3 => {
					StatementMessage::encode_statement_refs(&send_stmts)
				},
			};
			let bytes_sent = encoded.len() as u64;
			let Some(message_sink) = self.notification_service.message_sink(&who) else {
				let abandoned = statement_count + self.propagation_outboxes.remove(&who);
				log::debug!(
					target: LOG_TARGET,
					"Failed to get message sink for peer {who}, abandoning {abandoned} statements ({bytes_sent} bytes in the current chunk)",
				);
				self.record_abandoned_send(send_failure::NO_SINK, abandoned);
				return;
			};
			let chunk_id = self.occupy_send_slot(who);
			let in_flight = self.propagation_in_flight_bytes.saturating_add(bytes_sent);
			self.set_propagation_in_flight_bytes(in_flight);
			self.charge_send_budget(&who, statement_count);
			let route = self.peers.get(&who).map_or(peer_route::LEGACY, Peer::route);
			let sent_latency =
				self.metrics.as_ref().map(|metrics| metrics.sent_latency_seconds.clone());
			self.pending_sends.push(Box::pin(async move {
				let sent_latency_timer = sent_latency.map(|metric| metric.start_timer());
				let result = send_with_timeout(message_sink.send_async_notification(encoded)).await;
				drop(sent_latency_timer);
				PendingSendResult {
					peer: who,
					statement_count,
					bytes_sent,
					result,
					kind: SendKind::Propagation,
					route,
					chunk_id,
				}
			}));
			return;
		}
	}

	fn handle_send_result(&mut self, send_result: PendingSendResult) {
		let peer = send_result.peer;
		let slot_freed = self.process_send_result(send_result);
		self.fill_parked_propagations();
		if slot_freed {
			self.try_send_next_chunk(peer);
		}
	}

	/// Returns whether the result freed the peer's send slot.
	fn process_send_result(&mut self, send_result: PendingSendResult) -> bool {
		let PendingSendResult { peer, statement_count, bytes_sent, result, kind, route, chunk_id } =
			send_result;

		let kind_label = kind.label();
		match kind {
			SendKind::Propagation => {
				debug_assert!(
					self.propagation_in_flight_bytes >= bytes_sent,
					"propagation in-flight byte counter underflow"
				);
				let in_flight = self.propagation_in_flight_bytes.saturating_sub(bytes_sent);
				self.set_propagation_in_flight_bytes(in_flight);
			},
			SendKind::InitialSync { .. } | SendKind::Announce { .. } | SendKind::Wanted { .. } => {
				debug_assert!(
					self.initial_sync_in_flight_bytes >= bytes_sent,
					"initial-sync in-flight byte counter underflow"
				);
				let in_flight = self.initial_sync_in_flight_bytes.saturating_sub(bytes_sent);
				self.set_initial_sync_in_flight_bytes(in_flight);
			},
			SendKind::Answer => {},
		}

		let failure = match result {
			SendOutcome::Sent => {
				log::trace!(target: LOG_TARGET, "Sent {} statements to {}", statement_count, peer);
				self.metrics.as_ref().map(|metrics| {
					metrics
						.propagated_statements
						.with_label_values(&[route])
						.inc_by(statement_count as u64);
					metrics.bytes_sent_total.with_label_values(&[route]).inc_by(bytes_sent);
					if !matches!(kind, SendKind::Announce { .. } | SendKind::Answer) {
						metrics
							.propagated_statements_chunks
							.with_label_values(&[kind_label])
							.observe(statement_count as f64);
					}
				});
				None
			},
			SendOutcome::NetworkError(error) => {
				log::debug!(
					target: LOG_TARGET,
					"Failed to send {statement_count} statements ({bytes_sent} bytes) to {peer}: {error}",
				);
				Some(send_failure::NETWORK)
			},
			SendOutcome::TimedOut => {
				log::warn!(
					target: LOG_TARGET,
					"Send of {statement_count} statements ({bytes_sent} bytes) to {peer} timed out after {SEND_TIMEOUT:?}",
				);
				Some(send_failure::TIMEOUT)
			},
		};

		if let Some(reason) = failure {
			match kind {
				SendKind::Propagation => self.record_abandoned_send(reason, statement_count),
				SendKind::InitialSync { .. } |
				SendKind::Announce { .. } |
				SendKind::Wanted { .. } |
				SendKind::Answer => self.record_send_failure(reason),
			}
		}

		// A send future is not cancelled on disconnect, so its result can outlive the
		// connection. Only the result of the chunk still occupying the slot frees it.
		let slot_freed = self.in_flight_chunks.get(&peer) == Some(&chunk_id);
		if slot_freed {
			self.in_flight_chunks.remove(&peer);
		}

		let sync_id = match kind {
			SendKind::Propagation | SendKind::Answer => return slot_freed,
			SendKind::InitialSync { sync_id, .. } |
			SendKind::Announce { sync_id, .. } |
			SendKind::Wanted { sync_id, .. } => sync_id,
		};

		// A peer that reconnects inside the send timeout loses its sync on disconnect and gets a
		// fresh one under the same `PeerId`; a stale result would advance or abort the wrong sync.
		let Some(pending) = self
			.pending_initial_syncs
			.get_mut(&peer)
			.filter(|pending| pending.sync_id == sync_id)
		else {
			return slot_freed;
		};

		if let SendKind::Announce { id, .. } = kind {
			// An answer that outran this result has already put the peer back in the queue.
			let awaited = pending
				.announce
				.as_mut()
				.and_then(|announce| announce.awaiting.as_mut())
				.filter(|awaited| awaited.id == id);
			match awaited {
				// The answer or the timeout continues the sync.
				Some(awaited) if failure.is_none() => awaited.sent_at = Some(Instant::now()),
				// A later burst sends the unconfirmed announce again.
				Some(_) => self.initial_sync_peer_queue.push_back(peer),
				None => {},
			}
			return slot_freed;
		}

		if failure.is_some() {
			// The cursor still points at the unconfirmed chunk, so a later burst resends it.
			self.initial_sync_peer_queue.push_back(peer);
			return slot_freed;
		}

		match kind {
			SendKind::InitialSync { next_cursor, .. } => pending.cursor = next_cursor,
			SendKind::Wanted { count, .. } => {
				if let Some(announce) = pending.announce.as_mut() {
					let sent = count.min(announce.wanted.len());
					announce.wanted.drain(..sent);
				}
			},
			SendKind::Propagation | SendKind::Announce { .. } | SendKind::Answer => {},
		}
		self.metrics.as_ref().map(|metrics| {
			metrics
				.initial_sync_statements_sent
				.with_label_values(&[route])
				.inc_by(statement_count as u64);
		});
		// Reached only for the live sync, which is out of the queue for as long as its chunk is
		// in flight; a superseded sync's chunk can still be in flight under the same `PeerId`, so
		// the bound is one chunk per sync, not per peer.
		self.initial_sync_peer_queue.push_back(peer);
		slot_freed
	}

	#[cfg(test)]
	async fn flush_pending_sends(&mut self) {
		while let Some(result) = self.pending_sends.next().await {
			self.handle_send_result(result);
		}
	}

	fn do_propagate_statements(&mut self, statements: &[(u64, Hash, Statement)]) {
		log::debug!(target: LOG_TARGET, "Propagating {} statements for {} peers", statements.len(), self.peers.len());
		let peers: Vec<_> = self.peers.keys().copied().collect();
		for who in peers {
			log::trace!(target: LOG_TARGET, "Start propagating statements for {}", who);
			self.queue_statements_for_peer(&who, statements);
		}
		log::trace!(target: LOG_TARGET, "Statements queued for propagation to all peers");
	}

	/// Call when we must propagate ready statements to peers.
	async fn propagate_statements(&mut self) {
		// Send out statements only when node is not major syncing
		if self.sync.is_major_syncing() {
			return;
		}

		// A peer parked by a failed store fetch has no completed send to unpark it, so
		// parked peers are also refilled on the tick.
		self.fill_parked_propagations();

		let Ok(statements) = self.statement_store.take_recent_statements() else { return };
		if !statements.is_empty() {
			if dht_enabled() {
				let plan = self.dht.propagation_plan(&statements, |peer| {
					self.peers.get(peer).is_some_and(Peer::can_receive)
				});
				self.queue_statements_for_targets(&statements, plan.targets);
				self.hold_dead_ends(&statements, plan.dead_ends);
			} else {
				self.do_propagate_statements(&statements);
			}
			// The outboxes and the dead ends hold what they keep, so the take's holds can go.
			self.propagation_outboxes.release_taken(&statements);
		}
		if dht_enabled() {
			self.resolve_dead_ends();
		}
		// Every entry here belongs to an already drained statement. It is done propagating, or
		// it is a dead end with its own copy of the senders. Statements imported after the
		// drain get their entries only after this clear, when the event loop processes their
		// submit results.
		self.recently_received_statements.clear();
	}

	/// Schedule an initial sync for a peer, replacing any pending one.
	///
	/// This is called both when a new peer connects and when a peer's topic affinity changes.
	/// `already_served` is the filter the peer advertised before a change: the sync then sends
	/// only what its new filter matches beyond it. Without it the sync sends everything due to
	/// the peer.
	///
	/// Returns `false` when the store failed to give the admission watermark.
	fn schedule_initial_sync_for_peer(
		&mut self,
		peer: PeerId,
		already_served: Option<AffinityFilter>,
	) -> bool {
		// A peer absent from the map has no entry to mirror the sync's watermark into.
		if !self.peers.contains_key(&peer) {
			return true;
		}
		// The watermark is read before the existing sync is touched, so a store error
		// leaves an in-progress sync running instead of destroying it with no successor.
		let watermark = match self.statement_store.admission_watermark() {
			Ok(watermark) => watermark,
			Err(e) => {
				log::warn!(
					target: LOG_TARGET,
					"Failed to read the admission watermark, skipping initial sync for {peer}: {e:?}",
				);
				return false;
			},
		};
		let sync_id = self.next_initial_sync_id;
		self.next_initial_sync_id = self.next_initial_sync_id.saturating_add(1);
		let announce = self
			.peers
			.get(&peer)
			.filter(|peer_data| dht_enabled() && peer_data.syncs_by_announce())
			.map(|_| AnnounceSync::new());
		if let Some(pending) = self.pending_initial_syncs.remove(&peer) {
			self.record_initial_sync_completion(sync_outcome::ABANDONED, pending.started_at);
			self.initial_sync_peer_queue.retain(|p| *p != peer);
		}
		if watermark > 0 {
			// A sync after a filter change skips what the previous filter matched, so the outbox
			// and the propagation watermark of the previous sync still have to deliver it.
			if already_served.is_none() {
				if let Some(peer_data) = self.peers.get_mut(&peer) {
					peer_data.sync_watermark = peer_data.sync_watermark.max(watermark);
				}
				// Hashes queued for propagation before this scheduling sit below the new
				// watermark, so the cursor already covers them; dropping the outbox keeps
				// them from arriving twice. A hash on a detour is lost here, because the sync of
				// a peer that is no routing target skips it.
				self.propagation_outboxes.remove(&peer);
			}
			self.pending_initial_syncs.insert(
				peer,
				PendingInitialSync {
					cursor: 0,
					watermark,
					started_at: Instant::now(),
					sync_id,
					already_served,
					announce,
				},
			);
			self.initial_sync_peer_queue.push_back(peer);
			self.metrics.as_ref().map(|metrics| {
				metrics.initial_sync_peers_active.inc();
			});
		}
		true
	}

	/// Serve a full `statement/2` peer as a node without the DHT path: a match-all filter, the
	/// v1 meaning of no filter, no place in the DHT index, and an initial sync.
	fn serve_without_dht(&mut self, peer: PeerId) {
		let Some(peer_data) = self.peers.get_mut(&peer) else {
			return;
		};
		// The seed is irrelevant to a filter with every bit set.
		let filter = AffinityFilter::match_all(0);
		peer_data.topic_affinity = Some(filter.clone());
		if dht_enabled() {
			// The propagation plan reads peer filters from the orchestrator.
			self.dht.on_peer_filter_update(peer, filter);
			self.dht.on_peer_without_dht(peer);
		}
		self.schedule_initial_sync_for_peer(peer, None);
	}

	/// Count the connected peers served as nodes without the DHT path.
	fn update_peers_without_dht(&self) {
		if !dht_enabled() {
			return;
		}
		let Some(metrics) = &self.metrics else { return };
		let count = self.peers.values().filter(|peer| peer.without_dht()).count();
		metrics.peers_without_dht.set(count as u64);
	}

	/// Process pending topic affinity changes for peers that have no active initial sync.
	///
	/// When a peer sends `ExplicitTopicAffinity`, we defer the expensive
	/// `schedule_initial_sync_for_peer` call. This method applies the pending affinity
	/// and schedules the sync once the peer's current sync (if any) has completed.
	fn process_pending_affinities(&mut self) {
		let ready_peers: Vec<PeerId> = self
			.peers
			.iter()
			.filter(|(peer_id, peer_data)| {
				peer_data.pending_topic_affinity.is_some() &&
					!self.pending_initial_syncs.contains_key(peer_id)
			})
			.map(|(peer_id, _)| *peer_id)
			.collect();

		for peer_id in ready_peers {
			let Some(peer_data) = self.peers.get_mut(&peer_id) else { continue };
			let Some(filter) = peer_data.pending_topic_affinity.take() else { continue };
			// A filter repeated unchanged asks for a full replay: that is how a peer recovers
			// what it dropped during major sync.
			let previous = std::mem::replace(&mut peer_data.topic_affinity, Some(filter.clone()));
			// The request outlives a changed filter that arrives before the replay is scheduled.
			let full_replay = std::mem::take(&mut peer_data.full_replay_requested);
			let already_served =
				previous.clone().filter(|previous| !full_replay && *previous != filter);
			if !self.schedule_initial_sync_for_peer(peer_id, already_served) {
				// A filter counts as served only once its sync is scheduled, so it waits for the
				// next pass.
				if let Some(peer_data) = self.peers.get_mut(&peer_id) {
					peer_data.full_replay_requested = full_replay;
					peer_data.topic_affinity = previous;
					peer_data.pending_topic_affinity = Some(filter);
				}
			}
		}
	}

	/// Set the in-flight initial-sync byte counter.
	fn set_initial_sync_in_flight_bytes(&mut self, bytes: u64) {
		self.initial_sync_in_flight_bytes = bytes;
		self.metrics
			.as_ref()
			.map(|metrics| metrics.initial_sync_in_flight_bytes.set(bytes));
	}

	/// Set the in-flight propagation byte counter.
	fn set_propagation_in_flight_bytes(&mut self, bytes: u64) {
		self.propagation_in_flight_bytes = bytes;
		self.metrics
			.as_ref()
			.map(|metrics| metrics.propagation_in_flight_bytes.set(bytes));
	}

	/// Total encoded bytes in flight across initial-sync and propagation chunks,
	/// held against the shared [`MAX_SEND_IN_FLIGHT_BYTES`] budget.
	fn send_in_flight_bytes(&self) -> u64 {
		self.initial_sync_in_flight_bytes
			.saturating_add(self.propagation_in_flight_bytes)
	}

	/// Byte budget available to propagation sends.
	///
	/// While initial syncs are pending, [`INITIAL_SYNC_RESERVED_BYTES`] are withheld:
	/// propagation reclaims freed budget synchronously on every completed send, while sync
	/// bursts only check on a timer, so without the reserve enough parked propagations
	/// starve initial sync indefinitely.
	fn propagation_send_budget(&self) -> u64 {
		if self.pending_initial_syncs.is_empty() {
			MAX_SEND_IN_FLIGHT_BYTES
		} else {
			MAX_SEND_IN_FLIGHT_BYTES - INITIAL_SYNC_RESERVED_BYTES
		}
	}

	/// Refill parked peers' send slots while the in-flight byte budget allows.
	///
	/// Peers are served in parking order. When the budget saturates the loop stops and the
	/// remaining peers keep their position for the next completed send. Each peer gets one
	/// attempt per pass: a peer whose store fetch fails parks itself again, and an unbounded
	/// loop would spin on it.
	fn fill_parked_propagations(&mut self) {
		for _ in 0..self.parked_propagations.len() {
			if self.send_in_flight_bytes() >= self.propagation_send_budget() {
				return;
			}
			let Some(peer) = self.parked_propagations.pop_front() else { return };
			self.try_send_next_chunk(peer);
		}
	}

	/// Propagation never waits, so a chunk past the quota goes out uncharged.
	fn charge_send_budget(&self, peer: &PeerId, statements: usize) {
		if let Some(peer_data) = self.peers.get(peer) {
			peer_data.send_budget.is_flooding(statements);
		}
	}

	/// Occupy the peer's send slot with a fresh chunk id and return the id.
	fn occupy_send_slot(&mut self, peer: PeerId) -> u64 {
		let chunk_id = self.next_chunk_id;
		self.next_chunk_id = self.next_chunk_id.saturating_add(1);
		self.in_flight_chunks.insert(peer, chunk_id);
		chunk_id
	}

	/// Record initial sync completion metrics for a peer being removed.
	fn record_initial_sync_completion(&self, outcome: &str, started_at: Instant) {
		self.metrics.as_ref().map(|metrics| {
			metrics.initial_sync_peers_active.dec();
			metrics
				.initial_sync_duration_seconds
				.with_label_values(&[outcome])
				.observe(started_at.elapsed().as_secs_f64());
		});
	}

	/// Process one batch of initial sync for the next peer in the queue (round-robin).
	fn process_initial_sync_burst(&mut self) {
		if self.sync.is_major_syncing() {
			return;
		}

		if self.send_in_flight_bytes() >= MAX_SEND_IN_FLIGHT_BYTES {
			log::debug!(
				target: LOG_TARGET,
				"Skipping initial sync burst, {} bytes still in flight",
				self.send_in_flight_bytes(),
			);
			return;
		}

		// A peer whose send slot is busy keeps its turn for a later burst, so one slow peer
		// does not stall every other pending sync.
		let Some(pos) = self
			.initial_sync_peer_queue
			.iter()
			.position(|peer| !self.in_flight_chunks.contains_key(peer))
		else {
			return;
		};
		self.initial_sync_peer_queue.rotate_left(pos);
		let Some(peer_id) = self.initial_sync_peer_queue.pop_front() else {
			return;
		};

		let Entry::Occupied(mut entry) = self.pending_initial_syncs.entry(peer_id) else {
			return;
		};
		let sync_id = entry.get().sync_id;
		let route = self.peers.get(&peer_id).map_or(peer_route::LEGACY, Peer::route);

		self.metrics.as_ref().map(|metrics| {
			metrics.initial_sync_bursts_total.with_label_values(&[route]).inc();
		});

		if entry.get().announce.is_some() {
			self.process_announce_sync_burst(peer_id, sync_id, route);
			return;
		}

		if entry.get().cursor >= entry.get().watermark {
			let started_at = entry.get().started_at;
			entry.remove();
			self.record_initial_sync_completion(sync_outcome::COMPLETED, started_at);
			return;
		}

		// Fetch statements up to max_statement_payload_size, filtering directly in the
		// callback (see `fetch_admitted_chunk`).
		let Some(peer_data) = self.peers.get(&peer_id) else {
			log::error!(target: LOG_TARGET, "Peer {peer_id} has pending initial sync but is not in peers map");
			let pending = entry.remove();
			self.record_initial_sync_completion(sync_outcome::ABANDONED, pending.started_at);
			return;
		};
		let peer_version = peer_data.protocol_version;
		let envelope_overhead = peer_version.envelope_overhead();
		let max_size = max_statement_payload_size(envelope_overhead);
		let (batch, accumulated_size) = match fetch_admitted_chunk(
			&*self.statement_store,
			&self.recently_received_statements,
			&self.pending_statements_peers,
			&peer_id,
			peer_data,
			&self.dht.dht_target_predicate(peer_id),
			entry.get().already_served.as_ref(),
			entry.get().cursor,
			entry.get().watermark,
			max_size,
		) {
			Ok(r) => r,
			Err(e) => {
				// A store read error says nothing about the journal, and the cursor is
				// retained, so the sync resumes from the same position on a later burst.
				log::warn!(
					target: LOG_TARGET,
					"Failed to fetch statements for initial sync of {peer_id}, will retry: {e:?}",
				);
				self.initial_sync_peer_queue.push_back(peer_id);
				return;
			},
		};

		// A failed send must resend the same admissions, so the cursor advances only when a
		// send is confirmed; chunks that queue no send advance it here.
		if accumulated_size > max_size {
			log::warn!(target: LOG_TARGET, "Statement too large, skipping");
			self.metrics.as_ref().map(|metrics| {
				metrics.skipped_oversized_statements.inc();
			});
			entry.get_mut().cursor = batch.cursor;
			self.initial_sync_peer_queue.push_back(peer_id);
			return;
		}

		if batch.statements.is_empty() {
			// Nothing was queued, so no result will arrive for this peer. Put it back and let the
			// next burst either send the remainder or observe that the sync is done.
			entry.get_mut().cursor = batch.cursor;
			self.initial_sync_peer_queue.push_back(peer_id);
			return;
		}

		let next_cursor = batch.cursor;

		let statement_count = batch.statements.len();
		let send_stmts: Vec<_> = batch.statements.iter().map(|(_, stmt)| stmt).collect();
		let encoded = match peer_version {
			PeerProtocolVersion::V1 => send_stmts.encode(),
			PeerProtocolVersion::V2 | PeerProtocolVersion::V3 => {
				StatementMessage::encode_statement_refs(&send_stmts)
			},
		};
		let bytes_to_send = encoded.len() as u64;
		let Some(message_sink) = self.notification_service.message_sink(&peer_id) else {
			// A missing sink usually means the peer is disconnecting, which removes the sync;
			// until then the retained cursor lets a later burst retry.
			log::debug!(
				target: LOG_TARGET,
				"Failed to get message sink for peer {peer_id}, its initial sync will retry",
			);
			self.record_send_failure(send_failure::NO_SINK);
			self.initial_sync_peer_queue.push_back(peer_id);
			return;
		};
		let sent_latency =
			self.metrics.as_ref().map(|metrics| metrics.sent_latency_seconds.clone());
		let in_flight = self.initial_sync_in_flight_bytes.saturating_add(bytes_to_send);
		self.set_initial_sync_in_flight_bytes(in_flight);
		let chunk_id = self.occupy_send_slot(peer_id);
		self.pending_sends.push(Box::pin(async move {
			let sent_latency_timer = sent_latency.map(|metric| metric.start_timer());
			let result = send_with_timeout(message_sink.send_async_notification(encoded)).await;
			drop(sent_latency_timer);
			PendingSendResult {
				peer: peer_id,
				statement_count,
				bytes_sent: bytes_to_send,
				result,
				kind: SendKind::InitialSync { sync_id, next_cursor },
				route,
				chunk_id,
			}
		}));
	}

	/// One step of a sync that announces: send what the peer asked for, else announce the next
	/// hashes, else finish.
	fn process_announce_sync_burst(&mut self, peer_id: PeerId, sync_id: u64, route: &'static str) {
		let Some(pending) = self.pending_initial_syncs.get_mut(&peer_id) else { return };
		let Some(announce) = pending.announce.as_mut() else { return };
		if let Some(awaited) = &announce.awaiting {
			// The answer or the timeout continues the sync. Only an announce whose send failed
			// goes out again.
			if awaited.sent_at.is_none() {
				self.send_announce(peer_id, sync_id, route);
			}
			return;
		}
		if !announce.wanted.is_empty() {
			self.send_wanted_chunk(peer_id, sync_id, route);
			return;
		}
		let Some(from) = announce.next_row else {
			self.end_initial_sync(peer_id, sync_outcome::COMPLETED);
			return;
		};
		let Some(peer_data) = self.peers.get(&peer_id) else {
			log::error!(target: LOG_TARGET, "Peer {peer_id} has pending initial sync but is not in peers map");
			self.end_initial_sync(peer_id, sync_outcome::ABANDONED);
			return;
		};

		// The test of `fetch_admitted_chunk`, applied to a topic. A statement with several
		// topics may pass where its other topic was served already: the peer then holds it and
		// asks for nothing.
		let filter = peer_data.topic_affinity.as_ref();
		let already_served = pending.already_served.as_ref();
		let dht = &self.dht;
		let mut is_due = |topic: &Topic| {
			let newly_matched = filter.is_none_or(|filter| filter.contains(topic)) &&
				already_served.is_none_or(|served| !served.contains(topic));
			newly_matched || (already_served.is_none() && dht.is_dht_target(*topic, &peer_id))
		};
		let batch =
			match self.statement_store.hashes_by_topic(&from, MAX_ANNOUNCED_HASHES, &mut is_due) {
				Ok(batch) => batch,
				Err(e) => {
					// The row is retained, so the sync resumes from it on a later burst.
					log::warn!(
						target: LOG_TARGET,
						"Failed to walk the topic index for initial sync of {peer_id}, will retry: {e:?}",
					);
					self.initial_sync_peer_queue.push_back(peer_id);
					return;
				},
			};

		// A statement with several due topics shows up once for each, and the peer has no use
		// for a statement it supplied.
		let mut seen = HashSet::new();
		let hashes: Vec<Hash> = batch
			.hashes
			.into_iter()
			.filter(|hash| {
				!has_received_from(
					&self.recently_received_statements,
					&self.pending_statements_peers,
					hash,
					&peer_id,
				) && seen.insert(*hash)
			})
			.collect();
		if hashes.is_empty() {
			announce.next_row = batch.next;
			self.initial_sync_peer_queue.push_back(peer_id);
			return;
		}
		// The row stays, so a later burst walks the same rows once the quota has room again.
		if peer_data.announce_budget.is_flooding(hashes.len()) {
			self.initial_sync_peer_queue.push_back(peer_id);
			return;
		}
		announce.next_row = batch.next;
		// Sync ids are unique on this node, so no two of its announces share an id.
		let id = (sync_id << 32) | announce.announces;
		announce.announces += 1;
		self.metrics.as_ref().map(|metrics| {
			metrics.announced_hashes_sent.inc_by(hashes.len() as u64);
		});
		announce.awaiting = Some(AwaitedAnnounce { id, hashes, sent_at: None });
		self.send_announce(peer_id, sync_id, route);
	}

	/// Send the announce that the sync of `peer_id` awaits the answer to.
	fn send_announce(&mut self, peer_id: PeerId, sync_id: u64, route: &'static str) {
		let Some(awaited) = self
			.pending_initial_syncs
			.get(&peer_id)
			.and_then(|pending| pending.announce.as_ref())
			.and_then(|announce| announce.awaiting.as_ref())
		else {
			return;
		};
		let id = awaited.id;
		let hashes: Vec<ShortHash> = awaited.hashes.iter().map(short_hash).collect();
		let encoded =
			StatementMessage::Announce { id, hashes: AnnouncedHashes::truncate_from(hashes) }
				.encode();
		self.send_sync_notification(peer_id, encoded, 0, SendKind::Announce { sync_id, id }, route);
	}

	/// Send the next chunk of the statements that `peer_id` asked for.
	fn send_wanted_chunk(&mut self, peer_id: PeerId, sync_id: u64, route: &'static str) {
		let Some(peer_data) = self.peers.get(&peer_id) else {
			log::error!(target: LOG_TARGET, "Peer {peer_id} has pending initial sync but is not in peers map");
			self.end_initial_sync(peer_id, sync_outcome::ABANDONED);
			return;
		};
		let Some(announce) = self
			.pending_initial_syncs
			.get_mut(&peer_id)
			.and_then(|pending| pending.announce.as_mut())
		else {
			return;
		};
		let max_size = max_statement_payload_size(peer_data.protocol_version.envelope_overhead());
		let chunk = match fetch_statement_chunk(
			&*self.statement_store,
			&self.recently_received_statements,
			&self.pending_statements_peers,
			&peer_id,
			peer_data,
			false,
			true,
			announce.wanted.make_contiguous(),
			max_size,
		) {
			Ok(chunk) => chunk,
			Err(e) => {
				// The wanted hashes are retained, so a later burst fetches them again.
				log::warn!(
					target: LOG_TARGET,
					"Failed to fetch statements for initial sync of {peer_id}, will retry: {e:?}",
				);
				self.initial_sync_peer_queue.push_back(peer_id);
				return;
			},
		};
		let FetchedChunk { statements, processed, encoded_size, .. } = chunk;

		// A failed send must resend the same statements, so the wanted hashes are dropped only
		// when a send is confirmed; chunks that queue no send drop them here.
		let oversized = encoded_size > max_size;
		if oversized || statements.is_empty() {
			announce.wanted.drain(..processed);
			if oversized {
				log::warn!(target: LOG_TARGET, "Statement too large, skipping");
				self.metrics.as_ref().map(|metrics| {
					metrics.skipped_oversized_statements.inc();
				});
			}
			self.initial_sync_peer_queue.push_back(peer_id);
			return;
		}

		let send_stmts: Vec<_> = statements.iter().map(|(_, stmt)| stmt).collect();
		let encoded = StatementMessage::encode_statement_refs(&send_stmts);
		self.send_sync_notification(
			peer_id,
			encoded,
			statements.len(),
			SendKind::Wanted { sync_id, count: processed },
			route,
		);
	}

	/// Queue a notification of an announced sync in the send slot of `peer_id`.
	fn send_sync_notification(
		&mut self,
		peer_id: PeerId,
		encoded: Vec<u8>,
		statement_count: usize,
		kind: SendKind,
		route: &'static str,
	) {
		let Some(message_sink) = self.notification_service.message_sink(&peer_id) else {
			// A missing sink usually means the peer is disconnecting, which removes the sync;
			// until then a later burst retries.
			log::debug!(
				target: LOG_TARGET,
				"Failed to get message sink for peer {peer_id}, its initial sync will retry",
			);
			self.record_send_failure(send_failure::NO_SINK);
			self.initial_sync_peer_queue.push_back(peer_id);
			return;
		};
		let bytes_sent = encoded.len() as u64;
		let sent_latency =
			self.metrics.as_ref().map(|metrics| metrics.sent_latency_seconds.clone());
		let in_flight = self.initial_sync_in_flight_bytes.saturating_add(bytes_sent);
		self.set_initial_sync_in_flight_bytes(in_flight);
		let chunk_id = self.occupy_send_slot(peer_id);
		self.pending_sends.push(Box::pin(async move {
			let sent_latency_timer = sent_latency.map(|metric| metric.start_timer());
			let result = send_with_timeout(message_sink.send_async_notification(encoded)).await;
			drop(sent_latency_timer);
			PendingSendResult {
				peer: peer_id,
				statement_count,
				bytes_sent,
				result,
				kind,
				route,
				chunk_id,
			}
		}));
	}
}

#[cfg(test)]
mod tests {

	use super::*;
	use crate::test_helpers::{filter_over, topic, topology_config};
	use governor::clock::FakeRelativeClock;
	use sp_statement_store::Topic;
	use std::{
		sync::{
			atomic::{AtomicBool, AtomicUsize, Ordering},
			Mutex,
		},
		time::Duration,
	};

	/// Default seed used for bloom filters in tests.
	const BLOOM_SEED: u128 = 0x5EED_5EED_5EED_5EED;

	fn default_statement_limiter() -> PeerRateLimiter {
		PeerRateLimiter::new(
			NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND)
				.expect("DEFAULT_STATEMENTS_PER_SECOND is nonzero"),
			NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND * config::STATEMENTS_BURST_COEFFICIENT)
				.expect("burst capacity is nonzero"),
		)
	}

	fn new_live_statement() -> Statement {
		let mut statement = sp_statement_store::Statement::new();
		statement.set_expiry_from_parts(u32::MAX, 0);
		statement
	}

	#[derive(Clone)]
	struct TestNetwork {
		local_peer: PeerId,
		reported_peers: Arc<Mutex<Vec<(PeerId, sc_network::ReputationChange)>>>,
		disconnected_peers: Arc<Mutex<Vec<PeerId>>>,
		/// Role to return from `peer_role`. Default: `Full`.
		default_role: sc_network::ObservedRole,
		added_reserved: Arc<Mutex<Vec<HashSet<sc_network::Multiaddr>>>>,
		removed_reserved: Arc<Mutex<Vec<Vec<PeerId>>>>,
	}

	impl TestNetwork {
		fn new() -> Self {
			Self {
				local_peer: PeerId::random(),
				reported_peers: Arc::new(Mutex::new(Vec::new())),
				disconnected_peers: Arc::new(Mutex::new(Vec::new())),
				default_role: sc_network::ObservedRole::Full,
				added_reserved: Arc::new(Mutex::new(Vec::new())),
				removed_reserved: Arc::new(Mutex::new(Vec::new())),
			}
		}

		fn new_light() -> Self {
			Self {
				local_peer: PeerId::random(),
				reported_peers: Arc::new(Mutex::new(Vec::new())),
				disconnected_peers: Arc::new(Mutex::new(Vec::new())),
				default_role: sc_network::ObservedRole::Light,
				added_reserved: Arc::new(Mutex::new(Vec::new())),
				removed_reserved: Arc::new(Mutex::new(Vec::new())),
			}
		}

		fn get_reports(&self) -> Vec<(PeerId, sc_network::ReputationChange)> {
			self.reported_peers.lock().unwrap().clone()
		}

		fn get_disconnected_peers(&self) -> Vec<PeerId> {
			self.disconnected_peers.lock().unwrap().clone()
		}

		fn get_added_reserved(&self) -> Vec<HashSet<sc_network::Multiaddr>> {
			self.added_reserved.lock().unwrap().clone()
		}

		fn get_removed_reserved(&self) -> Vec<Vec<PeerId>> {
			self.removed_reserved.lock().unwrap().clone()
		}
	}

	#[async_trait::async_trait]
	impl NetworkPeers for TestNetwork {
		fn set_authorized_peers(&self, _: std::collections::HashSet<PeerId>) {
			unimplemented!()
		}

		fn set_authorized_only(&self, _: bool) {
			unimplemented!()
		}

		fn add_known_address(&self, _: PeerId, _: sc_network::Multiaddr) {
			unimplemented!()
		}

		fn report_peer(&self, peer_id: PeerId, cost_benefit: sc_network::ReputationChange) {
			self.reported_peers.lock().unwrap().push((peer_id, cost_benefit));
		}

		fn peer_reputation(&self, _: &PeerId) -> i32 {
			unimplemented!()
		}

		fn disconnect_peer(&self, peer: PeerId, _: sc_network::ProtocolName) {
			self.disconnected_peers.lock().unwrap().push(peer);
		}

		fn accept_unreserved_peers(&self) {
			unimplemented!()
		}

		fn deny_unreserved_peers(&self) {
			unimplemented!()
		}

		fn add_reserved_peer(
			&self,
			_: sc_network::config::MultiaddrWithPeerId,
		) -> Result<(), String> {
			unimplemented!()
		}

		fn remove_reserved_peer(&self, _: PeerId) {
			unimplemented!()
		}

		fn set_reserved_peers(
			&self,
			_: sc_network::ProtocolName,
			_: std::collections::HashSet<sc_network::Multiaddr>,
		) -> Result<(), String> {
			unimplemented!()
		}

		fn add_peers_to_reserved_set(
			&self,
			_: sc_network::ProtocolName,
			addrs: std::collections::HashSet<sc_network::Multiaddr>,
		) -> Result<(), String> {
			self.added_reserved.lock().unwrap().push(addrs);
			Ok(())
		}

		fn remove_peers_from_reserved_set(
			&self,
			_: sc_network::ProtocolName,
			peers: Vec<PeerId>,
		) -> Result<(), String> {
			self.removed_reserved.lock().unwrap().push(peers);
			Ok(())
		}

		fn sync_num_connected(&self) -> usize {
			unimplemented!()
		}

		fn peer_role(&self, _: PeerId, _: Vec<u8>) -> Option<sc_network::ObservedRole> {
			Some(self.default_role)
		}

		async fn reserved_peers(&self) -> Result<Vec<PeerId>, ()> {
			unimplemented!();
		}
	}

	#[derive(Clone)]
	struct TestSync {
		major_syncing: Arc<AtomicBool>,
	}

	impl TestSync {
		fn new() -> Self {
			Self { major_syncing: Arc::new(AtomicBool::new(false)) }
		}

		fn with_syncing(initial: bool) -> (Self, Arc<AtomicBool>) {
			let flag = Arc::new(AtomicBool::new(initial));
			(Self { major_syncing: flag.clone() }, flag)
		}
	}

	impl SyncEventStream for TestSync {
		fn event_stream(
			&self,
			_name: &'static str,
		) -> Pin<Box<dyn Stream<Item = sc_network_sync::types::SyncEvent> + Send>> {
			Box::pin(futures::stream::pending())
		}
	}

	impl sp_consensus::SyncOracle for TestSync {
		fn is_major_syncing(&self) -> bool {
			self.major_syncing.load(Ordering::Relaxed)
		}

		fn is_offline(&self) -> bool {
			unimplemented!()
		}
	}

	impl NetworkStateInfo for TestNetwork {
		fn external_addresses(&self) -> Vec<sc_network::Multiaddr> {
			Vec::new()
		}

		fn listen_addresses(&self) -> Vec<sc_network::Multiaddr> {
			Vec::new()
		}

		fn local_peer_id(&self) -> PeerId {
			self.local_peer
		}
	}

	impl NetworkEventStream for TestNetwork {
		fn event_stream(
			&self,
			_name: &'static str,
		) -> Pin<Box<dyn Stream<Item = sc_network::Event> + Send>> {
			unimplemented!()
		}
	}

	#[derive(Debug, Clone)]
	struct TestNotificationService {
		sent_notifications: Arc<Mutex<Vec<(PeerId, Vec<u8>)>>>,
		block_sends: Arc<AtomicBool>,
		fail_sends: Arc<AtomicBool>,
		sinks_available: Arc<AtomicUsize>,
	}

	impl TestNotificationService {
		fn new() -> Self {
			Self {
				sent_notifications: Arc::new(Mutex::new(Vec::new())),
				block_sends: Arc::new(AtomicBool::new(false)),
				fail_sends: Arc::new(AtomicBool::new(false)),
				sinks_available: Arc::new(AtomicUsize::new(usize::MAX)),
			}
		}

		fn get_sent_notifications(&self) -> Vec<(PeerId, Vec<u8>)> {
			self.sent_notifications.lock().unwrap().clone()
		}

		fn clear_sent_notifications(&self) {
			self.sent_notifications.lock().unwrap().clear();
		}

		fn block_sends(&self) {
			self.block_sends.store(true, Ordering::Relaxed);
		}

		fn fail_sends(&self) {
			self.fail_sends.store(true, Ordering::Relaxed);
		}

		fn allow_sends(&self) {
			self.fail_sends.store(false, Ordering::Relaxed);
		}

		fn serve_sinks(&self, count: usize) {
			self.sinks_available.store(count, Ordering::Relaxed);
		}
	}

	struct TestMessageSink {
		peer: PeerId,
		sent_notifications: Arc<Mutex<Vec<(PeerId, Vec<u8>)>>>,
		block_sends: Arc<AtomicBool>,
		fail_sends: Arc<AtomicBool>,
	}

	#[async_trait::async_trait]
	impl sc_network::service::traits::MessageSink for TestMessageSink {
		fn send_sync_notification(&self, notification: Vec<u8>) {
			self.sent_notifications.lock().unwrap().push((self.peer, notification));
		}

		async fn send_async_notification(
			&self,
			notification: Vec<u8>,
		) -> Result<(), sc_network::error::Error> {
			if self.block_sends.load(Ordering::Relaxed) {
				futures::future::pending::<()>().await;
			}
			if self.fail_sends.load(Ordering::Relaxed) {
				return Err(sc_network::error::Error::ConnectionClosed);
			}
			self.sent_notifications.lock().unwrap().push((self.peer, notification));
			Ok(())
		}
	}

	#[async_trait::async_trait]
	impl NotificationService for TestNotificationService {
		async fn open_substream(&mut self, _peer: PeerId) -> Result<(), ()> {
			unimplemented!()
		}

		async fn close_substream(&mut self, _peer: PeerId) -> Result<(), ()> {
			unimplemented!()
		}

		fn send_sync_notification(&mut self, peer: &PeerId, notification: Vec<u8>) {
			self.sent_notifications.lock().unwrap().push((*peer, notification));
		}

		async fn send_async_notification(
			&mut self,
			peer: &PeerId,
			notification: Vec<u8>,
		) -> Result<(), sc_network::error::Error> {
			if self.fail_sends.load(Ordering::Relaxed) {
				return Err(sc_network::error::Error::ConnectionClosed);
			}
			self.sent_notifications.lock().unwrap().push((*peer, notification));
			Ok(())
		}

		async fn set_handshake(&mut self, _handshake: Vec<u8>) -> Result<(), ()> {
			unimplemented!()
		}

		fn try_set_handshake(&mut self, _handshake: Vec<u8>) -> Result<(), ()> {
			unimplemented!()
		}

		async fn next_event(&mut self) -> Option<sc_network::service::traits::NotificationEvent> {
			None
		}

		fn clone(&mut self) -> Result<Box<dyn NotificationService>, ()> {
			unimplemented!()
		}

		fn protocol(&self) -> &sc_network::types::ProtocolName {
			unimplemented!()
		}

		fn message_sink(
			&self,
			peer: &PeerId,
		) -> Option<Box<dyn sc_network::service::traits::MessageSink>> {
			self.sinks_available
				.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(1))
				.ok()?;
			Some(Box::new(TestMessageSink {
				peer: *peer,
				sent_notifications: self.sent_notifications.clone(),
				block_sends: self.block_sends.clone(),
				fail_sends: self.fail_sends.clone(),
			}))
		}
	}

	#[derive(Clone)]
	struct TestStatementStore {
		statements: Arc<Mutex<HashMap<sp_statement_store::Hash, sp_statement_store::Statement>>>,
		recent_statements:
			Arc<Mutex<HashMap<sp_statement_store::Hash, sp_statement_store::Statement>>>,
		subscription_topics: Arc<Mutex<Vec<Topic>>>,
		/// Admission journal: the vector index is the statement's admission sequence number.
		admissions: Arc<Mutex<Vec<sp_statement_store::Hash>>>,
		fail_fetches: Arc<AtomicBool>,
		held: Arc<Mutex<HashMap<sp_statement_store::Hash, usize>>>,
	}

	impl TestStatementStore {
		fn new() -> Self {
			Self {
				statements: Default::default(),
				recent_statements: Default::default(),
				subscription_topics: Default::default(),
				admissions: Default::default(),
				fail_fetches: Arc::new(AtomicBool::new(false)),
				held: Default::default(),
			}
		}

		/// Insert a statement into the store, recording it in the admission journal.
		fn insert(&self, statement: sp_statement_store::Statement) {
			let hash = statement.hash();
			self.statements.lock().unwrap().insert(hash, statement);
			self.admit(hash);
		}

		/// Record the hash in the admission journal unless it is already admitted, and
		/// return its admission sequence number.
		fn admit(&self, hash: sp_statement_store::Hash) -> u64 {
			let mut admissions = self.admissions.lock().unwrap();
			if let Some(seq) = admissions.iter().position(|admitted| *admitted == hash) {
				return seq as u64;
			}
			admissions.push(hash);
			(admissions.len() - 1) as u64
		}
	}

	impl StatementStore for TestStatementStore {
		fn statements(
			&self,
		) -> sp_statement_store::Result<
			Vec<(sp_statement_store::Hash, sp_statement_store::Statement)>,
		> {
			Ok(self.statements.lock().unwrap().iter().map(|(h, s)| (*h, s.clone())).collect())
		}

		fn take_recent_statements(
			&self,
		) -> sp_statement_store::Result<
			Vec<(u64, sp_statement_store::Hash, sp_statement_store::Statement)>,
		> {
			// A recent statement is a statement the store holds, so make the drained
			// statements visible to `statements_by_hashes` like the real store does.
			let drained: Vec<_> = self.recent_statements.lock().unwrap().drain().collect();
			let mut statements = self.statements.lock().unwrap();
			for (hash, statement) in &drained {
				statements.insert(*hash, statement.clone());
			}
			drop(statements);
			let mut result: Vec<_> = drained
				.into_iter()
				.map(|(hash, statement)| (self.admit(hash), hash, statement))
				.collect();
			result.sort_unstable_by_key(|(seq, ..)| *seq);
			let hashes: Vec<_> = result.iter().map(|(_, hash, _)| *hash).collect();
			self.hold_statements(&hashes);
			Ok(result)
		}

		fn hold_statements(&self, hashes: &[sp_statement_store::Hash]) {
			let mut held = self.held.lock().unwrap();
			for hash in hashes {
				*held.entry(*hash).or_default() += 1;
			}
		}

		fn release_statements(&self, hashes: &[sp_statement_store::Hash]) {
			let mut held = self.held.lock().unwrap();
			for hash in hashes {
				let Entry::Occupied(mut count) = held.entry(*hash) else {
					panic!("released {hash:?} without a hold");
				};
				*count.get_mut() -= 1;
				if *count.get() == 0 {
					count.remove();
				}
			}
		}

		fn statement(
			&self,
			_hash: &sp_statement_store::Hash,
		) -> sp_statement_store::Result<Option<sp_statement_store::Statement>> {
			unimplemented!()
		}

		fn has_statement(&self, hash: &sp_statement_store::Hash) -> bool {
			self.statements.lock().unwrap().contains_key(hash)
		}

		fn subscription_topics(&self) -> HashSet<Topic> {
			self.subscription_topics.lock().unwrap().iter().copied().collect()
		}

		fn statements_by_hashes(
			&self,
			hashes: &[sp_statement_store::Hash],
			filter: &mut dyn FnMut(
				&sp_statement_store::Hash,
				&[u8],
				&sp_statement_store::Statement,
			) -> FilterDecision,
		) -> sp_statement_store::Result<(
			Vec<(sp_statement_store::Hash, sp_statement_store::Statement)>,
			usize,
		)> {
			if self.fail_fetches.load(Ordering::Relaxed) {
				return Err(sp_statement_store::Error::Db("fetch failed".into()));
			}
			let statements = self.statements.lock().unwrap();
			let mut result = Vec::new();
			let mut processed = 0;
			for hash in hashes {
				let Some(stmt) = statements.get(hash) else {
					processed += 1;
					continue;
				};
				let encoded = stmt.encode();
				match filter(hash, &encoded, stmt) {
					FilterDecision::Skip => {
						processed += 1;
					},
					FilterDecision::Take => {
						processed += 1;
						result.push((*hash, stmt.clone()));
					},
					FilterDecision::Abort => break,
				}
			}
			Ok((result, processed))
		}

		fn admission_watermark(&self) -> sp_statement_store::Result<u64> {
			if self.fail_fetches.load(Ordering::Relaxed) {
				return Err(sp_statement_store::Error::Db("injected watermark failure".into()));
			}
			Ok(self.admissions.lock().unwrap().len() as u64)
		}

		fn admitted_statements(
			&self,
			mut cursor: u64,
			watermark: u64,
			scan_limit: usize,
			filter: &mut dyn FnMut(
				&sp_statement_store::Hash,
				&[u8],
				&sp_statement_store::Statement,
			) -> FilterDecision,
		) -> sp_statement_store::Result<sp_statement_store::AdmittedBatch> {
			if self.fail_fetches.load(Ordering::Relaxed) {
				return Err(sp_statement_store::Error::Db("fetch failed".into()));
			}
			let admissions = self.admissions.lock().unwrap();
			let statements = self.statements.lock().unwrap();
			let mut result = Vec::new();
			let mut aborted = false;
			let mut scanned = 0usize;
			while cursor < watermark {
				if scanned == scan_limit {
					aborted = true;
					break;
				}
				scanned += 1;
				let Some(hash) = admissions.get(cursor as usize) else { break };
				// A journal entry whose statement left the store is a dead sequence number.
				let Some(statement) = statements.get(hash) else {
					cursor += 1;
					continue;
				};
				let encoded = statement.encode();
				match filter(hash, &encoded, statement) {
					FilterDecision::Skip => cursor += 1,
					FilterDecision::Take => {
						result.push((*hash, statement.clone()));
						cursor += 1;
					},
					FilterDecision::Abort => {
						aborted = true;
						break;
					},
				}
			}
			if !aborted && cursor < watermark {
				cursor = watermark;
			}
			Ok(sp_statement_store::AdmittedBatch {
				statements: result,
				cursor,
				done: cursor >= watermark,
			})
		}

		fn hashes_by_topic(
			&self,
			from: &TopicIndexPosition,
			scan_limit: usize,
			wanted: &mut dyn FnMut(&Topic) -> bool,
		) -> sp_statement_store::Result<sp_statement_store::TopicHashesBatch> {
			if self.fail_fetches.load(Ordering::Relaxed) {
				return Err(sp_statement_store::Error::Db("fetch failed".into()));
			}
			let mut rows: Vec<TopicIndexPosition> = self
				.statements
				.lock()
				.unwrap()
				.iter()
				.flat_map(|(hash, statement)| {
					statement.topics().iter().map(|topic| (*topic, *hash)).collect::<Vec<_>>()
				})
				.filter(|row| row >= from)
				.collect();
			rows.sort_unstable();
			let mut hashes = Vec::new();
			for (scanned, row) in rows.into_iter().enumerate() {
				if scanned == scan_limit {
					return Ok(sp_statement_store::TopicHashesBatch { hashes, next: Some(row) });
				}
				if wanted(&row.0) {
					hashes.push(row.1);
				}
			}
			Ok(sp_statement_store::TopicHashesBatch { hashes, next: None })
		}

		fn knows_short_hash(&self, short: &ShortHash) -> bool {
			self.statements.lock().unwrap().keys().any(|hash| short_hash(hash) == *short)
		}

		fn broadcasts(
			&self,
			_match_all_topics: &[sp_statement_store::Topic],
		) -> sp_statement_store::Result<Vec<Vec<u8>>> {
			unimplemented!()
		}

		fn posted(
			&self,
			_match_all_topics: &[sp_statement_store::Topic],
			_dest: [u8; 32],
		) -> sp_statement_store::Result<Vec<Vec<u8>>> {
			unimplemented!()
		}

		fn posted_clear(
			&self,
			_match_all_topics: &[sp_statement_store::Topic],
			_dest: [u8; 32],
		) -> sp_statement_store::Result<Vec<Vec<u8>>> {
			unimplemented!()
		}

		fn broadcasts_stmt(
			&self,
			_match_all_topics: &[sp_statement_store::Topic],
		) -> sp_statement_store::Result<Vec<Vec<u8>>> {
			unimplemented!()
		}

		fn posted_stmt(
			&self,
			_match_all_topics: &[sp_statement_store::Topic],
			_dest: [u8; 32],
		) -> sp_statement_store::Result<Vec<Vec<u8>>> {
			unimplemented!()
		}

		fn posted_clear_stmt(
			&self,
			_match_all_topics: &[sp_statement_store::Topic],
			_dest: [u8; 32],
		) -> sp_statement_store::Result<Vec<Vec<u8>>> {
			unimplemented!()
		}

		fn submit(
			&self,
			statement: sp_statement_store::Statement,
			_source: sp_statement_store::StatementSource,
		) -> sp_statement_store::SubmitResult {
			let hash = statement.hash();
			self.statements.lock().unwrap().insert(hash, statement.clone());
			self.recent_statements.lock().unwrap().insert(hash, statement);
			SubmitResult::New
		}

		fn remove(&self, _hash: &sp_statement_store::Hash) -> sp_statement_store::Result<()> {
			unimplemented!()
		}

		fn remove_by(&self, _who: [u8; 32]) -> sp_statement_store::Result<()> {
			unimplemented!()
		}
	}

	fn build_handler(
		num_peers: usize,
	) -> (
		StatementHandler<TestNetwork, TestSync>,
		TestStatementStore,
		TestNetwork,
		TestNotificationService,
		async_channel::Receiver<(Statement, oneshot::Sender<SubmitResult>)>,
		Vec<PeerId>,
	) {
		let statement_store = TestStatementStore::new();
		let (queue_sender, queue_receiver) = async_channel::bounded(100);
		let network = TestNetwork::new();
		let notification_service = TestNotificationService::new();
		let mut peers = HashMap::new();
		let mut peer_ids = Vec::with_capacity(num_peers);

		for _ in 0..num_peers {
			let peer_id = PeerId::random();
			peer_ids.push(peer_id);
			peers.insert(
				peer_id,
				Peer {
					statement_rate_limiter: default_statement_limiter(),
					affinity_rate_limiter: PeerRateLimiter::for_affinity_updates(),
					announce_rate_limiter: PeerRateLimiter::for_received_announces(),
					announce_budget: PeerRateLimiter::for_sent_announces(),
					send_budget: default_statement_limiter(),
					protocol_version: PeerProtocolVersion::V1,
					topic_affinity: None,
					is_light: false,
					inbound: true,
					pending_topic_affinity: None,
					full_replay_requested: false,
					sync_watermark: 0,
				},
			);
		}

		let handler = StatementHandler {
			protocol_name: format!("/{STATEMENT_PROTOCOL_V1}").into(),
			notification_service: Box::new(notification_service.clone()),
			propagate_timeout: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = ()> + Send>>)
				.fuse(),
			pending_statements: FuturesUnordered::new(),
			pending_statements_peers: HashMap::new(),
			short_hashes_in_validation: HashSet::new(),
			recently_received_statements: HashMap::new(),
			network: network.clone(),
			sync: TestSync::new(),
			sync_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = sc_network_sync::types::SyncEvent> + Send>>)
				.fuse(),
			network_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = Event> + Send>>)
				.fuse(),
			peers,
			statement_store: Arc::new(statement_store.clone()),
			queue_sender,
			statements_per_second: NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND)
				.expect("DEFAULT_STATEMENTS_PER_SECOND is nonzero"),
			metrics: None,
			initial_sync_timeout: Box::pin(futures::future::pending()),
			pending_affinities_timeout: Box::pin(futures::future::pending()),
			peer_eviction_timeout: Box::pin(futures::future::pending()),
			pending_initial_syncs: HashMap::new(),
			initial_sync_peer_queue: VecDeque::new(),
			next_initial_sync_id: 0,
			initial_sync_in_flight_bytes: 0,
			propagation_outboxes: PropagationOutboxes::new(Some(Arc::new(statement_store.clone()))),
			in_flight_chunks: HashMap::new(),
			next_chunk_id: 0,
			propagation_in_flight_bytes: 0,
			parked_propagations: VecDeque::new(),
			pending_sends: FuturesUnordered::new(),
			deferred_peers: HashSet::new(),
			in_peers_full: DEFAULT_IN_PEERS,
			in_peers_light: 0,
			dropped_statements_during_sync: false,
			sync_recovery_peer: None,
			sync_recovery_readd_timeout: Box::pin(futures::future::pending()),
			dht: DhtOrchestrator::new(
				&[],
				None,
				crate::config::DEFAULT_BLOOM_FALSE_POS_RATE,
				network.local_peer_id(),
				topology_config(20, 3),
				"/statement/test".into(),
				None,
			),
		};
		(handler, statement_store, network, notification_service, queue_receiver, peer_ids)
	}

	fn get_peer_hashes(sent: &[(PeerId, Vec<u8>)], peer: PeerId) -> Vec<sp_statement_store::Hash> {
		sent.iter()
			.filter(|(p, _)| *p == peer)
			.flat_map(|(_, notification)| {
				<Statements as Decode>::decode(&mut notification.as_slice()).unwrap()
			})
			.map(|s| s.hash())
			.collect()
	}

	/// Import one queued statement into the store and feed `result` back into the
	/// handler as the main loop would.
	async fn import_queued_statement(
		handler: &mut StatementHandler<TestNetwork, TestSync>,
		statement_store: &TestStatementStore,
		queue_receiver: &async_channel::Receiver<(Statement, oneshot::Sender<SubmitResult>)>,
		result: SubmitResult,
	) {
		let (statement, completion) = queue_receiver.try_recv().unwrap();
		let hash = statement.hash();
		statement_store.insert(statement.clone());
		statement_store.recent_statements.lock().unwrap().insert(hash, statement);
		completion.send(result).unwrap();
		let (hash, result) = handler.pending_statements.next().await.unwrap();
		handler.on_statement_submit_result(hash, result);
	}

	#[tokio::test]
	async fn statement_is_not_sent_back_to_the_peers_it_came_from() {
		let (
			mut handler,
			statement_store,
			_network,
			notification_service,
			queue_receiver,
			peer_ids,
		) = build_handler(3);
		let (sender_a, sender_b, receiver) = (peer_ids[0], peer_ids[1], peer_ids[2]);

		let mut statement = new_live_statement();
		statement.set_plain_data(b"statement from two peers".to_vec());
		let hash = statement.hash();

		// Both peers supply the statement while it is queued for validation.
		handler.on_statements(sender_a, vec![statement.clone()]);
		handler.on_statements(sender_b, vec![statement.clone()]);
		import_queued_statement(&mut handler, &statement_store, &queue_receiver, SubmitResult::New)
			.await;

		handler.propagate_statements().await;
		handler.flush_pending_sends().await;

		let sent = notification_service.get_sent_notifications();
		assert!(get_peer_hashes(&sent, sender_a).is_empty(), "statement returned to sender_a");
		assert!(get_peer_hashes(&sent, sender_b).is_empty(), "statement returned to sender_b");
		assert_eq!(get_peer_hashes(&sent, receiver), vec![hash]);
		assert!(
			handler.recently_received_statements.is_empty(),
			"recently received statements must be cleared after the propagation pass"
		);

		// Replaying a propagated statement finds no entry to join, so a peer cannot
		// grow the map by resending.
		handler.on_statements(sender_a, vec![statement]);
		assert!(handler.recently_received_statements.is_empty());
	}

	#[tokio::test]
	async fn statement_received_mid_import_is_not_sent_back_to_the_sender() {
		let (
			mut handler,
			statement_store,
			_network,
			notification_service,
			queue_receiver,
			peer_ids,
		) = build_handler(3);
		let (sender_a, sender_b, receiver) = (peer_ids[0], peer_ids[1], peer_ids[2]);

		let mut statement = new_live_statement();
		statement.set_plain_data(b"statement received mid-import".to_vec());
		let hash = statement.hash();

		handler.on_statements(sender_a, vec![statement.clone()]);

		// The worker has inserted the statement into the store, but the event loop
		// has not processed the validation completion yet.
		let (queued, completion) = queue_receiver.try_recv().unwrap();
		statement_store.insert(queued.clone());
		statement_store.recent_statements.lock().unwrap().insert(hash, queued);

		// The second peer sends the same statement inside that window.
		handler.on_statements(sender_b, vec![statement]);
		assert!(handler
			.pending_statements_peers
			.get(&hash)
			.is_some_and(|peers| peers.contains(&sender_b)));

		completion.send(SubmitResult::New).unwrap();
		let (hash, result) = handler.pending_statements.next().await.unwrap();
		handler.on_statement_submit_result(hash, result);

		handler.propagate_statements().await;
		handler.flush_pending_sends().await;

		let sent = notification_service.get_sent_notifications();
		assert!(get_peer_hashes(&sent, sender_a).is_empty(), "statement returned to sender_a");
		assert!(get_peer_hashes(&sent, sender_b).is_empty(), "statement returned to sender_b");
		assert_eq!(get_peer_hashes(&sent, receiver), vec![hash]);
	}

	#[tokio::test]
	async fn statement_forwarded_before_the_tick_is_not_sent_back_to_the_forwarder() {
		let (
			mut handler,
			statement_store,
			_network,
			notification_service,
			queue_receiver,
			peer_ids,
		) = build_handler(3);
		let (sender, forwarder, receiver) = (peer_ids[0], peer_ids[1], peer_ids[2]);

		let mut statement = new_live_statement();
		statement.set_plain_data(b"late forwarder".to_vec());
		let hash = statement.hash();

		handler.on_statements(sender, vec![statement.clone()]);
		// Another path imported the statement while it was queued, so validation
		// completes with `Known`. The peer that sent it must be recorded all the same.
		import_queued_statement(
			&mut handler,
			&statement_store,
			&queue_receiver,
			SubmitResult::Known,
		)
		.await;
		// The statement is imported but not yet propagated when another peer forwards it.
		handler.on_statements(forwarder, vec![statement]);

		handler.propagate_statements().await;
		handler.flush_pending_sends().await;

		let sent = notification_service.get_sent_notifications();
		assert!(
			get_peer_hashes(&sent, sender).is_empty(),
			"statement returned to the peer that sent it"
		);
		assert!(get_peer_hashes(&sent, forwarder).is_empty(), "statement returned to forwarder");
		assert_eq!(get_peer_hashes(&sent, receiver), vec![hash]);
	}

	#[tokio::test]
	async fn recently_received_statements_survive_a_major_sync_early_return() {
		let (
			mut handler,
			statement_store,
			_network,
			notification_service,
			queue_receiver,
			peer_ids,
		) = build_handler(2);
		let (sender, receiver) = (peer_ids[0], peer_ids[1]);

		let mut statement = new_live_statement();
		statement.set_plain_data(b"during major sync".to_vec());
		let hash = statement.hash();

		handler.on_statements(sender, vec![statement]);
		import_queued_statement(&mut handler, &statement_store, &queue_receiver, SubmitResult::New)
			.await;

		// The early return skips the drain, so the statement stays recent and its
		// entry must stay with it.
		handler.sync.major_syncing.store(true, Ordering::Relaxed);
		handler.propagate_statements().await;
		assert!(
			handler.recently_received_statements.contains_key(&hash),
			"entries must survive the major-sync early return"
		);

		handler.sync.major_syncing.store(false, Ordering::Relaxed);
		handler.propagate_statements().await;
		handler.flush_pending_sends().await;

		let sent = notification_service.get_sent_notifications();
		assert!(
			get_peer_hashes(&sent, sender).is_empty(),
			"statement returned to the peer that sent it"
		);
		assert_eq!(get_peer_hashes(&sent, receiver), vec![hash]);
		assert!(handler.recently_received_statements.is_empty());
	}

	#[tokio::test]
	async fn propagation_does_not_wait_for_pending_send() {
		let (mut handler, statement_store, _, notification_service, _, _) = build_handler(1);
		let mut statement = new_live_statement();
		statement.set_plain_data(b"statement".to_vec());
		statement_store
			.recent_statements
			.lock()
			.unwrap()
			.insert(statement.hash(), statement);

		notification_service.block_sends();
		let result =
			tokio::time::timeout(Duration::from_secs(1), handler.propagate_statements()).await;

		assert!(result.is_ok(), "Propagation waited for a pending send");
		assert_eq!(handler.pending_sends.len(), 1);
	}

	#[tokio::test]
	async fn slow_peer_keeps_one_propagation_chunk_in_flight() {
		let (mut handler, statement_store, _network, notification_service, _, peer_ids) =
			build_handler(1);
		let peer_id = peer_ids[0];

		// 100 KB each, so the tick spans several 1 MiB chunks.
		for i in 0..25u8 {
			let mut statement = new_live_statement();
			let mut data = vec![0u8; 100 * 1024];
			data[0] = i;
			statement.set_plain_data(data);
			statement_store
				.recent_statements
				.lock()
				.unwrap()
				.insert(statement.hash(), statement);
		}

		// The peer never reads its substream.
		notification_service.block_sends();
		handler.propagate_statements().await;

		assert_eq!(handler.pending_sends.len(), 1, "only one chunk may be in flight");
		let backlog = handler.propagation_outboxes.get(&peer_id).unwrap().len();
		assert!(backlog > 0, "the remaining hashes stay in the outbox");

		// Another tick accumulates into the same outbox while the slot is busy.
		let mut statement = new_live_statement();
		statement.set_plain_data(b"second tick".to_vec());
		statement_store
			.recent_statements
			.lock()
			.unwrap()
			.insert(statement.hash(), statement);
		handler.propagate_statements().await;

		assert_eq!(handler.pending_sends.len(), 1);
		assert_eq!(handler.propagation_outboxes.get(&peer_id).unwrap().len(), backlog + 1);
	}

	#[tokio::test]
	async fn statement_pruned_between_tick_and_send_is_skipped() {
		let (mut handler, statement_store, _network, notification_service, _, peer_ids) =
			build_handler(1);
		let peer_id = peer_ids[0];

		let mut kept = new_live_statement();
		kept.set_plain_data(b"kept".to_vec());
		let kept_hash = kept.hash();
		statement_store.insert(kept);

		let mut pruned = new_live_statement();
		pruned.set_plain_data(b"pruned".to_vec());
		let pruned_hash = pruned.hash();

		// The pruned statement's hash is queued but the statement left the store.
		handler.propagation_outboxes.push(peer_id, vec![pruned_hash, kept_hash]);
		handler.try_send_next_chunk(peer_id);
		handler.flush_pending_sends().await;

		let sent = get_peer_hashes(&notification_service.get_sent_notifications(), peer_id);
		assert_eq!(sent, vec![kept_hash]);
		assert!(
			handler.propagation_outboxes.get(&peer_id).is_none(),
			"the drained outbox must be removed"
		);
	}

	#[tokio::test]
	async fn failed_store_fetch_parks_the_peer_and_the_tick_retries() {
		let (mut handler, statement_store, _network, notification_service, _, peer_ids) =
			build_handler(1);
		let peer_id = peer_ids[0];

		let mut statement = new_live_statement();
		statement.set_plain_data(b"statement".to_vec());
		let hash = statement.hash();
		statement_store.statements.lock().unwrap().insert(hash, statement);
		handler.propagation_outboxes.push(peer_id, vec![hash]);

		statement_store.fail_fetches.store(true, Ordering::Relaxed);
		handler.try_send_next_chunk(peer_id);

		assert!(handler.pending_sends.is_empty(), "a failed fetch must not queue a send");
		assert_eq!(
			handler.propagation_outboxes.get(&peer_id).unwrap().len(),
			1,
			"the outbox must be retained"
		);
		assert_eq!(handler.parked_propagations, VecDeque::from([peer_id]));

		// No new statements arrive for the peer: the tick alone must retry the parked fetch.
		statement_store.fail_fetches.store(false, Ordering::Relaxed);
		handler.propagate_statements().await;
		handler.flush_pending_sends().await;

		let sent = get_peer_hashes(&notification_service.get_sent_notifications(), peer_id);
		assert_eq!(sent, vec![hash]);
		assert!(handler.propagation_outboxes.get(&peer_id).is_none());
		assert!(handler.parked_propagations.is_empty());
	}

	#[tokio::test]
	async fn oversized_statement_in_the_outbox_is_consumed() {
		let (mut handler, statement_store, _network, notification_service, _, peer_ids) =
			build_handler(1);
		handler.metrics = Some(Metrics::register(&Registry::new()).unwrap());
		let peer_id = peer_ids[0];

		let mut oversized = new_live_statement();
		oversized.set_plain_data(vec![1u8; MAX_STATEMENT_NOTIFICATION_SIZE as usize]);
		let oversized_hash = oversized.hash();
		let mut small = new_live_statement();
		small.set_plain_data(b"small".to_vec());
		let small_hash = small.hash();
		statement_store.insert(oversized);
		statement_store.insert(small);

		// The oversized statement heads the outbox. It must be consumed, not
		// re-fetched forever, and the statement behind it must still go out.
		handler.propagation_outboxes.push(peer_id, vec![oversized_hash, small_hash]);
		handler.try_send_next_chunk(peer_id);
		handler.flush_pending_sends().await;

		let sent = get_peer_hashes(&notification_service.get_sent_notifications(), peer_id);
		assert_eq!(sent, vec![small_hash]);
		assert_eq!(handler.metrics.as_ref().unwrap().skipped_oversized_statements.get(), 1);
	}

	#[tokio::test]
	async fn disconnect_clears_the_outbox_and_send_slot() {
		let (mut handler, statement_store, _network, notification_service, _, peer_ids) =
			build_handler(1);
		let peer_id = peer_ids[0];

		// Several chunks worth of statements with a blocked substream, so the slot
		// is taken and a backlog stays queued.
		for i in 0..25u8 {
			let mut statement = new_live_statement();
			let mut data = vec![0u8; 100 * 1024];
			data[0] = i;
			statement.set_plain_data(data);
			statement_store
				.recent_statements
				.lock()
				.unwrap()
				.insert(statement.hash(), statement);
		}
		notification_service.block_sends();
		handler.metrics = Some(Metrics::register(&Registry::new()).unwrap());
		handler.propagate_statements().await;
		let queued = handler.propagation_outboxes.get(&peer_id).map_or(0, |outbox| outbox.len());
		assert!(queued > 0);
		assert!(handler.in_flight_chunks.contains_key(&peer_id));
		assert_eq!(statement_store.held.lock().unwrap().values().sum::<usize>(), queued);

		handler
			.handle_notification_event(NotificationEvent::NotificationStreamClosed {
				peer: peer_id,
			})
			.await;

		assert!(handler.propagation_outboxes.get(&peer_id).is_none());
		assert!(statement_store.held.lock().unwrap().is_empty());
		assert!(!handler.in_flight_chunks.contains_key(&peer_id));
		let metrics = handler.metrics.as_ref().unwrap();
		assert_eq!(
			metrics
				.undelivered_statements
				.with_label_values(&[send_failure::DISCONNECTED])
				.get(),
			queued as u64,
			"every hash still queued at disconnect counts as undelivered"
		);
		assert_eq!(
			metrics.send_failures.with_label_values(&[send_failure::DISCONNECTED]).get(),
			1,
			"one disconnect event"
		);
	}

	#[tokio::test]
	async fn disconnect_with_an_empty_outbox_records_no_loss() {
		let (mut handler, _store, _network, _notification_service, _, peer_ids) = build_handler(1);
		handler.metrics = Some(Metrics::register(&Registry::new()).unwrap());

		handler
			.handle_notification_event(NotificationEvent::NotificationStreamClosed {
				peer: peer_ids[0],
			})
			.await;

		let metrics = handler.metrics.as_ref().unwrap();
		assert_eq!(
			metrics
				.undelivered_statements
				.with_label_values(&[send_failure::DISCONNECTED])
				.get(),
			0
		);
		assert_eq!(metrics.send_failures.with_label_values(&[send_failure::DISCONNECTED]).get(), 0);
	}

	#[tokio::test]
	async fn drain_counts_queued_statements_the_store_no_longer_serves() {
		let (mut handler, statement_store, _network, notification_service, _, peer_ids) =
			build_handler(1);
		handler.metrics = Some(Metrics::register(&Registry::new()).unwrap());
		let peer_id = peer_ids[0];

		let mut gone = new_live_statement();
		gone.set_plain_data(b"gone".to_vec());
		let gone_hash = gone.hash();
		let mut expired = Statement::new();
		expired.set_plain_data(b"expired".to_vec());
		let expired_hash = expired.hash();
		let mut live = new_live_statement();
		live.set_plain_data(b"live".to_vec());
		let live_hash = live.hash();
		statement_store.insert(expired);
		statement_store.insert(live);

		handler
			.propagation_outboxes
			.push(peer_id, vec![gone_hash, expired_hash, live_hash]);
		handler.try_send_next_chunk(peer_id);
		handler.flush_pending_sends().await;

		let sent = get_peer_hashes(&notification_service.get_sent_notifications(), peer_id);
		assert_eq!(sent, vec![live_hash]);
		let metrics = handler.metrics.as_ref().unwrap();
		assert_eq!(
			metrics
				.undelivered_statements
				.with_label_values(&[send_failure::MISSING_FROM_STORE])
				.get(),
			2,
			"the hash the store never held and the expired one are lost"
		);
	}

	#[tokio::test]
	async fn failed_propagation_send_frees_the_slot_and_the_backlog_keeps_draining() {
		let (mut handler, statement_store, _network, notification_service, _, peer_ids) =
			build_handler(1);
		let peer_id = peer_ids[0];

		// 700 KiB each, so a 1 MiB chunk carries exactly one statement.
		let mut first = new_live_statement();
		first.set_plain_data(vec![1u8; 700 * 1024]);
		let first_hash = first.hash();
		let mut second = new_live_statement();
		second.set_plain_data(vec![2u8; 700 * 1024]);
		let second_hash = second.hash();
		statement_store.insert(first);
		statement_store.insert(second);
		handler.propagation_outboxes.push(peer_id, vec![first_hash, second_hash]);

		// Only the first chunk's send fails.
		notification_service.fail_sends();
		handler.try_send_next_chunk(peer_id);
		assert!(handler.in_flight_chunks.contains_key(&peer_id));
		let result = handler.pending_sends.next().await.unwrap();
		notification_service.allow_sends();
		handler.handle_send_result(result);

		// The failure freed the slot and the backlog kept draining: the second
		// statement went out, the failed one was not retried.
		handler.flush_pending_sends().await;
		let sent = get_peer_hashes(&notification_service.get_sent_notifications(), peer_id);
		assert_eq!(sent, vec![second_hash]);
		assert!(handler.propagation_outboxes.get(&peer_id).is_none());
		assert!(!handler.in_flight_chunks.contains_key(&peer_id));
	}

	#[tokio::test]
	async fn overflowing_outbox_drops_the_oldest_hashes() {
		let (mut handler, statement_store, _network, _notification_service, _, peer_ids) =
			build_handler(1);
		handler.metrics = Some(Metrics::register(&Registry::new()).unwrap());
		let peer_id = peer_ids[0];

		let mut old = new_live_statement();
		old.set_plain_data(b"oldest".to_vec());
		let old_hash = old.hash();

		// Three fresh statements arrive by tick while the outbox is full and the
		// peer's slot is busy.
		let fresh_hashes: HashSet<_> = (0..3u8)
			.map(|i| {
				let mut fresh = new_live_statement();
				fresh.set_plain_data(vec![i; 8]);
				let hash = fresh.hash();
				statement_store.recent_statements.lock().unwrap().insert(hash, fresh);
				hash
			})
			.collect();
		handler.in_flight_chunks.insert(peer_id, 0);
		handler
			.propagation_outboxes
			.push(peer_id, vec![old_hash; MAX_PROPAGATION_OUTBOX_LEN]);

		handler.propagate_statements().await;

		let outbox = handler.propagation_outboxes.get(&peer_id).unwrap();
		assert_eq!(outbox.len(), MAX_PROPAGATION_OUTBOX_LEN);
		let tail: HashSet<_> =
			outbox.iter().skip(MAX_PROPAGATION_OUTBOX_LEN - 3).copied().collect();
		assert_eq!(tail, fresh_hashes, "the freshest hashes must survive the overflow");
		let held = statement_store.held.lock().unwrap();
		assert_eq!(held[&old_hash], MAX_PROPAGATION_OUTBOX_LEN - 3);
		assert!(fresh_hashes.iter().all(|hash| held[hash] == 1));
		drop(held);

		let metrics = handler.metrics.as_ref().unwrap();
		assert_eq!(
			metrics
				.undelivered_statements
				.with_label_values(&[send_failure::OUTBOX_FULL])
				.get(),
			3,
			"each dropped hash counts as undelivered"
		);
		assert_eq!(
			metrics.send_failures.with_label_values(&[send_failure::OUTBOX_FULL]).get(),
			1,
			"one overflow event"
		);
	}

	#[tokio::test]
	async fn statement_received_while_queued_in_the_outbox_is_not_echoed() {
		let (mut handler, statement_store, _network, notification_service, _, peer_ids) =
			build_handler(1);
		let peer_id = peer_ids[0];

		let mut statement = new_live_statement();
		statement.set_plain_data(b"received after append".to_vec());
		let hash = statement.hash();
		statement_store.insert(statement);

		// The hash was appended while the peer's slot was busy, and the peer sent
		// us the statement before the slot freed: the encode-time senders check
		// must catch what the append-time check could not have seen.
		handler.propagation_outboxes.push(peer_id, vec![hash]);
		handler.recently_received_statements.insert(hash, HashSet::from_iter([peer_id]));

		handler.try_send_next_chunk(peer_id);
		handler.flush_pending_sends().await;

		assert!(
			get_peer_hashes(&notification_service.get_sent_notifications(), peer_id).is_empty(),
			"statement returned to the peer that sent it"
		);
	}

	/// Simulate the network closing the substream for every disconnected
	/// peer, so the handler runs its per-peer cleanup.
	async fn dispatch_disconnects(
		handler: &mut StatementHandler<TestNetwork, TestSync>,
		network: &TestNetwork,
	) {
		for peer in network.get_disconnected_peers() {
			handler
				.handle_notification_event(NotificationEvent::NotificationStreamClosed { peer })
				.await;
		}
	}

	#[tokio::test]
	async fn test_skips_processing_statements_that_already_in_store() {
		let (mut handler, statement_store, _network, _notification_service, queue_receiver, _) =
			build_handler(1);

		let mut statement1 = new_live_statement();
		statement1.set_plain_data(b"statement1".to_vec());

		statement_store.insert(statement1.clone());

		let mut statement2 = new_live_statement();
		statement2.set_plain_data(b"statement2".to_vec());
		let hash2 = statement2.hash();

		let peer_id = *handler.peers.keys().next().unwrap();

		handler.on_statements(peer_id, vec![statement1, statement2]);

		let to_submit = queue_receiver.try_recv();
		assert_eq!(to_submit.unwrap().0.hash(), hash2, "Expected only statement2 to be queued");

		let no_more = queue_receiver.try_recv();
		assert!(no_more.is_err(), "Expected only one statement to be queued");
	}

	#[tokio::test]
	async fn test_reports_for_duplicate_statements() {
		let (mut handler, statement_store, network, _notification_service, queue_receiver, _) =
			build_handler(1);

		let peer_id = *handler.peers.keys().next().unwrap();

		let mut statement1 = new_live_statement();
		statement1.set_plain_data(b"statement1".to_vec());

		handler.on_statements(peer_id, vec![statement1.clone()]);
		{
			// Manually process statements submission
			let (s, _) = queue_receiver.try_recv().unwrap();
			statement_store.insert(s);
			handler.network.report_peer(peer_id, rep::ANY_STATEMENT_REFUND);
		}

		handler.on_statements(peer_id, vec![statement1]);

		let reports = network.get_reports();
		assert_eq!(
			reports,
			vec![
				(peer_id, rep::ANY_STATEMENT),        // Report for first statement
				(peer_id, rep::ANY_STATEMENT_REFUND), // Refund for first statement
				(peer_id, rep::DUPLICATE_STATEMENT)   // Report for duplicate statement
			],
			"Expected ANY_STATEMENT, ANY_STATEMENT_REFUND, DUPLICATE_STATEMENT reputation change, but got: {:?}",
			reports
		);
	}

	#[tokio::test]
	async fn targets_charge_the_peer_send_budget() {
		let (mut handler, statement_store, _network, _notification_service, _, peer_ids) =
			build_handler(1);
		let peer_id = peer_ids[0];
		let clock = FakeRelativeClock::default();
		handler.peers.get_mut(&peer_id).expect("peer is connected").send_budget =
			PeerRateLimiter::with_clock(
				NonZeroU32::new(1).expect("nonzero"),
				NonZeroU32::new(10).expect("nonzero"),
				&clock,
			);
		let statements: Vec<_> = (0..10u8)
			.map(|seed| {
				let mut statement = new_live_statement();
				statement.set_plain_data(vec![seed]);
				statement_store.insert(statement.clone());
				(seed as u64, statement.hash(), statement)
			})
			.collect();
		let indices: Vec<usize> = (0..statements.len()).collect();

		handler.queue_statements_for_targets(&statements, vec![(peer_id, indices)]);
		handler.flush_pending_sends().await;
		assert!(handler.peers[&peer_id].send_budget.is_flooding(1), "the chunk drained the burst");
	}

	#[tokio::test]
	async fn targets_ignore_a_disconnected_peer() {
		let (mut handler, statement_store, _network, notification_service, _, _) = build_handler(0);

		let mut statement = Statement::new();
		statement.set_plain_data(b"orphan".to_vec());
		let hash = statement.hash();
		statement_store.insert(statement.clone());
		let statements = vec![(0, hash, statement)];

		let disconnected_peer = PeerId::random();
		handler.queue_statements_for_targets(&statements, vec![(disconnected_peer, vec![0])]);
		handler.flush_pending_sends().await;
		assert!(notification_service.get_sent_notifications().is_empty());
		assert!(handler.propagation_outboxes.get(&disconnected_peer).is_none());
	}

	#[tokio::test]
	async fn targets_receive_the_indexed_statements_except_those_from_the_peer() {
		let (mut handler, statement_store, _network, notification_service, _, _) = build_handler(0);

		let make = |seed: u8| {
			let mut statement = new_live_statement();
			statement.set_plain_data(vec![seed]);
			statement_store.insert(statement.clone());
			(seed as u64, statement.hash(), statement)
		};
		let statements = vec![make(1), make(2), make(3)];

		let peer_id = PeerId::random();
		let peer = Peer::new_for_testing(
			NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND).expect("nonzero"),
			NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND * config::STATEMENTS_BURST_COEFFICIENT)
				.expect("nonzero"),
		);
		// The peer supplied the statement at index 1.
		handler
			.recently_received_statements
			.entry(statements[1].1)
			.or_default()
			.insert(peer_id);
		handler.peers.insert(peer_id, peer);

		// The orchestrator names indices 0 and 1: index 1 drops as received from the peer,
		// index 2 is never offered.
		handler.queue_statements_for_targets(&statements, vec![(peer_id, vec![0, 1])]);
		handler.flush_pending_sends().await;
		assert_eq!(
			get_peer_hashes(&notification_service.get_sent_notifications(), peer_id),
			vec![statements[0].1]
		);
	}

	#[tokio::test]
	async fn targets_are_queued_past_the_affinity_filter_and_the_sync_watermark() {
		let (mut handler, statement_store, _network, _notification_service, _, peer_ids) =
			build_handler(1);
		let peer_id = peer_ids[0];

		let mut statement = new_live_statement();
		statement.set_plain_data(b"targeted".to_vec());
		statement.set_topic(0, Topic([7u8; 32]));
		let hash = statement.hash();
		statement_store.insert(statement.clone());
		let statements = vec![(0, hash, statement)];

		// The peer's filter matches no topic and its watermark sits above the statement. The
		// orchestrator chose the peer regardless, and the initial sync would skip the statement.
		let peer = handler.peers.get_mut(&peer_id).unwrap();
		peer.topic_affinity = Some(AffinityFilter::new(BLOOM_SEED, 0.01, 10));
		peer.sync_watermark = 10;
		handler.in_flight_chunks.insert(peer_id, 0);

		handler.queue_statements_for_targets(&statements, vec![(peer_id, vec![0])]);
		assert_eq!(
			handler.propagation_outboxes.get(&peer_id).unwrap(),
			&VecDeque::from(vec![hash])
		);
	}

	#[tokio::test]
	async fn targets_skip_a_peer_that_cannot_receive_yet() {
		let (mut handler, statement_store, _network, _notification_service, _, peer_ids) =
			build_handler(1);
		let peer_id = peer_ids[0];

		let mut statement = new_live_statement();
		statement.set_plain_data(b"targeted".to_vec());
		let hash = statement.hash();
		statement_store.insert(statement.clone());
		let statements = vec![(0, hash, statement)];

		// A light V2 peer owes its affinity filter before it may receive anything.
		let peer = handler.peers.get_mut(&peer_id).unwrap();
		peer.protocol_version = PeerProtocolVersion::V2;
		peer.is_light = true;

		handler.queue_statements_for_targets(&statements, vec![(peer_id, vec![0])]);
		assert!(handler.propagation_outboxes.get(&peer_id).is_none());
	}

	#[tokio::test]
	async fn dead_end_statement_stays_held_until_a_routing_target_syncs_it() {
		let (mut handler, statement_store, _network, _notification_service, _, peer_ids) =
			build_handler(1);
		let target = peer_ids[0];
		// With one replica per topic, the target is the replica of the topic at its own key.
		handler.dht = DhtOrchestrator::new(
			&[],
			None,
			crate::config::DEFAULT_BLOOM_FALSE_POS_RATE,
			PeerId::random(),
			topology_config(1, 1),
			"/statement/test".into(),
			None,
		);
		handler.dht.on_peer_identified(target, true);

		let mut statement = new_live_statement();
		statement.set_topic(0, Topic(sp_crypto_hashing::blake2_256(&target.to_bytes())));
		let hash = statement.hash();
		statement_store.insert(statement.clone());
		let mut late = statement.clone();
		late.set_plain_data(b"admitted after the sync started".to_vec());
		let late_hash = late.hash();
		statement_store.insert(late.clone());
		let statements = vec![(5, hash, statement), (6, late_hash, late)];
		let held = || statement_store.held.lock().unwrap().get(&hash).copied();

		let plan = handler.dht.propagation_plan(&statements, |_| true);
		assert_eq!(plan.dead_ends, vec![0, 1]);
		handler.hold_dead_ends(&statements, plan.dead_ends);
		handler.resolve_dead_ends();
		assert_eq!(held(), Some(1), "no routing target is connected");

		handler.dht.on_substream_opened(target);
		// A busy send slot keeps the queued hash in the outbox.
		handler.in_flight_chunks.insert(target, 0);
		handler.peers.get_mut(&target).unwrap().sync_watermark = 6;
		handler.pending_initial_syncs.insert(
			target,
			PendingInitialSync {
				cursor: 0,
				watermark: 6,
				started_at: Instant::now(),
				sync_id: 0,
				already_served: None,
				announce: None,
			},
		);
		handler.resolve_dead_ends();
		assert_eq!(held(), Some(1), "the initial sync is still running");
		assert_eq!(
			handler.propagation_outboxes.get(&target).unwrap(),
			&VecDeque::from([late_hash]),
			"a sync up to the statement's sequence leaves it out, so the target gets it directly"
		);

		handler.pending_initial_syncs.remove(&target);
		handler.resolve_dead_ends();
		assert_eq!(held(), None);
		assert_eq!(
			statement_store.held.lock().unwrap().get(&late_hash),
			Some(&1),
			"the target's outbox keeps its hold"
		);
		assert!(handler.propagation_outboxes.dead_ends().is_empty());
	}

	#[tokio::test]
	async fn dead_end_statement_is_released_on_expiry_or_once_the_node_is_its_replica() {
		let (mut handler, statement_store, _network, _notification_service, _, _) =
			build_handler(0);
		// A lone node with one discovered peer is no replica of the topic at that peer's key,
		// and the replica of the topic at its own key.
		let remote = PeerId::random();
		let local = PeerId::random();
		handler.dht = DhtOrchestrator::new(
			&[],
			None,
			crate::config::DEFAULT_BLOOM_FALSE_POS_RATE,
			local,
			topology_config(1, 1),
			"/statement/test".into(),
			None,
		);
		handler.dht.on_peer_identified(remote, true);

		let mut statement = new_live_statement();
		statement.set_topic(0, Topic(sp_crypto_hashing::blake2_256(&remote.to_bytes())));
		let mut expired = statement.clone();
		expired.set_expiry_from_parts(1, 0);
		let mut replicated = statement.clone();
		replicated.set_topic(0, Topic(sp_crypto_hashing::blake2_256(&local.to_bytes())));
		let statements = vec![
			(0, statement.hash(), statement),
			(1, expired.hash(), expired),
			(2, replicated.hash(), replicated),
		];

		handler.hold_dead_ends(&statements, vec![0, 1, 2]);
		handler.resolve_dead_ends();

		let held = statement_store.held.lock().unwrap();
		assert_eq!(held.get(&statements[0].1), Some(&1), "a live dead end waits for a peer");
		assert_eq!(held.get(&statements[1].1), None);
		assert_eq!(held.get(&statements[2].1), None);
	}

	#[tokio::test]
	async fn dead_end_statement_takes_a_detour_past_its_sender_and_a_light_peer() {
		let (mut handler, statement_store, _network, _notification_service, _, peer_ids) =
			build_handler(2);
		let key = |peer: &PeerId| sp_crypto_hashing::blake2_256(&peer.to_bytes());
		// With one replica per topic, the unreachable peer is the replica of the topic at its
		// own key. The local node is closer to the topic than both connected peers, so neither
		// is a routing target.
		let unreachable = PeerId::random();
		let topic = key(&unreachable);
		let distance = |peer: &PeerId| -> Vec<u8> {
			key(peer).iter().zip(topic).map(|(a, b)| a ^ b).collect()
		};
		let local = iter::repeat_with(PeerId::random)
			.find(|local| peer_ids.iter().all(|peer| distance(local) < distance(peer)))
			.expect("a random peer is closer to the topic than two others soon enough");
		handler.dht = DhtOrchestrator::new(
			&[],
			None,
			crate::config::DEFAULT_BLOOM_FALSE_POS_RATE,
			local,
			topology_config(1, 1),
			"/statement/test".into(),
			None,
		);
		handler.dht.on_peer_identified(unreachable, true);
		for peer in &peer_ids {
			handler.dht.on_substream_opened(*peer);
			// A busy send slot keeps the queued hash in the outbox.
			handler.in_flight_chunks.insert(*peer, 0);
		}
		let farthest = handler.dht.peers_farthest_first()[0];
		let other = *peer_ids.iter().find(|peer| **peer != farthest).unwrap();

		let statements: Vec<_> = [b"from the farthest peer", b"submitted on this node"]
			.into_iter()
			.enumerate()
			.map(|(seq, data)| {
				let mut statement = new_live_statement();
				statement.set_plain_data(data.to_vec());
				statement.set_topic(0, Topic(topic));
				(seq as u64, statement.hash(), statement)
			})
			.collect();
		let (received, submitted) = (statements[0].1, statements[1].1);
		handler
			.recently_received_statements
			.entry(received)
			.or_default()
			.insert(farthest);

		let plan = handler.dht.propagation_plan(&statements, |_| true);
		assert_eq!(plan.dead_ends, vec![0, 1]);
		handler.hold_dead_ends(&statements, plan.dead_ends);
		handler.resolve_dead_ends();

		assert_eq!(handler.propagation_outboxes.get(&other).unwrap(), &VecDeque::from([received]));
		assert_eq!(
			handler.propagation_outboxes.get(&farthest).unwrap(),
			&VecDeque::from([submitted])
		);

		// A light peer forwards nothing, so the detour passes it by.
		handler.peers.get_mut(&farthest).unwrap().is_light = true;
		let mut statement = new_live_statement();
		statement.set_plain_data(b"past a light peer".to_vec());
		statement.set_topic(0, Topic(topic));
		let past_light = statement.hash();
		handler.hold_dead_ends(&[(2, past_light, statement)], vec![0]);
		handler.resolve_dead_ends();

		assert_eq!(
			handler.propagation_outboxes.get(&other).unwrap(),
			&VecDeque::from([received, past_light])
		);
		assert!(handler.propagation_outboxes.dead_ends().is_empty());
		let held = statement_store.held.lock().unwrap();
		assert_eq!(
			(held[&received], held[&submitted], held[&past_light]),
			(1, 1, 1),
			"only the outboxes hold them"
		);
	}

	#[tokio::test]
	async fn test_splits_large_batches_into_smaller_chunks() {
		let (mut handler, statement_store, _network, notification_service, _queue_receiver, _) =
			build_handler(1);

		let num_statements = 30;
		let statement_size = 100 * 1024; // 100KB per statement
		for i in 0..num_statements {
			let mut statement = new_live_statement();
			let mut data = vec![0u8; statement_size];
			data[0] = i as u8;
			statement.set_plain_data(data);
			let hash = statement.hash();
			statement_store.recent_statements.lock().unwrap().insert(hash, statement);
		}

		handler.propagate_statements().await;
		handler.flush_pending_sends().await;

		let sent = notification_service.get_sent_notifications();
		let mut total_statements_sent = 0;
		assert!(
			sent.len() == 3,
			"Expected batch to be split into 3 chunks, but got {} chunks",
			sent.len()
		);
		for (_peer, notification) in sent.iter() {
			assert!(
				notification.len() <= MAX_STATEMENT_NOTIFICATION_SIZE as usize,
				"Notification size {} exceeds limit {}",
				notification.len(),
				MAX_STATEMENT_NOTIFICATION_SIZE
			);
			if let Ok(stmts) = <Statements as Decode>::decode(&mut notification.as_slice()) {
				total_statements_sent += stmts.len();
			}
		}

		assert_eq!(
			total_statements_sent, num_statements,
			"Expected all {} statements to be sent, but only {} were sent",
			num_statements, total_statements_sent
		);
	}

	#[tokio::test]
	async fn test_skips_only_oversized_statements() {
		let (mut handler, statement_store, _network, notification_service, _queue_receiver, _) =
			build_handler(1);

		let mut statement1 = new_live_statement();
		statement1.set_plain_data(vec![1u8; 100]);
		let hash1 = statement1.hash();
		statement_store
			.recent_statements
			.lock()
			.unwrap()
			.insert(hash1, statement1.clone());

		let mut oversized1 = new_live_statement();
		oversized1.set_plain_data(vec![2u8; MAX_STATEMENT_NOTIFICATION_SIZE as usize * 100]);
		let hash_oversized1 = oversized1.hash();
		statement_store
			.recent_statements
			.lock()
			.unwrap()
			.insert(hash_oversized1, oversized1);

		let mut statement2 = new_live_statement();
		statement2.set_plain_data(vec![3u8; 100]);
		let hash2 = statement2.hash();
		statement_store
			.recent_statements
			.lock()
			.unwrap()
			.insert(hash2, statement2.clone());

		let mut oversized2 = new_live_statement();
		oversized2.set_plain_data(vec![4u8; MAX_STATEMENT_NOTIFICATION_SIZE as usize]);
		let hash_oversized2 = oversized2.hash();
		statement_store
			.recent_statements
			.lock()
			.unwrap()
			.insert(hash_oversized2, oversized2);

		let mut statement3 = new_live_statement();
		statement3.set_plain_data(vec![5u8; 100]);
		let hash3 = statement3.hash();
		statement_store
			.recent_statements
			.lock()
			.unwrap()
			.insert(hash3, statement3.clone());

		handler.propagate_statements().await;
		handler.flush_pending_sends().await;

		let sent = notification_service.get_sent_notifications();

		let mut sent_hashes = sent
			.iter()
			.flat_map(|(_peer, notification)| {
				<Statements as Decode>::decode(&mut notification.as_slice()).unwrap()
			})
			.map(|s| s.hash())
			.collect::<Vec<_>>();
		sent_hashes.sort();
		let mut expected_hashes = vec![hash1, hash2, hash3];
		expected_hashes.sort();
		assert_eq!(sent_hashes, expected_hashes, "Only small statements should be sent");
	}

	fn build_handler_no_peers() -> (
		StatementHandler<TestNetwork, TestSync>,
		TestStatementStore,
		TestNetwork,
		TestNotificationService,
	) {
		let statement_store = TestStatementStore::new();
		let (queue_sender, _queue_receiver) = async_channel::bounded(2);
		let network = TestNetwork::new();
		let notification_service = TestNotificationService::new();

		let handler = StatementHandler {
			protocol_name: format!("/{STATEMENT_PROTOCOL_V1}").into(),
			notification_service: Box::new(notification_service.clone()),
			propagate_timeout: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = ()> + Send>>)
				.fuse(),
			pending_statements: FuturesUnordered::new(),
			pending_statements_peers: HashMap::new(),
			short_hashes_in_validation: HashSet::new(),
			recently_received_statements: HashMap::new(),
			network: network.clone(),
			sync: TestSync::new(),
			sync_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = sc_network_sync::types::SyncEvent> + Send>>)
				.fuse(),
			network_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = Event> + Send>>)
				.fuse(),
			peers: HashMap::new(),
			statement_store: Arc::new(statement_store.clone()),
			queue_sender,
			statements_per_second: NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND)
				.expect("DEFAULT_STATEMENTS_PER_SECOND is nonzero"),
			metrics: None,
			initial_sync_timeout: Box::pin(futures::future::pending()),
			pending_affinities_timeout: Box::pin(futures::future::pending()),
			peer_eviction_timeout: Box::pin(futures::future::pending()),
			pending_initial_syncs: HashMap::new(),
			initial_sync_peer_queue: VecDeque::new(),
			next_initial_sync_id: 0,
			initial_sync_in_flight_bytes: 0,
			propagation_outboxes: PropagationOutboxes::new(None),
			in_flight_chunks: HashMap::new(),
			next_chunk_id: 0,
			propagation_in_flight_bytes: 0,
			parked_propagations: VecDeque::new(),
			pending_sends: FuturesUnordered::new(),
			deferred_peers: HashSet::new(),
			in_peers_full: DEFAULT_IN_PEERS,
			in_peers_light: 0,
			dropped_statements_during_sync: false,
			sync_recovery_peer: None,
			sync_recovery_readd_timeout: Box::pin(futures::future::pending()),
			dht: DhtOrchestrator::new(
				&[],
				None,
				crate::config::DEFAULT_BLOOM_FALSE_POS_RATE,
				network.local_peer_id(),
				topology_config(20, 3),
				"/statement/test".into(),
				None,
			),
		};
		(handler, statement_store, network, notification_service)
	}

	/// Like `build_handler_no_peers` but the network mock returns `Light` for peer roles.
	fn build_handler_no_peers_light() -> (
		StatementHandler<TestNetwork, TestSync>,
		TestStatementStore,
		TestNetwork,
		TestNotificationService,
	) {
		let statement_store = TestStatementStore::new();
		let (queue_sender, _queue_receiver) = async_channel::bounded(2);
		let network = TestNetwork::new_light();
		let notification_service = TestNotificationService::new();

		let handler = StatementHandler {
			protocol_name: format!("/{STATEMENT_PROTOCOL_V1}").into(),
			notification_service: Box::new(notification_service.clone()),
			propagate_timeout: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = ()> + Send>>)
				.fuse(),
			pending_statements: FuturesUnordered::new(),
			pending_statements_peers: HashMap::new(),
			short_hashes_in_validation: HashSet::new(),
			recently_received_statements: HashMap::new(),
			network: network.clone(),
			sync: TestSync::new(),
			sync_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = sc_network_sync::types::SyncEvent> + Send>>)
				.fuse(),
			network_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = Event> + Send>>)
				.fuse(),
			peers: HashMap::new(),
			statement_store: Arc::new(statement_store.clone()),
			queue_sender,
			statements_per_second: NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND)
				.expect("DEFAULT_STATEMENTS_PER_SECOND is nonzero"),
			metrics: None,
			initial_sync_timeout: Box::pin(futures::future::pending()),
			pending_affinities_timeout: Box::pin(futures::future::pending()),
			peer_eviction_timeout: Box::pin(futures::future::pending()),
			pending_initial_syncs: HashMap::new(),
			initial_sync_peer_queue: VecDeque::new(),
			next_initial_sync_id: 0,
			initial_sync_in_flight_bytes: 0,
			propagation_outboxes: PropagationOutboxes::new(None),
			in_flight_chunks: HashMap::new(),
			next_chunk_id: 0,
			propagation_in_flight_bytes: 0,
			parked_propagations: VecDeque::new(),
			pending_sends: FuturesUnordered::new(),
			deferred_peers: HashSet::new(),
			in_peers_full: DEFAULT_IN_PEERS,
			in_peers_light: 0,
			dropped_statements_during_sync: false,
			sync_recovery_peer: None,
			sync_recovery_readd_timeout: Box::pin(futures::future::pending()),
			dht: DhtOrchestrator::new(
				&[],
				None,
				crate::config::DEFAULT_BLOOM_FALSE_POS_RATE,
				network.local_peer_id(),
				topology_config(20, 3),
				"/statement/test".into(),
				None,
			),
		};
		(handler, statement_store, network, notification_service)
	}

	#[tokio::test]
	async fn test_initial_sync_burst_single_peer() {
		let (mut handler, statement_store, _network, notification_service, _, _) = build_handler(0);

		// Create 20MB of statements (200 statements x 100KB each)
		// Using 100KB ensures ~10 statements per 1MB batch, requiring ~20 bursts
		let num_statements = 200;
		let statement_size = 100 * 1024; // 100KB per statement
		let mut expected_hashes = Vec::new();
		for i in 0..num_statements {
			let mut statement = new_live_statement();
			let mut data = vec![0u8; statement_size];
			// Use multiple bytes for uniqueness since we have >255 statements
			data[0] = (i % 256) as u8;
			data[1] = (i / 256) as u8;
			statement.set_plain_data(data);
			let hash = statement.hash();
			expected_hashes.push(hash);
			statement_store.insert(statement);
		}

		// Setup peer and simulate connection
		let peer_id = PeerId::random();

		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: Some(format!("/{STATEMENT_PROTOCOL_V1}").into()),
			})
			.await;

		// Verify peer was added and initial sync was queued
		assert!(handler.peers.contains_key(&peer_id));
		assert!(handler.pending_initial_syncs.contains_key(&peer_id));
		assert_eq!(handler.initial_sync_peer_queue.len(), 1);

		// Process bursts until all statements are sent
		let mut burst_count = 0;
		while handler.pending_initial_syncs.contains_key(&peer_id) {
			handler.process_initial_sync_burst();
			handler.flush_pending_sends().await;
			burst_count += 1;
			// Safety limit
			assert!(burst_count <= 300, "Too many bursts, possible infinite loop");
		}

		// Verify multiple bursts were needed
		// With 200 statements x 100KB each and ~1MB per batch, we expect many bursts
		assert!(
			burst_count >= 10,
			"Expected multiple bursts for 200 statements of 100KB each, got {}",
			burst_count
		);

		// Verify all statements were sent
		let sent = notification_service.get_sent_notifications();
		let mut sent_hashes: Vec<_> = sent
			.iter()
			.flat_map(|(peer, notification)| {
				assert_eq!(*peer, peer_id);
				<Statements as Decode>::decode(&mut notification.as_slice()).unwrap()
			})
			.map(|s| s.hash())
			.collect();
		sent_hashes.sort();
		expected_hashes.sort();

		assert_eq!(
			sent_hashes.len(),
			expected_hashes.len(),
			"Expected {} statements to be sent, got {}",
			expected_hashes.len(),
			sent_hashes.len()
		);
		assert_eq!(sent_hashes, expected_hashes, "All statements should be sent");

		// Verify cleanup
		assert!(!handler.pending_initial_syncs.contains_key(&peer_id));
		assert!(handler.initial_sync_peer_queue.is_empty());
	}

	#[tokio::test]
	async fn initial_sync_network_error_leaves_the_sync_to_retry() {
		let (mut handler, statement_store, _, notification_service, _, peer_ids) = build_handler(1);
		let peer_id = peer_ids[0];
		handler.metrics = Some(Metrics::register(&Registry::new()).unwrap());

		// Two statements small enough to share one chunk, so a statement count cannot be
		// mistaken for a chunk count.
		let mut hashes: Vec<_> = [b"initial-sync-one".to_vec(), b"initial-sync-two".to_vec()]
			.into_iter()
			.map(|payload| {
				let mut statement = new_live_statement();
				statement.set_plain_data(payload);
				let hash = statement.hash();
				statement_store.insert(statement);
				hash
			})
			.collect();

		handler.schedule_initial_sync_for_peer(peer_id, None);
		assert!(handler.pending_initial_syncs.contains_key(&peer_id));

		// The network layer rejects the send with a real error, not a timeout.
		notification_service.fail_sends();
		handler.process_initial_sync_burst();
		handler.flush_pending_sends().await;

		assert!(notification_service.get_sent_notifications().is_empty());
		let pending = handler.pending_initial_syncs.get(&peer_id).unwrap();
		assert_eq!(pending.cursor, 0, "an unconfirmed chunk must not advance the cursor");
		assert_eq!(
			handler.initial_sync_peer_queue.iter().filter(|p| **p == peer_id).count(),
			1,
			"the failed send must requeue the peer"
		);

		let metrics = handler.metrics.as_ref().unwrap();
		assert_eq!(metrics.send_failures.with_label_values(&[send_failure::NETWORK]).get(), 1,);
		assert_eq!(
			metrics.undelivered_statements.with_label_values(&[send_failure::NETWORK]).get(),
			0,
			"a retried sync chunk is not abandoned, so its statements are not undelivered"
		);
		assert_eq!(
			metrics.send_failures.with_label_values(&[send_failure::TIMEOUT]).get(),
			0,
			"a network error must not be attributed to a timeout"
		);

		// The network recovers: the retried chunk delivers the same statements.
		notification_service.allow_sends();
		handler.process_initial_sync_burst();
		handler.flush_pending_sends().await;

		let mut sent = get_peer_hashes(&notification_service.get_sent_notifications(), peer_id);
		sent.sort();
		hashes.sort();
		assert_eq!(sent, hashes);

		handler.process_initial_sync_burst();
		assert!(!handler.pending_initial_syncs.contains_key(&peer_id));
	}

	#[tokio::test]
	async fn rescheduling_a_sync_drops_the_propagation_outbox() {
		let (mut handler, statement_store, _network, notification_service, _, peer_ids) =
			build_handler(1);
		let peer_id = peer_ids[0];

		let mut statement = new_live_statement();
		statement.set_plain_data(b"queued before re-sync".to_vec());
		let hash = statement.hash();
		statement_store.insert(statement);
		handler.propagation_outboxes.push(peer_id, vec![hash]);

		handler.schedule_initial_sync_for_peer(peer_id, None);

		assert!(
			handler.propagation_outboxes.get(&peer_id).is_none(),
			"hashes queued before the sync are covered by its cursor"
		);

		// The sync cursor remains the only path, so the statement arrives exactly once.
		handler.process_initial_sync_burst();
		handler.flush_pending_sends().await;
		let sent = get_peer_hashes(&notification_service.get_sent_notifications(), peer_id);
		assert_eq!(sent, vec![hash]);
	}

	#[tokio::test]
	async fn a_sync_after_a_filter_change_keeps_the_outbox_and_the_watermark() {
		let (mut handler, statement_store, _network, notification_service, _, peer_ids) =
			build_handler(1);
		let peer_id = peer_ids[0];

		// On topic 7, so the previous filter matched it and delivered it.
		let mut statement = new_live_statement();
		statement.set_plain_data(b"queued before the filter change".to_vec());
		statement.set_topic(0, topic(7));
		let hash = statement.hash();
		statement_store.insert(statement);
		handler.propagation_outboxes.push(peer_id, vec![hash]);
		let watermark_before = handler.peers.get(&peer_id).unwrap().sync_watermark;

		handler.schedule_initial_sync_for_peer(peer_id, Some(filter_over(&[topic(7)])));

		assert!(
			handler.propagation_outboxes.get(&peer_id).is_some(),
			"the outbox stays the only path for what the sync skips"
		);
		assert_eq!(
			handler.peers.get(&peer_id).unwrap().sync_watermark,
			watermark_before,
			"propagation stays the only path for what the sync skips"
		);

		while handler.pending_initial_syncs.contains_key(&peer_id) {
			handler.process_initial_sync_burst();
			handler.flush_pending_sends().await;
		}
		let sent = get_peer_hashes(&notification_service.get_sent_notifications(), peer_id);
		assert!(sent.is_empty(), "the sync skips what the previous filter matched");
	}

	#[tokio::test]
	async fn missing_sink_leaves_the_initial_sync_to_retry() {
		let (mut handler, statement_store, _network, notification_service, _, peer_ids) =
			build_handler(1);
		let peer_id = peer_ids[0];

		let mut statement = new_live_statement();
		statement.set_plain_data(b"initial-sync statement".to_vec());
		let hash = statement.hash();
		statement_store.insert(statement);

		handler.schedule_initial_sync_for_peer(peer_id, None);

		// No sink is available for the first burst.
		notification_service.serve_sinks(0);
		handler.process_initial_sync_burst();

		assert!(handler.pending_sends.is_empty());
		assert_eq!(
			handler.pending_initial_syncs.get(&peer_id).unwrap().cursor,
			0,
			"a chunk that found no sink must not advance the cursor"
		);
		assert_eq!(
			handler.initial_sync_peer_queue.iter().filter(|p| **p == peer_id).count(),
			1,
			"the peer must be requeued for a retry"
		);

		// The sink comes back: the retried chunk delivers.
		notification_service.serve_sinks(usize::MAX);
		handler.process_initial_sync_burst();
		handler.flush_pending_sends().await;

		let sent = get_peer_hashes(&notification_service.get_sent_notifications(), peer_id);
		assert_eq!(sent, vec![hash]);
	}

	#[tokio::test]
	async fn failed_store_fetch_retains_the_sync_and_a_later_burst_retries() {
		let (mut handler, statement_store, _network, notification_service, _, peer_ids) =
			build_handler(1);
		let peer_id = peer_ids[0];

		let mut statement = new_live_statement();
		statement.set_plain_data(b"initial-sync statement".to_vec());
		let hash = statement.hash();
		statement_store.insert(statement);

		handler.schedule_initial_sync_for_peer(peer_id, None);
		let watermark = handler.pending_initial_syncs.get(&peer_id).unwrap().watermark;

		statement_store.fail_fetches.store(true, Ordering::Relaxed);
		handler.process_initial_sync_burst();

		assert!(handler.pending_sends.is_empty(), "a failed fetch must not queue a send");
		let pending = handler.pending_initial_syncs.get(&peer_id).unwrap();
		assert_eq!(pending.cursor, 0, "the cursor must stay at the failed position");
		assert_eq!(pending.watermark, watermark);
		assert_eq!(
			handler.initial_sync_peer_queue.iter().filter(|p| **p == peer_id).count(),
			1,
			"the peer must be requeued for a retry"
		);

		// The store recovers: the next burst resumes from the retained cursor.
		statement_store.fail_fetches.store(false, Ordering::Relaxed);
		handler.process_initial_sync_burst();
		handler.flush_pending_sends().await;

		let sent = get_peer_hashes(&notification_service.get_sent_notifications(), peer_id);
		assert_eq!(sent, vec![hash]);

		// The completed send requeued the peer; the next burst observes the finished sync.
		handler.process_initial_sync_burst();
		assert!(!handler.pending_initial_syncs.contains_key(&peer_id));
	}

	#[tokio::test]
	async fn missing_sink_counts_every_abandoned_statement() {
		let (mut handler, statement_store, _, notification_service, _, peer_ids) = build_handler(1);
		let peer_id = peer_ids[0];
		handler.metrics = Some(Metrics::register(&Registry::new()).unwrap());

		// 100 KB each, so the batch spans three 1 MiB chunks. Two chunks must remain after the
		// sink disappears, otherwise "the whole remainder" and "the current chunk" are the same
		// number and the test proves nothing.
		let total = 25;
		for i in 0..total {
			let mut statement = new_live_statement();
			let mut data = vec![0u8; 100 * 1024];
			data[0] = i as u8;
			statement.set_plain_data(data);
			statement_store
				.recent_statements
				.lock()
				.unwrap()
				.insert(statement.hash(), statement);
		}

		// Serve the first chunk, then behave as if the peer went away.
		notification_service.serve_sinks(1);
		handler.propagate_statements().await;
		handler.flush_pending_sends().await;

		let delivered =
			get_peer_hashes(&notification_service.get_sent_notifications(), peer_id).len();
		assert!(delivered > 0);
		assert!(
			total - delivered > delivered,
			"the abandoned remainder must exceed one full chunk, delivered {delivered} of {total}"
		);

		let metrics = handler.metrics.as_ref().unwrap();
		assert_eq!(metrics.send_failures.with_label_values(&[send_failure::NO_SINK]).get(), 1,);
		assert_eq!(
			metrics.undelivered_statements.with_label_values(&[send_failure::NO_SINK]).get(),
			(total - delivered) as u64,
			"every statement after the delivered chunk counts as undelivered"
		);
	}

	#[tokio::test]
	async fn burst_with_nothing_to_send_returns_the_peer_to_the_queue() {
		let (mut handler, statement_store, _network, notification_service, _, peer_ids) =
			build_handler(1);
		let peer_id = peer_ids[0];

		let mut statement = new_live_statement();
		statement.set_plain_data(b"filtered by affinity".to_vec());
		statement.set_topic(0, [0xAA; 32].into());
		statement_store.insert(statement);

		// A topic affinity matching nothing in the store, so the burst finds no
		// statement to send.
		let mut filter = AffinityFilter::new(BLOOM_SEED, 0.01, 100);
		filter.insert(&[0xBB; 32]);
		{
			let peer = handler.peers.get_mut(&peer_id).unwrap();
			peer.protocol_version = PeerProtocolVersion::V2;
			peer.topic_affinity = Some(filter);
		}
		handler.schedule_initial_sync_for_peer(peer_id, None);

		// No chunk means no send result to advance the sync, so the burst has to requeue the peer
		// itself or the sync stalls with its entry alive and the peer out of the queue.
		handler.process_initial_sync_burst();
		assert!(handler.pending_sends.is_empty());
		assert!(notification_service.get_sent_notifications().is_empty());
		assert_eq!(handler.initial_sync_peer_queue.len(), 1);

		handler.process_initial_sync_burst();
		assert!(!handler.pending_initial_syncs.contains_key(&peer_id));
		assert!(handler.initial_sync_peer_queue.is_empty());
	}

	#[tokio::test]
	async fn initial_sync_keeps_one_chunk_per_peer_in_flight() {
		let (mut handler, statement_store, _network, notification_service, _, peer_ids) =
			build_handler(1);
		let peer_id = peer_ids[0];

		// 100 KB each, so the store spans several 1 MiB chunks and the peer has more to receive
		// after its first one.
		for i in 0..25u8 {
			let mut statement = new_live_statement();
			let mut data = vec![0u8; 100 * 1024];
			data[0] = i;
			statement.set_plain_data(data);
			statement_store.insert(statement);
		}

		// The peer never reads its substream.
		notification_service.block_sends();
		handler.schedule_initial_sync_for_peer(peer_id, None);

		for _ in 0..10 {
			handler.process_initial_sync_burst();
		}

		assert_eq!(handler.pending_sends.len(), 1);
		assert!(handler.initial_sync_peer_queue.is_empty());
	}

	#[tokio::test]
	async fn superseded_initial_sync_result_is_discarded() {
		let (mut handler, statement_store, _network, _notification_service, _, peer_ids) =
			build_handler(1);
		let peer_id = peer_ids[0];

		let mut statement = new_live_statement();
		statement.set_plain_data(b"superseded".to_vec());
		statement_store.insert(statement);

		handler.schedule_initial_sync_for_peer(peer_id, None);
		handler.process_initial_sync_burst();
		assert_eq!(handler.pending_sends.len(), 1);
		assert!(handler.initial_sync_in_flight_bytes > 0);

		// An affinity change re-schedules the sync while the chunk is still in flight.
		handler.schedule_initial_sync_for_peer(peer_id, None);
		handler.flush_pending_sends().await;

		assert_eq!(
			handler.initial_sync_peer_queue.iter().filter(|peer| **peer == peer_id).count(),
			1
		);
		assert_eq!(handler.initial_sync_in_flight_bytes, 0);
	}

	#[tokio::test]
	async fn failed_superseded_initial_sync_result_does_not_abort_the_new_sync() {
		let (mut handler, statement_store, _network, notification_service, _, peer_ids) =
			build_handler(1);
		let peer_id = peer_ids[0];

		let mut statement = new_live_statement();
		statement.set_plain_data(b"superseded failure".to_vec());
		statement_store.insert(statement);

		// Queue a chunk, then arrange for that very send to come back as a failure.
		handler.schedule_initial_sync_for_peer(peer_id, None);
		handler.process_initial_sync_burst();
		notification_service.fail_sends();

		// A reconnect replaces the sync while that chunk is still in flight.
		handler.schedule_initial_sync_for_peer(peer_id, None);
		let sync_id = handler.pending_initial_syncs.get(&peer_id).unwrap().sync_id;
		handler.flush_pending_sends().await;

		assert_eq!(
			handler.pending_initial_syncs.get(&peer_id).map(|pending| pending.sync_id),
			Some(sync_id)
		);
		assert_eq!(
			handler.initial_sync_peer_queue.iter().filter(|peer| **peer == peer_id).count(),
			1
		);
	}

	#[tokio::test]
	async fn initial_sync_respects_the_payload_size_boundary() {
		let (mut handler, statement_store, _network, notification_service, _, peer_ids) =
			build_handler(1);
		handler.metrics = Some(Metrics::register(&Registry::new()).unwrap());
		let peer_id = peer_ids[0];
		let overhead = handler.peers.get(&peer_id).unwrap().protocol_version.envelope_overhead();
		let max_size = max_statement_payload_size(overhead);

		let mut data_len = max_size - 32;
		let exact = loop {
			let mut candidate = new_live_statement();
			candidate.set_plain_data(vec![7u8; data_len]);
			let size = candidate.encoded_size();
			assert!(size <= max_size, "no data length encodes to exactly {max_size}");
			if size == max_size {
				break candidate;
			}
			data_len += 1;
		};
		let exact_hash = exact.hash();
		let mut oversized = new_live_statement();
		oversized.set_plain_data(vec![2u8; MAX_STATEMENT_NOTIFICATION_SIZE as usize]);
		statement_store.insert(exact);
		statement_store.insert(oversized);

		handler.schedule_initial_sync_for_peer(peer_id, None);
		for _ in 0..10 {
			if !handler.pending_initial_syncs.contains_key(&peer_id) {
				break;
			}
			handler.process_initial_sync_burst();
			handler.flush_pending_sends().await;
		}

		assert!(!handler.pending_initial_syncs.contains_key(&peer_id));
		assert_eq!(handler.metrics.as_ref().unwrap().skipped_oversized_statements.get(), 1);
		let sent = get_peer_hashes(&notification_service.get_sent_notifications(), peer_id);
		assert_eq!(sent, vec![exact_hash], "only the exactly-fitting statement is delivered");
	}

	#[tokio::test]
	async fn initial_sync_in_flight_budget_is_bounded() {
		let (mut handler, statement_store, _network, notification_service, _, peer_ids) =
			build_handler(20);

		// ~1.1 MB of statements, so each peer's first chunk sits just under the 1 MiB cap and a
		// handful of peers is enough to exhaust the budget.
		for i in 0..11u8 {
			let mut statement = new_live_statement();
			let mut data = vec![0u8; 100 * 1024];
			data[0] = i;
			statement.set_plain_data(data);
			statement_store.insert(statement);
		}

		// No peer reads, so nothing ever leaves the budget.
		notification_service.block_sends();
		for peer_id in &peer_ids {
			handler.schedule_initial_sync_for_peer(*peer_id, None);
		}

		let mut bursts = 0;
		while handler.initial_sync_in_flight_bytes < MAX_SEND_IN_FLIGHT_BYTES {
			handler.process_initial_sync_burst();
			bursts += 1;
			assert!(bursts <= 100, "the budget was never reached after {bursts} bursts");
		}

		let in_flight = handler.initial_sync_in_flight_bytes;
		assert!(in_flight >= MAX_SEND_IN_FLIGHT_BYTES);
		assert!(
			in_flight < MAX_SEND_IN_FLIGHT_BYTES + MAX_STATEMENT_NOTIFICATION_SIZE,
			"the budget may only be overshot by the single chunk that crossed it, got {in_flight}"
		);

		// A throttled burst must leave the round-robin untouched rather than burn a peer's turn.
		let queued = handler.initial_sync_peer_queue.clone();
		handler.process_initial_sync_burst();
		assert_eq!(handler.initial_sync_peer_queue, queued);
		assert_eq!(handler.initial_sync_in_flight_bytes, in_flight);
	}

	#[tokio::test]
	async fn saturated_send_budget_defers_propagation() {
		let (mut handler, statement_store, _network, notification_service, _, peer_ids) =
			build_handler(1);
		let peer_id = peer_ids[0];

		let mut statement = new_live_statement();
		statement.set_plain_data(b"deferred by budget".to_vec());
		let hash = statement.hash();
		statement_store.recent_statements.lock().unwrap().insert(hash, statement);

		// The whole budget is taken by initial-sync bytes.
		handler.initial_sync_in_flight_bytes = MAX_SEND_IN_FLIGHT_BYTES;
		handler.propagate_statements().await;

		assert!(handler.pending_sends.is_empty(), "no chunk may be queued over the budget");
		assert_eq!(
			handler.propagation_outboxes.get(&peer_id).unwrap(),
			&VecDeque::from(vec![hash])
		);
		assert_eq!(handler.parked_propagations, VecDeque::from([peer_id]));

		// A completed initial-sync send frees the budget and refills the parked peer.
		handler.handle_send_result(PendingSendResult {
			peer: peer_id,
			statement_count: 1,
			bytes_sent: MAX_SEND_IN_FLIGHT_BYTES,
			result: SendOutcome::Sent,
			kind: SendKind::InitialSync { sync_id: 0, next_cursor: 0 },
			route: peer_route::LEGACY,
			chunk_id: 0,
		});
		handler.flush_pending_sends().await;

		let sent = get_peer_hashes(&notification_service.get_sent_notifications(), peer_id);
		assert_eq!(sent, vec![hash]);
		assert!(handler.parked_propagations.is_empty());
	}

	#[tokio::test]
	async fn parked_peers_are_refilled_in_parking_order() {
		let (mut handler, statement_store, _network, notification_service, _, _) = build_handler(2);

		// One chunk is ~900 KB, so freeing a few bytes admits exactly one of the
		// two parked peers into the 16 MiB budget.
		let mut statement = new_live_statement();
		statement.set_plain_data(vec![7u8; 900 * 1024]);
		let hash = statement.hash();
		statement_store.recent_statements.lock().unwrap().insert(hash, statement);

		handler.initial_sync_in_flight_bytes = MAX_SEND_IN_FLIGHT_BYTES;
		handler.propagate_statements().await;
		assert_eq!(handler.parked_propagations.len(), 2);
		let first = handler.parked_propagations[0];
		let second = handler.parked_propagations[1];

		// Freeing a sliver of budget admits only the first parked peer.
		handler.handle_send_result(PendingSendResult {
			peer: first,
			statement_count: 1,
			bytes_sent: 100,
			result: SendOutcome::Sent,
			kind: SendKind::InitialSync { sync_id: 0, next_cursor: 0 },
			route: peer_route::LEGACY,
			chunk_id: 0,
		});
		assert_eq!(handler.pending_sends.len(), 1);
		assert_eq!(handler.parked_propagations, VecDeque::from([second]));

		// The first chunk's completion frees enough for the second peer.
		handler.flush_pending_sends().await;
		let sent = notification_service.get_sent_notifications();
		assert_eq!(
			sent.iter().map(|(peer, _)| *peer).collect::<Vec<_>>(),
			vec![first, second],
			"peers must be served in parking order"
		);
		assert!(handler.parked_propagations.is_empty());
		assert_eq!(handler.propagation_in_flight_bytes, 0);
	}

	#[tokio::test]
	async fn pending_initial_sync_reserves_send_budget_from_propagation() {
		let (mut handler, statement_store, _network, notification_service, _, peer_ids) =
			build_handler(2);
		let propagation_peer = peer_ids[0];
		let sync_peer = peer_ids[1];

		let mut statement = new_live_statement();
		statement.set_plain_data(b"backlog".to_vec());
		let hash = statement.hash();
		statement_store.insert(statement);
		handler.schedule_initial_sync_for_peer(sync_peer, None);
		assert!(handler.pending_initial_syncs.contains_key(&sync_peer));

		// Propagation holds the whole budget and one peer waits parked.
		handler.propagation_in_flight_bytes = MAX_SEND_IN_FLIGHT_BYTES;
		handler.propagation_outboxes.push(propagation_peer, vec![hash]);
		handler.parked_propagations.push_back(propagation_peer);

		// A completed send frees exactly the reserve: the parked peer must stay parked,
		// keeping the headroom for the sync burst.
		handler.handle_send_result(PendingSendResult {
			peer: propagation_peer,
			statement_count: 1,
			bytes_sent: INITIAL_SYNC_RESERVED_BYTES,
			result: SendOutcome::Sent,
			kind: SendKind::Propagation,
			route: peer_route::LEGACY,
			chunk_id: 0,
		});
		assert!(handler.pending_sends.is_empty(), "the reserve must not refill propagation");
		assert_eq!(handler.parked_propagations, VecDeque::from([propagation_peer]));

		// The sync burst finds the reserved headroom and proceeds.
		handler.process_initial_sync_burst();
		assert_eq!(handler.pending_sends.len(), 1, "the sync burst must use the reserve");
		handler.flush_pending_sends().await;
		let synced = get_peer_hashes(&notification_service.get_sent_notifications(), sync_peer);
		assert_eq!(synced, vec![hash]);
		assert_eq!(
			handler.parked_propagations,
			VecDeque::from([propagation_peer]),
			"the reserve holds while the sync is still pending"
		);

		// The next burst observes the drained sync and completes it, releasing the
		// reserve back to propagation.
		handler.process_initial_sync_burst();
		assert!(handler.pending_initial_syncs.is_empty());
		handler.fill_parked_propagations();
		assert!(handler.parked_propagations.is_empty());
		handler.flush_pending_sends().await;
		let propagated =
			get_peer_hashes(&notification_service.get_sent_notifications(), propagation_peer);
		assert_eq!(propagated, vec![hash]);
	}

	#[tokio::test]
	async fn burst_skips_a_peer_with_a_busy_send_slot() {
		let (mut handler, statement_store, _network, notification_service, _, peer_ids) =
			build_handler(2);

		let mut statement = new_live_statement();
		statement.set_plain_data(b"burst behind a busy slot".to_vec());
		statement_store.insert(statement);

		handler.schedule_initial_sync_for_peer(peer_ids[0], None);
		handler.schedule_initial_sync_for_peer(peer_ids[1], None);
		// The first queued peer's send slot is taken by a propagation chunk.
		handler.in_flight_chunks.insert(peer_ids[0], 42);

		handler.process_initial_sync_burst();
		handler.flush_pending_sends().await;

		// The same burst serves the next queued peer instead of returning.
		let sent = notification_service.get_sent_notifications();
		assert_eq!(sent.len(), 1);
		assert_eq!(sent[0].0, peer_ids[1]);
		// The busy peer keeps its sync and its turn.
		assert!(handler.pending_initial_syncs.contains_key(&peer_ids[0]));
		assert!(handler.initial_sync_peer_queue.contains(&peer_ids[0]));
	}

	#[tokio::test]
	async fn peer_with_both_kinds_pending_sends_propagation_first() {
		let (mut handler, statement_store, _network, notification_service, _, peer_ids) =
			build_handler(1);
		let peer_id = peer_ids[0];

		// An initial-sync chunk takes the slot, then a fresh statement arrives by
		// tick while the slot is busy.
		let mut synced = new_live_statement();
		synced.set_plain_data(b"snapshot statement".to_vec());
		let synced_hash = synced.hash();
		statement_store.insert(synced);
		handler.schedule_initial_sync_for_peer(peer_id, None);
		handler.process_initial_sync_burst();
		assert!(handler.in_flight_chunks.contains_key(&peer_id));

		let mut fresh = new_live_statement();
		fresh.set_plain_data(b"fresh gossip".to_vec());
		let fresh_hash = fresh.hash();
		statement_store.recent_statements.lock().unwrap().insert(fresh_hash, fresh);
		handler.propagate_statements().await;
		assert_eq!(handler.pending_sends.len(), 1, "the propagation chunk must wait in the outbox");

		// The initial-sync completion frees the slot and propagation claims it
		// before the next burst tick gets a chance.
		let result = handler.pending_sends.next().await.unwrap();
		handler.handle_send_result(result);
		assert_eq!(handler.pending_sends.len(), 1);
		assert!(handler.in_flight_chunks.contains_key(&peer_id));
		handler.process_initial_sync_burst();
		assert_eq!(handler.pending_sends.len(), 1, "a burst must not bypass the busy slot");

		handler.flush_pending_sends().await;
		let sent = get_peer_hashes(&notification_service.get_sent_notifications(), peer_id);
		assert_eq!(sent, vec![synced_hash, fresh_hash]);
	}

	#[tokio::test]
	async fn initial_sync_and_propagation_share_the_budget() {
		let (mut handler, statement_store, _network, _notification_service, _, peer_ids) =
			build_handler(1);
		let peer_id = peer_ids[0];

		let mut statement = new_live_statement();
		statement.set_plain_data(b"shared budget".to_vec());
		statement_store.insert(statement);
		handler.schedule_initial_sync_for_peer(peer_id, None);

		// Propagation bytes alone exhaust the shared budget, so the burst must wait.
		handler.propagation_in_flight_bytes = MAX_SEND_IN_FLIGHT_BYTES;
		let queued = handler.initial_sync_peer_queue.clone();
		handler.process_initial_sync_burst();

		assert!(handler.pending_sends.is_empty());
		assert_eq!(
			handler.initial_sync_peer_queue, queued,
			"a throttled burst must not burn the peer's turn"
		);
	}

	#[tokio::test]
	async fn initial_sync_waits_for_the_peer_rate_limit() {
		let (mut handler, statement_store, _network, notification_service, _, peer_ids) =
			build_handler(2);
		let throttled = peer_ids[0];
		let served = peer_ids[1];
		let hashes: Vec<_> = (0..20u8)
			.map(|i| {
				let mut statement = new_live_statement();
				statement.set_plain_data(vec![i]);
				let hash = statement.hash();
				statement_store.insert(statement);
				hash
			})
			.collect();
		handler.schedule_initial_sync_for_peer(throttled, None);
		handler.schedule_initial_sync_for_peer(served, None);

		// One statement per second with a burst of ten, drained by a propagation chunk.
		let clock = FakeRelativeClock::default();
		handler.peers.get_mut(&throttled).expect("peer is connected").send_budget =
			PeerRateLimiter::with_clock(
				NonZeroU32::new(1).expect("nonzero"),
				NonZeroU32::new(10).expect("nonzero"),
				&clock,
			);
		handler.propagation_outboxes.push(throttled, hashes[..10].to_vec());
		handler.try_send_next_chunk(throttled);
		handler.flush_pending_sends().await;
		assert!(handler.peers[&throttled].send_budget.is_flooding(1), "propagation charged");

		// The throttled peer heads the queue: its fetch stops at the first statement and the
		// following burst serves its neighbour.
		handler.process_initial_sync_burst();
		handler.process_initial_sync_burst();
		handler.flush_pending_sends().await;
		let sent = notification_service.get_sent_notifications();
		assert_eq!(get_peer_hashes(&sent, served), hashes);
		let mut expected = hashes[..10].to_vec();
		assert_eq!(get_peer_hashes(&sent, throttled), expected, "the propagation chunk alone");
		assert!(handler.initial_sync_peer_queue.contains(&throttled), "a skipped turn is kept");

		// Ten seconds refill ten statements, so the chunk carries half the backlog.
		clock.advance(Duration::from_secs(10));
		handler.process_initial_sync_burst();
		handler.process_initial_sync_burst();
		handler.flush_pending_sends().await;
		let sent = notification_service.get_sent_notifications();
		expected.extend_from_slice(&hashes[..10]);
		assert_eq!(get_peer_hashes(&sent, throttled), expected);

		// The fetch resumes at the statement the quota stopped it on.
		clock.advance(Duration::from_secs(10));
		handler.process_initial_sync_burst();
		handler.process_initial_sync_burst();
		handler.flush_pending_sends().await;
		let sent = notification_service.get_sent_notifications();
		expected.extend_from_slice(&hashes[10..]);
		assert_eq!(get_peer_hashes(&sent, throttled), expected);
	}

	#[tokio::test]
	async fn test_initial_sync_burst_multiple_peers_round_robin() {
		let (mut handler, statement_store, _network, notification_service, _, _) = build_handler(0);

		// Create 20MB of statements (200 statements x 100KB each)
		let num_statements = 200;
		let statement_size = 100 * 1024; // 100KB per statement
		let mut expected_hashes = Vec::new();
		for i in 0..num_statements {
			let mut statement = new_live_statement();
			let mut data = vec![0u8; statement_size];
			data[0] = (i % 256) as u8;
			data[1] = (i / 256) as u8;
			statement.set_plain_data(data);
			let hash = statement.hash();
			expected_hashes.push(hash);
			statement_store.insert(statement);
		}

		// Setup 3 peers and simulate connections
		let peer1 = PeerId::random();
		let peer2 = PeerId::random();
		let peer3 = PeerId::random();

		// Connect peers
		for peer in [peer1, peer2, peer3] {
			handler
				.handle_notification_event(NotificationEvent::NotificationStreamOpened {
					peer,
					direction: sc_network::service::traits::Direction::Inbound,
					handshake: vec![],
					negotiated_fallback: Some(format!("/{STATEMENT_PROTOCOL_V1}").into()),
				})
				.await;
		}

		// Verify all peers were added and initial syncs were queued
		assert_eq!(handler.peers.len(), 3);
		assert_eq!(handler.pending_initial_syncs.len(), 3);
		assert_eq!(handler.initial_sync_peer_queue.len(), 3);

		// Track which peer was processed on each burst for round-robin verification
		let mut peer_burst_order = Vec::new();
		let mut burst_count = 0;

		while !handler.pending_initial_syncs.is_empty() {
			// Record which peer will be processed next
			if let Some(&next_peer) = handler.initial_sync_peer_queue.front() {
				peer_burst_order.push(next_peer);
			}
			handler.process_initial_sync_burst();
			handler.flush_pending_sends().await;
			burst_count += 1;
			// Safety limit
			assert!(burst_count <= 500, "Too many bursts, possible infinite loop");
		}

		// Verify multiple bursts were needed
		// With 3 peers and many bursts per peer, we expect many bursts total
		assert!(
			burst_count >= 30,
			"Expected many bursts for 3 peers with 200 statements each, got {}",
			burst_count
		);

		// Verify round-robin pattern in first 9 bursts (3 peers x 3 rounds)
		assert!(peer_burst_order.len() >= 9, "Expected at least 9 bursts");
		// First round
		assert_eq!(peer_burst_order[0], peer1, "First burst should be peer1");
		assert_eq!(peer_burst_order[1], peer2, "Second burst should be peer2");
		assert_eq!(peer_burst_order[2], peer3, "Third burst should be peer3");
		// Second round
		assert_eq!(peer_burst_order[3], peer1, "Fourth burst should be peer1");
		assert_eq!(peer_burst_order[4], peer2, "Fifth burst should be peer2");
		assert_eq!(peer_burst_order[5], peer3, "Sixth burst should be peer3");

		// Verify all peers received all statements
		let sent = notification_service.get_sent_notifications();
		let mut peer1_hashes = get_peer_hashes(&sent, peer1);
		let mut peer2_hashes = get_peer_hashes(&sent, peer2);
		let mut peer3_hashes = get_peer_hashes(&sent, peer3);

		peer1_hashes.sort();
		peer2_hashes.sort();
		peer3_hashes.sort();
		expected_hashes.sort();

		assert_eq!(peer1_hashes, expected_hashes, "Peer1 should receive all statements");
		assert_eq!(peer2_hashes, expected_hashes, "Peer2 should receive all statements");
		assert_eq!(peer3_hashes, expected_hashes, "Peer3 should receive all statements");

		// Verify cleanup
		assert!(handler.pending_initial_syncs.is_empty());
		assert!(handler.initial_sync_peer_queue.is_empty());
	}

	#[tokio::test]
	async fn test_send_statements_in_chunks_exact_max_size() {
		let (mut handler, statement_store, _network, notification_service, _queue_receiver, _) =
			build_handler(1);

		// Calculate the data sizes so that 100 statements together exactly fill max_size.
		// This tests that all 100 statements fit in a single notification.
		//
		// The limit check in `fetch_statement_chunk` is:
		//   max_size = MAX_STATEMENT_NOTIFICATION_SIZE - Compact::<u32>::max_encoded_len()
		//
		// Statement encoding (encodes as Vec<Field>):
		// - Compact<u32> for number of fields (1 byte for value 2: expiry + data)
		// - Field::Expiry discriminant (1 byte, value 2)
		// - u64 expiry value (8 bytes)
		// - Field::Data discriminant (1 byte, value 8)
		// - Compact<u32> for the data length (2 bytes for small data)
		// So per-statement overhead = 1 + 1 + 8 + 1 + 2 = 13 bytes
		let max_size = MAX_STATEMENT_NOTIFICATION_SIZE as usize - Compact::<u32>::max_encoded_len();
		let num_statements: usize = 100;
		// Vec<Field> length + expiry field + data discriminant + Compact data length
		let per_statement_overhead = 1 + 1 + 8 + 1 + 2;
		let total_overhead = per_statement_overhead * num_statements;
		let total_data_size = max_size - total_overhead;
		let per_statement_data_size = total_data_size / num_statements;
		let remainder = total_data_size % num_statements;

		let mut expected_hashes = Vec::with_capacity(num_statements);
		let mut total_encoded_size = 0;

		for i in 0..num_statements {
			let mut statement = new_live_statement();
			// Distribute remainder across first `remainder` statements to exactly fill max_size
			let extra = if i < remainder { 1 } else { 0 };
			let mut data = vec![42u8; per_statement_data_size + extra];
			// Make each statement unique by modifying the first few bytes
			data[0] = i as u8;
			data[1] = (i >> 8) as u8;
			statement.set_plain_data(data);

			total_encoded_size += statement.encoded_size();

			let hash = statement.hash();
			expected_hashes.push(hash);
			statement_store.recent_statements.lock().unwrap().insert(hash, statement);
		}

		// Verify our calculation: total encoded size should be <= max_size
		assert!(
			total_encoded_size == max_size,
			"Total encoded size {} should be <= max_size {}",
			total_encoded_size,
			max_size
		);

		handler.propagate_statements().await;
		handler.flush_pending_sends().await;

		let sent = notification_service.get_sent_notifications();

		// All statements should fit in a single chunk
		assert_eq!(
			sent.len(),
			1,
			"Expected 1 notification for all {} statements, but got {}",
			num_statements,
			sent.len()
		);

		let (_peer, notification) = &sent[0];
		assert!(
			notification.len() <= MAX_STATEMENT_NOTIFICATION_SIZE as usize,
			"Notification size {} exceeds limit {}",
			notification.len(),
			MAX_STATEMENT_NOTIFICATION_SIZE
		);

		let decoded = <Statements as Decode>::decode(&mut notification.as_slice()).unwrap();
		assert_eq!(
			decoded.len(),
			num_statements,
			"Expected {} statements in the notification",
			num_statements
		);

		// Verify all statements were sent (order may differ due to HashMap iteration)
		let mut received_hashes: Vec<_> = decoded.iter().map(|s| s.hash()).collect();
		expected_hashes.sort();
		received_hashes.sort();
		assert_eq!(expected_hashes, received_hashes, "All statement hashes should match");
	}

	#[tokio::test]
	async fn test_initial_sync_burst_size_limit_consistency() {
		// A filter measuring against the raw `MAX_STATEMENT_NOTIFICATION_SIZE` would admit
		// statements the encoder cannot fit, so both must use `max_statement_payload_size`.
		let (mut handler, statement_store, _network, notification_service, _, _) = build_handler(0);

		// This peer connects as V1 (see negotiated_fallback below).
		let payload_limit = max_statement_payload_size(V1_ENVELOPE_OVERHEAD);

		// Create first statement that's just over half the payload limit
		let first_stmt_data_size = payload_limit / 2 + 10;
		let mut stmt1 = new_live_statement();
		stmt1.set_plain_data(vec![1u8; first_stmt_data_size]);
		let stmt1_encoded_size = stmt1.encoded_size();

		// Create second statement that, combined with the first, exceeds the payload limit.
		// This means the filter will only accept the first statement.
		let remaining = payload_limit.saturating_sub(stmt1_encoded_size);
		let target_stmt2_encoded = remaining + 3; // 3 bytes over limit when combined
		let stmt2_data_size = target_stmt2_encoded.saturating_sub(4); // ~4 bytes encoding overhead
		let mut stmt2 = new_live_statement();
		stmt2.set_plain_data(vec![2u8; stmt2_data_size]);
		let stmt2_encoded_size = stmt2.encoded_size();

		let total_encoded = stmt1_encoded_size + stmt2_encoded_size;

		// Verify our setup: total exceeds payload limit
		assert!(
			total_encoded > payload_limit,
			"Total {} should exceed payload_limit {} so filter rejects second statement",
			total_encoded,
			payload_limit
		);

		let hash1 = stmt1.hash();
		let hash2 = stmt2.hash();
		statement_store.insert(stmt1);
		statement_store.insert(stmt2);

		// Setup peer and simulate connection
		let peer_id = PeerId::random();

		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: Some(format!("/{STATEMENT_PROTOCOL_V1}").into()),
			})
			.await;

		// Verify initial sync was queued covering both statements
		assert!(handler.pending_initial_syncs.contains_key(&peer_id));
		let pending = handler.pending_initial_syncs.get(&peer_id).unwrap();
		assert_eq!(pending.watermark - pending.cursor, 2);

		// Process first burst - should send only one statement (the other doesn't fit)
		handler.process_initial_sync_burst();
		handler.flush_pending_sends().await;

		// The filter and the chunk bound agree, so only the statement that fits is
		// fetched and sent.
		let sent = notification_service.get_sent_notifications();
		assert_eq!(sent.len(), 1, "First burst should send one notification");

		let decoded = <Statements as Decode>::decode(&mut sent[0].1.as_slice()).unwrap();
		assert_eq!(decoded.len(), 1, "First notification should contain one statement");

		// Verify one of the two statements was sent (order is non-deterministic due to HashMap)
		let sent_hash = decoded[0].hash();
		assert!(
			sent_hash == hash1 || sent_hash == hash2,
			"Sent statement should be one of the two created"
		);

		// Second statement should still be pending
		assert!(handler.pending_initial_syncs.contains_key(&peer_id));
		let pending = handler.pending_initial_syncs.get(&peer_id).unwrap();
		assert_eq!(pending.watermark - pending.cursor, 1);

		// Process second burst - should send the remaining statement
		handler.process_initial_sync_burst();
		handler.flush_pending_sends().await;

		let sent = notification_service.get_sent_notifications();
		assert_eq!(sent.len(), 2, "Second burst should send another notification");

		// Both statements should now be sent
		let mut sent_hashes: Vec<_> = sent
			.iter()
			.flat_map(|(_, notification)| {
				<Statements as Decode>::decode(&mut notification.as_slice()).unwrap()
			})
			.map(|s| s.hash())
			.collect();
		sent_hashes.sort();
		let mut expected_hashes = vec![hash1, hash2];
		expected_hashes.sort();
		assert_eq!(sent_hashes, expected_hashes, "Both statements should be sent");

		// The sync is closed by the burst that finds nothing left to send, not by the one that sent
		// the last chunk.
		handler.process_initial_sync_burst();
		assert!(!handler.pending_initial_syncs.contains_key(&peer_id));
	}

	#[tokio::test]
	async fn test_peer_disconnected_on_flooding() {
		let (mut handler, _statement_store, network, _notification_service, _queue_receiver, _) =
			build_handler(1);

		let peer_id = *handler.peers.keys().next().unwrap();

		let mut flood_statements = Vec::new();
		for i in 0..600_000 {
			let mut statement = new_live_statement();
			statement.set_plain_data(vec![i as u8, (i >> 8) as u8, (i >> 16) as u8]);
			flood_statements.push(statement);
		}

		handler.on_statements(peer_id, flood_statements);

		let reports = network.get_reports();
		assert!(
			reports
				.iter()
				.any(|(id, rep)| *id == peer_id && *rep == rep::STATEMENT_FLOODING),
			"Expected STATEMENT_FLOODING reputation change, but got: {:?}",
			reports
		);

		let disconnected = network.get_disconnected_peers();
		assert!(
			disconnected.contains(&peer_id),
			"Expected peer {} to be disconnected, but it wasn't. Disconnected peers: {:?}",
			peer_id,
			disconnected
		);

		dispatch_disconnects(&mut handler, &network).await;

		// Verify peer state was cleaned up
		assert!(!handler.peers.contains_key(&peer_id), "Peer should be removed from peers map");
		assert!(
			!handler.pending_initial_syncs.contains_key(&peer_id),
			"Peer should be removed from pending_initial_syncs"
		);
		assert!(
			!handler.initial_sync_peer_queue.contains(&peer_id),
			"Peer should be removed from initial_sync_peer_queue"
		);
	}

	#[tokio::test]
	async fn test_legitimate_traffic_not_flagged() {
		let (
			mut handler,
			_statement_store,
			network,
			_notification_service,
			_queue_receiver,
			_,
			clock,
		) = build_handler_with_fake_clock(1);

		let peer_id = *handler.peers.keys().next().unwrap();
		let mut counter = 0u32;

		// 100 steps of 100ms is 10 simulated seconds: twice the 5s the full burst takes to drain,
		// so any drift in the accounting would have tripped by the end.
		for _ in 0..100 {
			handler.on_statements(peer_id, statement_batch(5_000, &mut counter));
			clock.advance(Duration::from_millis(100));
		}

		let reports = network.get_reports();
		assert!(
			!reports
				.iter()
				.any(|(id, rep)| *id == peer_id && *rep == rep::STATEMENT_FLOODING),
			"Legitimate traffic should not trigger flooding detection. Reports: {:?}",
			reports
		);

		let disconnected = network.get_disconnected_peers();
		assert!(
			!disconnected.contains(&peer_id),
			"Legitimate traffic should not cause disconnection. Disconnected peers: {:?}",
			disconnected
		);

		assert!(handler.peers.contains_key(&peer_id), "Peer should still be connected");
	}

	#[tokio::test]
	async fn test_just_over_rate_limit_triggers_flooding() {
		let (mut handler, _statement_store, network, _notification_service, _queue_receiver, _) =
			build_handler(1);

		let peer_id = *handler.peers.keys().next().unwrap();

		let mut statements = Vec::new();
		for i in 0..260_000 {
			let mut statement = new_live_statement();
			statement.set_plain_data(vec![
				i as u8,
				(i >> 8) as u8,
				(i >> 16) as u8,
				(i >> 24) as u8,
			]);
			statements.push(statement);
		}

		handler.on_statements(peer_id, statements);

		let reports = network.get_reports();
		let expected_burst = DEFAULT_STATEMENTS_PER_SECOND * config::STATEMENTS_BURST_COEFFICIENT;
		assert!(
			reports
				.iter()
				.any(|(id, rep)| *id == peer_id && *rep == rep::STATEMENT_FLOODING),
			"Sending 260,000 statements should trigger flooding (burst limit: {}). Reports: {:?}",
			expected_burst,
			reports
		);

		let disconnected = network.get_disconnected_peers();
		assert!(
			disconnected.contains(&peer_id),
			"Peer should be disconnected after exceeding rate limit. Disconnected: {:?}",
			disconnected
		);

		dispatch_disconnects(&mut handler, &network).await;

		assert!(!handler.peers.contains_key(&peer_id), "Peer should be removed from peers map");
	}

	#[tokio::test]
	async fn test_burst_of_250k_statements_allowed() {
		let (mut handler, _statement_store, network, _notification_service, _queue_receiver, _) =
			build_handler(1);

		let peer_id = *handler.peers.keys().next().unwrap();

		let mut statements = Vec::new();
		for i in 0..250_000 {
			let mut statement = new_live_statement();
			statement.set_plain_data(vec![
				i as u8,
				(i >> 8) as u8,
				(i >> 16) as u8,
				(i >> 24) as u8,
			]);
			statements.push(statement);
		}

		handler.on_statements(peer_id, statements);

		let reports = network.get_reports();
		assert!(
			!reports
				.iter()
				.any(|(id, rep)| *id == peer_id && *rep == rep::STATEMENT_FLOODING),
			"250k burst should be allowed (burst = rate × 5). Reports: {:?}",
			reports
		);

		assert!(
			handler.peers.contains_key(&peer_id),
			"Peer should still be connected after 250k burst"
		);
	}

	/// Like [`build_handler`], but every peer's rate limiter runs on the returned clock instead of
	/// on wall-clock time, so rate-limit behaviour spanning several batches is exact. The quota is
	/// the production one.
	fn build_handler_with_fake_clock(
		num_peers: usize,
	) -> (
		StatementHandler<TestNetwork, TestSync>,
		TestStatementStore,
		TestNetwork,
		TestNotificationService,
		async_channel::Receiver<(Statement, oneshot::Sender<SubmitResult>)>,
		Vec<PeerId>,
		FakeRelativeClock,
	) {
		let (mut handler, statement_store, network, notification_service, queue_receiver, peer_ids) =
			build_handler(num_peers);

		let clock = FakeRelativeClock::default();
		for peer in handler.peers.values_mut() {
			peer.statement_rate_limiter =
				PeerRateLimiter::with_clock(statements_per_second(), burst(), &clock);
		}

		(handler, statement_store, network, notification_service, queue_receiver, peer_ids, clock)
	}

	/// The production quota the handler is built with: 50k statements/sec, 250k burst.
	fn statements_per_second() -> NonZeroU32 {
		NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND)
			.expect("DEFAULT_STATEMENTS_PER_SECOND is nonzero")
	}

	fn burst() -> NonZeroU32 {
		NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND * STATEMENTS_BURST_COEFFICIENT)
			.expect("burst capacity is nonzero")
	}

	fn statement_batch(count: u32, counter: &mut u32) -> Statements {
		(0..count)
			.map(|i| {
				let mut statement = Statement::new();
				statement.set_plain_data(vec![
					*counter as u8,
					(*counter >> 8) as u8,
					(*counter >> 16) as u8,
					i as u8,
				]);
				*counter = counter.wrapping_add(1);
				statement
			})
			.collect()
	}

	/// A rate sustained above the quota must drain the bucket across successive batches and
	/// eventually get the peer reported and disconnected.
	#[tokio::test]
	async fn test_sustained_rate_above_limit_triggers_flooding() {
		let (
			mut handler,
			_statement_store,
			network,
			_notification_service,
			_queue_receiver,
			_,
			clock,
		) = build_handler_with_fake_clock(1);

		let peer_id = *handler.peers.keys().next().unwrap();
		let mut counter = 0u32;

		let flooding_reported = |network: &TestNetwork| {
			network
				.get_reports()
				.iter()
				.any(|(id, rep)| *id == peer_id && *rep == rep::STATEMENT_FLOODING)
		};

		for batch in 0..9 {
			handler.on_statements(peer_id, statement_batch(30_000, &mut counter));
			assert!(
				!flooding_reported(&network),
				"batch {batch} is still within the burst and must not be flagged",
			);
			clock.advance(Duration::from_millis(100));
		}

		handler.on_statements(peer_id, statement_batch(30_000, &mut counter));
		assert!(
			flooding_reported(&network),
			"the 10th batch overdraws the burst and must be flagged as flooding",
		);

		let disconnected = network.get_disconnected_peers();
		assert!(
			disconnected.contains(&peer_id),
			"Peer should be disconnected after sustained high rate. Disconnected: {:?}",
			disconnected
		);

		dispatch_disconnects(&mut handler, &network).await;

		assert!(!handler.peers.contains_key(&peer_id), "Peer should be removed from peers map");
	}

	#[tokio::test]
	async fn test_v2_peer_detected_when_no_fallback() {
		let (mut handler, _statement_store, _network, _notification_service) =
			build_handler_no_peers();

		let peer_id = PeerId::random();

		// No negotiated_fallback means the peer connected on the main protocol (v2).
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;

		assert_eq!(
			handler.peers.get(&peer_id).unwrap().protocol_version,
			PeerProtocolVersion::V2,
			"Peer should be detected as v2 when no fallback is negotiated"
		);
	}

	#[tokio::test]
	async fn test_v1_peer_detected_when_fallback_negotiated() {
		let (mut handler, _statement_store, _network, _notification_service) =
			build_handler_no_peers();

		let peer_id = PeerId::random();

		// negotiated_fallback is Some means the peer fell back to v1.
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: Some(format!("/{STATEMENT_PROTOCOL_V1}").into()),
			})
			.await;

		assert_eq!(
			handler.peers.get(&peer_id).unwrap().protocol_version,
			PeerProtocolVersion::V1,
			"Peer should be detected as v1 when fallback is negotiated"
		);
	}

	#[tokio::test]
	async fn test_v1_peer_decodes_raw_statements() {
		let (mut handler, _statement_store, _network, _notification_service) =
			build_handler_no_peers();

		let peer_id = PeerId::random();
		let (queue_sender, queue_receiver) = async_channel::bounded(10);
		handler.queue_sender = queue_sender;

		// Connect peer as v1 (with fallback).
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: Some(format!("/{STATEMENT_PROTOCOL_V1}").into()),
			})
			.await;

		// V1 peer sends raw Vec<Statement>.
		let mut statement = new_live_statement();
		statement.set_plain_data(b"v1 statement".to_vec());
		let hash = statement.hash();
		let raw_encoded = vec![statement].encode();

		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification: raw_encoded.into(),
			})
			.await;

		let (received, _) = queue_receiver.try_recv().unwrap();
		assert_eq!(received.hash(), hash, "V1 peer's raw statement should be decoded correctly");
	}

	#[tokio::test]
	async fn test_v2_peer_decodes_statement_message() {
		let (mut handler, _statement_store, _network, _notification_service) =
			build_handler_no_peers();

		let peer_id = PeerId::random();
		let (queue_sender, queue_receiver) = async_channel::bounded(10);
		handler.queue_sender = queue_sender;

		// Connect peer as v2 (no fallback).
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;

		// V2 peer sends StatementMessage::Statements.
		let mut statement = new_live_statement();
		statement.set_plain_data(b"v2 statement".to_vec());
		let hash = statement.hash();
		let msg = StatementMessage::Statements(vec![statement].try_into().unwrap());
		let encoded = msg.encode();

		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification: encoded.into(),
			})
			.await;

		let (received, _) = queue_receiver.try_recv().unwrap();
		assert_eq!(received.hash(), hash, "V2 peer's StatementMessage should be decoded correctly");
	}

	#[tokio::test]
	async fn test_v2_peer_topic_affinity_stored() {
		let (mut handler, _statement_store, _network, _notification_service) =
			build_handler_no_peers();

		let peer_id = PeerId::random();

		// Connect peer as v2.
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;

		assert!(
			handler.peers.get(&peer_id).unwrap().topic_affinity.is_none(),
			"Topic affinity should be None initially"
		);

		// Send ExplicitTopicAffinity message.
		let topic: [u8; 32] = [0xAA; 32];
		let mut filter = AffinityFilter::new(BLOOM_SEED, 0.01, 100);
		filter.insert(&topic);
		let msg = StatementMessage::ExplicitTopicAffinity(filter);
		let encoded = msg.encode();

		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification: encoded.into(),
			})
			.await;

		// Affinity is deferred; process it.
		handler.process_pending_affinities();

		let peer_data = handler.peers.get(&peer_id).unwrap();
		assert!(
			peer_data.topic_affinity.is_some(),
			"Topic affinity should be set after receiving ExplicitTopicAffinity"
		);
		// The filter should match the topic we inserted.
		assert!(
			peer_data.topic_affinity.as_ref().unwrap().contains(&topic),
			"Stored affinity filter should match the topic"
		);
	}

	#[tokio::test]
	async fn a_changed_filter_syncs_beyond_the_previous_one_and_a_repeated_filter_in_full() {
		let (mut handler, statement_store, _network, _notification_service) =
			build_handler_no_peers();
		statement_store.insert(new_live_statement());
		let peer_id = PeerId::random();
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;
		let already_served = |handler: &StatementHandler<TestNetwork, TestSync>| {
			handler
				.pending_initial_syncs
				.get(&peer_id)
				.expect("a sync is pending")
				.already_served
				.clone()
		};
		let complete_sync = |handler: &mut StatementHandler<TestNetwork, TestSync>| {
			handler.pending_initial_syncs.remove(&peer_id);
			handler.initial_sync_peer_queue.clear();
		};
		// With the gate off a full peer gets its sync on connect, ahead of any filter.
		complete_sync(&mut handler);

		send_affinity(&mut handler, peer_id, [0xAA; 32]).await;
		handler.process_pending_affinities();
		assert!(already_served(&handler).is_none(), "the first filter asks for everything");
		complete_sync(&mut handler);

		send_affinity(&mut handler, peer_id, [0xBB; 32]).await;
		handler.process_pending_affinities();
		let mut first = AffinityFilter::new(BLOOM_SEED, 0.01, 100);
		first.insert(&[0xAA; 32]);
		assert_eq!(already_served(&handler), Some(first), "a changed filter asks for the rest");
		complete_sync(&mut handler);

		send_affinity(&mut handler, peer_id, [0xBB; 32]).await;
		handler.process_pending_affinities();
		assert!(already_served(&handler).is_none(), "a repeated filter asks for everything");
	}

	#[tokio::test]
	async fn a_changed_filter_after_a_repeated_one_syncs_in_full() {
		let (mut handler, statement_store, _network, _notification_service) =
			build_handler_no_peers();
		statement_store.insert(new_live_statement());
		let peer_id = PeerId::random();
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;
		let already_served = |handler: &StatementHandler<TestNetwork, TestSync>| {
			handler
				.pending_initial_syncs
				.get(&peer_id)
				.expect("a sync is pending")
				.already_served
				.clone()
		};
		let complete_sync = |handler: &mut StatementHandler<TestNetwork, TestSync>| {
			handler.pending_initial_syncs.remove(&peer_id);
			handler.initial_sync_peer_queue.clear();
		};
		complete_sync(&mut handler);
		send_affinity(&mut handler, peer_id, [0xAA; 32]).await;
		handler.process_pending_affinities();
		complete_sync(&mut handler);

		// Both filters arrive within one tick.
		send_affinity(&mut handler, peer_id, [0xAA; 32]).await;
		send_affinity(&mut handler, peer_id, [0xBB; 32]).await;
		handler.process_pending_affinities();
		assert!(already_served(&handler).is_none());
		assert!(handler
			.peers
			.get(&peer_id)
			.unwrap()
			.topic_affinity
			.as_ref()
			.unwrap()
			.contains(&[0xBB; 32]));
		complete_sync(&mut handler);

		// The repeated filter starts a sync, and the changed one waits behind it.
		send_affinity(&mut handler, peer_id, [0xBB; 32]).await;
		handler.process_pending_affinities();
		assert!(already_served(&handler).is_none());
		send_affinity(&mut handler, peer_id, [0xCC; 32]).await;
		complete_sync(&mut handler);
		handler.process_pending_affinities();
		let mut second = AffinityFilter::new(BLOOM_SEED, 0.01, 100);
		second.insert(&[0xBB; 32]);
		assert_eq!(already_served(&handler), Some(second), "the replay already ran");
		complete_sync(&mut handler);

		// The repeated filter waits behind a running sync, and the changed one replaces it.
		send_affinity(&mut handler, peer_id, [0xDD; 32]).await;
		handler.process_pending_affinities();
		send_affinity(&mut handler, peer_id, [0xDD; 32]).await;
		send_affinity(&mut handler, peer_id, [0xEE; 32]).await;
		complete_sync(&mut handler);
		handler.process_pending_affinities();
		assert!(already_served(&handler).is_none());
	}

	#[tokio::test]
	async fn a_changed_filter_waits_for_the_running_sync() {
		let (mut handler, statement_store, _network, _notification_service) =
			build_handler_no_peers();
		statement_store.insert(new_live_statement());
		let peer_id = PeerId::random();
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;
		handler.pending_initial_syncs.remove(&peer_id);
		handler.initial_sync_peer_queue.clear();

		send_affinity(&mut handler, peer_id, [0xAA; 32]).await;
		handler.process_pending_affinities();
		let first_sync_id = handler.pending_initial_syncs.get(&peer_id).unwrap().sync_id;

		// The sync for the first filter has sent nothing yet, so the next filter waits: a sync
		// that skips the first filter's matches is sound only once they were all delivered.
		send_affinity(&mut handler, peer_id, [0xBB; 32]).await;
		handler.process_pending_affinities();
		let peer = handler.peers.get(&peer_id).unwrap();
		assert!(peer.topic_affinity.as_ref().unwrap().contains(&[0xAA; 32]));
		assert!(peer.pending_topic_affinity.is_some());
		assert_eq!(handler.pending_initial_syncs.get(&peer_id).unwrap().sync_id, first_sync_id);

		handler.pending_initial_syncs.remove(&peer_id);
		handler.initial_sync_peer_queue.clear();
		handler.process_pending_affinities();
		let peer = handler.peers.get(&peer_id).unwrap();
		assert!(peer.topic_affinity.as_ref().unwrap().contains(&[0xBB; 32]));
		assert!(peer.pending_topic_affinity.is_none());
		assert!(handler.pending_initial_syncs.get(&peer_id).unwrap().already_served.is_some());
	}

	#[tokio::test]
	async fn a_changed_filter_stays_pending_while_the_watermark_is_unreadable() {
		let (mut handler, statement_store, _network, _notification_service) =
			build_handler_no_peers();
		statement_store.insert(new_live_statement());
		let peer_id = PeerId::random();
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;
		handler.pending_initial_syncs.remove(&peer_id);
		handler.initial_sync_peer_queue.clear();

		send_affinity(&mut handler, peer_id, [0xAA; 32]).await;
		handler.process_pending_affinities();
		handler.pending_initial_syncs.remove(&peer_id);
		handler.initial_sync_peer_queue.clear();

		statement_store.fail_fetches.store(true, Ordering::Relaxed);
		send_affinity(&mut handler, peer_id, [0xBB; 32]).await;
		handler.process_pending_affinities();
		let peer = handler.peers.get(&peer_id).unwrap();
		assert!(peer.topic_affinity.as_ref().unwrap().contains(&[0xAA; 32]));
		assert!(peer.pending_topic_affinity.is_some());
		assert!(!handler.pending_initial_syncs.contains_key(&peer_id));

		statement_store.fail_fetches.store(false, Ordering::Relaxed);
		handler.process_pending_affinities();
		let mut first = AffinityFilter::new(BLOOM_SEED, 0.01, 100);
		first.insert(&[0xAA; 32]);
		assert_eq!(
			handler.pending_initial_syncs.get(&peer_id).unwrap().already_served,
			Some(first),
			"the retry skips only what the last served filter matched"
		);
	}

	#[tokio::test]
	async fn a_full_v2_peer_is_served_without_the_dht_path() {
		let (mut handler, statement_store, _network, notification_service) =
			build_handler_no_peers();
		statement_store.insert(new_live_statement());
		let peer_id = PeerId::random();
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;
		handler.pending_initial_syncs.remove(&peer_id);
		handler.initial_sync_peer_queue.clear();
		notification_service.clear_sent_notifications();

		// With the gate off a connection never leads here, so the call is made by hand.
		handler.serve_without_dht(peer_id);

		let peer = handler.peers.get(&peer_id).unwrap();
		assert!(peer.topic_affinity.as_ref().is_some_and(|filter| filter.contains(&[0xAA; 32])));
		assert!(handler.pending_initial_syncs.contains_key(&peer_id));
		assert!(notification_service.get_sent_notifications().is_empty(), "it gets no filter");

		// A filter the peer sends later narrows what it gets.
		handler.pending_initial_syncs.remove(&peer_id);
		handler.initial_sync_peer_queue.clear();
		send_affinity(&mut handler, peer_id, [0xAA; 32]).await;
		handler.process_pending_affinities();

		assert!(notification_service.get_sent_notifications().is_empty(), "no filter in return");
		let filter = handler.peers.get(&peer_id).unwrap().topic_affinity.as_ref().unwrap();
		assert!(filter.contains(&[0xAA; 32]));
		assert!(!filter.contains(&[0xBB; 32]));
	}

	#[tokio::test]
	async fn a_filter_broadcast_skips_a_peer_without_dht() {
		let (mut handler, _statement_store, _network, notification_service) =
			build_handler_no_peers();
		let without_dht = PeerId::random();
		let with_dht = PeerId::random();
		let light = PeerId::random();
		for peer in [without_dht, with_dht, light] {
			handler
				.handle_notification_event(NotificationEvent::NotificationStreamOpened {
					peer,
					direction: sc_network::service::traits::Direction::Inbound,
					handshake: vec![],
					negotiated_fallback: None,
				})
				.await;
		}
		handler.peers.get_mut(&with_dht).unwrap().protocol_version = PeerProtocolVersion::V3;
		handler.peers.get_mut(&light).unwrap().is_light = true;
		notification_service.clear_sent_notifications();

		let mut filter = AffinityFilter::new(BLOOM_SEED, 0.01, 100);
		filter.insert(&[0xAA; 32]);
		handler.broadcast_local_filter(filter.clone()).await;

		let encoded = StatementMessage::ExplicitTopicAffinity(filter).encode();
		let sent: HashSet<_> = notification_service.get_sent_notifications().into_iter().collect();
		assert_eq!(sent, HashSet::from([(with_dht, encoded.clone()), (light, encoded)]));
	}

	#[tokio::test]
	async fn a_new_peer_without_dht_gets_no_local_filter() {
		let (mut handler, _statement_store, _network, notification_service) =
			build_handler_no_peers();
		let [v1, full_v2, light_v2, v3] = [(); 4].map(|_| PeerId::random());
		for peer in [v1, full_v2, light_v2, v3] {
			handler
				.handle_notification_event(NotificationEvent::NotificationStreamOpened {
					peer,
					direction: sc_network::service::traits::Direction::Inbound,
					handshake: vec![],
					negotiated_fallback: None,
				})
				.await;
		}
		handler.peers.get_mut(&v1).unwrap().protocol_version = PeerProtocolVersion::V1;
		handler.peers.get_mut(&light_v2).unwrap().is_light = true;
		handler.peers.get_mut(&v3).unwrap().protocol_version = PeerProtocolVersion::V3;
		notification_service.clear_sent_notifications();

		for peer in [v1, full_v2, light_v2, v3, PeerId::random()] {
			handler.send_local_filter(&peer).await;
		}

		let local =
			StatementMessage::ExplicitTopicAffinity(handler.dht.advertised_filter()).encode();
		let sent: HashSet<_> = notification_service.get_sent_notifications().into_iter().collect();
		assert_eq!(sent, HashSet::from([(light_v2, local.clone()), (v3, local)]));
	}

	#[test]
	fn identify_indexes_only_a_peer_that_lists_the_main_protocol() {
		let (mut handler, _statement_store, _network, _notification_service) =
			build_handler_no_peers();
		let main: ProtocolName = format!("/aa/{STATEMENT_PROTOCOL_V3}").into();
		let fallbacks: HashSet<ProtocolName> = HashSet::from([
			format!("/aa/{STATEMENT_PROTOCOL_V2}").into(),
			format!("/aa/{STATEMENT_PROTOCOL_V1}").into(),
		]);
		handler.protocol_name = main.clone();
		// A lone node is the replica of the topic at a peer's key until it indexes that peer.
		let remote = PeerId::random();
		handler.dht = DhtOrchestrator::new(
			&[],
			None,
			crate::config::DEFAULT_BLOOM_FALSE_POS_RATE,
			PeerId::random(),
			topology_config(1, 1),
			main.clone(),
			None,
		);
		let topic = Topic(sp_crypto_hashing::blake2_256(&remote.to_bytes()));

		handler.handle_network_event(Event::PeerIdentified {
			peer: remote,
			supported_protocols: fallbacks.clone(),
		});
		assert!(handler.dht.is_replica_for(&[topic]), "a peer without statement/3 is not indexed");

		let mut with_main = fallbacks;
		with_main.insert(main);
		handler.handle_network_event(Event::PeerIdentified {
			peer: remote,
			supported_protocols: with_main,
		});
		assert!(!handler.dht.is_replica_for(&[topic]));
	}

	#[test]
	fn the_negotiated_protocol_tells_the_peer_version() {
		let v1 = format!("/genesis/{STATEMENT_PROTOCOL_V1}");
		let v2 = format!("/genesis/{STATEMENT_PROTOCOL_V2}");
		for dht in [false, true] {
			assert_eq!(PeerProtocolVersion::negotiated(Some(&v1), dht), PeerProtocolVersion::V1);
		}
		assert_eq!(PeerProtocolVersion::negotiated(Some(&v2), true), PeerProtocolVersion::V2);
		assert_eq!(PeerProtocolVersion::negotiated(None, true), PeerProtocolVersion::V3);
		assert_eq!(PeerProtocolVersion::negotiated(None, false), PeerProtocolVersion::V2);
	}

	#[test]
	fn only_a_node_on_the_dht_path_offers_statement_3() {
		let (main, fallbacks) = protocol_names("aa", None, false);
		assert_eq!(main, "/aa/statement/2");
		assert_eq!(fallbacks, vec!["/aa/statement/1"]);

		let (main, fallbacks) = protocol_names("aa", Some("fork"), true);
		assert_eq!(main, "/aa/fork/statement/3");
		assert_eq!(fallbacks, vec!["/aa/fork/statement/2", "/aa/fork/statement/1"]);
	}

	#[test]
	fn full_peers_below_statement_3_flood() {
		let burst = NonZeroU32::new(1).unwrap();
		let mut peer = Peer::new_for_testing(burst, burst);
		assert!(peer.without_dht(), "a v1 peer floods");

		peer.protocol_version = PeerProtocolVersion::V2;
		assert!(peer.without_dht(), "a full v2 peer floods");
		peer.is_light = true;
		assert!(!peer.without_dht(), "a light v2 peer is served by its filter");

		peer.is_light = false;
		peer.protocol_version = PeerProtocolVersion::V3;
		assert!(!peer.without_dht());
	}

	async fn send_affinity(
		handler: &mut StatementHandler<TestNetwork, TestSync>,
		peer_id: PeerId,
		topic: [u8; 32],
	) {
		let mut filter = AffinityFilter::new(BLOOM_SEED, 0.01, 100);
		filter.insert(&topic);
		let notification = StatementMessage::ExplicitTopicAffinity(filter).encode().into();
		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification,
			})
			.await;
	}

	#[test]
	fn the_wanted_bitmap_sets_the_lowest_bit_of_a_byte_first() {
		let hashes: Vec<ShortHash> = (0..10u8).map(|n| [n; 8]).collect();

		let bitmap = wanted_bitmap(&hashes, |hash| [0, 9].contains(&hash[0]));

		assert_eq!(bitmap, vec![0b0000_0001, 0b0000_0010]);
		assert!(wanted_bitmap(&[], |_| true).is_empty());
		let wanted: Vec<usize> = (0..12).filter(|position| is_wanted(&bitmap, *position)).collect();
		assert_eq!(wanted, vec![0, 9], "positions past the bitmap are not wanted");
	}

	#[test]
	fn announce_and_want_encoding_snapshot() {
		let announce =
			StatementMessage::Announce { id: 1, hashes: vec![[0xAA; 8]].try_into().unwrap() };
		let mut expected = vec![2, 1, 0, 0, 0, 0, 0, 0, 0, 4];
		expected.extend([0xAA; 8]);
		assert_eq!(announce.encode(), expected);

		let want = StatementMessage::Want { id: 1, wanted: vec![0b101].try_into().unwrap() };
		assert_eq!(want.encode(), vec![3, 1, 0, 0, 0, 0, 0, 0, 0, 4, 0b101]);
	}

	#[tokio::test]
	async fn an_announce_is_answered_with_the_statements_the_node_lacks() {
		let (mut handler, statement_store, _network, notification_service) =
			build_handler_no_peers();
		handler.metrics = Some(Metrics::register(&Registry::new()).unwrap());
		let peer_id = PeerId::random();
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;
		notification_service.clear_sent_notifications();
		let stored = new_live_statement();
		statement_store.insert(stored.clone());
		let (in_validation, lacking) = ([0xBB; 8], [0xCC; 8]);
		handler.short_hashes_in_validation.insert(in_validation);

		handler.on_announce(peer_id, 7, vec![short_hash(&stored.hash()), in_validation, lacking]);
		handler.flush_pending_sends().await;

		let want = StatementMessage::Want { id: 7, wanted: vec![0b100].try_into().unwrap() };
		assert_eq!(notification_service.get_sent_notifications(), vec![(peer_id, want.encode())]);
		let metrics = handler.metrics.as_ref().unwrap();
		assert_eq!(metrics.announced_hashes_received.get(), 3);
		assert_eq!(metrics.announced_hashes_wanted.get(), 1);
	}

	#[tokio::test]
	async fn an_announce_past_the_rate_limit_gets_no_answer() {
		let (mut handler, _statement_store, network, notification_service) =
			build_handler_no_peers();
		let peer_id = PeerId::random();
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;
		notification_service.clear_sent_notifications();
		let two = NonZeroU32::new(2).unwrap();
		handler.peers.get_mut(&peer_id).unwrap().announce_rate_limiter =
			PeerRateLimiter::with_clock(two, two, &FakeRelativeClock::default());

		handler.on_announce(peer_id, 1, vec![[1; 8], [2; 8]]);
		handler.flush_pending_sends().await;
		assert_eq!(notification_service.get_sent_notifications().len(), 1, "within the quota");

		handler.on_announce(peer_id, 2, vec![[3; 8]]);
		handler.flush_pending_sends().await;
		assert_eq!(notification_service.get_sent_notifications().len(), 1);
		assert!(network.get_reports().contains(&(peer_id, rep::ANNOUNCE_FLOODING)));
	}

	#[tokio::test]
	async fn announce_and_want_are_bad_messages_off_the_dht_path() {
		let (mut handler, _statement_store, network, notification_service) =
			build_handler_no_peers();
		let peer_id = PeerId::random();
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;
		notification_service.clear_sent_notifications();
		let messages = [
			StatementMessage::Announce { id: 1, hashes: vec![[1; 8]].try_into().unwrap() },
			StatementMessage::Want { id: 1, wanted: vec![1].try_into().unwrap() },
		];

		for message in messages {
			handler
				.handle_notification_event(NotificationEvent::NotificationReceived {
					peer: peer_id,
					notification: message.encode().into(),
				})
				.await;
		}

		let bad_messages = network
			.get_reports()
			.iter()
			.filter(|report| **report == (peer_id, rep::BAD_MESSAGE))
			.count();
		assert_eq!(bad_messages, 2);
		assert!(notification_service.get_sent_notifications().is_empty());
	}

	/// A handler with one full `statement/3` peer whose sync announces, and the statements on
	/// `topics` in its store, in the order of the store's index by topic.
	async fn handler_with_announced_sync(
		topics: &[Topic],
	) -> (
		StatementHandler<TestNetwork, TestSync>,
		TestNetwork,
		TestNotificationService,
		PeerId,
		Vec<Statement>,
	) {
		let (mut handler, statement_store, network, notification_service) =
			build_handler_no_peers();
		handler.metrics = Some(Metrics::register(&Registry::new()).unwrap());
		let peer_id = PeerId::random();
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;
		notification_service.clear_sent_notifications();
		handler.peers.get_mut(&peer_id).unwrap().protocol_version = PeerProtocolVersion::V3;
		let mut statements: Vec<Statement> = topics
			.iter()
			.map(|topic| {
				let mut statement = new_live_statement();
				statement.set_topic(0, *topic);
				statement
			})
			.collect();
		statements.sort_unstable_by_key(|statement| (statement.topics()[0], statement.hash()));
		for statement in &statements {
			statement_store.insert(statement.clone());
		}
		start_announced_sync(&mut handler, peer_id, None);
		(handler, network, notification_service, peer_id, statements)
	}

	const ANNOUNCED_SYNC_ID: u64 = 7;

	fn start_announced_sync(
		handler: &mut StatementHandler<TestNetwork, TestSync>,
		peer_id: PeerId,
		already_served: Option<AffinityFilter>,
	) {
		handler.pending_initial_syncs.insert(
			peer_id,
			PendingInitialSync {
				cursor: 0,
				watermark: 0,
				started_at: Instant::now(),
				sync_id: ANNOUNCED_SYNC_ID,
				already_served,
				announce: Some(AnnounceSync::new()),
			},
		);
		handler.initial_sync_peer_queue.push_back(peer_id);
	}

	/// Run one sync burst and let its send finish.
	async fn sync_burst(handler: &mut StatementHandler<TestNetwork, TestSync>) {
		handler.process_initial_sync_burst();
		handler.flush_pending_sends().await;
	}

	fn announce_of(id: u64, statements: &[&Statement]) -> Vec<u8> {
		let hashes: Vec<ShortHash> =
			statements.iter().map(|statement| short_hash(&statement.hash())).collect();
		StatementMessage::Announce { id, hashes: hashes.try_into().unwrap() }.encode()
	}

	#[tokio::test]
	async fn an_announced_sync_sends_only_the_statements_the_peer_asks_for() {
		let (mut handler, _network, notification_service, peer_id, statements) =
			handler_with_announced_sync(&[topic(1), topic(2), topic(3)]).await;
		let id = ANNOUNCED_SYNC_ID << 32;

		sync_burst(&mut handler).await;
		assert_eq!(
			notification_service.get_sent_notifications(),
			vec![(peer_id, announce_of(id, &statements.iter().collect::<Vec<_>>()))],
		);

		// No answer yet, so a burst sends nothing.
		notification_service.clear_sent_notifications();
		sync_burst(&mut handler).await;
		assert!(notification_service.get_sent_notifications().is_empty());

		handler.on_want(peer_id, id, vec![0b010]);
		sync_burst(&mut handler).await;
		assert_eq!(
			notification_service.get_sent_notifications(),
			vec![(peer_id, StatementMessage::encode_statement_refs(&[&statements[1]]))],
		);

		sync_burst(&mut handler).await;
		assert!(!handler.pending_initial_syncs.contains_key(&peer_id), "the index is walked");
		let metrics = handler.metrics.as_ref().unwrap();
		assert_eq!(metrics.announced_hashes_sent.get(), 3);
		assert_eq!(metrics.announced_hashes_requested.get(), 1);
	}

	#[tokio::test]
	async fn an_announced_sync_announces_only_the_topics_due_to_the_peer() {
		let (mut handler, _network, notification_service, peer_id, statements) =
			handler_with_announced_sync(&[topic(1), topic(2), topic(3)]).await;
		handler.peers.get_mut(&peer_id).unwrap().topic_affinity =
			Some(filter_over(&[topic(2), topic(3)]));
		// The previous filter matched topic 3, so its statement was served already.
		handler.pending_initial_syncs.clear();
		handler.initial_sync_peer_queue.clear();
		start_announced_sync(&mut handler, peer_id, Some(filter_over(&[topic(3)])));

		sync_burst(&mut handler).await;

		assert_eq!(
			notification_service.get_sent_notifications(),
			vec![(peer_id, announce_of(ANNOUNCED_SYNC_ID << 32, &[&statements[1]]))],
		);
	}

	#[tokio::test]
	async fn an_announced_sync_leaves_out_what_the_peer_supplied() {
		let (mut handler, _network, notification_service, peer_id, statements) =
			handler_with_announced_sync(&[topic(1), topic(2)]).await;
		handler
			.recently_received_statements
			.insert(statements[0].hash(), HashSet::from_iter([peer_id]));

		sync_burst(&mut handler).await;

		assert_eq!(
			notification_service.get_sent_notifications(),
			vec![(peer_id, announce_of(ANNOUNCED_SYNC_ID << 32, &[&statements[1]]))],
		);
	}

	#[tokio::test]
	async fn a_failed_announce_goes_out_again() {
		let (mut handler, _network, notification_service, peer_id, statements) =
			handler_with_announced_sync(&[topic(1)]).await;
		notification_service.fail_sends();
		sync_burst(&mut handler).await;
		assert!(notification_service.get_sent_notifications().is_empty());

		notification_service.allow_sends();
		sync_burst(&mut handler).await;

		assert_eq!(
			notification_service.get_sent_notifications(),
			vec![(peer_id, announce_of(ANNOUNCED_SYNC_ID << 32, &[&statements[0]]))],
		);
	}

	#[tokio::test]
	async fn wanted_statements_go_out_again_after_a_failed_send() {
		let (mut handler, _network, notification_service, peer_id, statements) =
			handler_with_announced_sync(&[topic(1), topic(2), topic(3)]).await;
		sync_burst(&mut handler).await;
		notification_service.clear_sent_notifications();
		handler.on_want(peer_id, ANNOUNCED_SYNC_ID << 32, vec![0b111]);

		notification_service.fail_sends();
		sync_burst(&mut handler).await;
		assert!(notification_service.get_sent_notifications().is_empty());
		notification_service.allow_sends();
		sync_burst(&mut handler).await;

		let wanted: Vec<&Statement> = statements.iter().collect();
		assert_eq!(
			notification_service.get_sent_notifications(),
			vec![(peer_id, StatementMessage::encode_statement_refs(&wanted))],
		);
		sync_burst(&mut handler).await;
		assert!(!handler.pending_initial_syncs.contains_key(&peer_id));
	}

	#[tokio::test]
	async fn an_announced_sync_resumes_where_the_previous_announce_stopped() {
		let topics: Vec<Topic> = (0..=MAX_ANNOUNCED_HASHES as u16)
			.map(|n| {
				let mut topic = [0; 32];
				topic[..2].copy_from_slice(&n.to_be_bytes());
				Topic(topic)
			})
			.collect();
		let (mut handler, _network, notification_service, peer_id, statements) =
			handler_with_announced_sync(&topics).await;
		// Room for both announces at once, so the test does not depend on the clock.
		handler.peers.get_mut(&peer_id).unwrap().announce_budget =
			PeerRateLimiter::for_received_announces();
		sync_burst(&mut handler).await;
		notification_service.clear_sent_notifications();

		handler.on_want(peer_id, ANNOUNCED_SYNC_ID << 32, vec![0; MAX_ANNOUNCED_HASHES / 8]);
		sync_burst(&mut handler).await;

		let last = statements.last().unwrap();
		assert_eq!(
			notification_service.get_sent_notifications(),
			vec![(peer_id, announce_of((ANNOUNCED_SYNC_ID << 32) | 1, &[last]))],
		);
	}

	#[tokio::test]
	async fn a_statement_counts_as_held_while_it_waits_for_validation() {
		let (mut handler, statement_store, _network, _notification_service, queue_receiver, peers) =
			build_handler(1);
		let statement = new_live_statement();
		let short = short_hash(&statement.hash());

		handler.on_statements(peers[0], vec![statement]);
		assert!(handler.short_hashes_in_validation.contains(&short));

		import_queued_statement(&mut handler, &statement_store, &queue_receiver, SubmitResult::New)
			.await;
		assert!(handler.short_hashes_in_validation.is_empty());
	}

	#[tokio::test]
	async fn a_sync_ends_when_its_announce_stays_unanswered() {
		let (mut handler, _network, _notification_service, peer_id, _statements) =
			handler_with_announced_sync(&[topic(1)]).await;
		sync_burst(&mut handler).await;

		handler.end_unanswered_announce_syncs();
		assert!(handler.pending_initial_syncs.contains_key(&peer_id), "the answer is not late yet");

		let awaited = handler
			.pending_initial_syncs
			.get_mut(&peer_id)
			.and_then(|pending| pending.announce.as_mut())
			.and_then(|announce| announce.awaiting.as_mut())
			.unwrap();
		awaited.sent_at = Instant::now().checked_sub(ANNOUNCE_ANSWER_TIMEOUT);
		handler.end_unanswered_announce_syncs();

		assert!(!handler.pending_initial_syncs.contains_key(&peer_id));
	}

	#[tokio::test]
	async fn an_answer_of_the_wrong_length_ends_the_sync() {
		let (mut handler, network, _notification_service, peer_id, _statements) =
			handler_with_announced_sync(&[topic(1)]).await;
		sync_burst(&mut handler).await;

		handler.on_want(peer_id, ANNOUNCED_SYNC_ID << 32, vec![1, 0]);

		assert!(network.get_reports().contains(&(peer_id, rep::BAD_MESSAGE)));
		assert!(!handler.pending_initial_syncs.contains_key(&peer_id));
	}

	#[tokio::test]
	async fn an_answer_to_no_awaited_announce_is_ignored() {
		let (mut handler, network, notification_service, peer_id, _statements) =
			handler_with_announced_sync(&[topic(1)]).await;
		sync_burst(&mut handler).await;
		notification_service.clear_sent_notifications();

		handler.on_want(peer_id, 1, vec![1]);
		sync_burst(&mut handler).await;

		assert!(!network.get_reports().contains(&(peer_id, rep::BAD_MESSAGE)));
		assert!(notification_service.get_sent_notifications().is_empty());
		assert!(handler.pending_initial_syncs.contains_key(&peer_id), "the sync still waits");
	}

	#[tokio::test]
	async fn affinity_updates_are_rate_limited_per_peer() {
		let (mut handler, _statement_store, network, _notification_service) =
			build_handler_no_peers();
		let peer_id = PeerId::random();
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;
		let clock = FakeRelativeClock::default();
		handler.peers.get_mut(&peer_id).unwrap().affinity_rate_limiter =
			PeerRateLimiter::with_clock(
				config::AFFINITY_UPDATES_PER_SECOND,
				config::AFFINITY_UPDATES_BURST,
				&clock,
			);
		let flooding_reports = || {
			network
				.get_reports()
				.iter()
				.filter(|(id, rep)| *id == peer_id && *rep == rep::AFFINITY_FLOODING)
				.count()
		};

		let nth_topic = |n: u32| {
			let mut topic = [0u8; 32];
			topic[..4].copy_from_slice(&n.to_le_bytes());
			topic
		};

		let burst = config::AFFINITY_UPDATES_BURST.get();
		for i in 0..=burst {
			send_affinity(&mut handler, peer_id, nth_topic(i)).await;
		}
		assert_eq!(flooding_reports(), 1, "only the update past the burst is reported");
		assert!(!network.get_disconnected_peers().contains(&peer_id));
		handler.process_pending_affinities();
		let affinity = handler.peers[&peer_id].topic_affinity.clone().unwrap();
		assert!(affinity.contains(&nth_topic(burst - 1)), "last accepted update is applied");
		assert!(!affinity.contains(&nth_topic(burst)), "dropped update is not applied");

		clock.advance(Duration::from_secs(1));
		send_affinity(&mut handler, peer_id, nth_topic(burst)).await;
		assert_eq!(flooding_reports(), 1, "the refilled token admits the next update");
		handler.process_pending_affinities();
		let affinity = handler.peers[&peer_id].topic_affinity.clone().unwrap();
		assert!(affinity.contains(&nth_topic(burst)), "update after the refill is applied");
	}

	#[tokio::test]
	async fn test_topic_affinity_filters_propagation() {
		let (mut handler, statement_store, _network, notification_service) =
			build_handler_no_peers();

		let peer_id = PeerId::random();

		// Connect peer as v2.
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;

		// Set up topic affinity: peer is interested in topic 0xAA only.
		let topic_aa: [u8; 32] = [0xAA; 32];
		let topic_bb: [u8; 32] = [0xBB; 32];
		let mut filter = AffinityFilter::new(BLOOM_SEED, 0.01, 100);
		filter.insert(&topic_aa);
		let msg = StatementMessage::ExplicitTopicAffinity(filter);
		let encoded = msg.encode();
		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification: encoded.into(),
			})
			.await;

		// Affinity is deferred; process it.
		handler.process_pending_affinities();

		// Create statements: one matching, one not matching.
		let mut stmt_matching = new_live_statement();
		stmt_matching.set_plain_data(b"matching".to_vec());
		stmt_matching.set_topic(0, topic_aa.into());
		let hash_matching = stmt_matching.hash();

		let mut stmt_not_matching = new_live_statement();
		stmt_not_matching.set_plain_data(b"not matching".to_vec());
		stmt_not_matching.set_topic(0, topic_bb.into());
		let hash_not_matching = stmt_not_matching.hash();

		statement_store
			.recent_statements
			.lock()
			.unwrap()
			.insert(hash_matching, stmt_matching);
		statement_store
			.recent_statements
			.lock()
			.unwrap()
			.insert(hash_not_matching, stmt_not_matching);

		handler.propagate_statements().await;
		handler.flush_pending_sends().await;

		let sent = notification_service.get_sent_notifications();
		let mut sent_hashes: Vec<_> = sent
			.iter()
			.flat_map(|(_, notification)| {
				// V2 peer gets StatementMessage encoding.
				match StatementMessage::decode(&mut notification.as_slice()).unwrap() {
					StatementMessage::Statements(stmts) => stmts,
					_ => panic!("Expected StatementMessage::Statements"),
				}
			})
			.map(|s| s.hash())
			.collect();
		sent_hashes.sort();

		// The matching statement should be sent, the non-matching one filtered.
		assert!(
			sent_hashes.contains(&hash_matching),
			"Statement matching topic affinity should be propagated"
		);
		assert!(
			!sent_hashes.contains(&hash_not_matching),
			"Statement NOT matching topic affinity should be filtered out"
		);
	}

	#[tokio::test]
	async fn test_v1_peer_no_topic_filtering() {
		let (mut handler, statement_store, _network, notification_service) =
			build_handler_no_peers();

		let peer_id = PeerId::random();

		// Connect peer as v1 (with fallback).
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: Some(format!("/{STATEMENT_PROTOCOL_V1}").into()),
			})
			.await;

		// V1 peers have no topic affinity - all statements should be propagated.
		let topic_aa: [u8; 32] = [0xAA; 32];
		let mut stmt_with_topic = new_live_statement();
		stmt_with_topic.set_plain_data(b"with topic".to_vec());
		stmt_with_topic.set_topic(0, topic_aa.into());
		let hash_with_topic = stmt_with_topic.hash();

		let topic_bb: [u8; 32] = [0xBB; 32];
		let mut stmt_other_topic = new_live_statement();
		stmt_other_topic.set_plain_data(b"other topic".to_vec());
		stmt_other_topic.set_topic(0, topic_bb.into());
		let hash_other_topic = stmt_other_topic.hash();

		statement_store
			.recent_statements
			.lock()
			.unwrap()
			.insert(hash_with_topic, stmt_with_topic);
		statement_store
			.recent_statements
			.lock()
			.unwrap()
			.insert(hash_other_topic, stmt_other_topic);

		handler.propagate_statements().await;
		handler.flush_pending_sends().await;

		let sent = notification_service.get_sent_notifications();
		let sent_hashes: Vec<_> = sent
			.iter()
			.flat_map(|(_, notification)| {
				<Statements as Decode>::decode(&mut notification.as_slice()).unwrap()
			})
			.map(|s| s.hash())
			.collect();

		assert_eq!(
			sent_hashes.len(),
			2,
			"V1 peer should receive all statements regardless of topics"
		);
		assert!(sent_hashes.contains(&hash_with_topic));
		assert!(sent_hashes.contains(&hash_other_topic));
	}

	#[tokio::test]
	async fn test_affinity_change_triggers_resync() {
		let (mut handler, statement_store, _network, notification_service) =
			build_handler_no_peers_light();

		let peer_id = PeerId::random();

		// Add statements with different topics to the store.
		let topic_aa: [u8; 32] = [0xAA; 32];
		let topic_bb: [u8; 32] = [0xBB; 32];

		let mut stmt_aa = new_live_statement();
		stmt_aa.set_plain_data(b"stmt_aa".to_vec());
		stmt_aa.set_topic(0, topic_aa.into());
		let hash_aa = stmt_aa.hash();

		let mut stmt_bb = new_live_statement();
		stmt_bb.set_plain_data(b"stmt_bb".to_vec());
		stmt_bb.set_topic(0, topic_bb.into());
		let hash_bb = stmt_bb.hash();

		let mut stmt_both = new_live_statement();
		stmt_both.set_plain_data(b"stmt_both".to_vec());
		stmt_both.set_topic(0, topic_aa.into());
		stmt_both.set_topic(1, topic_bb.into());
		let hash_both = stmt_both.hash();

		statement_store.insert(stmt_aa);
		statement_store.insert(stmt_bb);
		statement_store.insert(stmt_both);

		// Connect peer as v2.
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;

		// Light V2 peers should NOT get initial sync on connect (must set affinity first).
		assert!(
			!handler.pending_initial_syncs.contains_key(&peer_id),
			"Light V2 peer should NOT have initial sync scheduled on connect"
		);

		// Set topic affinity to topic_aa — this triggers the first initial sync.
		let mut filter = AffinityFilter::new(BLOOM_SEED, 0.01, 100);
		filter.insert(&topic_aa);
		let msg = StatementMessage::ExplicitTopicAffinity(filter);
		let encoded = msg.encode();
		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification: encoded.into(),
			})
			.await;

		// Affinity is deferred; process it.
		handler.process_pending_affinities();

		assert!(
			handler.pending_initial_syncs.contains_key(&peer_id),
			"Initial sync should be scheduled after setting affinity"
		);

		// Drain initial sync — only stmt_aa and stmt_both should be sent.
		while handler.pending_initial_syncs.contains_key(&peer_id) {
			handler.process_initial_sync_burst();
			handler.flush_pending_sends().await;
		}

		let sent = notification_service.get_sent_notifications();
		let sent_hashes: HashSet<_> = sent
			.iter()
			.flat_map(|(_, notification)| {
				match StatementMessage::decode(&mut notification.as_slice()).unwrap() {
					StatementMessage::Statements(stmts) => stmts,
					_ => panic!("Expected StatementMessage::Statements"),
				}
			})
			.map(|s| s.hash())
			.collect();
		assert!(sent_hashes.contains(&hash_aa), "stmt_aa should be sent (matches affinity)");
		assert!(sent_hashes.contains(&hash_both), "stmt_both should be sent (matches affinity)");
		assert!(!sent_hashes.contains(&hash_bb), "stmt_bb should NOT be sent (filtered)");

		// Now change affinity to topic_bb — triggers re-sync.
		let mut filter = AffinityFilter::new(BLOOM_SEED, 0.01, 100);
		filter.insert(&topic_bb);
		let msg = StatementMessage::ExplicitTopicAffinity(filter);
		let encoded = msg.encode();
		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification: encoded.into(),
			})
			.await;

		// Affinity is deferred; process it.
		handler.process_pending_affinities();

		assert!(
			handler.pending_initial_syncs.contains_key(&peer_id),
			"Initial sync should be re-scheduled after affinity change"
		);

		notification_service.clear_sent_notifications();
		while handler.pending_initial_syncs.contains_key(&peer_id) {
			handler.process_initial_sync_burst();
			handler.flush_pending_sends().await;
		}

		let sent_after_bb = notification_service.get_sent_notifications();
		let sent_hashes_bb: HashSet<_> = sent_after_bb
			.iter()
			.flat_map(|(_, notification)| {
				match StatementMessage::decode(&mut notification.as_slice()).unwrap() {
					StatementMessage::Statements(stmts) => stmts,
					_ => panic!("Expected StatementMessage::Statements"),
				}
			})
			.map(|s| s.hash())
			.collect();
		// stmt_bb was previously filtered and should now be sent.
		assert!(
			sent_hashes_bb.contains(&hash_bb),
			"stmt_bb should now be sent after affinity changed to topic_bb"
		);
		// The first sync delivered the statement on both topics under the previous filter.
		assert!(
			!sent_hashes_bb.contains(&hash_both),
			"stmt_both should not be re-sent after the affinity change"
		);
	}

	#[tokio::test]
	async fn test_affinity_change_sends_previously_filtered_statements() {
		// This tests the scenario where:
		// 1. Peer connects and immediately sets affinity (before initial sync).
		// 2. Statements not matching the initial affinity are not delivered.
		// 3. When affinity changes to include those topics, they ARE sent.
		let (mut handler, statement_store, _network, notification_service) =
			build_handler_no_peers_light();

		let peer_id = PeerId::random();

		let topic_aa: [u8; 32] = [0xAA; 32];
		let topic_bb: [u8; 32] = [0xBB; 32];

		let mut stmt_aa = new_live_statement();
		stmt_aa.set_plain_data(b"stmt_aa".to_vec());
		stmt_aa.set_topic(0, topic_aa.into());
		let hash_aa = stmt_aa.hash();

		let mut stmt_bb = new_live_statement();
		stmt_bb.set_plain_data(b"stmt_bb".to_vec());
		stmt_bb.set_topic(0, topic_bb.into());
		let hash_bb = stmt_bb.hash();

		statement_store.insert(stmt_aa.clone());
		statement_store.insert(stmt_bb.clone());

		// Also put them in recent_statements so propagate_statements can find them.
		statement_store.recent_statements.lock().unwrap().insert(hash_aa, stmt_aa);
		statement_store.recent_statements.lock().unwrap().insert(hash_bb, stmt_bb);

		// Connect peer as v2.
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;

		// Immediately set affinity to topic_aa BEFORE any initial sync runs.
		let mut filter = AffinityFilter::new(BLOOM_SEED, 0.01, 100);
		filter.insert(&topic_aa);
		let msg = StatementMessage::ExplicitTopicAffinity(filter);
		let encoded = msg.encode();
		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification: encoded.into(),
			})
			.await;

		// Affinity is deferred; process it.
		handler.process_pending_affinities();

		// Drain initial sync — should only send stmt_aa (matches affinity).
		while handler.pending_initial_syncs.contains_key(&peer_id) {
			handler.process_initial_sync_burst();
			handler.flush_pending_sends().await;
		}

		let sent = notification_service.get_sent_notifications();
		let sent_hashes: HashSet<_> = sent
			.iter()
			.flat_map(|(_, notification)| {
				match StatementMessage::decode(&mut notification.as_slice()).unwrap() {
					StatementMessage::Statements(stmts) => stmts,
					_ => panic!("Expected StatementMessage::Statements"),
				}
			})
			.map(|s| s.hash())
			.collect();
		assert!(sent_hashes.contains(&hash_aa), "stmt_aa should be sent (matches affinity)");
		assert!(
			!sent_hashes.contains(&hash_bb),
			"stmt_bb should NOT be sent (filtered by affinity)"
		);

		// Propagation must apply the same affinity filter. The original statements sit below
		// the sync watermark and belong to the cursor, so fresh admissions carry the check.
		let mut stmt_aa2 = new_live_statement();
		stmt_aa2.set_plain_data(b"stmt_aa2".to_vec());
		stmt_aa2.set_topic(0, topic_aa.into());
		let hash_aa2 = stmt_aa2.hash();
		let mut stmt_bb2 = new_live_statement();
		stmt_bb2.set_plain_data(b"stmt_bb2".to_vec());
		stmt_bb2.set_topic(0, topic_bb.into());
		let hash_bb2 = stmt_bb2.hash();
		statement_store.recent_statements.lock().unwrap().insert(hash_aa2, stmt_aa2);
		statement_store.recent_statements.lock().unwrap().insert(hash_bb2, stmt_bb2);

		notification_service.clear_sent_notifications();
		handler.propagate_statements().await;
		handler.flush_pending_sends().await;

		let sent = notification_service.get_sent_notifications();
		let sent_hashes: HashSet<_> = sent
			.iter()
			.flat_map(|(_, notification)| {
				match StatementMessage::decode(&mut notification.as_slice()).unwrap() {
					StatementMessage::Statements(stmts) => stmts,
					_ => panic!("Expected StatementMessage::Statements"),
				}
			})
			.map(|s| s.hash())
			.collect();
		assert!(
			sent_hashes.contains(&hash_aa2),
			"stmt_aa2 should be propagated (matches affinity)"
		);
		assert!(
			!sent_hashes.contains(&hash_bb2),
			"stmt_bb2 should NOT be propagated (filtered by affinity)"
		);
		assert!(
			!sent_hashes.contains(&hash_aa),
			"a statement below the sync watermark is delivered by the cursor, not propagation"
		);

		// Now change affinity to include topic_bb.
		let mut filter = AffinityFilter::new(BLOOM_SEED, 0.01, 100);
		filter.insert(&topic_aa);
		filter.insert(&topic_bb);
		let msg = StatementMessage::ExplicitTopicAffinity(filter);
		let encoded = msg.encode();

		notification_service.clear_sent_notifications();
		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification: encoded.into(),
			})
			.await;

		// Affinity is deferred; process it.
		handler.process_pending_affinities();

		// Drain re-sync — stmt_bb should now be sent.
		while handler.pending_initial_syncs.contains_key(&peer_id) {
			handler.process_initial_sync_burst();
			handler.flush_pending_sends().await;
		}

		let sent = notification_service.get_sent_notifications();
		let sent_hashes: HashSet<_> = sent
			.iter()
			.flat_map(|(_, notification)| {
				match StatementMessage::decode(&mut notification.as_slice()).unwrap() {
					StatementMessage::Statements(stmts) => stmts,
					_ => panic!("Expected StatementMessage::Statements"),
				}
			})
			.map(|s| s.hash())
			.collect();
		assert!(
			sent_hashes.contains(&hash_bb),
			"stmt_bb should now be sent after affinity expanded to include topic_bb"
		);
		// The first sync delivered stmt_aa under the previous filter.
		assert!(
			!sent_hashes.contains(&hash_aa),
			"stmt_aa should not be re-sent after the affinity change"
		);
	}

	#[test]
	fn test_encode_statement_refs_matches_derive_encoding() {
		let mut stmt1 = new_live_statement();
		stmt1.set_plain_data(b"first".to_vec());
		let mut stmt2 = new_live_statement();
		stmt2.set_plain_data(b"second".to_vec());

		let refs: Vec<&Statement> = vec![&stmt1, &stmt2];
		let hand_rolled = StatementMessage::encode_statement_refs(&refs);
		let derive_encoded =
			StatementMessage::Statements(vec![stmt1, stmt2].try_into().unwrap()).encode();

		assert_eq!(
			hand_rolled, derive_encoded,
			"encode_statement_refs must produce identical bytes to derive Encode"
		);
	}

	#[test]
	fn test_encode_statement_refs_empty() {
		let refs: Vec<&Statement> = vec![];
		let hand_rolled = StatementMessage::encode_statement_refs(&refs);
		let derive_encoded = StatementMessage::Statements(vec![].try_into().unwrap()).encode();

		assert_eq!(hand_rolled, derive_encoded);
	}

	#[test]
	fn test_can_receive_all_combinations() {
		let make_peer = |is_light: bool, version: PeerProtocolVersion, has_affinity: bool| {
			let topic_affinity = has_affinity.then(|| AffinityFilter::new(BLOOM_SEED, 0.01, 10));
			Peer {
				statement_rate_limiter: default_statement_limiter(),
				affinity_rate_limiter: PeerRateLimiter::for_affinity_updates(),
				announce_rate_limiter: PeerRateLimiter::for_received_announces(),
				announce_budget: PeerRateLimiter::for_sent_announces(),
				send_budget: default_statement_limiter(),
				protocol_version: version,
				topic_affinity,
				is_light,
				inbound: true,
				pending_topic_affinity: None,
				full_replay_requested: false,
				sync_watermark: 0,
			}
		};

		// Full node, V1, no affinity → can receive
		assert!(make_peer(false, PeerProtocolVersion::V1, false).can_receive());
		// Full node, V2, no affinity → can receive
		assert!(make_peer(false, PeerProtocolVersion::V2, false).can_receive());
		// Light, V1, no affinity → can receive (V1 doesn't gate)
		assert!(make_peer(true, PeerProtocolVersion::V1, false).can_receive());
		// Light, V2, no affinity → CANNOT receive (must set affinity first)
		assert!(!make_peer(true, PeerProtocolVersion::V2, false).can_receive());
		// Light, V2, with affinity → can receive
		assert!(make_peer(true, PeerProtocolVersion::V2, true).can_receive());
		// Full node, V2, with affinity → can receive
		assert!(make_peer(false, PeerProtocolVersion::V2, true).can_receive());
		// Light, V3, no affinity → CANNOT receive (must set affinity first)
		assert!(!make_peer(true, PeerProtocolVersion::V3, false).can_receive());
		// Light, V3, with affinity → can receive
		assert!(make_peer(true, PeerProtocolVersion::V3, true).can_receive());
	}

	#[tokio::test]
	async fn test_send_chunk_v1_vs_v2_encoding() {
		let (mut handler, statement_store, _network, notification_service) =
			build_handler_no_peers();

		let v1_peer = PeerId::random();
		let v2_peer = PeerId::random();

		// Connect V1 peer.
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: v1_peer,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: Some(format!("/{STATEMENT_PROTOCOL_V1}").into()),
			})
			.await;

		// Connect V2 peer.
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: v2_peer,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;

		let mut stmt = new_live_statement();
		stmt.set_plain_data(b"encoding test".to_vec());
		statement_store.insert(stmt);

		// Send to V1 peer.
		notification_service.clear_sent_notifications();
		handler.schedule_initial_sync_for_peer(v1_peer, None);
		while handler.pending_initial_syncs.contains_key(&v1_peer) {
			handler.process_initial_sync_burst();
			handler.flush_pending_sends().await;
		}
		let v1_sent = notification_service.get_sent_notifications();
		assert_eq!(v1_sent.len(), 1);
		let v1_bytes = &v1_sent[0].1;
		// V1 encoding is raw Vec<Statement>.
		let decoded_v1 = <Statements as Decode>::decode(&mut v1_bytes.as_slice())
			.expect("V1 peer should receive raw Vec<Statement> encoding");
		assert_eq!(decoded_v1.len(), 1);

		// Send to V2 peer.
		notification_service.clear_sent_notifications();
		handler.schedule_initial_sync_for_peer(v2_peer, None);
		while handler.pending_initial_syncs.contains_key(&v2_peer) {
			handler.process_initial_sync_burst();
			handler.flush_pending_sends().await;
		}
		let v2_sent = notification_service.get_sent_notifications();
		assert_eq!(v2_sent.len(), 1);
		let v2_bytes = &v2_sent[0].1;
		// V2 encoding is StatementMessage::Statements.
		let decoded_v2 = StatementMessage::decode(&mut v2_bytes.as_slice())
			.expect("V2 peer should receive StatementMessage encoding");
		match decoded_v2 {
			StatementMessage::Statements(stmts) => assert_eq!(stmts.len(), 1),
			_ => panic!("Expected StatementMessage::Statements for V2 peer"),
		}

		// Verify the two encodings are different (V2 has an extra enum discriminant byte).
		assert_ne!(v1_bytes, v2_bytes, "V1 and V2 encodings should differ");
	}

	#[tokio::test]
	async fn test_schedule_initial_sync_replaces_existing() {
		let (mut handler, statement_store, _network, _notification_service) =
			build_handler_no_peers();

		let peer_id = PeerId::random();

		// Add some statements to the store.
		let mut stmt1 = new_live_statement();
		stmt1.set_plain_data(b"stmt1".to_vec());
		statement_store.insert(stmt1);

		// Connect peer as V1.
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: Some(format!("/{STATEMENT_PROTOCOL_V1}").into()),
			})
			.await;

		// Should have initial sync scheduled.
		assert!(handler.pending_initial_syncs.contains_key(&peer_id));
		assert_eq!(
			handler.initial_sync_peer_queue.iter().filter(|p| **p == peer_id).count(),
			1,
			"Peer should appear exactly once in the queue"
		);

		// Add another statement and re-schedule.
		let mut stmt2 = new_live_statement();
		stmt2.set_plain_data(b"stmt2".to_vec());
		statement_store.insert(stmt2);

		handler.schedule_initial_sync_for_peer(peer_id, None);

		// Peer should still appear exactly once in the queue (no duplicates).
		assert_eq!(
			handler.initial_sync_peer_queue.iter().filter(|p| **p == peer_id).count(),
			1,
			"Peer should NOT be duplicated in the queue after re-schedule"
		);
		// The new sync should cover both admissions.
		let pending = handler.pending_initial_syncs.get(&peer_id).unwrap();
		assert_eq!(pending.cursor, 0);
		assert_eq!(pending.watermark, 2);
	}

	#[tokio::test]
	async fn test_initial_sync_queued_during_major_sync_processed_after() {
		let statement_store = TestStatementStore::new();
		let (queue_sender, _queue_receiver) = async_channel::bounded(2);
		let network = TestNetwork::new();
		let notification_service = TestNotificationService::new();
		let sync = TestSync::new();
		// Set major syncing to true.
		sync.major_syncing.store(true, Ordering::Relaxed);

		let mut handler = StatementHandler {
			protocol_name: format!("/{STATEMENT_PROTOCOL_V1}").into(),
			notification_service: Box::new(notification_service.clone()),
			propagate_timeout: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = ()> + Send>>)
				.fuse(),
			pending_statements: FuturesUnordered::new(),
			pending_statements_peers: HashMap::new(),
			short_hashes_in_validation: HashSet::new(),
			recently_received_statements: HashMap::new(),
			network: network.clone(),
			sync: sync.clone(),
			sync_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = sc_network_sync::types::SyncEvent> + Send>>)
				.fuse(),
			network_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = Event> + Send>>)
				.fuse(),
			peers: HashMap::new(),
			statement_store: Arc::new(statement_store.clone()),
			queue_sender,
			statements_per_second: NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND)
				.expect("DEFAULT_STATEMENTS_PER_SECOND is nonzero"),
			metrics: None,
			initial_sync_timeout: Box::pin(futures::future::pending()),
			pending_affinities_timeout: Box::pin(futures::future::pending()),
			peer_eviction_timeout: Box::pin(futures::future::pending()),
			pending_initial_syncs: HashMap::new(),
			initial_sync_peer_queue: VecDeque::new(),
			next_initial_sync_id: 0,
			initial_sync_in_flight_bytes: 0,
			propagation_outboxes: PropagationOutboxes::new(None),
			in_flight_chunks: HashMap::new(),
			next_chunk_id: 0,
			propagation_in_flight_bytes: 0,
			parked_propagations: VecDeque::new(),
			pending_sends: FuturesUnordered::new(),
			deferred_peers: HashSet::new(),
			in_peers_full: DEFAULT_IN_PEERS,
			in_peers_light: 0,
			dropped_statements_during_sync: false,
			sync_recovery_peer: None,
			sync_recovery_readd_timeout: Box::pin(futures::future::pending()),
			dht: DhtOrchestrator::new(
				&[],
				None,
				crate::config::DEFAULT_BLOOM_FALSE_POS_RATE,
				network.local_peer_id(),
				topology_config(20, 3),
				"/statement/test".into(),
				None,
			),
		};

		// Add a statement so there's something to sync.
		let mut stmt = new_live_statement();
		stmt.set_plain_data(b"during major sync".to_vec());
		statement_store.insert(stmt);

		// Add a peer manually.
		let peer_id = PeerId::random();
		handler.peers.insert(
			peer_id,
			Peer::new_for_testing(
				NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND).unwrap(),
				NonZeroU32::new(
					DEFAULT_STATEMENTS_PER_SECOND * config::STATEMENTS_BURST_COEFFICIENT,
				)
				.unwrap(),
			),
		);

		// Scheduling during major sync should queue the peer.
		handler.schedule_initial_sync_for_peer(peer_id, None);

		assert!(
			handler.pending_initial_syncs.contains_key(&peer_id),
			"Initial sync should be queued even during major sync"
		);
		assert_eq!(handler.initial_sync_peer_queue.len(), 1);

		// But burst processing should be a no-op while major syncing.
		handler.process_initial_sync_burst();
		handler.flush_pending_sends().await;
		assert!(
			handler.pending_initial_syncs.contains_key(&peer_id),
			"Pending sync should remain untouched during major sync"
		);

		// Once major sync completes, burst processing should proceed.
		sync.major_syncing.store(false, Ordering::Relaxed);
		while handler.pending_initial_syncs.contains_key(&peer_id) {
			handler.process_initial_sync_burst();
			handler.flush_pending_sends().await;
		}
		assert!(
			handler.initial_sync_peer_queue.is_empty(),
			"Peer should have been processed after major sync ended"
		);
	}

	#[tokio::test]
	async fn test_schedule_initial_sync_resends_all_matching() {
		let (mut handler, statement_store, _network, _notification_service) =
			build_handler_no_peers();

		let peer_id = PeerId::random();

		// Add statements to the store.
		let mut stmt1 = new_live_statement();
		stmt1.set_plain_data(b"delivered before".to_vec());
		let mut stmt2 = new_live_statement();
		stmt2.set_plain_data(b"never delivered".to_vec());

		statement_store.insert(stmt1);
		statement_store.insert(stmt2);

		handler.peers.insert(
			peer_id,
			Peer {
				statement_rate_limiter: default_statement_limiter(),
				affinity_rate_limiter: PeerRateLimiter::for_affinity_updates(),
				announce_rate_limiter: PeerRateLimiter::for_received_announces(),
				announce_budget: PeerRateLimiter::for_sent_announces(),
				send_budget: default_statement_limiter(),
				protocol_version: PeerProtocolVersion::V1,
				topic_affinity: None,
				is_light: false,
				inbound: true,
				pending_topic_affinity: None,
				full_replay_requested: false,
				sync_watermark: 0,
			},
		);

		handler.schedule_initial_sync_for_peer(peer_id, None);

		let pending = handler.pending_initial_syncs.get(&peer_id).unwrap();
		// The whole admission journal is covered for redelivery.
		assert_eq!(pending.cursor, 0, "The sync must start from the oldest admission");
		assert_eq!(pending.watermark, 2, "Both admissions must sit below the watermark");
	}

	#[tokio::test]
	async fn statement_below_the_watermark_reaches_a_syncing_peer_exactly_once() {
		let (mut handler, statement_store, _network, notification_service) =
			build_handler_no_peers();
		let peer_id = PeerId::random();

		let mut statement = new_live_statement();
		statement.set_plain_data(b"pre-watermark".to_vec());
		let hash = statement.hash();
		statement_store.insert(statement.clone());
		// The statement is also due for a propagation tick, racing the sync.
		statement_store.recent_statements.lock().unwrap().insert(hash, statement);

		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: Some(format!("/{STATEMENT_PROTOCOL_V1}").into()),
			})
			.await;
		assert!(handler.pending_initial_syncs.contains_key(&peer_id));

		// A tick between scheduling and the first burst must not queue the statement:
		// below the peer's sync watermark it is the cursor's job.
		handler.propagate_statements().await;
		assert!(handler.propagation_outboxes.get(&peer_id).is_none());

		while handler.pending_initial_syncs.contains_key(&peer_id) {
			handler.process_initial_sync_burst();
			handler.flush_pending_sends().await;
		}

		let sent = get_peer_hashes(&notification_service.get_sent_notifications(), peer_id);
		assert_eq!(sent, vec![hash], "the sync cursor is the only delivery path");
	}

	#[tokio::test]
	async fn sync_watermark_keeps_filtering_propagation_after_the_sync_completes() {
		let (mut handler, statement_store, _network, notification_service) =
			build_handler_no_peers();
		let peer_id = PeerId::random();

		let mut pre = new_live_statement();
		pre.set_plain_data(b"pre-watermark".to_vec());
		let pre_hash = pre.hash();
		statement_store.insert(pre.clone());

		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: Some(format!("/{STATEMENT_PROTOCOL_V1}").into()),
			})
			.await;
		while handler.pending_initial_syncs.contains_key(&peer_id) {
			handler.process_initial_sync_burst();
			handler.flush_pending_sends().await;
		}
		notification_service.clear_sent_notifications();

		// A late tick drains the already synced statement together with a fresh one.
		let mut fresh = new_live_statement();
		fresh.set_plain_data(b"post-watermark".to_vec());
		let fresh_hash = fresh.hash();
		statement_store.recent_statements.lock().unwrap().insert(pre_hash, pre);
		statement_store.recent_statements.lock().unwrap().insert(fresh_hash, fresh);

		handler.propagate_statements().await;
		handler.flush_pending_sends().await;

		let sent = get_peer_hashes(&notification_service.get_sent_notifications(), peer_id);
		assert_eq!(
			sent,
			vec![fresh_hash],
			"only admissions at or above the watermark are propagated"
		);
	}

	#[tokio::test]
	async fn test_malformed_v2_message_does_not_panic() {
		let (mut handler, _statement_store, _network, _notification_service) =
			build_handler_no_peers();

		let peer_id = PeerId::random();

		// Connect peer as V2.
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;

		// Send garbage data — should not panic, just log debug.
		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification: vec![0xFF, 0xFE, 0xFD].into(),
			})
			.await;

		// Send V1-encoded data to V2 peer — also should not panic.
		let mut stmt = new_live_statement();
		stmt.set_plain_data(b"v1 encoded".to_vec());
		let v1_encoded = vec![stmt].encode();
		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification: v1_encoded.into(),
			})
			.await;

		// If we got here without panic, the test passes.
		assert!(handler.peers.contains_key(&peer_id), "Peer should still be connected");
	}

	// A batch of one more empty statement (a single `0x00` field-count byte each) than a
	// notification can hold. An unbounded `Vec<Statement>` would decode it in full.
	fn oversized_statement_batch() -> Vec<u8> {
		let count = MAX_STATEMENTS_PER_NOTIFICATION + 1;
		let mut batch = Compact(count as u32).encode();
		batch.extend(iter::repeat_n(0u8, count));
		batch
	}

	#[tokio::test]
	async fn test_v1_oversized_batch_reported_as_bad_message() {
		let (mut handler, _statement_store, network, _notification_service) =
			build_handler_no_peers();

		let peer_id = PeerId::random();

		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: Some(format!("/{STATEMENT_PROTOCOL_V1}").into()),
			})
			.await;

		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification: oversized_statement_batch().into(),
			})
			.await;

		let reports = network.get_reports();
		assert!(
			reports.iter().any(|(id, rep)| *id == peer_id && *rep == rep::BAD_MESSAGE),
			"Expected BAD_MESSAGE reputation change, but got: {reports:?}"
		);
		assert!(
			!network.get_disconnected_peers().contains(&peer_id),
			"Expected oversized-batch peer to stay connected"
		);
	}

	#[tokio::test]
	async fn test_v2_oversized_batch_reported_as_bad_message() {
		let (mut handler, _statement_store, network, _notification_service) =
			build_handler_no_peers();

		let peer_id = PeerId::random();

		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;

		// `StatementMessage::Statements` variant byte followed by the oversized batch.
		let mut notification = vec![STATEMENTS_VARIANT_INDEX];
		notification.extend(oversized_statement_batch());

		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification: notification.into(),
			})
			.await;

		let reports = network.get_reports();
		assert!(
			reports.iter().any(|(id, rep)| *id == peer_id && *rep == rep::BAD_MESSAGE),
			"Expected BAD_MESSAGE reputation change, but got: {reports:?}"
		);
		assert!(
			!network.get_disconnected_peers().contains(&peer_id),
			"Expected oversized-batch peer to stay connected"
		);
	}

	#[test]
	fn test_max_statement_payload_size_v2_overhead() {
		let v1_max = max_statement_payload_size(V1_ENVELOPE_OVERHEAD);
		let v2_max = max_statement_payload_size(V2_ENVELOPE_OVERHEAD);

		// V2 has strictly less payload space than V1.
		assert!(
			v2_max < v1_max,
			"V2 payload capacity ({v2_max}) should be less than V1 ({v1_max})"
		);
		assert_eq!(v1_max - v2_max, 1, "V2 overhead is exactly 1 byte more than V1");
	}

	#[tokio::test]
	async fn test_full_node_v2_gets_initial_sync_immediately() {
		let (mut handler, statement_store, _network, _notification_service) =
			build_handler_no_peers();

		// Add a statement so there's something to sync.
		let mut stmt = new_live_statement();
		stmt.set_plain_data(b"full node v2".to_vec());
		statement_store.insert(stmt);

		let peer_id = PeerId::random();

		// Connect as full-node V2 (no fallback, network returns Full role).
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;

		// Full-node V2 peer should get initial sync immediately (not gated).
		assert!(
			handler.pending_initial_syncs.contains_key(&peer_id),
			"Full-node V2 peer should have initial sync scheduled immediately"
		);
		assert_eq!(handler.peers.get(&peer_id).unwrap().protocol_version, PeerProtocolVersion::V2);
		assert!(!handler.peers.get(&peer_id).unwrap().is_light);
	}

	#[tokio::test]
	async fn test_propagation_reaches_all_connected_peers() {
		let (
			mut handler,
			statement_store,
			_network,
			notification_service,
			_queue_receiver,
			peer_ids,
		) = build_handler(5);

		// Insert 3 statements into recent_statements for propagation
		let mut expected_hashes = Vec::new();
		for i in 0..3u8 {
			let mut statement = new_live_statement();
			statement.set_plain_data(vec![i; 100]);
			let hash = statement.hash();
			expected_hashes.push(hash);
			statement_store.recent_statements.lock().unwrap().insert(hash, statement);
		}
		expected_hashes.sort();

		handler.propagate_statements().await;
		handler.flush_pending_sends().await;

		let sent = notification_service.get_sent_notifications();

		// Verify each peer received all 3 statements
		for peer_id in &peer_ids {
			let mut received_hashes = get_peer_hashes(&sent, *peer_id);
			received_hashes.sort();

			assert_eq!(
				received_hashes, expected_hashes,
				"Peer {peer_id} should have received all 3 statements"
			);
		}

		// Recent statements should be drained
		assert!(statement_store.recent_statements.lock().unwrap().is_empty());
	}

	#[tokio::test]
	async fn test_received_statement_filtering_per_peer() {
		let (
			mut handler,
			statement_store,
			_network,
			notification_service,
			_queue_receiver,
			peer_ids,
		) = build_handler(3);

		let peer_a = peer_ids[0];
		let peer_b = peer_ids[1];
		let peer_c = peer_ids[2];

		// Create 5 statements
		let mut hashes = Vec::new();
		for i in 0..5u8 {
			let mut statement = new_live_statement();
			statement.set_plain_data(vec![i; 100]);
			let hash = statement.hash();
			hashes.push(hash);
			statement_store.recent_statements.lock().unwrap().insert(hash, statement);
		}

		// peer_a sent s1 and s2, peer_b sent s3, peer_c sent none.
		handler
			.recently_received_statements
			.insert(hashes[0], HashSet::from_iter([peer_a]));
		handler
			.recently_received_statements
			.insert(hashes[1], HashSet::from_iter([peer_a]));
		handler
			.recently_received_statements
			.insert(hashes[2], HashSet::from_iter([peer_b]));

		handler.propagate_statements().await;
		handler.flush_pending_sends().await;

		let sent = notification_service.get_sent_notifications();

		let peer_a_hashes = get_peer_hashes(&sent, peer_a);
		let peer_b_hashes = get_peer_hashes(&sent, peer_b);
		let peer_c_hashes = get_peer_hashes(&sent, peer_c);

		// peer_a sent s1,s2 → should only get s3,s4,s5
		assert_eq!(peer_a_hashes.len(), 3, "peer_a should get 3 statements");
		assert!(!peer_a_hashes.contains(&hashes[0]), "peer_a sent s1");
		assert!(!peer_a_hashes.contains(&hashes[1]), "peer_a sent s2");
		assert!(peer_a_hashes.contains(&hashes[2]));
		assert!(peer_a_hashes.contains(&hashes[3]));
		assert!(peer_a_hashes.contains(&hashes[4]));

		// peer_b sent s3 → should get s1,s2,s4,s5
		assert_eq!(peer_b_hashes.len(), 4, "peer_b should get 4 statements");
		assert!(!peer_b_hashes.contains(&hashes[2]), "peer_b sent s3");
		assert!(peer_b_hashes.contains(&hashes[0]));
		assert!(peer_b_hashes.contains(&hashes[1]));
		assert!(peer_b_hashes.contains(&hashes[3]));
		assert!(peer_b_hashes.contains(&hashes[4]));

		// peer_c sent nothing → should get all 5
		let mut sorted_peer_c: Vec<_> = peer_c_hashes.into_iter().collect();
		sorted_peer_c.sort();
		let mut all_hashes = hashes.clone();
		all_hashes.sort();
		assert_eq!(sorted_peer_c, all_hashes, "peer_c should get all 5 statements");
	}

	/// Verifies that peers connecting during major sync are buffered in `deferred_peers` with no
	/// network calls, and that a disconnect before sync ends removes the peer from the buffer
	#[test]
	fn major_sync_defers_peers_and_handles_disconnect() {
		let (sync, _flag) = TestSync::with_syncing(true);
		let network = TestNetwork::new();
		let notification_service = TestNotificationService::new();
		let statement_store = TestStatementStore::new();
		let (queue_sender, _queue_receiver) = async_channel::bounded(100);

		let mut handler = StatementHandler {
			protocol_name: "/statement/1".into(),
			notification_service: Box::new(notification_service),
			propagate_timeout: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = ()> + Send>>)
				.fuse(),
			pending_statements: FuturesUnordered::new(),
			pending_statements_peers: HashMap::new(),
			short_hashes_in_validation: HashSet::new(),
			recently_received_statements: HashMap::new(),
			network: network.clone(),
			sync,
			sync_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = sc_network_sync::types::SyncEvent> + Send>>)
				.fuse(),
			network_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = Event> + Send>>)
				.fuse(),
			peers: HashMap::new(),
			statement_store: Arc::new(statement_store),
			queue_sender,
			statements_per_second: NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND)
				.expect("DEFAULT_STATEMENTS_PER_SECOND is nonzero"),
			metrics: None,
			initial_sync_timeout: Box::pin(futures::future::pending()),
			pending_affinities_timeout: Box::pin(futures::future::pending()),
			peer_eviction_timeout: Box::pin(futures::future::pending()),
			pending_initial_syncs: HashMap::new(),
			initial_sync_peer_queue: VecDeque::new(),
			next_initial_sync_id: 0,
			initial_sync_in_flight_bytes: 0,
			propagation_outboxes: PropagationOutboxes::new(None),
			in_flight_chunks: HashMap::new(),
			next_chunk_id: 0,
			propagation_in_flight_bytes: 0,
			parked_propagations: VecDeque::new(),
			pending_sends: FuturesUnordered::new(),
			deferred_peers: HashSet::new(),
			in_peers_full: DEFAULT_IN_PEERS,
			in_peers_light: 0,
			dropped_statements_during_sync: false,
			sync_recovery_peer: None,
			sync_recovery_readd_timeout: Box::pin(pending().fuse()),
			dht: DhtOrchestrator::new(
				&[],
				None,
				crate::config::DEFAULT_BLOOM_FALSE_POS_RATE,
				network.local_peer_id(),
				topology_config(20, 3),
				"/statement/test".into(),
				None,
			),
		};

		let peer1 = PeerId::random();
		let peer2 = PeerId::random();
		let peer3 = PeerId::random();

		handler.handle_sync_event(SyncEvent::PeerConnected {
			peer_id: peer1,
			roles: sc_network::Roles::FULL,
		});
		handler.handle_sync_event(SyncEvent::PeerConnected {
			peer_id: peer2,
			roles: sc_network::Roles::FULL,
		});
		handler.handle_sync_event(SyncEvent::PeerConnected {
			peer_id: peer3,
			roles: sc_network::Roles::FULL,
		});

		// No network calls while major sync is active
		assert!(network.get_added_reserved().is_empty());
		assert!(network.get_removed_reserved().is_empty());
		assert_eq!(handler.deferred_peers.len(), 3);

		// Disconnect before sync ends must remove from buffer only
		handler.handle_sync_event(SyncEvent::PeerDisconnected(peer1));
		assert_eq!(handler.deferred_peers.len(), 2);
		assert!(!handler.deferred_peers.contains(&peer1), "disconnected peer must leave buffer");
		assert!(handler.deferred_peers.contains(&peer2));
		assert!(handler.deferred_peers.contains(&peer3));
		assert!(network.get_removed_reserved().is_empty(), "no remove call for buffered peer");
	}

	#[test]
	fn inbound_slots_are_split_by_role() {
		let (mut handler, _store, _network, _notification_service, _, peer_ids) =
			build_handler(DEFAULT_IN_PEERS as usize);
		handler.in_peers_light = 1;

		assert!(!handler.inbound_slot_free(ObservedRole::Full), "full inbound slots are taken");
		assert!(handler.inbound_slot_free(ObservedRole::Light), "light slots are counted apart");

		handler.peers.get_mut(&peer_ids[0]).unwrap().inbound = false;
		assert!(handler.inbound_slot_free(ObservedRole::Full), "an outbound peer holds no slot");

		handler.peers.get_mut(&peer_ids[1]).unwrap().is_light = true;
		assert!(!handler.inbound_slot_free(ObservedRole::Light), "the light quota is taken");
		assert!(handler.inbound_slot_free(ObservedRole::Full));
	}

	#[test]
	fn reserved_peers_hold_no_inbound_slot() {
		let (mut handler, _store, network, _notification_service, _, peer_ids) = build_handler(1);
		handler.in_peers_light = 1;
		handler.peers.get_mut(&peer_ids[0]).unwrap().is_light = true;
		assert!(!handler.inbound_slot_free(ObservedRole::Light));

		handler.dht.update_peers_needing_connections([peer_ids[0]]);
		handler.dht.refresh_connections(&network);
		assert_eq!(network.get_added_reserved().len(), 1);
		assert!(handler.inbound_slot_free(ObservedRole::Light), "a reserved peer is not counted");

		handler.dht.update_peers_needing_connections([]);
		handler.dht.refresh_connections(&network);
		assert_eq!(network.get_removed_reserved().len(), 1);
		assert!(!handler.dht.is_reserved(&peer_ids[0]), "a released peer is reserved no more");
		assert!(!handler.inbound_slot_free(ObservedRole::Light));
	}

	#[test]
	fn only_a_held_peer_without_dht_is_released_for_a_disconnect() {
		let (mut handler, _store, network, _notification_service) = build_handler_no_peers();
		let held = PeerId::random();
		let routing = PeerId::random();
		let unreserved = PeerId::random();
		for peer in [held, routing, unreserved] {
			handler.dht.on_substream_opened(peer);
		}
		for peer in [held, unreserved] {
			handler.dht.on_peer_without_dht(peer);
		}
		handler.dht.update_peers_needing_connections([held, routing]);
		handler.dht.refresh_connections(&network);

		for peer in [held, routing, unreserved] {
			handler.dht.release_peer_without_dht(&network, peer);
		}

		assert_eq!(network.get_removed_reserved(), vec![vec![held]]);
		assert!(!handler.dht.is_reserved(&held));
		assert!(handler.dht.is_reserved(&routing));
	}

	#[test]
	fn deferred_peers_flushed_on_sync_end_without_remove() {
		let (sync, flag) = TestSync::with_syncing(true);
		let network = TestNetwork::new();
		let notification_service = TestNotificationService::new();
		let statement_store = TestStatementStore::new();
		let (queue_sender, _queue_receiver) = async_channel::bounded(100);

		let peer1 = PeerId::random();
		let peer2 = PeerId::random();
		let mut deferred = HashSet::new();
		deferred.insert(peer1);
		deferred.insert(peer2);

		let mut handler = StatementHandler {
			protocol_name: "/statement/1".into(),
			notification_service: Box::new(notification_service),
			propagate_timeout: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = ()> + Send>>)
				.fuse(),
			pending_statements: FuturesUnordered::new(),
			pending_statements_peers: HashMap::new(),
			short_hashes_in_validation: HashSet::new(),
			recently_received_statements: HashMap::new(),
			network: network.clone(),
			sync,
			sync_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = sc_network_sync::types::SyncEvent> + Send>>)
				.fuse(),
			network_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = Event> + Send>>)
				.fuse(),
			peers: HashMap::new(),
			statement_store: Arc::new(statement_store),
			queue_sender,
			statements_per_second: NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND)
				.expect("DEFAULT_STATEMENTS_PER_SECOND is nonzero"),
			metrics: None,
			initial_sync_timeout: Box::pin(futures::future::pending()),
			pending_affinities_timeout: Box::pin(futures::future::pending()),
			peer_eviction_timeout: Box::pin(futures::future::pending()),
			pending_initial_syncs: HashMap::new(),
			initial_sync_peer_queue: VecDeque::new(),
			next_initial_sync_id: 0,
			initial_sync_in_flight_bytes: 0,
			propagation_outboxes: PropagationOutboxes::new(None),
			in_flight_chunks: HashMap::new(),
			next_chunk_id: 0,
			propagation_in_flight_bytes: 0,
			parked_propagations: VecDeque::new(),
			pending_sends: FuturesUnordered::new(),
			deferred_peers: deferred,
			in_peers_full: DEFAULT_IN_PEERS,
			in_peers_light: 0,
			dropped_statements_during_sync: false,
			sync_recovery_peer: None,
			sync_recovery_readd_timeout: Box::pin(pending().fuse()),
			dht: DhtOrchestrator::new(
				&[],
				None,
				crate::config::DEFAULT_BLOOM_FALSE_POS_RATE,
				network.local_peer_id(),
				topology_config(20, 3),
				"/statement/test".into(),
				None,
			),
		};

		flag.store(false, std::sync::atomic::Ordering::Relaxed);
		handler.drain_deferred_peers();

		assert!(handler.deferred_peers.is_empty());

		let added = network.get_added_reserved();
		assert_eq!(added.len(), 1);
		let added_addrs = &added[0];
		let expected_addr1: sc_network::Multiaddr =
			iter::once(multiaddr::Protocol::P2p(peer1.into())).collect();
		let expected_addr2: sc_network::Multiaddr =
			iter::once(multiaddr::Protocol::P2p(peer2.into())).collect();
		assert!(added_addrs.contains(&expected_addr1), "peer1 must be in added set");
		assert!(added_addrs.contains(&expected_addr2), "peer2 must be in added set");

		assert!(network.get_removed_reserved().is_empty());
	}

	#[tokio::test]
	async fn sync_recovery_schedules_remove_for_one_connected_peer() {
		let network = TestNetwork::new();
		let notification_service = TestNotificationService::new();
		let (sync, _flag) = TestSync::with_syncing(false);
		let (queue_sender, _) = async_channel::bounded(2);
		let statement_store = TestStatementStore::new();

		let connected_peer = PeerId::random();

		let mut peers = HashMap::new();
		peers.insert(
			connected_peer,
			Peer {
				statement_rate_limiter: default_statement_limiter(),
				affinity_rate_limiter: PeerRateLimiter::for_affinity_updates(),
				announce_rate_limiter: PeerRateLimiter::for_received_announces(),
				announce_budget: PeerRateLimiter::for_sent_announces(),
				send_budget: default_statement_limiter(),
				protocol_version: PeerProtocolVersion::V1,
				topic_affinity: None,
				is_light: false,
				inbound: true,
				pending_topic_affinity: None,
				full_replay_requested: false,
				sync_watermark: 0,
			},
		);

		let mut handler = StatementHandler {
			protocol_name: format!("/{STATEMENT_PROTOCOL_V1}").into(),
			notification_service: Box::new(notification_service),
			propagate_timeout: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = ()> + Send>>)
				.fuse(),
			pending_statements: FuturesUnordered::new(),
			pending_statements_peers: HashMap::new(),
			short_hashes_in_validation: HashSet::new(),
			recently_received_statements: HashMap::new(),
			network: network.clone(),
			sync,
			sync_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = sc_network_sync::types::SyncEvent> + Send>>)
				.fuse(),
			network_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = Event> + Send>>)
				.fuse(),
			peers,
			statement_store: Arc::new(statement_store),
			queue_sender,
			statements_per_second: NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND)
				.expect("DEFAULT_STATEMENTS_PER_SECOND is nonzero"),
			metrics: None,
			initial_sync_timeout: Box::pin(futures::future::pending()),
			pending_affinities_timeout: Box::pin(futures::future::pending()),
			peer_eviction_timeout: Box::pin(futures::future::pending()),
			pending_initial_syncs: HashMap::new(),
			initial_sync_peer_queue: VecDeque::new(),
			next_initial_sync_id: 0,
			initial_sync_in_flight_bytes: 0,
			propagation_outboxes: PropagationOutboxes::new(None),
			in_flight_chunks: HashMap::new(),
			next_chunk_id: 0,
			propagation_in_flight_bytes: 0,
			parked_propagations: VecDeque::new(),
			pending_sends: FuturesUnordered::new(),
			deferred_peers: HashSet::new(),
			in_peers_full: DEFAULT_IN_PEERS,
			in_peers_light: 0,
			dropped_statements_during_sync: true,
			sync_recovery_peer: None,
			sync_recovery_readd_timeout: Box::pin(futures::future::pending()),
			dht: DhtOrchestrator::new(
				&[],
				None,
				crate::config::DEFAULT_BLOOM_FALSE_POS_RATE,
				network.local_peer_id(),
				topology_config(20, 3),
				"/statement/test".into(),
				None,
			),
		};

		handler.start_sync_recovery();

		// One remove call must have been issued for the connected peer
		{
			let removed = network.removed_reserved.lock().unwrap();
			assert_eq!(
				removed.len(),
				1,
				"Expected exactly one remove_peers_from_reserved_set call"
			);
			assert!(removed[0].contains(&connected_peer));
		}

		// The recovery peer must be stored and the timeout future must be armed
		assert_eq!(handler.sync_recovery_peer, Some(connected_peer));

		// Calling try_readd_sync_recovery_peer directly (as the select arm would after the future
		// resolves) must re-add the peer and clear the field
		handler.try_readd_sync_recovery_peer();
		assert!(handler.sync_recovery_peer.is_none());
		{
			let added = network.added_reserved.lock().unwrap();
			assert_eq!(added.len(), 1);
			let expected_addr: multiaddr::Multiaddr =
				iter::once(multiaddr::Protocol::P2p(connected_peer.into())).collect();
			assert!(added[0].contains(&expected_addr));
		}

		// Re-entry guard: restore state to simulate a second sync-end while recovery is still
		// in flight (sync_recovery_peer is Some). The second call must not issue another remove.
		{
			let peer2 = PeerId::random();
			handler.sync_recovery_peer = Some(peer2);
			handler.start_sync_recovery();
			assert_eq!(
				handler.sync_recovery_peer,
				Some(peer2),
				"Re-entry guard: recovery peer must not change on second call"
			);
			assert_eq!(
				network.removed_reserved.lock().unwrap().len(),
				1,
				"Re-entry guard: no extra remove call while recovery is in flight"
			);
		}
	}

	#[tokio::test]
	async fn sync_recovery_gated_by_dropped_statements_flag() {
		let make_peer = || Peer {
			statement_rate_limiter: default_statement_limiter(),
			affinity_rate_limiter: PeerRateLimiter::for_affinity_updates(),
			announce_rate_limiter: PeerRateLimiter::for_received_announces(),
			announce_budget: PeerRateLimiter::for_sent_announces(),
			send_budget: default_statement_limiter(),
			protocol_version: PeerProtocolVersion::V1,
			topic_affinity: None,
			is_light: false,
			inbound: true,
			pending_topic_affinity: None,
			full_replay_requested: false,
			sync_watermark: 0,
		};

		let make_handler =
			|network: TestNetwork, dropped: bool| -> StatementHandler<TestNetwork, TestSync> {
				let (sync, _) = TestSync::with_syncing(false);
				let (queue_sender, _) = async_channel::bounded(2);
				let local_peer = network.local_peer_id();
				let mut peers = HashMap::new();
				peers.insert(PeerId::random(), make_peer());
				StatementHandler {
					protocol_name: format!("/{STATEMENT_PROTOCOL_V1}").into(),
					notification_service: Box::new(TestNotificationService::new()),
					propagate_timeout: (Box::pin(futures::stream::pending())
						as Pin<Box<dyn Stream<Item = ()> + Send>>)
						.fuse(),
					pending_statements: FuturesUnordered::new(),
					pending_statements_peers: HashMap::new(),
					short_hashes_in_validation: HashSet::new(),
					recently_received_statements: HashMap::new(),
					network,
					sync,
					sync_event_stream: (Box::pin(futures::stream::pending())
						as Pin<Box<dyn Stream<Item = sc_network_sync::types::SyncEvent> + Send>>)
						.fuse(),
					network_event_stream: (Box::pin(futures::stream::pending())
						as Pin<Box<dyn Stream<Item = Event> + Send>>)
						.fuse(),
					peers,
					statement_store: Arc::new(TestStatementStore::new()),
					queue_sender,
					statements_per_second: NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND)
						.expect("DEFAULT_STATEMENTS_PER_SECOND is nonzero"),
					metrics: None,
					initial_sync_timeout: Box::pin(futures::future::pending()),
					pending_affinities_timeout: Box::pin(futures::future::pending()),
					peer_eviction_timeout: Box::pin(futures::future::pending()),
					pending_initial_syncs: HashMap::new(),
					initial_sync_peer_queue: VecDeque::new(),
					next_initial_sync_id: 0,
					initial_sync_in_flight_bytes: 0,
					propagation_outboxes: PropagationOutboxes::new(None),
					in_flight_chunks: HashMap::new(),
					next_chunk_id: 0,
					propagation_in_flight_bytes: 0,
					parked_propagations: VecDeque::new(),
					pending_sends: FuturesUnordered::new(),
					deferred_peers: HashSet::new(),
					in_peers_full: DEFAULT_IN_PEERS,
					in_peers_light: 0,
					dropped_statements_during_sync: dropped,
					sync_recovery_peer: None,
					sync_recovery_readd_timeout: Box::pin(pending().fuse()),
					dht: DhtOrchestrator::new(
						&[],
						None,
						crate::config::DEFAULT_BLOOM_FALSE_POS_RATE,
						local_peer,
						topology_config(20, 3),
						"/statement/test".into(),
						None,
					),
				}
			};

		// flag=false → no recovery
		let net = TestNetwork::new();
		let mut handler = make_handler(net.clone(), false);
		handler.start_sync_recovery();
		assert!(handler.sync_recovery_peer.is_none());
		assert!(net.get_removed_reserved().is_empty());

		// flag=true → recovery fires
		let net2 = TestNetwork::new();
		let mut handler2 = make_handler(net2.clone(), true);
		handler2.start_sync_recovery();
		assert!(handler2.sync_recovery_peer.is_some());
		assert_eq!(net2.get_removed_reserved().len(), 1);
	}

	#[test]
	fn send_paths_skip_expired_statements() {
		let mut live = new_live_statement();
		live.set_expiry_from_parts(u32::MAX, 0);
		live.set_plain_data(vec![1u8; 16]);
		let live_hash = live.hash();

		let mut stale = Statement::new();
		stale.set_plain_data(vec![2u8; 16]);
		let stale_hash = stale.hash();

		let mut gone = new_live_statement();
		gone.set_plain_data(vec![3u8; 16]);
		let gone_hash = gone.hash();

		let store = TestStatementStore::new();
		store.insert(stale.clone());
		store.insert(live.clone());

		let peer = Peer {
			statement_rate_limiter: default_statement_limiter(),
			affinity_rate_limiter: PeerRateLimiter::for_affinity_updates(),
			announce_rate_limiter: PeerRateLimiter::for_received_announces(),
			announce_budget: PeerRateLimiter::for_sent_announces(),
			send_budget: default_statement_limiter(),
			protocol_version: PeerProtocolVersion::V1,
			topic_affinity: None,
			is_light: false,
			inbound: true,
			pending_topic_affinity: None,
			full_replay_requested: false,
			sync_watermark: 0,
		};
		let who = PeerId::random();
		let received = HashMap::new();
		let pending = HashMap::new();
		let max_size = max_statement_payload_size(V1_ENVELOPE_OVERHEAD);

		let chunk = fetch_statement_chunk(
			&store,
			&received,
			&pending,
			&who,
			&peer,
			true,
			false,
			&[gone_hash, stale_hash, live_hash],
			max_size,
		)
		.expect("the test store never fails a fetch");
		assert_eq!(
			chunk.statements.iter().map(|(hash, _)| *hash).collect::<Vec<_>>(),
			vec![live_hash],
		);
		assert_eq!(chunk.processed, 3);
		assert_eq!(chunk.missing, 2, "the hash the store never held and the expired one are lost");

		let watermark = store.admission_watermark().expect("watermark is readable");
		let (batch, _size) = fetch_admitted_chunk(
			&store,
			&received,
			&pending,
			&who,
			&peer,
			&|_| false,
			None,
			0,
			watermark,
			max_size,
		)
		.expect("the test store never fails a walk");
		assert_eq!(
			batch.statements.iter().map(|(hash, _)| *hash).collect::<Vec<_>>(),
			vec![live_hash],
		);
	}

	#[test]
	fn initial_sync_serves_dht_targeted_statements_outside_the_explicit_filter() {
		let mut statement = new_live_statement();
		statement.set_plain_data(vec![1u8; 16]);
		statement.set_topic(0, topic(7));
		let hash = statement.hash();

		let store = TestStatementStore::new();
		store.insert(statement);

		let peer = Peer {
			statement_rate_limiter: default_statement_limiter(),
			affinity_rate_limiter: PeerRateLimiter::for_affinity_updates(),
			announce_rate_limiter: PeerRateLimiter::for_received_announces(),
			announce_budget: PeerRateLimiter::for_sent_announces(),
			send_budget: default_statement_limiter(),
			protocol_version: PeerProtocolVersion::V2,
			topic_affinity: Some(filter_over(&[topic(9)])),
			is_light: false,
			inbound: true,
			pending_topic_affinity: None,
			full_replay_requested: false,
			sync_watermark: 0,
		};
		let who = PeerId::random();
		let received = HashMap::new();
		let pending = HashMap::new();
		let max_size = max_statement_payload_size(V2_ENVELOPE_OVERHEAD);
		let watermark = store.admission_watermark().expect("watermark is readable");

		let fetch = |peer: &Peer, is_dht_target: &dyn Fn(&Statement) -> bool| {
			fetch_admitted_chunk(
				&store,
				&received,
				&pending,
				&who,
				peer,
				is_dht_target,
				None,
				0,
				watermark,
				max_size,
			)
			.expect("the test store never fails a walk")
			.0
		};

		let batch = fetch(&peer, &|_| false);
		assert!(batch.statements.is_empty(), "the explicit filter alone rejects the statement");

		let batch = fetch(&peer, &|stmt| stmt.topics().contains(&topic(7)));
		assert_eq!(
			batch.statements.iter().map(|(hash, _)| *hash).collect::<Vec<_>>(),
			vec![hash],
			"a DHT routing target receives the statement despite its explicit filter"
		);

		let mut light_peer = peer;
		light_peer.is_light = true;
		let batch = fetch(&light_peer, &|_| true);
		assert!(batch.statements.is_empty(), "a light peer gets filter matches only");
	}

	#[test]
	fn a_sync_after_a_filter_change_serves_only_the_newly_matched_statements() {
		let mut kept = new_live_statement();
		kept.set_plain_data(vec![1u8; 16]);
		kept.set_topic(0, topic(7));
		let mut added = new_live_statement();
		added.set_plain_data(vec![2u8; 16]);
		added.set_topic(0, topic(9));
		let added_hash = added.hash();
		let mut routed = new_live_statement();
		routed.set_plain_data(vec![3u8; 16]);
		routed.set_topic(0, topic(5));

		let store = TestStatementStore::new();
		store.insert(kept);
		store.insert(added);
		store.insert(routed);

		let peer = Peer {
			statement_rate_limiter: default_statement_limiter(),
			affinity_rate_limiter: PeerRateLimiter::for_affinity_updates(),
			announce_rate_limiter: PeerRateLimiter::for_received_announces(),
			announce_budget: PeerRateLimiter::for_sent_announces(),
			send_budget: default_statement_limiter(),
			protocol_version: PeerProtocolVersion::V2,
			topic_affinity: Some(filter_over(&[topic(7), topic(9)])),
			is_light: false,
			inbound: true,
			pending_topic_affinity: None,
			full_replay_requested: false,
			sync_watermark: 0,
		};
		let previous = filter_over(&[topic(7)]);
		let is_dht_target = |stmt: &Statement| stmt.topics().contains(&topic(5));
		let who = PeerId::random();
		let received = HashMap::new();
		let pending = HashMap::new();
		let max_size = max_statement_payload_size(V2_ENVELOPE_OVERHEAD);
		let watermark = store.admission_watermark().expect("watermark is readable");

		let fetch = |peer: &Peer, already_served: Option<&AffinityFilter>| {
			fetch_admitted_chunk(
				&store,
				&received,
				&pending,
				&who,
				peer,
				&is_dht_target,
				already_served,
				0,
				watermark,
				max_size,
			)
			.expect("the test store never fails a walk")
			.0
		};

		let batch = fetch(&peer, None);
		assert_eq!(batch.statements.len(), 3, "a full sync serves the filter and the DHT targets");

		let batch = fetch(&peer, Some(&previous));
		assert_eq!(
			batch.statements.iter().map(|(hash, _)| *hash).collect::<Vec<_>>(),
			vec![added_hash],
			"only the statement matched by the new filter alone is due"
		);

		let mut light_peer = peer;
		light_peer.is_light = true;
		let batch = fetch(&light_peer, Some(&previous));
		assert_eq!(
			batch.statements.iter().map(|(hash, _)| *hash).collect::<Vec<_>>(),
			vec![added_hash],
		);
	}

	#[test]
	fn drain_applies_the_explicit_filter_only_when_asked() {
		let mut statement = new_live_statement();
		statement.set_plain_data(vec![1u8; 16]);
		statement.set_topic(0, topic(7));
		let hash = statement.hash();

		let store = TestStatementStore::new();
		store.insert(statement);

		let mut peer = Peer::new_for_testing(
			NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND).expect("nonzero"),
			NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND * config::STATEMENTS_BURST_COEFFICIENT)
				.expect("nonzero"),
		);
		peer.topic_affinity = Some(filter_over(&[topic(9)]));
		let who = PeerId::random();
		let received = HashMap::new();
		let pending = HashMap::new();
		let max_size = max_statement_payload_size(V2_ENVELOPE_OVERHEAD);

		let fetch = |filter_by_affinity: bool| {
			fetch_statement_chunk(
				&store,
				&received,
				&pending,
				&who,
				&peer,
				filter_by_affinity,
				false,
				&[hash],
				max_size,
			)
			.expect("the test store never fails a fetch")
			.statements
		};

		assert!(fetch(true).is_empty(), "the explicit filter rejects the statement");
		assert_eq!(
			fetch(false).iter().map(|(hash, _)| *hash).collect::<Vec<_>>(),
			vec![hash],
			"an orchestrator target receives the statement despite its explicit filter"
		);
	}

	#[tokio::test]
	async fn affinity_is_dropped_during_major_sync_with_gate_off() {
		assert!(!dht_enabled(), "this case pins the gate-off path");
		let (mut handler, _store, _network, _notifications) = build_handler_no_peers();
		let peer = PeerId::random();
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;

		handler.sync.major_syncing.store(true, Ordering::Relaxed);
		let mut filter = AffinityFilter::new(BLOOM_SEED, 0.01, 100);
		filter.insert(&[0xAA; 32]);
		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer,
				notification: StatementMessage::ExplicitTopicAffinity(filter).encode().into(),
			})
			.await;

		let peer_data = handler.peers.get(&peer).expect("the substream is open");
		assert!(peer_data.pending_topic_affinity.is_none());
		assert!(handler.dropped_statements_during_sync);
	}
}
