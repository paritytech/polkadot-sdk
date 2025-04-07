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

use crate::validator_side_experimental::{common::Score, peer_manager::ReputationUpdate};
use async_trait::async_trait;
use polkadot_node_network_protocol::PeerId;
use polkadot_primitives::{BlockNumber, Hash, Id as ParaId};
use std::collections::{BTreeMap, HashMap};

/// Trait describing the interface of the reputation database.
#[async_trait]
pub trait Backend {
	/// Instantiate a new backend.
	async fn new() -> Self;
	/// Return the latest known leaf for which the backend processed bumps.
	async fn highest_known_leaf() -> Option<(Hash, BlockNumber)>;
	/// Get the peer's stored reputation for this paraid, if any.
	async fn query(&self, peer_id: &PeerId, para_id: &ParaId) -> Option<Score>;
	/// Slash the peer's reputation for this paraid, with the given value.
	async fn slash(&mut self, peer_id: &PeerId, para_id: &ParaId, value: Score);
	/// Remove all stored data for this paraid. Useful when a para gets deregistered.
	async fn remove_para(&mut self, para_id: &ParaId);
	/// Process the reputation bumps, returning all the reputation changes that were done in
	/// consequence. This is needed because a reputation bump for a para also means a reputation
	/// decay for the other collators of that para.
	async fn process_bumps(
		&mut self,
		bumps: BTreeMap<ParaId, HashMap<PeerId, Score>>,
	) -> Vec<ReputationUpdate>;
}
