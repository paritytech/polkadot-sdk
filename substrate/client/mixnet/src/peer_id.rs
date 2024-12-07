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

use mixnet::core::PeerId as CorePeerId;
use sc_network_types::PeerId;

/// Convert a libp2p [`PeerId`] into a mixnet core [`PeerId`](CorePeerId).
///
/// This will succeed only if `peer_id` is an Ed25519 public key ("hashed" using the identity
/// hasher). Returns `None` on failure.
pub fn to_core_peer_id(peer_id: &PeerId) -> Option<CorePeerId> {
	peer_id.into_ed25519()
}

/// Convert a mixnet core [`PeerId`](CorePeerId) into a libp2p [`PeerId`].
///
/// This will succeed only if `peer_id` represents a point on the Ed25519 curve. Returns `None` on
/// failure.
pub fn from_core_peer_id(core_peer_id: &CorePeerId) -> Option<PeerId> {
	PeerId::from_ed25519(core_peer_id)
}
