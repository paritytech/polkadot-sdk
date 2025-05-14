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

use crate::validator_side_experimental::{
	common::Score,
	peer_manager::{backend::Backend, ReputationUpdate},
};
use async_trait::async_trait;
use polkadot_node_network_protocol::PeerId;
use polkadot_primitives::{BlockNumber, Hash, Id as ParaId};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub struct Db;

// Dummy implementation for now
#[async_trait]
impl Backend for Db {
	async fn new() -> Self {
		Db
	}

	async fn processed_finalized_block_number(&self) -> Option<BlockNumber> {
		None
	}

	async fn query(&self, _peer_id: &PeerId, _para_id: &ParaId) -> Option<Score> {
		None
	}

	async fn slash(&mut self, _peer_id: &PeerId, _para_id: &ParaId, _value: Score) {}

	async fn prune_paras(&mut self, _registered_paras: BTreeSet<ParaId>) {}

	async fn process_bumps(
		&mut self,
		_leaf_number: BlockNumber,
		_bumps: BTreeMap<ParaId, HashMap<PeerId, Score>>,
		_decay_value: Option<Score>,
	) -> Vec<ReputationUpdate> {
		vec![]
	}
}
