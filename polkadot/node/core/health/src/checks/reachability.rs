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

//! Inbound reachability: the addresses we are publishing on the DHT are reachable.
//!
//! Every connection a peer opens to us from a globally routable address is reported. At every
//! tick, the check passes when enough distinct peers connected since the previous tick. The
//! window is one tick so that a network change is detected fast.

use crate::LOG_TARGET;
use sc_network_types::PeerId;
use std::collections::BTreeSet;

const MIN_PEERS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
	/// Enough distinct peers connected to us.
	Reachable,
	/// Too few peers connected to us.
	Unreachable,
}

/// Checks if the addresses we are publishing on the DHT are reachable.
#[derive(Default)]
pub struct Reachability {
	/// The distinct peers that connected to us since the last tick, capped at `MIN_PEERS`.
	peers: BTreeSet<PeerId>,
	/// The result from the last check.
	outcome: Option<Outcome>,
}

impl Reachability {
	/// Note an inbound peer connection.
	pub fn on_inbound_connection(&mut self, peer: PeerId) {
		if self.peers.len() < MIN_PEERS {
			self.peers.insert(peer);
		}
	}

	/// Determine the check outcome for the window.
	pub fn on_tick(&mut self) {
		let outcome = self.judge();
		gum::trace!(target: LOG_TARGET, ?outcome, peers = self.peers.len(), "Reachability judged");
		self.peers.clear();

		if self.outcome.replace(outcome) == Some(outcome) {
			return;
		}

		// Log only on state change
		match outcome {
			Outcome::Reachable => {
				gum::debug!(target: LOG_TARGET, "Peers connect to us: the node is reachable")
			},
			Outcome::Unreachable => gum::warn!(
				target: LOG_TARGET,
				min_peers = MIN_PEERS,
				"Fewer than {MIN_PEERS} distinct peers connected to us since the last check: the \
				 node may be unreachable. Check the NAT port forwarding, the firewall and \
				 `--public-addr`",
			),
		}
	}

	fn judge(&self) -> Outcome {
		if self.peers.len() >= MIN_PEERS {
			Outcome::Reachable
		} else {
			Outcome::Unreachable
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn connected_by(check: &mut Reachability, peers: usize) {
		for _ in 0..peers {
			check.on_inbound_connection(PeerId::random());
		}
	}

	#[test]
	fn reachable_after_enough_distinct_peers() {
		let mut check = Reachability::default();
		connected_by(&mut check, MIN_PEERS);
		assert_eq!(check.judge(), Outcome::Reachable);
	}

	#[test]
	fn the_same_peer_counts_once() {
		let mut check = Reachability::default();
		let peer = PeerId::random();
		for _ in 0..MIN_PEERS {
			check.on_inbound_connection(peer);
		}
		assert_eq!(check.judge(), Outcome::Unreachable);
	}

	#[test]
	fn unreachable_below_the_threshold() {
		let mut check = Reachability::default();
		assert_eq!(check.judge(), Outcome::Unreachable);
		connected_by(&mut check, MIN_PEERS - 1);
		assert_eq!(check.judge(), Outcome::Unreachable);
	}

	#[test]
	fn the_window_resets_at_the_tick() {
		let mut check = Reachability::default();
		connected_by(&mut check, MIN_PEERS);
		check.on_tick();
		assert_eq!(check.judge(), Outcome::Unreachable);
	}
}
