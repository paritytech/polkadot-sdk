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

//! Mocked components for tests.

use crate::{
	peer_store::{PeerStoreProvider, ProtocolHandle},
	ReputationChange,
};

use sc_network_common::role::ObservedRole;
use sc_network_types::PeerId;

use std::{collections::HashSet, sync::Arc};

/// No-op `PeerStore`.
#[derive(Debug)]
pub struct MockPeerStore {}

impl PeerStoreProvider for MockPeerStore {
	fn is_banned(&self, _peer_id: &PeerId) -> bool {
		// Make sure that the peer is not banned.
		false
	}

	fn register_protocol(&self, _protocol_handle: Arc<dyn ProtocolHandle>) {
		// Make sure not to fail.
	}

	fn report_disconnect(&self, _peer_id: PeerId) {
		// Make sure not to fail.
	}

	fn report_peer(&self, _peer_id: PeerId, _change: ReputationChange) {
		// Make sure not to fail.
	}

	fn peer_reputation(&self, _peer_id: &PeerId) -> i32 {
		// Make sure that the peer is not banned.
		0
	}

	fn peer_role(&self, _peer_id: &PeerId) -> Option<ObservedRole> {
		None
	}

	fn set_peer_role(&self, _peer_id: &PeerId, _role: ObservedRole) {
		unimplemented!();
	}

	fn outgoing_candidates(&self, _count: usize, _ignored: HashSet<PeerId>) -> Vec<PeerId> {
		unimplemented!()
	}

	fn num_known_peers(&self) -> usize {
		0usize
	}

	fn add_known_peer(&self, _peer_id: PeerId) {
		unimplemented!()
	}
}
