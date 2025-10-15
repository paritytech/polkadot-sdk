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
use polkadot_node_subsystem::{
    overseer, ActiveLeavesUpdate, FromOrchestra, OverseerSignal, SpawnedSubsystem, SubsystemError,
};
use polkadot_node_subsystem::messages::{ChainApiMessage, ConsensusStatisticsCollectorMessage};
use polkadot_primitives::{AuthorityDiscoveryId, BlockNumber, Hash, Header, SessionIndex, ValidatorIndex};
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
use polkadot_node_subsystem_util::{request_candidate_events, request_session_index_for_child, request_session_info};
use crate::approval_voting_metrics::{handle_candidate_approved, handle_observed_no_shows};
use crate::availability_distribution_metrics::{handle_chunk_uploaded, handle_chunks_downloaded, AvailabilityChunks};
use self::metrics::Metrics;

const LOG_TARGET: &str = "parachain::consensus-statistics-collector";

struct PerRelayView {
    parent_hash: Option<Hash>,
    children: HashSet<Hash>,
    approvals_stats: HashMap<CandidateHash, ApprovalsStats>,
}

impl PerRelayView {
    fn new(parent_hash: Option<Hash>) -> Self {
        PerRelayView{
            parent_hash: parent_hash,
            children: HashSet::new(),
            approvals_stats: HashMap::new(),
        }
    }

    fn link_child(&mut self, hash: Hash) {
        self.children.insert(hash);
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct PerSessionView {
    authorities_lookup: HashMap<AuthorityDiscoveryId, ValidatorIndex>,
}

impl PerSessionView {
    fn new(authorities_lookup: HashMap<AuthorityDiscoveryId, ValidatorIndex>) -> Self {
        Self { authorities_lookup }
    }
}

struct View {
    latest_finalized: Option<(Hash, BlockNumber)>,
    roots: HashSet<Hash>,
    per_relay: HashMap<Hash, PerRelayView>,
    per_session: HashMap<SessionIndex, PerSessionView>,
    availability_chunks: HashMap<SessionIndex, AvailabilityChunks>,

}

impl View {
    fn new() -> Self {
        return View{
            latest_finalized: None,
            roots: HashSet::new(),
            per_relay: HashMap::new(),
            per_session: HashMap::new(),
            availability_chunks: HashMap::new()
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
                    let relay_hash = activated.hash;

                    let (tx, rx) = oneshot::channel();

                    ctx.send_message(ChainApiMessage::BlockHeader(relay_hash, tx)).await;
                    let header = rx
                        .map_err(JfyiError::OverseerCommunication)
                        .await?
                        .map_err(JfyiError::ChainApiCallError)?;

                    if let Some(ref h) = header {
                        let parent_hash = h.clone().parent_hash;
                        if let Some(parent) = view.per_relay.get_mut(&parent_hash) {
                            parent.link_child(relay_hash.clone());
                        } else {
                            view.roots.insert(relay_hash.clone());
                        }
                        view.per_relay.insert(relay_hash, PerRelayView::new(Some(parent_hash)));
                    } else {
                        view.roots.insert(relay_hash.clone());
                        view.per_relay.insert(relay_hash, PerRelayView::new(None));
                    }

                    let session_idx = request_session_index_for_child(relay_hash, ctx.sender())
                            .await
                            .await
                            .map_err(JfyiError::OverseerCommunication)?
                            .map_err(JfyiError::RuntimeApiCallError)?;

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
            FromOrchestra::Signal(OverseerSignal::BlockFinalized(fin_block_hash, fin_block_num)) => {
                let latest_finalized = match view.latest_finalized {
                    Some(latest_finalized) => latest_finalized,
                    None => (fin_block_hash, fin_block_num),
                };

                // update the latest finalized on the view
                view.latest_finalized = Some((fin_block_hash, fin_block_num));

                // since we want to reward only valid approvals, we retain
                // only finalized chain blocks and its descendants
                // identify the finalized chain so we don't prune
                let rb_view = match view.per_relay.get_mut(&fin_block_hash) {
                    Some(per_relay_view) => per_relay_view,
                    //TODO: the finalized block should already exists on the relay view mapping
                    None => continue
                };

                let mut removal_stack = Vec::new();
                let mut retain_relay_hashes = Vec::new();
                retain_relay_hashes.push(fin_block_hash);

                let mut current_block_hash = fin_block_hash;
                let mut current_parent_hash = rb_view.parent_hash;
                while let Some(parent_hash) = current_parent_hash {
                    retain_relay_hashes.push(parent_hash.clone());

                    match view.per_relay.get_mut(&parent_hash) {
                        Some(parent_view) => {
                            if parent_view.children.len() > 1 {
                                let filtered_set = parent_view.children
                                    .iter()
                                    .filter(|&child_hash| child_hash.eq(&current_block_hash))
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

                if view.roots.len() > 1 {
                    for root in view.roots.clone() {
                        if !retain_relay_hashes.contains(&root) {
                            removal_stack.push(root);
                        }
                    }
                }

                let mut to_prune = HashSet::new();
                let mut queue: VecDeque<Hash> = VecDeque::from(removal_stack);

                while let Some(hash) = queue.pop_front() {
                    if !to_prune.insert(hash) {
                        continue; // already seen
                    }

                    if let Some(r_view) = view.per_relay.get(&hash) {
                        for child in &r_view.children {
                            queue.push_back(child.clone());
                        }
                    }
                }

                for rb_hash in to_prune {
                    view.per_relay.remove(&rb_hash);
                    view.roots.remove(&rb_hash);
                }
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
                            metrics,
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