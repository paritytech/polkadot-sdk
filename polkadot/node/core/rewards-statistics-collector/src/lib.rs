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

//! Implementation of the Rewards Statistics Collector subsystem.
//! This component monitors and manages metrics related to parachain candidate approvals,
//! including approval votes, distribution of approval chunks, chunk downloads, and chunk uploads.
//!
//! Its primary responsibility is to collect and track data reflecting node’s perspective
//! on the approval work carried out by all session validators.

mod approval_voting_metrics;
mod availability_distribution_metrics;
mod error;
pub mod metrics;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::collections::hash_map::Entry;
use std::task::Context;
use futures::{channel::oneshot, prelude::*};
use sp_keystore::KeystorePtr;
use polkadot_node_primitives::{
    approval::{
        time::Tick,
        v1::DelayTranche
    },
    new_session_window_size, 
    SessionWindowSize, 
    DISPUTE_WINDOW
};
use polkadot_node_subsystem::{
    errors::RuntimeApiError as RuntimeApiSubsystemError,
    messages::{
        ChainApiMessage, RewardsStatisticsCollectorMessage,
        RuntimeApiMessage, RuntimeApiRequest
    },
    overseer, ActiveLeavesUpdate, FromOrchestra, OverseerSignal, SpawnedSubsystem, SubsystemError, SubsystemSender
};
use polkadot_primitives::{AuthorityDiscoveryId, BlockNumber, Hash, Header, SessionIndex, ValidatorId, ValidatorIndex, well_known_keys::relay_dispatch_queue_remaining_capacity, SessionInfo};
use crate::{
    error::{FatalError, FatalResult, JfyiError, Result},
};
use self::metrics::Metrics;
use crate::{
	approval_voting_metrics::{ApprovalsStats, handle_candidate_approved, handle_observed_no_shows},
	availability_distribution_metrics::{
		handle_chunk_uploaded, handle_chunks_downloaded, AvailabilityChunks,
	},
};
use polkadot_node_subsystem_util::{request_session_index_for_child, request_session_info};
use polkadot_primitives::vstaging::{ApprovalStatistics, ApprovalStatisticsTallyLine};

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
		PerRelayView { session_index, approvals_stats: ApprovalsStats::default() }
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
    credentials: Option<SigningCredentials>,
    authorities_ids: Vec<AuthorityDiscoveryId>,
    validators_tallies: HashMap<ValidatorIndex, PerValidatorTally>,
}

impl PerSessionView {
    fn new(
        authorities_ids: Vec<AuthorityDiscoveryId>,
        credentials: Option<SigningCredentials>,
    ) -> Self {
        Self {
            authorities_ids,
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
    
    latest_finalized_session: Option<SessionIndex>,
    latest_finalized_block: (BlockNumber, Hash),
    
    /// latest activated leaf
    recent_block: Option<(BlockNumber, Hash)>,
}

impl View {
    fn new() -> Self {
        View {
			per_relay: HashMap::new(),
			per_session: BTreeMap::new(),
			availability_chunks: BTreeMap::new(),
			latest_finalized_block: (0, Hash::default()),
            latest_finalized_session: None,
            recent_block: None,
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
    /// Create a new instance of the `RewardsStatisticsCollector`.
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
            future: run(ctx, self.keystore, (self.metrics, self.config.verbose_approval_metrics))
                .map_err(|e| SubsystemError::with_origin("statistics-parachains", e))
                .boxed(),
            name: "rewards-statistics-collector-subsystem",
        }
    }
}

#[overseer::contextbounds(RewardsStatisticsCollector, prefix = self::overseer)]
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
    loop {
        match ctx.recv().await.map_err(FatalError::SubsystemReceive)? {
            FromOrchestra::Signal(OverseerSignal::Conclude) => return Ok(()),
            FromOrchestra::Signal(OverseerSignal::ActiveLeaves(update)) => {
                if let Some(activated) = update.activated {
                    let relay_hash = activated.hash;
					let relay_number = activated.number;

                    let ActivationInfo {
                        activated_header,
                        session_index,
                        new_session_info,
                        recent_block,
                    } = extract_activated_leaf_info(
                        ctx.sender(),
                        view,
                        keystore,
                        relay_hash,
                        relay_number,
                    ).await?;

                    view.recent_block = Some(recent_block);
                    view.per_relay
			            .insert((relay_hash, relay_number), PerRelayView::new(session_index));
                    
                    prune_based_on_session_windows(
						view,
						session_index,
						MAX_SESSION_VIEWS_TO_KEEP,
						MAX_AVAILABILITIES_TO_KEEP,
					);

                    if let Some((session_info, credentials)) = new_session_info {
                        view.per_session.insert(
                            session_index, 
                            PerSessionView::new(
                                session_info.discovery_keys.iter().cloned().collect(),
                                credentials,
                            ),
                        );
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
				let ancestor_req_message = ChainApiMessage::Ancestors {
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

				let (mut before, after): (HashMap<_, _>, HashMap<_, _>) = view
					.per_relay
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

                submit_finalized_session_stats(
                    ctx.sender(),
                    keystore,
                    view,
                    fin_block_hash,
                    metrics.0,
                ).await?;
            },
			FromOrchestra::Communication { msg } => match msg {
				RewardsStatisticsCollectorMessage::ChunksDownloaded(session_index, downloads) =>
					handle_chunks_downloaded(view, session_index, downloads),
				RewardsStatisticsCollectorMessage::ChunkUploaded(session_index, authority_ids) =>
					handle_chunk_uploaded(view, session_index, authority_ids),
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

struct ActivationInfo {
    activated_header: Option<Header>,
    recent_block: (BlockNumber, Hash),
    session_index: SessionIndex,
    new_session_info: Option<(SessionInfo, Option<SigningCredentials>)>,
}

async fn extract_activated_leaf_info(
    sender: &mut impl overseer::RewardsStatisticsCollectorSenderTrait,
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

    let session_idx = request_session_index_for_child(relay_hash, sender)
        .await
        .await
        .map_err(JfyiError::OverseerCommunication)?
        .map_err(JfyiError::RuntimeApiCallError)?;

    let new_session_info = if !view.per_session.contains_key(&session_idx) {
        let session_info = request_session_info(relay_hash, session_idx, sender)
            .await
            .await
            .map_err(JfyiError::OverseerCommunication)?
            .map_err(JfyiError::RuntimeApiCallError)?;

        let (tx, rx) = oneshot::channel();
        let validators = runtime_api_request(
            sender,
            relay_hash,
            RuntimeApiRequest::Validators(tx),
            rx,
        ).await?;

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

// submit_finalized_session_stats works after a whole session is finalized
// getting all the collected data and submitting to the runtime, after the
// submition the data is cleaned from mapping
async fn submit_finalized_session_stats(
    sender: &mut impl SubsystemSender<RuntimeApiMessage>,
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

    let current_fin_session = request_session_index_for_child(finalized_hash, sender)
        .await
        .await
        .map_err(JfyiError::OverseerCommunication)?
        .map_err(JfyiError::RuntimeApiCallError)?;

    match view.latest_finalized_session {
        Some(latest_fin_session) if latest_fin_session < current_fin_session => {
            // the previous session was finalized
            for (session_idx, session_view) in view
                .per_session
                .iter()
                .filter(|stored_session_idx| stored_session_idx.0 < &current_fin_session) {

                if let Some(ref credentials) = session_view.credentials {
                    sign_and_submit_approvals_tallies(
                        sender,
                        recent_block_hash,
                        session_idx,
                        keystore,
                        credentials,
                        metrics,
                        session_view.validators_tallies.clone(),
                    ).await;
                }
            }

            view.per_session.retain(|session_index, _| *session_index >= current_fin_session);
            view.latest_finalized_session = Some(current_fin_session);
        }
        None => view.latest_finalized_session = Some(current_fin_session),
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
			"session collected statistics",
		);
	}
}

async fn sign_and_submit_approvals_tallies(
    sender: &mut impl SubsystemSender<RuntimeApiMessage>,
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

    let payload = ApprovalStatistics(session_index.clone(), credentials.validator_index, approvals_tallies);

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

async fn runtime_api_request<T>(
    sender: &mut impl SubsystemSender<RuntimeApiMessage>,
    relay_parent: Hash,
    request: RuntimeApiRequest,
    receiver: oneshot::Receiver<std::result::Result<T, RuntimeApiSubsystemError>>,
) -> std::result::Result<T, JfyiError> {
    sender
        .send_message(RuntimeApiMessage::Request(relay_parent, request).into())
        .await;

    receiver
        .map_err(JfyiError::OverseerCommunication)
        .await?
        .map_err(JfyiError::RuntimeApiCallError)
}
