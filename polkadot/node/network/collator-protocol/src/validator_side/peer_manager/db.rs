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

use crate::validator_side::common::{ReputationUpdate, Score};
use polkadot_node_network_protocol::PeerId;
use polkadot_primitives::{Hash, Id as ParaId};

#[derive(Default)]
pub struct ReputationDb {}

impl ReputationDb {
	pub fn query(&self, peer_id: &PeerId, para_id: &ParaId) -> Option<Score> {
		None
	}
	pub fn modify_reputation(&self, update: &ReputationUpdate) {}
}
