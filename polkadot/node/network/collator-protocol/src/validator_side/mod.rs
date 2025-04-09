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

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use futures::{
	channel::oneshot, future::BoxFuture, select, stream::FuturesUnordered, FutureExt, StreamExt,
};
use futures_timer::Delay;
use std::{
	collections::{hash_map::Entry, HashMap, HashSet, VecDeque},
	future::Future,
	time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

use sp_keystore::KeystorePtr;

use polkadot_node_network_protocol::{
	self as net_protocol,
	peer_set::{CollationVersion, PeerSet},
	request_response::{
		outgoing::{Recipient, RequestError},
		v1 as request_v1, v2 as request_v2, OutgoingRequest, Requests,
	},
	v1 as protocol_v1, v2 as protocol_v2, CollationProtocols, OurView, PeerId,
	UnifiedReputationChange as Rep, View,
};
use polkadot_node_primitives::{SignedFullStatement, Statement};
use polkadot_node_subsystem::{
	messages::{
		CanSecondRequest, CandidateBackingMessage, CollatorProtocolMessage, IfDisconnected,
		NetworkBridgeEvent, NetworkBridgeTxMessage, ParentHeadData, ProspectiveParachainsMessage,
		ProspectiveValidationDataRequest,
	},
	overseer, CollatorProtocolSenderTrait, FromOrchestra, OverseerSignal,
};
use polkadot_node_subsystem_util::{
	backing_implicit_view::View as ImplicitView,
	reputation::{ReputationAggregator, REPUTATION_CHANGE_INTERVAL},
	request_claim_queue, request_node_features, request_session_index_for_child,
};
use polkadot_primitives::{
	node_features,
	vstaging::{CandidateDescriptorV2, CandidateDescriptorVersion},
	CandidateHash, CollatorId, CoreIndex, Hash, HeadData, Id as ParaId, OccupiedCoreAssumption,
	PersistedValidationData, SessionIndex,
};

use super::{modify_reputation, tick_stream, LOG_TARGET};

mod claim_queue_state;
mod collation;
mod error;
mod metrics;

use claim_queue_state::ClaimQueueState;
use collation::{
	fetched_collation_sanity_check, BlockedCollationId, CollationEvent, CollationFetchError,
	CollationFetchRequest, CollationStatus, Collations, FetchedCollation, PendingCollation,
	PendingCollationFetch, ProspectiveCandidate,
};
use error::{Error, FetchError, Result, SecondingError};

#[cfg(test)]
mod tests;

pub use metrics::Metrics;

const COST_UNEXPECTED_MESSAGE: Rep = Rep::CostMinor("An unexpected message");
/// Message could not be decoded properly.
const COST_CORRUPTED_MESSAGE: Rep = Rep::CostMinor("Message was corrupt");
/// Network errors that originated at the remote host should have same cost as timeout.
const COST_NETWORK_ERROR: Rep = Rep::CostMinor("Some network error");
const COST_INVALID_SIGNATURE: Rep = Rep::Malicious("Invalid network message signature");
const COST_REPORT_BAD: Rep = Rep::Malicious("A collator was reported by another subsystem");
const COST_WRONG_PARA: Rep = Rep::Malicious("A collator provided a collation for the wrong para");
const COST_PROTOCOL_MISUSE: Rep =
	Rep::Malicious("A collator advertising a collation for an async backing relay parent using V1");
const COST_UNNEEDED_COLLATOR: Rep = Rep::CostMinor("An unneeded collator connected");
const BENEFIT_NOTIFY_GOOD: Rep =
	Rep::BenefitMinor("A collator was noted good by another subsystem");

/// Time after starting a collation download from a collator we will start another one from the
/// next collator even if the upload was not finished yet.
///
/// This is to protect from a single slow collator preventing collations from happening.
///
/// With a collation size of 5MB and bandwidth of 500Mbit/s (requirement for Kusama validators),
/// the transfer should be possible within 0.1 seconds. 400 milliseconds should therefore be
/// plenty, even with multiple heads and should be low enough for later collators to still be able
/// to finish on time.
///
/// There is debug logging output, so we can adjust this value based on production results.
#[cfg(not(test))]
const MAX_UNSHARED_DOWNLOAD_TIME: Duration = Duration::from_millis(400);

// How often to check all peers with activity.
#[cfg(not(test))]
const ACTIVITY_POLL: Duration = Duration::from_secs(1);

#[cfg(test)]
const MAX_UNSHARED_DOWNLOAD_TIME: Duration = Duration::from_millis(100);

#[cfg(test)]
const ACTIVITY_POLL: Duration = Duration::from_millis(10);

#[derive(Debug)]
struct CollatingPeerState {
	collator_id: CollatorId,
	para_id: ParaId,
	/// Collations advertised by peer per relay parent.
	///
	/// V1 network protocol doesn't include candidate hash in
	/// advertisements, we store an empty set in this case to occupy
	/// a slot in map.
	advertisements: HashMap<Hash, HashSet<CandidateHash>>,
	last_active: Instant,
}

#[derive(Debug)]
enum PeerState {
	// The peer has connected at the given instant.
	Connected(Instant),
	// Peer is collating.
	Collating(CollatingPeerState),
}

#[derive(Debug)]
enum InsertAdvertisementError {
	/// Advertisement is already known.
	Duplicate,
	/// Collation relay parent is out of our view.
	OutOfOurView,
	/// No prior declare message received.
	UndeclaredCollator,
	/// A limit for announcements per peer is reached.
	PeerLimitReached,
}

#[derive(Debug)]
struct PeerData {
	view: View,
	state: PeerState,
	version: CollationVersion,
}

impl PeerData {
	/// Update the view, clearing all advertisements that are no longer in the
	/// current view.
	fn update_view(
		&mut self,
		implicit_view: &ImplicitView,
		active_leaves: &HashSet<Hash>,
		new_view: View,
	) {
		let old_view = std::mem::replace(&mut self.view, new_view);
		if let PeerState::Collating(ref mut peer_state) = self.state {
			for removed in old_view.difference(&self.view) {
				// Remove relay parent advertisements if it went out of our (implicit) view.
				let keep = is_relay_parent_in_implicit_view(
					removed,
					implicit_view,
					active_leaves,
					peer_state.para_id,
				);

				if !keep {
					peer_state.advertisements.remove(&removed);
				}
			}
		}
	}

	/// Prune old advertisements relative to our view.
	fn prune_old_advertisements(
		&mut self,
		implicit_view: &ImplicitView,
		active_leaves: &HashSet<Hash>,
	) {
		if let PeerState::Collating(ref mut peer_state) = self.state {
			peer_state.advertisements.retain(|hash, _| {
				// Either
				// - Relay parent is an active leaf
				// - It belongs to allowed ancestry under some leaf
				// Discard otherwise.
				is_relay_parent_in_implicit_view(
					hash,
					implicit_view,
					active_leaves,
					peer_state.para_id,
				)
			});
		}
	}

	/// Performs sanity check for an advertisement and notes it as advertised.
	fn insert_advertisement(
		&mut self,
		on_relay_parent: Hash,
		candidate_hash: Option<CandidateHash>,
		implicit_view: &ImplicitView,
		active_leaves: &HashSet<Hash>,
		per_relay_parent: &PerRelayParent,
	) -> std::result::Result<(CollatorId, ParaId), InsertAdvertisementError> {
		match self.state {
			PeerState::Connected(_) => Err(InsertAdvertisementError::UndeclaredCollator),
			PeerState::Collating(ref mut state) => {
				if !is_relay_parent_in_implicit_view(
					&on_relay_parent,
					implicit_view,
					active_leaves,
					state.para_id,
				) {
					return Err(InsertAdvertisementError::OutOfOurView)
				}

				if let Some(candidate_hash) = candidate_hash {
					if state
						.advertisements
						.get(&on_relay_parent)
						.map_or(false, |candidates| candidates.contains(&candidate_hash))
					{
						return Err(InsertAdvertisementError::Duplicate)
					}

					let candidates = state.advertisements.entry(on_relay_parent).or_default();

					// Current assignments is equal to the length of the claim queue. No honest
					// collator should send that many advertisements.
					if candidates.len() > per_relay_parent.assignment.current.len() {
						return Err(InsertAdvertisementError::PeerLimitReached)
					}

					candidates.insert(candidate_hash);
				} else {
					if self.version != CollationVersion::V1 {
						gum::error!(
							target: LOG_TARGET,
							"Programming error, `candidate_hash` can not be `None` \
							 for non `V1` networking.",
						);
					}

					if state.advertisements.contains_key(&on_relay_parent) {
						return Err(InsertAdvertisementError::Duplicate)
					}

					state
						.advertisements
						.insert(on_relay_parent, HashSet::from_iter(candidate_hash));
				};

				state.last_active = Instant::now();
				Ok((state.collator_id.clone(), state.para_id))
			},
		}
	}

	/// Whether a peer is collating.
	fn is_collating(&self) -> bool {
		match self.state {
			PeerState::Connected(_) => false,
			PeerState::Collating(_) => true,
		}
	}

	/// Note that a peer is now collating with the given collator and para id.
	///
	/// This will overwrite any previous call to `set_collating` and should only be called
	/// if `is_collating` is false.
	fn set_collating(&mut self, collator_id: CollatorId, para_id: ParaId) {
		self.state = PeerState::Collating(CollatingPeerState {
			collator_id,
			para_id,
			advertisements: HashMap::new(),
			last_active: Instant::now(),
		});
	}

	fn collator_id(&self) -> Option<&CollatorId> {
		match self.state {
			PeerState::Connected(_) => None,
			PeerState::Collating(ref state) => Some(&state.collator_id),
		}
	}

	fn collating_para(&self) -> Option<ParaId> {
		match self.state {
			PeerState::Connected(_) => None,
			PeerState::Collating(ref state) => Some(state.para_id),
		}
	}

	/// Whether the peer has advertised the given collation.
	fn has_advertised(
		&self,
		relay_parent: &Hash,
		maybe_candidate_hash: Option<CandidateHash>,
	) -> bool {
		let collating_state = match self.state {
			PeerState::Connected(_) => return false,
			PeerState::Collating(ref state) => state,
		};

		if let Some(ref candidate_hash) = maybe_candidate_hash {
			collating_state
				.advertisements
				.get(relay_parent)
				.map_or(false, |candidates| candidates.contains(candidate_hash))
		} else {
			collating_state.advertisements.contains_key(relay_parent)
		}
	}

	/// Whether the peer is now inactive according to the current instant and the eviction policy.
	fn is_inactive(&self, policy: &crate::CollatorEvictionPolicy) -> bool {
		match self.state {
			PeerState::Connected(connected_at) => connected_at.elapsed() >= policy.undeclared,
			PeerState::Collating(ref state) =>
				state.last_active.elapsed() >= policy.inactive_collator,
		}
	}
}

#[derive(Debug)]
struct GroupAssignments {
	/// Current assignments.
	current: Vec<ParaId>,
}

struct PerRelayParent {
	assignment: GroupAssignments,
	collations: Collations,
	v2_receipts: bool,
	current_core: CoreIndex,
	session_index: SessionIndex,
}

/// All state relevant for the validator side of the protocol lives here.
#[derive(Default)]
struct State {
	/// Leaves that do support asynchronous backing along with
	/// implicit ancestry. Leaves from the implicit view are present in
	/// `active_leaves`, the opposite doesn't hold true.
	///
	/// Relay-chain blocks which don't support prospective parachains are
	/// never included in the fragment chains of active leaves which do. In
	/// particular, this means that if a given relay parent belongs to implicit
	/// ancestry of some active leaf, then it does support prospective parachains.
	implicit_view: ImplicitView,

	/// All active leaves observed by us. This works as a replacement for
	/// [`polkadot_node_network_protocol::View`] and can be dropped once the transition
	/// to asynchronous backing is done.
	active_leaves: HashSet<Hash>,

	/// State tracked per relay parent.
	per_relay_parent: HashMap<Hash, PerRelayParent>,

	/// Track all active collators and their data.
	peer_data: HashMap<PeerId, PeerData>,

	/// Parachains we're currently assigned to. With async backing enabled
	/// this includes assignments from the implicit view.
	current_assignments: HashMap<ParaId, usize>,

	/// The collations we have requested from collators.
	collation_requests: FuturesUnordered<CollationFetchRequest>,

	/// Cancellation handles for the collation fetch requests.
	collation_requests_cancel_handles: HashMap<PendingCollation, CancellationToken>,

	/// Metrics.
	metrics: Metrics,

	/// When a timer in this `FuturesUnordered` triggers, we should dequeue the next request
	/// attempt in the corresponding `collations_per_relay_parent`.
	///
	/// A triggering timer means that the fetching took too long for our taste and we should give
	/// another collator the chance to be faster (dequeue next fetch request as well).
	collation_fetch_timeouts:
		FuturesUnordered<BoxFuture<'static, (CollatorId, Option<CandidateHash>, Hash)>>,

	/// Collations that we have successfully requested from peers and waiting
	/// on validation.
	fetched_candidates: HashMap<FetchedCollation, CollationEvent>,

	/// Collations which we haven't been able to second due to their parent not being known by
	/// prospective-parachains. Mapped from the paraid and parent_head_hash to the fetched
	/// collation data. Only needed for async backing. For elastic scaling, the fetched collation
	/// must contain the full parent head data.
	blocked_from_seconding: HashMap<BlockedCollationId, Vec<PendingCollationFetch>>,

	/// Aggregated reputation change
	reputation: ReputationAggregator,
}

impl State {
	// Returns the number of seconded and pending collations for a specific `ParaId`. Pending
	// collations are:
	// 1. Collations being fetched from a collator.
	// 2. Collations waiting for validation from backing subsystem.
	// 3. Collations blocked from seconding due to parent not being known by backing subsystem.
	fn seconded_and_pending_for_para(&self, relay_parent: &Hash, para_id: &ParaId) -> usize {
		let seconded = self
			.per_relay_parent
			.get(relay_parent)
			.map_or(0, |per_relay_parent| per_relay_parent.collations.seconded_for_para(para_id));

		let pending_fetch = self.per_relay_parent.get(relay_parent).map_or(0, |rp_state| {
			match rp_state.collations.status {
				CollationStatus::Fetching(pending_para_id) if pending_para_id == *para_id => 1,
				_ => 0,
			}
		});

		let waiting_for_validation = self
			.fetched_candidates
			.keys()
			.filter(|fc| fc.relay_parent == *relay_parent && fc.para_id == *para_id)
			.count();

		let blocked_from_seconding =
			self.blocked_from_seconding.values().fold(0, |acc, blocked_collations| {
				acc + blocked_collations
					.iter()
					.filter(|pc| {
						pc.candidate_receipt.descriptor.para_id() == *para_id &&
							pc.candidate_receipt.descriptor.relay_parent() == *relay_parent
					})
					.count()
			});

		gum::trace!(
			target: LOG_TARGET,
			?relay_parent,
			?para_id,
			seconded,
			pending_fetch,
			waiting_for_validation,
			blocked_from_seconding,
			"Seconded and pending collations for para",
		);

		seconded + pending_fetch + waiting_for_validation + blocked_from_seconding
	}
}

fn is_relay_parent_in_implicit_view(
	relay_parent: &Hash,
	implicit_view: &ImplicitView,
	active_leaves: &HashSet<Hash>,
	para_id: ParaId,
) -> bool {
	active_leaves.iter().any(|hash| {
		implicit_view
			.known_allowed_relay_parents_under(hash, Some(para_id))
			.unwrap_or_default()
			.contains(relay_parent)
	})
}

async fn construct_per_relay_parent<Sender>(
	sender: &mut Sender,
	current_assignments: &mut HashMap<ParaId, usize>,
	keystore: &KeystorePtr,
	relay_parent: Hash,
	v2_receipts: bool,
	session_index: SessionIndex,
) -> Result<Option<PerRelayParent>>
where
	Sender: CollatorProtocolSenderTrait,
{
	let validators = polkadot_node_subsystem_util::request_validators(relay_parent, sender)
		.await
		.await
		.map_err(Error::CancelledActiveValidators)??;

	let (groups, rotation_info) =
		polkadot_node_subsystem_util::request_validator_groups(relay_parent, sender)
			.await
			.await
			.map_err(Error::CancelledValidatorGroups)??;

	let core_now = if let Some(group) =
		polkadot_node_subsystem_util::signing_key_and_index(&validators, keystore).and_then(
			|(_, index)| polkadot_node_subsystem_util::find_validator_group(&groups, index),
		) {
		rotation_info.core_for_group(group, groups.len())
	} else {
		gum::trace!(target: LOG_TARGET, ?relay_parent, "Not a validator");
		return Ok(None)
	};

	let mut claim_queue = request_claim_queue(relay_parent, sender)
		.await
		.await
		.map_err(Error::CancelledClaimQueue)??;

	let assigned_paras = claim_queue.remove(&core_now).unwrap_or_else(|| VecDeque::new());

	for para_id in assigned_paras.iter() {
		let entry = current_assignments.entry(*para_id).or_default();
		*entry += 1;
		if *entry == 1 {
			gum::debug!(
				target: LOG_TARGET,
				?relay_parent,
				para_id = ?para_id,
				"Assigned to a parachain",
			);
		}
	}

	let assignment = GroupAssignments { current: assigned_paras.into_iter().collect() };
	let collations = Collations::new(&assignment.current);

	Ok(Some(PerRelayParent {
		assignment,
		collations,
		v2_receipts,
		session_index,
		current_core: core_now,
	}))
}

fn remove_outgoing(
	current_assignments: &mut HashMap<ParaId, usize>,
	per_relay_parent: PerRelayParent,
) {
	let GroupAssignments { current, .. } = per_relay_parent.assignment;

	for cur in current {
		if let Entry::Occupied(mut occupied) = current_assignments.entry(cur) {
			*occupied.get_mut() -= 1;
			if *occupied.get() == 0 {
				occupied.remove_entry();
				gum::debug!(
					target: LOG_TARGET,
					para_id = ?cur,
					"Unassigned from a parachain",
				);
			}
		}
	}
}

// O(n) search for collator ID by iterating through the peers map. This should be fast enough
// unless a large amount of peers is expected.
fn collator_peer_id(
	peer_data: &HashMap<PeerId, PeerData>,
	collator_id: &CollatorId,
) -> Option<PeerId> {
	peer_data
		.iter()
		.find_map(|(peer, data)| data.collator_id().filter(|c| c == &collator_id).map(|_| *peer))
}

async fn disconnect_peer(sender: &mut impl overseer::CollatorProtocolSenderTrait, peer_id: PeerId) {
	sender
		.send_message(NetworkBridgeTxMessage::DisconnectPeer(peer_id, PeerSet::Collation))
		.await
}

/// Another subsystem has requested to fetch collations on a particular leaf for some para.
async fn fetch_collation(
	sender: &mut impl overseer::CollatorProtocolSenderTrait,
	state: &mut State,
	pc: PendingCollation,
	id: CollatorId,
) -> std::result::Result<(), FetchError> {
	let PendingCollation { relay_parent, peer_id, prospective_candidate, .. } = pc;
	let candidate_hash = prospective_candidate.as_ref().map(ProspectiveCandidate::candidate_hash);

	let peer_data = state.peer_data.get(&peer_id).ok_or(FetchError::UnknownPeer)?;

	if peer_data.has_advertised(&relay_parent, candidate_hash) {
		request_collation(sender, state, pc, id.clone(), peer_data.version).await?;
		let timeout = |collator_id, candidate_hash, relay_parent| async move {
			Delay::new(MAX_UNSHARED_DOWNLOAD_TIME).await;
			(collator_id, candidate_hash, relay_parent)
		};
		state
			.collation_fetch_timeouts
			.push(timeout(id.clone(), candidate_hash, relay_parent).boxed());

		Ok(())
	} else {
		Err(FetchError::NotAdvertised)
	}
}

/// Report a collator for some malicious actions.
async fn report_collator(
	reputation: &mut ReputationAggregator,
	sender: &mut impl overseer::CollatorProtocolSenderTrait,
	peer_data: &HashMap<PeerId, PeerData>,
	id: CollatorId,
) {
	if let Some(peer_id) = collator_peer_id(peer_data, &id) {
		modify_reputation(reputation, sender, peer_id, COST_REPORT_BAD).await;
	}
}

/// Some other subsystem has reported a collator as a good one, bump reputation.
async fn note_good_collation(
	reputation: &mut ReputationAggregator,
	sender: &mut impl overseer::CollatorProtocolSenderTrait,
	peer_data: &HashMap<PeerId, PeerData>,
	id: CollatorId,
) {
	if let Some(peer_id) = collator_peer_id(peer_data, &id) {
		modify_reputation(reputation, sender, peer_id, BENEFIT_NOTIFY_GOOD).await;
	}
}

/// Notify a collator that its collation got seconded.
async fn notify_collation_seconded(
	sender: &mut impl overseer::CollatorProtocolSenderTrait,
	peer_id: PeerId,
	version: CollationVersion,
	relay_parent: Hash,
	statement: SignedFullStatement,
) {
	let statement = statement.into();
	let wire_message = match version {
		CollationVersion::V1 =>
			CollationProtocols::V1(protocol_v1::CollationProtocol::CollatorProtocol(
				protocol_v1::CollatorProtocolMessage::CollationSeconded(relay_parent, statement),
			)),
		CollationVersion::V2 =>
			CollationProtocols::V2(protocol_v2::CollationProtocol::CollatorProtocol(
				protocol_v2::CollatorProtocolMessage::CollationSeconded(relay_parent, statement),
			)),
	};
	sender
		.send_message(NetworkBridgeTxMessage::SendCollationMessage(vec![peer_id], wire_message))
		.await;
}

/// A peer's view has changed. A number of things should be done:
///  - Ongoing collation requests have to be canceled.
///  - Advertisements by this peer that are no longer relevant have to be removed.
fn handle_peer_view_change(state: &mut State, peer_id: PeerId, view: View) {
	let peer_data = match state.peer_data.get_mut(&peer_id) {
		Some(peer_data) => peer_data,
		None => return,
	};

	peer_data.update_view(&state.implicit_view, &state.active_leaves, view);
	state.collation_requests_cancel_handles.retain(|pc, handle| {
		let keep = pc.peer_id != peer_id || peer_data.has_advertised(&pc.relay_parent, None);
		if !keep {
			handle.cancel();
		}
		keep
	});
}

/// Request a collation from the network.
/// This function will
///  - Check for duplicate requests.
///  - Check if the requested collation is in our view.
/// And as such invocations of this function may rely on that.
async fn request_collation(
	sender: &mut impl overseer::CollatorProtocolSenderTrait,
	state: &mut State,
	pending_collation: PendingCollation,
	collator_id: CollatorId,
	peer_protocol_version: CollationVersion,
) -> std::result::Result<(), FetchError> {
	if state.collation_requests_cancel_handles.contains_key(&pending_collation) {
		return Err(FetchError::AlreadyRequested)
	}

	let PendingCollation { relay_parent, para_id, peer_id, prospective_candidate, .. } =
		pending_collation;
	let per_relay_parent = state
		.per_relay_parent
		.get_mut(&relay_parent)
		.ok_or(FetchError::RelayParentOutOfView)?;

	let (requests, response_recv) = match (peer_protocol_version, prospective_candidate) {
		(CollationVersion::V1, _) => {
			let (req, response_recv) = OutgoingRequest::new(
				Recipient::Peer(peer_id),
				request_v1::CollationFetchingRequest { relay_parent, para_id },
			);
			let requests = Requests::CollationFetchingV1(req);
			(requests, response_recv.boxed())
		},
		(CollationVersion::V2, Some(ProspectiveCandidate { candidate_hash, .. })) => {
			let (req, response_recv) = OutgoingRequest::new(
				Recipient::Peer(peer_id),
				request_v2::CollationFetchingRequest { relay_parent, para_id, candidate_hash },
			);
			let requests = Requests::CollationFetchingV2(req);
			(requests, response_recv.boxed())
		},
		_ => return Err(FetchError::ProtocolMismatch),
	};

	let cancellation_token = CancellationToken::new();
	let collation_request = CollationFetchRequest {
		pending_collation,
		collator_id: collator_id.clone(),
		collator_protocol_version: peer_protocol_version,
		from_collator: response_recv,
		cancellation_token: cancellation_token.clone(),
		_lifetime_timer: state.metrics.time_collation_request_duration(),
	};

	state.collation_requests.push(collation_request);
	state
		.collation_requests_cancel_handles
		.insert(pending_collation, cancellation_token);

	gum::debug!(
		target: LOG_TARGET,
		peer_id = %peer_id,
		%para_id,
		?relay_parent,
		"Requesting collation",
	);

	let maybe_candidate_hash =
		prospective_candidate.as_ref().map(ProspectiveCandidate::candidate_hash);
	per_relay_parent.collations.status = CollationStatus::Fetching(para_id);
	per_relay_parent
		.collations
		.fetching_from
		.replace((collator_id, maybe_candidate_hash));

	sender
		.send_message(NetworkBridgeTxMessage::SendRequests(
			vec![requests],
			IfDisconnected::ImmediateError,
		))
		.await;
	Ok(())
}

/// Networking message has been received.
#[overseer::contextbounds(CollatorProtocol, prefix = overseer)]
async fn process_incoming_peer_message<Context>(
	ctx: &mut Context,
	state: &mut State,
	origin: PeerId,
	msg: CollationProtocols<
		protocol_v1::CollatorProtocolMessage,
		protocol_v2::CollatorProtocolMessage,
	>,
) {
	use protocol_v1::CollatorProtocolMessage as V1;
	use protocol_v2::CollatorProtocolMessage as V2;
	use sp_runtime::traits::AppVerify;

	match msg {
		CollationProtocols::V1(V1::Declare(collator_id, para_id, signature)) |
		CollationProtocols::V2(V2::Declare(collator_id, para_id, signature)) => {
			if collator_peer_id(&state.peer_data, &collator_id).is_some() {
				modify_reputation(
					&mut state.reputation,
					ctx.sender(),
					origin,
					COST_UNEXPECTED_MESSAGE,
				)
				.await;
				return
			}

			let peer_data = match state.peer_data.get_mut(&origin) {
				Some(p) => p,
				None => {
					gum::debug!(
						target: LOG_TARGET,
						peer_id = ?origin,
						?para_id,
						"Unknown peer",
					);
					modify_reputation(
						&mut state.reputation,
						ctx.sender(),
						origin,
						COST_UNEXPECTED_MESSAGE,
					)
					.await;
					return
				},
			};

			if peer_data.is_collating() {
				gum::debug!(
					target: LOG_TARGET,
					peer_id = ?origin,
					?para_id,
					"Peer is already in the collating state",
				);
				modify_reputation(
					&mut state.reputation,
					ctx.sender(),
					origin,
					COST_UNEXPECTED_MESSAGE,
				)
				.await;
				return
			}

			if !signature.verify(&*protocol_v1::declare_signature_payload(&origin), &collator_id) {
				gum::debug!(
					target: LOG_TARGET,
					peer_id = ?origin,
					?para_id,
					"Signature verification failure",
				);
				modify_reputation(
					&mut state.reputation,
					ctx.sender(),
					origin,
					COST_INVALID_SIGNATURE,
				)
				.await;
				return
			}

			if state.current_assignments.contains_key(&para_id) {
				gum::debug!(
					target: LOG_TARGET,
					peer_id = ?origin,
					?collator_id,
					?para_id,
					"Declared as collator for current para",
				);

				peer_data.set_collating(collator_id, para_id);
			} else {
				gum::debug!(
					target: LOG_TARGET,
					peer_id = ?origin,
					?collator_id,
					?para_id,
					"Declared as collator for unneeded para. Current assignments: {:?}",
					&state.current_assignments
				);

				modify_reputation(
					&mut state.reputation,
					ctx.sender(),
					origin,
					COST_UNNEEDED_COLLATOR,
				)
				.await;
				gum::trace!(target: LOG_TARGET, "Disconnecting unneeded collator");
				disconnect_peer(ctx.sender(), origin).await;
			}
		},
		CollationProtocols::V1(V1::AdvertiseCollation(relay_parent)) =>
			if let Err(err) =
				handle_advertisement(ctx.sender(), state, relay_parent, origin, None).await
			{
				gum::debug!(
					target: LOG_TARGET,
					peer_id = ?origin,
					?relay_parent,
					error = ?err,
					"Rejected v1 advertisement",
				);

				if let Some(rep) = err.reputation_changes() {
					modify_reputation(&mut state.reputation, ctx.sender(), origin, rep).await;
				}
			},
		CollationProtocols::V2(V2::AdvertiseCollation {
			relay_parent,
			candidate_hash,
			parent_head_data_hash,
		}) => {
			if let Err(err) = handle_advertisement(
				ctx.sender(),
				state,
				relay_parent,
				origin,
				Some((candidate_hash, parent_head_data_hash)),
			)
			.await
			{
				gum::debug!(
					target: LOG_TARGET,
					peer_id = ?origin,
					?relay_parent,
					?candidate_hash,
					error = ?err,
					"Rejected v2 advertisement",
				);

				if let Some(rep) = err.reputation_changes() {
					modify_reputation(&mut state.reputation, ctx.sender(), origin, rep).await;
				}
			}
		},
		CollationProtocols::V1(V1::CollationSeconded(..)) |
		CollationProtocols::V2(V2::CollationSeconded(..)) => {
			gum::warn!(
				target: LOG_TARGET,
				peer_id = ?origin,
				"Unexpected `CollationSeconded` message, decreasing reputation",
			);

			modify_reputation(&mut state.reputation, ctx.sender(), origin, COST_UNEXPECTED_MESSAGE)
				.await;
		},
	}
}

#[derive(Debug)]
enum AdvertisementError {
	/// Relay parent is unknown.
	RelayParentUnknown,
	/// Peer is not present in the subsystem state.
	UnknownPeer,
	/// Peer has not declared its para id.
	UndeclaredCollator,
	/// We're assigned to a different para at the given relay parent.
	InvalidAssignment,
	/// Para reached a limit of seconded candidates for this relay parent.
	SecondedLimitReached,
	/// Collator trying to advertise a collation using V1 protocol for an async backing relay
	/// parent.
	ProtocolMisuse,
	/// Advertisement is invalid.
	#[allow(dead_code)]
	Invalid(InsertAdvertisementError),
	/// Seconding not allowed by backing subsystem
	BlockedByBacking,
}

impl AdvertisementError {
	fn reputation_changes(&self) -> Option<Rep> {
		use AdvertisementError::*;
		match self {
			InvalidAssignment => Some(COST_WRONG_PARA),
			ProtocolMisuse => Some(COST_PROTOCOL_MISUSE),
			RelayParentUnknown | UndeclaredCollator | Invalid(_) => Some(COST_UNEXPECTED_MESSAGE),
			UnknownPeer | SecondedLimitReached | BlockedByBacking => None,
		}
	}
}

// Requests backing to sanity check the advertisement.
async fn can_second<Sender>(
	sender: &mut Sender,
	candidate_para_id: ParaId,
	candidate_relay_parent: Hash,
	candidate_hash: CandidateHash,
	parent_head_data_hash: Hash,
) -> bool
where
	Sender: CollatorProtocolSenderTrait,
{
	let request = CanSecondRequest {
		candidate_para_id,
		candidate_relay_parent,
		candidate_hash,
		parent_head_data_hash,
	};
	let (tx, rx) = oneshot::channel();
	sender.send_message(CandidateBackingMessage::CanSecond(request, tx)).await;

	rx.await.unwrap_or_else(|err| {
		gum::warn!(
			target: LOG_TARGET,
			?err,
			?candidate_relay_parent,
			?candidate_para_id,
			?candidate_hash,
			"CanSecond-request responder was dropped",
		);

		false
	})
}

// Try seconding any collations which were waiting on the validation of their parent
#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
async fn second_unblocked_collations<Context>(
	ctx: &mut Context,
	state: &mut State,
	para_id: ParaId,
	head_data: HeadData,
	head_data_hash: Hash,
) {
	if let Some(unblocked_collations) = state
		.blocked_from_seconding
		.remove(&BlockedCollationId { para_id, parent_head_data_hash: head_data_hash })
	{
		if !unblocked_collations.is_empty() {
			gum::debug!(
				target: LOG_TARGET,
				"Candidate outputting head data with hash {} unblocked {} collations for seconding.",
				head_data_hash,
				unblocked_collations.len()
			);
		}

		for mut unblocked_collation in unblocked_collations {
			unblocked_collation.maybe_parent_head_data = Some(head_data.clone());
			let peer_id = unblocked_collation.collation_event.pending_collation.peer_id;
			let relay_parent = unblocked_collation.candidate_receipt.descriptor.relay_parent();

			if let Err(err) = kick_off_seconding(ctx, state, unblocked_collation).await {
				gum::warn!(
					target: LOG_TARGET,
					?relay_parent,
					?para_id,
					?peer_id,
					error = %err,
					"Seconding aborted due to an error",
				);

				if err.is_malicious() {
					// Report malicious peer.
					modify_reputation(
						&mut state.reputation,
						ctx.sender(),
						peer_id,
						COST_REPORT_BAD,
					)
					.await;
				}
			}
		}
	}
}

fn ensure_seconding_limit_is_respected(
	relay_parent: &Hash,
	para_id: ParaId,
	state: &State,
) -> std::result::Result<(), AdvertisementError> {
	let paths = state.implicit_view.paths_via_relay_parent(relay_parent);

	gum::trace!(
		target: LOG_TARGET,
		?relay_parent,
		?para_id,
		?paths,
		"Checking seconding limit",
	);

	let mut has_claim_at_some_path = false;
	for path in paths {
		let mut cq_state = ClaimQueueState::new();
		for ancestor in &path {
			let seconded_and_pending = state.seconded_and_pending_for_para(&ancestor, &para_id);
			cq_state.add_leaf(
				&ancestor,
				&state
					.per_relay_parent
					.get(ancestor)
					.ok_or(AdvertisementError::RelayParentUnknown)?
					.assignment
					.current,
			);
			for _ in 0..seconded_and_pending {
				cq_state.claim_at(ancestor, &para_id);
			}
		}

		if cq_state.can_claim_at(relay_parent, &para_id) {
			gum::trace!(
				target: LOG_TARGET,
				?relay_parent,
				?para_id,
				?path,
				"Seconding limit respected at path",
			);
			has_claim_at_some_path = true;
			break
		}
	}

	// If there is a place in the claim queue for the candidate at at least one path we will accept
	// it.
	if has_claim_at_some_path {
		Ok(())
	} else {
		Err(AdvertisementError::SecondedLimitReached)
	}
}

async fn handle_advertisement<Sender>(
	sender: &mut Sender,
	state: &mut State,
	relay_parent: Hash,
	peer_id: PeerId,
	prospective_candidate: Option<(CandidateHash, Hash)>,
) -> std::result::Result<(), AdvertisementError>
where
	Sender: CollatorProtocolSenderTrait,
{
	let peer_data = state.peer_data.get_mut(&peer_id).ok_or(AdvertisementError::UnknownPeer)?;

	if peer_data.version == CollationVersion::V1 && !state.active_leaves.contains(&relay_parent) {
		return Err(AdvertisementError::ProtocolMisuse)
	}

	let per_relay_parent = state
		.per_relay_parent
		.get(&relay_parent)
		.ok_or(AdvertisementError::RelayParentUnknown)?;

	let assignment = &per_relay_parent.assignment;

	let collator_para_id =
		peer_data.collating_para().ok_or(AdvertisementError::UndeclaredCollator)?;

	// Check if this is assigned to us.
	if !assignment.current.contains(&collator_para_id) {
		return Err(AdvertisementError::InvalidAssignment)
	}

	// Always insert advertisements that pass all the checks for spam protection.
	let candidate_hash = prospective_candidate.map(|(hash, ..)| hash);
	let (collator_id, para_id) = peer_data
		.insert_advertisement(
			relay_parent,
			candidate_hash,
			&state.implicit_view,
			&state.active_leaves,
			&per_relay_parent,
		)
		.map_err(AdvertisementError::Invalid)?;

	ensure_seconding_limit_is_respected(&relay_parent, para_id, state)?;

	if let Some((candidate_hash, parent_head_data_hash)) = prospective_candidate {
		// Check if backing subsystem allows to second this candidate.
		//
		// This is also only important when async backing or elastic scaling is enabled.
		let can_second = can_second(
			sender,
			collator_para_id,
			relay_parent,
			candidate_hash,
			parent_head_data_hash,
		)
		.await;

		if !can_second {
			return Err(AdvertisementError::BlockedByBacking)
		}
	}

	let result = enqueue_collation(
		sender,
		state,
		relay_parent,
		para_id,
		peer_id,
		collator_id,
		prospective_candidate,
	)
	.await;

	if let Err(fetch_error) = result {
		gum::debug!(
			target: LOG_TARGET,
			relay_parent = ?relay_parent,
			para_id = ?para_id,
			peer_id = ?peer_id,
			error = %fetch_error,
			"Failed to request advertised collation",
		);
	}

	Ok(())
}

/// Enqueue collation for fetching. The advertisement is expected to be validated and the seconding
/// limit checked.
async fn enqueue_collation<Sender>(
	sender: &mut Sender,
	state: &mut State,
	relay_parent: Hash,
	para_id: ParaId,
	peer_id: PeerId,
	collator_id: CollatorId,
	prospective_candidate: Option<(CandidateHash, Hash)>,
) -> std::result::Result<(), FetchError>
where
	Sender: CollatorProtocolSenderTrait,
{
	gum::debug!(
		target: LOG_TARGET,
		peer_id = ?peer_id,
		%para_id,
		?relay_parent,
		"Received advertise collation",
	);
	let per_relay_parent = match state.per_relay_parent.get_mut(&relay_parent) {
		Some(rp_state) => rp_state,
		None => {
			// Race happened, not an error.
			gum::trace!(
				target: LOG_TARGET,
				peer_id = ?peer_id,
				%para_id,
				?relay_parent,
				?prospective_candidate,
				"Candidate relay parent went out of view for valid advertisement",
			);
			return Ok(())
		},
	};
	let prospective_candidate =
		prospective_candidate.map(|(candidate_hash, parent_head_data_hash)| ProspectiveCandidate {
			candidate_hash,
			parent_head_data_hash,
		});

	let collations = &mut per_relay_parent.collations;
	let pending_collation =
		PendingCollation::new(relay_parent, para_id, &peer_id, prospective_candidate);

	match collations.status {
		CollationStatus::Fetching(_) | CollationStatus::WaitingOnValidation => {
			gum::trace!(
				target: LOG_TARGET,
				peer_id = ?peer_id,
				%para_id,
				?relay_parent,
				"Added collation to the pending list"
			);
			collations.add_to_waiting_queue((pending_collation, collator_id));
		},
		CollationStatus::Waiting => {
			// We were waiting for a collation to be advertised to us (we were idle) so we can fetch
			// the new collation immediately
			fetch_collation(sender, state, pending_collation, collator_id).await?;
		},
	}

	Ok(())
}

/// Our view has changed.
async fn handle_our_view_change<Sender>(
	sender: &mut Sender,
	state: &mut State,
	keystore: &KeystorePtr,
	view: OurView,
) -> Result<()>
where
	Sender: CollatorProtocolSenderTrait,
{
	let current_leaves = state.active_leaves.clone();

	let removed = current_leaves.iter().filter(|h| !view.contains(h));
	let added = view.iter().filter(|h| !current_leaves.contains(h));

	for leaf in added {
		let session_index = request_session_index_for_child(*leaf, sender)
			.await
			.await
			.map_err(Error::CancelledSessionIndex)??;

		let v2_receipts = request_node_features(*leaf, session_index, sender)
			.await
			.await
			.map_err(Error::CancelledNodeFeatures)??
			.get(node_features::FeatureIndex::CandidateReceiptV2 as usize)
			.map(|b| *b)
			.unwrap_or(false);

		let Some(per_relay_parent) = construct_per_relay_parent(
			sender,
			&mut state.current_assignments,
			keystore,
			*leaf,
			v2_receipts,
			session_index,
		)
		.await?
		else {
			continue
		};

		state.active_leaves.insert(*leaf);
		state.per_relay_parent.insert(*leaf, per_relay_parent);

		state
			.implicit_view
			.activate_leaf(sender, *leaf)
			.await
			.map_err(Error::ImplicitViewFetchError)?;

		// Order is always descending.
		let allowed_ancestry = state
			.implicit_view
			.known_allowed_relay_parents_under(leaf, None)
			.unwrap_or_default();
		for block_hash in allowed_ancestry {
			if let Entry::Vacant(entry) = state.per_relay_parent.entry(*block_hash) {
				// Safe to use the same v2 receipts config for the allowed relay parents as well
				// as the same session index since they must be in the same session.
				if let Some(per_relay_parent) = construct_per_relay_parent(
					sender,
					&mut state.current_assignments,
					keystore,
					*block_hash,
					v2_receipts,
					session_index,
				)
				.await?
				{
					entry.insert(per_relay_parent);
				}
			}
		}
	}

	for removed in removed {
		gum::trace!(
			target: LOG_TARGET,
			?view,
			?removed,
			"handle_our_view_change - removed",
		);

		state.active_leaves.remove(removed);
		// If the leaf is deactivated it still may stay in the view as a part
		// of implicit ancestry. Only update the state after the hash is actually
		// pruned from the block info storage.
		let pruned = state.implicit_view.deactivate_leaf(*removed);

		for removed in pruned {
			if let Some(per_relay_parent) = state.per_relay_parent.remove(&removed) {
				remove_outgoing(&mut state.current_assignments, per_relay_parent);
			}

			state.collation_requests_cancel_handles.retain(|pc, handle| {
				let keep = pc.relay_parent != removed;
				if !keep {
					handle.cancel();
				}
				keep
			});
			state.fetched_candidates.retain(|k, _| k.relay_parent != removed);
		}
	}

	// Remove blocked seconding requests that left the view.
	state.blocked_from_seconding.retain(|_, collations| {
		collations.retain(|collation| {
			state
				.per_relay_parent
				.contains_key(&collation.candidate_receipt.descriptor.relay_parent())
		});

		!collations.is_empty()
	});

	for (peer_id, peer_data) in state.peer_data.iter_mut() {
		peer_data.prune_old_advertisements(&state.implicit_view, &state.active_leaves);

		// Disconnect peers who are not relevant to our current or next para.
		//
		// If the peer hasn't declared yet, they will be disconnected if they do not
		// declare.
		if let Some(para_id) = peer_data.collating_para() {
			if !state.current_assignments.contains_key(&para_id) {
				gum::trace!(
					target: LOG_TARGET,
					?peer_id,
					?para_id,
					"Disconnecting peer on view change (not current parachain id)"
				);
				disconnect_peer(sender, *peer_id).await;
			}
		}
	}

	Ok(())
}

/// Bridge event switch.
#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
async fn handle_network_msg<Context>(
	ctx: &mut Context,
	state: &mut State,
	keystore: &KeystorePtr,
	bridge_message: NetworkBridgeEvent<net_protocol::CollatorProtocolMessage>,
) -> Result<()> {
	use NetworkBridgeEvent::*;

	match bridge_message {
		PeerConnected(peer_id, observed_role, protocol_version, _) => {
			let version = match protocol_version.try_into() {
				Ok(version) => version,
				Err(err) => {
					// Network bridge is expected to handle this.
					gum::error!(
						target: LOG_TARGET,
						?peer_id,
						?observed_role,
						?err,
						"Unsupported protocol version"
					);
					return Ok(())
				},
			};
			state.peer_data.entry(peer_id).or_insert_with(|| PeerData {
				view: View::default(),
				state: PeerState::Connected(Instant::now()),
				version,
			});
			state.metrics.note_collator_peer_count(state.peer_data.len());
		},
		PeerDisconnected(peer_id) => {
			state.peer_data.remove(&peer_id);
			state.metrics.note_collator_peer_count(state.peer_data.len());
		},
		NewGossipTopology { .. } => {
			// impossible!
		},
		PeerViewChange(peer_id, view) => {
			handle_peer_view_change(state, peer_id, view);
		},
		OurViewChange(view) => {
			handle_our_view_change(ctx.sender(), state, keystore, view).await?;
		},
		PeerMessage(remote, msg) => {
			process_incoming_peer_message(ctx, state, remote, msg).await;
		},
		UpdatedAuthorityIds { .. } => {
			// The validator side doesn't deal with `AuthorityDiscoveryId`s.
		},
	}

	Ok(())
}

/// The main message receiver switch.
#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
async fn process_msg<Context>(
	ctx: &mut Context,
	keystore: &KeystorePtr,
	msg: CollatorProtocolMessage,
	state: &mut State,
) {
	use CollatorProtocolMessage::*;

	let _timer = state.metrics.time_process_msg();

	match msg {
		CollateOn(id) => {
			gum::warn!(
				target: LOG_TARGET,
				para_id = %id,
				"CollateOn message is not expected on the validator side of the protocol",
			);
		},
		DistributeCollation { .. } => {
			gum::warn!(
				target: LOG_TARGET,
				"DistributeCollation message is not expected on the validator side of the protocol",
			);
		},
		NetworkBridgeUpdate(event) => {
			if let Err(e) = handle_network_msg(ctx, state, keystore, event).await {
				gum::warn!(
					target: LOG_TARGET,
					err = ?e,
					"Failed to handle incoming network message",
				);
			}
		},
		Seconded(parent, stmt) => {
			let receipt = match stmt.payload() {
				Statement::Seconded(receipt) => receipt,
				Statement::Valid(_) => {
					gum::warn!(
						target: LOG_TARGET,
						?stmt,
						relay_parent = %parent,
						"Seconded message received with a `Valid` statement",
					);
					return
				},
			};
			let output_head_data = receipt.commitments.head_data.clone();
			let output_head_data_hash = receipt.descriptor.para_head();
			let fetched_collation = FetchedCollation::from(&receipt.to_plain());
			if let Some(CollationEvent { collator_id, pending_collation, .. }) =
				state.fetched_candidates.remove(&fetched_collation)
			{
				let PendingCollation {
					relay_parent, peer_id, prospective_candidate, para_id, ..
				} = pending_collation;
				note_good_collation(
					&mut state.reputation,
					ctx.sender(),
					&state.peer_data,
					collator_id.clone(),
				)
				.await;
				if let Some(peer_data) = state.peer_data.get(&peer_id) {
					notify_collation_seconded(
						ctx.sender(),
						peer_id,
						peer_data.version,
						relay_parent,
						stmt,
					)
					.await;
				}

				if let Some(rp_state) = state.per_relay_parent.get_mut(&parent) {
					rp_state.collations.note_seconded(para_id);
				}

				// See if we've unblocked other collations for seconding.
				second_unblocked_collations(
					ctx,
					state,
					fetched_collation.para_id,
					output_head_data,
					output_head_data_hash,
				)
				.await;

				// If async backing is enabled, make an attempt to fetch next collation.
				let maybe_candidate_hash =
					prospective_candidate.as_ref().map(ProspectiveCandidate::candidate_hash);
				dequeue_next_collation_and_fetch(
					ctx,
					state,
					parent,
					(collator_id, maybe_candidate_hash),
				)
				.await;
			} else {
				gum::debug!(
					target: LOG_TARGET,
					relay_parent = ?parent,
					"Collation has been seconded, but the relay parent is deactivated",
				);
			}
		},
		Invalid(parent, candidate_receipt) => {
			// Remove collations which were blocked from seconding and had this candidate as parent.
			state.blocked_from_seconding.remove(&BlockedCollationId {
				para_id: candidate_receipt.descriptor.para_id(),
				parent_head_data_hash: candidate_receipt.descriptor.para_head(),
			});

			let fetched_collation = FetchedCollation::from(&candidate_receipt);
			let candidate_hash = fetched_collation.candidate_hash;
			let id = match state.fetched_candidates.entry(fetched_collation) {
				Entry::Occupied(entry)
					if entry.get().pending_collation.commitments_hash ==
						Some(candidate_receipt.commitments_hash) =>
					entry.remove().collator_id,
				Entry::Occupied(_) => {
					gum::error!(
						target: LOG_TARGET,
						relay_parent = ?parent,
						candidate = ?candidate_receipt.hash(),
						"Reported invalid candidate for unknown `pending_candidate`!",
					);
					return
				},
				Entry::Vacant(_) => return,
			};

			report_collator(&mut state.reputation, ctx.sender(), &state.peer_data, id.clone())
				.await;

			dequeue_next_collation_and_fetch(ctx, state, parent, (id, Some(candidate_hash))).await;
		},
	}
}

/// The main run loop.
#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
pub(crate) async fn run<Context>(
	ctx: Context,
	keystore: KeystorePtr,
	eviction_policy: crate::CollatorEvictionPolicy,
	metrics: Metrics,
) -> std::result::Result<(), std::convert::Infallible> {
	run_inner(
		ctx,
		keystore,
		eviction_policy,
		metrics,
		ReputationAggregator::default(),
		REPUTATION_CHANGE_INTERVAL,
	)
	.await
}

#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
async fn run_inner<Context>(
	mut ctx: Context,
	keystore: KeystorePtr,
	eviction_policy: crate::CollatorEvictionPolicy,
	metrics: Metrics,
	reputation: ReputationAggregator,
	reputation_interval: Duration,
) -> std::result::Result<(), std::convert::Infallible> {
	let new_reputation_delay = || futures_timer::Delay::new(reputation_interval).fuse();
	let mut reputation_delay = new_reputation_delay();

	let mut state = State { metrics, reputation, ..Default::default() };

	let next_inactivity_stream = tick_stream(ACTIVITY_POLL);
	futures::pin_mut!(next_inactivity_stream);

	let mut network_error_freq = gum::Freq::new();
	let mut canceled_freq = gum::Freq::new();

	loop {
		select! {
			_ = reputation_delay => {
				state.reputation.send(ctx.sender()).await;
				reputation_delay = new_reputation_delay();
			},
			res = ctx.recv().fuse() => {
				match res {
					Ok(FromOrchestra::Communication { msg }) => {
						gum::trace!(target: LOG_TARGET, msg = ?msg, "received a message");
						process_msg(
							&mut ctx,
							&keystore,
							msg,
							&mut state,
						).await;
					}
					Ok(FromOrchestra::Signal(OverseerSignal::Conclude)) | Err(_) => break,
					Ok(FromOrchestra::Signal(_)) => continue,
				}
			},
			_ = next_inactivity_stream.next() => {
				disconnect_inactive_peers(ctx.sender(), &eviction_policy, &state.peer_data).await;
			},
			resp = state.collation_requests.select_next_some() => {
				let relay_parent = resp.0.pending_collation.relay_parent;
				let res = match handle_collation_fetch_response(
					&mut state,
					resp,
					&mut network_error_freq,
					&mut canceled_freq,
				).await {
					Err(Some((peer_id, rep))) => {
						modify_reputation(&mut state.reputation, ctx.sender(), peer_id, rep).await;
						// Reset the status for the relay parent
						state.per_relay_parent.get_mut(&relay_parent).map(|rp| {
							rp.collations.status.back_to_waiting();
						});
						continue
					},
					Err(None) => {
						// Reset the status for the relay parent
						state.per_relay_parent.get_mut(&relay_parent).map(|rp| {
							rp.collations.status.back_to_waiting();
						});
						continue
					},
					Ok(res) => res
				};

				let CollationEvent {collator_id, pending_collation, .. } = res.collation_event.clone();

				match kick_off_seconding(&mut ctx, &mut state, res).await {
					Err(err) => {
						gum::warn!(
							target: LOG_TARGET,
							relay_parent = ?pending_collation.relay_parent,
							para_id = ?pending_collation.para_id,
							peer_id = ?pending_collation.peer_id,
							error = %err,
							"Seconding aborted due to an error",
						);

						if err.is_malicious() {
							// Report malicious peer.
							modify_reputation(&mut state.reputation, ctx.sender(), pending_collation.peer_id, COST_REPORT_BAD).await;
						}
						let maybe_candidate_hash =
						pending_collation.prospective_candidate.as_ref().map(ProspectiveCandidate::candidate_hash);
						dequeue_next_collation_and_fetch(
							&mut ctx,
							&mut state,
							pending_collation.relay_parent,
							(collator_id, maybe_candidate_hash),
						)
						.await;
					},
					Ok(false) => {
						// No hard error occurred, but we can try fetching another collation.
						let maybe_candidate_hash =
						pending_collation.prospective_candidate.as_ref().map(ProspectiveCandidate::candidate_hash);
						dequeue_next_collation_and_fetch(
							&mut ctx,
							&mut state,
							pending_collation.relay_parent,
							(collator_id, maybe_candidate_hash),
						)
						.await;
					}
					Ok(true) => {}
				}
			},
			res = state.collation_fetch_timeouts.select_next_some() => {
				let (collator_id, maybe_candidate_hash, relay_parent) = res;
				gum::debug!(
					target: LOG_TARGET,
					?relay_parent,
					?collator_id,
					"Timeout hit - already seconded?"
				);
				dequeue_next_collation_and_fetch(
					&mut ctx,
					&mut state,
					relay_parent,
					(collator_id, maybe_candidate_hash),
				)
				.await;
			}
		}
	}

	Ok(())
}

/// Dequeue another collation and fetch.
#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
async fn dequeue_next_collation_and_fetch<Context>(
	ctx: &mut Context,
	state: &mut State,
	relay_parent: Hash,
	// The collator we tried to fetch from last, optionally which candidate.
	previous_fetch: (CollatorId, Option<CandidateHash>),
) {
	while let Some((next, id)) = get_next_collation_to_fetch(&previous_fetch, relay_parent, state) {
		gum::debug!(
			target: LOG_TARGET,
			?relay_parent,
			?id,
			"Successfully dequeued next advertisement - fetching ..."
		);
		if let Err(err) = fetch_collation(ctx.sender(), state, next, id).await {
			gum::debug!(
				target: LOG_TARGET,
				relay_parent = ?next.relay_parent,
				para_id = ?next.para_id,
				peer_id = ?next.peer_id,
				error = %err,
				"Failed to request a collation, dequeueing next one",
			);
		} else {
			break
		}
	}
}

async fn request_persisted_validation_data<Sender>(
	sender: &mut Sender,
	relay_parent: Hash,
	para_id: ParaId,
) -> std::result::Result<Option<PersistedValidationData>, SecondingError>
where
	Sender: CollatorProtocolSenderTrait,
{
	// The core is guaranteed to be scheduled since we accepted the advertisement.
	polkadot_node_subsystem_util::request_persisted_validation_data(
		relay_parent,
		para_id,
		OccupiedCoreAssumption::Free,
		sender,
	)
	.await
	.await
	.map_err(SecondingError::CancelledRuntimePersistedValidationData)?
	.map_err(SecondingError::RuntimeApi)
}

async fn request_prospective_validation_data<Sender>(
	sender: &mut Sender,
	candidate_relay_parent: Hash,
	parent_head_data_hash: Hash,
	para_id: ParaId,
	maybe_parent_head_data: Option<HeadData>,
) -> std::result::Result<Option<PersistedValidationData>, SecondingError>
where
	Sender: CollatorProtocolSenderTrait,
{
	let (tx, rx) = oneshot::channel();

	let parent_head_data = if let Some(head_data) = maybe_parent_head_data {
		ParentHeadData::WithData { head_data, hash: parent_head_data_hash }
	} else {
		ParentHeadData::OnlyHash(parent_head_data_hash)
	};

	let request =
		ProspectiveValidationDataRequest { para_id, candidate_relay_parent, parent_head_data };

	sender
		.send_message(ProspectiveParachainsMessage::GetProspectiveValidationData(request, tx))
		.await;

	rx.await.map_err(SecondingError::CancelledProspectiveValidationData)
}

/// Handle a fetched collation result.
/// Returns whether or not seconding has begun.
#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
async fn kick_off_seconding<Context>(
	ctx: &mut Context,
	state: &mut State,
	PendingCollationFetch { mut collation_event, candidate_receipt, pov, maybe_parent_head_data }: PendingCollationFetch,
) -> std::result::Result<bool, SecondingError> {
	let pending_collation = collation_event.pending_collation;
	let relay_parent = pending_collation.relay_parent;

	let per_relay_parent = match state.per_relay_parent.get_mut(&relay_parent) {
		Some(state) => state,
		None => {
			// Relay parent went out of view, not an error.
			gum::trace!(
				target: LOG_TARGET,
				relay_parent = ?relay_parent,
				"Fetched collation for a parent out of view",
			);
			return Ok(false)
		},
	};

	// Sanity check of the candidate receipt version.
	descriptor_version_sanity_check(candidate_receipt.descriptor(), per_relay_parent)?;

	let collations = &mut per_relay_parent.collations;

	let fetched_collation = FetchedCollation::from(&candidate_receipt);
	if let Entry::Vacant(entry) = state.fetched_candidates.entry(fetched_collation) {
		collation_event.pending_collation.commitments_hash =
			Some(candidate_receipt.commitments_hash);

		let (maybe_pvd, maybe_parent_head, maybe_parent_head_hash) = match (
			collation_event.collator_protocol_version,
			collation_event.pending_collation.prospective_candidate,
		) {
			(CollationVersion::V2, Some(ProspectiveCandidate { parent_head_data_hash, .. })) => {
				let pvd = request_prospective_validation_data(
					ctx.sender(),
					relay_parent,
					parent_head_data_hash,
					pending_collation.para_id,
					maybe_parent_head_data.clone(),
				)
				.await?;

				(pvd, maybe_parent_head_data, Some(parent_head_data_hash))
			},
			(CollationVersion::V1, _) => {
				let pvd = request_persisted_validation_data(
					ctx.sender(),
					candidate_receipt.descriptor().relay_parent(),
					candidate_receipt.descriptor().para_id(),
				)
				.await?;
				(
					Some(pvd.ok_or(SecondingError::PersistedValidationDataNotFound)?),
					maybe_parent_head_data,
					None,
				)
			},
			_ => {
				// `handle_advertisement` checks for protocol mismatch.
				return Ok(false)
			},
		};

		let pvd = match (maybe_pvd, maybe_parent_head.clone(), maybe_parent_head_hash) {
			(Some(pvd), _, _) => pvd,
			(None, None, Some(parent_head_data_hash)) => {
				// In this case, the collator did not supply the head data and neither could
				// prospective-parachains. We add this to the blocked_from_seconding collection
				// until we second its parent.
				let blocked_collation = PendingCollationFetch {
					collation_event,
					candidate_receipt,
					pov,
					maybe_parent_head_data: None,
				};
				gum::debug!(
					target: LOG_TARGET,
					candidate_hash = ?blocked_collation.candidate_receipt.hash(),
					relay_parent = ?blocked_collation.candidate_receipt.descriptor.relay_parent(),
					"Collation having parent head data hash {} is blocked from seconding. Waiting on its parent to be validated.",
					parent_head_data_hash
				);
				state
					.blocked_from_seconding
					.entry(BlockedCollationId {
						para_id: blocked_collation.candidate_receipt.descriptor.para_id(),
						parent_head_data_hash,
					})
					.or_insert_with(Vec::new)
					.push(blocked_collation);

				return Ok(false)
			},
			(None, _, _) => {
				// Even though we already have the parent head data, the pvd fetching failed. We
				// don't need to wait for seconding another collation outputting this head data.
				return Err(SecondingError::PersistedValidationDataNotFound)
			},
		};

		fetched_collation_sanity_check(
			&collation_event.pending_collation,
			&candidate_receipt,
			&pvd,
			maybe_parent_head.and_then(|head| maybe_parent_head_hash.map(|hash| (head, hash))),
		)?;

		ctx.send_message(CandidateBackingMessage::Second(
			relay_parent,
			candidate_receipt,
			pvd,
			pov,
		))
		.await;
		// There's always a single collation being fetched at any moment of time.
		// In case of a failure, we reset the status back to waiting.
		collations.status = CollationStatus::WaitingOnValidation;

		entry.insert(collation_event);
		Ok(true)
	} else {
		Err(SecondingError::Duplicate)
	}
}

// This issues `NetworkBridge` notifications to disconnect from all inactive peers at the
// earliest possible point. This does not yet clean up any metadata, as that will be done upon
// receipt of the `PeerDisconnected` event.
async fn disconnect_inactive_peers(
	sender: &mut impl overseer::CollatorProtocolSenderTrait,
	eviction_policy: &crate::CollatorEvictionPolicy,
	peers: &HashMap<PeerId, PeerData>,
) {
	for (peer, peer_data) in peers {
		if peer_data.is_inactive(&eviction_policy) {
			gum::trace!(target: LOG_TARGET, ?peer, "Disconnecting inactive peer");
			disconnect_peer(sender, *peer).await;
		}
	}
}

/// Handle a collation fetch response.
async fn handle_collation_fetch_response(
	state: &mut State,
	response: <CollationFetchRequest as Future>::Output,
	network_error_freq: &mut gum::Freq,
	canceled_freq: &mut gum::Freq,
) -> std::result::Result<PendingCollationFetch, Option<(PeerId, Rep)>> {
	let (CollationEvent { collator_id, collator_protocol_version, pending_collation }, response) =
		response;
	// Remove the cancellation handle, as the future already completed.
	state.collation_requests_cancel_handles.remove(&pending_collation);

	let response = match response {
		Err(CollationFetchError::Cancelled) => {
			gum::debug!(
				target: LOG_TARGET,
				hash = ?pending_collation.relay_parent,
				para_id = ?pending_collation.para_id,
				peer_id = ?pending_collation.peer_id,
				"Request was cancelled from the validator side"
			);
			return Err(None)
		},
		Err(CollationFetchError::Request(req_error)) => Err(req_error),
		Ok(resp) => Ok(resp),
	};

	let _timer = state.metrics.time_handle_collation_request_result();

	let mut metrics_result = Err(());

	let result = match response {
		Err(RequestError::InvalidResponse(err)) => {
			gum::warn!(
				target: LOG_TARGET,
				hash = ?pending_collation.relay_parent,
				para_id = ?pending_collation.para_id,
				peer_id = ?pending_collation.peer_id,
				err = ?err,
				"Collator provided response that could not be decoded"
			);
			Err(Some((pending_collation.peer_id, COST_CORRUPTED_MESSAGE)))
		},
		Err(err) if err.is_timed_out() => {
			gum::debug!(
				target: LOG_TARGET,
				hash = ?pending_collation.relay_parent,
				para_id = ?pending_collation.para_id,
				peer_id = ?pending_collation.peer_id,
				"Request timed out"
			);
			// For now we don't want to change reputation on timeout, to mitigate issues like
			// this: https://github.com/paritytech/polkadot/issues/4617
			Err(None)
		},
		Err(RequestError::NetworkError(err)) => {
			gum::warn_if_frequent!(
				freq: network_error_freq,
				max_rate: gum::Times::PerHour(100),
				target: LOG_TARGET,
				hash = ?pending_collation.relay_parent,
				para_id = ?pending_collation.para_id,
				peer_id = ?pending_collation.peer_id,
				err = ?err,
				"Fetching collation failed due to network error"
			);
			// A minor decrease in reputation for any network failure seems
			// sensible. In theory this could be exploited, by DoSing this node,
			// which would result in reduced reputation for proper nodes, but the
			// same can happen for penalties on timeouts, which we also have.
			Err(Some((pending_collation.peer_id, COST_NETWORK_ERROR)))
		},
		Err(RequestError::Canceled(err)) => {
			gum::warn_if_frequent!(
				freq: canceled_freq,
				max_rate: gum::Times::PerHour(100),
				target: LOG_TARGET,
				hash = ?pending_collation.relay_parent,
				para_id = ?pending_collation.para_id,
				peer_id = ?pending_collation.peer_id,
				err = ?err,
				"Canceled should be handled by `is_timed_out` above - this is a bug!"
			);
			Err(None)
		},
		Ok(
			request_v1::CollationFetchingResponse::Collation(receipt, _) |
			request_v2::CollationFetchingResponse::Collation(receipt, _) |
			request_v1::CollationFetchingResponse::CollationWithParentHeadData { receipt, .. } |
			request_v2::CollationFetchingResponse::CollationWithParentHeadData { receipt, .. },
		) if receipt.descriptor().para_id() != pending_collation.para_id => {
			gum::debug!(
				target: LOG_TARGET,
				expected_para_id = ?pending_collation.para_id,
				got_para_id = ?receipt.descriptor().para_id(),
				peer_id = ?pending_collation.peer_id,
				"Got wrong para ID for requested collation."
			);

			Err(Some((pending_collation.peer_id, COST_WRONG_PARA)))
		},
		Ok(request_v1::CollationFetchingResponse::Collation(candidate_receipt, pov)) => {
			gum::debug!(
				target: LOG_TARGET,
				para_id = %pending_collation.para_id,
				hash = ?pending_collation.relay_parent,
				candidate_hash = ?candidate_receipt.hash(),
				"Received collation",
			);

			metrics_result = Ok(());
			Ok(PendingCollationFetch {
				collation_event: CollationEvent {
					collator_id,
					pending_collation,
					collator_protocol_version,
				},
				candidate_receipt,
				pov,
				maybe_parent_head_data: None,
			})
		},
		Ok(request_v2::CollationFetchingResponse::CollationWithParentHeadData {
			receipt,
			pov,
			parent_head_data,
		}) => {
			gum::debug!(
				target: LOG_TARGET,
				para_id = %pending_collation.para_id,
				hash = ?pending_collation.relay_parent,
				candidate_hash = ?receipt.hash(),
				"Received collation (v3)",
			);

			metrics_result = Ok(());
			Ok(PendingCollationFetch {
				collation_event: CollationEvent {
					collator_id,
					pending_collation,
					collator_protocol_version,
				},
				candidate_receipt: receipt,
				pov,
				maybe_parent_head_data: Some(parent_head_data),
			})
		},
	};
	state.metrics.on_request(metrics_result);
	result
}

// Returns the claim queue without fetched or pending advertisement. The resulting `Vec` keeps the
// order in the claim queue so the earlier an element is located in the `Vec` the higher its
// priority is.
fn unfulfilled_claim_queue_entries(relay_parent: &Hash, state: &State) -> Result<Vec<ParaId>> {
	let relay_parent_state = state
		.per_relay_parent
		.get(relay_parent)
		.ok_or(Error::RelayParentStateNotFound)?;
	let scheduled_paras = relay_parent_state.assignment.current.iter().collect::<HashSet<_>>();
	let paths = state.implicit_view.paths_via_relay_parent(relay_parent);

	let mut claim_queue_states = Vec::new();
	for path in paths {
		let mut cq_state = ClaimQueueState::new();
		for ancestor in &path {
			cq_state.add_leaf(
				&ancestor,
				&state
					.per_relay_parent
					.get(&ancestor)
					.ok_or(Error::RelayParentStateNotFound)?
					.assignment
					.current,
			);

			for para_id in &scheduled_paras {
				let seconded_and_pending = state.seconded_and_pending_for_para(&ancestor, &para_id);
				for _ in 0..seconded_and_pending {
					cq_state.claim_at(&ancestor, &para_id);
				}
			}
		}
		claim_queue_states.push(cq_state);
	}

	// From the claim queue state for each leaf we have to return a combined single one. Go for a
	// simple solution and return the longest one. In theory we always prefer the earliest entries
	// in the claim queue so there is a good chance that the longest path is the one with
	// unsatisfied entries in the beginning. This is not guaranteed as we might have fetched 2nd or
	// 3rd spot from the claim queue but it should be good enough.
	let unfulfilled_entries = claim_queue_states
		.iter_mut()
		.map(|cq| cq.unclaimed_at(relay_parent))
		.max_by(|a, b| a.len().cmp(&b.len()))
		.unwrap_or_default();

	Ok(unfulfilled_entries)
}

/// Returns the next collation to fetch from the `waiting_queue` and reset the status back to
/// `Waiting`.
fn get_next_collation_to_fetch(
	finished_one: &(CollatorId, Option<CandidateHash>),
	relay_parent: Hash,
	state: &mut State,
) -> Option<(PendingCollation, CollatorId)> {
	let unfulfilled_entries = match unfulfilled_claim_queue_entries(&relay_parent, &state) {
		Ok(entries) => entries,
		Err(err) => {
			gum::error!(
				target: LOG_TARGET,
				?relay_parent,
				?err,
				"Failed to get unfulfilled claim queue entries"
			);
			return None
		},
	};
	let rp_state = match state.per_relay_parent.get_mut(&relay_parent) {
		Some(rp_state) => rp_state,
		None => {
			gum::error!(
				target: LOG_TARGET,
				?relay_parent,
				"Failed to get relay parent state"
			);
			return None
		},
	};

	// If finished one does not match waiting_collation, then we already dequeued another fetch
	// to replace it.
	if let Some((collator_id, maybe_candidate_hash)) = rp_state.collations.fetching_from.as_ref() {
		// If a candidate hash was saved previously, `finished_one` must include this too.
		if collator_id != &finished_one.0 &&
			maybe_candidate_hash.map_or(true, |hash| Some(&hash) != finished_one.1.as_ref())
		{
			gum::trace!(
				target: LOG_TARGET,
				waiting_collation = ?rp_state.collations.fetching_from,
				?finished_one,
				"Not proceeding to the next collation - has already been done."
			);
			return None
		}
	}
	rp_state.collations.status.back_to_waiting();
	rp_state.collations.pick_a_collation_to_fetch(unfulfilled_entries)
}

// Sanity check the candidate descriptor version.
fn descriptor_version_sanity_check(
	descriptor: &CandidateDescriptorV2,
	per_relay_parent: &PerRelayParent,
) -> std::result::Result<(), SecondingError> {
	match descriptor.version() {
		CandidateDescriptorVersion::V1 => Ok(()),
		CandidateDescriptorVersion::V2 if per_relay_parent.v2_receipts => {
			if let Some(core_index) = descriptor.core_index() {
				if core_index != per_relay_parent.current_core {
					return Err(SecondingError::InvalidCoreIndex(
						core_index.0,
						per_relay_parent.current_core.0,
					))
				}
			}

			if let Some(session_index) = descriptor.session_index() {
				if session_index != per_relay_parent.session_index {
					return Err(SecondingError::InvalidSessionIndex(
						session_index,
						per_relay_parent.session_index,
					))
				}
			}

			Ok(())
		},
		descriptor_version => Err(SecondingError::InvalidReceiptVersion(descriptor_version)),
	}
}
