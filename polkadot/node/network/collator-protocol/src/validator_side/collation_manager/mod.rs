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

use crate::{
	error::SecondingError,
	validator_side::{
		common::{Advertisement, CollationFetchError, CollationFetchOutcome, FetchedCollation},
		peer_manager::PeerManager,
	},
	LOG_TARGET,
};
use futures::{
	channel::oneshot,
	future::BoxFuture,
	stream::{FusedStream, FuturesUnordered},
	task::Poll,
	FutureExt,
};
use polkadot_node_network_protocol::{
	request_response::{
		outgoing::{Recipient, RequestError},
		v2 as request_v2, OutgoingRequest, OutgoingResult, Requests,
	},
	OurView, PeerId,
};
use polkadot_node_subsystem::{
	messages::{CanSecondRequest, CandidateBackingMessage, IfDisconnected, NetworkBridgeTxMessage},
	CollatorProtocolSenderTrait,
};
use polkadot_node_subsystem_util::{
	backing_implicit_view::View as ImplicitView, request_claim_queue,
};
use polkadot_primitives::{
	vstaging::CandidateReceiptV2 as CandidateReceipt, CandidateHash, CoreIndex, Hash, Id as ParaId,
};
use sp_keystore::KeystorePtr;
use std::{
	collections::{BTreeSet, HashMap, HashSet, VecDeque},
	future::Future,
	pin::Pin,
};
use tokio_util::sync::CancellationToken;

use super::common::CollationFetchResponse;

#[derive(Default)]
pub struct CollationManager {
	implicit_view: ImplicitView,
	// One per active leaf
	claim_queue_state: HashMap<Hash, ClaimQueueState>,

	// One per relay parent
	collations: HashMap<Hash, Collations>,

	fetching: PendingRequests,

	core: CoreIndex,
}

impl CollationManager {
	pub async fn view_update<Sender: CollatorProtocolSenderTrait>(
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

	pub fn response_stream(&mut self) -> &mut impl FusedStream<Item = CollationFetchResponse> {
		&mut self.fetching.futures
	}

	pub fn assignments(&self) -> BTreeSet<ParaId> {
		let mut scheduled_paras = BTreeSet::new();

		for state in self.claim_queue_state.values() {
			scheduled_paras.extend(state.claim_queue.iter().map(|c| c.para));
		}

		scheduled_paras
	}

	pub async fn try_launch_fetch_requests<Sender: CollatorProtocolSenderTrait>(
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
									let peer_rep = peer_manager
										.connected_peer_rep(&claim.para, peer_id)
										.unwrap();
									has_some_advertisements = true;

									if peer_rep >= threshold {
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

	pub fn remove_peers(&mut self, peers_to_remove: Vec<PeerId>) {
		if peers_to_remove.is_empty() {
			return
		}

		let peers_to_remove = peers_to_remove.into_iter().collect::<HashSet<_>>();

		for collations in self.collations.values_mut() {
			collations.advertisements.retain(|peer, _| !peers_to_remove.contains(peer));
		}
	}

	pub async fn try_accept_advertisement<Sender: CollatorProtocolSenderTrait>(
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

	pub fn completed_fetch(
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

	pub fn seconding_began(&mut self, relay_parent: Hash, candidate_hash: CandidateHash) {
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
	pub fn back_to_waiting(
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

	pub fn seconded(
		&mut self,
		relay_parent: Hash,
		candidate_hash: CandidateHash,
	) -> Option<PeerId> {
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

/// Future that concludes when the collator has responded to our collation fetch request
/// or the request was cancelled by the validator.
struct CollationFetchRequest {
	/// Info about the requested collation.
	pub advertisement: Advertisement,
	/// Responses from collator.
	pub from_collator: BoxFuture<'static, OutgoingResult<request_v2::CollationFetchingResponse>>,
	/// Handle used for checking if this request was cancelled.
	pub cancellation_token: CancellationToken,
}

impl Future for CollationFetchRequest {
	type Output = CollationFetchResponse;

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
