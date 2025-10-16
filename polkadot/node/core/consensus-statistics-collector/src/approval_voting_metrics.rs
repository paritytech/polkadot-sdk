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
use polkadot_primitives::{CandidateHash, Hash, SessionIndex, ValidatorIndex};
use crate::metrics::Metrics;
use crate::View;

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct ApprovalsStats {
    pub votes: HashSet<ValidatorIndex>,
    pub no_shows: HashSet<ValidatorIndex>,
}

impl ApprovalsStats {
    pub fn new(votes: HashSet<ValidatorIndex>, no_shows: HashSet<ValidatorIndex>) -> Self {
        Self { votes, no_shows }
    }
}

pub fn handle_candidate_approved(
    view: &mut View,
    block_hash: Hash,
    candidate_hash: CandidateHash,
    approvals: Vec<ValidatorIndex>,
    metrics: &Metrics,
) {
    if let Some(relay_view) = view.per_relay.get_mut(&block_hash) {
        relay_view.approvals_stats
            .entry(candidate_hash)
            .and_modify(|a: &mut ApprovalsStats| {
                metrics.record_approvals_usage(approvals.len() as u64);
                a.votes.extend(approvals.iter())
            })
            .or_insert_with(|| {
                metrics.record_approvals_usage(approvals.len() as u64);
                ApprovalsStats::new(HashSet::from_iter(approvals), HashSet::new())
            });
    }
}

pub fn  handle_observed_no_shows(
    view: &mut View,
    block_hash: Hash,
    candidate_hash: CandidateHash,
    no_show_validators: Vec<ValidatorIndex>,
) {
    if let Some(relay_view) = view.per_relay.get_mut(&block_hash) {
        relay_view.approvals_stats
            .entry(candidate_hash)
            .and_modify(|a: &mut ApprovalsStats| a.no_shows.extend(no_show_validators.iter()))
            .or_insert(ApprovalsStats::new(HashSet::new(), HashSet::from_iter(no_show_validators)));
    }
}