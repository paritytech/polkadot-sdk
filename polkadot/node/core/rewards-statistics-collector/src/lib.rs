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


use std::collections::{HashMap, HashSet, VecDeque};
use std::collections::hash_map::Entry;
use futures::{channel::oneshot, prelude::*};
use gum::CandidateHash;
use sp_keystore::KeystorePtr;
use polkadot_node_subsystem::{
    errors::RuntimeApiError as RuntimeApiSubsystemError,
    messages::{ChainApiMessage, ConsensusStatisticsCollectorMessage, RuntimeApiMessage, RuntimeApiRequest},
    overseer, ActiveLeavesUpdate, FromOrchestra, OverseerSignal, SpawnedSubsystem, SubsystemError, SubsystemSender
};
use polkadot_primitives::{
    AuthorityDiscoveryId, BlockNumber, Hash, Header, SessionIndex, ValidatorId, ValidatorIndex,
    well_known_keys::relay_dispatch_queue_remaining_capacity
};
use polkadot_node_primitives::{
    approval::{
        time::Tick,
        v1::DelayTranche
    }
};
use crate::{
    error::{FatalError, FatalResult, JfyiError, JfyiErrorResult, Result},
};

mod error;
#[cfg(test)]
mod tests;
mod approval_voting_metrics;
mod availability_distribution_metrics;
pub mod metrics;

use approval_voting_metrics::ApprovalsStats;
use polkadot_node_subsystem::RuntimeApiError::{Execution, NotSupported};
use polkadot_node_subsystem_util::{request_candidate_events, request_session_index_for_child, request_session_info};
use polkadot_primitives::vstaging::{ApprovalStatisticsTallyLine, ApprovalStatistics};
use crate::approval_voting_metrics::{handle_candidate_approved, handle_observed_no_shows};
use crate::availability_distribution_metrics::{handle_chunk_uploaded, handle_chunks_downloaded, AvailabilityChunks};
use self::metrics::Metrics;

const MAX_SESSIONS_TO_KEEP: u32 = 2;
const LOG_TARGET: &str = "parachain::rewards-statistics-collector";

#[derive(Default)]
pub struct Config {
    pub publish_per_validator_approval_metrics: bool
}

struct PerRelayView {
    session_index: SessionIndex,
    parent_hash: Option<Hash>,
    children: HashSet<Hash>,
    approvals_stats: HashMap<CandidateHash, ApprovalsStats>,
}

impl PerRelayView {
    fn new(parent_hash: Option<Hash>, session_index: SessionIndex) -> Self {
        PerRelayView{
            session_index: session_index,
            parent_hash: parent_hash,
            children: HashSet::new(),
            approvals_stats: HashMap::new(),
        }
    }

    fn link_child(&mut self, hash: Hash) {
        self.children.insert(hash);
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Default)]
struct PerValidatorTally {
    no_shows: u32,
    approvals: u32,
}

impl PerValidatorTally {
    fn increment_noshow(&mut self) {
        self.no_shows += 1;
    }

    fn increment_approval(&mut self) {
        self.approvals += 1;
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct PerSessionView {
    authorities_lookup: HashMap<AuthorityDiscoveryId, ValidatorIndex>,
    validators_tallies: HashMap<ValidatorIndex, PerValidatorTally>,
}

impl PerSessionView {
    fn new(authorities_lookup: HashMap<AuthorityDiscoveryId, ValidatorIndex>) -> Self {
        Self { authorities_lookup, validators_tallies: HashMap::new() }
    }
}

/// A struct that holds the credentials required to sign the PVF check statements. These credentials
/// are implicitly to pinned to a session where our node acts as a validator.
struct SigningCredentials {
    /// The validator public key.
    validator_key: ValidatorId,
    /// The validator index in the current session.
    validator_index: ValidatorIndex,
}

struct View {
    roots: HashSet<Hash>,
    per_relay: HashMap<Hash, PerRelayView>,
    per_session: HashMap<SessionIndex, PerSessionView>,
    availability_chunks: HashMap<SessionIndex, AvailabilityChunks>,
    current_session: Option<SessionIndex>,
    credentials: Option<SigningCredentials>,
}

impl View {
    fn new() -> Self {
        return View{
            roots: HashSet::new(),
            per_relay: HashMap::new(),
            per_session: HashMap::new(),
            availability_chunks: HashMap::new(),
            current_session: None,
            credentials: None,
        };
    }
}

/// The statistics collector subsystem.
#[derive(Default)]
pub struct RewardsStatisticsCollector {
    metrics: Metrics,
    config: Config
}

impl RewardsStatisticsCollector {
    /// Create a new instance of the `ConsensusStatisticsCollector`.
    pub fn new(metrics: Metrics, config: Config) -> Self {
        Self {
            metrics,
            config,
        }
    }
}

#[overseer::subsystem(ConsensusStatisticsCollector, error = SubsystemError, prefix = self::overseer)]
impl<Context> RewardsStatisticsCollector
where
    Context: Send + Sync,
{
    fn start(self, ctx: Context) -> SpawnedSubsystem {
        SpawnedSubsystem {
            future: run(ctx, (self.metrics, self.config.publish_per_validator_approval_metrics))
                .map_err(|e| SubsystemError::with_origin("statistics-parachains", e))
                .boxed(),
            name: "rewards-statistics-collector-subsystem",
        }
    }
}

#[overseer::contextbounds(ConsensusStatisticsCollector, prefix = self::overseer)]
async fn run<Context>(mut ctx: Context, metrics: (Metrics, bool)) -> FatalResult<()> {
    let mut view = View::new();
    loop {
        crate::error::log_error(
            run_iteration(&mut ctx, &mut view, (&metrics.0, metrics.1)).await,
            "Encountered issue during run iteration",
        )?;
    }
}

#[overseer::contextbounds(ConsensusStatisticsCollector, prefix = self::overseer)]
pub(crate) async fn run_iteration<Context>(
    ctx: &mut Context,
    view: &mut View,
    metrics: (&Metrics, bool),
) -> Result<()> {
    let mut sender = ctx.sender().clone();
    let per_validator_metrics = metrics.1;
    loop {
        match ctx.recv().await.map_err(FatalError::SubsystemReceive)? {
            FromOrchestra::Signal(OverseerSignal::Conclude) => return Ok(()),
            FromOrchestra::Signal(OverseerSignal::ActiveLeaves(update)) => {
                if let Some(activated) = update.activated {
                    let relay_hash = activated.hash;

                    let (tx, rx) = oneshot::channel();

                    ctx.send_message(ChainApiMessage::BlockHeader(relay_hash, tx)).await;
                    let header = rx
                        .map_err(JfyiError::OverseerCommunication)
                        .await?
                        .map_err(JfyiError::ChainApiCallError)?;

                    let session_idx = request_session_index_for_child(relay_hash, ctx.sender())
                        .await
                        .await
                        .map_err(JfyiError::OverseerCommunication)?
                        .map_err(JfyiError::RuntimeApiCallError)?;

                    if let Some(ref h) = header {
                        let parent_hash = h.parent_hash;
                        let parent_hash = match view.per_relay.get_mut(&parent_hash) {
                            Some(per_relay_view) => {
                                per_relay_view.link_child(relay_hash);
                                Some(parent_hash)
                            },
                            None => {
                                _ = view.roots.insert(relay_hash);
                                None
                            },
                        };

                        view.per_relay.insert(relay_hash, PerRelayView::new(parent_hash, session_idx));
                    } else {
                        view.roots.insert(relay_hash);
                        view.per_relay.insert(relay_hash, PerRelayView::new(None, session_idx));
                    }

                    if !view.per_session.contains_key(&session_idx) {
                        let session_info = request_session_info(relay_hash, session_idx, ctx.sender())
                            .await
                            .await
                            .map_err(JfyiError::OverseerCommunication)?
                            .map_err(JfyiError::RuntimeApiCallError)?;

                        if let Some(session_info) = session_info {
                            let mut authority_lookup = HashMap::new();
                            for (i, ad) in session_info.discovery_keys.iter().cloned().enumerate() {
                                authority_lookup.insert(ad, ValidatorIndex(i as _));
                            }

                            view.per_session.insert(session_idx, PerSessionView::new(authority_lookup));
                        }
                    }
                }
            },
            FromOrchestra::Signal(OverseerSignal::BlockFinalized(fin_block_hash, _)) => {
                // when a block is finalized it performs:
                // 1. Pruning unneeded forks
                // 2. Collected statistics that belongs to the finalized chain
                // 3. After collection of finalized statistics then remove finalized
                //    nodes from the mapping leaving only the unfinalized blocks after finalization
                let finalized_hashes = prune_unfinalised_forks(view, fin_block_hash);

                // so we revert it and check from the oldest to the newest
                for hash in finalized_hashes.iter().rev() {
                    if let Some((session_idx, approvals_stats)) = view
                        .per_relay
                        .remove(hash)
                        .map(|rb_view| (rb_view.session_index, rb_view.approvals_stats))
                    {
                        if let Some(session_view) = view.per_session.get_mut(&session_idx) {
                            metrics.0.record_approvals_stats(
                                session_idx,
                                approvals_stats.clone(),
                                per_validator_metrics,
                            );

                            for stats in approvals_stats.values() {
                                // Increment no-show tallies
                                for &validator_idx in &stats.no_shows {
                                    session_view
                                        .validators_tallies
                                        .entry(validator_idx)
                                        .or_default()
                                        .increment_noshow();
                                }

                                // Increment approval tallies
                                for &validator_idx in &stats.votes {
                                    session_view
                                        .validators_tallies
                                        .entry(validator_idx)
                                        .or_default()
                                        .increment_approval();
                                }
                            }
                        }
                    }
                }

                log_session_view_general_stats(view);
                prune_old_session_views(ctx, view, fin_block_hash).await?;
            }
            FromOrchestra::Communication { msg } => {
                match msg {
                    ConsensusStatisticsCollectorMessage::ChunksDownloaded(
                        session_index,
                        candidate_hash,
                        downloads,
                    )=> {
                        handle_chunks_downloaded(
                            view,
                            session_index,
                            candidate_hash,
                            downloads,
                        )
                    },
                    ConsensusStatisticsCollectorMessage::ChunkUploaded(
                        candidate_hash,
                        authority_ids,
                    ) => {
                        handle_chunk_uploaded(
                            view,
                            candidate_hash,
                            authority_ids,
                        )
                    },
                    ConsensusStatisticsCollectorMessage::CandidateApproved(
                        candidate_hash,
                        block_hash,
                        approvals,
                    ) => {
                        handle_candidate_approved(
                            view,
                            block_hash,
                            candidate_hash,
                            approvals,
                        );
                    }
                    ConsensusStatisticsCollectorMessage::NoShows(
                        candidate_hash,
                        block_hash,
                        no_show_validators,
                    ) => {
                        handle_observed_no_shows(
                            view,
                            block_hash,
                            candidate_hash,
                            no_show_validators,
                        );
                    },
                }
            },
        }
    }
}

// prune_unfinalised_forks will remove all the relay chain blocks
// that are not in the finalized chain and its de   pendants children using the latest finalized block as reference
// and will return a list of finalized hashes
fn prune_unfinalised_forks(view: &mut View, fin_block_hash: Hash) -> Vec<Hash> {
    // since we want to reward only valid approvals, we retain
    // only finalized chain blocks and its descendants
    // identify the finalized chain so we don't prune
    let rb_view = match view.per_relay.get_mut(&fin_block_hash) {
        Some(per_relay_view) => per_relay_view,
        None => return Vec::new(),
    };

    let mut removal_stack = Vec::new();
    let mut retain_relay_hashes = Vec::new();
    retain_relay_hashes.push(fin_block_hash);

    let mut current_block_hash = fin_block_hash;
    let mut current_parent_hash = rb_view.parent_hash;
    while let Some(parent_hash) = current_parent_hash {

        match view.per_relay.get_mut(&parent_hash) {
            Some(parent_view) => {
                retain_relay_hashes.push(parent_hash.clone());

                if parent_view.children.len() > 1 {
                    let filtered_set = parent_view.children
                        .iter()
                        .filter(|&child_hash| !child_hash.eq(&current_block_hash))
                        .cloned() // Clone the elements to own them in the new HashSet
                        .collect::<Vec<_>>();

                    removal_stack.extend(filtered_set);

                    // unlink all the other children keeping only
                    // the one that belongs to the finalized chain
                    parent_view.children = HashSet::from_iter(vec![current_block_hash.clone()]);
                }
                current_block_hash = parent_hash;
                current_parent_hash = parent_view.parent_hash;
            },
            None => break
        };
    }

    // update the roots to be the children of the latest finalized block
    if let Some(finalized_hash) = retain_relay_hashes.first() {
        if let Some(rb_view) = view.per_relay.get(finalized_hash) {
            view.roots = rb_view.children.clone();
        }
    }

    let mut to_prune = HashSet::new();
    let mut queue: VecDeque<Hash> = VecDeque::from(removal_stack);
    while let Some(hash) = queue.pop_front() {
        _ = to_prune.insert(hash);

        if let Some(r_view) = view.per_relay.get(&hash) {
            for child in &r_view.children {
                queue.push_back(child.clone());
            }
        }
    }

    for rb_hash in to_prune {
        view.per_relay.remove(&rb_hash);
    }

    retain_relay_hashes
}

// prune_old_session_views avoid the per_session mapping to grow
// indefinitely by removing sessions stored for more than MAX_SESSIONS_TO_KEEP (2)
// finalized sessions.
#[overseer::contextbounds(ConsensusStatisticsCollector, prefix = self::overseer)]
async fn prune_old_session_views<Context>(
    ctx: &mut Context,
    view: &mut View,
    finalized_hash: Hash,
) -> Result<()> {
    let session_idx = request_session_index_for_child(finalized_hash, ctx.sender())
        .await
        .await
        .map_err(JfyiError::OverseerCommunication)?
        .map_err(JfyiError::RuntimeApiCallError)?;

    match view.current_session {
        Some(current_session) if current_session < session_idx => {
            if let Some(wipe_before) = session_idx.checked_sub(MAX_SESSIONS_TO_KEEP) {
                view.per_session.retain(|stored_session_index, _| *stored_session_index > wipe_before);
            }
            view.current_session = Some(session_idx)
        }
        None => view.current_session = Some(session_idx),
        _ => {}
    };

    Ok(())
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

async fn sign_and_submit_approvals_tallies(
    sender: &mut impl SubsystemSender<RuntimeApiMessage>,
    relay_parent: Hash,
    session_index: SessionIndex,
    keystore: &KeystorePtr,
    credentials: &SigningCredentials,
    metrics: &Metrics,
    tallies: HashMap<ValidatorIndex, PerValidatorTally>,
) {
    gum::debug!(
		target: LOG_TARGET,
        ?relay_parent,
		"submitting {} approvals tallies for session {}",
        tallies.len(),
        session_index,
	);

    metrics.submit_approvals_tallies(tallies.len());

    let mut validators_indexes = tallies.keys().collect::<Vec<_>>();
    validators_indexes.sort();

    let mut approvals_tallies: Vec<ApprovalStatisticsTallyLine> = Vec::with_capacity(tallies.len());
    for validator_index in validators_indexes {
        let current_tally = tallies.get(validator_index).unwrap();
        approvals_tallies.push(ApprovalStatisticsTallyLine {
            validator_index: validator_index.clone(),
            approvals_usage: current_tally.approvals,
            no_shows: current_tally.no_shows,
        });
    }

    let payload = ApprovalStatistics(session_index, approvals_tallies);

    let signature = match polkadot_node_subsystem_util::sign(
        keystore,
        &credentials.validator_key,
        &payload.signing_payload(),
    ) {
        Ok(Some(signature)) => signature,
        Ok(None) => {
            gum::warn!(
				target: LOG_TARGET,
                ?relay_parent,
				validator_index = ?credentials.validator_index,
				"private key for signing is not available",
			);
            return
        },
        Err(e) => {
            gum::warn!(
				target: LOG_TARGET,
                ?relay_parent,
				validator_index = ?credentials.validator_index,
				"error signing the statement: {:?}",
				e,
			);
            return
        },
    };

    let (tx, rx) = oneshot::channel();
    let runtime_req = runtime_api_request(
        sender,
        relay_parent,
        RuntimeApiRequest::SubmitApprovalStatistics(payload, signature, tx),
        rx,
    );

    match runtime_req.await {
        Ok(()) => {
            metrics.on_vote_submitted();
        },
        Err(e) => {
            gum::warn!(
				target: LOG_TARGET,
				"error occurred during submitting a approvals rewards tallies: {:?}",
				e,
			);
        },
    }
}

#[derive(Debug)]
pub(crate) enum RuntimeRequestError {
    NotSupported,
    ApiError,
    CommunicationError,
}

pub(crate) async fn runtime_api_request<T>(
    sender: &mut impl SubsystemSender<RuntimeApiMessage>,
    relay_parent: Hash,
    request: RuntimeApiRequest,
    receiver: oneshot::Receiver<std::result::Result<T, RuntimeApiSubsystemError>>,
) -> std::result::Result<T, RuntimeRequestError> {
    sender
        .send_message(RuntimeApiMessage::Request(relay_parent, request).into())
        .await;

    receiver
        .await
        .map_err(|_| {
            gum::debug!(target: LOG_TARGET, ?relay_parent, "Runtime API request dropped");
            RuntimeRequestError::CommunicationError
        })
        .and_then(|res| {
            res.map_err(|e| {
                use RuntimeApiSubsystemError::*;
                match e {
                    Execution { .. } => {
                        gum::debug!(
							target: LOG_TARGET,
							?relay_parent,
							err = ?e,
							"Runtime API request internal error"
						);
                        RuntimeRequestError::ApiError
                    },
                    NotSupported { .. } => RuntimeRequestError::NotSupported,
                }
            })
        })
}
