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

use crate::{PerSessionView, View, LOG_TARGET};
use polkadot_primitives::{AuthorityDiscoveryId, CandidateHash, SessionIndex, ValidatorIndex};
use std::{
	collections::{hash_map::Entry, HashMap, HashSet},
};
use std::collections::btree_map;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailabilityChunks {
	pub downloads_per_candidate: HashMap<CandidateHash, HashMap<AuthorityDiscoveryId, u64>>,
	pub uploads_per_candidate: HashMap<CandidateHash, HashMap<AuthorityDiscoveryId, u64>>,
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
		authority_id: AuthorityDiscoveryId,
		count: u64,
	) {
		let validator_downloads = self
			.downloads_per_candidate
			.entry(candidate_hash)
			.or_default()
			.entry(authority_id);

		Self::increment_validator_counter(validator_downloads, count);
	}

	pub fn note_candidate_chunk_uploaded(
		&mut self,
		candidate_hash: CandidateHash,
		authority_id: AuthorityDiscoveryId,
		count: u64,
	) {
		let validator_uploads = self
			.uploads_per_candidate
			.entry(candidate_hash)
			.or_default()
			.entry(authority_id);

		Self::increment_validator_counter(validator_uploads, count);
	}

	fn increment_validator_counter(stats: Entry<AuthorityDiscoveryId, u64>, increment_by: u64) {
		match stats {
			Entry::Occupied(mut validator_stats) => {
				validator_stats.insert(validator_stats.get().saturating_add(increment_by));
			},
			Entry::Vacant(entry) => {
				entry.insert(increment_by);
			},
		}
	}
}

// whenever chunks are acquired throughout availability
// recovery we collect the metrics about which validator
// provided and the amount of chunks
pub fn handle_chunks_downloaded(
	view: &mut View,
	session_index: SessionIndex,
	candidate_hash: CandidateHash,
	downloads: HashMap<ValidatorIndex, u64>,
) {
	let av_chunks = view
		.availability_chunks
		.entry(session_index)
		.or_insert(AvailabilityChunks::new());

	for (validator_index, download_count) in downloads {
		let authority_id = view.per_session
			.get(&session_index)
			.and_then(|(session_view)| session_view.authorities_ids.get(validator_index.0 as usize));

		match authority_id {
			Some(authority_id) => {
				av_chunks.note_candidate_chunk_downloaded(candidate_hash, authority_id.clone(), download_count);
			}
			None => {
				gum::debug!(
					target: LOG_TARGET,
					validator_index = ?validator_index,
					download_count = download_count,
					session_idx = ?session_index,
					candidate_hash = ?candidate_hash,
					"could not find validator authority id"
				);
			}
		};
	}
}

// handle_chunk_uploaded receive the authority ids of the peer
// it just uploaded the candidate hash, to collect this statistic
// it needs to find the validator index that is bounded to any of the
// authority id, from the oldest to newest session.
pub fn handle_chunk_uploaded(
	view: &mut View,
	candidate_hash: CandidateHash,
	authority_ids: HashSet<AuthorityDiscoveryId>,
) {
	let auth_id = match authority_ids.iter().next() {
		Some(authority_id) => authority_id.clone(),
		None => {
			gum::debug!(
				target: LOG_TARGET,
				"unexpected empty authority ids while handling chunk uploaded"
			);

			return;
		},
	};

	// aggregate the statistic on the most up-to-date session
	if let Some((session_idx, session_view)) = view.per_session.iter().next_back() {
		let av_chunks = view.availability_chunks.entry(*session_idx);
		match av_chunks {
			btree_map::Entry::Occupied(mut entry) => {
				entry.get_mut().note_candidate_chunk_uploaded(
					candidate_hash,
					auth_id,
					1,
				);
			},
			btree_map::Entry::Vacant(entry) => {
				entry.insert(AvailabilityChunks::new()).note_candidate_chunk_uploaded(
					candidate_hash,
					auth_id,
					1,
				);
			},
		}
	}
}
