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
use futures::channel::oneshot;
use polkadot_node_network_protocol::{
	request_response::{outgoing::RequestError, v2 as request_v2},
	PeerId,
};
use polkadot_node_primitives::PoV;
use polkadot_node_subsystem::{messages::ChainApiMessage, CollatorProtocolSenderTrait};
use polkadot_node_subsystem_util::{
	request_candidate_events, request_candidates_pending_availability,
};
use polkadot_primitives::{
	vstaging::{CandidateEvent, CandidateReceiptV2 as CandidateReceipt},
	CandidateHash, Hash, HeadData, Id as ParaId,
};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const CONNECTED_PEERS_LIMIT: u8 = 255;

pub const CONNECTED_PER_PARA_LIMIT: u8 = CONNECTED_PEERS_LIMIT / 3;

pub const VALID_INCLUDED_CANDIDATE_BUMP: u8 = 10;

pub const INACTIVITY_SLASH: u8 = 1;

pub const INSTANT_FETCH_REP_THRESHOLD: Score = 20;

// In millis
pub const UNDER_THRESHOLD_FETCH_DELAY: u128 = 1000;

pub const INVALID_COLLATION_SLASH: Score = 100;

pub const MAX_REPUTATION: Score = 255;

pub const FAILED_FETCH_SLASH: Score = 10;

pub type Score = u8;

pub type DisconnectedPeers = Vec<PeerId>;

pub enum PeerState {
	/// Connected.
	Connected,
	/// Peer has declared.
	Collating(ParaId),
}

impl PeerState {
	pub fn para_id(&self) -> Option<ParaId> {
		match self {
			Self::Collating(para_id) => Some(*para_id),
			_ => None,
		}
	}
}

/// Candidate supplied with a para head it's built on top of.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub struct ProspectiveCandidate {
	/// Candidate hash.
	pub candidate_hash: CandidateHash,
	/// Parent head-data hash as supplied in advertisement.
	pub parent_head_data_hash: Hash,
}

/// Identifier of a collation being requested.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub struct Advertisement {
	/// Candidate's relay parent.
	pub relay_parent: Hash,
	/// Parachain id.
	pub para_id: ParaId,
	/// Peer that advertised this collation.
	pub peer_id: PeerId,
	/// Optional candidate hash and parent head-data hash if were
	/// supplied in advertisement.
	/// TODO: this needs to be optional (for collator protocol V1)
	pub prospective_candidate: ProspectiveCandidate,
}

/// Fetched collation data.
#[derive(Debug, Clone)]
pub struct FetchedCollation {
	/// Candidate receipt.
	pub candidate_receipt: CandidateReceipt,
	/// Proof of validity. Wrap it in an Arc to avoid expensive copying
	pub pov: PoV,
	/// Optional parachain parent head data.
	/// Only needed for elastic scaling.
	pub maybe_parent_head_data: Option<HeadData>,
	pub parent_head_data_hash: Hash,
}

pub enum CollationFetchOutcome {
	TryNew(Score),
	Success(FetchedCollation),
}

pub type CollationFetchResponse = (
	Advertisement,
	std::result::Result<request_v2::CollationFetchingResponse, CollationFetchError>,
);

pub enum DeclarationOutcome {
	Disconnected,
	Switched(ParaId),
	Accepted,
}

// Any error that can occur when awaiting a collation fetch response.
#[derive(Debug, thiserror::Error)]
pub enum CollationFetchError {
	#[error("Future was cancelled.")]
	Cancelled,
	#[error("{0}")]
	Request(#[from] RequestError),
}

pub async fn extract_reputation_updates_from_new_leaves<Sender: CollatorProtocolSenderTrait>(
	sender: &mut Sender,
	leaves: &[Hash],
) -> BTreeMap<ParaId, HashMap<PeerId, Score>> {
	// TODO: this could be much easier if we added a new CandidateEvent variant that includes the
	// info we need (the approved peer)
	let mut included_candidates_per_rp: HashMap<Hash, HashMap<ParaId, HashSet<CandidateHash>>> =
		HashMap::new();

	for leaf in leaves {
		let candidate_events =
			request_candidate_events(*leaf, sender).await.await.unwrap().unwrap();

		for event in candidate_events {
			if let CandidateEvent::CandidateIncluded(receipt, _, _, _) = event {
				included_candidates_per_rp
					.entry(*leaf)
					.or_default()
					.entry(receipt.descriptor.para_id())
					.or_default()
					.insert(receipt.hash());
			}
		}
	}

	let mut updates: BTreeMap<ParaId, HashMap<PeerId, Score>> = BTreeMap::new();
	for (rp, per_para) in included_candidates_per_rp {
		let parent = get_parent(sender, rp).await;

		for (para_id, included_candidates) in per_para {
			let candidates_pending_availability =
				request_candidates_pending_availability(parent, para_id, sender)
					.await
					.await
					.unwrap()
					.unwrap();

			for candidate in candidates_pending_availability {
				if included_candidates.contains(&candidate.hash()) {
					if let Some(approved_peer) = candidate.commitments.approved_peer() {
						if let Ok(peer_id) = PeerId::from_bytes(&approved_peer.0) {
							*(updates.entry(para_id).or_default().entry(peer_id).or_default()) +=
								VALID_INCLUDED_CANDIDATE_BUMP;
						}
					}
				}
			}
		}
	}

	updates
}

async fn get_parent<Sender: CollatorProtocolSenderTrait>(sender: &mut Sender, hash: Hash) -> Hash {
	// TODO: we could use the implicit view for this info.
	let (tx, rx) = oneshot::channel();
	sender.send_message(ChainApiMessage::BlockHeader(hash, tx)).await;

	rx.await.unwrap().unwrap().unwrap().parent_hash
}
