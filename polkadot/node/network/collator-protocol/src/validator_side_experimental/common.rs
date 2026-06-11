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

use codec::{Decode, Encode};
use polkadot_node_network_protocol::{
	peer_set::CollationVersion,
	request_response::{outgoing::RequestError, v2 as request_v2},
	PeerId,
};
use polkadot_node_primitives::PoV;
use polkadot_primitives::{
	CandidateDescriptorVersion, CandidateHash, CandidateReceiptV2 as CandidateReceipt, Hash,
	Id as ParaId, PersistedValidationData,
};
use std::{collections::HashSet, num::NonZeroU16, time::Duration};

/// The following parameters (`MAX_SCORE`, `VALID_INCLUDED_CANDIDATE_BUMP` and `INACTIVITY_DECAY`)
/// determine how fast collators build reputation, how fast they lose it due to inactivity and how
/// many collators we can have per parachain.
///
/// The combination of `VALID_INCLUDED_CANDIDATE_BUMP` and `INACTIVITY_DECAY` determines the maximum
/// number of collators for the parachain. We assume that each collator has got an equal chance in
/// having its collation included (and therefore getting score for it). This means that for a set of
/// N collators, for N blocks each collator will gain `VALID_INCLUDED_CANDIDATE_BUMP` points and
/// lose (N-1)*INACTIVITY_DECAY points. With the values below (VALID_INCLUDED_CANDIDATE_BUMP=100
/// and INACTIVITY_DECAY=1) with N=100 collators no longer gain any score.
///
/// Time to reach max score (in hours) =
/// 	VALID_INCLUDED_CANDIDATE_BUMP / MAX_SCORE * NUM_COLLATORS * BLOCK_TIME / 60 / 60
///
/// REASONING:
/// - We want high MAX_SCORE and low VALID_INCLUDED_CANDIDATE_BUMP so that it takes a lot of time
///   (weeks) for a collator to build up reputation. Setting low value for MAX_SCORE will allow a
///   malicious collator to reach top score fast and if it try to commit an offence the rest of the
///   collators won't have an advantage against him. Keeping high MAX_SCORE will require far more
///   effort to reach this level.
/// - INACTIVITY_DECAY is 0 because its not fair to punish collator for not claiming a slot they are
///   not supposed to claim. We can reconsider this value in the future if there is a mechanism to
///   know the number of collators in the parachain and actually know when a collator skips its
///   slot.
/// - FAILED_FETCH_SLASH equals MAX_SCORE because this is a serious offence and we want a harsh
///   punishment for it. A collator builds up reputation slowly but loses it fast if it act
///   maliciously. Setting it to anything less will give unneeded advantage to a malicious collator.
///
/// Maximum reputation score. Scores higher than this will be saturated to this value.
pub const MAX_SCORE: u16 = u16::MAX;
/// Reputation bump for getting a valid candidate included in a finalized block.
pub const VALID_INCLUDED_CANDIDATE_BUMP: u16 = 1;
/// Reputation slash for peer inactivity (for each included candidate of the para that was not
/// authored by the peer)
pub const INACTIVITY_DECAY: u16 = 0;

/// For the values above (MAX_SCORE=u16::MAX, VALID_INCLUDED_CANDIDATE_BUMP=1, INACTIVITY_DECAY=0)
/// we have the following times to reach max score:
/// - 1 collator = ~218 hrs
/// - 2 collators = ~436 hrs
/// - 3 collators = ~655 hrs
/// - 5 collators = ~1092 hrs
/// - 10 collators = ~2184 hrs
///
/// Which means `MAX_SCORE` and `VALID_INCLUDED_CANDIDATE_BUMP` combination guarantees reputation
/// will build up slowly.

/// The next two parameters determine the punishments for misbehaviours. `FAILED_FETCH_SLASH`
/// indicates malicious behavior and the consequences are severe.

/// Slashing value for a failed fetch which might or might not be a malicious act. We can't make
/// this punishment too severe because it can affect legitimate collators experiencing temporary
/// networking issues too hard.
pub const FAILED_FETCH_SLASH: Score = Score::new(MAX_SCORE / 6);

/// Slashing value for an invalid collation (half of the max).
pub const INVALID_COLLATION_SLASH: Score = Score::new(MAX_SCORE / 2);

/// Minimum reputation threshold that warrants an instant fetch.
pub const INSTANT_FETCH_REP_THRESHOLD: Score = Score::new(VALID_INCLUDED_CANDIDATE_BUMP);

/// Limit for the total number connected peers.
pub const CONNECTED_PEERS_LIMIT: NonZeroU16 = NonZeroU16::new(300).expect("300 is greater than 0");

/// Limit for the total number of connected peers for a paraid.
/// Must be smaller than `CONNECTED_PEERS_LIMIT`.
/// The value is optimal for lookahead=5. In that case we can have no more than 5 parachains in the
/// claim queue => 60 is a good value for per para limit.
pub const CONNECTED_PEERS_PARA_LIMIT: NonZeroU16 = const {
	assert!(CONNECTED_PEERS_LIMIT.get() == 300);
	NonZeroU16::new(60).expect("60 is greater than 0")
};

/// Maximum number of relay parents to process for reputation bumps on startup and between finality
/// notifications.
pub const MAX_STARTUP_ANCESTRY_LOOKBACK: u32 = 20;

/// Maximum number of stored peer scores for a paraid. Should be greater than
/// `CONNECTED_PEERS_PARA_LIMIT`.
pub const MAX_STORED_SCORES_PER_PARA: u16 = 1000;

/// The maximum acceptable delay which can be applied on an advertisement from a collator with score
/// less than the maximum score for the parachain.
pub const MAX_FETCH_DELAY: Duration = Duration::from_millis(300);

/// The minimum interval after which we may want to stop the main loop in order to fetch available
/// advertised collations.
pub const MIN_FETCH_TIMER_DELAY: Duration = Duration::from_millis(150);

/// Reputation score type.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Default, Encode, Decode)]
pub struct Score(u16);

impl Score {
	/// Create a new instance. Fail if over the `MAX_SCORE`.
	pub const fn new(val: u16) -> Self {
		Self(val)
	}

	/// Add `val` to the inner value, saturating at `MAX_SCORE`.
	pub fn saturating_add(&mut self, val: u16) {
		let sum = self.0.saturating_add(val);
		self.0 = std::cmp::min(sum, MAX_SCORE);
	}

	/// Subtract `val` from the inner value, saturating at 0.
	pub fn saturating_sub(&mut self, val: u16) {
		self.0 = self.0.saturating_sub(val);
	}

	/// Returns the ration (self / rhs) between the current score and rhs.
	pub fn ratio(&self, rhs: &Self) -> f32 {
		self.0 as f32 / rhs.0 as f32
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
#[derive(Debug, Copy, Clone, PartialOrd, Ord, Eq, Hash, PartialEq)]
pub struct Advertisement {
	/// Candidate's scheduling parent.
	pub scheduling_parent: Hash,
	/// Parachain id.
	pub para_id: ParaId,
	/// Peer that advertised this collation.
	pub peer_id: PeerId,
	/// Optional candidate hash and parent head-data hash if were
	/// supplied in advertisement.
	pub prospective_candidate: Option<ProspectiveCandidate>,
	/// Advertised candidate descriptor version (for V3 protocol).
	/// None for V1/V2 protocols.
	pub advertised_descriptor_version: Option<CandidateDescriptorVersion>,
}

impl Advertisement {
	pub fn candidate_hash(&self) -> Option<CandidateHash> {
		self.prospective_candidate.map(|candidate| candidate.candidate_hash)
	}

	pub fn parent_head_data_hash(&self) -> Option<Hash> {
		self.prospective_candidate.map(|candidate| candidate.parent_head_data_hash)
	}

	pub fn fetch_key(&self) -> FetchKey {
		match self.prospective_candidate {
			Some(prospective_candidate) => {
				FetchKey::Candidate(prospective_candidate.candidate_hash)
			},
			None => FetchKey::V1(self.scheduling_parent, self.para_id),
		}
	}
}

/// In flight fetch identity. V1 has no candidate hash, its identity is the (SP, para)
/// pair of which at most one fetch may exist
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum FetchKey {
	Candidate(CandidateHash),
	V1(Hash, ParaId),
}

/// The identity of the candidate being fetched from the collator
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct FetchTarget {
	pub peer_id: PeerId,
	pub para_id: ParaId,
	pub scheduling_parent: Hash,
	/// None for V1
	pub candidate_hash: Option<CandidateHash>,
	pub parent_head_data_hash: Option<Hash>,
	pub descriptor_version: Option<CandidateDescriptorVersion>,
	pub output_head_data_hash: Option<Hash>,
	pub relay_parent: Option<Hash>,
}

impl FetchTarget {
	pub fn to_advertisement(&self) -> Advertisement {
		let prospective_candidate = self.candidate_hash.zip(self.parent_head_data_hash).map(
			|(candidate_hash, parent_head_data_hash)| ProspectiveCandidate {
				candidate_hash,
				parent_head_data_hash,
			},
		);
		Advertisement {
			scheduling_parent: self.scheduling_parent,
			para_id: self.para_id,
			peer_id: self.peer_id,
			prospective_candidate,
			advertised_descriptor_version: self.descriptor_version,
		}
	}

	pub fn fetch_key(&self) -> FetchKey {
		match self.candidate_hash {
			Some(candidate_hash) => FetchKey::Candidate(candidate_hash),
			None => FetchKey::V1(self.scheduling_parent, self.para_id),
		}
	}

	pub fn from_advertisement(advertisement: &Advertisement) -> FetchTarget {
		FetchTarget {
			peer_id: advertisement.peer_id,
			para_id: advertisement.para_id,
			scheduling_parent: advertisement.scheduling_parent,
			candidate_hash: advertisement.candidate_hash(),
			parent_head_data_hash: advertisement.parent_head_data_hash(),
			descriptor_version: advertisement.advertised_descriptor_version,
			output_head_data_hash: None,
			relay_parent: None,
		}
	}
}

/// Output of a `CollationFetchRequest`, which includes the advertisement identifier.
pub type CollationFetchResponse =
	(FetchTarget, std::result::Result<request_v2::CollationFetchingResponse, CollationFetchError>);

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
#[derive(Debug)]
pub struct SecondingRejectionInfo {
	pub scheduling_parent: Hash,
	pub peer_id: PeerId,
	pub para_id: ParaId,
	pub maybe_output_head_hash: Option<Hash>,
	pub maybe_candidate_hash: Option<CandidateHash>,
}

impl From<&FetchTarget> for SecondingRejectionInfo {
	fn from(fetch_target: &FetchTarget) -> Self {
		SecondingRejectionInfo {
			scheduling_parent: fetch_target.scheduling_parent,
			peer_id: fetch_target.peer_id,
			para_id: fetch_target.para_id,
			maybe_output_head_hash: None,
			maybe_candidate_hash: fetch_target.candidate_hash,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	// Test that the `Score` functions are working correctly.
	#[test]
	fn score_functions() {
		assert!(MAX_SCORE > 50);

		// Test saturating arithmetic functions.
		let score = Score::new(50);

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

	#[test]
	fn fetch_key_mapping() {
		let sp = Hash::repeat_byte(1);
		let para = ParaId::from(100);
		let hash = CandidateHash(Hash::repeat_byte(2));

		let mut adv = Advertisement {
			scheduling_parent: sp,
			para_id: para,
			peer_id: PeerId::random(),
			prospective_candidate: None,
			advertised_descriptor_version: None,
		};
		assert_eq!(adv.fetch_key(), FetchKey::V1(sp, para));

		adv.prospective_candidate =
			Some(ProspectiveCandidate { candidate_hash: hash, parent_head_data_hash: sp });
		assert_eq!(adv.fetch_key(), FetchKey::Candidate(hash));

		// The target derived from an advertisement keys identically.
		assert_eq!(FetchTarget::from_advertisement(&adv).fetch_key(), adv.fetch_key());
	}

	#[test]
	fn fetch_target_advertisement_round_trip() {
		let v1 = Advertisement {
			scheduling_parent: Hash::repeat_byte(1),
			para_id: ParaId::from(100),
			peer_id: PeerId::random(),
			prospective_candidate: None,
			advertised_descriptor_version: None,
		};
		assert_eq!(FetchTarget::from_advertisement(&v1).to_advertisement(), v1);

		let v3 = Advertisement {
			prospective_candidate: Some(ProspectiveCandidate {
				candidate_hash: CandidateHash(Hash::repeat_byte(2)),
				parent_head_data_hash: Hash::repeat_byte(3),
			}),
			advertised_descriptor_version: Some(CandidateDescriptorVersion::V3),
			..v1
		};
		assert_eq!(FetchTarget::from_advertisement(&v3).to_advertisement(), v3);
	}
}
