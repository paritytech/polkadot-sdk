use std::collections::{HashMap, HashSet};
use polkadot_primitives::{CandidateHash, Hash, SessionIndex, ValidatorIndex};
use crate::View;
use crate::error::Result;

#[derive(Debug, Clone, Default)]
pub struct ApprovalsStats {
    pub votes: HashSet<ValidatorIndex>,
}

impl ApprovalsStats {
    pub fn new(votes: HashSet<ValidatorIndex>) -> Self {
        Self { votes }
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
            .or_default()
            .and_modify(|a: &mut ApprovalsStats| {
                a.votes.extend(approvals.into());
            });
    }
}

pub fn handle_observed_no_shows(
    view: &mut View,
    session_index: SessionIndex,
    no_show_validators: Vec<ValidatorIndex>,
) {
    view.no_shows_per_session
        .entry(session_index)
        .and_modify(|q: &mut HashMap<ValidatorIndex, usize>| {
            for v_idx in no_show_validators {
                q.entry(*v_idx)
                    .and_modify(|v: &mut usize| *v += 1)
                    .or_insert(1);
            }
        })
        .or_insert(HashMap::new());
}