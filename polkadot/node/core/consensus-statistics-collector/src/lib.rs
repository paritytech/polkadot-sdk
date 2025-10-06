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


use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry;
use futures::{channel::oneshot, prelude::*};
use gum::CandidateHash;
use polkadot_node_subsystem::{
    overseer, ActiveLeavesUpdate, FromOrchestra, OverseerSignal, SpawnedSubsystem, SubsystemError,
};
use polkadot_node_subsystem::messages::ConsensusStatisticsCollectorMessage;
use polkadot_primitives::{AuthorityDiscoveryId, Hash, SessionIndex, ValidatorIndex};
use polkadot_node_primitives::approval::time::Tick;
use polkadot_node_primitives::approval::v1::DelayTranche;
use polkadot_primitives::well_known_keys::relay_dispatch_queue_remaining_capacity;
use crate::{
    error::{FatalError, FatalResult, JfyiError, JfyiErrorResult, Result},
};

mod error;
mod metrics;
#[cfg(test)]
mod tests;
mod approval_voting_metrics;
mod availability_distribution_metrics;

use approval_voting_metrics::ApprovalsStats;
use polkadot_node_subsystem_util::{request_session_index_for_child, request_session_info};
use crate::approval_voting_metrics::{handle_candidate_approved, handle_observed_no_shows};
use crate::availability_distribution_metrics::{handle_chunks_downloaded, AvailabilityChunks};
use self::metrics::Metrics;

const LOG_TARGET: &str = "parachain::consensus-statistics-collector";

struct PerRelayView {
    relay_approved: bool,
    approvals_stats: HashMap<CandidateHash, ApprovalsStats>,
}

impl PerRelayView {
    fn new(candidates: Vec<CandidateHash>) -> Self {
        return PerRelayView{
            relay_approved: false,
            approvals_stats: HashMap::new(),
        }
    }
}

pub struct PerSessionView {
    authorities_lookup: HashMap<AuthorityDiscoveryId, ValidatorIndex>,
}

impl PerSessionView {
    fn new(authorities_lookup: HashMap<AuthorityDiscoveryId, ValidatorIndex>) -> Self {
        Self { authorities_lookup }
    }
}

struct View {
    per_relay: HashMap<Hash, PerRelayView>,
    per_session: HashMap<SessionIndex, PerSessionView>,
    // TODO: this information should not be needed
    candidates_per_session: HashMap<SessionIndex, HashSet<CandidateHash>>,
    availability_chunks: AvailabilityChunks,
}

impl View {
    fn new() -> Self {
        return View{
            per_relay: HashMap::new(),
            per_session: HashMap::new(),
            candidates_per_session: HashMap::new(),
            availability_chunks: AvailabilityChunks::new(),
        };
    }
}

/// The statistics collector subsystem.
#[derive(Default)]
pub struct ConsensusStatisticsCollector {
    metrics: Metrics,
}

impl ConsensusStatisticsCollector {
    /// Create a new instance of the `ConsensusStatisticsCollector`.
    pub fn new(metrics: Metrics) -> Self {
        Self { metrics }
    }
}

#[overseer::subsystem(ConsensusStatisticsCollector, error = SubsystemError, prefix = self::overseer)]
impl<Context> ConsensusStatisticsCollector
where
    Context: Send + Sync,
{
    fn start(self, ctx: Context) -> SpawnedSubsystem {
        SpawnedSubsystem {
            future: run(ctx, self.metrics)
                .map_err(|e| SubsystemError::with_origin("statistics-parachains", e))
                .boxed(),
            name: "consensus-statistics-collector-subsystem",
        }
    }
}

#[overseer::contextbounds(ConsensusStatisticsCollector, prefix = self::overseer)]
async fn run<Context>(mut ctx: Context, metrics: Metrics) -> FatalResult<()> {
    let mut view = View::new();
    loop {
        crate::error::log_error(
            run_iteration(&mut ctx, &mut view, &metrics).await,
            "Encountered issue during run iteration",
        )?;
    }
}

#[overseer::contextbounds(ConsensusStatisticsCollector, prefix = self::overseer)]
pub(crate) async fn run_iteration<Context>(
    ctx: &mut Context,
    view: &mut View,
    metrics: &Metrics,
) -> Result<()> {
    let mut sender = ctx.sender().clone();
    loop {
        match ctx.recv().await.map_err(FatalError::SubsystemReceive)? {
            FromOrchestra::Signal(OverseerSignal::Conclude) => return Ok(()),
            FromOrchestra::Signal(OverseerSignal::ActiveLeaves(update)) => {
                if let Some(activated) = update.activated {
                    view.per_relay.insert(activated.hash, PerRelayView::new(vec![]));

                    let session_idx = request_session_index_for_child(activated.hash, ctx.sender())
                            .await
                            .await
                            .map_err(JfyiError::OverseerCommunication)?
                            .map_err(JfyiError::RuntimeApiCallError)?;

                    if !view.per_session.contains_key(&session_idx) {
                        let session_info = request_session_info(activated.hash, session_idx, ctx.sender())
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
            FromOrchestra::Signal(OverseerSignal::BlockFinalized(..)) => {},
            FromOrchestra::Communication { msg } => {
                match msg {
                    ConsensusStatisticsCollectorMessage::ChunksDownloaded(
                        session_index, candidate_hash, downloads)=> {
                        handle_chunks_downloaded(
                            view,
                            session_index,
                            candidate_hash,
                            downloads,
                        )
                    },
                    ConsensusStatisticsCollectorMessage::ChunkUploaded(candidate_hash, authority_ids) => {
                        handle_chunk_uploaded(
                            view,
                            candidate_hash,
                            authority_ids,
                        )
                    },
                    ConsensusStatisticsCollectorMessage::CandidateApproved(candidate_hash, block_hash, approvals) => {
                        handle_candidate_approved(
                            view,
                            block_hash,
                            candidate_hash,
                            approvals,
                        );
                    }
                    ConsensusStatisticsCollectorMessage::NoShows(candidate_hash, block_hash, no_show_validators) => {
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