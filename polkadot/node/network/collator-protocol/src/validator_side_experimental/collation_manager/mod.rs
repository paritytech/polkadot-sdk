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
	validator_side::{
		error::SecondingError, request_persisted_validation_data,
		request_prospective_validation_data, BlockedCollationId, PerLeafClaimQueueState,
	},
	validator_side_experimental::{
		common::{
			Advertisement, CanSecond, CollationFetchError, CollationFetchResponse,
			ProspectiveCandidate, Score, SecondingRejection, FAILED_FETCH_SLASH,
		},
		error::{Error, FatalResult, Result},
		peer_manager::PeerManager,
	},
	LOG_TARGET,
};
use fatality::{Nested, Split};
use futures::{channel::oneshot, stream::FusedStream};
use polkadot_node_network_protocol::{
	request_response::{outgoing::RequestError, v2 as request_v2, Requests},
	OurView, PeerId, View,
};
use polkadot_node_primitives::PoV;
use polkadot_node_subsystem::{
	messages::{CanSecondRequest, CandidateBackingMessage, IfDisconnected, NetworkBridgeTxMessage},
	ActivatedLeaf, CollatorProtocolSenderTrait,
};
use polkadot_node_subsystem_util::{
	backing_implicit_view::View as ImplicitView, request_claim_queue, request_node_features,
	request_session_index_for_child, request_validator_groups, request_validators,
	runtime::recv_runtime,
};
use polkadot_primitives::{
	node_features,
	vstaging::{
		CandidateDescriptorV2 as CandidateDescriptor, CandidateDescriptorVersion,
		CandidateReceiptV2 as CandidateReceipt,
	},
	CandidateHash, CoreIndex, GroupRotationInfo, Hash, HeadData, Id as ParaId,
	PersistedValidationData, SessionIndex, ValidatorId, ValidatorIndex,
};
use requests::PendingRequests;
use schnellru::{ByLength, LruMap};
use sp_keystore::KeystorePtr;
use std::{
	collections::{hash_map::Entry, BTreeSet, HashMap, HashSet, VecDeque},
	time::{SystemTime, UNIX_EPOCH},
};

mod requests;

/// Reason for rejecting an advertisement.
#[derive(Debug, thiserror::Error)]
pub enum AdvertisementError {
	#[error("Validator is not assigned to this paraid")]
	InvalidAssignment,
	#[error("Duplicate advertisement")]
	Duplicate,
	#[error("Advertised relay parent is out of our view")]
	OutOfOurView,
	#[error("Para reached the candidate limit")]
	PeerLimitReached,
	#[error("Seconding not allowed by backing subsystem")]
	BlockedByBacking,
}

/// Fetched collation data.
#[derive(Debug, Clone)]
struct FetchedCollation {
	/// Candidate receipt.
	pub candidate_receipt: CandidateReceipt,
	/// Proof of validity.
	pub pov: PoV,
	/// Optional parachain parent head data.
	pub maybe_parent_head_data: Option<HeadData>,
	/// TODO
	pub maybe_parent_head_data_hash: Option<Hash>,
	/// TODO
	pub peer_id: PeerId,
}

pub struct CollationManager {
	implicit_view: ImplicitView,
	// One per active leaf
	claim_queue_state: PerLeafClaimQueueState,

	/// Collations which we haven't been able to second due to their parent not being known by
	/// prospective-parachains. Mapped from the paraid and parent_head_hash to the fetched
	/// collation data. Only needed for async backing. For elastic scaling, the fetched collation
	/// must contain the full parent head data.
	blocked_from_seconding: HashMap<BlockedCollationId, Vec<FetchedCollation>>,

	// One per relay parent
	per_relay_parent: HashMap<Hash, PerRelayParent>,

	per_session: LruMap<SessionIndex, PerSessionInfo>,

	fetching: PendingRequests,
}

impl CollationManager {
	pub async fn new<Sender: CollatorProtocolSenderTrait>(
		sender: &mut Sender,
		keystore: &KeystorePtr,
		active_leaf: ActivatedLeaf,
	) -> FatalResult<Self> {
		let mut instance = Self {
			implicit_view: ImplicitView::new(None),
			claim_queue_state: PerLeafClaimQueueState::new(),
			per_relay_parent: HashMap::new(),
			blocked_from_seconding: HashMap::new(),
			per_session: LruMap::new(ByLength::new(2)),
			fetching: PendingRequests::default(),
		};

		instance
			.view_update(sender, keystore, OurView::new([active_leaf.hash], 0))
			.await?;

		Ok(instance)
	}

	pub async fn view_update<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		keystore: &KeystorePtr,
		new_view: OurView,
	) -> FatalResult<()> {
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
			if let Err(err) = self
				.implicit_view
				.activate_leaf(sender, *leaf)
				.await
				.map_err(Error::FailedToActivateLeafInImplicitView)
			{
				err.split()?.log();
				continue
			}
		}

		for leaf in removed {
			let deactivated_ancestry = self.implicit_view.deactivate_leaf(leaf);

			// Remove the fetching collations and advertisements for the deactivated RPs.
			for deactivated in deactivated_ancestry.iter() {
				if let Some(deactivated_rp) = self.per_relay_parent.remove(deactivated) {
					for advertisement in deactivated_rp.all_advertisements() {
						if self.fetching.contains(&advertisement) {
							self.fetching.cancel(&advertisement);
						}
					}
				}
			}

			self.claim_queue_state
				.remove_pruned_ancestors(&deactivated_ancestry.into_iter().collect());
		}

		// Remove blocked seconding requests that left the view.
		self.blocked_from_seconding.retain(|_, collations| {
			collations.retain(|collation| {
				self.per_relay_parent
					.contains_key(&collation.candidate_receipt.descriptor.relay_parent())
			});

			!collations.is_empty()
		});

		for leaf in added.iter() {
			let Some(allowed_ancestry) = self
				.implicit_view
				.known_allowed_relay_parents_under(leaf, None)
				.map(|v| v.into_iter().copied().collect::<Vec<_>>())
			else {
				continue
			};

			// Includes the leaf
			for ancestor in allowed_ancestry.iter() {
				if self.per_relay_parent.contains_key(&ancestor) {
					continue
				}

				let session_index =
					match recv_runtime(request_session_index_for_child(*ancestor, sender).await)
						.await
						.map_err(Error::Runtime)
					{
						Ok(session_index) => session_index,
						Err(err) => {
							err.split()?.log();
							continue
						},
					};

				let (core, assignments) =
					match self.get_our_core_schedule(sender, keystore, leaf, session_index).await {
						Ok(assignments) => assignments,
						Err(err) => {
							err.split()?.log();
							Default::default()
						},
					};

				self.per_relay_parent
					.insert(*ancestor, PerRelayParent::new(session_index, core));

				if ancestor == leaf {
					let maybe_parent = allowed_ancestry.get(1).copied();

					self.claim_queue_state.add_leaf(leaf, &assignments, maybe_parent.as_ref());
				}
			}
		}

		Ok(())
	}

	pub fn response_stream(&mut self) -> &mut impl FusedStream<Item = CollationFetchResponse> {
		self.fetching.response_stream()
	}

	pub fn assignments(&self) -> BTreeSet<ParaId> {
		self.claim_queue_state.all_assignments()
	}

	pub async fn try_accept_advertisement<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		advertisement: Advertisement,
	) -> std::result::Result<(), AdvertisementError> {
		let Some(per_rp) = self.per_relay_parent.get_mut(&advertisement.relay_parent) else {
			return Err(AdvertisementError::OutOfOurView)
		};

		let mut peer_advertisements = per_rp
			.peer_advertisements
			.entry(advertisement.peer_id)
			.or_insert_with(|| Default::default());

		peer_advertisements.total += 1;

		let max_assignments = self
			.claim_queue_state
			.get_all_slots_for_para_at(&advertisement.relay_parent, &advertisement.para_id);

		if max_assignments == 0 {
			return Err(AdvertisementError::InvalidAssignment)
		}

		if peer_advertisements.total > max_assignments {
			return Err(AdvertisementError::PeerLimitReached)
		}

		if peer_advertisements.advertisements.contains(&advertisement) {
			return Err(AdvertisementError::Duplicate)
		}

		if let Some(ProspectiveCandidate { candidate_hash, .. }) =
			advertisement.prospective_candidate
		{
			if per_rp.fetched_collations.contains_key(&candidate_hash) {
				return Err(AdvertisementError::Duplicate)
			}
		}

		if self.fetching.contains(&advertisement) {
			return Err(AdvertisementError::Duplicate)
		}

		let can_second = backing_allows_seconding(sender, &advertisement).await;

		if !can_second {
			return Err(AdvertisementError::BlockedByBacking)
		}

		peer_advertisements.advertisements.insert(advertisement);

		Ok(())
	}

	// pub async fn try_launch_fetch_requests<Sender: CollatorProtocolSenderTrait>(
	// 	&mut self,
	// 	sender: &mut Sender,
	// 	peer_manager: &PeerManager,
	// ) {
	// 	// Advertisements and collations are up to date.
	// 	// Claim queue states for leaves are also up to date.
	// 	// Launch requests when it makes sense.
	// 	let mut requests = vec![];
	// 	let leaves: Vec<_> = self.claim_queue_state.leaves().copied().collect();
	// 	let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();

	// 	for leaf in leaves {
	// 		let free_slots = self.claim_queue_state.free_slots(&leaf);
	// 		let Some(parents) = self.implicit_view.known_allowed_relay_parents_under(&leaf, None)
	// 		else {
	// 			continue
	// 		};

	// 		'per_slot: for para_id in free_slots {
	// 			// Try picking an advertisement. I'd like this to be a separate method but
	// 			// compiler gets confused with ownership.
	// 			for parent in parents {
	// 				let Some(per_rp) = self.per_relay_parent.get(parent) else { continue };

	// 				for advertisement in per_rp.eligible_advertisements(&para_id).filter(|adv| {
	// 					!self.fetching.contains(&adv.prospective_candidate.candidate_hash)
	// 				}) {
	// 					let Some(peer_rep) =
	// 						peer_manager.connected_peer_rep(&para_id, &advertisement.peer_id)
	// 					else {
	// 						// Is the peer no longer connected? Impossible, as its advertisements
	// 						// should no longer exist.
	// 						continue
	// 					};
	// 					let Some(advertisement_timestamp) =
	// 						per_rp.advertisement_timestamps.get(advertisement)
	// 					else {
	// 						continue
	// 					};

	// 					let doesnt_have_better_peers = false;
	// 					let time_since_advertisement = now.saturating_sub(*advertisement_timestamp);
	// 					if peer_rep >= INSTANT_FETCH_REP_THRESHOLD ||
	// 						time_since_advertisement >= UNDER_THRESHOLD_FETCH_DELAY ||
	// 						doesnt_have_better_peers
	// 					{
	// 						// This here may also claim a slot of another leaf if eligible.
	// 						if self.claim_queue_state.claim_pending_slot(
	// 							&advertisement.prospective_candidate.candidate_hash,
	// 							&advertisement.relay_parent,
	// 							&para_id,
	// 						) {
	// 							let req = self.fetching.launch(&advertisement);
	// 							requests.push(Requests::CollationFetchingV2(req));
	// 							continue 'per_slot
	// 						}
	// 					} else {
	// 						gum::debug!(
	// 							target: LOG_TARGET,
	// 							"Skipping advertisement, as the peer doesn't have a high enough reputation to warrant
	// a fetch now" 						);
	// 					}
	// 				}
	// 			}
	// 		}
	// 	}

	// 	if !requests.is_empty() {
	// 		sender
	// 			.send_message(NetworkBridgeTxMessage::SendRequests(
	// 				requests,
	// 				IfDisconnected::ImmediateError,
	// 			))
	// 			.await;
	// 	}
	// }

	pub fn remove_peers(&mut self, peers_to_remove: HashSet<PeerId>) {
		if peers_to_remove.is_empty() {
			return
		}

		let mut cancelled_fetches = vec![];
		for peer in peers_to_remove {
			for per_rp in self.per_relay_parent.values_mut() {
				if let Some(removed_advertisements) = per_rp.peer_advertisements.remove(&peer) {
					for advertisement in removed_advertisements.advertisements {
						if self.fetching.contains(&advertisement) {
							self.fetching.cancel(&advertisement);
							cancelled_fetches.push(advertisement);
						}
					}
				}
			}
		}

		// No need to reset now the statuses of claims that were pending fetch for these candidates,
		// as the futures will soon conclude with Cancelled reason.
	}

	pub async fn completed_fetch<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		res: CollationFetchResponse,
	) -> CanSecond {
		let advertisement = res.0;
		self.fetching.completed(&advertisement);

		let mut reject_info = SecondingRejection {
			relay_parent: advertisement.relay_parent,
			peer_id: advertisement.peer_id,
			para_id: advertisement.para_id,
			maybe_output_head_hash: None,
			maybe_candidate_hash: advertisement.prospective_candidate.map(|p| p.candidate_hash),
		};

		let Some(per_rp) = self.per_relay_parent.get_mut(&advertisement.relay_parent) else {
			gum::debug!(
				target: LOG_TARGET,
				hash = ?advertisement.relay_parent,
				para_id = ?advertisement.para_id,
				peer_id = ?advertisement.peer_id,
				"Collation fetch concluded for relay parent out of view"
			);
			return CanSecond::No(None, reject_info)
		};
		let Some(session_info) = self.per_session.get(&per_rp.session_index) else {
			gum::debug!(
				target: LOG_TARGET,
				hash = ?advertisement.relay_parent,
				para_id = ?advertisement.para_id,
				peer_id = ?advertisement.peer_id,
				"Collation fetch concluded for relay parent whose session index is unknown"
			);
			return CanSecond::No(None, reject_info)
		};

		if let Some(advertisements) = per_rp.peer_advertisements.get_mut(&advertisement.peer_id) {
			advertisements.advertisements.remove(&advertisement);
		}

		let res = process_collation_fetch_result(res);

		match res {
			Ok(fetched_collation) => {
				// It can't be a duplicate, because we check before initiating fetch. For the old
				// protocol version, we anyway only fetch one per relay parent.
				per_rp
					.fetched_collations
					.insert(fetched_collation.candidate_receipt.hash(), advertisement.peer_id);

				reject_info.maybe_output_head_hash =
					Some(fetched_collation.candidate_receipt.descriptor.para_head());

				// Some initial sanity checks on the fetched collation, based on the advertisement.
				if let Err(err) = compare_fetched_collation_with_advertisement(
					&advertisement,
					&fetched_collation.candidate_receipt,
				) {
					gum::warn!(
						target: LOG_TARGET,
						?advertisement,
						"Invalid fetched collation: {}",
						err
					);
					return CanSecond::No(Some(FAILED_FETCH_SLASH), reject_info)
				}

				// Sanity check of the candidate receipt version.
				if let Err(err) = descriptor_version_sanity_check(
					fetched_collation.candidate_receipt.descriptor(),
					session_info.v2_receipts,
					per_rp,
				) {
					gum::warn!(
						target: LOG_TARGET,
						?advertisement,
						"Failed descriptor version sanity check for fetched collation: {}",
						err
					);
					return CanSecond::No(Some(FAILED_FETCH_SLASH), reject_info)
				}

				self.can_begin_seconding(sender, fetched_collation, true, reject_info).await
			},
			Err(rep_change) => CanSecond::No(rep_change, reject_info),
		}
	}

	pub fn release_slot(
		&mut self,
		relay_parent: &Hash,
		para_id: ParaId,
		maybe_candidate_hash: Option<&CandidateHash>,
		maybe_output_head_hash: Option<Hash>,
	) {
		if let Some(candidate_hash) = maybe_candidate_hash {
			if !self.claim_queue_state.release_claims_for_candidate(candidate_hash) {
				gum::debug!(
					target: LOG_TARGET,
					?relay_parent,
					?candidate_hash,
					"Could not release slot for candidate, it wasn't claimed",
				);
			}
		} else {
			if !self.claim_queue_state.release_claims_for_relay_parent(relay_parent) {
				gum::debug!(
					target: LOG_TARGET,
					?relay_parent,
					"Could not release slot for candidate, it wasn't claimed",
				);
			}
		}

		if let Some(output_head_hash) = maybe_output_head_hash {
			// Remove any collations that were blocked on this parent. TODO: add a log
			let Some(blocked) = self
				.blocked_from_seconding
				.remove(&BlockedCollationId { para_id, parent_head_data_hash: output_head_hash })
			else {
				return
			};

			for collation in blocked {
				let candidate_hash = collation.candidate_receipt.hash();
				if !self.claim_queue_state.release_claims_for_candidate(&candidate_hash) {
					gum::debug!(
						target: LOG_TARGET,
						relay_parent = ?collation.candidate_receipt.descriptor.relay_parent(),
						?candidate_hash,
						"Could not release slot for candidate, it wasn't claimed",
					);
				}
			}
		}
	}

	pub fn get_peer_id_of_fetched_collation(
		&self,
		relay_parent: &Hash,
		candidate_hash: &CandidateHash,
	) -> Option<PeerId> {
		self.per_relay_parent
			.get(relay_parent)
			.and_then(|per_rp| per_rp.fetched_collations.get(candidate_hash))
			.copied()
	}

	pub async fn seconded<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		relay_parent: &Hash,
		candidate_hash: &CandidateHash,
		para_id: &ParaId,
		output_head_hash: Hash,
	) -> (Option<PeerId>, Vec<CanSecond>) {
		let peer_id = self
			.per_relay_parent
			.get(relay_parent)
			.and_then(|per_rp| per_rp.fetched_collations.get(candidate_hash))
			.copied();

		self.claim_queue_state
			.claim_seconded_slot(candidate_hash, relay_parent, para_id);

		let mut unblocked_can_second = vec![];

		// See if we've unblocked other collations here too.
		if let Some(unblocked) = self.blocked_from_seconding.remove(&BlockedCollationId {
			para_id: *para_id,
			parent_head_data_hash: output_head_hash,
		}) {
			// TODO: log

			for fetched_collation in unblocked {
				let reject_info = SecondingRejection {
					relay_parent: fetched_collation.candidate_receipt.descriptor.relay_parent(),
					peer_id: fetched_collation.peer_id,
					para_id: fetched_collation.candidate_receipt.descriptor.para_id(),
					maybe_output_head_hash: Some(
						fetched_collation.candidate_receipt.descriptor.para_head(),
					),
					maybe_candidate_hash: Some(fetched_collation.candidate_receipt.hash()),
				};
				let can_second =
					self.can_begin_seconding(sender, fetched_collation, false, reject_info).await;

				unblocked_can_second.push(can_second)
			}
		}

		(peer_id, unblocked_can_second)
	}

	async fn get_our_core_schedule<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		keystore: &KeystorePtr,
		parent: &Hash,
		session_index: SessionIndex,
	) -> Result<(CoreIndex, VecDeque<ParaId>)> {
		let block_number = self
			.implicit_view
			.get_block_number(parent)
			.ok_or_else(|| Error::BlockNumberNotFoundInImplicitView(*parent))?;
		let session_info = self.get_session_info(sender, parent, session_index).await?;
		let mut rotation_info = session_info.group_rotation_info.clone();

		rotation_info.now = block_number;

		let core_now = if let Some(group) =
			polkadot_node_subsystem_util::signing_key_and_index(&session_info.validators, keystore)
				.and_then(|(_, index)| {
					polkadot_node_subsystem_util::find_validator_group(&session_info.groups, index)
				}) {
			rotation_info.core_for_group(group, session_info.groups.len())
		} else {
			gum::trace!(target: LOG_TARGET, ?parent, "Not a validator");
			return Ok(Default::default())
		};

		let mut claim_queue = recv_runtime(request_claim_queue(*parent, sender).await).await?;
		Ok((core_now, claim_queue.remove(&core_now).unwrap_or_else(|| VecDeque::new())))
	}

	async fn get_session_info<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		parent: &Hash,
		index: SessionIndex,
	) -> Result<&PerSessionInfo> {
		if self.per_session.get(&index).is_none() {
			let validators = recv_runtime(request_validators(*parent, sender).await).await?;
			let (groups, group_rotation_info) =
				recv_runtime(request_validator_groups(*parent, sender).await).await?;
			let v2_receipts = recv_runtime(request_node_features(*parent, index, sender).await)
				.await?
				.get(node_features::FeatureIndex::CandidateReceiptV2 as usize)
				.map(|b| *b)
				.unwrap_or(false);

			self.per_session.insert(
				index,
				PerSessionInfo { validators, groups, group_rotation_info, v2_receipts },
			);
		}

		Ok(self.per_session.get(&index).expect("Just inserted"))
	}

	async fn can_begin_seconding<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		fetched_collation: FetchedCollation,
		queue_blocked_collations: bool,
		reject_info: SecondingRejection,
	) -> CanSecond {
		let relay_parent = fetched_collation.candidate_receipt.descriptor.relay_parent();
		let candidate_hash = fetched_collation.candidate_receipt.hash();
		let output_head_hash = fetched_collation.candidate_receipt.descriptor.para_head();
		let para_id = fetched_collation.candidate_receipt.descriptor.para_id();

		let can_second = match fetch_pvd(
			sender,
			&fetched_collation.candidate_receipt,
			fetched_collation.maybe_parent_head_data_hash,
			fetched_collation.maybe_parent_head_data.clone(),
		)
		.await
		{
			Err(error) => match error {
				SecondingError::BlockedOnParent(parent) => {
					gum::debug!(
						target: LOG_TARGET,
						?candidate_hash,
						?relay_parent,
						?para_id,
						"Collation having parent head data hash {} is blocked from seconding. Waiting on its parent to be validated.",
						parent
					);

					if queue_blocked_collations {
						self.blocked_from_seconding
							.entry(BlockedCollationId { para_id, parent_head_data_hash: parent })
							.or_insert_with(Vec::new)
							.push(fetched_collation);
					}

					// Mark this claim with the right candidate hash. This is a no-op if for
					// protocol v2 but in case of v1, the claim was made on the relay parent but
					// without a candidate hash.
					self.claim_queue_state.mark_pending_slot_with_candidate(
						&candidate_hash,
						&relay_parent,
						&para_id,
					);

					CanSecond::BlockedOnParent(parent, reject_info)
				},
				error if error.is_malicious() => {
					gum::warn!(
						target: LOG_TARGET,
						?candidate_hash,
						?relay_parent,
						?para_id,
						"Failed persisted validation data checks: {}",
						error
					);
					return CanSecond::No(Some(FAILED_FETCH_SLASH), reject_info)
				},
				err => {
					gum::warn!(
						target: LOG_TARGET,
						?candidate_hash,
						?relay_parent,
						?para_id,
						"Failed persisted validation data checks: {}",
						err
					);
					return CanSecond::No(None, reject_info)
				},
			},
			Ok(pvd) => {
				// Mark this claim with the right candidate hash. This is a no-op if for
				// protocol v2 but in case of v1, the claim was made on the relay parent but
				// without a candidate hash.
				self.claim_queue_state.mark_pending_slot_with_candidate(
					&candidate_hash,
					&relay_parent,
					&para_id,
				);
				CanSecond::Yes(fetched_collation.candidate_receipt, fetched_collation.pov, pvd)
			},
		};

		can_second
	}
}

struct PerRelayParent {
	peer_advertisements: HashMap<PeerId, PeerAdvertisements>,
	// advertisement_timestamps: HashMap<Advertisement, u128>,
	// Only kept to make sure that we don't re-request the same collations and so that we know who
	// to punish for supplying an invalid collation.
	fetched_collations: HashMap<CandidateHash, PeerId>,
	session_index: SessionIndex,
	core_index: CoreIndex,
}

impl PerRelayParent {
	fn new(session_index: SessionIndex, core_index: CoreIndex) -> Self {
		Self {
			session_index,
			core_index,
			peer_advertisements: Default::default(),
			fetched_collations: Default::default(),
		}
	}

	fn all_advertisements(&self) -> impl Iterator<Item = &Advertisement> {
		self.peer_advertisements.values().map(|adv| adv.advertisements.iter()).flatten()
	}

	// fn eligible_advertisements<'a>(
	// 	&'a self,
	// 	para_id: &'a ParaId,
	// ) -> impl Iterator<Item = &'a Advertisement> + 'a {
	// 	self.advertisements
	// 		.values()
	// 		.map(|list| list.iter())
	// 		.flatten()
	// 		.filter(move |adv| {
	// 			(&adv.para_id == para_id) &&
	// 			// We can be pretty sure that this is true
	// 			!self.fetched_collations.contains_key(&adv.prospective_candidate.candidate_hash)
	// 		})
	// }
}

#[derive(Default)]
struct PeerAdvertisements {
	advertisements: HashSet<Advertisement>,
	// We increment this even for advertisements that we don't end up accepting, so that we take
	// these into account when rate limiting.
	total: usize,
}

struct PerSessionInfo {
	validators: Vec<ValidatorId>,
	groups: Vec<Vec<ValidatorIndex>>,
	// The group rotation info changes once per session, apart from the `now` field. The caller
	// must ensure to override it with the right value.
	group_rotation_info: GroupRotationInfo,
	v2_receipts: bool,
}

// Requests backing to sanity check the advertisement.
async fn backing_allows_seconding<Sender>(
	sender: &mut Sender,
	advertisement: &Advertisement,
) -> bool
where
	Sender: CollatorProtocolSenderTrait,
{
	let Some(prospective_candidate) = advertisement.prospective_candidate else {
		// Nothing to check for v1 protocol.
		return true
	};

	let request = CanSecondRequest {
		candidate_para_id: advertisement.para_id,
		candidate_relay_parent: advertisement.relay_parent,
		candidate_hash: prospective_candidate.candidate_hash,
		parent_head_data_hash: prospective_candidate.parent_head_data_hash,
	};
	let (tx, rx) = oneshot::channel();
	sender.send_message(CandidateBackingMessage::CanSecond(request, tx)).await;

	rx.await.unwrap_or_else(|err| {
		gum::warn!(
			target: LOG_TARGET,
			?err,
			relay_parent = ?advertisement.relay_parent,
			para_id = ?advertisement.para_id,
			candidate_hash = ?prospective_candidate.candidate_hash,
			"CanSecond-request responder was dropped",
		);

		false
	})
}

/// Performs a sanity check between advertised and fetched collations.
fn compare_fetched_collation_with_advertisement(
	advertised: &Advertisement,
	fetched: &CandidateReceipt,
) -> std::result::Result<(), SecondingError> {
	// This implies a check on the declared para if this was a v2 advertisement
	if let Some(ProspectiveCandidate { candidate_hash, .. }) = advertised.prospective_candidate {
		if candidate_hash != fetched.hash() {
			return Err(SecondingError::CandidateHashMismatch)
		}
	// Otherwise, do the explicit check for the paraid.
	} else if advertised.para_id != fetched.descriptor.para_id() {
		return Err(SecondingError::ParaIdMismatch)
	}

	if advertised.relay_parent != fetched.descriptor.relay_parent() {
		return Err(SecondingError::RelayParentMismatch)
	}

	Ok(())
}

// Sanity check the candidate descriptor version.
fn descriptor_version_sanity_check(
	descriptor: &CandidateDescriptor,
	v2_receipts: bool,
	per_relay_parent: &PerRelayParent,
) -> std::result::Result<(), SecondingError> {
	match descriptor.version() {
		CandidateDescriptorVersion::V1 => Ok(()),
		CandidateDescriptorVersion::V2 if v2_receipts => {
			if let Some(core_index) = descriptor.core_index() {
				if core_index != per_relay_parent.core_index {
					return Err(SecondingError::InvalidCoreIndex(
						core_index.0,
						per_relay_parent.core_index.0,
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

async fn fetch_pvd<Sender: CollatorProtocolSenderTrait>(
	sender: &mut Sender,
	receipt: &CandidateReceipt,
	maybe_parent_head_data_hash: Option<Hash>,
	maybe_parent_head_data: Option<HeadData>,
) -> std::result::Result<PersistedValidationData, SecondingError> {
	let para_id = receipt.descriptor.para_id();

	let pvd = match maybe_parent_head_data_hash {
		Some(parent_head_data_hash) => {
			let maybe_pvd = request_prospective_validation_data(
				sender,
				receipt.descriptor.relay_parent(),
				parent_head_data_hash,
				para_id,
				maybe_parent_head_data.clone(),
			)
			.await?;

			let pvd = match (maybe_pvd, &maybe_parent_head_data) {
				(None, None) => return Err(SecondingError::BlockedOnParent(parent_head_data_hash)),
				(Some(pvd), None) => {
					if parent_head_data_hash != pvd.parent_head.hash() {
						return Err(SecondingError::ParentHeadDataMismatch)
					}
					pvd
				},
				(Some(pvd), Some(parent_head)) => {
					if parent_head.hash() != parent_head_data_hash {
						return Err(SecondingError::ParentHeadDataMismatch)
					}
					pvd
				},
				(None, _) => return Err(SecondingError::PersistedValidationDataNotFound),
			};

			pvd
		},
		None => {
			let pvd = request_persisted_validation_data(
				sender,
				receipt.descriptor.relay_parent(),
				para_id,
			)
			.await?;
			pvd.ok_or(SecondingError::PersistedValidationDataNotFound)?
		},
	};

	if pvd.hash() != receipt.descriptor.persisted_validation_data_hash() {
		return Err(SecondingError::PersistedValidationDataMismatch)
	}

	Ok(pvd)
}

fn process_collation_fetch_result(
	(advertisement, res): CollationFetchResponse,
) -> std::result::Result<FetchedCollation, Option<Score>> {
	match res {
		Err(CollationFetchError::Cancelled) => {
			// Was cancelled by the subsystem.
			Err(None)
		},
		Err(CollationFetchError::Request(RequestError::InvalidResponse(err))) => {
			gum::warn!(
				target: LOG_TARGET,
				?advertisement,
				err = ?err,
				"Collator provided response that could not be decoded"
			);
			Err(Some(FAILED_FETCH_SLASH))
		},
		Err(CollationFetchError::Request(err)) if err.is_timed_out() => {
			gum::debug!(
				target: LOG_TARGET,
				?advertisement,
				"Request timed out"
			);
			Err(Some(FAILED_FETCH_SLASH))
		},
		Err(CollationFetchError::Request(RequestError::NetworkError(err))) => {
			gum::warn!(
				target: LOG_TARGET,
				?advertisement,
				err = ?err,
				"Fetching collation failed due to network error"
			);
			Err(None)
		},
		Err(CollationFetchError::Request(RequestError::Canceled(err))) => {
			gum::warn!(
				target: LOG_TARGET,
				?advertisement,
				err = ?err,
				"Canceled should be handled by `is_timed_out` above - this is a bug!"
			);
			Err(Some(FAILED_FETCH_SLASH))
		},
		Ok(request_v2::CollationFetchingResponse::Collation(candidate_receipt, pov)) => {
			gum::debug!(
				target: LOG_TARGET,
				?advertisement,
				"Received collation",
			);

			Ok(FetchedCollation {
				candidate_receipt,
				pov,
				peer_id: advertisement.peer_id,
				maybe_parent_head_data: None,
				maybe_parent_head_data_hash: advertisement
					.prospective_candidate
					.map(|p| p.parent_head_data_hash),
			})
		},
		Ok(request_v2::CollationFetchingResponse::CollationWithParentHeadData {
			receipt,
			pov,
			parent_head_data,
		}) => {
			gum::debug!(
				target: LOG_TARGET,
				?advertisement,
				"Received collation with parent head data",
			);

			Ok(FetchedCollation {
				candidate_receipt: receipt,
				pov,
				peer_id: advertisement.peer_id,
				maybe_parent_head_data: Some(parent_head_data),
				maybe_parent_head_data_hash: advertisement
					.prospective_candidate
					.map(|p| p.parent_head_data_hash),
			})
		},
	}
}
