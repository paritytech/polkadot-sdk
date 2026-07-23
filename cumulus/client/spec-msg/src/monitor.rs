// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! The receiver's relay chain monitor: watches imported relay blocks for the
//! own sources' newly *included* `StreamsRoot`s.
//!
//! The inclusion tier's trust anchor is the newest `StreamsRoot` a source
//! committed via `UMPSignal::Provides` in an *included* (enacted) candidate:
//! the relay chain pushes it into the source's `RecentProvides` ring, and a
//! receiver candidate's `Requires` entries are matched against exactly that
//! ring. This monitor is where a receiving collator learns that the ring
//! moved:
//!
//! - Per imported relay block, for each source the own runtime currently consumes
//!   (`SpecMsgApi::consumed_streams()` plus the peers of `out_channels()` — register-only sources
//!   count, version-gated like the archiver), it reads `RecentProvides[source]` and diffs it
//!   against what was already offered — each newly included root is handed to the fetch/verify
//!   pipeline (issue 11) exactly once, as [`RelayProvidesEvent::Included`] `(source, root,
//!   relay_block)`, oldest first. Authoring always targets the newest included root per source; the
//!   older entries only tell the fetch loop what it may already prove things under.
//! - As a latency win it also parses the source's `candidates_pending_availability` commitments
//!   ([`pending_provides`]): a root committed by a backed-but-not-yet-included candidate is offered
//!   as [`RelayProvidesEvent::Pending`] — a *prefetch hint* only. Fetching may start early;
//!   consumption and authoring key off included roots exclusively (the optimistic tier is
//!   post-MVP).
//!
//! Relay forks need no special machinery: offers are deduplicated by root
//! across branches (fetching is root-keyed and idempotent), and a re-org to
//! a fork without an enactment retracts nothing — the root simply stops
//! being in current state, against which issue 11 re-verifies anyway.
//!
//! Post-MVP, the speculative tier replaces this monitor's trigger role with
//! header digests (`DigestItem::Consensus(SPMS_ENGINE_ID, root)`) verified
//! via the off-chain block verification stack; the relay monitor then only
//! serves the inclusion fallback.

use std::{
	collections::{BTreeMap, BTreeSet, VecDeque},
	sync::Arc,
};

use codec::Decode;
use futures::StreamExt;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;

use cumulus_primitives_core::{
	relay_chain::{BlockId, CandidateCommitments},
	ParaId, SpecMsgApi,
};
use cumulus_primitives_spec_messaging::StreamsRoot;
use cumulus_relay_chain_interface::{
	BlockNumber as RelayBlockNumber, PHash, PHeader, RelayChainError, RelayChainInterface,
	RelayChainResult,
};

use crate::{pool::SpecMsgPool, LOG_TARGET};

/// A source's `StreamsRoot` observed on the relay chain — the trigger tuple
/// handed to the fetch/verify pipeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceProvides {
	/// The source (sending) parachain.
	pub source: ParaId,
	/// The source's stream commitment root.
	pub root: StreamsRoot,
	/// The relay chain block at which the root was observed.
	pub relay_block: PHash,
}

/// An offer from the relay `Provides` monitor to the fetch/verify pipeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelayProvidesEvent {
	/// The root entered the source's `RecentProvides` ring: the candidate committing it was
	/// *included*. This is the inclusion-tier trust anchor — fetch, verify and consume under
	/// it. Per source, offers arrive oldest first; the newest included root is the authoring
	/// target.
	Included(SourceProvides),
	/// The root was committed by a pending-availability (backed, not yet included) candidate
	/// of the source. Prefetch hint only: start fetching early; consumption and authoring
	/// key off [`Self::Included`] roots exclusively.
	Pending(SourceProvides),
}

/// Bound on remembered roots per source and tier.
///
/// The relay-side ring holds `RECENT_PROVIDES_WINDOW` (128) roots; remembering a few
/// windows' worth makes premature eviction practically impossible at a few KiB per source.
/// Eviction is harmless anyway: a re-offered root re-triggers a root-keyed, idempotent
/// fetch, and issue 11 re-verifies against relay state before consuming.
const SEEN_ROOTS_BOUND: usize = 512;

/// A bounded insertion-ordered set: the "was this root already offered" memory
/// of one source and tier.
#[derive(Default)]
struct SeenRoots {
	set: BTreeSet<StreamsRoot>,
	order: VecDeque<StreamsRoot>,
}

impl SeenRoots {
	/// Inserts `root`; `true` iff it was not yet present. Evicts the oldest
	/// remembered root beyond [`SEEN_ROOTS_BOUND`].
	fn insert(&mut self, root: StreamsRoot) -> bool {
		if !self.set.insert(root) {
			return false;
		}
		self.order.push_back(root);
		if self.order.len() > SEEN_ROOTS_BOUND {
			if let Some(evicted) = self.order.pop_front() {
				self.set.remove(&evicted);
			}
		}
		true
	}

	fn contains(&self, root: &StreamsRoot) -> bool {
		self.set.contains(root)
	}
}

/// Diffing state of one consumed source.
#[derive(Default)]
struct SourceState {
	/// Whether the source's ring was read at least once.
	bootstrapped: bool,
	/// Roots already offered as included.
	included: SeenRoots,
	/// Roots already offered as pending-availability prefetch hints.
	pending: SeenRoots,
}

/// The pure diffing core of the monitor, factored out of the async shell for
/// testability: rings in, offers out, exactly-once per root and tier.
#[derive(Default)]
struct ProvidesTracker {
	sources: BTreeMap<ParaId, SourceState>,
}

impl ProvidesTracker {
	/// Drops the state of sources no longer consumed (closed channels): their
	/// roots stop being offered, and a source that returns re-bootstraps.
	fn retain_sources(&mut self, keep: impl Fn(&ParaId) -> bool) {
		self.sources.retain(|source, _| keep(source));
	}

	/// Diffs `ring` — `RecentProvides[source]` as read at `relay_block`, oldest first —
	/// against what was already offered. Returns the newly included roots, oldest first.
	///
	/// On first sight of a source everything in the ring predates the monitor: only the
	/// newest entry is offered (it is the authoring target, and streams being append-only
	/// it covers the history below it); the rest just seeds the dedup memory.
	fn note_ring(
		&mut self,
		source: ParaId,
		ring: &[(StreamsRoot, RelayBlockNumber)],
		relay_block: PHash,
	) -> Vec<SourceProvides> {
		let state = self.sources.entry(source).or_default();
		if !state.bootstrapped {
			state.bootstrapped = true;
			for (root, _) in ring {
				state.included.insert(*root);
			}
			return ring
				.last()
				.map(|(root, _)| SourceProvides { source, root: *root, relay_block })
				.into_iter()
				.collect();
		}

		ring.iter()
			.filter(|(root, _)| state.included.insert(*root))
			.map(|(root, _)| SourceProvides { source, root: *root, relay_block })
			.collect()
	}

	/// Notes a root committed by a pending-availability candidate of `source`; returns
	/// the prefetch hint unless the root was already offered (as either tier — a root
	/// already included has been triggered for real).
	fn note_pending(
		&mut self,
		source: ParaId,
		root: StreamsRoot,
		relay_block: PHash,
	) -> Option<SourceProvides> {
		let state = self.sources.entry(source).or_default();
		if state.included.contains(&root) {
			return None;
		}
		state
			.pending
			.insert(root)
			.then_some(SourceProvides { source, root, relay_block })
	}
}

/// The `Provides` root a candidate's commitments carry, if any.
///
/// Pure format parse ([`CandidateCommitments::ump_signals`]): the relay chain already
/// validated the signals of anything backed, so a candidate whose signals fail to parse
/// yields `None` (skipped), never an error.
pub fn pending_provides(commitments: &CandidateCommitments) -> Option<StreamsRoot> {
	commitments.ump_signals().ok().and_then(|signals| signals.provides())
}

/// Reads `RecentProvides[source]` — the ring of the source's recently *included*
/// [`StreamsRoot`]s, oldest first, each paired with the relay block number it was pushed
/// at — from relay chain state at `relay_block`. An absent value is an empty ring (the
/// source never provided, or was offboarded).
///
/// This is also the read the inherent provider (issue 11) performs at its candidate's
/// relay parent to pick the authoring target: the newest entry.
pub async fn read_recent_provides(
	relay_chain: &impl RelayChainInterface,
	relay_block: PHash,
	source: ParaId,
) -> RelayChainResult<Vec<(StreamsRoot, RelayBlockNumber)>> {
	let key =
		cumulus_primitives_core::relay_chain::well_known_keys::spec_msg_recent_provides(source);
	let Some(value) = relay_chain.get_storage_by_key(relay_block, &key).await? else {
		return Ok(Vec::new());
	};
	// The runtime-side `RecentRoots` is a newtype over a `BoundedVec`, which SCALE-encodes
	// exactly like a plain `Vec` (layout pinned by a test next to the relay-side pallet).
	Vec::decode(&mut value.as_slice()).map_err(RelayChainError::from)
}

/// Errors handling one relay block notification.
#[derive(Debug, thiserror::Error)]
enum MonitorError {
	#[error("relay chain error: {0}")]
	RelayChain(#[from] RelayChainError),
	#[error("runtime api error: {0}")]
	Api(#[from] sp_api::ApiError),
	#[error("events channel closed")]
	EventsChannelClosed,
}

/// Runs the relay `Provides` monitor until the relay import stream ends or every
/// [`RelayProvidesEvent`] receiver is dropped. Spawn next to the receiver-side fetch
/// pipeline (issue 11), which consumes the paired `async_channel::Receiver`.
pub async fn run_relay_provides_monitor<Block, Client, RCInterface>(
	parachain: Arc<Client>,
	relay_chain: RCInterface,
	pool: Arc<SpecMsgPool>,
	events: async_channel::Sender<RelayProvidesEvent>,
) where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + HeaderBackend<Block>,
	Client::Api: SpecMsgApi<Block>,
	RCInterface: RelayChainInterface,
{
	let mut imports = match relay_chain.import_notification_stream().await {
		Ok(imports) => imports,
		Err(error) => {
			tracing::error!(
				target: LOG_TARGET,
				?error,
				"Failed to subscribe to relay chain imports; relay provides monitor idle",
			);
			return;
		},
	};

	let mut tracker = ProvidesTracker::default();

	// Process the relay tip once up front: a (re)started collator must not wait for the
	// next relay block to learn the current inclusion frontier of its sources.
	match startup_header(&relay_chain).await {
		Ok(Some(header)) => {
			match process_relay_block(
				&*parachain,
				&relay_chain,
				&mut tracker,
				&pool,
				&events,
				&header,
			)
			.await
			{
				Ok(()) => (),
				Err(MonitorError::EventsChannelClosed) => return,
				Err(error) => tracing::debug!(
					target: LOG_TARGET,
					?error,
					"Failed to process the relay tip at startup",
				),
			}
		},
		Ok(None) => (),
		Err(error) => tracing::debug!(
			target: LOG_TARGET,
			?error,
			"Failed to fetch the relay tip at startup",
		),
	}

	while let Some(header) = imports.next().await {
		match process_relay_block(&*parachain, &relay_chain, &mut tracker, &pool, &events, &header)
			.await
		{
			Ok(()) => (),
			Err(MonitorError::EventsChannelClosed) => return,
			Err(error) => tracing::warn!(
				target: LOG_TARGET,
				?error,
				relay_block = ?header.hash(),
				"Failed to process imported relay block",
			),
		}
	}
}

/// The current best relay header, if any is retrievable.
async fn startup_header(
	relay_chain: &impl RelayChainInterface,
) -> RelayChainResult<Option<PHeader>> {
	let best = relay_chain.best_block_hash().await?;
	relay_chain.header(BlockId::Hash(best)).await
}

/// Offers everything one imported relay block newly says about the consumed sources.
async fn process_relay_block<Block, Client>(
	parachain: &Client,
	relay_chain: &impl RelayChainInterface,
	tracker: &mut ProvidesTracker,
	pool: &SpecMsgPool,
	events: &async_channel::Sender<RelayProvidesEvent>,
	header: &PHeader,
) -> Result<(), MonitorError>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + HeaderBackend<Block>,
	Client::Api: SpecMsgApi<Block>,
{
	// Historical rings are pointless triggers; the post-sync tip re-bootstraps every source.
	if relay_chain.is_major_syncing().await? {
		return Ok(());
	}

	// The version gate, exactly like the archiver: no `SpecMsgApi`, nothing is consumed.
	let best = parachain.info().best_hash;
	if !parachain.runtime_api().has_api::<dyn SpecMsgApi<Block>>(best)? {
		return Ok(());
	}
	// Register-only sources count too: an outbound channel's handshake
	// completes by reading the peer's ack register under the peer's
	// included root, whether or not any inbound channel consumes that
	// peer's data streams (one-directional channels are first-class).
	let consumed = parachain.runtime_api().consumed_streams(best)?;
	let out_channels = parachain.runtime_api().out_channels(best)?;
	let sources: BTreeSet<ParaId> = consumed
		.keys()
		.copied()
		.chain(out_channels.keys().map(|channel| channel.peer))
		.collect();
	tracker.retain_sources(|source| sources.contains(source));

	let relay_block = header.hash();
	for source in sources {
		// The inclusion-tier trigger: roots newly present in the source's ring.
		let bootstrap = tracker.sources.get(&source).map_or(true, |state| !state.bootstrapped);
		let ring = read_recent_provides(relay_chain, relay_block, source).await?;
		for included in tracker.note_ring(source, &ring, relay_block) {
			tracing::info!(
				target: LOG_TARGET,
				source = %u32::from(source),
				root = ?included.root,
				?relay_block,
				bootstrap,
				"Included root offered to the fetcher",
			);
			// A round for this source is now imminent: record it before the
			// send so authoring's grace window sees the offer even while it
			// is still queued in the (bounded) monitor→fetcher channel — the
			// late-offer photo-finish (issue 10). Cleared when the fetcher
			// begins the real round, expired under the window's bound
			// otherwise.
			pool.note_pending_offer(included.source);
			events
				.send(RelayProvidesEvent::Included(included))
				.await
				.map_err(|_| MonitorError::EventsChannelClosed)?;
		}

		// The prefetch hint: roots committed by the source's pending-availability
		// candidates. Deliberately non-fatal — hints are a latency win, not correctness.
		match relay_chain.candidates_pending_availability(relay_block, source).await {
			Ok(candidates) => {
				for root in candidates.iter().filter_map(|c| pending_provides(&c.commitments)) {
					if let Some(hint) = tracker.note_pending(source, root, relay_block) {
						events
							.send(RelayProvidesEvent::Pending(hint))
							.await
							.map_err(|_| MonitorError::EventsChannelClosed)?;
					}
				}
			},
			Err(error) => tracing::debug!(
				target: LOG_TARGET,
				?error,
				source = %u32::from(source),
				"Failed to read pending-availability candidates",
			),
		}
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;
	use cumulus_primitives_core::relay_chain::{
		ClaimQueueOffset, CoreSelector, RequiresSet, UMPSignal, UMP_SEPARATOR,
	};
	use polkadot_core_primitives::Hash;

	fn root(byte: u8) -> StreamsRoot {
		StreamsRoot(Hash::repeat_byte(byte))
	}

	/// A root derived from an index, unique for `i < u32::MAX`.
	fn root_at(i: u32) -> StreamsRoot {
		StreamsRoot(Hash::from_low_u64_be(i as u64 + 1))
	}

	fn relay(byte: u8) -> PHash {
		PHash::repeat_byte(byte)
	}

	fn offer(source: ParaId, root: StreamsRoot, relay_block: PHash) -> SourceProvides {
		SourceProvides { source, root, relay_block }
	}

	#[test]
	fn bootstrap_offers_only_the_newest_included_root() {
		let mut tracker = ProvidesTracker::default();
		let source = ParaId::from(2000);

		// First sight: the whole ring predates the monitor — only the newest is offered.
		let ring = [(root(1), 10), (root(2), 11), (root(3), 12)];
		assert_eq!(
			tracker.note_ring(source, &ring, relay(0xA1)),
			vec![offer(source, root(3), relay(0xA1))],
		);
		// The same ring at the next relay block offers nothing new.
		assert!(tracker.note_ring(source, &ring, relay(0xA2)).is_empty());
		// A source that never provided bootstraps silently.
		assert!(tracker.note_ring(ParaId::from(2001), &[], relay(0xA1)).is_empty());
	}

	#[test]
	fn diffing_yields_each_newly_included_root_exactly_once_oldest_first() {
		let mut tracker = ProvidesTracker::default();
		let source = ParaId::from(2000);

		tracker.note_ring(source, &[(root(1), 10)], relay(0xA1));

		// Two enactments land in one relay block: both offered, oldest first.
		let grown = [(root(1), 10), (root(2), 11), (root(3), 11)];
		assert_eq!(
			tracker.note_ring(source, &grown, relay(0xA2)),
			vec![offer(source, root(2), relay(0xA2)), offer(source, root(3), relay(0xA2))],
		);
		assert!(tracker.note_ring(source, &grown, relay(0xA3)).is_empty());

		// The ring rotating the oldest entry out changes nothing about novelty.
		let rotated = [(root(2), 11), (root(3), 11), (root(4), 12)];
		assert_eq!(
			tracker.note_ring(source, &rotated, relay(0xA4)),
			vec![offer(source, root(4), relay(0xA4))],
		);
	}

	#[test]
	fn forks_offer_a_root_exactly_once_and_reorgs_retract_nothing() {
		let mut tracker = ProvidesTracker::default();
		let source = ParaId::from(2000);

		tracker.note_ring(source, &[], relay(0xA0));

		// Fork A enacts the source candidate committing `root(1)`.
		assert_eq!(
			tracker.note_ring(source, &[(root(1), 11)], relay(0xA1)),
			vec![offer(source, root(1), relay(0xA1))],
		);
		// Fork B enacts the very same candidate: the root is not offered again.
		assert!(tracker.note_ring(source, &[(root(1), 11)], relay(0xB1)).is_empty());

		// Re-org to a fork WITHOUT the enactment: nothing is offered and nothing needs
		// retracting — the root simply is not in that branch's state, against which the
		// fetch pipeline re-verifies anyway.
		assert!(tracker.note_ring(source, &[], relay(0xC1)).is_empty());
		// The abandoned branch's fetch was not wasted either: when the enactment lands
		// on the surviving fork, the root is known already and stays offered-once.
		assert!(tracker.note_ring(source, &[(root(1), 13)], relay(0xC2)).is_empty());
		// New roots on the surviving fork trigger as usual.
		assert_eq!(
			tracker.note_ring(source, &[(root(1), 13), (root(2), 14)], relay(0xC3)),
			vec![offer(source, root(2), relay(0xC3))],
		);
	}

	#[test]
	fn pending_roots_hint_once_and_still_trigger_on_inclusion() {
		let mut tracker = ProvidesTracker::default();
		let source = ParaId::from(2000);

		tracker.note_ring(source, &[], relay(0xA0));

		// A pending-availability commitment hints exactly once.
		assert_eq!(
			tracker.note_pending(source, root(1), relay(0xA1)),
			Some(offer(source, root(1), relay(0xA1))),
		);
		assert_eq!(tracker.note_pending(source, root(1), relay(0xA2)), None);

		// Inclusion of the hinted root still triggers — authoring keys off included only.
		assert_eq!(
			tracker.note_ring(source, &[(root(1), 12)], relay(0xA2)),
			vec![offer(source, root(1), relay(0xA2))],
		);
		// Once included, the root cannot regress to a hint.
		assert_eq!(tracker.note_pending(source, root(1), relay(0xA3)), None);
	}

	#[test]
	fn dropped_sources_are_forgotten_and_rebootstrap_on_return() {
		let mut tracker = ProvidesTracker::default();
		let source = ParaId::from(2000);
		let ring = [(root(1), 10), (root(2), 11)];

		tracker.note_ring(source, &ring, relay(0xA1));

		// The channel closes: the source drops out of `consumed_streams()`.
		tracker.retain_sources(|_| false);

		// Reopened later: first sight again — only the (new) newest root is offered,
		// even though older entries were seen in the source's previous life.
		let returned = [(root(1), 10), (root(2), 11), (root(3), 12)];
		assert_eq!(
			tracker.note_ring(source, &returned, relay(0xA9)),
			vec![offer(source, root(3), relay(0xA9))],
		);
	}

	#[test]
	fn seen_roots_memory_is_bounded() {
		let mut tracker = ProvidesTracker::default();
		let source = ParaId::from(2000);

		tracker.note_ring(source, &[], relay(0xA0));
		for i in 0..2 * SEEN_ROOTS_BOUND as u32 {
			let offered = tracker.note_ring(source, &[(root_at(i), i)], relay(0xA1));
			assert_eq!(offered.len(), 1);
		}

		let state = &tracker.sources[&source];
		assert_eq!(state.included.set.len(), SEEN_ROOTS_BOUND);
		assert_eq!(state.included.order.len(), SEEN_ROOTS_BOUND);
		// Evicted memory only ever means a duplicate (harmless) offer, never a miss.
		assert!(state.included.contains(&root_at(2 * SEEN_ROOTS_BOUND as u32 - 1)));
		assert!(!state.included.contains(&root_at(0)));
	}

	#[test]
	fn pending_provides_extracts_the_root_among_other_signals() {
		let commitments = |upward_messages: Vec<Vec<u8>>| CandidateCommitments {
			upward_messages: upward_messages.try_into().expect("few messages; qed"),
			..Default::default()
		};
		let select_core = UMPSignal::SelectCore(CoreSelector(3), ClaimQueueOffset(1));
		let approved_peer =
			UMPSignal::ApprovedPeer(vec![1u8; 32].try_into().expect("32 <= 64; qed"));
		let requires = UMPSignal::Requires(
			RequiresSet::try_from_iter([(ParaId::from(2000), root(9))]).expect("one entry; qed"),
		);

		// A candidate carrying every signal kind (and a plain XCM message up front).
		let full = commitments(vec![
			vec![1, 2, 3],
			UMP_SEPARATOR,
			select_core.encode(),
			approved_peer.encode(),
			UMPSignal::Provides(root(7)).encode(),
			requires.encode(),
		]);
		assert_eq!(pending_provides(&full), Some(root(7)));

		// No separator, signals without `Provides`, or malformed signals: `None`.
		assert_eq!(pending_provides(&CandidateCommitments::default()), None);
		let no_provides = commitments(vec![UMP_SEPARATOR, select_core.encode(), requires.encode()]);
		assert_eq!(pending_provides(&no_provides), None);
		let malformed = commitments(vec![UMP_SEPARATOR, vec![0xFF]]);
		assert_eq!(pending_provides(&malformed), None);
	}
}
