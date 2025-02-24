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
	channel::oneshot,
	future::BoxFuture,
	select,
	stream::{AbortHandle, Abortable, FuturesUnordered},
	task::Poll,
	FutureExt, StreamExt,
};
use std::{
	collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
	future::Future,
	pin::Pin,
	time::Duration,
};
use tokio_util::sync::CancellationToken;

use sp_keystore::KeystorePtr;

use polkadot_node_network_protocol::{
	self as net_protocol,
	peer_set::{CollationVersion, PeerSet, ProtocolVersion},
	request_response::{
		outgoing::{Recipient, RequestError},
		v1 as request_v1, v2 as request_v2, OutgoingRequest, OutgoingResult, Requests,
	},
	v1 as protocol_v1, v2 as protocol_v2, OurView, PeerId, UnifiedReputationChange as Rep,
	Versioned, View,
};
use polkadot_node_primitives::{PoV, SignedFullStatement, Statement};
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
	request_claim_queue, request_session_index_for_child,
	runtime::request_node_features,
};
use polkadot_primitives::{
	node_features,
	vstaging::{
		CandidateDescriptorV2, CandidateDescriptorVersion, CandidateReceiptV2 as CandidateReceipt,
		CommittedCandidateReceiptV2,
	},
	CandidateHash, CollatorId, CoreIndex, Hash, HeadData, Id as ParaId, OccupiedCoreAssumption,
	PersistedValidationData, SessionIndex,
};

use crate::error::{Error, FetchError, Result, SecondingError};

use super::{modify_reputation, tick_stream, LOG_TARGET};

mod claim_queue_state;

#[cfg(test)]
mod tests;

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

type Score = u8;

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

/// Candidate supplied with a para head it's built on top of.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub struct ProspectiveCandidate {
	/// Candidate hash.
	pub candidate_hash: CandidateHash,
	/// Parent head-data hash as supplied in advertisement.
	pub parent_head_data_hash: Hash,
}

#[derive(Default)]
struct PerParaId {
	limit: usize,
	// TODO: Probably implement the priority queue using a min-heap
	scores: HashMap<PeerId, Score>,
}

impl PerParaId {
	fn new(limit: usize) -> Self {
		Self { limit, scores: Default::default() }
	}

	fn try_add(&mut self, peer_id: PeerId, reputation: Score) -> (bool, Option<PeerId>) {
		// If we've got enough room, add it. Otherwise, see if it has a higher reputation than any
		// other connected peer.
		if self.scores.len() < self.limit {
			self.scores.insert(peer_id, reputation);
			(true, None)
		} else {
			let Some(min_score) = self.min_score() else {
				unreachable!();
			};

			if min_score >= reputation {
				(false, None)
			} else {
				self.scores.insert(peer_id, reputation);
				(true, self.pop_min_score().map(|x| x.0))
			}
		}
	}

	fn min_score(&self) -> Option<Score> {
		// TODO
		None
	}

	fn pop_min_score(&mut self) -> Option<(PeerId, Score)> {
		// TODO
		None
	}
}

enum PeerState {
	/// Connected.
	Connected,
	/// Peer has declared.
	Collating(ParaId),
}

#[derive(Default)]
struct ConnectedPeers {
	per_paraid: BTreeMap<ParaId, PerParaId>,
	peers: HashMap<PeerId, PeerState>,
}

impl ConnectedPeers {
	fn new() -> Self {
		Self { per_paraid: Default::default(), peers: Default::default() }
	}

	fn contains(&self, peer_id: &PeerId) -> bool {
		self.peers.contains_key(peer_id)
	}

	fn disconnect(&mut self, mut peers_to_disconnect: Vec<PeerId>) -> DisconnectedPeers {
		peers_to_disconnect.retain(|peer| !self.contains(&peer));
		for peer in peers_to_disconnect.iter() {
			self.peers.remove(peer);
		}

		peers_to_disconnect
		// TODO: send disconnect messages
	}

	fn disconnected(&mut self, peer_id: PeerId) {
		for per_para_id in self.per_paraid.values_mut() {
			per_para_id.scores.remove(&peer_id);
		}

		self.peers.remove(&peer_id);
	}
}

#[derive(Default)]
struct ReputationDb {}

impl ReputationDb {
	fn query(&self, peer_id: &PeerId, para_id: &ParaId) -> Option<Score> {
		None
	}
	fn modify_reputation(&self, peer_id: PeerId, para_id: ParaId, score: Score) {}
	fn flush() {}
	fn new_leaves(&mut self, leaves: Vec<Hash>) -> Vec<ReputationBump> {
		// Here we need to process approved peer events and modify the DB entries.
		// Additionally, return the rep bumps we've made.
		vec![]
	}
}

const SCHEDULING_LOOKAHEAD: u32 = 3;
const CONNECTED_PEERS_LIMIT: usize = 300;

type DisconnectedPeers = Vec<PeerId>;
#[derive(Default)]
struct PeerManager {
	// TODO: does it make more sense to add reputationdb here?
	connected_peers: ConnectedPeers,
}

impl PeerManager {
	fn scheduled_paras_update(
		&mut self,
		reputation_db: &ReputationDb,
		scheduled_paras: BTreeSet<ParaId>,
	) -> DisconnectedPeers {
		let old_scheduled_paras =
			self.connected_peers.per_paraid.keys().copied().collect::<BTreeSet<_>>();
		if old_scheduled_paras == scheduled_paras {
			// Nothing to do if the scheduled paras didn't change.
			return vec![]
		}

		let mut connected_peers = ConnectedPeers::new();
		let n_scheduled_paras = scheduled_paras.len();
		for para_id in scheduled_paras {
			connected_peers.per_paraid.insert(
				para_id,
				PerParaId {
					// TODO: it makes sense to limit to a maximum if there's only one or NO paras.
					limit: CONNECTED_PEERS_LIMIT / n_scheduled_paras,
					scores: HashMap::new(),
				},
			);
		}

		std::mem::swap(&mut connected_peers, &mut self.connected_peers);
		let old_connected_peers = connected_peers;

		let mut peers_to_disconnect = vec![];
		// See which of the old peers we should keep.
		// TODO: should we have them sorted or shuffle them at this point?
		for peer_id in old_connected_peers.peers.keys() {
			peers_to_disconnect.extend(self.try_accept_inner(reputation_db, *peer_id).into_iter());
			if !self.connected_peers.peers.contains_key(peer_id) {
				peers_to_disconnect.push(*peer_id);
			}
		}

		self.connected_peers.disconnect(peers_to_disconnect)
	}

	fn try_accept(&mut self, reputation_db: &ReputationDb, peer_id: PeerId) -> DisconnectedPeers {
		let peers_to_disconnect = self.try_accept_inner(reputation_db, peer_id);
		self.connected_peers.disconnect(peers_to_disconnect)
	}

	fn try_accept_inner(&mut self, reputation_db: &ReputationDb, peer_id: PeerId) -> Vec<PeerId> {
		if self.connected_peers.contains(&peer_id) {
			// cannot happen, must be already connected
			return vec![]
		}

		let mut kept = false;
		let mut peers_to_disconnect = vec![];
		for (para_id, per_para_id) in self.connected_peers.per_paraid.iter_mut() {
			let past_reputation = reputation_db.query(&peer_id, para_id).unwrap_or(0);
			let res = per_para_id.try_add(peer_id, past_reputation);
			if res.0 {
				kept = true;

				if let Some(to_disconnect) = res.1 {
					peers_to_disconnect.push(to_disconnect);
				}
			}
		}

		if !kept {
			peers_to_disconnect.push(peer_id);
		} else {
			self.connected_peers.peers.insert(peer_id, PeerState::Connected);
		}

		peers_to_disconnect
	}

	fn declared(&mut self, peer_id: PeerId, para_id: ParaId) {
		let Some(state) = self.connected_peers.peers.get_mut(&peer_id) else { return };

		let mut kept = false;

		match state {
			PeerState::Connected => {
				for (para, per_para_id) in self.connected_peers.per_paraid.iter_mut() {
					if para != &para_id {
						per_para_id.scores.remove(&peer_id);
					} else {
						kept = true;
					}
				}
			},
			PeerState::Collating(old_para_id) if old_para_id == &para_id => {
				// Nothing to do.
			},
			PeerState::Collating(old_para_id) => {
				if let Some(old_per_paraid) = self.connected_peers.per_paraid.get_mut(&old_para_id)
				{
					old_per_paraid.scores.remove(&peer_id);
				}
				if let Some(per_para_id) = self.connected_peers.per_paraid.get(&para_id) {
					if per_para_id.scores.contains_key(&peer_id) {
						kept = true;
					}
				}
			},
		}

		if !kept {
			self.connected_peers.disconnect(vec![peer_id]);
		} else {
			*state = PeerState::Collating(para_id);
		}
	}

	fn process_bumps(&mut self, rep_bumps: Vec<ReputationBump>) {
		for bump in rep_bumps {
			let Some(per_para) = self.connected_peers.per_paraid.get_mut(&bump.para_id) else {
				continue
			};
			let Some(score) = per_para.scores.get_mut(&bump.peer_id) else { continue };

			*score = score.saturating_add(bump.value);
		}
	}

	fn process_decrease(&mut self, rep_bumps: Vec<ReputationBump>) {
		for bump in rep_bumps {
			let Some(per_para) = self.connected_peers.per_paraid.get_mut(&bump.para_id) else {
				continue
			};
			let Some(score) = per_para.scores.get_mut(&bump.peer_id) else { continue };
			*score = score.saturating_sub(bump.value);
		}
	}

	fn handle_disconnected(&mut self, peer_id: PeerId) {
		self.connected_peers.disconnected(peer_id);
	}
}

// One per active leaf.
struct ClaimQueueState {
	leaf: Hash,
	claim_queue: Vec<Claim>,
}

impl ClaimQueueState {
	fn removed_relay_parents(&mut self, removed_rps: &Vec<Hash>) {
		for claim in self.claim_queue.iter_mut() {
			if let Some(rp) = claim.state.relay_parent() {
				if removed_rps.contains(&rp) {
					claim.state = ClaimState::Waiting;
				}
			}
		}
	}
}

struct Claim {
	para: ParaId,
	state: ClaimState,
}

enum ClaimState {
	Waiting,
	Fetching(Advertisement),
	Validating(Advertisement),
	BlockedByParent(Advertisement),
	Fulfilled(Advertisement),
}

impl ClaimState {
	fn relay_parent(&self) -> Option<Hash> {
		match self {
			Self::Waiting => None,
			Self::Fetching(a) |
			Self::Validating(a) |
			Self::Fulfilled(a) |
			Self::BlockedByParent(a) => Some(a.relay_parent),
		}
	}
}

/// Identifier of a collation being requested.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
struct Advertisement {
	/// Candidate's relay parent.
	relay_parent: Hash,
	/// Parachain id.
	para_id: ParaId,
	/// Peer that advertised this collation.
	peer_id: PeerId,
	/// Optional candidate hash and parent head-data hash if were
	/// supplied in advertisement.
	/// TODO: this needs to be optional
	prospective_candidate: ProspectiveCandidate,
}

// Any error that can occur when awaiting a collation fetch response.
#[derive(Debug, thiserror::Error)]
pub(super) enum CollationFetchError {
	#[error("Future was cancelled.")]
	Cancelled,
	#[error("{0}")]
	Request(#[from] RequestError),
}

/// Future that concludes when the collator has responded to our collation fetch request
/// or the request was cancelled by the validator.
pub(super) struct CollationFetchRequest {
	/// Info about the requested collation.
	pub advertisement: Advertisement,
	/// Responses from collator.
	pub from_collator: BoxFuture<'static, OutgoingResult<request_v2::CollationFetchingResponse>>,
	/// Handle used for checking if this request was cancelled.
	pub cancellation_token: CancellationToken,
}

impl Future for CollationFetchRequest {
	type Output = (
		Advertisement,
		std::result::Result<request_v2::CollationFetchingResponse, CollationFetchError>,
	);

	fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
		// First check if this fetch request was cancelled.
		let cancelled = match std::pin::pin!(self.cancellation_token.cancelled()).poll(cx) {
			Poll::Ready(()) => true,
			Poll::Pending => false,
		};

		if cancelled {
			return Poll::Ready((self.advertisement.clone(), Err(CollationFetchError::Cancelled)))
		}

		let res = self
			.from_collator
			.poll_unpin(cx)
			.map(|res| (self.advertisement.clone(), res.map_err(CollationFetchError::Request)));

		res
	}
}

#[derive(Default)]
struct PendingRequests {
	futures: FuturesUnordered<CollationFetchRequest>,
	cancellation_tokens: HashMap<CandidateHash, CancellationToken>,
}

impl PendingRequests {
	fn contains(&self, candidate_hash: &CandidateHash) -> bool {
		self.cancellation_tokens.contains_key(candidate_hash)
	}

	fn launch(
		&mut self,
		advertisement: &Advertisement,
	) -> OutgoingRequest<request_v2::CollationFetchingRequest> {
		let cancellation_token = CancellationToken::new();
		let (req, response_recv) = OutgoingRequest::new(
			Recipient::Peer(advertisement.peer_id),
			request_v2::CollationFetchingRequest {
				relay_parent: advertisement.relay_parent,
				para_id: advertisement.para_id,
				candidate_hash: advertisement.prospective_candidate.candidate_hash,
			},
		);

		self.futures.push(CollationFetchRequest {
			advertisement: *advertisement,
			from_collator: response_recv.boxed(),
			cancellation_token: cancellation_token.clone(),
		});

		self.cancellation_tokens
			.insert(advertisement.prospective_candidate.candidate_hash, cancellation_token);

		req
	}

	fn cancel(&mut self, candidate_hash: &CandidateHash) {
		if let Some(cancellation_token) = self.cancellation_tokens.remove(candidate_hash) {
			cancellation_token.cancel();
		}
	}

	fn completed(&mut self, candidate_hash: &CandidateHash) {
		self.cancellation_tokens.remove(candidate_hash);
	}
}

#[derive(Default)]
struct Collations {
	advertisements: HashMap<PeerId, HashSet<Advertisement>>,
	// Only kept to make sure that we don't re-request the same collations.
	// TODO: rename to fetched collations
	collations: HashSet<CandidateHash>,
}

impl Collations {}

struct ReputationBump {
	peer_id: PeerId,
	para_id: ParaId,
	value: Score,
}

enum CollationFetchOutcome {
	TryNew(Option<Score>),
	Success(FetchedCollation),
}

/// Fetched collation data.
#[derive(Debug, Clone)]
pub struct FetchedCollation {
	/// Candidate receipt.
	pub candidate_receipt: CandidateReceipt,
	/// Proof of validity. Wrap it in an Arc to avoid expensive copying
	pub pov: PoV,
	/// Optional parachain parent head data.
	/// Only needed for elastic scaling.
	pub maybe_parent_head_data: Option<HeadData>,
	pub parent_head_data_hash: Hash,
}

#[derive(Default)]
struct CollationManager {
	implicit_view: ImplicitView,
	// One per active leaf
	claim_queue_state: HashMap<Hash, ClaimQueueState>,

	// One per relay parent
	collations: HashMap<Hash, Collations>,

	fetching: PendingRequests,

	core: CoreIndex,
}

impl CollationManager {
	async fn view_update<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		keystore: &KeystorePtr,
		new_view: OurView,
	) -> Vec<Hash> {
		let removed = self
			.implicit_view
			.leaves()
			.filter(|h| !new_view.contains(h))
			.cloned()
			.collect::<Vec<_>>();
		let added = new_view
			.iter()
			.filter(|h| !self.implicit_view.contains_leaf(h))
			.cloned()
			.collect::<Vec<_>>();

		for leaf in removed {
			let mut deactivated_ancestry = self.implicit_view.deactivate_leaf(leaf);

			deactivated_ancestry.push(leaf);
			for deactivated in deactivated_ancestry.iter() {
				self.collations.remove(deactivated);
			}

			for claim_queue_state in self.claim_queue_state.values_mut() {
				claim_queue_state.removed_relay_parents(&deactivated_ancestry);
			}
		}

		for leaf in added.iter() {
			self.implicit_view.activate_leaf(sender, *leaf).await.unwrap();

			// TODO: cache them per session, as well as groups info. We can augment the cached
			// rotation info with the block number easily.
			let validators = polkadot_node_subsystem_util::request_validators(*leaf, sender)
				.await
				.await
				.unwrap()
				.unwrap();

			let (groups, rotation_info) =
				polkadot_node_subsystem_util::request_validator_groups(*leaf, sender)
					.await
					.await
					.unwrap()
					.unwrap();

			let core_now = if let Some(group) =
				polkadot_node_subsystem_util::signing_key_and_index(&validators, keystore).and_then(
					|(_, index)| polkadot_node_subsystem_util::find_validator_group(&groups, index),
				) {
				rotation_info.core_for_group(group, groups.len())
			} else {
				gum::trace!(target: LOG_TARGET, ?leaf, "Not a validator");
				return vec![];
			};

			let mut claim_queue = request_claim_queue(*leaf, sender).await.await.unwrap().unwrap();
			let scheduled =
				claim_queue.remove(&core_now).unwrap_or_else(|| VecDeque::new()).into_iter();

			if core_now != self.core {
				// We rotated to a different core, we can purge everything.
				self.core = core_now;
				self.claim_queue_state.clear();
				self.collations.clear();
			}

			let allowed_ancestry =
				self.implicit_view.known_allowed_relay_parents_under(leaf, None).unwrap();

			self.collations.insert(*leaf, Collations::default());

			let parent = allowed_ancestry.get(1).cloned();
			self.init_claim_queue_state(*leaf, scheduled.collect(), parent);
		}

		added
	}

	fn assignments(&self) -> BTreeSet<ParaId> {
		let mut scheduled_paras = BTreeSet::new();

		for state in self.claim_queue_state.values() {
			scheduled_paras.extend(state.claim_queue.iter().map(|c| c.para));
		}

		scheduled_paras
	}

	fn init_claim_queue_state(&mut self, leaf: Hash, cq: Vec<ParaId>, parent: Option<Hash>) {
		let mut cq_state = ClaimQueueState {
			leaf,
			claim_queue: cq
				.into_iter()
				.map(|para| Claim { para, state: ClaimState::Waiting })
				.collect(),
		};

		if let Some(parent) = parent {
			if let Some(parent_state) = self.claim_queue_state.remove(&parent) {
				// We assume the claim queue always progresses by one (one candidate gets backed on
				// the core). Technically, the claim queue can also stagnate, if the previous
				// candidate is timed out during availability distribution. But it's not a case
				// worth optimising for.
				if parent_state.claim_queue.len() >= 2 {
					let mut parent_state_iter = parent_state.claim_queue.into_iter();
					// Skip the first item.
					parent_state_iter.next();
					for claim_state in cq_state.claim_queue.iter_mut() {
						let Some(parent_claim_state) = parent_state_iter.next() else { break };
						if parent_claim_state.para != claim_state.para {
							break
						}
						if let Some(rp) = parent_claim_state.state.relay_parent() {
							// Check if the RP is still in scope. If it is, inherit the state.
							if self.collations.contains_key(&rp) {
								*claim_state = parent_claim_state;
							}
						}
					}
				}
			}
		}

		self.claim_queue_state.insert(leaf, cq_state);
	}

	async fn try_launch_fetch_requests<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		peer_manager: &PeerManager,
	) {
		let threshold = 30;
		// Advertisements and collations are up to date.
		// Claim queue states for leaves are also up to date.
		// Launch requests when it makes sense.
		let mut requests = vec![];
		for state in self.claim_queue_state.values_mut() {
			for claim in state.claim_queue.iter_mut() {
				if matches!(claim.state, ClaimState::Waiting) {
					// Try picking an advertisement. I'd like this to be a separate method but
					// compiler gets confused with ownership.
					let mut over_threshold = None;
					let mut has_some_advertisements = false;
					'per_rp: for collations in self.collations.values() {
						for (peer_id, advertisements) in collations.advertisements.iter() {
							for advertisement in advertisements {
								if advertisement.para_id == claim.para &&
									!self.fetching.contains(
										&advertisement.prospective_candidate.candidate_hash,
									) && !collations
									.collations
									.contains(&advertisement.prospective_candidate.candidate_hash)
								{
									// TODO: add a method for this.
									let peer_rep = peer_manager
										.connected_peers
										.per_paraid
										.get(&claim.para)
										.unwrap()
										.scores
										.get(peer_id)
										.unwrap();
									has_some_advertisements = true;

									if *peer_rep >= threshold {
										over_threshold = Some(*advertisement);
										break 'per_rp;
									}
								}
							}
						}
					}

					if let Some(advertisement) = over_threshold {
						let req = self.fetching.launch(&advertisement);
						requests.push(Requests::CollationFetchingV2(req));
						claim.state = ClaimState::Fetching(advertisement);
					} else if has_some_advertisements {
						// TODO: we need to arm some timer.
					}
				}
			}
		}

		if !requests.is_empty() {
			sender
				.send_message(NetworkBridgeTxMessage::SendRequests(
					requests,
					IfDisconnected::ImmediateError,
				))
				.await;
		}
	}

	fn remove_peers(&mut self, peers_to_remove: Vec<PeerId>) {
		if peers_to_remove.is_empty() {
			return
		}

		let peers_to_remove = peers_to_remove.into_iter().collect::<HashSet<_>>();

		for collations in self.collations.values_mut() {
			collations.advertisements.retain(|peer, _| !peers_to_remove.contains(peer));
		}
	}

	async fn try_accept_advertisement<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		advertisement: Advertisement,
	) -> std::result::Result<(), AdvertisementError> {
		let Some(collations) = self.collations.get_mut(&advertisement.relay_parent) else {
			return Err(AdvertisementError::Invalid(InsertAdvertisementError::OutOfOurView))
		};

		let advertisements = collations
			.advertisements
			.entry(advertisement.peer_id)
			.or_insert_with(|| Default::default());
		// TODO: use claim queue len for the specific para

		// Check if backing subsystem allows to second this candidate.
		//
		// This is also only important when async backing or elastic scaling is enabled.
		let can_second = can_second(sender, &advertisement).await;

		if !can_second {
			return Err(AdvertisementError::BlockedByBacking)
		}

		if advertisements.len() >= 3 {
			return Err(AdvertisementError::Invalid(InsertAdvertisementError::PeerLimitReached))
		}

		if advertisements.contains(&advertisement) {
			return Err(AdvertisementError::Invalid(InsertAdvertisementError::Duplicate))
		}

		advertisements.insert(advertisement);
		Ok(())
	}

	fn completed_fetch(
		&mut self,
		res: <CollationFetchRequest as Future>::Output,
	) -> CollationFetchOutcome {
		let (advertisement, res) = res;
		self.fetching.completed(&advertisement.prospective_candidate.candidate_hash);

		let collations = self.collations.entry(advertisement.relay_parent).or_default();
		if let Some(advertisements) = collations.advertisements.get_mut(&advertisement.peer_id) {
			advertisements.remove(&advertisement);
		}

		let outcome = match res {
			Err(CollationFetchError::Cancelled) => {
				// Was cancelled by the subsystem.
				CollationFetchOutcome::TryNew(Some(0))
			},
			Err(CollationFetchError::Request(RequestError::InvalidResponse(err))) => {
				gum::warn!(
					target: LOG_TARGET,
					hash = ?advertisement.relay_parent,
					para_id = ?advertisement.para_id,
					peer_id = ?advertisement.peer_id,
					err = ?err,
					"Collator provided response that could not be decoded"
				);
				CollationFetchOutcome::TryNew(Some(10))
			},
			Err(CollationFetchError::Request(err)) if err.is_timed_out() => {
				gum::debug!(
					target: LOG_TARGET,
					hash = ?advertisement.relay_parent,
					para_id = ?advertisement.para_id,
					peer_id = ?advertisement.peer_id,
					"Request timed out"
				);
				// For now we don't want to change reputation on timeout, to mitigate issues like
				// this: https://github.com/paritytech/polkadot/issues/4617
				CollationFetchOutcome::TryNew(Some(10))
			},
			Err(CollationFetchError::Request(RequestError::NetworkError(err))) => {
				gum::warn!(
					target: LOG_TARGET,
					hash = ?advertisement.relay_parent,
					para_id = ?advertisement.para_id,
					peer_id = ?advertisement.peer_id,
					err = ?err,
					"Fetching collation failed due to network error"
				);
				// A minor decrease in reputation for any network failure seems
				// sensible. In theory this could be exploited, by DoSing this node,
				// which would result in reduced reputation for proper nodes, but the
				// same can happen for penalties on timeouts, which we also have.
				CollationFetchOutcome::TryNew(Some(10))
			},
			Err(CollationFetchError::Request(RequestError::Canceled(err))) => {
				gum::warn!(
					target: LOG_TARGET,
					hash = ?advertisement.relay_parent,
					para_id = ?advertisement.para_id,
					peer_id = ?advertisement.peer_id,
					err = ?err,
					"Canceled should be handled by `is_timed_out` above - this is a bug!"
				);
				CollationFetchOutcome::TryNew(Some(10))
			},
			Ok(request_v2::CollationFetchingResponse::Collation(candidate_receipt, pov)) => {
				gum::debug!(
					target: LOG_TARGET,
					para_id = %advertisement.para_id,
					hash = ?advertisement.relay_parent,
					candidate_hash = ?candidate_receipt.hash(),
					"Received collation",
				);

				CollationFetchOutcome::Success(FetchedCollation {
					candidate_receipt,
					pov,
					maybe_parent_head_data: None,
					parent_head_data_hash: advertisement
						.prospective_candidate
						.parent_head_data_hash,
				})
			},
			Ok(request_v2::CollationFetchingResponse::CollationWithParentHeadData {
				receipt,
				pov,
				parent_head_data,
			}) => {
				gum::debug!(
					target: LOG_TARGET,
					para_id = %advertisement.para_id,
					hash = ?advertisement.relay_parent,
					candidate_hash = ?receipt.hash(),
					"Received collation (v3)",
				);

				CollationFetchOutcome::Success(FetchedCollation {
					candidate_receipt: receipt,
					pov,
					maybe_parent_head_data: Some(parent_head_data),
					parent_head_data_hash: advertisement
						.prospective_candidate
						.parent_head_data_hash,
				})
			},
		};

		match outcome {
			CollationFetchOutcome::Success(fetched_collation) => {
				if let Err(err) = initial_fetched_collation_sanity_check(
					&advertisement,
					&fetched_collation.candidate_receipt,
				) {
					return CollationFetchOutcome::TryNew(Some(10))
				}

				// It can't be a duplicate, because we check before initiating fetch. TODO: with the
				// old protocol version, it can be.
				collations.collations.insert(advertisement.prospective_candidate.candidate_hash);

				CollationFetchOutcome::Success(fetched_collation)
			},
			CollationFetchOutcome::TryNew(rep_change) => CollationFetchOutcome::TryNew(rep_change),
		}
	}

	fn seconding_began(&mut self, relay_parent: Hash, candidate_hash: CandidateHash) {
		for claim_queue_state in self.claim_queue_state.values_mut() {
			for claim in claim_queue_state.claim_queue.iter_mut() {
				if let ClaimState::Fetching(advertisement) = claim.state {
					if relay_parent == advertisement.relay_parent &&
						candidate_hash == advertisement.prospective_candidate.candidate_hash
					{
						claim.state = ClaimState::Validating(advertisement);
						return
					}
				}
			}
		}

		// TODO: log smth
	}

	// TODO: we can deduplicate common code with seconding_began and seconded
	fn back_to_waiting(
		&mut self,
		relay_parent: Hash,
		candidate_hash: CandidateHash,
	) -> Option<PeerId> {
		for claim_queue_state in self.claim_queue_state.values_mut() {
			for claim in claim_queue_state.claim_queue.iter_mut() {
				if let ClaimState::Fetching(advertisement) = claim.state {
					if relay_parent == advertisement.relay_parent &&
						candidate_hash == advertisement.prospective_candidate.candidate_hash
					{
						claim.state = ClaimState::Waiting;
						return Some(advertisement.peer_id)
					}
				}
			}
		}

		// TODO: log smth
		None
	}

	fn seconded(&mut self, relay_parent: Hash, candidate_hash: CandidateHash) -> Option<PeerId> {
		for claim_queue_state in self.claim_queue_state.values_mut() {
			for claim in claim_queue_state.claim_queue.iter_mut() {
				if let ClaimState::Validating(advertisement) = claim.state {
					if relay_parent == advertisement.relay_parent &&
						candidate_hash == advertisement.prospective_candidate.candidate_hash
					{
						claim.state = ClaimState::Fulfilled(advertisement);
						return Some(advertisement.peer_id)
					}
				}
			}
		}

		// TODO: log smth
		None
	}
}

/// Performs a sanity check between advertised and fetched collations.
fn initial_fetched_collation_sanity_check(
	advertised: &Advertisement,
	fetched: &CandidateReceipt,
) -> std::result::Result<(), SecondingError> {
	// This implies a check on the declared para. TODO: we need explicit check for older protocol
	// version.
	if advertised.prospective_candidate.candidate_hash != fetched.hash() {
		return Err(SecondingError::CandidateHashMismatch)
	}

	if advertised.relay_parent != fetched.descriptor.relay_parent() {
		return Err(SecondingError::RelayParentMismatch)
	}

	Ok(())
}

fn persisted_validation_data_sanity_check(
	persisted_validation_data: &PersistedValidationData,
	fetched: &CandidateReceipt,
	maybe_parent_head_and_hash: Option<(&HeadData, &Hash)>,
) -> std::result::Result<(), SecondingError> {
	if persisted_validation_data.hash() != fetched.descriptor().persisted_validation_data_hash() {
		return Err(SecondingError::PersistedValidationDataMismatch)
	}

	if maybe_parent_head_and_hash.map_or(false, |(head, hash)| head.hash() != *hash) {
		return Err(SecondingError::ParentHeadDataMismatch)
	}

	Ok(())
}

/// All state relevant for the validator side of the protocol lives here.
struct State {
	collation_manager: CollationManager,

	peer_manager: PeerManager,

	reputation_db: ReputationDb,

	keystore: KeystorePtr,
}

impl State {
	fn new(keystore: KeystorePtr) -> Self {
		Self {
			peer_manager: PeerManager::default(),
			collation_manager: CollationManager::default(),
			reputation_db: ReputationDb::default(),
			keystore,
		}
	}

	async fn handle_our_view_change<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		new_view: OurView,
	) {
		let new_leaves = self.collation_manager.view_update(sender, &self.keystore, new_view).await;

		let rep_bumps = self.reputation_db.new_leaves(new_leaves);

		self.peer_manager.process_bumps(rep_bumps);
		// TODO: collation manager may need to also process bumps, to potentially trigger
		// advertisements that are waiting for a higher rep peer to advertise.

		let maybe_disconnected_peers = self
			.peer_manager
			.scheduled_paras_update(&self.reputation_db, self.collation_manager.assignments());

		self.collation_manager.remove_peers(maybe_disconnected_peers);
	}

	async fn handle_advertisement<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		origin: PeerId,
		relay_parent: Hash,
		maybe_prospective_candidate: Option<ProspectiveCandidate>,
	) {
		let Some(peer_state) = self.peer_manager.connected_peers.peers.get(&origin) else { return };

		// Advertised without being declared. Not a big waste of our time, so ignore it
		let PeerState::Collating(para_id) = peer_state else { return };

		// TODO: We have a result here. we could use the old reputation system for a minor decrease.
		// TODO: we'll later need to handle maybe_prospective_candidate being None.

		self.collation_manager
			.try_accept_advertisement(
				sender,
				Advertisement {
					peer_id: origin,
					para_id: *para_id,
					relay_parent,
					prospective_candidate: maybe_prospective_candidate.unwrap(),
				},
			)
			.await;
	}

	fn handle_declare(&mut self, origin: PeerId, para_id: ParaId) {
		self.peer_manager.declared(origin, para_id)
	}

	fn handle_disconnected(&mut self, peer_id: PeerId) {
		self.peer_manager.handle_disconnected(peer_id);

		self.collation_manager.remove_peers(vec![peer_id]);
	}

	fn handle_connected(&mut self, peer_id: PeerId) {
		let disconnected = self.peer_manager.try_accept(&self.reputation_db, peer_id);

		self.collation_manager.remove_peers(disconnected);
	}

	async fn handle_fetched_collation<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		res: <CollationFetchRequest as Future>::Output,
	) {
		let advertisement = res.0;
		let relay_parent = advertisement.relay_parent;
		let candidate_hash = advertisement.prospective_candidate.candidate_hash;
		let outcome = self.collation_manager.completed_fetch(res);

		match outcome {
			CollationFetchOutcome::Success(fetched_collation) => {
				let pvd = request_prospective_validation_data(
					sender,
					relay_parent,
					fetched_collation.parent_head_data_hash,
					fetched_collation.candidate_receipt.descriptor.para_id(),
					fetched_collation.maybe_parent_head_data.clone(),
				)
				.await
				.unwrap();

				// TODO: handle collations whose parent we don't know yet.
				let pvd = pvd.unwrap();

				persisted_validation_data_sanity_check(
					&pvd,
					&fetched_collation.candidate_receipt,
					fetched_collation
						.maybe_parent_head_data
						.as_ref()
						.and_then(|head| Some((head, &fetched_collation.parent_head_data_hash))),
				);

				sender
					.send_message(CandidateBackingMessage::Second(
						relay_parent,
						fetched_collation.candidate_receipt,
						pvd,
						fetched_collation.pov,
					))
					.await;

				self.collation_manager.seconding_began(relay_parent, candidate_hash);
			},
			CollationFetchOutcome::TryNew(maybe_rep_update) => {
				if let Some(rep_update) = maybe_rep_update {
					self.peer_manager.process_decrease(vec![ReputationBump {
						peer_id: advertisement.peer_id,
						para_id: advertisement.para_id,
						value: rep_update,
					}]);
				}

				// reset collation status
				self.collation_manager.back_to_waiting(relay_parent, candidate_hash);
			},
		}
	}

	async fn collation_seconded<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		relay_parent: Hash,
		statement: SignedFullStatement,
	) {
		let receipt = match statement.payload() {
			Statement::Seconded(receipt) => receipt,
			Statement::Valid(_) => {
				gum::warn!(
					target: LOG_TARGET,
					?statement,
					?relay_parent,
					"Seconded message received with a `Valid` statement",
				);
				return
			},
		};

		let Some(peer_id) = self.collation_manager.seconded(relay_parent, receipt.hash()) else {
			return
		};

		notify_collation_seconded(sender, peer_id, CollationVersion::V2, relay_parent, statement)
			.await;

		// TODO: see if we've unblocked other collations here too.
	}

	async fn invalid_collation(&mut self, receipt: CandidateReceipt) {
		let relay_parent = receipt.descriptor.relay_parent();
		let candidate_hash = receipt.hash();

		if let Some(peer_id) = self.collation_manager.back_to_waiting(relay_parent, candidate_hash)
		{
			self.peer_manager.process_decrease(vec![ReputationBump {
				peer_id,
				para_id: receipt.descriptor.para_id(),
				value: 10,
			}]);
		}
	}

	async fn try_launch_fetch_requests<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
	) {
		self.collation_manager
			.try_launch_fetch_requests(sender, &self.peer_manager)
			.await;
	}
}

#[overseer::contextbounds(CollatorProtocol, prefix = overseer)]
async fn process_incoming_peer_message<Context>(
	ctx: &mut Context,
	state: &mut State,
	origin: PeerId,
	msg: Versioned<protocol_v1::CollatorProtocolMessage, protocol_v2::CollatorProtocolMessage>,
) {
	use protocol_v1::CollatorProtocolMessage as V1;
	use protocol_v2::CollatorProtocolMessage as V2;

	match msg {
		Versioned::V1(V1::Declare(collator_id, para_id, signature)) |
		Versioned::V2(V2::Declare(collator_id, para_id, signature)) |
		Versioned::V3(V2::Declare(collator_id, para_id, signature)) => {
			state.handle_declare(origin, para_id);
		},
		Versioned::V1(V1::CollationSeconded(..)) |
		Versioned::V2(V2::CollationSeconded(..)) |
		Versioned::V3(V2::CollationSeconded(..)) => {
			gum::warn!(
				target: LOG_TARGET,
				peer_id = ?origin,
				"Unexpected `CollationSeconded` message, decreasing reputation",
			);

			// modify_reputation(&mut state.reputation, ctx.sender(), origin,
			// COST_UNEXPECTED_MESSAGE) 	.await;
		},
		Versioned::V1(V1::AdvertiseCollation(relay_parent)) =>
			state.handle_advertisement(ctx.sender(), origin, relay_parent, None).await,
		Versioned::V3(V2::AdvertiseCollation {
			relay_parent,
			candidate_hash,
			parent_head_data_hash,
		}) |
		Versioned::V2(V2::AdvertiseCollation {
			relay_parent,
			candidate_hash,
			parent_head_data_hash,
		}) =>
			state
				.handle_advertisement(
					ctx.sender(),
					origin,
					relay_parent,
					Some(ProspectiveCandidate { candidate_hash, parent_head_data_hash }),
				)
				.await,
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
async fn can_second<Sender>(sender: &mut Sender, advertisement: &Advertisement) -> bool
where
	Sender: CollatorProtocolSenderTrait,
{
	let request = CanSecondRequest {
		candidate_para_id: advertisement.para_id,
		candidate_relay_parent: advertisement.relay_parent,
		candidate_hash: advertisement.prospective_candidate.candidate_hash,
		parent_head_data_hash: advertisement.prospective_candidate.parent_head_data_hash,
	};
	let (tx, rx) = oneshot::channel();
	sender.send_message(CandidateBackingMessage::CanSecond(request, tx)).await;

	rx.await.unwrap_or_else(|err| {
		gum::warn!(
			target: LOG_TARGET,
			?err,
			relay_parent = ?advertisement.relay_parent,
			para_id = ?advertisement.para_id,
			candidate_hash = ?advertisement.prospective_candidate.candidate_hash,
			"CanSecond-request responder was dropped",
		);

		false
	})
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
		CollationVersion::V1 => Versioned::V1(protocol_v1::CollationProtocol::CollatorProtocol(
			protocol_v1::CollatorProtocolMessage::CollationSeconded(relay_parent, statement),
		)),
		CollationVersion::V2 => Versioned::V2(protocol_v2::CollationProtocol::CollatorProtocol(
			protocol_v2::CollatorProtocolMessage::CollationSeconded(relay_parent, statement),
		)),
	};
	sender
		.send_message(NetworkBridgeTxMessage::SendCollationMessage(vec![peer_id], wire_message))
		.await;
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

/// Bridge event switch.
#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
async fn handle_network_msg<Context>(
	ctx: &mut Context,
	state: &mut State,
	bridge_message: NetworkBridgeEvent<net_protocol::CollatorProtocolMessage>,
) -> Result<()> {
	use NetworkBridgeEvent::*;

	match bridge_message {
		PeerConnected(peer_id, observed_role, protocol_version, _) => {
			// let version = match protocol_version.try_into() {
			// 	Ok(version) => version,
			// 	Err(err) => {
			// 		// Network bridge is expected to handle this.
			// 		gum::error!(
			// 			target: LOG_TARGET,
			// 			?peer_id,
			// 			?observed_role,
			// 			?err,
			// 			"Unsupported protocol version"
			// 		);
			// 		return Ok(())
			// 	},
			// };
			state.handle_connected(peer_id);
		},
		PeerDisconnected(peer_id) => {
			state.handle_disconnected(peer_id);
		},
		NewGossipTopology { .. } => {
			// impossible!
		},
		PeerViewChange(peer_id, view) => {},
		OurViewChange(view) => state.handle_our_view_change(ctx.sender(), view).await,
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
async fn process_msg<Context>(ctx: &mut Context, msg: CollatorProtocolMessage, state: &mut State) {
	use CollatorProtocolMessage::*;

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
		NetworkBridgeUpdate(event) =>
			if let Err(e) = handle_network_msg(ctx, state, event).await {
				gum::warn!(
					target: LOG_TARGET,
					err = ?e,
					"Failed to handle incoming network message",
				);
			},
		Seconded(parent, stmt) => {
			state.collation_seconded(ctx.sender(), parent, stmt).await;
		},
		Invalid(_parent, candidate_receipt) => {
			state.invalid_collation(candidate_receipt);
		},
	}
}

/// The main run loop.
#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
pub(crate) async fn run<Context>(
	ctx: Context,
	keystore: KeystorePtr,
) -> std::result::Result<(), crate::error::FatalError> {
	run_inner(ctx, keystore).await
}

#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
async fn run_inner<Context>(
	mut ctx: Context,
	keystore: KeystorePtr,
) -> std::result::Result<(), crate::error::FatalError> {
	let mut state = State::new(keystore);

	loop {
		select! {
			res = ctx.recv().fuse() => {
				match res {
					Ok(FromOrchestra::Communication { msg }) => {
						gum::trace!(target: LOG_TARGET, msg = ?msg, "received a message");
						process_msg(
							&mut ctx,
							msg,
							&mut state,
						).await;
					}
					Ok(FromOrchestra::Signal(OverseerSignal::Conclude)) | Err(_) => break,
					Ok(FromOrchestra::Signal(_)) => continue,
				}
			},
			resp = state.collation_manager.fetching.futures.select_next_some() => {
				state.handle_fetched_collation(ctx.sender(), resp).await;
			}
		}

		// Now try triggering advertisement fetching, if we have room in any of the active leaves
		// (any of them are in Waiting state).
		// TODO: we could optimise to not always re-run this code. Have the other functions return
		// whether or not we should attempt launching fetch requests.
		state.try_launch_fetch_requests(ctx.sender()).await;
	}

	Ok(())
}
