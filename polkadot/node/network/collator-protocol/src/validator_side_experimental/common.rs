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

use polkadot_node_network_protocol::peer_set::CollationVersion;
use polkadot_primitives::Id as ParaId;

/// Maximum reputation score.
pub const MAX_SCORE: u16 = 1000;

/// Limit for the total number connected peers.
pub const CONNECTED_PEERS_LIMIT: u16 = 300;

/// Limit for the total number of connected peers for a paraid.
/// Must be smaller than `CONNECTED_PEERS_LIMIT`.
pub const CONNECTED_PEERS_PARA_LIMIT: u16 = 100;

/// Maximum number of relay parents to process for reputation bumps on startup.
pub const MAX_STARTUP_ANCESTRY_LOOKBACK: u32 = 20;

/// Reputation bump for getting a valid candidate included.
pub const VALID_INCLUDED_CANDIDATE_BUMP: u16 = 10;

/// Reputation slash for peer inactivity (for each included candidate of the para that was not
/// authored by the peer)
pub const INACTIVITY_DECAY: u16 = 1;

/// Maximum number of stored peer scores for a paraid. Should be greater than
/// `CONNECTED_PEERS_PARA_LIMIT`.
pub const MAX_STORED_SCORES_PER_PARA: u8 = 150;

/// Reputation score type.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Default)]
pub struct Score(u16);

impl Score {
	/// Create a new instance. Fail if over the `MAX_SCORE`.
	pub const fn new(val: u16) -> Option<Self> {
		if val > MAX_SCORE {
			None
		} else {
			Some(Self(val))
		}
	}

	/// Add `val` to the inner value, saturating at `MAX_SCORE`.
	pub fn saturating_add(&mut self, val: u16) {
		if (MAX_SCORE - self.0) >= val {
			self.0 += val;
		} else {
			self.0 = MAX_SCORE;
		}
	}

	/// Subtract `val` from the inner value, saturating at 0.
	pub fn saturating_sub(&mut self, val: u16) {
		if val >= self.0 {
			self.0 = 0;
		} else {
			self.0 -= val;
		}
	}
}

impl From<Score> for u16 {
	fn from(value: Score) -> Self {
		value.0
	}
}

/// Information about a connected peer.
pub struct PeerInfo {
	/// Protocol version.
	pub version: CollationVersion,
	/// State of the peer.
	pub state: PeerState,
}

impl PeerInfo {
	pub fn state(&self) -> &PeerState {
		&self.state
	}

	pub fn set_state(&mut self, new_state: PeerState) {
		self.state = new_state;
	}

	pub fn version(&self) -> CollationVersion {
		self.version
	}
}

/// State of a connected peer
pub enum PeerState {
	/// Connected.
	Connected,
	/// Peer has declared.
	Collating(ParaId),
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
