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

//! Implementation of the Prospective Parachains subsystem - this tracks and handles
//! prospective parachain fragments and informs other backing-stage subsystems
//! of work to be done.
//!
//! This is the main coordinator of work within the node for the collation and
//! backing phases of parachain consensus.
//!
//! This is primarily an implementation of "Fragment Chains", as described in
//! [`polkadot_node_subsystem_util::inclusion_emulator`].
//!
//! This subsystem also handles concerns such as the relay-chain being forkful and session changes.

use std::collections::{HashMap, HashSet};

use fragment_chain::{FragmentChain, PotentialAddition};
use futures::{channel::oneshot, prelude::*};

use polkadot_node_subsystem::{
	messages::{
		Ancestors, ChainApiMessage, HypotheticalCandidate, HypotheticalMembership,
		HypotheticalMembershipRequest, IntroduceSecondedCandidateRequest, ParentHeadData,
		ProspectiveParachainsMessage, ProspectiveValidationDataRequest, RuntimeApiMessage,
		RuntimeApiRequest,
	},
	overseer, ActiveLeavesUpdate, FromOrchestra, OverseerSignal, SpawnedSubsystem, SubsystemError,
};
use polkadot_node_subsystem_util::{
	inclusion_emulator::{Constraints, RelayChainBlockInfo},
	request_session_index_for_child,
	runtime::{prospective_parachains_mode, ProspectiveParachainsMode},
};
use polkadot_primitives::{
	async_backing::CandidatePendingAvailability, BlockNumber, CandidateHash,
	CommittedCandidateReceipt, CoreState, Hash, HeadData, Header, Id as ParaId,
	PersistedValidationData,
};

use crate::{
	error::{FatalError, FatalResult, JfyiError, JfyiErrorResult, Result},
	fragment_chain::{
		CandidateState, CandidateStorage, CandidateStorageInsertionError,
		Scope as FragmentChainScope,
	},
};

mod error;
mod fragment_chain;
#[cfg(test)]
mod tests;

mod metrics;
use self::metrics::Metrics;

const LOG_TARGET: &str = "parachain::prospective-parachains";

struct RelayBlockViewData {
	// Scheduling info for paras and upcoming paras.
	fragment_chains: HashMap<ParaId, FragmentChain>,
	pending_availability: HashSet<CandidateHash>,
}

struct View {
	// Active or recent relay-chain blocks by block hash.
	active_leaves: HashMap<Hash, RelayBlockViewData>,
	candidate_storage: HashMap<ParaId, CandidateStorage>,
}

impl View {
	fn new() -> Self {
		View { active_leaves: HashMap::new(), candidate_storage: HashMap::new() }
	}
}

/// The prospective parachains subsystem.
#[derive(Default)]
pub struct ProspectiveParachainsSubsystem {
	metrics: Metrics,
}

impl ProspectiveParachainsSubsystem {
	/// Create a new instance of the `ProspectiveParachainsSubsystem`.
	pub fn new(metrics: Metrics) -> Self {
		Self { metrics }
	}
}

#[overseer::subsystem(ProspectiveParachains, error = SubsystemError, prefix = self::overseer)]
impl<Context> ProspectiveParachainsSubsystem
where
	Context: Send + Sync,
{
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		SpawnedSubsystem {
			future: run(ctx, self.metrics)
				.map_err(|e| SubsystemError::with_origin("prospective-parachains", e))
				.boxed(),
			name: "prospective-parachains-subsystem",
		}
	}
}

#[overseer::contextbounds(ProspectiveParachains, prefix = self::overseer)]
async fn run<Context>(mut ctx: Context, metrics: Metrics) -> FatalResult<()> {
	let mut view = View::new();
	loop {
		crate::error::log_error(
			run_iteration(&mut ctx, &mut view, &metrics).await,
			"Encountered issue during run iteration",
		)?;
	}
}

#[overseer::contextbounds(ProspectiveParachains, prefix = self::overseer)]
async fn run_iteration<Context>(
	ctx: &mut Context,
	view: &mut View,
	metrics: &Metrics,
) -> Result<()> {
	loop {
		match ctx.recv().await.map_err(FatalError::SubsystemReceive)? {
			FromOrchestra::Signal(OverseerSignal::Conclude) => return Ok(()),
			FromOrchestra::Signal(OverseerSignal::ActiveLeaves(update)) => {
				handle_active_leaves_update(&mut *ctx, view, update, metrics).await?;
			},
			FromOrchestra::Signal(OverseerSignal::BlockFinalized(..)) => {},
			FromOrchestra::Communication { msg } => match msg {
				ProspectiveParachainsMessage::IntroduceSecondedCandidate(request, tx) =>
					handle_introduce_seconded_candidate(&mut *ctx, view, request, tx, metrics).await,
				ProspectiveParachainsMessage::CandidateBacked(para, candidate_hash) =>
					handle_candidate_backed(&mut *ctx, view, para, candidate_hash).await,
				ProspectiveParachainsMessage::GetBackableCandidates(
					relay_parent,
					para,
					count,
					ancestors,
					tx,
				) => answer_get_backable_candidates(&view, relay_parent, para, count, ancestors, tx),
				ProspectiveParachainsMessage::GetHypotheticalMembership(request, tx) =>
					answer_hypothetical_membership_request(&view, request, tx, metrics),
				ProspectiveParachainsMessage::GetMinimumRelayParents(relay_parent, tx) =>
					answer_minimum_relay_parents_request(&view, relay_parent, tx),
				ProspectiveParachainsMessage::GetProspectiveValidationData(request, tx) =>
					answer_prospective_validation_data_request(&view, request, tx),
			},
		}
	}
}

#[overseer::contextbounds(ProspectiveParachains, prefix = self::overseer)]
async fn handle_active_leaves_update<Context>(
	ctx: &mut Context,
	view: &mut View,
	update: ActiveLeavesUpdate,
	metrics: &Metrics,
) -> JfyiErrorResult<()> {
	// 1. clean up inactive leaves
	// 2. determine all scheduled paras at the new block
	// 3. construct new fragment chain for each para for each new leaf
	// 4. prune candidate storage.

	for deactivated in &update.deactivated {
		view.active_leaves.remove(deactivated);
	}

	let mut temp_header_cache = HashMap::new();
	for activated in update.activated.into_iter() {
		let hash = activated.hash;

		let mode = prospective_parachains_mode(ctx.sender(), hash)
			.await
			.map_err(JfyiError::Runtime)?;

		let ProspectiveParachainsMode::Enabled { max_candidate_depth, allowed_ancestry_len } = mode
		else {
			gum::trace!(
				target: LOG_TARGET,
				block_hash = ?hash,
				"Skipping leaf activation since async backing is disabled"
			);

			// Not a part of any allowed ancestry.
			return Ok(())
		};

		let scheduled_paras = fetch_upcoming_paras(&mut *ctx, hash).await?;

		let block_info: RelayChainBlockInfo =
			match fetch_block_info(&mut *ctx, &mut temp_header_cache, hash).await? {
				None => {
					gum::warn!(
						target: LOG_TARGET,
						block_hash = ?hash,
						"Failed to get block info for newly activated leaf block."
					);

					// `update.activated` is an option, but we can use this
					// to exit the 'loop' and skip this block without skipping
					// pruning logic.
					continue
				},
				Some(info) => info,
			};

		let ancestry =
			fetch_ancestry(&mut *ctx, &mut temp_header_cache, hash, allowed_ancestry_len).await?;

		let mut all_pending_availability = HashSet::new();

		// Find constraints.
		let mut fragment_chains = HashMap::new();
		for para in scheduled_paras {
			let candidate_storage =
				view.candidate_storage.entry(para).or_insert_with(CandidateStorage::default);

			let backing_state = fetch_backing_state(&mut *ctx, hash, para).await?;

			let Some((constraints, pending_availability)) = backing_state else {
				// This indicates a runtime conflict of some kind.
				gum::debug!(
					target: LOG_TARGET,
					para_id = ?para,
					relay_parent = ?hash,
					"Failed to get inclusion backing state."
				);

				continue
			};

			all_pending_availability.extend(pending_availability.iter().map(|c| c.candidate_hash));

			let pending_availability = preprocess_candidates_pending_availability(
				ctx,
				&mut temp_header_cache,
				constraints.required_parent.clone(),
				pending_availability,
			)
			.await?;
			let mut compact_pending = Vec::with_capacity(pending_availability.len());

			for c in pending_availability {
				let res = candidate_storage.add_candidate(
					c.candidate,
					c.persisted_validation_data,
					CandidateState::Backed,
				);
				let candidate_hash = c.compact.candidate_hash;

				match res {
					Ok(_) | Err(CandidateStorageInsertionError::CandidateAlreadyKnown(_)) => {},
					Err(err) => {
						gum::warn!(
							target: LOG_TARGET,
							?candidate_hash,
							para_id = ?para,
							?err,
							"Scraped invalid candidate pending availability",
						);

						break
					},
				}

				compact_pending.push(c.compact);
			}

			let scope = FragmentChainScope::with_ancestors(
				para,
				block_info.clone(),
				constraints,
				compact_pending,
				max_candidate_depth,
				ancestry.iter().cloned(),
			)
			.expect("ancestors are provided in reverse order and correctly; qed");

			gum::trace!(
				target: LOG_TARGET,
				relay_parent = ?hash,
				min_relay_parent = scope.earliest_relay_parent().number,
				para_id = ?para,
				"Creating fragment chain"
			);

			let chain = FragmentChain::populate(scope, &*candidate_storage);

			gum::trace!(
				target: LOG_TARGET,
				relay_parent = ?hash,
				para_id = ?para,
				"Populated fragment chain with {} candidates",
				chain.len()
			);

			fragment_chains.insert(para, chain);
		}

		view.active_leaves.insert(
			hash,
			RelayBlockViewData { fragment_chains, pending_availability: all_pending_availability },
		);
	}

	if !update.deactivated.is_empty() {
		// This has potential to be a hotspot.
		prune_view_candidate_storage(view, metrics);
	}

	Ok(())
}

fn prune_view_candidate_storage(view: &mut View, metrics: &Metrics) {
	let _timer = metrics.time_prune_view_candidate_storage();

	let active_leaves = &view.active_leaves;
	let mut live_candidates = HashSet::new();
	let mut live_paras = HashSet::new();
	for sub_view in active_leaves.values() {
		live_candidates.extend(sub_view.pending_availability.iter().cloned());

		for (para_id, fragment_chain) in &sub_view.fragment_chains {
			live_candidates.extend(fragment_chain.to_vec());
			live_paras.insert(*para_id);
		}
	}

	let connected_candidates_count = live_candidates.len();
	for (leaf, sub_view) in active_leaves.iter() {
		for (para_id, fragment_chain) in &sub_view.fragment_chains {
			if let Some(storage) = view.candidate_storage.get(para_id) {
				let unconnected_potential =
					fragment_chain.find_unconnected_potential_candidates(storage, None);
				if !unconnected_potential.is_empty() {
					gum::trace!(
						target: LOG_TARGET,
						?leaf,
						"Keeping {} unconnected candidates for paraid {} in storage: {:?}",
						unconnected_potential.len(),
						para_id,
						unconnected_potential
					);
				}
				live_candidates.extend(unconnected_potential);
			}
		}
	}

	view.candidate_storage.retain(|para_id, storage| {
		if !live_paras.contains(&para_id) {
			return false
		}

		storage.retain(|h| live_candidates.contains(&h));

		// Even if `storage` is now empty, we retain.
		// This maintains a convenient invariant that para-id storage exists
		// as long as there's an active head which schedules the para.
		true
	});

	for (para_id, storage) in view.candidate_storage.iter() {
		gum::trace!(
			target: LOG_TARGET,
			"Keeping a total of {} connected candidates for paraid {} in storage",
			storage.candidates().count(),
			para_id,
		);
	}

	metrics.record_candidate_storage_size(
		connected_candidates_count as u64,
		live_candidates.len().saturating_sub(connected_candidates_count) as u64,
	);
}

struct ImportablePendingAvailability {
	candidate: CommittedCandidateReceipt,
	persisted_validation_data: PersistedValidationData,
	compact: crate::fragment_chain::PendingAvailability,
}

#[overseer::contextbounds(ProspectiveParachains, prefix = self::overseer)]
async fn preprocess_candidates_pending_availability<Context>(
	ctx: &mut Context,
	cache: &mut HashMap<Hash, Header>,
	required_parent: HeadData,
	pending_availability: Vec<CandidatePendingAvailability>,
) -> JfyiErrorResult<Vec<ImportablePendingAvailability>> {
	let mut required_parent = required_parent;

	let mut importable = Vec::new();
	let expected_count = pending_availability.len();

	for (i, pending) in pending_availability.into_iter().enumerate() {
		let Some(relay_parent) =
			fetch_block_info(ctx, cache, pending.descriptor.relay_parent).await?
		else {
			gum::debug!(
				target: LOG_TARGET,
				?pending.candidate_hash,
				?pending.descriptor.para_id,
				index = ?i,
				?expected_count,
				"Had to stop processing pending candidates early due to missing info.",
			);

			break
		};

		let next_required_parent = pending.commitments.head_data.clone();
		importable.push(ImportablePendingAvailability {
			candidate: CommittedCandidateReceipt {
				descriptor: pending.descriptor,
				commitments: pending.commitments,
			},
			persisted_validation_data: PersistedValidationData {
				parent_head: required_parent,
				max_pov_size: pending.max_pov_size,
				relay_parent_number: relay_parent.number,
				relay_parent_storage_root: relay_parent.storage_root,
			},
			compact: crate::fragment_chain::PendingAvailability {
				candidate_hash: pending.candidate_hash,
				relay_parent,
			},
		});

		required_parent = next_required_parent;
	}

	Ok(importable)
}

#[overseer::contextbounds(ProspectiveParachains, prefix = self::overseer)]
async fn handle_introduce_seconded_candidate<Context>(
	_ctx: &mut Context,
	view: &mut View,
	request: IntroduceSecondedCandidateRequest,
	tx: oneshot::Sender<bool>,
	metrics: &Metrics,
) {
	let _timer = metrics.time_introduce_seconded_candidate();

	let IntroduceSecondedCandidateRequest {
		candidate_para: para,
		candidate_receipt: candidate,
		persisted_validation_data: pvd,
	} = request;

	let Some(storage) = view.candidate_storage.get_mut(&para) else {
		gum::warn!(
			target: LOG_TARGET,
			para_id = ?para,
			candidate_hash = ?candidate.hash(),
			"Received seconded candidate for inactive para",
		);

		let _ = tx.send(false);
		return
	};

	let parent_head_hash = pvd.parent_head.hash();
	let output_head_hash = Some(candidate.commitments.head_data.hash());

	// We first introduce the candidate in the storage and then try to extend the chain.
	// If the candidate gets included in the chain, we can keep it in storage.
	// If it doesn't, check that it's still a potential candidate in at least one fragment chain.
	// If it's not, we can remove it.

	let candidate_hash =
		match storage.add_candidate(candidate.clone(), pvd, CandidateState::Seconded) {
			Ok(c) => c,
			Err(CandidateStorageInsertionError::CandidateAlreadyKnown(_)) => {
				gum::debug!(
					target: LOG_TARGET,
					para = ?para,
					"Attempting to introduce an already known candidate: {:?}",
					candidate.hash()
				);
				// Candidate already known.
				let _ = tx.send(true);
				return
			},
			Err(CandidateStorageInsertionError::PersistedValidationDataMismatch) => {
				// We can't log the candidate hash without either doing more ~expensive
				// hashing but this branch indicates something is seriously wrong elsewhere
				// so it's doubtful that it would affect debugging.

				gum::warn!(
					target: LOG_TARGET,
					para = ?para,
					"Received seconded candidate had mismatching validation data",
				);

				let _ = tx.send(false);
				return
			},
		};

	let mut keep_in_storage = false;
	for (relay_parent, leaf_data) in view.active_leaves.iter_mut() {
		if let Some(chain) = leaf_data.fragment_chains.get_mut(&para) {
			gum::trace!(
				target: LOG_TARGET,
				para = ?para,
				?relay_parent,
				"Candidates in chain before trying to introduce a new one: {:?}",
				chain.to_vec()
			);
			chain.extend_from_storage(&*storage);
			if chain.contains_candidate(&candidate_hash) {
				keep_in_storage = true;

				gum::trace!(
					target: LOG_TARGET,
					?relay_parent,
					para = ?para,
					?candidate_hash,
					"Added candidate to chain.",
				);
			} else {
				match chain.can_add_candidate_as_potential(
					&storage,
					&candidate_hash,
					&candidate.descriptor.relay_parent,
					parent_head_hash,
					output_head_hash,
				) {
					PotentialAddition::Anyhow => {
						gum::trace!(
							target: LOG_TARGET,
							para = ?para,
							?relay_parent,
							?candidate_hash,
							"Kept candidate as unconnected potential.",
						);

						keep_in_storage = true;
					},
					_ => {
						gum::trace!(
							target: LOG_TARGET,
							para = ?para,
							?relay_parent,
							"Not introducing a new candidate: {:?}",
							candidate_hash
						);
					},
				}
			}
		}
	}

	// If there is at least one leaf where this candidate can be added or potentially added in the
	// future, keep it in storage.
	if !keep_in_storage {
		storage.remove_candidate(&candidate_hash);

		gum::debug!(
			target: LOG_TARGET,
			para = ?para,
			candidate = ?candidate_hash,
			"Newly-seconded candidate cannot be kept under any active leaf",
		);
	}

	let _ = tx.send(keep_in_storage);
}

#[overseer::contextbounds(ProspectiveParachains, prefix = self::overseer)]
async fn handle_candidate_backed<Context>(
	_ctx: &mut Context,
	view: &mut View,
	para: ParaId,
	candidate_hash: CandidateHash,
) {
	let Some(storage) = view.candidate_storage.get_mut(&para) else {
		gum::warn!(
			target: LOG_TARGET,
			para_id = ?para,
			?candidate_hash,
			"Received instruction to back a candidate for unscheduled para",
		);

		return
	};

	if !storage.contains(&candidate_hash) {
		gum::warn!(
			target: LOG_TARGET,
			para_id = ?para,
			?candidate_hash,
			"Received instruction to back unknown candidate",
		);

		return
	}

	if storage.is_backed(&candidate_hash) {
		gum::debug!(
			target: LOG_TARGET,
			para_id = ?para,
			?candidate_hash,
			"Received redundant instruction to mark candidate as backed",
		);

		return
	}

	storage.mark_backed(&candidate_hash);
}

fn answer_get_backable_candidates(
	view: &View,
	relay_parent: Hash,
	para: ParaId,
	count: u32,
	ancestors: Ancestors,
	tx: oneshot::Sender<Vec<(CandidateHash, Hash)>>,
) {
	let Some(data) = view.active_leaves.get(&relay_parent) else {
		gum::debug!(
			target: LOG_TARGET,
			?relay_parent,
			para_id = ?para,
			"Requested backable candidate for inactive relay-parent."
		);

		let _ = tx.send(vec![]);
		return
	};

	let Some(chain) = data.fragment_chains.get(&para) else {
		gum::debug!(
			target: LOG_TARGET,
			?relay_parent,
			para_id = ?para,
			"Requested backable candidate for inactive para."
		);

		let _ = tx.send(vec![]);
		return
	};

	let Some(storage) = view.candidate_storage.get(&para) else {
		gum::warn!(
			target: LOG_TARGET,
			?relay_parent,
			para_id = ?para,
			"No candidate storage for active para",
		);

		let _ = tx.send(vec![]);
		return
	};

	gum::trace!(
		target: LOG_TARGET,
		?relay_parent,
		para_id = ?para,
		"Candidate storage for para: {:?}",
		storage.candidates().map(|candidate| candidate.hash()).collect::<Vec<_>>()
	);

	gum::trace!(
		target: LOG_TARGET,
		?relay_parent,
		para_id = ?para,
		"Candidate chain for para: {:?}",
		chain.to_vec()
	);

	let backable_candidates: Vec<_> = chain
		.find_backable_chain(ancestors.clone(), count, |candidate| storage.is_backed(candidate))
		.into_iter()
		.filter_map(|child_hash| {
			storage.relay_parent_of_candidate(&child_hash).map_or_else(
				|| {
					// Here, we'd actually need to trim all of the candidates that follow. Or
					// not, the runtime will do this. Impossible scenario anyway.
					gum::error!(
						target: LOG_TARGET,
						?child_hash,
						para_id = ?para,
						"Candidate is present in fragment chain but not in candidate's storage!",
					);
					None
				},
				|parent_hash| Some((child_hash, parent_hash)),
			)
		})
		.collect();

	if backable_candidates.is_empty() {
		gum::trace!(
			target: LOG_TARGET,
			?ancestors,
			para_id = ?para,
			%relay_parent,
			"Could not find any backable candidate",
		);
	} else {
		gum::trace!(
			target: LOG_TARGET,
			?relay_parent,
			?backable_candidates,
			?ancestors,
			"Found backable candidates",
		);
	}

	let _ = tx.send(backable_candidates);
}

fn answer_hypothetical_membership_request(
	view: &View,
	request: HypotheticalMembershipRequest,
	tx: oneshot::Sender<Vec<(HypotheticalCandidate, HypotheticalMembership)>>,
	metrics: &Metrics,
) {
	let _timer = metrics.time_hypothetical_membership_request();

	let mut response = Vec::with_capacity(request.candidates.len());
	for candidate in request.candidates {
		response.push((candidate, vec![]));
	}

	let required_active_leaf = request.fragment_chain_relay_parent;
	for (active_leaf, leaf_view) in view
		.active_leaves
		.iter()
		.filter(|(h, _)| required_active_leaf.as_ref().map_or(true, |x| h == &x))
	{
		for &mut (ref candidate, ref mut membership) in &mut response {
			let para_id = &candidate.candidate_para();
			let Some(fragment_chain) = leaf_view.fragment_chains.get(para_id) else { continue };
			let Some(candidate_storage) = view.candidate_storage.get(para_id) else { continue };

			if fragment_chain.hypothetical_membership(candidate.clone(), candidate_storage) {
				membership.push(*active_leaf);
			}
		}
	}

	let _ = tx.send(response);
}

fn answer_minimum_relay_parents_request(
	view: &View,
	relay_parent: Hash,
	tx: oneshot::Sender<Vec<(ParaId, BlockNumber)>>,
) {
	let mut v = Vec::new();
	if let Some(leaf_data) = view.active_leaves.get(&relay_parent) {
		for (para_id, fragment_chain) in &leaf_data.fragment_chains {
			v.push((*para_id, fragment_chain.scope().earliest_relay_parent().number));
		}
	}

	let _ = tx.send(v);
}

fn answer_prospective_validation_data_request(
	view: &View,
	request: ProspectiveValidationDataRequest,
	tx: oneshot::Sender<Option<PersistedValidationData>>,
) {
	// 1. Try to get the head-data from the candidate store if known.
	// 2. Otherwise, it might exist as the base in some relay-parent and we can find it by iterating
	//    fragment chains.
	// 3. Otherwise, it is unknown.
	// 4. Also try to find the relay parent block info by scanning fragment chains.
	// 5. If head data and relay parent block info are found - success. Otherwise, failure.

	let storage = match view.candidate_storage.get(&request.para_id) {
		None => {
			let _ = tx.send(None);
			return
		},
		Some(s) => s,
	};

	let (mut head_data, parent_head_data_hash) = match request.parent_head_data {
		ParentHeadData::OnlyHash(parent_head_data_hash) => (
			storage.head_data_by_hash(&parent_head_data_hash).map(|x| x.clone()),
			parent_head_data_hash,
		),
		ParentHeadData::WithData { head_data, hash } => (Some(head_data), hash),
	};

	let mut relay_parent_info = None;
	let mut max_pov_size = None;

	for fragment_chain in view
		.active_leaves
		.values()
		.filter_map(|x| x.fragment_chains.get(&request.para_id))
	{
		if head_data.is_some() && relay_parent_info.is_some() && max_pov_size.is_some() {
			break
		}
		if relay_parent_info.is_none() {
			relay_parent_info = fragment_chain.scope().ancestor(&request.candidate_relay_parent);
		}
		if head_data.is_none() {
			let required_parent = &fragment_chain.scope().base_constraints().required_parent;
			if required_parent.hash() == parent_head_data_hash {
				head_data = Some(required_parent.clone());
			}
		}
		if max_pov_size.is_none() {
			let contains_ancestor =
				fragment_chain.scope().ancestor(&request.candidate_relay_parent).is_some();
			if contains_ancestor {
				// We are leaning hard on two assumptions here.
				// 1. That the fragment chain never contains allowed relay-parents whose session for
				//    children is different from that of the base block's.
				// 2. That the max_pov_size is only configurable per session.
				max_pov_size = Some(fragment_chain.scope().base_constraints().max_pov_size);
			}
		}
	}

	let _ = tx.send(match (head_data, relay_parent_info, max_pov_size) {
		(Some(h), Some(i), Some(m)) => Some(PersistedValidationData {
			parent_head: h,
			relay_parent_number: i.number,
			relay_parent_storage_root: i.storage_root,
			max_pov_size: m as _,
		}),
		_ => None,
	});
}

#[overseer::contextbounds(ProspectiveParachains, prefix = self::overseer)]
async fn fetch_backing_state<Context>(
	ctx: &mut Context,
	relay_parent: Hash,
	para_id: ParaId,
) -> JfyiErrorResult<Option<(Constraints, Vec<CandidatePendingAvailability>)>> {
	let (tx, rx) = oneshot::channel();
	ctx.send_message(RuntimeApiMessage::Request(
		relay_parent,
		RuntimeApiRequest::ParaBackingState(para_id, tx),
	))
	.await;

	Ok(rx
		.await
		.map_err(JfyiError::RuntimeApiRequestCanceled)??
		.map(|s| (From::from(s.constraints), s.pending_availability)))
}

#[overseer::contextbounds(ProspectiveParachains, prefix = self::overseer)]
async fn fetch_upcoming_paras<Context>(
	ctx: &mut Context,
	relay_parent: Hash,
) -> JfyiErrorResult<Vec<ParaId>> {
	let (tx, rx) = oneshot::channel();

	// This'll have to get more sophisticated with parathreads,
	// but for now we can just use the `AvailabilityCores`.
	ctx.send_message(RuntimeApiMessage::Request(
		relay_parent,
		RuntimeApiRequest::AvailabilityCores(tx),
	))
	.await;

	let cores = rx.await.map_err(JfyiError::RuntimeApiRequestCanceled)??;
	let mut upcoming = HashSet::new();
	for core in cores {
		match core {
			CoreState::Occupied(occupied) => {
				if let Some(next_up_on_available) = occupied.next_up_on_available {
					upcoming.insert(next_up_on_available.para_id);
				}
				if let Some(next_up_on_time_out) = occupied.next_up_on_time_out {
					upcoming.insert(next_up_on_time_out.para_id);
				}
			},
			CoreState::Scheduled(scheduled) => {
				upcoming.insert(scheduled.para_id);
			},
			CoreState::Free => {},
		}
	}

	Ok(upcoming.into_iter().collect())
}

// Fetch ancestors in descending order, up to the amount requested.
#[overseer::contextbounds(ProspectiveParachains, prefix = self::overseer)]
async fn fetch_ancestry<Context>(
	ctx: &mut Context,
	cache: &mut HashMap<Hash, Header>,
	relay_hash: Hash,
	ancestors: usize,
) -> JfyiErrorResult<Vec<RelayChainBlockInfo>> {
	if ancestors == 0 {
		return Ok(Vec::new())
	}

	let (tx, rx) = oneshot::channel();
	ctx.send_message(ChainApiMessage::Ancestors {
		hash: relay_hash,
		k: ancestors,
		response_channel: tx,
	})
	.await;

	let hashes = rx.map_err(JfyiError::ChainApiRequestCanceled).await??;
	let required_session = request_session_index_for_child(relay_hash, ctx.sender())
		.await
		.await
		.map_err(JfyiError::RuntimeApiRequestCanceled)??;

	let mut block_info = Vec::with_capacity(hashes.len());
	for hash in hashes {
		let info = match fetch_block_info(ctx, cache, hash).await? {
			None => {
				gum::warn!(
					target: LOG_TARGET,
					relay_hash = ?hash,
					"Failed to fetch info for hash returned from ancestry.",
				);

				// Return, however far we got.
				break
			},
			Some(info) => info,
		};

		// The relay chain cannot accept blocks backed from previous sessions, with
		// potentially previous validators. This is a technical limitation we need to
		// respect here.

		let session = request_session_index_for_child(hash, ctx.sender())
			.await
			.await
			.map_err(JfyiError::RuntimeApiRequestCanceled)??;

		if session == required_session {
			block_info.push(info);
		} else {
			break
		}
	}

	Ok(block_info)
}

#[overseer::contextbounds(ProspectiveParachains, prefix = self::overseer)]
async fn fetch_block_header_with_cache<Context>(
	ctx: &mut Context,
	cache: &mut HashMap<Hash, Header>,
	relay_hash: Hash,
) -> JfyiErrorResult<Option<Header>> {
	if let Some(h) = cache.get(&relay_hash) {
		return Ok(Some(h.clone()))
	}

	let (tx, rx) = oneshot::channel();

	ctx.send_message(ChainApiMessage::BlockHeader(relay_hash, tx)).await;
	let header = rx.map_err(JfyiError::ChainApiRequestCanceled).await??;
	if let Some(ref h) = header {
		cache.insert(relay_hash, h.clone());
	}
	Ok(header)
}

#[overseer::contextbounds(ProspectiveParachains, prefix = self::overseer)]
async fn fetch_block_info<Context>(
	ctx: &mut Context,
	cache: &mut HashMap<Hash, Header>,
	relay_hash: Hash,
) -> JfyiErrorResult<Option<RelayChainBlockInfo>> {
	let header = fetch_block_header_with_cache(ctx, cache, relay_hash).await?;

	Ok(header.map(|header| RelayChainBlockInfo {
		hash: relay_hash,
		number: header.number,
		storage_root: header.state_root,
	}))
}
