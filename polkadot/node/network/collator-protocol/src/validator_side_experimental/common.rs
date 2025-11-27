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

use std::{collections::HashSet, num::NonZeroU16, time::Duration};

use polkadot_node_network_protocol::{
	peer_set::CollationVersion,
	request_response::{outgoing::RequestError, v2 as request_v2},
	PeerId,
};
use polkadot_node_primitives::PoV;
use polkadot_primitives::{
	CandidateHash, CandidateReceiptV2 as CandidateReceipt, Hash, Id as ParaId,
	PersistedValidationData,
};

/// Maximum reputation score. Scores higher than this will be saturated to this value.
pub const MAX_SCORE: u16 = 5000;

/// Limit for the total number connected peers.
pub const CONNECTED_PEERS_LIMIT: NonZeroU16 = NonZeroU16::new(300).expect("300 is greater than 0");

/// Limit for the total number of connected peers for a paraid.
/// Must be smaller than `CONNECTED_PEERS_LIMIT`.
pub const CONNECTED_PEERS_PARA_LIMIT: NonZeroU16 = const {
	assert!(CONNECTED_PEERS_LIMIT.get() >= 100);
	NonZeroU16::new(100).expect("100 is greater than 0")
};

/// Maximum number of relay parents to process for reputation bumps on startup and between finality
/// notifications.
pub const MAX_STARTUP_ANCESTRY_LOOKBACK: u32 = 20;

/// Reputation bump for getting a valid candidate included in a finalized block.
pub const VALID_INCLUDED_CANDIDATE_BUMP: u16 = 50;

/// Reputation slash for peer inactivity (for each included candidate of the para that was not
/// authored by the peer)
pub const INACTIVITY_DECAY: u16 = 1;

/// Maximum number of stored peer scores for a paraid. Should be greater than
/// `CONNECTED_PEERS_PARA_LIMIT`.
pub const MAX_STORED_SCORES_PER_PARA: u8 = 150;

/// Slashing value for a failed fetch that we can be fairly sure does not happen by accident.
pub const FAILED_FETCH_SLASH: Score = Score::new(20).expect("20 is less than MAX_SCORE");

/// Slashing value for an invalid collation.
pub const INVALID_COLLATION_SLASH: Score = Score::new(1000).expect("1000 is less than MAX_SCORE");

/// Minimum reputation threshold that warrants an instant fetch.
pub const INSTANT_FETCH_REP_THRESHOLD: Score =
	Score::new(1000).expect("1000 is less than MAX_SCORE");

/// Delay for fetching collations when the reputation score is below the threshold
/// defined by `INSTANT_FETCH_REP_THRESHOLD`.
/// This gives us a chance to fetch collations from other peers with higher reputation
/// before we try to fetch from this peer.
pub const UNDER_THRESHOLD_FETCH_DELAY: Duration = Duration::from_millis(1000);

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
		if (self.0 + val) <= MAX_SCORE {
			self.0 += val;
		} else {
			self.0 = MAX_SCORE;
		}
	}

	/// Subtract `val` from the inner value, saturating at 0.
	pub fn saturating_sub(&mut self, val: u16) {
		self.0 = self.0.saturating_sub(val);
	}
}

impl From<Score> for u16 {
	fn from(value: Score) -> Self {
		value.0
	}
}

/// Information about a connected peer.
#[derive(PartialEq, Debug, Clone)]
pub struct PeerInfo {
	/// Protocol version.
	pub version: CollationVersion,
	/// State of the peer.
	pub state: PeerState,
}

/// State of a connected peer
#[derive(PartialEq, Debug, Clone)]
pub enum PeerState {
	/// Connected.
	Connected,
	/// Peer has declared.
	Collating(ParaId),
}

#[derive(Debug, PartialEq)]
/// Outcome of triaging a new connection.
pub enum TryAcceptOutcome {
	/// Connection was accepted.
	Added,
	/// Connection was accepted, but it replaced the slot(s) of some peers
	/// This can hold more than one `PeerId` because before receiving the `Declare` message,
	/// one peer can hold connection slots for multiple paraids.
	/// The set can also be empty if this peer replaced some other peer's slot but that other peer
	/// maintained a connection slot for another para (therefore not disconnected).
	/// The number of peers in the set is bound to the number of scheduled paras.
	Replaced(HashSet<PeerId>),
	/// Connection was rejected.
	Rejected,
}

impl TryAcceptOutcome {
	/// Combine two outcomes into one. If at least one of them allows the connection,
	/// the connection is allowed.
	pub fn combine(self, other: Self) -> Self {
		use TryAcceptOutcome::*;
		match (self, other) {
			(Added, Added) => Added,
			(Rejected, Rejected) => Rejected,
			(Added, Rejected) | (Rejected, Added) => Added,
			(Replaced(mut replaced_a), Replaced(replaced_b)) => {
				replaced_a.extend(replaced_b);
				Replaced(replaced_a)
			},
			(_, Replaced(replaced)) | (Replaced(replaced), _) => Replaced(replaced),
		}
	}
}

/// Candidate supplied with a para head it's built on top of.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub struct ProspectiveCandidate {
	/// Candidate hash.
	pub candidate_hash: CandidateHash,
	/// Parent head-data hash as supplied in advertisement.
	pub parent_head_data_hash: Hash,
}

/// Identifier of a collation being requested.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub struct Advertisement {
	/// Candidate's relay parent.
	pub relay_parent: Hash,
	/// Parachain id.
	pub para_id: ParaId,
	/// Peer that advertised this collation.
	pub peer_id: PeerId,
	/// Optional candidate hash and parent head-data hash if were
	/// supplied in advertisement.
	pub prospective_candidate: Option<ProspectiveCandidate>,
}

impl Advertisement {
	pub fn candidate_hash(&self) -> Option<CandidateHash> {
		self.prospective_candidate.map(|candidate| candidate.candidate_hash)
	}
}

/// Output of a `CollationFetchRequest`, which includes the advertisement identifier.
pub type CollationFetchResponse = (
	Advertisement,
	std::result::Result<request_v2::CollationFetchingResponse, CollationFetchError>,
);

/// Any error that can occur when awaiting a collation fetch response.
#[derive(Debug, thiserror::Error)]
pub enum CollationFetchError {
	#[error("Future was cancelled.")]
	Cancelled,
	#[error("{0}")]
	Request(#[from] RequestError),
}

/// Whether we can start seconding a fetched candidate or not.
pub enum CanSecond {
	/// Seconding is not possible. Returns an optional reputation slash, together with the rejected
	/// collation info.
	No(Option<Score>, SecondingRejectionInfo),
	/// Seconding can begin. Returns all the needed data for seconding.
	Yes(CandidateReceipt, PoV, PersistedValidationData),
	/// Seconding is blocked because we are waiting for the parent to be seconded.
	/// Returns the hash of the parent candidate header, together with the rejected collation info.
	BlockedOnParent(Hash, SecondingRejectionInfo),
}

/// Information that identifies a collation that was rejected from seconding.
pub struct SecondingRejectionInfo {
	pub relay_parent: Hash,
	pub peer_id: PeerId,
	pub para_id: ParaId,
	pub maybe_output_head_hash: Option<Hash>,
	pub maybe_candidate_hash: Option<CandidateHash>,
}

#[cfg(test)]
mod tests {
	use super::*;

	// Test that the `Score` functions are working correctly.
	#[test]
	fn score_functions() {
		assert!(MAX_SCORE > 50);

		// Test that the constructor returns None for values that exceed the limit.
		for score in (0..MAX_SCORE).step_by(10) {
			assert_eq!(u16::from(Score::new(score).unwrap()), score);
		}
		assert_eq!(u16::from(Score::new(MAX_SCORE).unwrap()), MAX_SCORE);
		for score in ((MAX_SCORE + 1)..(MAX_SCORE + 50)).step_by(5) {
			assert_eq!(Score::new(score), None);
		}

		// Test saturating arithmetic functions.
		let score = Score::new(50).unwrap();

		// Test addition with value that does not go over the limit.
		for other_score in (0..(MAX_SCORE - 50)).step_by(10) {
			let expected_value = u16::from(score) + other_score;

			let mut score = score;
			score.saturating_add(other_score);

			assert_eq!(expected_value, u16::from(score));
		}

		// Test overflowing addition.
		for other_score in ((MAX_SCORE - 50)..MAX_SCORE).step_by(10) {
			let mut score = score;
			score.saturating_add(other_score);

			assert_eq!(MAX_SCORE, u16::from(score));
		}

		// Test subtraction with value that does not go under zero.
		for other_score in (0..50).step_by(10) {
			let expected_value = u16::from(score) - other_score;

			let mut score = score;
			score.saturating_sub(other_score);

			assert_eq!(expected_value, u16::from(score));
		}

		// Test underflowing subtraction.
		for other_score in (50..100).step_by(10) {
			let mut score = score;
			score.saturating_sub(other_score);

			assert_eq!(0, u16::from(score));
		}
	}
}
