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

//! Implementation of the Consensus Statistics Collector subsystem.
//! This component monitors and manages metrics related to parachain candidate approvals,
//! including approval votes, distribution of approval chunks, chunk downloads, and chunk uploads.
//!
//! Its primary responsibility is to collect and track data reflecting each node’s perspective
//! on the approval work carried out by all session validators.

use crate::error::{FatalError, FatalResult, JfyiError, JfyiErrorResult, Result};
use futures::{channel::oneshot, prelude::*};
use polkadot_node_primitives::{approval::{time::Tick, v1::DelayTranche}, new_session_window_size, SessionWindowSize, DISPUTE_WINDOW};
use polkadot_node_subsystem::{
	errors::RuntimeApiError as RuntimeApiSubsystemError,
	messages::{
		ChainApiMessage, RewardsStatisticsCollectorMessage, RuntimeApiMessage, RuntimeApiRequest,
	},
	overseer, ActiveLeavesUpdate, FromOrchestra, OverseerSignal, SpawnedSubsystem, SubsystemError,
	SubsystemSender,
};
use polkadot_primitives::{
	AuthorityDiscoveryId, BlockNumber,
	Hash, Header, SessionIndex, ValidatorId, ValidatorIndex, CandidateHash
};
use sp_keystore::KeystorePtr;
use std::collections::{hash_map::Entry, BTreeMap, HashMap, HashSet, VecDeque};

mod approval_voting_metrics;
mod availability_distribution_metrics;
mod error;
pub mod metrics;
#[cfg(test)]
mod tests;

use self::metrics::Metrics;
use crate::{
	approval_voting_metrics::{handle_candidate_approved, handle_observed_no_shows},
	availability_distribution_metrics::{
		handle_chunk_uploaded, handle_chunks_downloaded, AvailabilityChunks,
	},
};
use approval_voting_metrics::ApprovalsStats;
use polkadot_node_subsystem::RuntimeApiError::{Execution, NotSupported};
use polkadot_node_subsystem_util::{
	request_candidate_events, request_session_index_for_child, request_session_info,
};

const MAX_SESSION_VIEWS_TO_KEEP: SessionWindowSize = DISPUTE_WINDOW;
const MAX_AVAILABILITIES_TO_KEEP: SessionWindowSize = new_session_window_size!(3);

const LOG_TARGET: &str = "parachain::rewards-statistics-collector";

#[derive(Default)]
pub struct Config {
	pub verbose_approval_metrics: bool,
}

#[derive(Debug, Default, Clone)]
struct PerRelayView {
	session_index: SessionIndex,
	approvals_stats: ApprovalsStats,
}

impl PerRelayView {
	fn new(session_index: SessionIndex) -> Self {
		PerRelayView {
			session_index,
			approvals_stats: ApprovalsStats::default(),
		}
	}
}

#[derive(Debug, Eq, PartialEq, Clone, Default)]
struct PerValidatorTally {
	no_shows: u32,
	approvals: u32,
}

impl PerValidatorTally {
	fn increment_noshow_by(&mut self, value: u32) {
		self.no_shows = self.no_shows.saturating_add(value);
	}

	fn increment_approval_by(&mut self, value: u32) {
		self.approvals = self.approvals.saturating_add(value);
	}
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct PerSessionView {
	authorities_ids: Vec<AuthorityDiscoveryId>,
	validators_tallies: HashMap<ValidatorIndex, PerValidatorTally>,
}

impl PerSessionView {
	fn new(authorities_ids: Vec<AuthorityDiscoveryId>) -> Self {
		Self { authorities_ids, validators_tallies: HashMap::new() }
	}
}

/// View holds the subsystem internal state
struct View {
	/// per_relay holds collected approvals statistics for
	/// all the candidates under the given unfinalized relay hash
	per_relay: HashMap<(Hash, BlockNumber), PerRelayView>,
	/// per_session holds session information (authorities lookup)
	/// and approvals tallies which is the aggregation of collected
	/// approvals statistics under finalized blocks
	per_session: BTreeMap<SessionIndex, PerSessionView>,
	/// availability_chunks holds collected upload and download chunks
	/// statistics per validator
	availability_chunks: BTreeMap<SessionIndex, AvailabilityChunks>,
	latest_finalized_block: (BlockNumber, Hash),
}

impl View {
	fn new() -> Self {
		View {
			per_relay: HashMap::new(),
			per_session: BTreeMap::new(),
			availability_chunks: BTreeMap::new(),
			latest_finalized_block: (0, Hash::default()),
		}
	}
}

/// The statistics collector subsystem.
#[derive(Default)]
pub struct RewardsStatisticsCollector {
	metrics: Metrics,
	config: Config,
}

impl RewardsStatisticsCollector {
	/// Create a new instance of the `RewardsStatisticsCollector`.
	pub fn new(metrics: Metrics, config: Config) -> Self {
		Self { metrics, config }
	}
}

#[overseer::subsystem(RewardsStatisticsCollector, error = SubsystemError, prefix = self::overseer)]
impl<Context> RewardsStatisticsCollector
where
	Context: Send + Sync,
{
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		SpawnedSubsystem {
			future: run(ctx, (self.metrics, self.config.verbose_approval_metrics))
				.map_err(|e| SubsystemError::with_origin("statistics-parachains", e))
				.boxed(),
			name: "rewards-statistics-collector-subsystem",
		}
	}
}

#[overseer::contextbounds(RewardsStatisticsCollector, prefix = self::overseer)]
async fn run<Context>(mut ctx: Context, metrics: (Metrics, bool)) -> FatalResult<()> {
	let mut view = View::new();
	loop {
		error::log_error(
			run_iteration(&mut ctx, &mut view, (&metrics.0, metrics.1)).await,
			"Encountered issue during run iteration",
		)?;
	}
}

#[overseer::contextbounds(RewardsStatisticsCollector, prefix = self::overseer)]
pub(crate) async fn run_iteration<Context>(
	ctx: &mut Context,
	view: &mut View,
	// the boolean flag indicates to the subsystem's
	// inner metric to publish the accumulated tallies
	// per session per validator, enabling the flag
	// could cause overhead to prometheus depending on
	// the amount of active validators
	metrics: (&Metrics, bool),
) -> Result<()> {
	loop {
		match ctx.recv().await.map_err(FatalError::SubsystemReceive)? {
			FromOrchestra::Signal(OverseerSignal::Conclude) => return Ok(()),
			FromOrchestra::Signal(OverseerSignal::ActiveLeaves(update)) => {
				if let Some(activated) = update.activated {
					let relay_hash = activated.hash;
					let relay_number = activated.number;

					let session_idx = request_session_index_for_child(relay_hash, ctx.sender())
						.await
						.await
						.map_err(JfyiError::OverseerCommunication)?
						.map_err(JfyiError::RuntimeApiCallError)?;

					view.per_relay.insert((relay_hash, relay_number), PerRelayView::new(session_idx));

					prune_based_on_session_windows(
						view,
						session_idx,
						MAX_SESSION_VIEWS_TO_KEEP,
						MAX_AVAILABILITIES_TO_KEEP,
					);

					if !view.per_session.contains_key(&session_idx) {
						let session_info =
							request_session_info(relay_hash, session_idx, ctx.sender())
								.await
								.await
								.map_err(JfyiError::OverseerCommunication)?
								.map_err(JfyiError::RuntimeApiCallError)?;

						if let Some(session_info) = session_info {
							view.per_session
								.insert(session_idx, PerSessionView::new(
									session_info.discovery_keys.iter().cloned().collect(),
								));
						}
					}
				}
			},
			FromOrchestra::Signal(OverseerSignal::BlockFinalized(fin_block_hash, fin_block_number)) => {
				// when a block is finalized it performs:
				// 1. Pruning unneeded forks
				// 2. Collected statistics that belongs to the finalized chain
				// 3. After collection of finalized statistics then remove finalized nodes from the
				//    mapping leaving only the unfinalized blocks after finalization
				let (tx, rx) = oneshot::channel();
				let ancestor_req_message = ChainApiMessage::Ancestors{
					hash: fin_block_hash,
					k: fin_block_number.saturating_sub(view.latest_finalized_block.0) as _,
					response_channel: tx,
				};
				ctx.send_message(ancestor_req_message).await;

				let mut finalized_hashes = rx
					.map_err(JfyiError::OverseerCommunication)
					.await?
					.map_err(JfyiError::ChainApiCallError)?;
				finalized_hashes.push(fin_block_hash);

				let (mut before, after) : (HashMap<_, _>, HashMap<_, _>) = view.per_relay
					.clone()
					.into_iter()
					.partition(|((_, relay_number), _)| *relay_number <= fin_block_number);

				before.retain(|(relay_hash, _), _| finalized_hashes.contains(relay_hash));
				let finalized_views: HashMap<&Hash, &PerRelayView> = before
					.iter()
					.map(|((relay_hash, _), per_relay_view)| (relay_hash, per_relay_view))
					.collect::<HashMap<_, _>>();

				aggregate_finalized_approvals_stats(view, finalized_views, metrics);
				log_session_view_general_stats(view);

				view.per_relay = after;
				view.latest_finalized_block = (fin_block_number, fin_block_hash);
			},
			FromOrchestra::Communication { msg } => match msg {
				RewardsStatisticsCollectorMessage::ChunksDownloaded(
					session_index,
					downloads,
				) => handle_chunks_downloaded(view, session_index, downloads),
				RewardsStatisticsCollectorMessage::ChunkUploaded(candidate_hash, authority_ids) => {
					let Ok(session_idx) = get_session_from_candidate(&mut *ctx, candidate_hash) else {
						gum::warn!(
							target: LOG_TARGET,
							?candidate_hash,
							?authority_ids,
							"failed to retrieve candidate's session index"
						);
						continue
					};

					handle_chunk_uploaded(view, session_idx, authority_ids);
				},

				RewardsStatisticsCollectorMessage::CandidateApproved(
					block_hash,
					block_number,
					approvals,
				) => {
					handle_candidate_approved(view, block_hash, block_number, approvals);
				},
				RewardsStatisticsCollectorMessage::NoShows(
					block_hash,
					block_number,
					no_show_validators,
				) => {
					handle_observed_no_shows(view, block_hash, block_number, no_show_validators);
				},
			},
		}
	}
}

#[overseer::contextbounds(RewardsStatisticsCollector, prefix = self::overseer)]
fn get_session_from_candidate<Context>(ctx: &mut Context, candidate_hash: CandidateHash) -> Result<SessionIndex, String> {
	Ok(0 as SessionIndex)
}

// aggregate_finalized_approvals_stats will iterate over the finalized hashes
// tallying each collected approval stats on its correct session per validator index
fn aggregate_finalized_approvals_stats(
	view: &mut View,
	finalized_relays: HashMap<&Hash, &PerRelayView>,
	metrics: (&Metrics, bool),
) {
	for (_, per_relay_view) in finalized_relays {
		if let Some(session_view) = view.per_session.get_mut(&per_relay_view.session_index) {
			metrics.0.record_approvals_stats(
				per_relay_view.session_index,
				per_relay_view.approvals_stats.clone(),
				// if true will report the metrics per validator index
				metrics.1,
			);

			for (validator_idx, total_votes) in &per_relay_view.approvals_stats.votes {
				session_view
					.validators_tallies
					.entry(*validator_idx)
					.or_default()
					.increment_approval_by(*total_votes);
			}

			for (validator_idx, total_noshows) in &per_relay_view.approvals_stats.no_shows {
				session_view
					.validators_tallies
					.entry(*validator_idx)
					.or_default()
					.increment_noshow_by(*total_noshows);
			}
		}
	}
}

// prune_based_on_session_windows prunes the per_session and the availability_chunks
// mappings based on a session windows avoiding them to grow indefinitely
fn prune_based_on_session_windows(
	view: &mut View,
	session_idx: SessionIndex,
	max_session_view_to_keep: SessionWindowSize,
	max_availabilities_to_keep: SessionWindowSize,
) {
	if let Some(wipe_before) = session_idx.checked_sub(max_session_view_to_keep.get()) {
		view.per_session = view.per_session.split_off(&wipe_before);
	}

	if let Some(wipe_before) = session_idx.checked_sub(max_availabilities_to_keep.get()) {
		view.availability_chunks = view.availability_chunks.split_off(&wipe_before);
	}
}

fn log_session_view_general_stats(view: &View) {
	for (session_index, session_view) in &view.per_session {
		let session_tally = session_view
			.validators_tallies
			.values()
			.map(|tally| (tally.approvals, tally.no_shows))
			.fold((0, 0), |acc, (approvals, noshows)| (acc.0 + approvals, acc.1 + noshows));

		gum::debug!(
			target: LOG_TARGET,
			session_idx = ?session_index,
			approvals = ?session_tally.0,
			noshows = ?session_tally.1,
			"session collected statistics"
		);
	}
}

