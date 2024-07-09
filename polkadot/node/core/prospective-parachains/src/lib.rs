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

use fragment_chain::CandidateStorage;
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
	vstaging::fetch_claim_queue,
};
use polkadot_primitives::{
	async_backing::CandidatePendingAvailability, BlockNumber, CandidateHash,
	CommittedCandidateReceipt, CoreState, Hash, HeadData, Header, Id as ParaId,
	PersistedValidationData,
};

use crate::{
	error::{FatalError, FatalResult, JfyiError, JfyiErrorResult, Result},
	fragment_chain::{
		CandidateEntry, Error as FragmentChainError, FragmentChain, Scope as FragmentChainScope,
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
	// The fragment chains for current and upcoming scheduled paras.
	fragment_chains: HashMap<ParaId, FragmentChain>,
}

struct View {
	// Active or recent relay-chain blocks by block hash.
	active_leaves: HashMap<Hash, RelayBlockViewData>,
}

impl View {
	fn new() -> Self {
		View { active_leaves: HashMap::new() }
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
					handle_introduce_seconded_candidate(view, request, tx, metrics).await,
				ProspectiveParachainsMessage::CandidateBacked(para, candidate_hash) =>
					handle_candidate_backed(view, para, candidate_hash, metrics).await,
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
	// For each active leaf:
	// - determine the scheduled paras
	// - pre-populate the candidate storage with pending availability candidates and candidates from
	//   the parent leaf.
	// - populate the fragment chain
	//
	// Only then, clean up inactive leaves. They must be cleaned only after new leaves are
	// processed, because we may reuse their candidates.

	let _timer = metrics.time_handle_active_leaves_update();

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

		let scheduled_paras = fetch_upcoming_paras(ctx, hash).await?;

		let block_info: RelayChainBlockInfo =
			match fetch_block_info(ctx, &mut temp_header_cache, hash).await? {
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

		let requested_ancestry_len = if allowed_ancestry_len == 0 {
			1
			// We should try requesting at least one, so that we can know the previous leaf.
		} else {
			allowed_ancestry_len
		};
		let mut ancestry =
			fetch_ancestry(ctx, &mut temp_header_cache, hash, requested_ancestry_len).await?;

		let prev_fragment_chains =
			ancestry.first().and_then(|prev_leaf| view.active_leaves.get(&prev_leaf.hash));

		if allowed_ancestry_len == 0 {
			// Now, if the allowed ancestry len was 0, clear the one ancestor we requested.
			ancestry.clear();
		}

		let mut fragment_chains = HashMap::new();
		for para in scheduled_paras {
			// Get the candidate storage of the parent leaf, if present.
			let prev_candidate_storage = prev_fragment_chains
				.map(|chains| {
					chains
						.fragment_chains
						.get(&para)
						.map(|chain| chain.as_candidate_storage())
						.unwrap_or_default()
				})
				.unwrap_or_default();

			// Find constraints and pending availability candidates.
			let backing_state = fetch_backing_state(ctx, hash, para).await?;
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

			let pending_availability = preprocess_candidates_pending_availability(
				ctx,
				&mut temp_header_cache,
				constraints.required_parent.clone(),
				pending_availability,
			)
			.await?;
			let mut compact_pending = Vec::with_capacity(pending_availability.len());

			let mut new_storage = CandidateStorage::default();

			for c in pending_availability {
				let candidate_hash = c.compact.candidate_hash;
				let res = new_storage.add_pending_availability_candidate(
					candidate_hash,
					c.candidate,
					c.persisted_validation_data,
				);

				match res {
					Ok(_) |
					Err(FragmentChainError::CandidateAlreadyKnown) |
					Err(FragmentChainError::CandidateAlreadyPendingAvailability) => {},
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

			let scope = match FragmentChainScope::with_ancestors(
				block_info.clone(),
				constraints,
				compact_pending,
				max_candidate_depth,
				ancestry.iter().cloned(),
			) {
				Ok(scope) => scope,
				Err(unexpected_ancestors) => {
					gum::warn!(
						target: LOG_TARGET,
						para_id = ?para,
						max_candidate_depth,
						?ancestry,
						leaf = ?hash,
						"Relay chain ancestors have wrong order: {:?}",
						unexpected_ancestors
					);
					continue
				},
			};

			gum::trace!(
				target: LOG_TARGET,
				relay_parent = ?hash,
				min_relay_parent = scope.earliest_relay_parent().number,
				para_id = ?para,
				ancestors = ?ancestry,
				"Creating fragment chain"
			);

			// Add old candidates to the new storage only after we added the pending availability
			// candidates. The pending candidates have higher priority and can conflict with the old
			// candidates.
			for candidate in prev_candidate_storage.into_candidates() {
				// We need to swallow any potential errors here, as they can happen under normal
				// operation, with candidates becoming out of scope for example.
				let _ = new_storage.add_candidate_entry(candidate);
			}

			// Finally, populate the fragment chain.
			let chain = FragmentChain::populate(scope, new_storage);

			gum::trace!(
				target: LOG_TARGET,
				relay_parent = ?hash,
				para_id = ?para,
				"Populated fragment chain with {} candidates: {:?}",
				chain.len(),
				chain.to_vec()
			);

			gum::trace!(
				target: LOG_TARGET,
				relay_parent = ?hash,
				para_id = ?para,
				"Potential candidate storage for para: {:?}",
				chain.unconnected().map(|candidate| candidate.hash()).collect::<Vec<_>>()
			);

			fragment_chains.insert(para, chain);
		}

		view.active_leaves.insert(hash, RelayBlockViewData { fragment_chains });
	}

	for deactivated in &update.deactivated {
		view.active_leaves.remove(deactivated);
	}

	if metrics.0.is_some() {
		let mut connected = 0;
		let mut unconnected = 0;
		for RelayBlockViewData { fragment_chains } in view.active_leaves.values() {
			for chain in fragment_chains.values() {
				connected += chain.len();
				unconnected += chain.unconnected_len();
			}
		}

		metrics.record_candidate_count(connected as u64, unconnected as u64);
	}

	Ok(())
}

struct ImportablePendingAvailability {
	candidate: CommittedCandidateReceipt,
	persisted_validation_data: PersistedValidationData,
	compact: fragment_chain::PendingAvailability,
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
			compact: fragment_chain::PendingAvailability {
				candidate_hash: pending.candidate_hash,
				relay_parent,
			},
		});

		required_parent = next_required_parent;
	}

	Ok(importable)
}

async fn handle_introduce_seconded_candidate(
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

	let candidate_hash = candidate.hash();
	let candidate_entry = match CandidateEntry::new_seconded(candidate_hash, candidate, pvd) {
		Ok(candidate) => candidate,
		Err(err) => {
			gum::warn!(
				target: LOG_TARGET,
				para = ?para,
				"Cannot add seconded candidate: {}",
				err
			);

			let _ = tx.send(false);
			return
		},
	};

	let mut added = false;
	let mut para_scheduled = false;
	for (leaf, leaf_data) in view.active_leaves.iter_mut() {
		if let Some(chain) = leaf_data.fragment_chains.get_mut(&para) {
			para_scheduled = true;

			match chain.try_adding_seconded_candidate(&candidate_entry) {
				Ok(()) => {
					gum::debug!(
						target: LOG_TARGET,
						para = ?para,
						relay_parent = ?leaf,
						"Added seconded candidate {:?}",
						candidate_hash
					);
					added = true;
				},
				Err(FragmentChainError::CandidateAlreadyKnown) => {
					gum::debug!(
						target: LOG_TARGET,
						para = ?para,
						relay_parent = ?leaf,
						"Attempting to introduce an already known candidate: {:?}",
						candidate_hash
					);
					added = true;
				},
				Err(FragmentChainError::CandidateAlreadyPendingAvailability) => {
					gum::debug!(
						target: LOG_TARGET,
						para = ?para,
						relay_parent = ?leaf,
						"Attempting to introduce a candidate which is already pending availability: {:?}",
						candidate_hash
					);
					added = true;
				},
				Err(err) => {
					gum::debug!(
						target: LOG_TARGET,
						para = ?para,
						relay_parent = ?leaf,
						?candidate_hash,
						"Cannot introduce seconded candidate: {}",
						err
					)
				},
			}
		}
	}

	if !para_scheduled {
		gum::warn!(
			target: LOG_TARGET,
			para_id = ?para,
			?candidate_hash,
			"Received seconded candidate for inactive para",
		);
	}

	if !added {
		gum::debug!(
			target: LOG_TARGET,
			para = ?para,
			candidate = ?candidate_hash,
			"Newly-seconded candidate cannot be kept under any active leaf",
		);
	}

	let _ = tx.send(added);
}

async fn handle_candidate_backed(
	view: &mut View,
	para: ParaId,
	candidate_hash: CandidateHash,
	metrics: &Metrics,
) {
	let _timer = metrics.time_candidate_backed();

	let mut found_candidate = false;
	let mut found_para = false;
	for (leaf, leaf_data) in view.active_leaves.iter_mut() {
		if let Some(chain) = leaf_data.fragment_chains.get_mut(&para) {
			found_para = true;
			if chain.is_candidate_backed(&candidate_hash) {
				gum::debug!(
					target: LOG_TARGET,
					para_id = ?para,
					?candidate_hash,
					"Received redundant instruction to mark as backed an already backed candidate",
				);
				found_candidate = true;
			} else if chain.contains_unconnected_candidate(&candidate_hash) {
				found_candidate = true;
				// Now that a candidate was backed, attempt to recreate the fragment chain.
				let maybe_new_chain = chain.candidate_backed(&candidate_hash);

				gum::trace!(
					target: LOG_TARGET,
					relay_parent = ?leaf,
					para_id = ?para,
					"Candidate backed. Candidate chain for para: {:?}",
					maybe_new_chain.as_ref().unwrap_or(chain).to_vec()
				);

				gum::trace!(
					target: LOG_TARGET,
					relay_parent = ?leaf,
					para_id = ?para,
					"Potential candidate storage for para: {:?}",
					maybe_new_chain.as_ref().unwrap_or(chain).unconnected().map(|candidate| candidate.hash()).collect::<Vec<_>>()
				);

				// Replace the old chain with the new one.
				if let Some(new_chain) = maybe_new_chain {
					*chain = new_chain;
				}
			}
		}
	}

	if !found_para {
		gum::warn!(
			target: LOG_TARGET,
			para_id = ?para,
			?candidate_hash,
			"Received instruction to back a candidate for unscheduled para",
		);

		return
	}

	if !found_candidate {
		// This can be harmless. It can happen if we received a better backed candidate before and
		// dropped this other candidate already.
		gum::debug!(
			target: LOG_TARGET,
			para_id = ?para,
			?candidate_hash,
			"Received instruction to back unknown candidate",
		);
	}
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

	gum::trace!(
		target: LOG_TARGET,
		?relay_parent,
		para_id = ?para,
		"Candidate chain for para: {:?}",
		chain.to_vec()
	);

	gum::trace!(
		target: LOG_TARGET,
		?relay_parent,
		para_id = ?para,
		"Potential candidate storage for para: {:?}",
		chain.unconnected().map(|candidate| candidate.hash()).collect::<Vec<_>>()
	);

	let backable_candidates = chain.find_backable_chain(ancestors.clone(), count);

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

			let res = fragment_chain.can_add_candidate_as_potential(candidate);
			match res {
				Err(FragmentChainError::CandidateAlreadyKnown) | Ok(()) => {
					membership.push(*active_leaf);
				},
				// This will also match if the candidate is already pending availability.
				// In this case, we don't need to validate it again or distribute its statements.
				// It's already on chain.
				Err(err) => {
					gum::debug!(
						target: LOG_TARGET,
						para = ?para_id,
						leaf = ?active_leaf,
						candidate = ?candidate.candidate_hash(),
						"Candidate is not a hypothetical member: {}",
						err
					)
				},
			};
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
	// Try getting the needed data from any fragment chain.

	let (mut head_data, parent_head_data_hash) = match request.parent_head_data {
		ParentHeadData::OnlyHash(parent_head_data_hash) => (None, parent_head_data_hash),
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
			head_data = fragment_chain.get_head_data_by_hash(&parent_head_data_hash);
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
) -> JfyiErrorResult<HashSet<ParaId>> {
	Ok(match fetch_claim_queue(ctx.sender(), relay_parent).await? {
		Some(claim_queue) => {
			// Runtime supports claim queue - use it
			claim_queue
				.iter_all_claims()
				.flat_map(|(_, paras)| paras.into_iter())
				.copied()
				.collect()
		},
		None => {
			// fallback to availability cores - remove this branch once claim queue is released
			// everywhere
			let (tx, rx) = oneshot::channel();
			ctx.send_message(RuntimeApiMessage::Request(
				relay_parent,
				RuntimeApiRequest::AvailabilityCores(tx),
			))
			.await;

			let cores = rx.await.map_err(JfyiError::RuntimeApiRequestCanceled)??;

			let mut upcoming = HashSet::with_capacity(cores.len());
			for core in cores {
				match core {
					CoreState::Occupied(occupied) => {
						// core sharing won't work optimally with this branch because the collations
						// can't be prepared in advance.
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

			upcoming
		},
	})
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
