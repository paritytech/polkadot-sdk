use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry;
use std::ops::Add;
use gum::CandidateHash;
use polkadot_primitives::{SessionIndex, ValidatorIndex};
use crate::View;

pub struct AvailabilityChunks {
    pub downloads_per_candidate: HashMap<CandidateHash, HashMap<ValidatorIndex, u64>>,
}

impl AvailabilityChunks {
    pub fn new() -> Self {
        Self {
            downloads_per_candidate: Default::default(),
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
            .note_candidate_chunk_downloaded(candidate_hash, validator_index, download_count)
    }
}

pub fn handle_chunk_uploaded(

) {

}