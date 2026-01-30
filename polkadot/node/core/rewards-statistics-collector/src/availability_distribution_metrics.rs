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

use crate::{View, LOG_TARGET};
use polkadot_primitives::{AuthorityDiscoveryId, SessionIndex, ValidatorIndex};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailabilityChunks {
	pub downloads: BTreeMap<AuthorityDiscoveryId, u64>,
	pub uploads: BTreeMap<AuthorityDiscoveryId, u64>,
}

impl AvailabilityChunks {
	pub fn new() -> Self {
		Self { downloads: Default::default(), uploads: Default::default() }
	}

	pub fn new_with_upload(auth_id: AuthorityDiscoveryId, count: u64) -> Self {
		Self {
			downloads: Default::default(),
			uploads: vec![(auth_id, count)].into_iter().collect(),
		}
	}

	pub fn note_candidate_chunk_downloaded(
		&mut self,
		authority_id: AuthorityDiscoveryId,
		count: u64,
	) {
		self.downloads
			.entry(authority_id)
			.and_modify(|current| *current = current.saturating_add(count))
			.or_insert(count);
	}

	pub fn note_candidate_chunk_uploaded(
		&mut self,
		authority_id: AuthorityDiscoveryId,
		count: u64,
	) {
		self.uploads
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
		let authority_id = view
			.per_session
			.get(&session_index)
			.and_then(|session_view| session_view.authorities_ids.get(validator_index.0 as usize));

		match authority_id {
			Some(authority_id) => {
				av_chunks.note_candidate_chunk_downloaded(authority_id.clone(), download_count);
			},
			None => {
				gum::debug!(
					target: LOG_TARGET,
					validator_index = ?validator_index,
					download_count = download_count,
					session_idx = ?session_index,
					"could not find validator authority id"
				);
			},
		};
	}
}

// handle_chunk_uploaded receive the authority ids of the peer
// we just uploaded chunks to
pub fn handle_chunk_uploaded(
	view: &mut View,
	session_index: SessionIndex,
	authority_ids: HashSet<AuthorityDiscoveryId>,
) {
	if let Some(session_info) = view.per_session.get(&session_index) {
		let validator_authority_id =
			session_info.authorities_ids.iter().find(|auth| authority_ids.contains(auth));

		if let Some(auth_id) = validator_authority_id {
			view.availability_chunks
				.entry(session_index)
				.and_modify(|av| av.note_candidate_chunk_uploaded(auth_id.clone(), 1))
				.or_insert(AvailabilityChunks::new_with_upload(auth_id.clone(), 1));
		}
	}
}
