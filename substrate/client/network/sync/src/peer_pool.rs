// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! [`PeerPool`] manages the peers available for requests by syncing strategies.

use crate::LOG_TARGET;
use libp2p::PeerId;
use log::warn;
use std::collections::HashMap;

#[derive(Debug)]
enum PeerStatus {
	Available,
	Reserved,
}

impl PeerStatus {
	fn is_available(&self) -> bool {
		matches!(self, PeerStatus::Available)
	}
}

#[derive(Default, Debug)]
pub struct PeerPool {
	peers: HashMap<PeerId, PeerStatus>,
}

pub struct AvailablePeer<'a> {
	peer_id: &'a PeerId,
	status: &'a mut PeerStatus,
}

impl<'a> AvailablePeer<'a> {
	pub fn peer_id(&self) -> &'a PeerId {
		self.peer_id
	}

	pub fn reserve(&mut self) {
		*self.status = PeerStatus::Reserved;
	}
}

impl PeerPool {
	pub fn add_peer(&mut self, peer_id: PeerId) {
		self.peers.insert(peer_id, PeerStatus::Available);
	}

	pub fn remove_peer(&mut self, peer_id: &PeerId) {
		self.peers.remove(peer_id);
	}

	pub fn available_peers<'a>(&'a mut self) -> impl Iterator<Item = AvailablePeer> + 'a {
		self.peers.iter_mut().filter_map(|(peer_id, status)| {
			status.is_available().then_some(AvailablePeer::<'a> { peer_id, status })
		})
	}

	pub fn try_reserve_peer(&mut self, peer_id: &PeerId) -> bool {
		match self.peers.get_mut(peer_id) {
			Some(peer_status) => match peer_status {
				PeerStatus::Available => {
					*peer_status = PeerStatus::Reserved;
					true
				},
				PeerStatus::Reserved => false,
			},
			None => {
				warn!(target: LOG_TARGET, "Trying to reserve unknown peer {peer_id}.");
				false
			},
		}
	}

	pub fn free_peer(&mut self, peer_id: &PeerId) {
		match self.peers.get_mut(peer_id) {
			Some(peer_status) => match peer_status {
				PeerStatus::Available => {
					warn!(target: LOG_TARGET, "Trying to free available peer {peer_id}.")
				},
				PeerStatus::Reserved => {
					*peer_status = PeerStatus::Available;
				},
			},
			None => {
				warn!(target: LOG_TARGET, "Trying to free unknown peer {peer_id}.");
			},
		}
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn adding_peer() {
		let mut peer_pool = PeerPool::default();
		assert_eq!(peer_pool.available_peers().count(), 0);

		// Add peer.
		let peer_id = PeerId::random();
		peer_pool.add_peer(peer_id);

		// Peer is available.
		assert_eq!(peer_pool.available_peers().count(), 1);
		assert!(peer_pool.available_peers().any(|p| *p.peer_id() == peer_id));
	}

	#[test]
	fn removing_peer() {
		let mut peer_pool = PeerPool::default();
		assert_eq!(peer_pool.available_peers().count(), 0);

		// Add peer.
		let peer_id = PeerId::random();
		peer_pool.add_peer(peer_id);
		assert_eq!(peer_pool.available_peers().count(), 1);
		assert!(peer_pool.available_peers().any(|p| *p.peer_id() == peer_id));

		// Remove peer.
		peer_pool.remove_peer(&peer_id);
		assert_eq!(peer_pool.available_peers().count(), 0);
	}

	#[test]
	fn reserving_peer_via_available_peers() {
		let mut peer_pool = PeerPool::default();
		assert_eq!(peer_pool.available_peers().count(), 0);

		let peer_id = PeerId::random();
		peer_pool.add_peer(peer_id);
		assert_eq!(peer_pool.available_peers().count(), 1);
		assert!(peer_pool.available_peers().any(|p| *p.peer_id() == peer_id));

		// Reserve via `available_peers()`.
		peer_pool.available_peers().for_each(|mut available_peer| {
			assert_eq!(*available_peer.peer_id(), peer_id);
			available_peer.reserve();
		});

		// Peer is reserved.
		assert_eq!(peer_pool.available_peers().count(), 0);
		assert!(!peer_pool.try_reserve_peer(&peer_id));
	}

	#[test]
	fn reserving_peer_via_try_reserve() {
		let mut peer_pool = PeerPool::default();
		assert_eq!(peer_pool.available_peers().count(), 0);

		let peer_id = PeerId::random();
		peer_pool.add_peer(peer_id);
		assert_eq!(peer_pool.available_peers().count(), 1);
		assert!(peer_pool.available_peers().any(|p| *p.peer_id() == peer_id));

		// Reserve via `try_reserve_peer()`.
		assert!(peer_pool.try_reserve_peer(&peer_id));

		// Peer is reserved.
		assert_eq!(peer_pool.available_peers().count(), 0);
		assert!(!peer_pool.try_reserve_peer(&peer_id));
	}

	#[test]
	fn freeing_peer() {
		let mut peer_pool = PeerPool::default();
		assert_eq!(peer_pool.available_peers().count(), 0);

		let peer_id = PeerId::random();
		peer_pool.add_peer(peer_id);
		assert_eq!(peer_pool.available_peers().count(), 1);
		assert!(peer_pool.available_peers().any(|p| *p.peer_id() == peer_id));

		// Reserve via `try_reserve_peer()`.
		assert!(peer_pool.try_reserve_peer(&peer_id));
		assert_eq!(peer_pool.available_peers().count(), 0);
		assert!(!peer_pool.try_reserve_peer(&peer_id));

		// Free peer.
		peer_pool.free_peer(&peer_id);

		// Peer is available.
		assert_eq!(peer_pool.available_peers().count(), 1);
		assert!(peer_pool.available_peers().any(|p| *p.peer_id() == peer_id));

		// And can be reserved again.
		assert!(peer_pool.try_reserve_peer(&peer_id));
	}

	#[test]
	fn reserving_unknown_peer_fails() {
		let mut peer_pool = PeerPool::default();
		assert_eq!(peer_pool.available_peers().count(), 0);

		let peer_id = PeerId::random();
		assert!(!peer_pool.try_reserve_peer(&peer_id));
	}
}
