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

/// Reputation score type.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Default)]
pub struct Score(u16);

impl Score {
	pub fn new(val: u16) -> Option<Self> {
		if val > MAX_SCORE {
			None
		} else {
			Some(Self(val))
		}
	}

	pub fn saturating_add(&mut self, val: u16) {
		if (MAX_SCORE - self.0) >= val {
			self.0 += val;
		} else {
			self.0 = MAX_SCORE;
		}
	}

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
	pub version: CollationVersion,
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
