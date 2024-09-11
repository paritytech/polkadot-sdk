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

//! Primitives for tracking collations-related data.
//!
//! Usually a path of collations is as follows:
//!    1. First, collation must be advertised by collator.
//!    2. If the advertisement was accepted, it's queued for fetch (per relay parent).
//!    3. Once it's requested, the collation is said to be Pending.
//!    4. Pending collation becomes Fetched once received, we send it to backing for validation.
//!    5. If it turns to be invalid or async backing allows seconding another candidate, carry on
//!       with the next advertisement, otherwise we're done with this relay parent.
//!
//!    ┌──────────────────────────────────────────┐
//!    └─▶Advertised ─▶ Pending ─▶ Fetched ─▶ Validated

use std::{
	collections::{BTreeMap, VecDeque},
	future::Future,
	pin::Pin,
	task::Poll,
};

use futures::{future::BoxFuture, FutureExt};
use polkadot_node_network_protocol::{
	peer_set::CollationVersion,
	request_response::{outgoing::RequestError, v1 as request_v1, OutgoingResult},
	PeerId,
};
use polkadot_node_primitives::PoV;
use polkadot_node_subsystem::jaeger;
use polkadot_node_subsystem_util::metrics::prometheus::prometheus::HistogramTimer;
use polkadot_primitives::{
	CandidateHash, CandidateReceipt, CollatorId, Hash, HeadData, Id as ParaId,
	PersistedValidationData,
};
use tokio_util::sync::CancellationToken;

use crate::{error::SecondingError, LOG_TARGET};

/// Candidate supplied with a para head it's built on top of.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub struct ProspectiveCandidate {
	/// Candidate hash.
	pub candidate_hash: CandidateHash,
	/// Parent head-data hash as supplied in advertisement.
	pub parent_head_data_hash: Hash,
}

impl ProspectiveCandidate {
	pub fn candidate_hash(&self) -> CandidateHash {
		self.candidate_hash
	}
}

/// Identifier of a fetched collation.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct FetchedCollation {
	/// Candidate's relay parent.
	pub relay_parent: Hash,
	/// Parachain id.
	pub para_id: ParaId,
	/// Candidate hash.
	pub candidate_hash: CandidateHash,
	/// Id of the collator the collation was fetched from.
	pub collator_id: CollatorId,
}

impl From<&CandidateReceipt<Hash>> for FetchedCollation {
	fn from(receipt: &CandidateReceipt<Hash>) -> Self {
		let descriptor = receipt.descriptor();
		Self {
			relay_parent: descriptor.relay_parent,
			para_id: descriptor.para_id,
			candidate_hash: receipt.hash(),
			collator_id: descriptor.collator.clone(),
		}
	}
}

/// Identifier of a collation being requested.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub struct PendingCollation {
	/// Candidate's relay parent.
	pub relay_parent: Hash,
	/// Parachain id.
	pub para_id: ParaId,
	/// Peer that advertised this collation.
	pub peer_id: PeerId,
	/// Optional candidate hash and parent head-data hash if were
	/// supplied in advertisement.
	pub prospective_candidate: Option<ProspectiveCandidate>,
	/// Hash of the candidate's commitments.
	pub commitments_hash: Option<Hash>,
}

impl PendingCollation {
	pub fn new(
		relay_parent: Hash,
		para_id: ParaId,
		peer_id: &PeerId,
		prospective_candidate: Option<ProspectiveCandidate>,
	) -> Self {
		Self {
			relay_parent,
			para_id,
			peer_id: *peer_id,
			prospective_candidate,
			commitments_hash: None,
		}
	}
}

/// An identifier for a fetched collation that was blocked from being seconded because we don't have
/// access to the parent's HeadData. Can be retried once the candidate outputting this head data is
/// seconded.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct BlockedCollationId {
	/// Para id.
	pub para_id: ParaId,
	/// Hash of the parent head data.
	pub parent_head_data_hash: Hash,
}

/// Performs a sanity check between advertised and fetched collations.
///
/// Since the persisted validation data is constructed using the advertised
/// parent head data hash, the latter doesn't require an additional check.
pub fn fetched_collation_sanity_check(
	advertised: &PendingCollation,
	fetched: &CandidateReceipt,
	persisted_validation_data: &PersistedValidationData,
	maybe_parent_head_and_hash: Option<(HeadData, Hash)>,
) -> Result<(), SecondingError> {
	if persisted_validation_data.hash() != fetched.descriptor().persisted_validation_data_hash {
		Err(SecondingError::PersistedValidationDataMismatch)
	} else if advertised
		.prospective_candidate
		.map_or(false, |pc| pc.candidate_hash() != fetched.hash())
	{
		Err(SecondingError::CandidateHashMismatch)
	} else if maybe_parent_head_and_hash.map_or(false, |(head, hash)| head.hash() != hash) {
		Err(SecondingError::ParentHeadDataMismatch)
	} else {
		Ok(())
	}
}

/// Identifier for a requested collation and the respective collator that advertised it.
#[derive(Debug, Clone)]
pub struct CollationEvent {
	/// Collator id.
	pub collator_id: CollatorId,
	/// The network protocol version the collator is using.
	pub collator_protocol_version: CollationVersion,
	/// The requested collation data.
	pub pending_collation: PendingCollation,
}

/// Fetched collation data.
#[derive(Debug, Clone)]
pub struct PendingCollationFetch {
	/// Collation identifier.
	pub collation_event: CollationEvent,
	/// Candidate receipt.
	pub candidate_receipt: CandidateReceipt,
	/// Proof of validity.
	pub pov: PoV,
	/// Optional parachain parent head data.
	/// Only needed for elastic scaling.
	pub maybe_parent_head_data: Option<HeadData>,
}

/// The status of the collations in [`CollationsPerRelayParent`].
#[derive(Debug, Clone, Copy)]
pub enum CollationStatus {
	/// We are waiting for a collation to be advertised to us.
	Waiting,
	/// We are currently fetching a collation.
	Fetching(ParaId),
	/// We are waiting that a collation is being validated.
	WaitingOnValidation(ParaId),
}

impl Default for CollationStatus {
	fn default() -> Self {
		Self::Waiting
	}
}

/// Information about collations per relay parent.
pub struct Collations {
	/// What is the current status in regards to a collation for this relay parent?
	pub status: CollationStatus,
	/// Collator we're fetching from, optionally which candidate was requested.
	///
	/// This is the last fetch for the relay parent. The value is used in
	/// `get_next_collation_to_fetch` (called from `dequeue_next_collation_and_fetch`) to determine
	/// if the last fetched collation is the same as the one which just finished. If yes - another
	/// collation should be fetched. If not - another fetch was already initiated and
	/// `get_next_collation_to_fetch` will do nothing.
	///
	/// For the reasons above this value is not set to `None` when the fetch is done! Don't use it
	/// to check if there is a pending fetch.
	pub fetching_from: Option<(CollatorId, Option<CandidateHash>)>,
	/// Collation that were advertised to us, but we did not yet fetch. Grouped by `ParaId`.
	waiting_queue: BTreeMap<ParaId, VecDeque<(PendingCollation, CollatorId)>>,
	/// How many collations have been seconded per `ParaId`.
	seconded_per_para: BTreeMap<ParaId, usize>,
	// Claims per `ParaId` for the assigned core at the relay parent. This information is obtained
	// from `GroupAssignments` which contains either the claim queue (if runtime supports it) for
	// the core or the `ParaId` of the parachain assigned to the core.
	claims_per_para: BTreeMap<ParaId, usize>,
	// Represents the claim queue at the relay parent. The `bool` field indicates if a candidate
	// was seconded for the `ParaId` at the position in question. In other words - if the claim is
	// 'satisfied'. If the claim queue is not available `claim_queue_state` will be `None`.
	claim_queue_state: Option<Vec<(bool, ParaId)>>,
}

impl Collations {
	/// `Collations` should work with and without claim queue support. If the claim queue runtime
	/// api is available `GroupAssignments` the claim queue. If not - group assignments will contain
	/// just one item (what's scheduled on the core).
	///
	/// Some of the logic in `Collations` relies on the claim queue and if it is not available
	/// fallbacks to another logic. For this reason `Collations` needs to know if claim queue is
	/// available or not.
	///
	/// Once claim queue runtime api is released everywhere this logic won't be needed anymore and
	/// can be cleaned up.
	pub(super) fn new(group_assignments: &Vec<ParaId>, has_claim_queue: bool) -> Self {
		let mut claims_per_para = BTreeMap::new();
		let mut claim_queue_state = Vec::with_capacity(group_assignments.len());

		for para_id in group_assignments {
			*claims_per_para.entry(*para_id).or_default() += 1;
			claim_queue_state.push((false, *para_id));
		}

		// Not optimal but if the claim queue is not available `group_assignments` will have just
		// one element. Can be fixed once claim queue api is released everywhere and the fallback
		// code is cleaned up.
		let claim_queue_state = if has_claim_queue { Some(claim_queue_state) } else { None };

		Self {
			status: Default::default(),
			fetching_from: None,
			waiting_queue: Default::default(),
			seconded_per_para: Default::default(),
			claims_per_para,
			claim_queue_state,
		}
	}

	/// Note a seconded collation for a given para.
	pub(super) fn note_seconded(&mut self, para_id: ParaId) {
		*self.seconded_per_para.entry(para_id).or_default() += 1;

		gum::trace!(target: LOG_TARGET, ?para_id, new_count=*self.seconded_per_para.entry(para_id).or_default(), "Note seconded.");

		// and the claim queue state
		if let Some(claim_queue_state) = self.claim_queue_state.as_mut() {
			for (satisfied, assignment) in claim_queue_state {
				if *satisfied {
					continue
				}

				if assignment == &para_id {
					*satisfied = true;
					break
				}
			}
		}
	}

	/// Adds a new collation to the waiting queue for the relay parent. This function doesn't
	/// perform any limits check. The caller (`enqueue_collation`) should assure that the collation
	/// limit is respected.
	pub(super) fn add_to_waiting_queue(&mut self, collation: (PendingCollation, CollatorId)) {
		self.waiting_queue.entry(collation.0.para_id).or_default().push_back(collation);
	}

	/// Picks a collation to fetch from the waiting queue.
	/// When fetching collations we need to ensure that each parachain has got a fair core time
	/// share depending on its assignments in the claim queue. This means that the number of
	/// collations seconded per parachain should ideally be equal to the number of claims for the
	/// particular parachain in the claim queue.
	///
	/// To achieve this each seconded collation is mapped to an entry from the claim queue. The next
	/// fetch is the first unsatisfied entry from the claim queue for which there is an
	/// advertisement.
	///
	/// If claim queue is not supported then `group_assignment` should contain just one element and
	/// the score won't matter. In this case collations will be fetched in the order they were
	/// received.
	///
	/// Note: `group_assignments` is needed just for the fall back logic. It should be removed once
	/// claim queue runtime api is released everywhere since it will be redundant - claim queue will
	/// already be available in `self.claim_queue_state`.
	pub(super) fn pick_a_collation_to_fetch(
		&mut self,
		claim_queue_state: Vec<(bool, ParaId)>,
	) -> Option<(PendingCollation, CollatorId)> {
		gum::trace!(
			target: LOG_TARGET,
			waiting_queue=?self.waiting_queue,
			claims_per_para=?self.claims_per_para,
			"Pick a collation to fetch."
		);

		for (fulfilled, assignment) in claim_queue_state {
			// if this assignment has been already fulfilled - move on
			if fulfilled {
				continue
			}

			// we have found and unfulfilled assignment - try to fulfill it
			if let Some(collations) = self.waiting_queue.get_mut(&assignment) {
				if let Some(collation) = collations.pop_front() {
					// we don't mark the entry as fulfilled because it is considered pending
					return Some(collation)
				}
			}
		}

		None
	}

	// Returns the number of pending collations for the specified `ParaId`. This function should
	// return either 0 or 1.
	fn pending_for_para(&self, para_id: &ParaId) -> usize {
		match self.status {
			CollationStatus::Fetching(pending_para_id) if pending_para_id == *para_id => 1,
			CollationStatus::WaitingOnValidation(pending_para_id)
				if pending_para_id == *para_id =>
				1,
			_ => 0,
		}
	}

	// Returns the number of seconded collations for the specified `ParaId`.
	pub(super) fn seconded_and_pending_for_para(&self, para_id: &ParaId) -> usize {
		let seconded_for_para = *self.seconded_per_para.get(&para_id).unwrap_or(&0);
		let pending_for_para = self.pending_for_para(para_id);

		gum::trace!(
			target: LOG_TARGET,
			?para_id,
			seconded_for_para,
			pending_for_para,
			"Seconded and pending for para."
		);

		seconded_for_para + pending_for_para
	}

	// Returns the number of claims in the claim queue for the specified `ParaId`.
	pub(super) fn claims_for_para(&self, para_id: &ParaId) -> usize {
		self.claims_per_para.get(para_id).copied().unwrap_or_default()
	}
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
	pub pending_collation: PendingCollation,
	/// Collator id.
	pub collator_id: CollatorId,
	/// The network protocol version the collator is using.
	pub collator_protocol_version: CollationVersion,
	/// Responses from collator.
	pub from_collator: BoxFuture<'static, OutgoingResult<request_v1::CollationFetchingResponse>>,
	/// Handle used for checking if this request was cancelled.
	pub cancellation_token: CancellationToken,
	/// A jaeger span corresponding to the lifetime of the request.
	pub span: Option<jaeger::Span>,
	/// A metric histogram for the lifetime of the request
	pub _lifetime_timer: Option<HistogramTimer>,
}

impl Future for CollationFetchRequest {
	type Output = (
		CollationEvent,
		std::result::Result<request_v1::CollationFetchingResponse, CollationFetchError>,
	);

	fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
		// First check if this fetch request was cancelled.
		let cancelled = match std::pin::pin!(self.cancellation_token.cancelled()).poll(cx) {
			Poll::Ready(()) => true,
			Poll::Pending => false,
		};

		if cancelled {
			self.span.as_mut().map(|s| s.add_string_tag("success", "false"));
			return Poll::Ready((
				CollationEvent {
					collator_protocol_version: self.collator_protocol_version,
					collator_id: self.collator_id.clone(),
					pending_collation: self.pending_collation,
				},
				Err(CollationFetchError::Cancelled),
			))
		}

		let res = self.from_collator.poll_unpin(cx).map(|res| {
			(
				CollationEvent {
					collator_protocol_version: self.collator_protocol_version,
					collator_id: self.collator_id.clone(),
					pending_collation: self.pending_collation,
				},
				res.map_err(CollationFetchError::Request),
			)
		});

		match &res {
			Poll::Ready((_, Ok(_))) => {
				self.span.as_mut().map(|s| s.add_string_tag("success", "true"));
			},
			Poll::Ready((_, Err(_))) => {
				self.span.as_mut().map(|s| s.add_string_tag("success", "false"));
			},
			_ => {},
		};

		res
	}
}
