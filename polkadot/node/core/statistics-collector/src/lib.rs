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

use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry;
use futures::{channel::oneshot, prelude::*};
use gum::CandidateHash;
use polkadot_node_subsystem::{
    overseer, ActiveLeavesUpdate, FromOrchestra, OverseerSignal, SpawnedSubsystem, SubsystemError,
};
use polkadot_node_subsystem::messages::StatisticsCollectorMessage;
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


use self::metrics::Metrics;

const LOG_TARGET: &str = "parachain::statistics-collector";

struct ApprovalsStats {
    votes: HashSet<ValidatorIndex>,
}

impl ApprovalsStats {
    fn new(votes: HashSet<ValidatorIndex>) -> Self {
        Self { votes }
    }
}

struct PerRelayView {
    relay_approved: bool,
    included_candidates: Vec<CandidateHash>,
    approvals_stats: HashMap<CandidateHash, ApprovalsStats>,
    votes_per_tranche: HashMap<DelayTranche, usize>,
}

impl PerRelayView {
    fn new(candidates: Vec<CandidateHash>) -> Self {
        return PerRelayView{
            relay_approved: false,
            included_candidates: candidates,
            approvals_stats: HashMap::new(),
            votes_per_tranche: HashMap::new(),
        }
    }
}

struct View {
    per_relay_parent: HashMap<Hash, PerRelayView>,
    no_shows_per_session: HashMap<SessionIndex, HashMap<ValidatorIndex, usize>>,
}

impl View {
    fn new() -> Self {
        return View{
            per_relay_parent: HashMap::new(),
            no_shows_per_session: HashMap::new(),
        };
    }
}

/// The statistics collector subsystem.
#[derive(Default)]
pub struct StatisticsCollectorSubsystem {
    metrics: Metrics,
}

impl StatisticsCollectorSubsystem {
    /// Create a new instance of the `StatisticsCollectorSubsystem`.
    pub fn new(metrics: Metrics) -> Self {
        Self { metrics }
    }
}

#[overseer::subsystem(StatisticsCollector, error = SubsystemError, prefix = self::overseer)]
impl<Context> StatisticsCollectorSubsystem
where
    Context: Send + Sync,
{
    fn start(self, ctx: Context) -> SpawnedSubsystem {
        SpawnedSubsystem {
            future: run(ctx, self.metrics)
                .map_err(|e| SubsystemError::with_origin("statistics-parachains", e))
                .boxed(),
            name: "statistics-collector-subsystem",
        }
    }
}

#[overseer::contextbounds(StatisticsCollector, prefix = self::overseer)]
async fn run<Context>(mut ctx: Context, metrics: Metrics) -> FatalResult<()> {
    let mut view = View::new();
    loop {
        crate::error::log_error(
            run_iteration(&mut ctx, &mut view, &metrics).await,
            "Encountered issue during run iteration",
        )?;
    }
}

#[overseer::contextbounds(StatisticsCollector, prefix = self::overseer)]
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
                    view.per_relay_parent.insert(actived.hash, PerRelayView::new(vec![]));
                }
            },
            FromOrchestra::Signal(OverseerSignal::BlockFinalized(..)) => {},
            FromOrchestra::Communication { msg } => {
                match msg {
                    StatisticsCollectorMessage::CandidateApproved(candidate_hash, block_hash, approvals) => {
                        if let Some(relay_view) = view.per_relay_parent.get_mut(&block_hash) {
                            relay_view.approvals_stats
                                .entry(candidate_hash)
                                .and_modify(|a: &mut ApprovalsStats| {
                                    for v_idx in approvals.iter_ones() {
                                        a.votes.insert(ValidatorIndex(v_idx as u32));
                                    }
                                });
                        }
                    }
                    StatisticsCollectorMessage::ObservedNoShows(session_idx, no_show_validators) => {
                        view.no_shows_per_session
                            .entry(session_idx)
                            .and_modify(|q: &mut HashMap<ValidatorIndex, usize>| {
                                for v_idx in no_show_validators {
                                    q.entry(*v_idx)
                                        .and_modify(|v: &mut usize| *v += 1)
                                        .or_insert(1);
                                }
                            })
                            .or_insert(HashMap::new());
                    },
                    StatisticsCollectorMessage::RelayBlockApproved(block_hash) => {
                        view.per_relay_parent
                            .entry(block_hash)
                            .and_modify(|q| q.relay_approved = true);
                    },
                }
            },
        }
    }
}