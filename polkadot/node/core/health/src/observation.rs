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

//! What the producers report.

use polkadot_primitives::AuthorityDiscoveryId;
use sc_network_types::PeerId;
use std::collections::HashSet;

/// One measurement = one variant. Fields are the concrete values.
#[derive(Debug, Clone)]
pub enum Observation {
	/// The authorities we hold a validation peer set connection with, a snapshot gossip support
	/// reports at every active leaf. May contain authorities of past, current and next sessions.
	ConnectedValidators { authorities: HashSet<AuthorityDiscoveryId> },
	/// A peer opened a connection to us.
	InboundConnection { peer: PeerId },
}
