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
use std::ops::Add;
use gum::CandidateHash;
use polkadot_primitives::{AuthorityDiscoveryId, SessionIndex, ValidatorIndex};
use crate::View;

pub struct AvailabilityChunks {
    pub downloads_per_candidate: HashMap<CandidateHash, HashMap<ValidatorIndex, u64>>,
    pub uploads_per_candidate: HashMap<CandidateHash, HashMap<ValidatorIndex, u64>>,
}

impl AvailabilityChunks {
    pub fn new() -> Self {
        Self {
            downloads_per_candidate: Default::default(),
            uploads_per_candidate: Default::default(),
        }
    }

    pub fn note_candidate_chunk_downloaded(
        &mut self,
        candidate_hash: CandidateHash,
        validator_index: ValidatorIndex,
        count: u64,
    ) {
        let _ = self.downloads_per_candidate
            .entry(candidate_hash)
            .or_default()
            .entry(validator_index)
            .and_modify(|v| *v += count)
            .or_insert(count);
    }

    pub fn note_candidate_chunk_uploaded(
        &mut self,
        candidate_hash: CandidateHash,
        validator_index: ValidatorIndex,
        count: u64,
    ) {
        let _ = self.uploads_per_candidate
            .entry(candidate_hash)
            .or_default()
            .entry(validator_index)
            .and_modify(|v| *v += count)
            .or_insert(count);
    }
}

// whenever chunks are acquired throughout availability
// recovery we collect the metrics about what validator
// provided and the amount of chunks
pub fn handle_chunks_downloaded(
    view: &mut View,
    session_index: SessionIndex,
    candidate_hash: CandidateHash,
    downloads: HashMap<ValidatorIndex, u64>,
) {
    view.candidates_per_session
        .entry(session_index)
        .or_default()
        .insert(candidate_hash);

    for (validator_index, download_count) in downloads {
        view.availability_chunks
            .entry(session_index)
            .and_modify(|av_chunks_stats| av_chunks_stats.note_candidate_chunk_downloaded(candidate_hash, validator_index, download_count))
            .or_insert(AvailabilityChunks::new());
    }
}

pub fn handle_chunk_uploaded(
    view: &mut View,
    candidate_hash: CandidateHash,
    authority_ids: HashSet<AuthorityDiscoveryId>,
) {
    // check if candidate is present
    let sessions = view.candidates_per_session
        .iter()
        .filter_map(|(session_index, candidates)| {
            if candidates.contains(&candidate_hash) {
                return Some(session_index);
            }

            None
        });

    for session_index in sessions {
        let validator_index = view.per_session.get(session_index).and_then(|session_view| {
            for authority_id in authority_ids.iter() {
                match session_view.authorities_lookup.get(authority_id) {
                    Some(validator_idx) => return Some(validator_idx),
                    None => continue,
                }
            }
            None
        });

        if validator_index.is_none() {
            continue;
        }

        view.availability_chunks
            .entry(session_index.clone())
            .and_modify(|av_chunks_stats| av_chunks_stats
                .note_candidate_chunk_uploaded(candidate_hash, validator_index.unwrap().clone(), 1))
            .or_insert(AvailabilityChunks::new());
    }
}