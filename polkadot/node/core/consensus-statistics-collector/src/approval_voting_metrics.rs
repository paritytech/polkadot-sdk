use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry;
use polkadot_primitives::{CandidateHash, Hash, SessionIndex, ValidatorIndex};
use crate::View;

#[derive(Debug, Clone, Default)]
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
    approvals: Vec<ValidatorIndex>
) {
    if let Some(relay_view) = view.per_relay.get_mut(&block_hash) {
        relay_view.approvals_stats
            .entry(candidate_hash)
            .and_modify(|a: &mut ApprovalsStats| a.votes.extend(approvals.iter()))
            .or_insert(ApprovalsStats::new(HashSet::from_iter(approvals), HashSet::new()));
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