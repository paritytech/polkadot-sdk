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
use std::collections::{btree_map, BTreeMap};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailabilityChunks {
	pub downloads_per_candidate: BTreeMap<AuthorityDiscoveryId, u64>,
	pub uploads_per_candidate: BTreeMap<AuthorityDiscoveryId, u64>,
}

impl AvailabilityChunks {
	pub fn new() -> Self {
		Self {
			downloads_per_candidate: Default::default(),
			uploads_per_candidate: Default::default(),
		}
	}

	pub fn new_with_upload(auth_id: AuthorityDiscoveryId, count: u64) -> Self {
		Self {
			downloads_per_candidate: Default::default(),
			uploads_per_candidate: vec![(auth_id, count)].into_iter().collect()
		}
	}

	pub fn note_candidate_chunk_downloaded(
		&mut self,
		authority_id: AuthorityDiscoveryId,
		count: u64,
	) {
		self
			.downloads_per_candidate
			.entry(authority_id)
			.and_modify(|current| *current = current.saturating_add(count))
			.or_insert(count);

	}

	pub fn note_candidate_chunk_uploaded(
		&mut self,
		authority_id: AuthorityDiscoveryId,
		count: u64,
	) {
		self
			.uploads_per_candidate
			.entry(authority_id)
			.and_modify(|current| *current = current.saturating_add(count))
			.or_insert(count);
	}
}

// whenever chunks are acquired throughout availability
// recovery we collect the metrics about which validator
// provided and the amount of chunks
pub fn handle_chunks_downloaded(
	view: &mut View,
	session_index: SessionIndex,
	downloads: HashMap<ValidatorIndex, u64>,
) {
	let av_chunks = view
		.availability_chunks
		.entry(session_index)
		.or_insert(AvailabilityChunks::new());

	for (validator_index, download_count) in downloads {
		let authority_id = view.per_session
			.get(&session_index)
			.and_then(|session_view| session_view.authorities_ids.get(validator_index.0 as usize));

		match authority_id {
			Some(authority_id) => {
				av_chunks.note_candidate_chunk_downloaded(authority_id.clone(), download_count);
			}
			None => {
				gum::debug!(
					target: LOG_TARGET,
					validator_index = ?validator_index,
					download_count = download_count,
					session_idx = ?session_index,
					"could not find validator authority id"
				);
			}
		};
	}
}

// handle_chunk_uploaded receive the authority ids of the peer
// we just uploaded chunks to
pub fn handle_chunk_uploaded(
	view: &mut View,
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
	if let Some(highest_session) = view.per_session.keys().max() {
		let av_chunks = view
			.availability_chunks
			.entry(*highest_session)
			.and_modify(|av| av.note_candidate_chunk_uploaded(auth_id.clone(), 1))
			.or_insert(AvailabilityChunks::new_with_upload(auth_id, 1));
	}
}
