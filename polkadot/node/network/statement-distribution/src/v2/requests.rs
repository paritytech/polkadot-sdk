// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! A requester for full information on candidates.
//!
//! 1. We use `RequestManager::get_or_insert().get_mut()` to add and mutate [`RequestedCandidate`]s,
//!    either setting the
//! priority or adding a peer we know has the candidate. We currently prioritize "cluster"
//! candidates (those from our own group, although the cluster mechanism could be made to include
//! multiple groups in the future) over "grid" candidates (those from other groups).
//!
//! 2. The main loop of the module will invoke [`RequestManager::next_request`] in a loop until it
//!    returns `None`,
//! dispatching all requests with the `NetworkBridgeTxMessage`. The receiving half of the channel is
//! owned by the [`RequestManager`].
//!
//! 3. The main loop of the module will also select over [`RequestManager::await_incoming`] to
//!    receive
//! [`UnhandledResponse`]s, which it then validates using [`UnhandledResponse::validate_response`]
//! (which requires state not owned by the request manager).

use super::{
	seconded_and_sufficient, TransposedClaimQueue, BENEFIT_VALID_RESPONSE, BENEFIT_VALID_STATEMENT,
	COST_IMPROPERLY_DECODED_RESPONSE, COST_INVALID_RESPONSE, COST_INVALID_SESSION_INDEX,
	COST_INVALID_SIGNATURE, COST_INVALID_UMP_SIGNALS, COST_UNREQUESTED_RESPONSE_STATEMENT,
	REQUEST_RETRY_DELAY,
};
use crate::{metrics::Metrics, LOG_TARGET};

use arrayvec::ArrayVec;
use bitvec::prelude::{BitVec, Lsb0};
use polkadot_node_network_protocol::{
	request_response::{
		outgoing::{Recipient as RequestRecipient, RequestError},
		v2::{AttestedCandidateRequest, AttestedCandidateResponse},
		OutgoingRequest, OutgoingResult, MAX_PARALLEL_ATTESTED_CANDIDATE_REQUESTS,
	},
	v3::StatementFilter,
	PeerId, UnifiedReputationChange as Rep,
};
use polkadot_primitives::{
	CandidateHash, CommittedCandidateReceiptV2 as CommittedCandidateReceipt, CompactStatement,
	GroupIndex, Hash, Id as ParaId, PersistedValidationData, SessionIndex, SignedStatement,
	SigningContext, ValidatorId, ValidatorIndex,
};

use futures::{future::BoxFuture, prelude::*, stream::FuturesUnordered};

use std::{
	collections::{
		hash_map::{Entry as HEntry, HashMap},
		HashSet, VecDeque,
	},
	time::{Duration, Instant},
};

/// After this much time without a response, dispatch a parallel `AttestedCandidateRequest`
/// to a different advertiser of the same candidate. First-valid response wins.
///
/// Slots beyond the second are dispatched staggered: slot `N+1` fires after `N *
/// PARALLEL_FETCH_THRESHOLD` has elapsed since the first request — each successive slot is
/// further evidence that the candidate is genuinely slow to propagate, not that one peer
/// happened to be sluggish.
///
/// Must be strictly less than the network-level `ATTESTED_CANDIDATE_TIMEOUT` (2500ms in
/// `polkadot-node-network-protocol::request_response`) so the parallel fetch is dispatched
/// while the original request is still alive.
pub(crate) const PARALLEL_FETCH_THRESHOLD: Duration = Duration::from_millis(600);

/// Maximum number of `AttestedCandidateRequest`s that can be in flight simultaneously for a
/// single candidate.
///
/// Set to 4 to match the k=4 random-gossip fan-out introduced by the next part of issue
/// #12028 — once random manifest gossip ships, each non-backer validator may learn about a
/// candidate from cluster + grid + up to 4 random originators + their forwarders, so up to
/// 4 parallel fetches are useful to escape multiple slow peers without exceeding the global
/// in-flight cap of `2 * MAX_PARALLEL_ATTESTED_CANDIDATE_REQUESTS = 10`.
///
/// In PR1 (parallel fetch alone) the practical effective cap is still ≤ 2 in most cases:
/// a candidate typically has only ~2 advertisers known until random gossip is enabled, so
/// `find_request_target_with_update` returns `None` for higher slots. Staggered firing
/// also means later slots only fire if many `PARALLEL_FETCH_THRESHOLD` intervals have
/// elapsed without a response.
pub(crate) const MAX_PARALLEL_FETCH_SLOTS: usize = 4;

const _: () =
	assert!(MAX_PARALLEL_FETCH_SLOTS >= 1, "MAX_PARALLEL_FETCH_SLOTS must be at least 1",);

// Sanity check against the network-level hard request timeout. The exact value of
// `ATTESTED_CANDIDATE_TIMEOUT` is private to the protocol crate; we hard-code the expected
// value here to catch any divergence.
const _: () = assert!(
	PARALLEL_FETCH_THRESHOLD.as_millis() < 2500,
	"PARALLEL_FETCH_THRESHOLD must be less than the network-level ATTESTED_CANDIDATE_TIMEOUT",
);

/// Which fetch attempt resolved — the original or one of the parallel ones fired after the
/// threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchSlot {
	/// The original request, sent on when we firt learn about the candidate.
	First,
	/// Any parallel request.
	Parallel,
}

impl FetchSlot {
	/// Static-string label suitable for prometheus.
	pub fn as_str(self) -> &'static str {
		match self {
			FetchSlot::First => "first",
			FetchSlot::Parallel => "parallel",
		}
	}
}

/// Outcome classification for a single fetch attempt. Used for metrics labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchOutcome {
	/// Response was valid and sufficient (this slot was the winner).
	Success,
	/// Response arrived but was rejected with a reputation penalty (e.g. bad signature).
	Invalid,
	/// Response did not arrive in time, or transport error (no reputation change).
	TimeoutOther,
	/// Response arrived after the candidate was already resolved or pruned.
	Dropped,
}

impl FetchOutcome {
	/// Static-string label suitable for prometheus.
	pub fn as_str(self) -> &'static str {
		match self {
			FetchOutcome::Success => "success",
			FetchOutcome::Invalid => "invalid",
			FetchOutcome::TimeoutOther => "timeout_other",
			FetchOutcome::Dropped => "dropped",
		}
	}
}

/// Observation emitted for every fetch attempt's resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FetchCompletion {
	pub slot: FetchSlot,
	pub outcome: FetchOutcome,
	pub duration: Duration,
}

/// An identifier for a candidate.
///
/// In this module, we are requesting candidates
/// for which we have no information other than the candidate hash and statements signed
/// by validators. It is possible for validators for multiple groups to abuse this lack of
/// information: until we actually get the preimage of this candidate we cannot confirm
/// anything other than the candidate hash.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CandidateIdentifier {
	/// The scheduling-parent this candidate is ostensibly under.
	pub scheduling_parent: Hash,
	/// The hash of the candidate.
	pub candidate_hash: CandidateHash,
	/// The index of the group claiming to be assigned to the candidate's
	/// para.
	pub group_index: GroupIndex,
}

struct TaggedResponse {
	identifier: CandidateIdentifier,
	requested_peer: PeerId,
	slot: FetchSlot,
	sent_at: Instant,
	props: RequestProperties,
	response: OutgoingResult<AttestedCandidateResponse>,
}

/// A pending request.
///
/// A candidate may have up to `MAX_PARALLEL_FETCH_SLOTS` in-flight requests at any time. The
/// first is dispatched as soon as the candidate becomes eligible; subsequent parallel slots
/// are fired staggered, each after one additional `PARALLEL_FETCH_THRESHOLD` has elapsed
/// since the first request, to *different* advertisers. First valid response wins; losers
/// are dropped via the existing `CandidateRequestStatus::Outdated` path, with no reputation
/// change.
///
/// Per-slot timing/identity travels with the response future via [`TaggedResponse`].
#[derive(Debug)]
pub struct RequestedCandidate {
	priority: Priority,
	known_by: VecDeque<PeerId>,
	/// Peers we are currently waiting on responses from for this candidate. Bounded to
	/// `MAX_PARALLEL_FETCH_SLOTS`.
	in_flight: ArrayVec<PeerId, MAX_PARALLEL_FETCH_SLOTS>,
	/// Time the first slot was dispatched, used to gate the staggered parallel-fire timer.
	first_request_sent_at: Option<Instant>,
	/// Time we first learned about this candidate.
	first_learned_at: Instant,
	/// The timestamp for the next time we should retry, if the response failed.
	next_retry_time: Option<Instant>,
}

impl RequestedCandidate {
	/// True if the candidate has no in-flight slots and the retry-cooldown window has elapsed.
	/// Used to dispatch the first request.
	fn is_pending_first(&self) -> bool {
		if !self.in_flight.is_empty() {
			return false;
		}

		if let Some(next_retry_time) = self.next_retry_time {
			if Instant::now() < next_retry_time {
				return false;
			}
		}

		true
	}

	/// True if at least one slot is in-flight, there is room for another (`< MAX`), and the
	/// staggered parallel-fire threshold has elapsed since the first request. Used to dispatch
	/// any parallel slot beyond the first. The staggering formula is `first_request_sent_at +
	/// in_flight.len() * PARALLEL_FETCH_THRESHOLD` — so slot N+1 fires N thresholds after the
	/// first.
	fn is_pending_parallel(&self) -> bool {
		if self.in_flight.is_empty() || self.in_flight.is_full() {
			return false;
		}

		let Some(sent_at) = self.first_request_sent_at else { return false };
		let wait = PARALLEL_FETCH_THRESHOLD
			.checked_mul(self.in_flight.len() as u32)
			.expect("len <= MAX_PARALLEL_FETCH_SLOTS << u32::MAX; qed");
		Instant::now() >= sent_at + wait
	}
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Origin {
	Cluster = 0,
	Unspecified = 1,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Priority {
	origin: Origin,
	attempts: usize,
}

/// An entry for manipulating a requested candidate.
pub struct Entry<'a> {
	prev_index: usize,
	identifier: CandidateIdentifier,
	by_priority: &'a mut Vec<(Priority, CandidateIdentifier)>,
	requested: &'a mut RequestedCandidate,
}

impl<'a> Entry<'a> {
	/// Add a peer to the set of known peers.
	pub fn add_peer(&mut self, peer: PeerId) {
		if !self.requested.known_by.contains(&peer) {
			self.requested.known_by.push_back(peer);
		}
	}

	/// Note that the candidate is required for the cluster.
	pub fn set_cluster_priority(&mut self) {
		self.requested.priority.origin = Origin::Cluster;

		insert_or_update_priority(
			&mut *self.by_priority,
			Some(self.prev_index),
			self.identifier.clone(),
			self.requested.priority.clone(),
		);
	}
}

/// A manager for outgoing requests.
pub struct RequestManager {
	requests: HashMap<CandidateIdentifier, RequestedCandidate>,
	// sorted by priority.
	by_priority: Vec<(Priority, CandidateIdentifier)>,
	// all unique identifiers for the candidate.
	unique_identifiers: HashMap<CandidateHash, HashSet<CandidateIdentifier>>,
}

impl RequestManager {
	/// Create a new [`RequestManager`].
	pub fn new() -> Self {
		RequestManager {
			requests: HashMap::new(),
			by_priority: Vec::new(),
			unique_identifiers: HashMap::new(),
		}
	}

	/// Gets an [`Entry`] for mutating a request and inserts it if the
	/// manager doesn't store this request already.
	pub fn get_or_insert(
		&mut self,
		scheduling_parent: Hash,
		candidate_hash: CandidateHash,
		group_index: GroupIndex,
	) -> Entry<'_> {
		let identifier = CandidateIdentifier { scheduling_parent, candidate_hash, group_index };

		let (candidate, fresh) = match self.requests.entry(identifier.clone()) {
			HEntry::Occupied(e) => (e.into_mut(), false),
			HEntry::Vacant(e) => (
				e.insert(RequestedCandidate {
					priority: Priority { attempts: 0, origin: Origin::Unspecified },
					known_by: VecDeque::new(),
					in_flight: ArrayVec::new(),
					first_request_sent_at: None,
					first_learned_at: Instant::now(),
					next_retry_time: None,
				}),
				true,
			),
		};

		let priority_index = if fresh {
			self.unique_identifiers
				.entry(candidate_hash)
				.or_default()
				.insert(identifier.clone());

			insert_or_update_priority(
				&mut self.by_priority,
				None,
				identifier.clone(),
				candidate.priority.clone(),
			)
		} else {
			match self
				.by_priority
				.binary_search(&(candidate.priority.clone(), identifier.clone()))
			{
				Ok(i) => i,
				Err(_) => unreachable!("requested candidates always have a priority entry; qed"),
			}
		};

		Entry {
			prev_index: priority_index,
			identifier,
			by_priority: &mut self.by_priority,
			requested: candidate,
		}
	}

	/// Remove all pending requests for the given candidate.
	pub fn remove_for(&mut self, candidate: CandidateHash) {
		if let Some(identifiers) = self.unique_identifiers.remove(&candidate) {
			self.by_priority.retain(|(_priority, id)| !identifiers.contains(&id));
			for id in identifiers {
				self.requests.remove(&id);
			}
		}
	}

	/// Remove all requests associated with the given scheduling parent.
	pub fn remove_by_scheduling_parent(&mut self, scheduling_parent: Hash) {
		let mut candidate_hashes = HashSet::new();

		// Remove from `by_priority` and `requests`.
		self.by_priority.retain(|(_priority, id)| {
			let retain = scheduling_parent != id.scheduling_parent;
			if !retain {
				self.requests.remove(id);
				candidate_hashes.insert(id.candidate_hash);
			}
			retain
		});

		// Remove from `unique_identifiers`.
		for candidate_hash in candidate_hashes {
			match self.unique_identifiers.entry(candidate_hash) {
				HEntry::Occupied(mut entry) => {
					entry.get_mut().retain(|id| scheduling_parent != id.scheduling_parent);
					if entry.get().is_empty() {
						entry.remove();
					}
				},
				// We can expect to encounter vacant entries, but only if nodes are misbehaving and
				// we don't use a deduplicating collection; there are no issues from ignoring it.
				HEntry::Vacant(_) => (),
			}
		}

		gum::debug!(
			target: LOG_TARGET,
			"Requests remaining after cleanup: {}",
			self.by_priority.len(),
		);
	}

	/// Returns true if there are pending requests that are dispatchable, either as a fresh
	/// first request or as any parallel slot whose staggered threshold has elapsed.
	pub fn has_pending_requests(&self) -> bool {
		for (_id, entry) in &self.requests {
			if entry.is_pending_first() || entry.is_pending_parallel() {
				return true;
			}
		}

		false
	}

	#[cfg(test)]
	pub(super) fn requests_count_by_scheduling_parent(&self, scheduling_parent: Hash) -> usize {
		self.requests
			.keys()
			.filter(|id| id.scheduling_parent == scheduling_parent)
			.count()
	}

	#[cfg(test)]
	pub(super) fn total_requests_count(&self) -> usize {
		self.requests.len()
	}

	/// Test-only: rewind a candidate's `first_request_sent_at` far enough that all parallel
	/// slots up to `MAX_PARALLEL_FETCH_SLOTS` are immediately dispatchable (staggered firing
	/// will collapse to a burst because every threshold has already elapsed). Lets tests
	/// exercise parallel-slot dispatch deterministically without sleeping on the wall clock.
	#[cfg(test)]
	pub(super) fn force_parallel_fire_ready(&mut self, identifier: &CandidateIdentifier) {
		if let Some(entry) = self.requests.get_mut(identifier) {
			let offset = PARALLEL_FETCH_THRESHOLD * MAX_PARALLEL_FETCH_SLOTS as u32 +
				Duration::from_millis(1);
			entry.first_request_sent_at = Some(Instant::now() - offset);
		}
	}

	/// Test-only: number of peers currently in flight for a candidate.
	#[cfg(test)]
	pub(super) fn in_flight_count_for(&self, identifier: &CandidateIdentifier) -> usize {
		self.requests.get(identifier).map(|e| e.in_flight.len()).unwrap_or(0)
	}

	/// Returns an instant at which the next request to be retried will be ready.
	///
	/// Only candidates with no in-flight slots are eligible for retry; a candidate that is
	/// already mid-flight (one or two slots filled) doesn't need a retry timer.
	pub fn next_retry_time(&mut self) -> Option<Instant> {
		let mut next = None;
		for (_id, request) in
			self.requests.iter().filter(|(_id, request)| request.in_flight.is_empty())
		{
			if let Some(next_retry_time) = request.next_retry_time {
				if next.map_or(true, |next| next_retry_time < next) {
					next = Some(next_retry_time);
				}
			}
		}
		next
	}

	/// Returns the soonest instant at which any candidate's next parallel slot becomes ready
	/// to dispatch. For a candidate with `k` slots already in flight (`1 <= k < MAX`), the
	/// next slot fires at `first_request_sent_at + k * PARALLEL_FETCH_THRESHOLD`
	///
	/// Returns `None` if no candidate has a parallel slot available.
	/// available (either nothing in flight, or all MAX slots filled).
	pub fn next_parallel_fire_time(&self) -> Option<Instant> {
		let mut next = None;
		for (_id, request) in &self.requests {
			if request.in_flight.is_empty() || request.in_flight.is_full() {
				continue;
			}
			let Some(sent_at) = request.first_request_sent_at else { continue };
			let wait = PARALLEL_FETCH_THRESHOLD
				.checked_mul(request.in_flight.len() as u32)
				.expect("len <= MAX_PARALLEL_FETCH_SLOTS << u32::MAX; qed");
			let fire_at = sent_at + wait;
			if next.map_or(true, |n| fire_at < n) {
				next = Some(fire_at);
			}
		}
		next
	}

	/// Yields the next request to dispatch, if there is any.
	///
	/// This function accepts two closures as an argument.
	///
	/// The first closure is used to gather information about the desired
	/// properties of a response, which is used to select targets and validate
	/// the response later on.
	///
	/// The second closure is used to determine the specific advertised
	/// statements by a peer, to be compared against the mask and backing
	/// threshold and returns `None` if the peer is no longer connected.
	///
	/// A candidate may receive up to two in-flight requests (the second is dispatched after
	/// `PARALLEL_FETCH_THRESHOLD` has elapsed without a response on the first). The metrics
	/// `parallel_fetch_fired_total` and `parallel_fetch_skipped_no_alt_peer_total` are emitted
	/// directly through `metrics`.
	pub fn next_request(
		&mut self,
		response_manager: &mut ResponseManager,
		metrics: &Metrics,
		request_props: impl Fn(&CandidateIdentifier) -> Option<RequestProperties>,
		peer_advertised: impl Fn(&CandidateIdentifier, &PeerId) -> Option<StatementFilter>,
	) -> Option<OutgoingRequest<AttestedCandidateRequest>> {
		// The number of parallel requests a node can answer is limited by
		// `MAX_PARALLEL_ATTESTED_CANDIDATE_REQUESTS`, however there is no
		// need for the current node to limit itself to the same amount the
		// requests, because the requests are going to different nodes anyways.
		// While looking at https://github.com/paritytech/polkadot-sdk/issues/3314,
		// found out that this requests take around 100ms to fulfill, so it
		// would make sense to try to request things as early as we can, given
		// we would need to request it for each candidate, around 25 right now
		// on kusama.
		if response_manager.len() >= 2 * MAX_PARALLEL_ATTESTED_CANDIDATE_REQUESTS as usize {
			return None;
		}

		let mut res = None;

		// loop over all requests, in order of priority.
		// do some active maintenance of the connected peers.
		// dispatch the first ready request (first slot, or parallel second slot).

		let mut cleanup_outdated = Vec::new();
		for (i, (_priority, id)) in self.by_priority.iter().enumerate() {
			let entry = match self.requests.get_mut(&id) {
				None => {
					gum::error!(
						target: LOG_TARGET,
						identifier = ?id,
						"Missing entry for priority queue member",
					);

					continue;
				},
				Some(e) => e,
			};

			let slot = if entry.is_pending_first() {
				FetchSlot::First
			} else if entry.is_pending_parallel() {
				FetchSlot::Parallel
			} else {
				continue;
			};

			let props = match request_props(&id) {
				None => {
					cleanup_outdated.push((i, id.clone()));
					continue;
				},
				Some(s) => s,
			};

			let target = match find_request_target_with_update(
				&mut entry.known_by,
				id,
				&props,
				&peer_advertised,
				&response_manager,
			) {
				None => {
					// For a parallel slot, "no target" specifically means no alternate
					// advertiser was available (every known peer is already in flight or
					// otherwise filtered out). Surface that for operator diagnostics.
					if matches!(slot, FetchSlot::Parallel) {
						metrics.on_parallel_fetch_skipped_no_alt_peer();
					}
					continue;
				},
				Some(t) => t,
			};

			gum::debug!(
				target: crate::LOG_TARGET,
				candidate_hash = ?id.candidate_hash,
				peer = ?target,
				?slot,
				"Issuing candidate request"
			);

			let (request, response_fut) = OutgoingRequest::new(
				RequestRecipient::Peer(target),
				AttestedCandidateRequest {
					candidate_hash: id.candidate_hash,
					mask: props.unwanted_mask.clone(),
				},
			);

			let sent_at = Instant::now();
			let stored_id = id.clone();
			response_manager.push(
				Box::pin(async move {
					TaggedResponse {
						identifier: stored_id,
						requested_peer: target,
						slot,
						sent_at,
						props,
						response: response_fut.await,
					}
				}),
				target,
			);

			entry.in_flight.push(target);
			if matches!(slot, FetchSlot::First) {
				entry.first_request_sent_at = Some(sent_at);
			} else {
				metrics.on_parallel_fetch_fired();
			}

			res = Some(request);
			break;
		}

		for (priority_index, identifier) in cleanup_outdated.into_iter().rev() {
			self.by_priority.remove(priority_index);
			self.requests.remove(&identifier);
			if let HEntry::Occupied(mut e) =
				self.unique_identifiers.entry(identifier.candidate_hash)
			{
				e.get_mut().remove(&identifier);
				if e.get().is_empty() {
					e.remove();
				}
			}
		}

		res
	}
}

/// A manager for pending responses.
pub struct ResponseManager {
	pending_responses: FuturesUnordered<BoxFuture<'static, TaggedResponse>>,
	active_peers: HashSet<PeerId>,
}

impl ResponseManager {
	pub fn new() -> Self {
		Self { pending_responses: FuturesUnordered::new(), active_peers: HashSet::new() }
	}

	/// Await the next incoming response to a sent request, or immediately
	/// return `None` if there are no pending responses.
	pub async fn incoming(&mut self) -> Option<UnhandledResponse> {
		self.pending_responses.next().await.map(|response| {
			self.active_peers.remove(&response.requested_peer);
			UnhandledResponse { response }
		})
	}

	fn len(&self) -> usize {
		self.pending_responses.len()
	}

	fn push(&mut self, response: BoxFuture<'static, TaggedResponse>, target: PeerId) {
		self.pending_responses.push(response);
		self.active_peers.insert(target);
	}

	/// Returns true if we are currently sending a request to the peer.
	fn is_sending_to(&self, peer: &PeerId) -> bool {
		self.active_peers.contains(peer)
	}
}

/// Properties used in target selection and validation of a request.
#[derive(Clone)]
pub struct RequestProperties {
	/// A mask for limiting the statements the response is allowed to contain.
	/// The mask has `OR` semantics: statements by validators corresponding to bits
	/// in the mask are not desired. It also returns the required backing threshold
	/// for the candidate.
	pub unwanted_mask: StatementFilter,
	/// The required backing threshold, if any. If this is `Some`, then requests will only
	/// be made to peers which can provide enough statements to back the candidate, when
	/// taking into account the `unwanted_mask`, and a response will only be validated
	/// in the case of those statements.
	///
	/// If this is `None`, it is assumed that only the candidate itself is needed.
	pub backing_threshold: Option<usize>,
}

/// Finds a valid request target, returning `None` if none exists.
/// Cleans up disconnected peers and places the returned peer at the back of the queue.
fn find_request_target_with_update(
	known_by: &mut VecDeque<PeerId>,
	candidate_identifier: &CandidateIdentifier,
	props: &RequestProperties,
	peer_advertised: impl Fn(&CandidateIdentifier, &PeerId) -> Option<StatementFilter>,
	response_manager: &ResponseManager,
) -> Option<PeerId> {
	let mut prune = Vec::new();
	let mut target = None;
	for (i, p) in known_by.iter().enumerate() {
		// If we are already sending to that peer, skip for now
		if response_manager.is_sending_to(p) {
			continue;
		}

		let mut filter = match peer_advertised(candidate_identifier, p) {
			None => {
				prune.push(i);
				continue;
			},
			Some(f) => f,
		};

		filter.mask_seconded(&props.unwanted_mask.seconded_in_group);
		filter.mask_valid(&props.unwanted_mask.validated_in_group);
		if seconded_and_sufficient(&filter, props.backing_threshold) {
			target = Some((i, *p));
			break;
		}
	}

	let prune_count = prune.len();
	for i in prune {
		known_by.remove(i);
	}

	if let Some((i, p)) = target {
		known_by.remove(i - prune_count);
		known_by.push_back(p);
		Some(p)
	} else {
		None
	}
}

/// A response to a request, which has not yet been handled.
pub struct UnhandledResponse {
	response: TaggedResponse,
}

impl UnhandledResponse {
	/// Get the candidate identifier which the corresponding request
	/// was classified under.
	pub fn candidate_identifier(&self) -> &CandidateIdentifier {
		&self.response.identifier
	}

	/// Get the peer we made the request to.
	pub fn requested_peer(&self) -> &PeerId {
		&self.response.requested_peer
	}

	/// Validate the response. If the response is valid, this will yield the
	/// candidate, the [`PersistedValidationData`] of the candidate, and requested
	/// checked statements.
	///
	/// Valid responses are defined as those which provide a valid candidate
	/// and signatures which match the identifier, and provide enough statements to back the
	/// candidate.
	///
	/// This will also produce a record of misbehaviors by peers:
	///   * If the response is partially valid, misbehavior by the responding peer.
	///   * If there are other peers which have advertised the same candidate for different
	///     relay-parents or para-ids, misbehavior reports for those peers will also be generated.
	///
	/// Finally, in the case that the response is either valid or partially valid,
	/// this will clean up all remaining requests for the candidate in the manager.
	///
	/// As parameters, the user should supply the canonical group array as well
	/// as a mapping from validator index to validator ID. The validator pubkey mapping
	/// will not be queried except for validator indices in the group.
	pub fn validate_response(
		self,
		manager: &mut RequestManager,
		group: &[ValidatorIndex],
		session: SessionIndex,
		validator_key_lookup: impl Fn(ValidatorIndex) -> Option<ValidatorId>,
		allowed_para_lookup: impl Fn(ParaId, GroupIndex) -> bool,
		disabled_mask: BitVec<u8, Lsb0>,
		transposed_cq: &TransposedClaimQueue,
	) -> ResponseValidationOutput {
		let UnhandledResponse {
			response: TaggedResponse { identifier, requested_peer, slot, sent_at, props, response },
		} = self;

		let duration = Instant::now().saturating_duration_since(sent_at);

		// handle races if the candidate is no longer known.
		// this could happen if we requested the candidate under two
		// different identifiers at the same time, and received a valid
		// response on the other, or because a parallel (second-slot) fetch
		// resolved later than the winner from the first slot.
		//
		// it could also happen in the case that we had a request in-flight
		// and the request entry was garbage-collected on outdated relay parent.
		let entry = match manager.requests.get_mut(&identifier) {
			None => {
				return ResponseValidationOutput {
					requested_peer,
					reputation_changes: Vec::new(),
					request_status: CandidateRequestStatus::Outdated,
					fetch_completion: FetchCompletion {
						slot,
						outcome: FetchOutcome::Dropped,
						duration,
					},
					// Entry already gone — typically the winner of a parallel race already
					// observed the learn-to-fetch duration on its own resolution.
					learn_to_fetch: None,
				};
			},
			Some(e) => e,
		};

		// Capture this before any potential `manager.remove_for` consumes the entry.
		let first_learned_at = entry.first_learned_at;

		let priority_index = match manager
			.by_priority
			.binary_search(&(entry.priority.clone(), identifier.clone()))
		{
			Ok(i) => i,
			Err(_) => unreachable!("requested candidates always have a priority entry; qed"),
		};

		// Clear the resolving peer's slot from the in-flight set. We retain whichever other
		// slot (if any) is still pending — its future remains in `ResponseManager` and will
		// resolve to `Outdated` if this response Completed (via `remove_for`), or continue
		// independently otherwise.
		entry.in_flight.retain(|p| *p != requested_peer);

		// Only schedule a retry if no other slot is still in flight: if the loser of a parallel
		// race resolves Incomplete, we don't want to bump the retry cooldown while the winner
		// is still in flight.
		if entry.in_flight.is_empty() {
			entry.next_retry_time = Some(Instant::now() + REQUEST_RETRY_DELAY);
			entry.first_request_sent_at = None;
			entry.priority.attempts += 1;
		}

		// update the location in the priority queue.
		insert_or_update_priority(
			&mut manager.by_priority,
			Some(priority_index),
			identifier.clone(),
			entry.priority.clone(),
		);

		let complete_response = match response {
			Err(RequestError::InvalidResponse(e)) => {
				gum::trace!(
					target: LOG_TARGET,
					err = ?e,
					peer = ?requested_peer,
					"Improperly encoded response"
				);

				return ResponseValidationOutput {
					requested_peer,
					reputation_changes: vec![(requested_peer, COST_IMPROPERLY_DECODED_RESPONSE)],
					request_status: CandidateRequestStatus::Incomplete,
					fetch_completion: FetchCompletion {
						slot,
						outcome: FetchOutcome::Invalid,
						duration,
					},
					// Not Complete yet — retry may follow. Only emit learn-to-fetch on the
					// terminal Complete observation.
					learn_to_fetch: None,
				};
			},
			Err(e @ RequestError::NetworkError(_) | e @ RequestError::Canceled(_)) => {
				gum::trace!(
					target: LOG_TARGET,
					err = ?e,
					peer = ?requested_peer,
					"Request error"
				);
				return ResponseValidationOutput {
					requested_peer,
					reputation_changes: vec![],
					request_status: CandidateRequestStatus::Incomplete,
					fetch_completion: FetchCompletion {
						slot,
						outcome: FetchOutcome::TimeoutOther,
						duration,
					},
					learn_to_fetch: None,
				};
			},
			Ok(response) => response,
		};

		let mut output = validate_complete_response(
			&identifier,
			props,
			complete_response,
			requested_peer,
			group,
			session,
			validator_key_lookup,
			allowed_para_lookup,
			disabled_mask,
			transposed_cq,
		);

		// Backfill the slot/duration info, since validate_complete_response doesn't know it.
		output.fetch_completion = FetchCompletion {
			slot,
			outcome: match &output.request_status {
				CandidateRequestStatus::Complete { .. } => FetchOutcome::Success,
				_ => FetchOutcome::Invalid,
			},
			duration,
		};

		if let CandidateRequestStatus::Complete { .. } = output.request_status {
			// End-to-end "we knew about this candidate" → "we have it" duration. Includes
			// queue wait, retry-cooldown, and the winning fetch RTT. The caller emits this
			// into `learn_to_fetch_seconds`.
			output.learn_to_fetch =
				Some(Instant::now().saturating_duration_since(first_learned_at));
			manager.remove_for(identifier.candidate_hash);
		}

		output
	}
}

fn validate_complete_response(
	identifier: &CandidateIdentifier,
	props: RequestProperties,
	response: AttestedCandidateResponse,
	requested_peer: PeerId,
	group: &[ValidatorIndex],
	session: SessionIndex,
	validator_key_lookup: impl Fn(ValidatorIndex) -> Option<ValidatorId>,
	allowed_para_lookup: impl Fn(ParaId, GroupIndex) -> bool,
	disabled_mask: BitVec<u8, Lsb0>,
	transposed_cq: &TransposedClaimQueue,
) -> ResponseValidationOutput {
	let RequestProperties { backing_threshold, mut unwanted_mask } = props;

	// sanity check bitmask size. this is based entirely on
	// local logic here.
	if !unwanted_mask.has_len(group.len()) {
		gum::error!(
			target: LOG_TARGET,
			group_len = group.len(),
			"Logic bug: group size != sent bitmask len"
		);

		// resize and attempt to continue.
		unwanted_mask.seconded_in_group.resize(group.len(), true);
		unwanted_mask.validated_in_group.resize(group.len(), true);
	}

	// `fetch_completion` is backfilled by the caller (`validate_response`) which has the
	// slot/duration context. Use a placeholder here that will be overwritten.
	// `learn_to_fetch` stays None for Incomplete outcomes.
	let invalid_candidate_output = |cost: Rep| ResponseValidationOutput {
		request_status: CandidateRequestStatus::Incomplete,
		reputation_changes: vec![(requested_peer, cost)],
		requested_peer,
		fetch_completion: FetchCompletion {
			slot: FetchSlot::First,
			outcome: FetchOutcome::Invalid,
			duration: Duration::ZERO,
		},
		learn_to_fetch: None,
	};

	let mut rep_changes = Vec::new();

	// sanity-check candidate response.
	// note: roughly ascending cost of operations
	{
		if response.candidate_receipt.descriptor.scheduling_parent() != identifier.scheduling_parent
		{
			return invalid_candidate_output(COST_INVALID_RESPONSE);
		}

		if response.candidate_receipt.descriptor.persisted_validation_data_hash() !=
			response.persisted_validation_data.hash()
		{
			return invalid_candidate_output(COST_INVALID_RESPONSE);
		}

		if !allowed_para_lookup(
			response.candidate_receipt.descriptor.para_id(),
			identifier.group_index,
		) {
			return invalid_candidate_output(COST_INVALID_RESPONSE);
		}

		if response.candidate_receipt.hash() != identifier.candidate_hash {
			return invalid_candidate_output(COST_INVALID_RESPONSE);
		}

		let candidate_hash = response.candidate_receipt.hash();

		// Validate the ump signals.
		if let Err(err) = response.candidate_receipt.parse_ump_signals(transposed_cq) {
			gum::debug!(
				target: LOG_TARGET,
				?candidate_hash,
				?err,
				peer = ?requested_peer,
				"Received candidate has invalid UMP signals"
			);
			return invalid_candidate_output(COST_INVALID_UMP_SIGNALS);
		}

		// Check if `session_index` of scheduling parent matches candidate descriptor
		// `scheduling_session`.
		if let Some(scheduling_session) = response.candidate_receipt.descriptor.scheduling_session()
		{
			if scheduling_session != session {
				gum::debug!(
					target: LOG_TARGET,
					?candidate_hash,
					peer = ?requested_peer,
					session_index = session,
					scheduling_session,
					"Received candidate has invalid scheduling session index"
				);
				return invalid_candidate_output(COST_INVALID_SESSION_INDEX);
			}
		}
	}

	// statement checks.
	let statements = {
		let mut statements =
			Vec::with_capacity(std::cmp::min(response.statements.len(), group.len() * 2));

		let mut received_filter = StatementFilter::blank(group.len());

		let index_in_group = |v: ValidatorIndex| group.iter().position(|x| &v == x);

		let signing_context =
			SigningContext { parent_hash: identifier.scheduling_parent, session_index: session };

		for unchecked_statement in response.statements.into_iter().take(group.len() * 2) {
			// ensure statement is from a validator in the group.
			let i = match index_in_group(unchecked_statement.unchecked_validator_index()) {
				Some(i) => i,
				None => {
					rep_changes.push((requested_peer, COST_UNREQUESTED_RESPONSE_STATEMENT));
					continue;
				},
			};

			// ensure statement is on the correct candidate hash.
			if unchecked_statement.unchecked_payload().candidate_hash() !=
				&identifier.candidate_hash
			{
				rep_changes.push((requested_peer, COST_UNREQUESTED_RESPONSE_STATEMENT));
				continue;
			}

			// filter out duplicates or statements outside the mask.
			// note on indexing: we have ensured that the bitmask and the
			// duplicate trackers have the correct size for the group.
			match unchecked_statement.unchecked_payload() {
				CompactStatement::Seconded(_) => {
					if unwanted_mask.seconded_in_group[i] {
						rep_changes.push((requested_peer, COST_UNREQUESTED_RESPONSE_STATEMENT));
						continue;
					}

					if received_filter.seconded_in_group[i] {
						rep_changes.push((requested_peer, COST_UNREQUESTED_RESPONSE_STATEMENT));
						continue;
					}
				},
				CompactStatement::Valid(_) => {
					if unwanted_mask.validated_in_group[i] {
						rep_changes.push((requested_peer, COST_UNREQUESTED_RESPONSE_STATEMENT));
						continue;
					}

					if received_filter.validated_in_group[i] {
						rep_changes.push((requested_peer, COST_UNREQUESTED_RESPONSE_STATEMENT));
						continue;
					}
				},
			}

			if disabled_mask.get(i).map_or(false, |x| *x) {
				continue;
			}

			let validator_public =
				match validator_key_lookup(unchecked_statement.unchecked_validator_index()) {
					None => {
						rep_changes.push((requested_peer, COST_INVALID_SIGNATURE));
						continue;
					},
					Some(p) => p,
				};

			let checked_statement =
				match unchecked_statement.try_into_checked(&signing_context, &validator_public) {
					Err(_) => {
						rep_changes.push((requested_peer, COST_INVALID_SIGNATURE));
						continue;
					},
					Ok(checked) => checked,
				};

			match checked_statement.payload() {
				CompactStatement::Seconded(_) => {
					received_filter.seconded_in_group.set(i, true);
				},
				CompactStatement::Valid(_) => {
					received_filter.validated_in_group.set(i, true);
				},
			}

			statements.push(checked_statement);
			rep_changes.push((requested_peer, BENEFIT_VALID_STATEMENT));
		}

		// Only accept responses which are sufficient, according to our
		// required backing threshold.
		if !seconded_and_sufficient(&received_filter, backing_threshold) {
			return invalid_candidate_output(COST_INVALID_RESPONSE);
		}

		statements
	};

	rep_changes.push((requested_peer, BENEFIT_VALID_RESPONSE));

	ResponseValidationOutput {
		requested_peer,
		request_status: CandidateRequestStatus::Complete {
			candidate: response.candidate_receipt,
			persisted_validation_data: response.persisted_validation_data,
			statements,
		},
		reputation_changes: rep_changes,
		// Backfilled by the caller (`validate_response`).
		fetch_completion: FetchCompletion {
			slot: FetchSlot::First,
			outcome: FetchOutcome::Success,
			duration: Duration::ZERO,
		},
		learn_to_fetch: None,
	}
}

/// The status of the candidate request after the handling of a response.
#[derive(Debug, PartialEq)]
pub enum CandidateRequestStatus {
	/// The request was outdated at the point of receiving the response.
	Outdated,
	/// The response either did not arrive or was invalid.
	Incomplete,
	/// The response completed the request. Statements sent beyond the
	/// mask have been ignored.
	Complete {
		candidate: CommittedCandidateReceipt,
		persisted_validation_data: PersistedValidationData,
		statements: Vec<SignedStatement>,
	},
}

/// Output of the response validation.
#[derive(Debug, PartialEq)]
pub struct ResponseValidationOutput {
	/// The peer we requested from.
	pub requested_peer: PeerId,
	/// The status of the request.
	pub request_status: CandidateRequestStatus,
	/// Any reputation changes as a result of validating the response.
	pub reputation_changes: Vec<(PeerId, Rep)>,
	/// Observation for the `fetch_completion_seconds` histogram and the
	/// `parallel_fetch_won_total` counter. Always populated.
	pub fetch_completion: FetchCompletion,
	/// Observation for the `learn_to_fetch_seconds` histogram. Set to `Some` only
	/// on `Complete` — the duration from when we first learned about the candidate (first
	/// insertion into the `RequestManager`) to now. Captures queue-wait + retry-cooldown +
	/// fetch RTT, end-to-end.
	pub learn_to_fetch: Option<Duration>,
}

fn insert_or_update_priority(
	priority_sorted: &mut Vec<(Priority, CandidateIdentifier)>,
	prev_index: Option<usize>,
	candidate_identifier: CandidateIdentifier,
	new_priority: Priority,
) -> usize {
	if let Some(prev_index) = prev_index {
		// GIGO: this behaves strangely if prev-index is not for the
		// expected identifier.
		if priority_sorted[prev_index].0 == new_priority {
			// unchanged.
			return prev_index;
		} else {
			priority_sorted.remove(prev_index);
		}
	}

	let item = (new_priority, candidate_identifier);
	match priority_sorted.binary_search(&item) {
		Ok(i) => i, // ignore if already present.
		Err(i) => {
			priority_sorted.insert(i, item);
			i
		},
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use polkadot_primitives::HeadData;
	use polkadot_primitives_test_helpers as test_helpers;

	fn dummy_pvd() -> PersistedValidationData {
		PersistedValidationData {
			parent_head: HeadData(vec![7, 8, 9]),
			relay_parent_number: 5,
			max_pov_size: 1024,
			relay_parent_storage_root: Default::default(),
		}
	}

	#[test]
	fn test_remove_by_scheduling_parent() {
		let parent_a = Hash::from_low_u64_le(1);
		let parent_b = Hash::from_low_u64_le(2);
		let parent_c = Hash::from_low_u64_le(3);

		let candidate_a1 = CandidateHash(Hash::from_low_u64_le(11));
		let candidate_a2 = CandidateHash(Hash::from_low_u64_le(12));
		let candidate_b1 = CandidateHash(Hash::from_low_u64_le(21));
		let candidate_b2 = CandidateHash(Hash::from_low_u64_le(22));
		let candidate_c1 = CandidateHash(Hash::from_low_u64_le(31));
		let duplicate_hash = CandidateHash(Hash::from_low_u64_le(31));

		let mut request_manager = RequestManager::new();
		request_manager.get_or_insert(parent_a, candidate_a1, 1.into());
		request_manager.get_or_insert(parent_a, candidate_a2, 1.into());
		request_manager.get_or_insert(parent_b, candidate_b1, 1.into());
		request_manager.get_or_insert(parent_b, candidate_b2, 2.into());
		request_manager.get_or_insert(parent_c, candidate_c1, 2.into());
		request_manager.get_or_insert(parent_a, duplicate_hash, 1.into());

		assert_eq!(request_manager.requests.len(), 6);
		assert_eq!(request_manager.by_priority.len(), 6);
		assert_eq!(request_manager.unique_identifiers.len(), 5);

		request_manager.remove_by_scheduling_parent(parent_a);

		assert_eq!(request_manager.requests.len(), 3);
		assert_eq!(request_manager.by_priority.len(), 3);
		assert_eq!(request_manager.unique_identifiers.len(), 3);

		assert!(!request_manager.unique_identifiers.contains_key(&candidate_a1));
		assert!(!request_manager.unique_identifiers.contains_key(&candidate_a2));
		// Duplicate hash should still be there (under a different parent).
		assert!(request_manager.unique_identifiers.contains_key(&duplicate_hash));

		request_manager.remove_by_scheduling_parent(parent_b);

		assert_eq!(request_manager.requests.len(), 1);
		assert_eq!(request_manager.by_priority.len(), 1);
		assert_eq!(request_manager.unique_identifiers.len(), 1);

		assert!(!request_manager.unique_identifiers.contains_key(&candidate_b1));
		assert!(!request_manager.unique_identifiers.contains_key(&candidate_b2));

		request_manager.remove_by_scheduling_parent(parent_c);

		assert!(request_manager.requests.is_empty());
		assert!(request_manager.by_priority.is_empty());
		assert!(request_manager.unique_identifiers.is_empty());
	}

	#[test]
	fn test_priority_ordering() {
		let parent_a = Hash::from_low_u64_le(1);
		let parent_b = Hash::from_low_u64_le(2);
		let parent_c = Hash::from_low_u64_le(3);

		let candidate_a1 = CandidateHash(Hash::from_low_u64_le(11));
		let candidate_a2 = CandidateHash(Hash::from_low_u64_le(12));
		let candidate_b1 = CandidateHash(Hash::from_low_u64_le(21));
		let candidate_b2 = CandidateHash(Hash::from_low_u64_le(22));
		let candidate_c1 = CandidateHash(Hash::from_low_u64_le(31));

		let mut request_manager = RequestManager::new();

		// Add some entries, set a couple of them to cluster (high) priority.
		let identifier_a1 = request_manager
			.get_or_insert(parent_a, candidate_a1, 1.into())
			.identifier
			.clone();
		let identifier_a2 = {
			let mut entry = request_manager.get_or_insert(parent_a, candidate_a2, 1.into());
			entry.set_cluster_priority();
			entry.identifier.clone()
		};
		let identifier_b1 = request_manager
			.get_or_insert(parent_b, candidate_b1, 1.into())
			.identifier
			.clone();
		let identifier_b2 = request_manager
			.get_or_insert(parent_b, candidate_b2, 2.into())
			.identifier
			.clone();
		let identifier_c1 = {
			let mut entry = request_manager.get_or_insert(parent_c, candidate_c1, 2.into());
			entry.set_cluster_priority();
			entry.identifier.clone()
		};

		let attempts = 0;
		assert_eq!(
			request_manager.by_priority,
			vec![
				(Priority { origin: Origin::Cluster, attempts }, identifier_a2),
				(Priority { origin: Origin::Cluster, attempts }, identifier_c1),
				(Priority { origin: Origin::Unspecified, attempts }, identifier_a1),
				(Priority { origin: Origin::Unspecified, attempts }, identifier_b1),
				(Priority { origin: Origin::Unspecified, attempts }, identifier_b2),
			]
		);
	}

	// Test case where candidate is requested under two different identifiers at the same time.
	// Should result in `Outdated` error.
	#[test]
	fn handle_outdated_response_due_to_requests_for_different_identifiers() {
		let mut request_manager = RequestManager::new();
		let mut response_manager = ResponseManager::new();

		let relay_parent = Hash::from_low_u64_le(1);
		let mut candidate_receipt = test_helpers::dummy_committed_candidate_receipt(relay_parent);
		let persisted_validation_data = dummy_pvd();
		candidate_receipt.descriptor.persisted_validation_data_hash =
			persisted_validation_data.hash();
		let candidate = candidate_receipt.hash();
		let candidate_receipt: CommittedCandidateReceipt = candidate_receipt.into();
		let requested_peer_1 = PeerId::random();
		let requested_peer_2 = PeerId::random();

		let identifier1 = request_manager
			.get_or_insert(relay_parent, candidate, 1.into())
			.identifier
			.clone();
		request_manager
			.get_or_insert(relay_parent, candidate, 1.into())
			.add_peer(requested_peer_1);
		let identifier2 = request_manager
			.get_or_insert(relay_parent, candidate, 2.into())
			.identifier
			.clone();
		request_manager
			.get_or_insert(relay_parent, candidate, 2.into())
			.add_peer(requested_peer_2);

		assert_ne!(identifier1, identifier2);
		assert_eq!(request_manager.requests.len(), 2);

		let group_size = 3;
		let group = &[ValidatorIndex(0), ValidatorIndex(1), ValidatorIndex(2)];

		let unwanted_mask = StatementFilter::blank(group_size);
		let disabled_mask: BitVec<u8, Lsb0> = Default::default();
		let request_properties = RequestProperties { unwanted_mask, backing_threshold: None };

		// Get requests.
		{
			let request_props =
				|_identifier: &CandidateIdentifier| Some((&request_properties).clone());
			let peer_advertised = |_identifier: &CandidateIdentifier, _peer: &_| {
				Some(StatementFilter::full(group_size))
			};

			let outgoing = request_manager
				.next_request(
					&mut response_manager,
					&Metrics::default(),
					request_props,
					peer_advertised,
				)
				.unwrap();
			assert_eq!(outgoing.payload.candidate_hash, candidate);
			let outgoing = request_manager
				.next_request(
					&mut response_manager,
					&Metrics::default(),
					request_props,
					peer_advertised,
				)
				.unwrap();
			assert_eq!(outgoing.payload.candidate_hash, candidate);
		}

		// Validate first response.
		{
			let statements = vec![];
			let response = UnhandledResponse {
				response: TaggedResponse {
					identifier: identifier1,
					requested_peer: requested_peer_1,
					slot: FetchSlot::First,
					sent_at: Instant::now(),
					props: request_properties.clone(),
					response: Ok(AttestedCandidateResponse {
						candidate_receipt: candidate_receipt.clone().into(),
						persisted_validation_data: persisted_validation_data.clone(),
						statements,
					}),
				},
			};
			let validator_key_lookup = |_v| None;
			let allowed_para_lookup = |_para, _g_index| true;
			let statements = vec![];
			let output = response.validate_response(
				&mut request_manager,
				group,
				0,
				validator_key_lookup,
				allowed_para_lookup,
				disabled_mask.clone(),
				&Default::default(),
			);
			assert_eq!(output.requested_peer, requested_peer_1);
			assert_eq!(
				output.request_status,
				CandidateRequestStatus::Complete {
					candidate: candidate_receipt.clone(),
					persisted_validation_data: persisted_validation_data.clone(),
					statements,
				}
			);
			assert_eq!(output.reputation_changes, vec![(requested_peer_1, BENEFIT_VALID_RESPONSE)]);
			assert_eq!(output.fetch_completion.slot, FetchSlot::First);
			assert_eq!(output.fetch_completion.outcome, FetchOutcome::Success);
		}

		// Try to validate second response.
		{
			let statements = vec![];
			let response = UnhandledResponse {
				response: TaggedResponse {
					identifier: identifier2,
					requested_peer: requested_peer_2,
					slot: FetchSlot::First,
					sent_at: Instant::now(),
					props: request_properties,
					response: Ok(AttestedCandidateResponse {
						candidate_receipt: candidate_receipt.clone().into(),
						persisted_validation_data: persisted_validation_data.clone(),
						statements,
					}),
				},
			};
			let validator_key_lookup = |_v| None;
			let allowed_para_lookup = |_para, _g_index| true;
			let output = response.validate_response(
				&mut request_manager,
				group,
				0,
				validator_key_lookup,
				allowed_para_lookup,
				disabled_mask,
				&Default::default(),
			);
			assert_eq!(output.requested_peer, requested_peer_2);
			assert_eq!(output.request_status, CandidateRequestStatus::Outdated);
			assert_eq!(output.reputation_changes, vec![]);
			assert_eq!(output.fetch_completion.outcome, FetchOutcome::Dropped);
		}

		assert_eq!(request_manager.requests.len(), 0);
	}

	// Test case where we had a request in-flight and the request entry was garbage-collected on
	// outdated relay parent.
	#[test]
	fn handle_outdated_response_due_to_garbage_collection() {
		let mut request_manager = RequestManager::new();
		let mut response_manager = ResponseManager::new();

		let relay_parent = Hash::from_low_u64_le(1);
		let mut candidate_receipt = test_helpers::dummy_committed_candidate_receipt(relay_parent);
		let persisted_validation_data = dummy_pvd();
		candidate_receipt.descriptor.persisted_validation_data_hash =
			persisted_validation_data.hash();
		let candidate = candidate_receipt.hash();
		let requested_peer = PeerId::random();

		let identifier = request_manager
			.get_or_insert(relay_parent, candidate, 1.into())
			.identifier
			.clone();
		request_manager
			.get_or_insert(relay_parent, candidate, 1.into())
			.add_peer(requested_peer);

		let group_size = 3;
		let group = &[ValidatorIndex(0), ValidatorIndex(1), ValidatorIndex(2)];

		let unwanted_mask = StatementFilter::blank(group_size);
		let request_properties = RequestProperties { unwanted_mask, backing_threshold: None };
		let peer_advertised =
			|_identifier: &CandidateIdentifier, _peer: &_| Some(StatementFilter::full(group_size));

		// Get request once successfully.
		{
			let request_props =
				|_identifier: &CandidateIdentifier| Some((&request_properties).clone());

			let outgoing = request_manager
				.next_request(
					&mut response_manager,
					&Metrics::default(),
					request_props,
					peer_advertised,
				)
				.unwrap();
			assert_eq!(outgoing.payload.candidate_hash, candidate);
		}

		// Garbage collect based on relay parent.
		request_manager.remove_by_scheduling_parent(relay_parent);

		// Try to validate response.
		{
			let statements = vec![];
			let response = UnhandledResponse {
				response: TaggedResponse {
					identifier,
					requested_peer,
					slot: FetchSlot::First,
					sent_at: Instant::now(),
					props: request_properties,
					response: Ok(AttestedCandidateResponse {
						candidate_receipt: candidate_receipt.clone().into(),
						persisted_validation_data: persisted_validation_data.clone(),
						statements,
					}),
				},
			};
			let validator_key_lookup = |_v| None;
			let allowed_para_lookup = |_para, _g_index| true;
			let disabled_mask: BitVec<u8, Lsb0> = Default::default();
			let output = response.validate_response(
				&mut request_manager,
				group,
				0,
				validator_key_lookup,
				allowed_para_lookup,
				disabled_mask,
				&Default::default(),
			);
			assert_eq!(output.requested_peer, requested_peer);
			assert_eq!(output.request_status, CandidateRequestStatus::Outdated);
			assert_eq!(output.reputation_changes, vec![]);
			assert_eq!(output.fetch_completion.outcome, FetchOutcome::Dropped);
		}
	}

	#[test]
	fn should_clean_up_after_successful_requests() {
		let mut request_manager = RequestManager::new();
		let mut response_manager = ResponseManager::new();

		let relay_parent = Hash::from_low_u64_le(1);
		let mut candidate_receipt = test_helpers::dummy_committed_candidate_receipt(relay_parent);
		let persisted_validation_data = dummy_pvd();
		candidate_receipt.descriptor.persisted_validation_data_hash =
			persisted_validation_data.hash();
		let candidate = candidate_receipt.hash();
		let requested_peer = PeerId::random();

		let identifier = request_manager
			.get_or_insert(relay_parent, candidate, 1.into())
			.identifier
			.clone();
		request_manager
			.get_or_insert(relay_parent, candidate, 1.into())
			.add_peer(requested_peer);

		assert_eq!(request_manager.requests.len(), 1);
		assert_eq!(request_manager.by_priority.len(), 1);

		let group_size = 3;
		let group = &[ValidatorIndex(0), ValidatorIndex(1), ValidatorIndex(2)];

		let unwanted_mask = StatementFilter::blank(group_size);
		let request_properties = RequestProperties { unwanted_mask, backing_threshold: None };
		let peer_advertised =
			|_identifier: &CandidateIdentifier, _peer: &_| Some(StatementFilter::full(group_size));

		// Get request once successfully.
		{
			let request_props =
				|_identifier: &CandidateIdentifier| Some((&request_properties).clone());

			let outgoing = request_manager
				.next_request(
					&mut response_manager,
					&Metrics::default(),
					request_props,
					peer_advertised,
				)
				.unwrap();
			assert_eq!(outgoing.payload.candidate_hash, candidate);
		}

		// Validate response.
		{
			let statements = vec![];
			let response = UnhandledResponse {
				response: TaggedResponse {
					identifier,
					requested_peer,
					slot: FetchSlot::First,
					sent_at: Instant::now(),
					props: request_properties.clone(),
					response: Ok(AttestedCandidateResponse {
						candidate_receipt: candidate_receipt.clone().into(),
						persisted_validation_data: persisted_validation_data.clone(),
						statements,
					}),
				},
			};
			let validator_key_lookup = |_v| None;
			let allowed_para_lookup = |_para, _g_index| true;
			let statements = vec![];
			let disabled_mask: BitVec<u8, Lsb0> = Default::default();
			let output = response.validate_response(
				&mut request_manager,
				group,
				0,
				validator_key_lookup,
				allowed_para_lookup,
				disabled_mask,
				&Default::default(),
			);
			assert_eq!(output.requested_peer, requested_peer);
			assert_eq!(
				output.request_status,
				CandidateRequestStatus::Complete {
					candidate: candidate_receipt.clone().into(),
					persisted_validation_data: persisted_validation_data.clone(),
					statements,
				}
			);
			assert_eq!(output.reputation_changes, vec![(requested_peer, BENEFIT_VALID_RESPONSE)]);
			assert_eq!(output.fetch_completion.slot, FetchSlot::First);
			assert_eq!(output.fetch_completion.outcome, FetchOutcome::Success);
		}

		// Ensure that cleanup occurred.
		assert_eq!(request_manager.requests.len(), 0);
		assert_eq!(request_manager.by_priority.len(), 0);
	}

	// Test case where we queue 2 requests to be sent to the same peer and 1 request to another
	// peer. Same peer requests should be served one at a time but they should not block the other
	// peer request.
	#[test]
	fn rate_limit_requests_to_same_peer() {
		let mut request_manager = RequestManager::new();
		let mut response_manager = ResponseManager::new();

		let relay_parent = Hash::from_low_u64_le(1);

		// Create 3 candidates
		let mut candidate_receipt_1 = test_helpers::dummy_committed_candidate_receipt(relay_parent);
		let persisted_validation_data_1 = dummy_pvd();
		candidate_receipt_1.descriptor.persisted_validation_data_hash =
			persisted_validation_data_1.hash();
		let candidate_1 = candidate_receipt_1.hash();

		let mut candidate_receipt_2 = test_helpers::dummy_committed_candidate_receipt(relay_parent);
		let persisted_validation_data_2 = dummy_pvd();
		candidate_receipt_2.descriptor.persisted_validation_data_hash =
			persisted_validation_data_2.hash();
		let candidate_2 = candidate_receipt_2.hash();

		let mut candidate_receipt_3 = test_helpers::dummy_committed_candidate_receipt(relay_parent);
		let persisted_validation_data_3 = dummy_pvd();
		candidate_receipt_3.descriptor.persisted_validation_data_hash =
			persisted_validation_data_3.hash();
		let candidate_3 = candidate_receipt_3.hash();

		// Create 2 peers
		let requested_peer_1 = PeerId::random();
		let requested_peer_2 = PeerId::random();

		let group_size = 3;
		let group = &[ValidatorIndex(0), ValidatorIndex(1), ValidatorIndex(2)];
		let unwanted_mask = StatementFilter::blank(group_size);
		let disabled_mask: BitVec<u8, Lsb0> = Default::default();
		let request_properties = RequestProperties { unwanted_mask, backing_threshold: None };
		let request_props = |_identifier: &CandidateIdentifier| Some((&request_properties).clone());
		let peer_advertised =
			|_identifier: &CandidateIdentifier, _peer: &_| Some(StatementFilter::full(group_size));

		// Add request for candidate 1 from peer 1
		let identifier1 = request_manager
			.get_or_insert(relay_parent, candidate_1, 1.into())
			.identifier
			.clone();
		request_manager
			.get_or_insert(relay_parent, candidate_1, 1.into())
			.add_peer(requested_peer_1);

		// Add request for candidate 3 from peer 2 (this one can be served in parallel)
		let _identifier3 = request_manager
			.get_or_insert(relay_parent, candidate_3, 1.into())
			.identifier
			.clone();
		request_manager
			.get_or_insert(relay_parent, candidate_3, 1.into())
			.add_peer(requested_peer_2);

		// Successfully dispatch request for candidate 1 from peer 1 and candidate 3 from peer 2
		for _ in 0..2 {
			let outgoing = request_manager.next_request(
				&mut response_manager,
				&Metrics::default(),
				request_props,
				peer_advertised,
			);
			assert!(outgoing.is_some());
		}
		assert_eq!(response_manager.active_peers.len(), 2);
		assert!(response_manager.is_sending_to(&requested_peer_1));
		assert!(response_manager.is_sending_to(&requested_peer_2));
		assert_eq!(request_manager.requests.len(), 2);

		// Add request for candidate 2 from peer 1
		let _identifier2 = request_manager
			.get_or_insert(relay_parent, candidate_2, 1.into())
			.identifier
			.clone();
		request_manager
			.get_or_insert(relay_parent, candidate_2, 1.into())
			.add_peer(requested_peer_1);

		// Do not dispatch the request for the second candidate from peer 1 (already serving that
		// peer)
		let outgoing = request_manager.next_request(
			&mut response_manager,
			&Metrics::default(),
			request_props,
			peer_advertised,
		);
		assert!(outgoing.is_none());
		assert_eq!(response_manager.active_peers.len(), 2);
		assert!(response_manager.is_sending_to(&requested_peer_1));
		assert!(response_manager.is_sending_to(&requested_peer_2));
		assert_eq!(request_manager.requests.len(), 3);

		// Manually mark response received (response future resolved)
		response_manager.active_peers.remove(&requested_peer_1);
		response_manager.pending_responses = FuturesUnordered::new();

		// Validate first response (candidate 1 from peer 1)
		{
			let statements = vec![];
			let response = UnhandledResponse {
				response: TaggedResponse {
					identifier: identifier1,
					requested_peer: requested_peer_1,
					slot: FetchSlot::First,
					sent_at: Instant::now(),
					props: request_properties.clone(),
					response: Ok(AttestedCandidateResponse {
						candidate_receipt: candidate_receipt_1.clone().into(),
						persisted_validation_data: persisted_validation_data_1.clone(),
						statements,
					}),
				},
			};
			let validator_key_lookup = |_v| None;
			let allowed_para_lookup = |_para, _g_index| true;
			let _output = response.validate_response(
				&mut request_manager,
				group,
				0,
				validator_key_lookup,
				allowed_para_lookup,
				disabled_mask.clone(),
				&Default::default(),
			);

			// First request served successfully
			assert_eq!(request_manager.requests.len(), 2);
			assert_eq!(response_manager.active_peers.len(), 1);
			assert!(response_manager.is_sending_to(&requested_peer_2));
		}

		// Check if the request that was ignored previously will be served now
		let outgoing = request_manager.next_request(
			&mut response_manager,
			&Metrics::default(),
			request_props,
			peer_advertised,
		);
		assert!(outgoing.is_some());
		assert_eq!(response_manager.active_peers.len(), 2);
		assert!(response_manager.is_sending_to(&requested_peer_1));
		assert!(response_manager.is_sending_to(&requested_peer_2));
		assert_eq!(request_manager.requests.len(), 2);
	}

	// --- Parallel `AttestedCandidate` fetch (issue #12028) -------------------------------

	/// Helper: insert a candidate with two known advertisers, ready for parallel-fetch tests.
	fn setup_two_advertiser_candidate(
		request_manager: &mut RequestManager,
	) -> (CandidateIdentifier, PeerId, PeerId, RequestProperties, usize) {
		let relay_parent = Hash::from_low_u64_le(1);
		let candidate = CandidateHash(Hash::from_low_u64_le(0x10));
		let peer_a = PeerId::random();
		let peer_b = PeerId::random();

		let identifier = request_manager
			.get_or_insert(relay_parent, candidate, 1.into())
			.identifier
			.clone();
		request_manager
			.get_or_insert(relay_parent, candidate, 1.into())
			.add_peer(peer_a);
		request_manager
			.get_or_insert(relay_parent, candidate, 1.into())
			.add_peer(peer_b);

		let group_size = 3;
		let unwanted_mask = StatementFilter::blank(group_size);
		let props = RequestProperties { unwanted_mask, backing_threshold: None };
		(identifier, peer_a, peer_b, props, group_size)
	}

	#[test]
	fn parallel_fetch_not_fired_before_threshold() {
		let mut request_manager = RequestManager::new();
		let mut response_manager = ResponseManager::new();
		let metrics = Metrics::default();

		let (identifier, _peer_a, _peer_b, props, group_size) =
			setup_two_advertiser_candidate(&mut request_manager);
		let request_props = |_: &CandidateIdentifier| Some(props.clone());
		let peer_advertised =
			|_: &CandidateIdentifier, _: &PeerId| Some(StatementFilter::full(group_size));

		// First request dispatches normally.
		let first = request_manager.next_request(
			&mut response_manager,
			&metrics,
			request_props,
			peer_advertised,
		);
		assert!(first.is_some());
		assert_eq!(request_manager.in_flight_count_for(&identifier), 1);

		// Without forcing the threshold to elapse, no second slot is dispatched.
		let second = request_manager.next_request(
			&mut response_manager,
			&metrics,
			request_props,
			peer_advertised,
		);
		assert!(second.is_none());
		assert_eq!(request_manager.in_flight_count_for(&identifier), 1);
		// Timer not yet ready either.
		assert!(request_manager.next_parallel_fire_time().is_some());
	}

	#[test]
	fn parallel_fetch_fired_after_threshold() {
		let mut request_manager = RequestManager::new();
		let mut response_manager = ResponseManager::new();
		let metrics = Metrics::default();

		let (identifier, _peer_a, _peer_b, props, group_size) =
			setup_two_advertiser_candidate(&mut request_manager);
		let request_props = |_: &CandidateIdentifier| Some(props.clone());
		let peer_advertised =
			|_: &CandidateIdentifier, _: &PeerId| Some(StatementFilter::full(group_size));

		// Dispatch the first request, then rewind the timer past the threshold.
		let first = request_manager
			.next_request(&mut response_manager, &metrics, request_props, peer_advertised)
			.expect("first dispatch");
		let first_peer = match first.peer.clone() {
			RequestRecipient::Peer(p) => p,
			_ => panic!("expected peer recipient"),
		};
		request_manager.force_parallel_fire_ready(&identifier);

		// Now the second slot should fire — to a different peer.
		let second = request_manager
			.next_request(&mut response_manager, &metrics, request_props, peer_advertised)
			.expect("second dispatch");
		let second_peer = match second.peer.clone() {
			RequestRecipient::Peer(p) => p,
			_ => panic!("expected peer recipient"),
		};
		assert_ne!(first_peer, second_peer);
		assert_eq!(request_manager.in_flight_count_for(&identifier), 2);

		// A third call returns `None` — even though `MAX_PARALLEL_FETCH_SLOTS` allows more
		// in-flight, there are only two advertisers known so no alternate peer is available.
		// This will bump `parallel_fetch_skipped_no_alt_peer` (not asserted here; covered by
		// `parallel_fetch_skipped_when_only_one_advertiser`).
		let third = request_manager.next_request(
			&mut response_manager,
			&metrics,
			request_props,
			peer_advertised,
		);
		assert!(third.is_none());
		assert_eq!(request_manager.in_flight_count_for(&identifier), 2);
	}

	#[test]
	fn parallel_fetch_skipped_when_only_one_advertiser() {
		let mut request_manager = RequestManager::new();
		let mut response_manager = ResponseManager::new();
		let metrics = Metrics::default();

		// Single advertiser scenario.
		let relay_parent = Hash::from_low_u64_le(1);
		let candidate = CandidateHash(Hash::from_low_u64_le(0x20));
		let peer_only = PeerId::random();

		let identifier = request_manager
			.get_or_insert(relay_parent, candidate, 1.into())
			.identifier
			.clone();
		request_manager
			.get_or_insert(relay_parent, candidate, 1.into())
			.add_peer(peer_only);

		let group_size = 3;
		let unwanted_mask = StatementFilter::blank(group_size);
		let props = RequestProperties { unwanted_mask, backing_threshold: None };
		let request_props = |_: &CandidateIdentifier| Some(props.clone());
		let peer_advertised =
			|_: &CandidateIdentifier, _: &PeerId| Some(StatementFilter::full(group_size));

		// First dispatch goes to the only peer.
		assert!(request_manager
			.next_request(&mut response_manager, &metrics, request_props, peer_advertised)
			.is_some());

		// Even after the threshold elapses, no second slot — there's no alt peer.
		request_manager.force_parallel_fire_ready(&identifier);
		let second = request_manager.next_request(
			&mut response_manager,
			&metrics,
			request_props,
			peer_advertised,
		);
		assert!(second.is_none());
		assert_eq!(request_manager.in_flight_count_for(&identifier), 1);
	}

	#[test]
	fn parallel_fetch_first_wins_late_second_outdated() {
		let mut request_manager = RequestManager::new();
		let mut response_manager = ResponseManager::new();
		let metrics = Metrics::default();

		let relay_parent = Hash::from_low_u64_le(1);
		let mut candidate_receipt = test_helpers::dummy_committed_candidate_receipt(relay_parent);
		let persisted_validation_data = dummy_pvd();
		candidate_receipt.descriptor.persisted_validation_data_hash =
			persisted_validation_data.hash();
		let candidate = candidate_receipt.hash();
		let candidate_receipt: CommittedCandidateReceipt = candidate_receipt.into();

		let peer_a = PeerId::random();
		let peer_b = PeerId::random();

		let identifier = request_manager
			.get_or_insert(relay_parent, candidate, 1.into())
			.identifier
			.clone();
		request_manager
			.get_or_insert(relay_parent, candidate, 1.into())
			.add_peer(peer_a);
		request_manager
			.get_or_insert(relay_parent, candidate, 1.into())
			.add_peer(peer_b);

		let group_size = 3;
		let group = &[ValidatorIndex(0), ValidatorIndex(1), ValidatorIndex(2)];
		let disabled_mask: BitVec<u8, Lsb0> = Default::default();
		let unwanted_mask = StatementFilter::blank(group_size);
		let request_properties = RequestProperties { unwanted_mask, backing_threshold: None };
		let request_props = |_: &CandidateIdentifier| Some(request_properties.clone());
		let peer_advertised =
			|_: &CandidateIdentifier, _: &PeerId| Some(StatementFilter::full(group_size));

		// Fire both slots.
		assert!(request_manager
			.next_request(&mut response_manager, &metrics, request_props, peer_advertised)
			.is_some());
		request_manager.force_parallel_fire_ready(&identifier);
		assert!(request_manager
			.next_request(&mut response_manager, &metrics, request_props, peer_advertised)
			.is_some());
		assert_eq!(request_manager.in_flight_count_for(&identifier), 2);

		// First response Completes. This removes all identifiers for the candidate.
		let first_response = UnhandledResponse {
			response: TaggedResponse {
				identifier: identifier.clone(),
				requested_peer: peer_a,
				slot: FetchSlot::First,
				sent_at: Instant::now(),
				props: request_properties.clone(),
				response: Ok(AttestedCandidateResponse {
					candidate_receipt: candidate_receipt.clone().into(),
					persisted_validation_data: persisted_validation_data.clone(),
					statements: vec![],
				}),
			},
		};
		let output = first_response.validate_response(
			&mut request_manager,
			group,
			0,
			|_| None,
			|_, _| true,
			disabled_mask.clone(),
			&Default::default(),
		);
		assert!(matches!(output.request_status, CandidateRequestStatus::Complete { .. }));
		assert_eq!(output.fetch_completion.slot, FetchSlot::First);
		assert_eq!(output.fetch_completion.outcome, FetchOutcome::Success);
		// End-to-end learn-to-fetch duration must be populated on Complete.
		assert!(output.learn_to_fetch.is_some());
		// `Complete` triggers `remove_for`; the entry is gone.
		assert_eq!(request_manager.total_requests_count(), 0);

		// The second-slot response now arrives late: the entry is gone → Outdated, no rep.
		let second_response = UnhandledResponse {
			response: TaggedResponse {
				identifier,
				requested_peer: peer_b,
				slot: FetchSlot::Parallel,
				sent_at: Instant::now(),
				props: request_properties,
				response: Ok(AttestedCandidateResponse {
					candidate_receipt: candidate_receipt.into(),
					persisted_validation_data,
					statements: vec![],
				}),
			},
		};
		let output = second_response.validate_response(
			&mut request_manager,
			group,
			0,
			|_| None,
			|_, _| true,
			disabled_mask,
			&Default::default(),
		);
		assert_eq!(output.request_status, CandidateRequestStatus::Outdated);
		assert_eq!(output.reputation_changes, vec![]);
		assert_eq!(output.fetch_completion.slot, FetchSlot::Parallel);
		assert_eq!(output.fetch_completion.outcome, FetchOutcome::Dropped);
		// Outdated path — entry already gone — no learn-to-fetch measurement.
		assert!(output.learn_to_fetch.is_none());
	}

	#[test]
	fn parallel_fetch_second_wins_late_first_outdated_no_rep() {
		let mut request_manager = RequestManager::new();
		let mut response_manager = ResponseManager::new();
		let metrics = Metrics::default();

		let relay_parent = Hash::from_low_u64_le(1);
		let mut candidate_receipt = test_helpers::dummy_committed_candidate_receipt(relay_parent);
		let persisted_validation_data = dummy_pvd();
		candidate_receipt.descriptor.persisted_validation_data_hash =
			persisted_validation_data.hash();
		let candidate = candidate_receipt.hash();
		let candidate_receipt: CommittedCandidateReceipt = candidate_receipt.into();

		let peer_a = PeerId::random();
		let peer_b = PeerId::random();

		let identifier = request_manager
			.get_or_insert(relay_parent, candidate, 1.into())
			.identifier
			.clone();
		request_manager
			.get_or_insert(relay_parent, candidate, 1.into())
			.add_peer(peer_a);
		request_manager
			.get_or_insert(relay_parent, candidate, 1.into())
			.add_peer(peer_b);

		let group_size = 3;
		let group = &[ValidatorIndex(0), ValidatorIndex(1), ValidatorIndex(2)];
		let disabled_mask: BitVec<u8, Lsb0> = Default::default();
		let unwanted_mask = StatementFilter::blank(group_size);
		let request_properties = RequestProperties { unwanted_mask, backing_threshold: None };
		let request_props = |_: &CandidateIdentifier| Some(request_properties.clone());
		let peer_advertised =
			|_: &CandidateIdentifier, _: &PeerId| Some(StatementFilter::full(group_size));

		assert!(request_manager
			.next_request(&mut response_manager, &metrics, request_props, peer_advertised)
			.is_some());
		request_manager.force_parallel_fire_ready(&identifier);
		assert!(request_manager
			.next_request(&mut response_manager, &metrics, request_props, peer_advertised)
			.is_some());

		// Second response wins.
		let second_response = UnhandledResponse {
			response: TaggedResponse {
				identifier: identifier.clone(),
				requested_peer: peer_b,
				slot: FetchSlot::Parallel,
				sent_at: Instant::now(),
				props: request_properties.clone(),
				response: Ok(AttestedCandidateResponse {
					candidate_receipt: candidate_receipt.clone().into(),
					persisted_validation_data: persisted_validation_data.clone(),
					statements: vec![],
				}),
			},
		};
		let output = second_response.validate_response(
			&mut request_manager,
			group,
			0,
			|_| None,
			|_, _| true,
			disabled_mask.clone(),
			&Default::default(),
		);
		assert!(matches!(output.request_status, CandidateRequestStatus::Complete { .. }));
		assert_eq!(output.fetch_completion.slot, FetchSlot::Parallel);
		assert_eq!(output.fetch_completion.outcome, FetchOutcome::Success);
		assert_eq!(request_manager.total_requests_count(), 0);

		// Late first response → Outdated, no reputation change for the slow peer.
		let first_response = UnhandledResponse {
			response: TaggedResponse {
				identifier,
				requested_peer: peer_a,
				slot: FetchSlot::First,
				sent_at: Instant::now(),
				props: request_properties,
				response: Ok(AttestedCandidateResponse {
					candidate_receipt: candidate_receipt.into(),
					persisted_validation_data,
					statements: vec![],
				}),
			},
		};
		let output = first_response.validate_response(
			&mut request_manager,
			group,
			0,
			|_| None,
			|_, _| true,
			disabled_mask,
			&Default::default(),
		);
		assert_eq!(output.request_status, CandidateRequestStatus::Outdated);
		assert_eq!(output.reputation_changes, vec![]);
		assert_eq!(output.fetch_completion.slot, FetchSlot::First);
		assert_eq!(output.fetch_completion.outcome, FetchOutcome::Dropped);
	}

	#[test]
	fn parallel_fetch_respects_per_peer_cap() {
		// Two candidates both have only peer_a as advertiser. First candidate fires slot 1
		// to peer_a; second candidate's `next_request` cannot fire (peer_a is busy with the
		// first candidate's request) and second-slot dispatch is similarly blocked.
		let mut request_manager = RequestManager::new();
		let mut response_manager = ResponseManager::new();
		let metrics = Metrics::default();

		let relay_parent = Hash::from_low_u64_le(1);
		let candidate_x = CandidateHash(Hash::from_low_u64_le(0x30));
		let candidate_y = CandidateHash(Hash::from_low_u64_le(0x31));
		let peer_a = PeerId::random();
		let peer_b = PeerId::random();

		// Candidate X knows peer_a and peer_b.
		let identifier_x = request_manager
			.get_or_insert(relay_parent, candidate_x, 1.into())
			.identifier
			.clone();
		request_manager
			.get_or_insert(relay_parent, candidate_x, 1.into())
			.add_peer(peer_a);
		request_manager
			.get_or_insert(relay_parent, candidate_x, 1.into())
			.add_peer(peer_b);

		// Candidate Y knows only peer_b.
		let _identifier_y = request_manager
			.get_or_insert(relay_parent, candidate_y, 1.into())
			.identifier
			.clone();
		request_manager
			.get_or_insert(relay_parent, candidate_y, 1.into())
			.add_peer(peer_b);

		let group_size = 3;
		let unwanted_mask = StatementFilter::blank(group_size);
		let props = RequestProperties { unwanted_mask, backing_threshold: None };
		let request_props = |_: &CandidateIdentifier| Some(props.clone());
		let peer_advertised =
			|_: &CandidateIdentifier, _: &PeerId| Some(StatementFilter::full(group_size));

		// First two dispatches: X → peer_a, Y → peer_b (no overlap).
		assert!(request_manager
			.next_request(&mut response_manager, &metrics, request_props, peer_advertised)
			.is_some());
		assert!(request_manager
			.next_request(&mut response_manager, &metrics, request_props, peer_advertised)
			.is_some());
		assert_eq!(response_manager.active_peers.len(), 2);

		// Now force X's parallel-fire threshold. The only alternate advertiser for X is peer_b,
		// but peer_b is already busy with Y → no second-slot dispatch for X.
		request_manager.force_parallel_fire_ready(&identifier_x);
		let blocked = request_manager.next_request(
			&mut response_manager,
			&metrics,
			request_props,
			peer_advertised,
		);
		assert!(blocked.is_none());
		assert_eq!(request_manager.in_flight_count_for(&identifier_x), 1);
	}

	#[test]
	fn parallel_fetch_cleanup_on_remove_for() {
		let mut request_manager = RequestManager::new();
		let mut response_manager = ResponseManager::new();
		let metrics = Metrics::default();

		let (identifier, _peer_a, _peer_b, props, group_size) =
			setup_two_advertiser_candidate(&mut request_manager);
		let request_props = |_: &CandidateIdentifier| Some(props.clone());
		let peer_advertised =
			|_: &CandidateIdentifier, _: &PeerId| Some(StatementFilter::full(group_size));

		// Fire both slots.
		assert!(request_manager
			.next_request(&mut response_manager, &metrics, request_props, peer_advertised)
			.is_some());
		request_manager.force_parallel_fire_ready(&identifier);
		assert!(request_manager
			.next_request(&mut response_manager, &metrics, request_props, peer_advertised)
			.is_some());

		// Remove the candidate. Both slots' state must be cleaned up — when the still-pending
		// response futures eventually resolve they'll find no entry and return Outdated.
		request_manager.remove_for(identifier.candidate_hash);
		assert_eq!(request_manager.total_requests_count(), 0);
		assert_eq!(request_manager.in_flight_count_for(&identifier), 0);
		assert!(request_manager.next_parallel_fire_time().is_none());
	}

	#[test]
	fn parallel_fetch_threshold_below_hard_timeout() {
		// Sanity: a 500ms parallel-fire threshold must sit below the 2500ms hard request
		// timeout so the parallel fetch can be dispatched while the original request is
		// still alive. Mirrors the static assertion at the top of the module.
		assert!(PARALLEL_FETCH_THRESHOLD.as_millis() < 2500);
	}

	/// Staggered-firing invariant test — guards the PR2 random-gossip extension. With
	/// `k` slots in-flight (`1 <= k < MAX`), the next parallel slot becomes dispatchable at
	/// exactly `first_request_sent_at + k * PARALLEL_FETCH_THRESHOLD`. Once the in-flight
	/// set is full (`k == MAX`), no further parallel slots are dispatched.
	#[test]
	fn parallel_fetch_staggered_threshold_invariant() {
		let mut request_manager = RequestManager::new();

		let relay_parent = Hash::from_low_u64_le(1);
		let candidate = CandidateHash(Hash::from_low_u64_le(0x42));

		let identifier = request_manager
			.get_or_insert(relay_parent, candidate, 1.into())
			.identifier
			.clone();

		// Anchor a known `first_request_sent_at`.
		let sent_at = Instant::now();

		// Walk through slots 1..MAX. For each slot count `k`, the next parallel fire time
		// must be exactly `sent_at + k * PARALLEL_FETCH_THRESHOLD`.
		for k in 1..=MAX_PARALLEL_FETCH_SLOTS {
			{
				let entry = request_manager.requests.get_mut(&identifier).unwrap();
				entry.in_flight.clear();
				for _ in 0..k {
					entry
						.in_flight
						.try_push(PeerId::random())
						.expect("k <= MAX_PARALLEL_FETCH_SLOTS");
				}
				entry.first_request_sent_at = Some(sent_at);
			}

			let next = request_manager.next_parallel_fire_time();

			if k == MAX_PARALLEL_FETCH_SLOTS {
				// All slots full — no further parallel dispatch.
				assert_eq!(
					next, None,
					"with {} slots filled (== MAX), no more parallel slots should be pending",
					k
				);
			} else {
				let expected = sent_at + PARALLEL_FETCH_THRESHOLD * k as u32;
				assert_eq!(
					next,
					Some(expected),
					"with {} slots filled, next parallel fire should be at sent_at + {}*THRESHOLD",
					k,
					k,
				);
			}
		}
	}
}
