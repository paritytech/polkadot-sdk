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
use std::task::Context;
use futures::{channel::oneshot, prelude::*};
use gum::CandidateHash;
use sp_keystore::KeystorePtr;
use polkadot_node_subsystem::{
    errors::RuntimeApiError as RuntimeApiSubsystemError,
    messages::{ChainApiMessage, RewardsStatisticsCollectorMessage, RuntimeApiMessage, RuntimeApiRequest},
    overseer, ActiveLeavesUpdate, FromOrchestra, OverseerSignal, SpawnedSubsystem, SubsystemError, SubsystemSender
};
use polkadot_primitives::{
    AuthorityDiscoveryId, BlockNumber, Hash, Header, SessionIndex, ValidatorId, ValidatorIndex,
    well_known_keys::relay_dispatch_queue_remaining_capacity
};
use polkadot_node_primitives::{approval::{
    time::Tick,
    v1::DelayTranche
}, SessionWindowSize, DISPUTE_WINDOW};
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
use crate::approval_voting_metrics::{handle_candidate_approved, handle_observed_no_shows};
use crate::availability_distribution_metrics::{handle_chunk_uploaded, handle_chunks_downloaded, AvailabilityChunks};
use self::metrics::Metrics;

const MAX_SESSIONS_TO_KEEP: SessionWindowSize = DISPUTE_WINDOW;
const LOG_TARGET: &str = "parachain::rewards-statistics-collector";

#[derive(Default)]
pub struct Config {
    pub verbose_approval_metrics: bool
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
    credentials: Option<SigningCredentials>,
    authorities_lookup: HashMap<AuthorityDiscoveryId, ValidatorIndex>,
    validators_tallies: HashMap<ValidatorIndex, PerValidatorTally>,
}

impl PerSessionView {
    fn new(
        authorities_lookup: HashMap<AuthorityDiscoveryId, ValidatorIndex>,
        credentials: Option<SigningCredentials>,
    ) -> Self {
        Self {
            authorities_lookup,
            credentials,
            validators_tallies: HashMap::new(),
        }
    }
}

/// A struct that holds the credentials required to sign the PVF check statements. These credentials
/// are implicitly to pinned to a session where our node acts as a validator.
#[derive(Debug, Eq, PartialEq, Clone)]
struct SigningCredentials {
    /// The validator public key.
    validator_key: ValidatorId,
    /// The validator index in the current session.
    validator_index: ValidatorIndex,
}

/// View holds the subsystem internal state
struct View {
    /// roots contains the only unfinalized relay hashes
    /// is used when finalization happens to prune unneeded forks
    roots: HashSet<Hash>,
    /// per_relay holds collected approvals statistics for
    /// all the candidates under the given unfinalized relay hash
    per_relay: HashMap<Hash, PerRelayView>,
    /// per_session holds session information (authorities lookup)
    /// and approvals tallies which is the aggregation of collected
    /// approvals statistics under finalized blocks
    per_session: HashMap<SessionIndex, PerSessionView>,
    /// availability_chunks holds collected upload and download chunks
    /// statistics per validator
    availability_chunks: HashMap<SessionIndex, AvailabilityChunks>,
    current_session: Option<SessionIndex>,
    recent_block: Option<(BlockNumber, Hash)>,
}

impl View {
    fn new() -> Self {
        return View{
            roots: HashSet::new(),
            per_relay: HashMap::new(),
            per_session: HashMap::new(),
            availability_chunks: HashMap::new(),
            current_session: None,
            recent_block: None,
        };
    }

    // add_node includes a new activated block
    // in the unfinalized blocks mapping, it also
    // links the including block with its parent
    // if its parent is present in the mapping
    // otherwise the including block will be added
    // in the roots set.
    fn add_node(
        &mut self,
        activated_hash: Hash,
        activated_header: Option<Header>,
        session_index: SessionIndex,
    ) {
        if let Some(h) = activated_header {
            let parent_hash = h.parent_hash;
            let parent_hash = match self.per_relay.get_mut(&parent_hash) {
                Some(per_relay_view) => {
                    per_relay_view.link_child(activated_hash);
                    Some(parent_hash)
                },
                None => {
                    _ = self.roots.insert(activated_hash);
                    None
                },
            };

            self.per_relay.insert(activated_hash, PerRelayView::new(parent_hash, session_index));
        } else {
            self.roots.insert(activated_hash);
            self.per_relay.insert(activated_hash, PerRelayView::new(None, session_index));
        }
    }
}

/// The statistics collector subsystem.
pub struct RewardsStatisticsCollector {
    keystore: KeystorePtr,
    metrics: Metrics,
    config: Config
}

impl RewardsStatisticsCollector {
    /// Create a new instance of the `ConsensusStatisticsCollector`.
    pub fn new(keystore: KeystorePtr, metrics: Metrics, config: Config) -> Self {
        Self {
            metrics,
            config,
            keystore,
        }
    }
}

#[overseer::subsystem(RewardsStatisticsCollector, error = SubsystemError, prefix = self::overseer)]
impl<Context> RewardsStatisticsCollector
where
    Context: Send + Sync,
{
    fn start(self, ctx: Context) -> SpawnedSubsystem {
        SpawnedSubsystem {
            future: run(ctx, self.keystore, (self.metrics, self.config.publish_per_validator_approval_metrics))
                .map_err(|e| SubsystemError::with_origin("statistics-parachains", e))
                .boxed(),
            name: "rewards-statistics-collector-subsystem",
        }
    }
}

#[overseer::contextbounds(ConsensusStatisticsCollector, prefix = self::overseer)]
async fn run<Context>(mut ctx: Context, keystore: KeystorePtr, metrics: (Metrics, bool)) -> FatalResult<()> {
    let mut view = View::new();
    loop {
        error::log_error(
            run_iteration(&mut ctx, &mut view, &keystore, (&metrics.0, metrics.1)).await,
            "Encountered issue during run iteration",
        )?;
    }
}

#[overseer::contextbounds(RewardsStatisticsCollector, prefix = self::overseer)]
pub(crate) async fn run_iteration<Context>(
    ctx: &mut Context,
    view: &mut View,
    keystore: &KeystorePtr,
    metrics: (&Metrics, bool),
) -> Result<()> {
    let per_validator_metrics = metrics.1;
    loop {
        match ctx.recv().await.map_err(FatalError::SubsystemReceive)? {
            FromOrchestra::Signal(OverseerSignal::Conclude) => return Ok(()),
            FromOrchestra::Signal(OverseerSignal::ActiveLeaves(update)) => {
                if let Some(activated) = update.activated {
                    let ActivationInfo {
                        activated_header,
                        session_index,
                        new_session_info,
                        recent_block,
                    } = extract_activated_leaf_info(
                        ctx.sender(),
                        view,
                        keystore,
                        activated.hash,
                        activated.number,
                    ).await?;

                    let relay_hash = activated.hash;
                    view.recent_block = Some(recent_block);

                    view.add_node(
                        relay_hash,
                        activated_header,
                        session_index,
                    );

                    if let Some((session_info, credentials)) = new_session_info {
                        let mut authority_lookup = HashMap::new();
                        for (i, ad) in session_info.discovery_keys.iter().cloned().enumerate() {
                            authority_lookup.insert(ad, ValidatorIndex(i as _));
                        }

                        view.per_session.insert(session_index, PerSessionView::new(authority_lookup, credentials));
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
                submit_finalized_session_stats(
                    ctx.sender(),
                    keystore,
                    view,
                    fin_block_hash,
                    metrics.0,
                ).await?;
            }
            FromOrchestra::Communication { msg } => {
                match msg {
                    RewardsStatisticsCollectorMessage::ChunksDownloaded(
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
                    RewardsStatisticsCollectorMessage::ChunkUploaded(
                        candidate_hash,
                        authority_ids,
                    ) => {
                        handle_chunk_uploaded(
                            view,
                            candidate_hash,
                            authority_ids,
                        )
                    },
                    RewardsStatisticsCollectorMessage::CandidateApproved(
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
                    RewardsStatisticsCollectorMessage::NoShows(
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

struct ActivationInfo {
    activated_header: Option<Header>,
    recent_block: (BlockNumber, Hash),
    session_index: SessionIndex,
    new_session_info: Option<(SessionInfo, Option<SigningCredentials>)>,
}

async fn extract_activated_leaf_info<
    Sender: SubsystemSender<ChainApiMessage>
        + SubsystemSender<RuntimeApiMessage>
>(
    mut sender: Sender,
    view: &mut View,
    keystore: &KeystorePtr,
    relay_hash: Hash,
    relay_number: BlockNumber,
) -> Result<ActivationInfo> {
    let recent_block = match view.recent_block {
        Some((recent_block_num, recent_block_hash)) if relay_number < recent_block_num => {
            // the existing recent block is not worse than the new activation, so leave it.
            (recent_block_num, recent_block_hash)
        },
        _ => (relay_number, relay_hash),
    };

    let (tx, rx) = oneshot::channel();
    sender.send_message(ChainApiMessage::BlockHeader(relay_hash, tx)).await;
    let header = rx
        .map_err(JfyiError::OverseerCommunication)
        .await?
        .map_err(JfyiError::ChainApiCallError)?;

    let session_idx = request_session_index_for_child(relay_hash, &mut sender)
        .await
        .await
        .map_err(JfyiError::OverseerCommunication)?
        .map_err(JfyiError::RuntimeApiCallError)?;

    let new_session_info = if !view.per_session.contains_key(&session_idx) {
        let session_info = request_session_info(relay_hash, session_idx, &mut sender)
            .await
            .await
            .map_err(JfyiError::OverseerCommunication)?
            .map_err(JfyiError::RuntimeApiCallError)?;

        let (tx, rx) = oneshot::channel();
        let validators = runtime_api_request(
            &mut sender,
            relay_hash,
            RuntimeApiRequest::Validators(tx),
            rx,
        )
            .await
            .map_err(JfyiError::RuntimeApiCallError)?;

        let signing_credentials = polkadot_node_subsystem_util::signing_key_and_index(&validators, keystore)
            .map(|(validator_key, validator_index)|
                SigningCredentials { validator_key, validator_index });

        if let Some(session_info) = session_info {
            Some((session_info, signing_credentials))
        } else {
            None
        }
    } else {
        None
    };

    Ok(ActivationInfo {
        activated_header: header,
        recent_block,
        session_index: session_idx,
        new_session_info,
    })
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
async fn prune_old_session_views<Sender: SubsystemSender<RuntimeApiMessage>>(
    mut sender: Sender,
    keystore: &KeystorePtr,
    view: &mut View,
    finalized_hash: Hash,
    metrics: &Metrics,
) -> Result<()> {
    let recent_block_hash = match view.recent_block {
        Some((_, block_hash)) => block_hash,
        None => {
            gum::debug!(
                target: LOG_TARGET,
                ?finalized_hash,
                "recent block does not exist or got erased, cannot submit finalized session statistics"
            );
            return Ok(());
        },
    };

    let finalized_session = request_session_index_for_child(finalized_hash, &mut sender)
        .await
        .await
        .map_err(JfyiError::OverseerCommunication)?
        .map_err(JfyiError::RuntimeApiCallError)?;

    match view.current_session {
        Some(current_session) if current_session < finalized_session => {
            // the previous session was finalized
            for (session_idx, session_view) in view
                .per_session
                .iter()
                .filter(|stored_session_idx| stored_session_idx.0 < &finalized_session) {

                if let Some(ref credentials) = session_view.credentials {
                    sign_and_submit_approvals_tallies(
                        &mut sender,
                        recent_block_hash,
                        session_idx,
                        keystore,
                        credentials,
                        metrics,
                        session_view.validators_tallies.clone(),
                    ).await;
                }

                if let Some(wipe_before) = session_idx.checked_sub(MAX_SESSIONS_TO_KEEP.get()) {
                    view.per_session.retain(|stored_session_index, _| *stored_session_index > wipe_before);
                }

                view.current_session = Some(finalized_session);
            }
            
        }
        None => view.current_session = Some(current_fin_session),
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

async fn sign_and_submit_approvals_tallies<
    Sender: SubsystemSender<RuntimeApiMessage>,
>(
    mut sender: Sender,
    relay_parent: Hash,
    session_index: &SessionIndex,
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

    let payload = ApprovalStatistics(session_index.clone(), approvals_tallies);

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
        &mut sender,
        relay_parent,
        RuntimeApiRequest::SubmitApprovalStatistics(payload, signature, tx),
        rx,
    ).await;

    match runtime_req {
        Ok(()) => {
            metrics.on_approvals_submitted();
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

async fn runtime_api_request<
    T,
    Sender: SubsystemSender<RuntimeApiMessage>,
>(
    mut sender: Sender,
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
