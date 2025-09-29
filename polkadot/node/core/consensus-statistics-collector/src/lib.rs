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
use polkadot_primitives::{Hash, SessionIndex, ValidatorIndex};
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
use crate::approval_voting_metrics::{handle_candidate_approved, handle_observed_no_shows};
use crate::availability_distribution_metrics::{handle_chunks_downloaded, AvailabilityDownloads};
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

struct View {
    per_relay: HashMap<Hash, PerRelayView>,
    no_shows_per_session: HashMap<SessionIndex, HashMap<ValidatorIndex, usize>>,
    candidates_per_session: HashMap<SessionIndex, HashSet<CandidateHash>>,
    chunks_downloaded: AvailabilityDownloads,
}

impl View {
    fn new() -> Self {
        return View{
            per_relay: HashMap::new(),
            no_shows_per_session: HashMap::new(),
            candidates_per_session: HashMap::new(),
            chunks_downloaded: AvailabilityDownloads::new(),
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
    loop {
        match ctx.recv().await.map_err(FatalError::SubsystemReceive)? {
            FromOrchestra::Signal(OverseerSignal::Conclude) => return Ok(()),
            FromOrchestra::Signal(OverseerSignal::ActiveLeaves(update)) => {
                if let Some(actived) = update.activated {
                    view.per_relay.insert(actived.hash, PerRelayView::new(vec![]));
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
                    ConsensusStatisticsCollectorMessage::CandidateApproved(candidate_hash, block_hash, approvals) => {
                        handle_candidate_approved(
                            view,
                            block_hash,
                            candidate_hash,
                            approvals,
                        );
                    }
                    ConsensusStatisticsCollectorMessage::NoShows(session_idx, no_show_validators) => {
                        handle_observed_no_shows(
                            view,
                            session_idx,
                            no_show_validators,
                        );
                    },
                }
            },
        }
    }
}