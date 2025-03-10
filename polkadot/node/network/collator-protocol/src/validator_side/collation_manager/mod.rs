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
		common::{
			Advertisement, CollationFetchError, CollationFetchOutcome, CollationFetchResponse,
			FetchedCollation, FAILED_FETCH_SLASH,
		},
		peer_manager::PeerManager,
	},
	LOG_TARGET,
};
use futures::{channel::oneshot, stream::FusedStream};
use polkadot_node_network_protocol::{
	request_response::{outgoing::RequestError, v2 as request_v2, Requests},
	OurView, PeerId,
};
use polkadot_node_subsystem::{
	messages::{CanSecondRequest, CandidateBackingMessage, IfDisconnected, NetworkBridgeTxMessage},
	CollatorProtocolSenderTrait,
};
use polkadot_node_subsystem_util::{
	backing_implicit_view::View as ImplicitView, claim_queue_state::PerLeafClaimQueueState,
	request_claim_queue,
};
use polkadot_primitives::{
	vstaging::CandidateReceiptV2 as CandidateReceipt, CandidateHash, Hash, Id as ParaId,
};
use requests::PendingRequests;
use sp_keystore::KeystorePtr;
use std::collections::{hash_map::Entry, BTreeSet, HashMap, HashSet, VecDeque};

mod requests;

#[derive(Default)]
pub struct CollationManager {
	implicit_view: ImplicitView,
	// One per active leaf
	claim_queue_state: PerLeafClaimQueueState,

	// One per relay parent
	per_relay_parent: HashMap<Hash, PerRelayParent>,

	fetching: PendingRequests,
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

		for leaf in added.iter() {
			self.implicit_view.activate_leaf(sender, *leaf).await.unwrap();
		}

		for leaf in removed {
			let deactivated_ancestry = self.implicit_view.deactivate_leaf(leaf);

			// Remove the fetching collations and advertisements for the deactivated RPs.
			for deactivated in deactivated_ancestry.iter() {
				if let Some(deactivated_rp) = self.per_relay_parent.remove(deactivated) {
					for advertisement in deactivated_rp.all_advertisements() {
						if self
							.fetching
							.contains(&advertisement.prospective_candidate.candidate_hash)
						{
							self.fetching
								.cancel(&advertisement.prospective_candidate.candidate_hash);
						}
					}
				}
			}

			self.claim_queue_state
				.remove_pruned_ancestors(&deactivated_ancestry.into_iter().collect());
		}

		for leaf in added.iter() {
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
			let scheduled = claim_queue.remove(&core_now).unwrap_or_else(|| VecDeque::new());

			let allowed_ancestry =
				self.implicit_view.known_allowed_relay_parents_under(leaf, None).unwrap();

			// Includes the leaf
			for ancestor in allowed_ancestry {
				if let Entry::Vacant(entry) = self.per_relay_parent.entry(*ancestor) {
					entry.insert(PerRelayParent::default());
				}
			}

			let maybe_parent = allowed_ancestry.get(1);

			self.claim_queue_state.add_leaf(leaf, &scheduled, maybe_parent);
		}

		added
	}

	pub fn response_stream(&mut self) -> &mut impl FusedStream<Item = CollationFetchResponse> {
		self.fetching.response_stream()
	}

	pub fn assignments(&self) -> BTreeSet<ParaId> {
		self.claim_queue_state.all_assignments()
	}

	pub async fn try_launch_fetch_requests<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		_peer_manager: &PeerManager,
	) {
		// Advertisements and collations are up to date.
		// Claim queue states for leaves are also up to date.
		// Launch requests when it makes sense.
		let mut requests = vec![];
		let leaves: Vec<_> = self.claim_queue_state.leaves().copied().collect();

		for leaf in leaves {
			let free_slots = self.claim_queue_state.free_slots(&leaf);
			let Some(parents) = self.implicit_view.known_allowed_relay_parents_under(&leaf, None)
			else {
				continue
			};

			'per_slot: for para_id in free_slots {
				// Try picking an advertisement. I'd like this to be a separate method but
				// compiler gets confused with ownership.
				for parent in parents {
					let Some(per_rp) = self.per_relay_parent.get(parent) else { continue };

					for advertisement in per_rp.eligible_advertisements(&para_id).filter(|adv| {
						!self.fetching.contains(&adv.prospective_candidate.candidate_hash)
					}) {
						// This here may also claim a slot of another leaf if eligible.
						if self.claim_queue_state.claim_pending_slot(
							&advertisement.prospective_candidate.candidate_hash,
							&advertisement.relay_parent,
							&para_id,
						) {
							let req = self.fetching.launch(&advertisement);
							requests.push(Requests::CollationFetchingV2(req));
							continue 'per_slot
						}

						// let peer_rep =
						// 	peer_manager.connected_peer_rep(&para_id, peer_id).unwrap();

						// if peer_rep >= INSTANT_FETCH_REP_THRESHOLD {
						// 	over_threshold = Some(*advertisement);
						// 	break 'per_rp;
						// } else {
						// we need to arm some timer
						// }
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

		let mut cancelled_fetches = vec![];
		for peer in peers_to_remove {
			for collations in self.per_relay_parent.values_mut() {
				if let Some(removed_advertisements) = collations.advertisements.remove(&peer) {
					for advertisement in removed_advertisements {
						if self
							.fetching
							.contains(&advertisement.prospective_candidate.candidate_hash)
						{
							self.fetching
								.cancel(&advertisement.prospective_candidate.candidate_hash);
							cancelled_fetches.push(advertisement);
						}
					}
				}
			}
		}

		for advertisement in cancelled_fetches {
			// Also reset the statuses of claims that were pending fetch for these
			// candidates.
			self.claim_queue_state
				.release_claims_for_candidate(&advertisement.prospective_candidate.candidate_hash);
		}
	}

	pub async fn try_accept_advertisement<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		advertisement: Advertisement,
	) -> std::result::Result<(), AdvertisementError> {
		let Some(collations) = self.per_relay_parent.get_mut(&advertisement.relay_parent) else {
			return Err(AdvertisementError::OutOfOurView)
		};

		let advertisements = collations
			.advertisements
			.entry(advertisement.peer_id)
			.or_insert_with(|| Default::default());

		// Check if backing subsystem allows to second this candidate.
		//
		// This is also only important when async backing or elastic scaling is enabled.
		let can_second = can_second(sender, &advertisement).await;

		if !can_second {
			return Err(AdvertisementError::BlockedByBacking)
		}

		let max_assignments = self
			.claim_queue_state
			.get_all_slots_for_para_at(&advertisement.relay_parent, &advertisement.para_id);
		if advertisements.len() >= max_assignments {
			return Err(AdvertisementError::PeerLimitReached)
		}

		if advertisements.contains(&advertisement) {
			return Err(AdvertisementError::Duplicate)
		}

		advertisements.insert(advertisement);

		Ok(())
	}

	pub fn completed_fetch(&mut self, res: CollationFetchResponse) -> CollationFetchOutcome {
		let (advertisement, res) = res;
		self.fetching.completed(&advertisement.prospective_candidate.candidate_hash);

		let collations = self.per_relay_parent.entry(advertisement.relay_parent).or_default();
		if let Some(advertisements) = collations.advertisements.get_mut(&advertisement.peer_id) {
			advertisements.remove(&advertisement);
		}

		let outcome = match res {
			Err(CollationFetchError::Cancelled) => {
				// Was cancelled by the subsystem.
				CollationFetchOutcome::TryNew(0)
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
				CollationFetchOutcome::TryNew(0)
			},
			Err(CollationFetchError::Request(err)) if err.is_timed_out() => {
				gum::debug!(
					target: LOG_TARGET,
					hash = ?advertisement.relay_parent,
					para_id = ?advertisement.para_id,
					peer_id = ?advertisement.peer_id,
					"Request timed out"
				);
				CollationFetchOutcome::TryNew(FAILED_FETCH_SLASH)
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
				CollationFetchOutcome::TryNew(0)
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
				CollationFetchOutcome::TryNew(FAILED_FETCH_SLASH)
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
					gum::warn!(
						target: LOG_TARGET,
						?advertisement,
						"Invalid fetched collation: {}",
						err
					);
					return CollationFetchOutcome::TryNew(FAILED_FETCH_SLASH)
				}

				// It can't be a duplicate, because we check before initiating fetch. TODO: with the
				// old protocol version, it can be.
				collations.fetched_collations.insert(
					advertisement.prospective_candidate.candidate_hash,
					advertisement.peer_id,
				);

				CollationFetchOutcome::Success(fetched_collation)
			},
			CollationFetchOutcome::TryNew(rep_change) => CollationFetchOutcome::TryNew(rep_change),
		}
	}

	pub fn release_slot(
		&mut self,
		relay_parent: &Hash,
		candidate_hash: &CandidateHash,
	) -> Option<PeerId> {
		let peer_id = self
			.per_relay_parent
			.get(relay_parent)
			.and_then(|per_rp| per_rp.fetched_collations.get(candidate_hash));

		self.claim_queue_state.release_claims_for_candidate(candidate_hash);

		peer_id.copied()
	}

	pub fn seconded(
		&mut self,
		relay_parent: &Hash,
		candidate_hash: &CandidateHash,
		para_id: &ParaId,
	) -> Option<PeerId> {
		let peer_id = self
			.per_relay_parent
			.get(relay_parent)
			.and_then(|per_rp| per_rp.fetched_collations.get(candidate_hash));

		self.claim_queue_state
			.claim_seconded_slot(candidate_hash, relay_parent, para_id);

		peer_id.copied()
	}
}

#[derive(Default)]
struct PerRelayParent {
	advertisements: HashMap<PeerId, HashSet<Advertisement>>,
	// Only kept to make sure that we don't re-request the same collations and so that we know who
	// to punish for supplying an invalid collation.
	fetched_collations: HashMap<CandidateHash, PeerId>,
}

impl PerRelayParent {
	fn all_advertisements(&self) -> impl Iterator<Item = &Advertisement> {
		self.advertisements.values().flatten()
	}

	fn eligible_advertisements<'a>(
		&'a self,
		para_id: &'a ParaId,
	) -> impl Iterator<Item = &'a Advertisement> + 'a {
		self.advertisements
			.values()
			.map(|list| list.iter())
			.flatten()
			.filter(move |adv| {
				(&adv.para_id == para_id) &&
				// We can be pretty sure that this is true
				!self.fetched_collations.contains_key(&adv.prospective_candidate.candidate_hash)
			})
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
pub enum AdvertisementError {
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
	/// Advertisement is already known.
	Duplicate,
	/// Collation relay parent is out of our view.
	OutOfOurView,
	/// A limit for announcements per peer is reached.
	PeerLimitReached,
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
