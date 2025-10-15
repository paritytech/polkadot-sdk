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
use crate::{PerSessionView, View};

#[derive(Debug, Clone, PartialEq, Eq)]
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
        let validator_downloads = self.downloads_per_candidate
            .entry(candidate_hash)
            .or_default()
            .entry(validator_index);

        match validator_downloads {
            Entry::Occupied(mut validator_downloads) => {
                *validator_downloads.get_mut() += count;
            }
            Entry::Vacant(entry) => { entry.insert(count); }
        }
    }

    pub fn note_candidate_chunk_uploaded(
        &mut self,
        candidate_hash: CandidateHash,
        validator_index: ValidatorIndex,
        count: u64,
    ) {
        let validator_uploads = self.uploads_per_candidate
            .entry(candidate_hash)
            .or_default()
            .entry(validator_index);

        match validator_uploads {
            Entry::Occupied(mut validator_uploads) => {
                *validator_uploads.get_mut() += count;
            }
            Entry::Vacant(entry) => { entry.insert(count); }
        }
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
    let av_chunks = view.availability_chunks
        .entry(session_index)
        .or_insert(AvailabilityChunks::new());

    for (validator_index, download_count) in downloads {
        av_chunks.note_candidate_chunk_downloaded(candidate_hash, validator_index, download_count)
    }
}

pub fn handle_chunk_uploaded(
    view: &mut View,
    candidate_hash: CandidateHash,
    authority_ids: HashSet<AuthorityDiscoveryId>,
) {
    // will look up in the stored sessions,
    // from the most recent session to the oldest session
    // to find the first validator index that matches
    // with a single authority discovery id from the set

    let mut sessions: Vec<(&SessionIndex, &PerSessionView)> = view.per_session.iter().collect();
    sessions.sort_by(|(a, _), (b, _)| a.partial_cmp(&b).unwrap());

    for (session_idx, session_view) in sessions {
        // Find the first authority with a matching validator index
        if let Some(validator_idx) = authority_ids
            .iter()
            .find_map(|id| session_view.authorities_lookup.get(id).map(|v| v))
        {
            let av_chunks = view.availability_chunks.entry(*session_idx);
            match av_chunks {
                Entry::Occupied(mut entry) => {
                    entry.get_mut()
                        .note_candidate_chunk_uploaded(candidate_hash, *validator_idx, 1);
                }
                Entry::Vacant(entry) => {
                    entry.insert(AvailabilityChunks::new())
                        .note_candidate_chunk_uploaded(candidate_hash, *validator_idx, 1);
                }
            }
            break;
        }
    }
}