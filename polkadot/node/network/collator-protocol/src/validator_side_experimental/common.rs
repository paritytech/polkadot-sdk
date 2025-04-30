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

use std::num::NonZeroU16;

use polkadot_node_network_protocol::peer_set::CollationVersion;
use polkadot_primitives::Id as ParaId;

/// Maximum reputation score.
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

/// Reputation bump for getting a valid candidate included.
pub const VALID_INCLUDED_CANDIDATE_BUMP: u16 = 50;

/// Reputation slash for peer inactivity (for each included candidate of the para that was not
/// authored by the peer)
pub const INACTIVITY_DECAY: u16 = 1;

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
