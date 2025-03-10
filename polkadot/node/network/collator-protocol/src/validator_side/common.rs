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
use polkadot_node_network_protocol::{
	request_response::{outgoing::RequestError, v2 as request_v2},
	PeerId,
};
use polkadot_node_primitives::PoV;
use polkadot_primitives::{
	vstaging::CandidateReceiptV2 as CandidateReceipt, CandidateHash, Hash, HeadData, Id as ParaId,
};

pub const CONNECTED_PEERS_LIMIT: u8 = 255;

pub const VALID_INCLUDED_CANDIDATE_BUMP: u8 = 10;

pub const INACTIVITY_SLASH: u8 = 1;

pub const INSTANT_FETCH_REP_THRESHOLD: Score = 0;

pub const INVALID_COLLATION_SLASH: Score = 100;

pub const MAX_REPUTATION: Score = 255;

pub const FAILED_FETCH_SLASH: Score = 10;

pub type Score = u8;

pub type DisconnectedPeers = Vec<PeerId>;

pub struct ReputationUpdate {
	pub peer_id: PeerId,
	pub para_id: ParaId,
	pub value: Score,
	pub kind: ReputationUpdateKind,
}

pub enum ReputationUpdateKind {
	Bump,
	Slash,
}

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
