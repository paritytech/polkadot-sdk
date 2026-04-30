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
	extract_leaf_scheduling_info, is_scheduling_parent_valid,
	validator_side::{
		descriptor_version_sanity_check_with_params, error::SecondingError,
		request_persisted_validation_data, request_prospective_validation_data, BlockedCollationId,
	},
	validator_side_experimental::{
		common::{
			Advertisement, CanSecond, CollationFetchError, CollationFetchResponse,
			ProspectiveCandidate, Score, SecondingRejectionInfo, FAILED_FETCH_SLASH,
			INSTANT_FETCH_REP_THRESHOLD, MAX_FETCH_DELAY,
		},
		error::{Error, FatalResult, Result},
	},
	LeafSchedulingInfo, LOG_TARGET,
};
use fatality::Split;
use futures::{channel::oneshot, stream::FusedStream};
use polkadot_node_network_protocol::{
	peer_set::CollationVersion,
	request_response::{outgoing::RequestError, v2 as request_v2, Requests},
	OurView, PeerId,
};
use polkadot_node_primitives::PoV;
use polkadot_node_subsystem::{
	messages::{CanSecondRequest, CandidateBackingMessage},
	ActivatedLeaf, CollatorProtocolSenderTrait,
};
use polkadot_node_subsystem_util::{
	backing_implicit_view::View as ImplicitView, metrics::prometheus::prometheus::HistogramTimer,
	request_claim_queue, request_session_index_for_child, request_validator_groups,
	request_validators, runtime::recv_runtime,
};
use polkadot_primitives::{
	CandidateDescriptorVersion, CandidateHash, CandidateReceiptV2 as CandidateReceipt, CoreIndex,
	GroupIndex, GroupRotationInfo, Hash, HeadData, Id as ParaId, PersistedValidationData,
	SessionIndex,
};
use requests::PendingRequests;
use schnellru::{ByLength, LruMap};
use sp_keystore::KeystorePtr;
use sp_runtime::Either;
use std::{
	collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
	time::{Duration, Instant},
};

mod requests;

/// Reason for rejecting an advertisement.
#[derive(Debug, thiserror::Error)]
pub enum AdvertisementError {
	#[error("Validator is not assigned to this paraid")]
	InvalidAssignment,
	#[error("Duplicate advertisement")]
	Duplicate,
	#[error("Advertised scheduling parent is out of our view")]
	OutOfOurView,
	#[error("Peer reached the candidate limit")]
	PeerLimitReached,
	#[error("Seconding not allowed by backing subsystem")]
	BlockedByBacking,
	#[error("V1 advertisements are only allowed on active leaves")]
	V1AdvertisementForImplicitParent,
	#[error("For V3 candidate descriptors, scheduling_parent does not match any expected scheduling parent.")]
	SchedulingParentNotValid,
}

pub struct CollationManager {
	// The backing implicit view, which is used to track the active leaves and their implicit
	// ancestors.
	implicit_view: ImplicitView,

	// The full claim queue per core for each active leaf, fetched once per leaf via
	// `request_claim_queue`. This is the authoritative CQ — it's what the runtime will see
	// when candidates get backed on-chain — so all capacity reasoning routes through it.
	// Per-scheduling-parent capacity is derived via offset arithmetic from the leaf's CQ
	// (`unfulfilled_claim_queue_entries`, `is_slot_available`).
	leaf_claim_queues: HashMap<Hash, BTreeMap<CoreIndex, VecDeque<ParaId>>>,

	// Collations which we haven't been able to second due to their parent not being known by
	// prospective-parachains. Mapped from the para_id and parent_head_hash to the fetched
	// collation data. Only needed for async backing. For elastic scaling, the fetched collation
	// must contain the full parent head data.
	blocked_from_seconding: HashMap<BlockedCollationId, Vec<FetchedCollation>>,

	// Information kept per scheduling parent.
	per_scheduling_parent: HashMap<Hash, PerSchedulingParent>,

	// Session info cache.
	per_session: LruMap<SessionIndex, PerSessionInfo>,

	// Collection of active collation fetch requests.
	fetching: PendingRequests,

	// Key store.
	keystore: KeystorePtr,
	leaf_scheduling_info: HashMap<Hash, LeafSchedulingInfo>,
}

impl CollationManager {
	pub async fn new<Sender: CollatorProtocolSenderTrait>(
		sender: &mut Sender,
		keystore: KeystorePtr,
		active_leaf: ActivatedLeaf,
	) -> FatalResult<Self> {
		let mut instance = Self {
			implicit_view: ImplicitView::new(),
			leaf_claim_queues: HashMap::new(),
			per_scheduling_parent: HashMap::new(),
			blocked_from_seconding: HashMap::new(),
			per_session: LruMap::new(ByLength::new(2)),
			fetching: PendingRequests::default(),
			keystore,
			leaf_scheduling_info: HashMap::default(),
		};

		instance.update_view(sender, OurView::new([active_leaf.hash], 0)).await?;

		Ok(instance)
	}

	pub async fn update_view<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
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

		gum::trace!(
			target: LOG_TARGET,
			?added,
			?removed,
			"CollationManager: Processing view update"
		);

		for leaf in added.iter() {
			match extract_leaf_scheduling_info(sender, *leaf).await {
				Some(info) => {
					self.leaf_scheduling_info.insert(*leaf, info);
				},
				None => {
					gum::warn!(
						target: LOG_TARGET,
						?leaf,
						"Could not extract BABE slot from leaf header; \
						 V3 scheduling parent validation will reject advertisements \
						 referencing this leaf",
					);
				},
			}

			if let Err(err) = self
				.implicit_view
				.activate_leaf(sender, *leaf)
				.await
				.map_err(Error::FailedToActivateLeafInImplicitView)
			{
				err.split()?.log();
				continue;
			}
		}

		for leaf in removed {
			self.implicit_view.deactivate_leaf(leaf);
			self.leaf_scheduling_info.remove(&leaf);
			self.leaf_claim_queues.remove(&leaf);
		}

		// Rebuild `per_scheduling_parent`, dropping entries no longer reachable from any
		// current leaf and cancelling their in-flight fetches.
		self.per_scheduling_parent = std::mem::take(&mut self.per_scheduling_parent)
			.into_iter()
			.filter_map(|(sp, per_sp)| {
				if !self.implicit_view.paths_via_relay_parent(&sp).is_empty() {
					return Some((sp, per_sp));
				}
				for advertisement in per_sp.all_advertisements() {
					self.fetching.cancel(advertisement);
				}
				None
			})
			.collect();

		// Remove blocked seconding requests whose scheduling parent is no longer tracked.
		self.blocked_from_seconding.retain(|_, collations| {
			collations.retain(|c| self.per_scheduling_parent.contains_key(&c.scheduling_parent()));
			!collations.is_empty()
		});

		for leaf in added.iter() {
			let Some(allowed_ancestry) =
				self.implicit_view.known_allowed_relay_parents_under(leaf).map(|v| v.to_vec())
			else {
				continue;
			};
			let session_index =
				match recv_runtime(request_session_index_for_child(*leaf, sender).await)
					.await
					.map_err(Error::Runtime)
				{
					Ok(session_index) => session_index,
					Err(err) => {
						err.split()?.log();
						continue;
					},
				};

			// Register every newly-known scheduling parent (the leaf and any of its allowed
			// ancestors not yet in our view) with the core our group is assigned to *at that
			// block*. This is what determines which core's view of the leaf's CQ applies to
			// advertisements rooted at that scheduling parent.
			for ancestor in allowed_ancestry.iter() {
				if self.per_scheduling_parent.contains_key(ancestor) {
					continue;
				}

				let core = match self.get_our_core(sender, ancestor, session_index).await {
					Ok(core) => core,
					Err(err) => {
						err.split()?.log();
						continue;
					},
				};
				self.per_scheduling_parent
					.insert(*ancestor, PerSchedulingParent::new(session_index, core));
			}

			// Fetch and store the leaf's full per-core claim queue. Capacity at every
			// scheduling parent on a path to this leaf is computed from this CQ via offset
			// arithmetic — the leaf is authoritative because it's closest to what the runtime
			// will see when candidates get backed.
			let claim_queues = recv_runtime(request_claim_queue(*leaf, sender).await).await?;
			self.leaf_claim_queues.insert(*leaf, claim_queues);
		}

		Ok(())
	}

	pub fn response_stream(&mut self) -> &mut impl FusedStream<Item = CollationFetchResponse> {
		self.fetching.response_stream()
	}

	/// All paras our group will back at *some* scheduling parent in our view. Used to decide
	/// which collators we should be willing to talk to. We take the union across all
	/// scheduling parents of `our_window(sp)` — the slice of the leaf's CQ visible from that
	/// SP for our core.
	pub fn assignments(&self) -> BTreeSet<ParaId> {
		self.per_scheduling_parent
			.iter()
			.flat_map(|(sp, per_sp)| self.our_window(sp, per_sp.core_index))
			.collect()
	}

	/// Returns `Ok(())` if `para_id` is reachable at `scheduling_parent` — i.e. it's in the SP's
	/// visible window of the leaf's CQ for our core (see `our_window`).
	///
	/// This is just an "is this advertisement valid at this scheduling parent" gate; capacity
	/// is enforced separately in `try_make_new_fetch_requests` via
	/// `unfulfilled_claim_queue_entries`. Accepting an advertisement that won't be fetchable
	/// right away is fine — it stays parked in `peer_advertisements` until a slot opens up.
	fn is_slot_available(
		&self,
		scheduling_parent: &Hash,
		para_id: ParaId,
	) -> std::result::Result<(), AdvertisementError> {
		let Some(per_sp) = self.per_scheduling_parent.get(scheduling_parent) else {
			return Err(AdvertisementError::OutOfOurView);
		};
		if self.our_window(scheduling_parent, per_sp.core_index).contains(&para_id) {
			Ok(())
		} else {
			Err(AdvertisementError::InvalidAssignment)
		}
	}

	/// Returns the paras with unfulfilled CQ entries at `scheduling_parent`, in claim queue
	/// order. One entry per available position (so a para with three free slots appears three
	/// times).
	///
	/// Each para's budget is its count of CQ positions visible at this SP minus its in-flight
	/// count here.
	fn unfulfilled_claim_queue_entries(
		&self,
		scheduling_parent: &Hash,
	) -> impl Iterator<Item = ParaId> {
		let window = match self.per_scheduling_parent.get(scheduling_parent) {
			Some(per_sp) => self.our_window(scheduling_parent, per_sp.core_index),
			None => Vec::new(),
		};

		let mut remaining: HashMap<ParaId, usize> = HashMap::new();
		for para in &window {
			*remaining.entry(*para).or_default() += 1;
		}
		for (para, n) in remaining.iter_mut() {
			*n = n.saturating_sub(self.consumed_for_para(scheduling_parent, *para));
		}

		// Walk the CQ in order, emitting one entry per still-unfulfilled position.
		window.into_iter().filter(move |para| {
			let n = remaining.get_mut(para).expect("inserted above; qed");
			if *n > 0 {
				*n -= 1;
				true
			} else {
				false
			}
		})
	}

	/// Candidates of `para_id` consuming a slot at `scheduling_parent`: in-flight fetches plus
	/// fetched ones (seconded or blocked-on-parent — both stay in `fetched_collations` until
	/// release).
	fn consumed_for_para(&self, scheduling_parent: &Hash, para_id: ParaId) -> usize {
		let Some(per_sp) = self.per_scheduling_parent.get(scheduling_parent) else {
			return 0;
		};

		let fetching = self
			.fetching
			.iter()
			.filter(|adv| adv.scheduling_parent == *scheduling_parent && adv.para_id == para_id)
			.count();

		let fetched = per_sp
			.fetched_collations
			.values()
			.filter(|info| info.para_id == para_id)
			.count();

		fetching + fetched
	}

	pub async fn try_accept_advertisement<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		scheduling_parent: Hash,
		para_id: ParaId,
		peer_id: PeerId,
		prospective_candidate: Option<ProspectiveCandidate>,
		advertised_descriptor_version: Option<CandidateDescriptorVersion>,
	) -> std::result::Result<(), AdvertisementError> {
		// V1 advertisements are only allowed on active leaves.
		if prospective_candidate.is_none() && !self.implicit_view.contains_leaf(&scheduling_parent)
		{
			return Err(AdvertisementError::V1AdvertisementForImplicitParent);
		}

		// V3 candidate descriptors require scheduling_parent to be the block from the last
		// finished relay chain slot.
		if advertised_descriptor_version == Some(CandidateDescriptorVersion::V3) &&
			!is_scheduling_parent_valid(&scheduling_parent, &self.leaf_scheduling_info)
		{
			return Err(AdvertisementError::SchedulingParentNotValid);
		}

		// Para must be schedulable from this scheduling parent — i.e. it appears in our
		// core's CQ within the SP's visible window.
		self.is_slot_available(&scheduling_parent, para_id)?;

		// Per-peer rate limit: we cap a peer at one advertisement per CQ slot allocated to
		// `para_id` in our view of `scheduling_parent`. A peer can't fulfil more slots than
		// exist for its para anyway, so anything beyond that is spam.
		let spam_limit = self.cq_slot_count(&scheduling_parent, para_id);

		let Some(per_sp) = self.per_scheduling_parent.get_mut(&scheduling_parent) else {
			return Err(AdvertisementError::OutOfOurView);
		};

		if let Some(ProspectiveCandidate { candidate_hash, .. }) = prospective_candidate {
			if per_sp.fetched_collations.contains_key(&candidate_hash) {
				return Err(AdvertisementError::Duplicate);
			}
		}

		let advertisement = Advertisement {
			peer_id,
			para_id,
			scheduling_parent,
			prospective_candidate,
			advertised_descriptor_version,
		};

		if self.fetching.contains(&advertisement) {
			return Err(AdvertisementError::Duplicate);
		}

		per_sp.can_keep_advertisement(advertisement, spam_limit)?;

		if !backing_allows_seconding(sender, &advertisement).await {
			return Err(AdvertisementError::BlockedByBacking);
		}

		per_sp.add_advertisement(advertisement, Instant::now());

		Ok(())
	}

	// Per-peer rate limit on advertisements at `scheduling_parent` for `para_id`: the number
	// of CQ positions assigned to that para in this SP's visible window.
	fn cq_slot_count(&self, scheduling_parent: &Hash, para_id: ParaId) -> usize {
		let Some(per_sp) = self.per_scheduling_parent.get(scheduling_parent) else {
			return 0;
		};
		self.our_window(scheduling_parent, per_sp.core_index)
			.iter()
			.filter(|p| **p == para_id)
			.count()
	}

	/// The widest slice of `core`'s claim queue visible from `scheduling_parent`, taken via
	/// the path with the smallest offset (= the shortest descending fork from `sp`, which has
	/// produced the fewest blocks and thus consumed the fewest predicted slots).
	///
	/// At `sp`'s own leaf the window is the full lookahead. At an ancestor `n` blocks back on
	/// some fork, that fork has already produced `n` blocks; their slots are filled and the
	/// remaining window is `cq[..lookahead - n]`. With multiple descending forks we pick the
	/// one with the most slots still in play — different forks have independent backing
	/// futures, so a slot still open on any of them is a real fetch opportunity.
	///
	/// Returns an empty `Vec` if `sp` isn't reachable from any leaf with a CQ for `core`.
	///
	/// We assume CQ entries on shared positions across descending forks agree (the runtime's
	/// scheduling is a per-SP prediction over `lookahead` blocks). Adversarial divergence is
	/// possible only across runtime upgrades and is left to recover on the next view update.
	fn our_window(&self, scheduling_parent: &Hash, core: CoreIndex) -> Vec<ParaId> {
		self.implicit_view
			.paths_via_relay_parent(scheduling_parent)
			.into_iter()
			.filter_map(|path| {
				let leaf = path.last()?;
				let cq = self.leaf_claim_queues.get(leaf)?.get(&core)?;
				let offset = path
					.iter()
					.rev()
					.position(|h| h == scheduling_parent)
					.expect("paths_via_relay_parent only returns paths containing the SP; qed");
				let valid_len = cq.len().saturating_sub(offset);
				Some(cq.iter().take(valid_len).copied().collect::<Vec<_>>())
			})
			.max_by_key(Vec::len)
			.unwrap_or_default()
	}

	pub fn try_make_new_fetch_requests<
		RepQueryFn: Fn(&PeerId, &ParaId) -> Option<Score>,
		TimerFn: FnMut() -> Option<HistogramTimer>,
	>(
		&mut self,
		connected_rep_query_fn: RepQueryFn,
		max_scores: HashMap<ParaId, Score>,
		mut create_timer_fn: TimerFn,
	) -> (Vec<Requests>, Option<Duration>) {
		let now = Instant::now();
		let mut requests = vec![];
		let mut maybe_min_delay = None;

		// Iterate scheduling parents — not leaves — so that pre-rotation ancestors get their
		// own pass with their own core's claim queue. After a group rotation, the leaf's
		// `our_core` differs from the ancestor's `our_core`, and only iterating per scheduling
		// parent surfaces both cores' free slots.
		let scheduling_parents: Vec<_> = self.per_scheduling_parent.keys().copied().collect();
		for sp in scheduling_parents {
			for para_id in self.unfulfilled_claim_queue_entries(&sp) {
				let highest_rep_of_para = max_scores.get(&para_id).copied().unwrap_or_default();

				let advertisement = match self.pick_best_advertisement(
					now,
					sp,
					para_id,
					highest_rep_of_para,
					&connected_rep_query_fn,
				) {
					Either::Left(Some(advertisement)) => advertisement,
					Either::Left(None) => continue,
					Either::Right(delay) => {
						let min_delay = maybe_min_delay.get_or_insert(delay);
						maybe_min_delay = Some(std::cmp::min(*min_delay, delay));
						continue;
					},
				};

				gum::trace!(
					target: LOG_TARGET,
					peer_id = ?advertisement.peer_id,
					?para_id,
					scheduling_parent = ?advertisement.scheduling_parent,
					maybe_candidate_hash = ?advertisement.candidate_hash(),
					"Requesting collation",
				);
				// Adding to `self.fetching` is what reserves the slot — `consumed_for_para`
				// counts in-flight requests, so subsequent capacity checks see this slot
				// taken without an explicit claim step.
				let req = self.fetching.launch(&advertisement, create_timer_fn());
				requests.push(req);
			}
		}

		(requests, maybe_min_delay)
	}

	pub fn remove_peer(&mut self, peer: &PeerId) {
		for per_sp in self.per_scheduling_parent.values_mut() {
			// No need to reset now the statuses of claims that were pending fetch for these
			// candidates, or even cancel the futures as the requests will soon conclude with a
			// network error.
			per_sp.remove_peer_advertisements(peer);
		}
	}

	pub fn remove_peers<'a>(&'a mut self, peers_to_remove: impl Iterator<Item = &'a PeerId>) {
		// Remove advertisements from these peers.
		for peer in peers_to_remove {
			self.remove_peer(peer)
		}
	}

	pub async fn note_fetched<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		res: CollationFetchResponse,
		maybe_collation_version: Option<CollationVersion>,
	) -> CanSecond {
		let advertisement = res.0;
		let mut reject_info = SecondingRejectionInfo::from(&advertisement);

		self.fetching.note_completed(&advertisement);

		let Some(per_sp) = self.per_scheduling_parent.get_mut(&advertisement.scheduling_parent)
		else {
			gum::debug!(
				target: LOG_TARGET,
				hash = ?advertisement.scheduling_parent,
				para_id = ?advertisement.para_id,
				peer_id = ?advertisement.peer_id,
				"Collation fetch concluded for scheduling parent out of view"
			);
			return CanSecond::No(None, reject_info);
		};

		per_sp.remove_advertisement(&advertisement);

		let Some(collation_version) = maybe_collation_version else {
			gum::debug!(
				target: LOG_TARGET,
				?advertisement,
				"Peer may not be connected."
			);
			return CanSecond::No(None, reject_info);
		};

		match process_collation_fetch_result(res) {
			Ok(fetched_collation) => {
				let candidate_hash = fetched_collation.candidate_receipt.hash();
				// It can't be a duplicate, because we check before initiating fetch. For the old
				// protocol version, we anyway only fetch one per scheduling parent.
				per_sp.fetched_collations.insert(
					candidate_hash,
					FetchedCollationInfo {
						peer_id: advertisement.peer_id,
						para_id: advertisement.para_id,
					},
				);

				// Now that the candidate hash is known, populate it on the rejection info so
				// V1 release paths can clean up the right entry too.
				reject_info.maybe_candidate_hash = Some(candidate_hash);
				reject_info.maybe_output_head_hash =
					Some(fetched_collation.candidate_receipt.descriptor.para_head());

				// Some initial sanity checks on the fetched collation, based on the advertisement.
				if let Err(err) = fetched_collation.ensure_matches_advertisement(&advertisement) {
					gum::warn!(
						target: LOG_TARGET,
						?advertisement,
						"Invalid fetched collation: {}",
						err
					);
					return CanSecond::No(Some(FAILED_FETCH_SLASH), reject_info);
				}

				// Sanity check of the candidate receipt version.
				if let Err(err) = descriptor_version_sanity_check_with_params(
					fetched_collation.candidate_receipt.descriptor(),
					per_sp.core_index,
					per_sp.session_index,
					collation_version,
				) {
					gum::warn!(
						target: LOG_TARGET,
						?advertisement,
						"Failed descriptor version sanity check for fetched collation: {}",
						err
					);
					return CanSecond::No(Some(FAILED_FETCH_SLASH), reject_info);
				}

				self.can_begin_seconding(sender, fetched_collation, true, reject_info).await
			},
			Err(rep_change) => CanSecond::No(rep_change, reject_info),
		}
	}

	/// Frees the slot consumed by a previously-fetched candidate. Called when seconding fails
	/// (validation rejected, blocked-on-parent gave up, etc.). After this, capacity at
	/// `scheduling_parent` for `para_id` increases by one.
	///
	/// `maybe_candidate_hash` is `None` only when called for an advertisement that never made
	/// it past acceptance (V1, no descriptor available) — nothing was consumed yet, so
	/// nothing to free.
	pub fn release_slot(
		&mut self,
		scheduling_parent: &Hash,
		para_id: ParaId,
		maybe_candidate_hash: Option<&CandidateHash>,
		maybe_output_head_hash: Option<Hash>,
	) {
		if let Some(candidate_hash) = maybe_candidate_hash {
			let removed = self
				.per_scheduling_parent
				.get_mut(scheduling_parent)
				.and_then(|per_sp| per_sp.fetched_collations.remove(candidate_hash))
				.is_some();
			if !removed {
				gum::debug!(
					target: LOG_TARGET,
					?scheduling_parent,
					?candidate_hash,
					?para_id,
					"Could not release slot for candidate, it wasn't fetched",
				);
			}
		}

		if let Some(output_head_hash) = maybe_output_head_hash {
			// Remove any collations that were blocked on this parent.
			self.remove_blocked_collations(BlockedCollationId {
				para_id,
				parent_head_data_hash: output_head_hash,
			});
		}
	}

	pub fn get_fetched_collation_peer_id(
		&self,
		scheduling_parent: &Hash,
		candidate_hash: &CandidateHash,
	) -> Option<&PeerId> {
		self.per_scheduling_parent
			.get(scheduling_parent)
			.and_then(|per_sp| per_sp.fetched_collations.get(candidate_hash))
			.map(|info| &info.peer_id)
	}

	pub async fn note_seconded<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		scheduling_parent: &Hash,
		para_id: &ParaId,
		candidate_hash: &CandidateHash,
		output_head_hash: Hash,
	) -> (Option<PeerId>, Vec<CanSecond>) {
		let peer_id =
			self.get_fetched_collation_peer_id(scheduling_parent, candidate_hash).copied();

		// The slot stays consumed by virtue of the candidate remaining in `fetched_collations`
		// — no separate per-slot bookkeeping is needed. We just check whether seconding this
		// candidate has unblocked others that were waiting on it.
		let Some(unblocked) = self.blocked_from_seconding.remove(&BlockedCollationId {
			para_id: *para_id,
			parent_head_data_hash: output_head_hash,
		}) else {
			return (peer_id, vec![]);
		};

		let mut unblocked_can_second = Vec::with_capacity(unblocked.len());
		for fetched_collation in unblocked {
			let reject_info = SecondingRejectionInfo {
				scheduling_parent: fetched_collation.scheduling_parent(),
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

		(peer_id, unblocked_can_second)
	}

	// Returns max delay for unknown collators and zero delay if the collator has provided at least
	// one good collation (it's score is >= INSTANT_FETCH_REP_THRESHOLD).
	fn calculate_delay(collator_score: Score, max_score_for_para: Score) -> Duration {
		if collator_score >= INSTANT_FETCH_REP_THRESHOLD || collator_score >= max_score_for_para {
			return Duration::ZERO;
		}

		MAX_FETCH_DELAY
	}

	/// Picks the best (= highest-scored, earliest, in that order) advertisement at
	/// `scheduling_parent` for `para_id` whose delay has elapsed.
	///
	/// Returns:
	/// - `Either::Left(Some(adv))` if a fetchable advertisement was found,
	/// - `Either::Left(None)` if there are no eligible advertisements,
	/// - `Either::Right(delay)` if the best advertisement still has remaining fetch delay relative
	///   to the scheduling parent's activation time.
	fn pick_best_advertisement<RepQueryFn: Fn(&PeerId, &ParaId) -> Option<Score>>(
		&self,
		now: Instant,
		scheduling_parent: Hash,
		para_id: ParaId,
		highest_rep_of_para: Score,
		connected_rep_query_fn: &RepQueryFn,
	) -> Either<Option<Advertisement>, Duration> {
		let Some(per_sp) = self.per_scheduling_parent.get(&scheduling_parent) else {
			return Either::Left(None);
		};
		// V1 advertisements are only fetchable when the scheduling parent is itself an active
		// leaf — V1 has no candidate hash, so the advertisement is only meaningful at the
		// block it was advertised against.
		let is_active_leaf = self.implicit_view.contains_leaf(&scheduling_parent);

		// V1 has no candidate identity, so we can fetch at most one V1 collation per (sp, para).
		// Skip V1 ads when a V1 fetch for the same (sp, para) is already in flight.
		let v1_fetching = self.fetching.iter().any(|adv| {
			adv.scheduling_parent == scheduling_parent &&
				adv.para_id == para_id &&
				adv.prospective_candidate.is_none()
		});

		let advertisements = per_sp
			.eligible_advertisements(para_id, is_active_leaf)
			.filter(|(adv, _)| !self.fetching.contains(adv))
			.filter(|(adv, _)| !(adv.prospective_candidate.is_none() && v1_fetching))
			.filter_map(|(adv, timestamp)| {
				Some(AcceptedAdvertisement {
					adv,
					score: connected_rep_query_fn(&adv.peer_id, &adv.para_id)?,
					timestamp,
					activated_at: per_sp.activated_at,
				})
			})
			.collect::<BTreeSet<_>>();

		let Some(best) = advertisements.first() else { return Either::Left(None) };

		let delay = Self::calculate_delay(best.score, highest_rep_of_para);

		// Delay is relative to the scheduling parent's activation, not advertisement arrival —
		// once the SP has been active long enough, even unknown peers' delays elapse and we
		// fetch immediately.
		let elapsed = now.duration_since(best.activated_at);
		let remaining = delay.saturating_sub(elapsed);

		if remaining.is_zero() {
			gum::debug!(
				target: LOG_TARGET,
				peer_id = ?best.adv.peer_id,
				scheduling_parent = ?best.adv.scheduling_parent,
				para_id = ?best.adv.para_id,
				?elapsed,
				?delay,
				"Delay elapsed; initiating fetch."
			);
			Either::Left(Some(*best.adv))
		} else {
			Either::Right(remaining)
		}
	}

	async fn get_our_core<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		parent: &Hash,
		session_index: SessionIndex,
	) -> Result<CoreIndex> {
		let block_number = self
			.implicit_view
			.block_number(parent)
			.ok_or_else(|| Error::BlockNumberNotFoundInImplicitView(*parent))?;
		let session_info = self.get_session_info(sender, parent, session_index).await?;

		Ok(match session_info.our_group {
			Some(group) => {
				let mut rotation_info = session_info.group_rotation_info.clone();
				// The `validator_groups` runtime API adds 1 to the block number, so we need to do
				// the same here.
				rotation_info.now = block_number + 1;
				rotation_info.core_for_group(group, session_info.n_cores)
			},
			None => {
				gum::trace!(target: LOG_TARGET, ?parent, "Not a validator");
				Default::default()
			},
		})
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

			let our_group =
				polkadot_node_subsystem_util::signing_key_and_index(&validators, &self.keystore)
					.and_then(|(_, index)| {
						polkadot_node_subsystem_util::find_validator_group(&groups, index)
					});

			self.per_session.insert(
				index,
				PerSessionInfo { our_group, n_cores: groups.len(), group_rotation_info },
			);
		}

		Ok(self.per_session.get(&index).expect("Just inserted"))
	}

	async fn can_begin_seconding<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		fetched_collation: FetchedCollation,
		queue_blocked_collations: bool,
		reject_info: SecondingRejectionInfo,
	) -> CanSecond {
		let scheduling_parent = fetched_collation.scheduling_parent();
		let candidate_hash = fetched_collation.candidate_receipt.hash();
		let para_id = fetched_collation.candidate_receipt.descriptor.para_id();

		match fetch_pvd(
			sender,
			&fetched_collation.candidate_receipt,
			fetched_collation.maybe_parent_head_data_hash,
			fetched_collation.maybe_parent_head_data.clone(),
		)
		.await
		{
			Ok(pvd) => {
				CanSecond::Yes(fetched_collation.candidate_receipt, fetched_collation.pov, pvd)
			},
			Err(SecondingError::BlockedOnParent(parent)) => {
				gum::debug!(
					target: LOG_TARGET,
					?candidate_hash,
					?scheduling_parent,
					?para_id,
					"Collation with parent head data hash {} is blocked from seconding. \
					 Waiting on its parent to be validated.",
					parent,
				);

				if queue_blocked_collations {
					self.blocked_from_seconding
						.entry(BlockedCollationId { para_id, parent_head_data_hash: parent })
						.or_default()
						.push(fetched_collation);
				}

				CanSecond::BlockedOnParent(parent, reject_info)
			},
			Err(err) => {
				gum::warn!(
					target: LOG_TARGET,
					?candidate_hash,
					?scheduling_parent,
					?para_id,
					"Failed persisted validation data checks: {}",
					err,
				);

				let slash = err.is_malicious().then_some(FAILED_FETCH_SLASH);
				CanSecond::No(slash, reject_info)
			},
		}
	}

	fn remove_blocked_collations(&mut self, id: BlockedCollationId) {
		let Some(blocked) = self.blocked_from_seconding.remove(&id) else { return };

		for collation in blocked {
			let candidate_hash = collation.candidate_receipt.hash();
			let scheduling_parent = collation.scheduling_parent();
			gum::debug!(
				target: LOG_TARGET,
				?scheduling_parent,
				?candidate_hash,
				para_id = ?id.para_id,
				parent_head_hash = ?id.parent_head_data_hash,
				"Dropping blocked collation because its parent was released",
			);
			if let Some(per_sp) = self.per_scheduling_parent.get_mut(&scheduling_parent) {
				per_sp.fetched_collations.remove(&candidate_hash);
			}
		}
	}

	#[cfg(test)]
	pub fn advertisements(&self) -> BTreeSet<Advertisement> {
		self.per_scheduling_parent
			.values()
			.flat_map(|per_sp| {
				per_sp
					.peer_advertisements
					.values()
					.flat_map(|peer_adv| peer_adv.advertisements.keys().cloned())
			})
			.collect()
	}
}

/// Fetched collation data.
#[derive(Debug, Clone)]
struct FetchedCollation {
	/// Candidate receipt.
	pub candidate_receipt: CandidateReceipt,
	/// Proof of validity.
	pub pov: PoV,
	/// Optional parachain parent head data. This is needed for elastic scaling to work.
	pub maybe_parent_head_data: Option<HeadData>,
	/// Optional parent head data hash. This is needed for async backing to work (sent by v2
	/// protocol).
	pub maybe_parent_head_data_hash: Option<Hash>,
	/// The peer that sent this collation.
	pub peer_id: PeerId,
}

impl FetchedCollation {
	pub fn new(
		candidate_receipt: CandidateReceipt,
		pov: PoV,
		maybe_parent_head_data: Option<HeadData>,
		maybe_parent_head_data_hash: Option<Hash>,
		peer_id: PeerId,
	) -> Self {
		Self {
			candidate_receipt,
			pov,
			maybe_parent_head_data,
			maybe_parent_head_data_hash,
			peer_id,
		}
	}

	pub fn scheduling_parent(&self) -> Hash {
		self.candidate_receipt.descriptor().scheduling_parent()
	}

	/// Performs a sanity check between advertised and fetched collations.
	fn ensure_matches_advertisement(
		&self,
		advertised: &Advertisement,
	) -> std::result::Result<(), SecondingError> {
		let candidate_receipt = &self.candidate_receipt;

		match advertised.prospective_candidate {
			// This implies a check on the declared para if this was a v2 advertisement
			Some(ProspectiveCandidate { candidate_hash, .. }) => {
				if candidate_hash != candidate_receipt.hash() {
					return Err(SecondingError::CandidateHashMismatch);
				}
			},
			// Otherwise, do the explicit check for the para_id.
			None => {
				if advertised.para_id != candidate_receipt.descriptor.para_id() {
					return Err(SecondingError::ParaIdMismatch);
				}
			},
		}

		if advertised.scheduling_parent != candidate_receipt.descriptor.scheduling_parent() {
			return Err(SecondingError::SchedulingParentMismatch);
		}
		if let Some(advertised_version) = &advertised.advertised_descriptor_version {
			let fetched_version = candidate_receipt.descriptor().version();
			if advertised_version != &fetched_version {
				return Err(SecondingError::DescriptorVersionMismatch(
					*advertised_version,
					fetched_version,
				));
			}
		}

		Ok(())
	}
}

/// Represents an advertisement which we have accepted. Supports ordering of the advertisements.
///
/// Ordering priority: score (descending), then timestamp (ascending), then advertisement as
/// tiebreaker. Higher scores come first so that `BTreeSet::first()` returns the best advertisement.
#[derive(PartialEq, Eq)]
struct AcceptedAdvertisement<'a> {
	adv: &'a Advertisement,
	score: Score,
	timestamp: &'a Instant,
	/// The time at which the scheduling parent was activated
	activated_at: Instant,
}

impl<'a> Ord for AcceptedAdvertisement<'a> {
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		other
			.score
			.cmp(&self.score) // Descending: higher score comes first
			.then_with(|| self.timestamp.cmp(other.timestamp)) // Ascending: earlier timestamp comes first
			.then_with(|| self.adv.cmp(other.adv))
	}
}

impl<'a> PartialOrd for AcceptedAdvertisement<'a> {
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		Some(self.cmp(other))
	}
}

struct FetchedCollationInfo {
	peer_id: PeerId,
	para_id: ParaId,
}

struct PerSchedulingParent {
	peer_advertisements: HashMap<PeerId, PeerAdvertisements>,
	// Candidates we have successfully fetched at this scheduling parent. Kept until the
	// scheduling parent leaves view, so that:
	// - duplicate advertisements are rejected (`try_accept_advertisement`),
	// - we know who to punish for supplying an invalid collation
	//   (`get_fetched_collation_peer_id`),
	// - and capacity tracking knows which slots are consumed (`consumed_for_para`).
	// On rejection (validation failure, blocked-on-parent timeout, etc.) entries are removed.
	fetched_collations: HashMap<CandidateHash, FetchedCollationInfo>,
	session_index: SessionIndex,
	// The core our group is assigned to at this scheduling parent. We look this up once at
	// activation (group rotation is per-block) and keep it for the lifetime of this SP.
	core_index: CoreIndex,
	// The time at which this scheduling parent was activated. Used to calculate fetch delays
	// relative to leaf activation.
	activated_at: Instant,
}

impl PerSchedulingParent {
	fn new(session_index: SessionIndex, core_index: CoreIndex) -> Self {
		Self {
			session_index,
			core_index,
			peer_advertisements: Default::default(),
			fetched_collations: Default::default(),
			activated_at: Instant::now(),
		}
	}

	fn all_advertisements(&self) -> impl Iterator<Item = &Advertisement> {
		self.peer_advertisements.values().flat_map(|adv| adv.advertisements.keys())
	}

	fn eligible_advertisements<'a>(
		&'a self,
		para_id: ParaId,
		is_active_leaf: bool,
	) -> impl Iterator<Item = (&'a Advertisement, &'a Instant)> {
		// V1 (no candidate hash) is single-shot per (sp, para): once any V1 collation has been
		// fetched at this SP for `para_id`, we can't fetch another since V1 has no candidate
		// identity to differentiate them.
		let v1_already_fetched =
			self.fetched_collations.values().any(|info| info.para_id == para_id);
		self.peer_advertisements.values().flat_map(|list| &list.advertisements).filter(
			move |(adv, _)| {
				if adv.para_id != para_id {
					return false;
				}
				match adv.prospective_candidate {
					None => is_active_leaf && !v1_already_fetched,
					Some(p) => !self.fetched_collations.contains_key(&p.candidate_hash),
				}
			},
		)
	}

	fn can_keep_advertisement(
		&mut self,
		advertisement: Advertisement,
		max_assignments: usize,
	) -> std::result::Result<(), AdvertisementError> {
		let peer_advertisements =
			self.peer_advertisements.entry(advertisement.peer_id).or_default();

		// we count all advertisements, check [`PeerAdvertisements::total`]
		peer_advertisements.total += 1;

		if peer_advertisements.total > max_assignments {
			return Err(AdvertisementError::PeerLimitReached);
		}

		if peer_advertisements.advertisements.contains_key(&advertisement) {
			return Err(AdvertisementError::Duplicate);
		}

		Ok(())
	}

	fn add_advertisement(&mut self, advertisement: Advertisement, now: Instant) {
		self.peer_advertisements
			.entry(advertisement.peer_id)
			.or_default()
			.advertisements
			.insert(advertisement, now);
	}

	fn remove_advertisement(&mut self, advertisement: &Advertisement) {
		if let Some(advertisements) = self.peer_advertisements.get_mut(&advertisement.peer_id) {
			advertisements.advertisements.remove(&advertisement);
		}
	}

	fn remove_peer_advertisements(&mut self, peer_id: &PeerId) {
		self.peer_advertisements.remove(peer_id);
	}
}

#[derive(Default)]
struct PeerAdvertisements {
	advertisements: HashMap<Advertisement, Instant>,
	// We increment this even for advertisements that we don't end up accepting, so that we take
	// these into account when rate limiting.
	total: usize,
}

struct PerSessionInfo {
	our_group: Option<GroupIndex>,
	n_cores: usize,
	// The group rotation info changes once per session, apart from the `now` field. The caller
	// must ensure to override it with the right value.
	group_rotation_info: GroupRotationInfo,
}

// Requests backing subsystem to sanity check the advertisement.
async fn backing_allows_seconding<Sender>(
	sender: &mut Sender,
	advertisement: &Advertisement,
) -> bool
where
	Sender: CollatorProtocolSenderTrait,
{
	let Some(prospective_candidate) = advertisement.prospective_candidate else {
		// Nothing to check for v1 protocol.
		return true;
	};

	let request = CanSecondRequest {
		candidate_para_id: advertisement.para_id,
		candidate_scheduling_parent: advertisement.scheduling_parent,
		candidate_hash: prospective_candidate.candidate_hash,
		parent_head_data_hash: prospective_candidate.parent_head_data_hash,
	};
	let (tx, rx) = oneshot::channel();
	sender.send_message(CandidateBackingMessage::CanSecond(request, tx)).await;

	rx.await.unwrap_or_else(|err| {
		gum::warn!(
			target: LOG_TARGET,
			?err,
			scheduling_parent = ?advertisement.scheduling_parent,
			para_id = ?advertisement.para_id,
			candidate_hash = ?prospective_candidate.candidate_hash,
			"CanSecond-request responder was dropped",
		);

		false
	})
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

			let (expected_hash, pvd) = match (maybe_pvd, &maybe_parent_head_data) {
				(Some(pvd), Some(parent_head)) => (parent_head.hash(), pvd),
				(Some(pvd), None) => (pvd.parent_head.hash(), pvd),
				(None, None) => return Err(SecondingError::BlockedOnParent(parent_head_data_hash)),
				(None, _) => return Err(SecondingError::PersistedValidationDataNotFound),
			};
			if parent_head_data_hash != expected_hash {
				return Err(SecondingError::ParentHeadDataMismatch);
			}
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
		return Err(SecondingError::PersistedValidationDataMismatch);
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

			Ok(FetchedCollation::new(
				candidate_receipt,
				pov,
				None,
				advertisement.prospective_candidate.map(|p| p.parent_head_data_hash),
				advertisement.peer_id,
			))
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

			Ok(FetchedCollation::new(
				receipt,
				pov,
				Some(parent_head_data),
				advertisement.prospective_candidate.map(|p| p.parent_head_data_hash),
				advertisement.peer_id,
			))
		},
	}
}

#[cfg(test)]
mod tests {
	use crate::validator_side_experimental::common::MAX_SCORE;

	use super::*;
	use std::sync::Arc;

	#[test]
	fn calculate_delay_works() {
		let score = |val: u16| Score::new(val);

		// collator score == max score => zero delay
		assert_eq!(
			CollationManager::calculate_delay(score(MAX_SCORE), score(MAX_SCORE)),
			Duration::ZERO
		);

		// collator score >= INSTANT_FETCH_REP_THRESHOLD => zero delay
		assert_eq!(
			CollationManager::calculate_delay(INSTANT_FETCH_REP_THRESHOLD, score(MAX_SCORE)),
			Duration::ZERO
		);

		// collator score > INSTANT_FETCH_REP_THRESHOLD => zero delay
		assert_eq!(CollationManager::calculate_delay(score(100), score(MAX_SCORE)), Duration::ZERO);

		// collator score >= max_score_for_para => zero delay
		assert_eq!(CollationManager::calculate_delay(score(50), score(50)), Duration::ZERO);

		// collator score == 0 and max_score_for_para > 0 => MAX_FETCH_DELAY
		assert_eq!(CollationManager::calculate_delay(score(0), score(MAX_SCORE)), MAX_FETCH_DELAY);

		// collator score == 0 and max_score_for_para == 0 => zero delay (no one has rep yet)
		assert_eq!(CollationManager::calculate_delay(score(0), score(0)), Duration::ZERO);
	}

	#[test]
	fn accepted_advertisement_ordering() {
		use std::cmp::Ordering;

		let score = |val: u16| Score::new(val);
		let now = Instant::now();
		let later = now + Duration::from_secs(1);

		let scheduling_parent = Hash::random();
		let para_id = ParaId::new(1);

		let make_adv = |peer_id: PeerId| Advertisement {
			scheduling_parent,
			para_id,
			peer_id,
			prospective_candidate: None,
			advertised_descriptor_version: None,
		};

		let peer_1 = PeerId::random();
		let peer_2 = PeerId::random();

		let adv_1 = make_adv(peer_1);
		let adv_2 = make_adv(peer_2);

		// Different scores - higher score comes first (is "less").
		{
			let high_score = AcceptedAdvertisement {
				adv: &adv_1,
				score: score(100),
				timestamp: &now,
				activated_at: now,
			};
			let low_score = AcceptedAdvertisement {
				adv: &adv_2,
				score: score(50),
				timestamp: &now,
				activated_at: now,
			};

			assert_eq!(high_score.cmp(&low_score), Ordering::Less,);
			assert_eq!(low_score.cmp(&high_score), Ordering::Greater);
		}

		// Same score, different timestamps - earlier timestamp comes first.
		{
			let earlier = AcceptedAdvertisement {
				adv: &adv_1,
				score: score(100),
				timestamp: &now,
				activated_at: now,
			};
			let later = AcceptedAdvertisement {
				adv: &adv_2,
				score: score(100),
				timestamp: &later,
				activated_at: now,
			};

			assert_eq!(earlier.cmp(&later), Ordering::Less);
			assert_eq!(later.cmp(&earlier), Ordering::Greater);
		}

		// Same score, same timestamp - falls back to advertisement comparison.
		{
			let acc_1 = AcceptedAdvertisement {
				adv: &adv_1,
				score: score(100),
				timestamp: &now,
				activated_at: now,
			};
			let acc_2 = AcceptedAdvertisement {
				adv: &adv_2,
				score: score(100),
				timestamp: &now,
				activated_at: now,
			};

			// Result depends on advertisement Ord, but must be consistent and not Equal.
			let cmp_result = acc_1.cmp(&acc_2);
			assert_ne!(cmp_result, Ordering::Equal);
			assert_eq!(acc_2.cmp(&acc_1), cmp_result.reverse());
		}

		// Same advertisement, same score, same timestamp - should be Equal.
		{
			let acc_1 = AcceptedAdvertisement {
				adv: &adv_1,
				score: score(100),
				timestamp: &now,
				activated_at: now,
			};
			let acc_2 = AcceptedAdvertisement {
				adv: &adv_1,
				score: score(100),
				timestamp: &now,
				activated_at: now,
			};

			assert_eq!(acc_1.cmp(&acc_2), Ordering::Equal);
		}

		// BTreeSet ordering - first() should return highest score.
		{
			let adv_3 = make_adv(PeerId::random());
			let adv_4 = make_adv(PeerId::random());

			let advertisements = [
				AcceptedAdvertisement {
					adv: &adv_1,
					score: score(50),
					timestamp: &now,
					activated_at: now,
				},
				AcceptedAdvertisement {
					adv: &adv_2,
					score: score(200),
					timestamp: &now,
					activated_at: now,
				},
				AcceptedAdvertisement {
					adv: &adv_3,
					score: score(100),
					timestamp: &now,
					activated_at: now,
				},
				AcceptedAdvertisement {
					adv: &adv_4,
					score: score(150),
					timestamp: &later,
					activated_at: now,
				},
			]
			.into_iter()
			.collect::<BTreeSet<_>>();

			let first = advertisements.first().unwrap();
			assert_eq!(first.score, score(200));
		}

		// BTreeSet with same scores - first() returns earliest timestamp.
		{
			let adv_3 = make_adv(PeerId::random());

			let advertisements: BTreeSet<_> = [
				AcceptedAdvertisement {
					adv: &adv_1,
					score: score(100),
					timestamp: &later,
					activated_at: now,
				},
				AcceptedAdvertisement {
					adv: &adv_2,
					score: score(100),
					timestamp: &now,
					activated_at: now,
				},
				AcceptedAdvertisement {
					adv: &adv_3,
					score: score(50),
					timestamp: &now,
					activated_at: now,
				},
			]
			.into_iter()
			.collect();

			let first = advertisements.first().unwrap();
			assert_eq!(first.score, score(100), "First should have score 100");
			assert_eq!(first.timestamp, &now, "First should have earlier timestamp");
		}
	}

	#[test]
	fn pick_best_advertisement_works() {
		let scheduling_parent = Hash::random();
		let para_id = ParaId::new(1);
		let score = |val: u16| Score::new(val);

		let now = Instant::now();
		// Timestamp far enough in the past that any delay has passed.
		let old_timestamp = now.checked_sub(MAX_FETCH_DELAY).unwrap();
		// Timestamp recent enough that delay hasn't passed.
		let recent_timestamp = now;

		let peer_a = PeerId::random();
		let peer_b = PeerId::random();
		let peer_c = PeerId::random();

		// V2 ad: fetchable from any in-view scheduling parent. V1 (`None`) is only fetchable on
		// active leaves, which would require implicit_view setup the unit test doesn't do.
		let prospective_candidate = Some(ProspectiveCandidate {
			candidate_hash: CandidateHash(Hash::repeat_byte(0xab)),
			parent_head_data_hash: Hash::repeat_byte(0xcd),
		});
		let make_adv = |peer: PeerId| Advertisement {
			scheduling_parent,
			para_id,
			peer_id: peer,
			prospective_candidate,
			advertised_descriptor_version: None,
		};

		let new_collation_manager_instance = || CollationManager {
			implicit_view: ImplicitView::new(),
			leaf_claim_queues: HashMap::new(),
			per_scheduling_parent: HashMap::from([(
				scheduling_parent,
				PerSchedulingParent::new(0, CoreIndex(0)),
			)]),
			blocked_from_seconding: HashMap::new(),
			per_session: LruMap::new(ByLength::new(2)),
			fetching: PendingRequests::default(),
			keystore: Arc::new(sc_keystore::LocalKeystore::in_memory()),
			leaf_scheduling_info: HashMap::default(),
		};

		// No advertisements - returns Left(None).
		{
			let collation_manager = new_collation_manager_instance();
			let get_rep = |_: &PeerId, _: &ParaId| Some(score(100));

			assert_eq!(
				collation_manager.pick_best_advertisement(
					now,
					scheduling_parent,
					para_id,
					score(100),
					&get_rep,
				),
				Either::Left(None)
			);
		}

		// Single advertisement with delay passed - returns the advertisement.
		{
			let mut collation_manager = new_collation_manager_instance();
			let get_rep = |_: &PeerId, _: &ParaId| Some(score(100));

			collation_manager
				.per_scheduling_parent
				.get_mut(&scheduling_parent)
				.unwrap()
				.add_advertisement(make_adv(peer_a), old_timestamp);

			assert_eq!(
				collation_manager.pick_best_advertisement(
					now,
					scheduling_parent,
					para_id,
					score(100), // highest_rep == peer's score, so delay = 0
					&get_rep,
				),
				Either::Left(Some(make_adv(peer_a)))
			);
		}

		// Single advertisement with delay not passed - returns Right(delay).
		{
			let mut collation_manager = new_collation_manager_instance();
			let get_rep = |_: &PeerId, _: &ParaId| Some(score(0));

			collation_manager
				.per_scheduling_parent
				.get_mut(&scheduling_parent)
				.unwrap()
				.add_advertisement(make_adv(peer_a), recent_timestamp);

			// highest_rep = 100, peer's score = 0 (< INSTANT_FETCH_REP_THRESHOLD), so delay =
			// MAX_FETCH_DELAY
			let result = collation_manager.pick_best_advertisement(
				now,
				scheduling_parent,
				para_id,
				score(100),
				&get_rep,
			);

			assert_eq!(result, Either::Right(MAX_FETCH_DELAY));
		}

		// Multiple advertisements - picks highest score.
		{
			let mut collation_manager = new_collation_manager_instance();
			let peer_a_clone = peer_a;
			let peer_b_clone = peer_b;
			let peer_c_clone = peer_c;
			let get_rep = move |peer: &PeerId, _: &ParaId| {
				if *peer == peer_a_clone {
					Some(score(50))
				} else if *peer == peer_b_clone {
					Some(score(100))
				} else if *peer == peer_c_clone {
					Some(score(75))
				} else {
					None
				}
			};

			let per_sp =
				collation_manager.per_scheduling_parent.get_mut(&scheduling_parent).unwrap();
			per_sp.add_advertisement(make_adv(peer_a), old_timestamp);
			per_sp.add_advertisement(make_adv(peer_b), old_timestamp);
			per_sp.add_advertisement(make_adv(peer_c), old_timestamp);

			// All have old timestamps, so delay has passed. Should pick peer_b (highest score).
			assert_eq!(
				collation_manager.pick_best_advertisement(
					now,
					scheduling_parent,
					para_id,
					score(100),
					&get_rep,
				),
				Either::Left(Some(make_adv(peer_b)))
			);
		}

		// Same score - picks earlier timestamp.
		{
			let mut collation_manager = new_collation_manager_instance();
			let get_rep = |_: &PeerId, _: &ParaId| Some(score(100));

			let earlier = old_timestamp;
			let later = old_timestamp + Duration::from_secs(1);

			let per_sp =
				collation_manager.per_scheduling_parent.get_mut(&scheduling_parent).unwrap();
			per_sp.add_advertisement(make_adv(peer_a), later);
			per_sp.add_advertisement(make_adv(peer_b), earlier);

			// Same score, peer_b has earlier timestamp.
			assert_eq!(
				collation_manager.pick_best_advertisement(
					now,
					scheduling_parent,
					para_id,
					score(100),
					&get_rep,
				),
				Either::Left(Some(make_adv(peer_b)))
			);
		}

		// Unknown peer (get_rep returns None) - advertisement is filtered out.
		{
			let mut collation_manager = new_collation_manager_instance();
			let get_rep = |_: &PeerId, _: &ParaId| -> Option<Score> { None };

			collation_manager
				.per_scheduling_parent
				.get_mut(&scheduling_parent)
				.unwrap()
				.add_advertisement(make_adv(peer_a), old_timestamp);

			assert_eq!(
				collation_manager.pick_best_advertisement(
					now,
					scheduling_parent,
					para_id,
					score(100),
					&get_rep,
				),
				Either::Left(None)
			);
		}

		// Unknown scheduling parent - returns Left(None).
		{
			let collation_manager = new_collation_manager_instance();
			let get_rep = |_: &PeerId, _: &ParaId| Some(score(100));
			let unknown_scheduling_parent = Hash::random();

			assert_eq!(
				collation_manager.pick_best_advertisement(
					now,
					unknown_scheduling_parent,
					para_id,
					score(100),
					&get_rep,
				),
				Either::Left(None)
			);
		}

		// Delay passed because leaf has been active long enough, even though advertisement arrived
		// recently. Tests that the delay is relative to activation time, not advertisement
		// arrival time. When the scheduling parent (leaf) has been active longer than the full
		// delay, the remaining delay should be zero and the advertisement should be fetched
		// immediately.
		{
			let mut collation_manager = new_collation_manager_instance();
			let get_rep = |_: &PeerId, _: &ParaId| Some(score(0));

			// Set activated_at far enough in the past that any delay has elapsed.
			let per_sp =
				collation_manager.per_scheduling_parent.get_mut(&scheduling_parent).unwrap();
			per_sp.activated_at = now.checked_sub(MAX_FETCH_DELAY * 2).unwrap();

			// Advertisement arrives now (recent), but the leaf has been active long enough.
			per_sp.add_advertisement(make_adv(peer_a), recent_timestamp);

			// highest_rep = 100, peer's score = 0 (< INSTANT_FETCH_REP_THRESHOLD), so delay =
			// MAX_FETCH_DELAY. But activated_at is 2*MAX_FETCH_DELAY ago, so remaining_delay = 0.
			assert_eq!(
				collation_manager.pick_best_advertisement(
					now,
					scheduling_parent,
					para_id,
					score(100),
					&get_rep,
				),
				Either::Left(Some(make_adv(peer_a)))
			);
		}

		// Advertisement with partial delay elapsed returns remaining delay.
		{
			let mut collation_manager = new_collation_manager_instance();
			let get_rep = |_: &PeerId, _: &ParaId| Some(score(0));

			// Set activated_at so that only part of the delay has elapsed.
			// score(0) < INSTANT_FETCH_REP_THRESHOLD and < highest_rep => delay = MAX_FETCH_DELAY
			// activated_at = MAX_FETCH_DELAY / 4 ago => remaining = MAX_FETCH_DELAY * 3/4
			let per_sp =
				collation_manager.per_scheduling_parent.get_mut(&scheduling_parent).unwrap();
			per_sp.activated_at = now.checked_sub(MAX_FETCH_DELAY / 4).unwrap();

			per_sp.add_advertisement(make_adv(peer_a), recent_timestamp);

			let result = collation_manager.pick_best_advertisement(
				now,
				scheduling_parent,
				para_id,
				score(100),
				&get_rep,
			);

			assert_eq!(result, Either::Right(MAX_FETCH_DELAY / 4 * 3));
		}
	}
}
