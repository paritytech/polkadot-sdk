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

//! Requester takes care of requesting erasure chunks for candidates that are pending
//! availability.

use futures::{
	channel::{mpsc, oneshot},
	task::{Context, Poll},
	Stream,
};
use std::{
	collections::{hash_map::HashMap, hash_set::HashSet},
	iter::IntoIterator,
	pin::Pin,
};

use polkadot_node_network_protocol::request_response::{v1, v2, IsRequest, ReqProtocolNames};
use polkadot_node_subsystem::{
	messages::{
		CandidateBackingMessage, ChainApiMessage, ProspectiveParachainsMessage, RuntimeApiMessage,
		RuntimeApiRequest,
	},
	overseer, ActivatedLeaf, ActiveLeavesUpdate,
};
use polkadot_node_subsystem_util::{
	availability_chunks::availability_chunk_index,
	request_backable_candidates,
	runtime::{get_availability_cores, get_occupied_cores, RuntimeInfo},
};
use polkadot_primitives::{
	BackedCandidate, CandidateHash, CoreIndex, CoreState, GroupIndex,
	GroupRotationInfo, Hash, Id as paraId, SessionIndex, ValidatorIndex,
};

use super::{FatalError, Metrics, Result, LOG_TARGET};

#[cfg(test)]
mod tests;

/// Cache for session information.
mod session_cache;
use session_cache::SessionCache;

/// A task fetching a particular chunk.
mod fetch_task;
use crate::error::Error::{
	CanceledValidatorGroups, FailedValidatorGroups, GetBackableCandidates, SubsystemUtil,
};
use fetch_task::{FetchTask, FetchTaskConfig, FromFetchTask};

/// Requester takes care of requesting erasure chunks from backing groups and stores them in the
/// av store.
///
/// It implements a stream that needs to be advanced for it making progress.
pub struct Requester {
	/// Candidates we need to fetch our chunk for.
	///
	/// We keep those around as long as a candidate is pending availability on some leaf, so we
	/// won't fetch chunks multiple times.
	///
	/// We remove them on failure, so we get retries on the next block still pending availability.
	fetches: HashMap<CandidateHash, FetchTask>,

	/// Track candidates for which we initiated early fetching.
	early_candidates: HashSet<CandidateHash>,

	/// The last session index we've seen, used to detect session changes
	last_session: Option<SessionIndex>,

	/// Localized information about sessions we are currently interested in.
	session_cache: SessionCache,

	/// Sender to be cloned for `FetchTask`s.
	tx: mpsc::Sender<FromFetchTask>,

	/// Receive messages from `FetchTask`.
	rx: mpsc::Receiver<FromFetchTask>,

	/// Prometheus Metrics
	metrics: Metrics,

	/// Mapping of the req-response protocols to the full protocol names.
	req_protocol_names: ReqProtocolNames,
}

/// A compact representation of a parachain candidate core's essential information,
/// used to streamline chunk-fetching tasks. This structure normalizes data from both
/// occupied and scheduled cores into a unified format containing only the fields
/// necessary for chunk fetching and validation.
#[derive(Debug)]
struct CoreInfo {
	/// The candidate hash.
	candidate_hash: CandidateHash,
	/// The relay parent of the candidate.
	relay_parent: Hash,
	/// The root hash of the erasure coded chunks for the candidate.
	erasure_root: Hash,
	/// The group index of the group responsible for the candidate.
	group_responsible: GroupIndex,
}

enum FetchOrigin {
	Early,
	Slow,
}

#[overseer::contextbounds(AvailabilityDistribution, prefix = self::overseer)]
impl Requester {
	/// How many ancestors of the leaf should we consider along with it.
	pub(crate) const LEAF_ANCESTRY_LEN_WITHIN_SESSION: usize = 3;

	/// Create a new `Requester`.
	///
	/// You must feed it with `ActiveLeavesUpdate` via `update_fetching_heads` and make it progress
	/// by advancing the stream.
	pub fn new(req_protocol_names: ReqProtocolNames, metrics: Metrics) -> Self {
		let (tx, rx) = mpsc::channel(1);
		Requester {
			fetches: HashMap::new(),
			early_candidates: HashSet::new(),
			last_session: None,
			session_cache: SessionCache::new(),
			tx,
			rx,
			metrics,
			req_protocol_names,
		}
	}

	/// Update heads that need availability distribution.
	///
	/// For all active heads we will be fetching our chunks for availability distribution.
	pub async fn update_fetching_heads<Context>(
		&mut self,
		ctx: &mut Context,
		runtime: &mut RuntimeInfo,
		update: ActiveLeavesUpdate,
	) -> Result<()> {
		gum::trace!(target: LOG_TARGET, ?update, "Update fetching heads");
		let ActiveLeavesUpdate { activated, deactivated } = update;
		if let Some(leaf) = activated {
			// Order important! We need to handle activated, prior to deactivated, otherwise we
			// might cancel still needed jobs.

			// Get the session index for this leaf
			let current_session = runtime
				.get_session_index_for_child(&mut ctx.sender().clone(), leaf.hash)
				.await?;

			// Check for session change or first initialization
			match self.last_session {
				None => {
					// First initialization counts as a session change
					self.handle_session_change();
				},
				Some(last_session) if current_session > last_session => {
					// Session has changed, clean up early candidates
					self.handle_session_change();
				},
				_ => {},
			}

			// Update our last seen session
			self.last_session = Some(current_session);

			self.start_requesting_chunks(ctx, runtime, leaf).await?;
		}

		self.stop_requesting_chunks(deactivated.into_iter());
		Ok(())
	}

	/// Start requesting chunks for newly imported head.
	///
	/// This will also request [`SESSION_ANCESTRY_LEN`] leaf ancestors from the same session
	/// and start requesting chunks for them too.
	async fn start_requesting_chunks<Context>(
		&mut self,
		ctx: &mut Context,
		runtime: &mut RuntimeInfo,
		new_head: ActivatedLeaf,
	) -> Result<()> {
		let sender = &mut ctx.sender().clone();
		let ActivatedLeaf { hash: leaf, .. } = new_head;
		let (leaf_session_index, ancestors_in_session) = get_block_ancestors_in_same_session(
			sender,
			runtime,
			leaf,
			Self::LEAF_ANCESTRY_LEN_WITHIN_SESSION,
		)
		.await?;

		// Also spawn or bump tasks for candidates in ancestry in the same session.
		for hash in std::iter::once(leaf).chain(ancestors_in_session) {
			let occupied_cores = get_occupied_cores(sender, hash).await?;
			gum::trace!(
				target: LOG_TARGET,
				occupied_cores = ?occupied_cores,
				"Query occupied core"
			);

			let cores = occupied_cores
				.into_iter()
				.map(|(index, occ)| {
					(
						index,
						CoreInfo {
							candidate_hash: occ.candidate_hash,
							relay_parent: occ.candidate_descriptor.relay_parent(),
							erasure_root: occ.candidate_descriptor.erasure_root(),
							group_responsible: occ.group_responsible,
						},
					)
				})
				.collect::<Vec<_>>();

			// Important:
			// We mark the whole ancestry as live in the **leaf** hash, so we don't need to track
			// any tasks separately.
			//
			// The next time the subsystem receives leaf update, some of spawned task will be bumped
			// to be live in fresh relay parent, while some might get dropped due to the current
			// leaf being deactivated.
			self.add_cores(ctx, runtime, leaf, leaf_session_index, cores, FetchOrigin::Slow)
				.await?;
		}

		if let Err(err) = self
			.early_request_chunks(ctx, runtime, new_head, leaf_session_index)
			.await
		{
			gum::warn!(
				target: LOG_TARGET,
				error = ?err,
				"Failed to early request chunks for activated leaf"
			);
		}
		Ok(())
	}

	async fn early_request_chunks<Context>(
		&mut self,
		ctx: &mut Context,
		runtime: &mut RuntimeInfo,
		activated_leaf: ActivatedLeaf,
		leaf_session_index: SessionIndex,
	) -> Result<()> {
		let sender = &mut ctx.sender().clone();
		let validator_groups = &get_validator_groups(sender, activated_leaf.hash).await?;

		let availability_cores =
			&get_availability_cores(sender, activated_leaf.hash).await.map_err(|err| {
				gum::warn!(
					target: LOG_TARGET,
					error = ?err,
					"Failed to get availability cores for activated leaf"
				);
				err
			})?;

		let backable_candidates = self
			.fetch_backable_candidates(&activated_leaf, sender, availability_cores)
			.await?;

		let total_cores = validator_groups.0.len();

		// Process candidates and collect cores
		let scheduled_cores = backable_candidates
			.into_iter()
			.flat_map(|(_, candidates)| {
				candidates.into_iter().filter_map(move |candidate| {
					let (_, core_index) = candidate.validator_indices_and_core_index();
					let Some(core_index) = core_index else { return None };

					let receipt = candidate.candidate();
					
					Some((
						core_index,
						CoreInfo {
							candidate_hash: receipt.hash(),
							relay_parent: receipt.descriptor.relay_parent(),
							erasure_root: receipt.descriptor.erasure_root(),
							group_responsible: validator_groups
								.1
								.group_for_core(core_index, total_cores),
						},
					))
				})
			})
			.collect::<Vec<_>>();

		self.add_cores(
			ctx,
			runtime,
			activated_leaf.hash,
			leaf_session_index,
			scheduled_cores,
			FetchOrigin::Early,
		)
		.await
	}

	/// Requests the hashes of backable candidates from prospective parachains subsystem,
	/// and then requests the backable candidates from the candidate backing subsystem.
	async fn fetch_backable_candidates<Sender>(
		&mut self,
		activated_leaf: &ActivatedLeaf,
		sender: &mut Sender,
		availability_cores: &Vec<CoreState>,
	) -> Result<HashMap<paraId, Vec<BackedCandidate>>>
	where
		Sender: overseer::SubsystemSender<ProspectiveParachainsMessage>
			+ overseer::SubsystemSender<CandidateBackingMessage>,
	{
		// provided `None` bitfields to assume cores are all available.
		let backable_candidate_hashes =
			request_backable_candidates(&availability_cores, None, &activated_leaf, sender)
				.await
				.map_err(|err| {
					gum::warn!(
						target: LOG_TARGET,
						error = ?err,
						"Failed to request backable candidate hashes for activated leaf"
					);
					SubsystemUtil(err)
				})?;

		let (tx, rx) = oneshot::channel();

		sender
			.send_message(CandidateBackingMessage::GetBackableCandidates(
				backable_candidate_hashes,
				tx,
			))
			.await;

		rx.await.map_err(|err| {
			gum::warn!(
				target: LOG_TARGET,
				error = ?err,
				"Failed to get backable candidates for activated leaf"
			);
			GetBackableCandidates(err)
		})
	}

	/// Stop requesting chunks for obsolete heads.
	fn stop_requesting_chunks(&mut self, obsolete_leaves: impl Iterator<Item = Hash>) {
		let obsolete_leaves: HashSet<_> = obsolete_leaves.collect();
		self.fetches.retain(|_, task| {
			task.remove_leaves(&obsolete_leaves);
			task.is_live()
		});
	}

	/// Clean up early candidate tracking at session change.
	///
	/// Any candidates that were fetched early but never seen on chain by session change
	/// can be considered "never made it" since backing groups change at session boundaries.
	fn handle_session_change(&mut self) {
		// Process all remaining early candidates that never made it on-chain
		for _candidate_hash in self.early_candidates.drain() {
			self.metrics.on_early_candidate_never_onchain();
		}
	}

	/// Add candidates corresponding for a particular relay parent.
	///
	/// Starting requests where necessary.
	///
	/// Note: The passed in `leaf` is not the same as `CandidateDescriptor::relay_parent` in the
	/// given cores. The latter is the `relay_parent` this candidate considers its parent, while the
	/// passed in leaf might be some later block where the candidate is still pending availability.
	async fn add_cores<Context>(
		&mut self,
		context: &mut Context,
		runtime: &mut RuntimeInfo,
		leaf: Hash,
		leaf_session_index: SessionIndex,
		cores: impl IntoIterator<Item = (CoreIndex, CoreInfo)>,
		origin: FetchOrigin,
	) -> Result<()> {
		for (core_index, core) in cores {
			if let Some(e) = self.fetches.get_mut(&core.candidate_hash) {
				// Just book keeping - we are already requesting that chunk:
				e.add_leaf(leaf);
				self.process_known_candidate(core.candidate_hash);
			} else {
				// If we are on the slow path and this candidate was already fetched early (even
				// if the task has completed), skip starting a duplicate fetch and record it.
				if matches!(origin, FetchOrigin::Slow) &&
					self.early_candidates.contains(&core.candidate_hash)
				{
					self.process_known_candidate(core.candidate_hash);
					continue;
				}

				let tx = self.tx.clone();
				let metrics = self.metrics.clone();

				let session_info = self
					.session_cache
					.get_session_info(
						context,
						runtime,
						// We use leaf here, the relay_parent must be in the same session as
						// the leaf. This is guaranteed by runtime which ensures that cores are
						// cleared at session boundaries. At the same time, only leaves are
						// guaranteed to be fetchable by the state trie.
						leaf,
						leaf_session_index,
					)
					.await
					.map_err(|err| {
						gum::warn!(
							target: LOG_TARGET,
							error = ?err,
							"Failed to spawn a fetch task"
						);
						err
					})?;

				if let Some(session_info) = session_info {
					let n_validators =
						session_info.validator_groups.iter().fold(0usize, |mut acc, group| {
							acc = acc.saturating_add(group.len());
							acc
						});
					let chunk_index = availability_chunk_index(
						session_info.node_features.as_ref(),
						n_validators,
						core_index,
						session_info.our_index,
					)?;

					let task_cfg = FetchTaskConfig::new(
						leaf,
						&core,
						tx,
						metrics,
						session_info,
						chunk_index,
						self.req_protocol_names.get_name(v1::ChunkFetchingRequest::PROTOCOL),
						self.req_protocol_names.get_name(v2::ChunkFetchingRequest::PROTOCOL),
					);

					self.fetches
						.insert(core.candidate_hash, FetchTask::start(task_cfg, context).await?);

					// Record metrics for fetch origin only once we actually start a task
					match origin {
						FetchOrigin::Early => {
							gum::debug!(
								target: LOG_TARGET,
								candidate_hash = ?core.candidate_hash,
								"Early candidate fetch initiated"
							);
							self.metrics.on_early_candidate_fetched();
							self.early_candidates.insert(core.candidate_hash);
						},
						FetchOrigin::Slow => {
							gum::debug!(
								target: LOG_TARGET,
								candidate_hash = ?core.candidate_hash,
								"Slow path candidate fetch initiated"
							);
							self.metrics.on_slow_candidate_fetched();
						},
					}
				}
			}
		}
		Ok(())
	}

	fn process_known_candidate(&mut self, candidate: CandidateHash) {
		// Only increment skip metric if we actually remove the candidate
		// This ensures we only count unique skips
		if self.early_candidates.remove(&candidate) {
			self.metrics.on_early_candidate_skipped_on_slow();
		}
	}
}

impl Stream for Requester {
	type Item = overseer::AvailabilityDistributionOutgoingMessages;

	fn poll_next(mut self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Option<Self::Item>> {
		loop {
			match Pin::new(&mut self.rx).poll_next(ctx) {
				Poll::Ready(Some(FromFetchTask::Message(m))) => return Poll::Ready(Some(m)),
				Poll::Ready(Some(FromFetchTask::Concluded(Some(bad_boys)))) => {
					self.session_cache.report_bad_log(bad_boys);
					continue;
				},
				Poll::Ready(Some(FromFetchTask::Concluded(None))) => continue,
				Poll::Ready(Some(FromFetchTask::Failed(candidate_hash))) => {
					// Make sure we retry on next block still pending availability.
					self.fetches.remove(&candidate_hash);
				},
				Poll::Ready(None) => return Poll::Ready(None),
				Poll::Pending => return Poll::Pending,
			}
		}
	}
}

/// Requests up to `limit` ancestor hashes of relay parent in the same session.
///
/// Also returns session index of the `head`.
async fn get_block_ancestors_in_same_session<Sender>(
	sender: &mut Sender,
	runtime: &mut RuntimeInfo,
	head: Hash,
	limit: usize,
) -> Result<(SessionIndex, Vec<Hash>)>
where
	Sender:
		overseer::SubsystemSender<RuntimeApiMessage> + overseer::SubsystemSender<ChainApiMessage>,
{
	// The order is parent, grandparent, ...
	//
	// `limit + 1` since a session index for the last element in ancestry
	// is obtained through its parent. It always gets truncated because
	// `session_ancestry_len` can only be incremented `ancestors.len() - 1` times.
	let mut ancestors = get_block_ancestors(sender, head, limit + 1).await?;
	let mut ancestors_iter = ancestors.iter();

	// `head` is the child of the first block in `ancestors`, request its session index.
	let head_session_index = match ancestors_iter.next() {
		Some(parent) => runtime.get_session_index_for_child(sender, *parent).await?,
		None => {
			// No first element, i.e. empty.
			return Ok((0, ancestors));
		},
	};

	let mut session_ancestry_len = 0;
	// The first parent is skipped.
	for parent in ancestors_iter {
		// Parent is the i-th ancestor, request session index for its child -- (i-1)th element.
		let session_index = runtime.get_session_index_for_child(sender, *parent).await?;
		if session_index == head_session_index {
			session_ancestry_len += 1;
		} else {
			break;
		}
	}

	// Drop the rest.
	ancestors.truncate(session_ancestry_len);

	Ok((head_session_index, ancestors))
}

/// Request up to `limit` ancestor hashes of relay parent from the Chain API.
async fn get_block_ancestors<Sender>(
	sender: &mut Sender,
	relay_parent: Hash,
	limit: usize,
) -> Result<Vec<Hash>>
where
	Sender: overseer::SubsystemSender<ChainApiMessage>,
{
	let (tx, rx) = oneshot::channel();
	sender
		.send_message(ChainApiMessage::Ancestors {
			hash: relay_parent,
			k: limit,
			response_channel: tx,
		})
		.await;

	let ancestors = rx
		.await
		.map_err(FatalError::ChainApiSenderDropped)?
		.map_err(FatalError::ChainApi)?;
	Ok(ancestors)
}

async fn get_validator_groups<Sender>(
	sender: &mut Sender,
	leaf: Hash,
) -> Result<(Vec<Vec<ValidatorIndex>>, GroupRotationInfo)>
where
	Sender: overseer::SubsystemSender<RuntimeApiMessage>,
{
	let (tx, rx) = oneshot::channel();
	sender
		.send_message(RuntimeApiMessage::Request(leaf, RuntimeApiRequest::ValidatorGroups(tx)))
		.await;

	let groups = rx
		.await
		.map_err(|err| CanceledValidatorGroups(err))?
		.map_err(|err| FailedValidatorGroups(err))?;

	Ok(groups)
}
