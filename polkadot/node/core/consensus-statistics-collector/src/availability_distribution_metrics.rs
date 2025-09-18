use std::collections::{HashMap, HashSet};
use std::ops::Add;
use gum::CandidateHash;
use polkadot_primitives::{SessionIndex, ValidatorIndex};
use crate::View;

pub struct AvailabilityDownloads {
    pub chunks_per_candidate: HashMap<CandidateHash, HashMap<ValidatorIndex, u64>>,
}

impl AvailabilityDownloads {
    pub fn new() -> Self {
        Self {
            chunks_per_candidate: Default::default(),
        }
    }

    pub fn note_candidate_chunk_downloaded(
        &mut self,
        hash: CandidateHash,
        validator_index: ValidatorIndex,
        count: u64,
    ) {
        self.chunks_per_candidate
            .entry(hash)
            .or_default()
            .entry(validator_index)
            .or_default()
            .add(count);
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
        view.chunks_downloaded
            .note_candidate_chunk_downloaded(candidate_hash, validator_index, download_count)
    }
}