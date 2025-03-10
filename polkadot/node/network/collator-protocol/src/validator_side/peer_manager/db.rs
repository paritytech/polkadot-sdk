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

use crate::validator_side::common::{ReputationUpdate, ReputationUpdateKind, Score};
use polkadot_node_network_protocol::PeerId;
use polkadot_primitives::Id as ParaId;
use std::collections::{BTreeMap, HashMap};

// TODO: this needs to be a proper DB, but for prototyping purposes it's fine to keep it in memory.
#[derive(Default)]
pub struct ReputationDb(BTreeMap<ParaId, HashMap<PeerId, Score>>);

// TODO: we need a maximum capacity for per paraid storage that is ideally larger than the maximum
// number of connected peers for a para.
impl ReputationDb {
	pub fn query(&self, peer_id: &PeerId, para_id: &ParaId) -> Option<Score> {
		self.0.get(para_id).and_then(|per_para| per_para.get(peer_id).copied())
	}

	pub fn slash_reputation(&mut self, peer_id: &PeerId, para_id: &ParaId, value: Score) {
		self.0.get_mut(para_id).and_then(|per_para| {
			per_para.get_mut(peer_id).map(|score| score.saturating_sub(value))
		});
	}

	pub fn bump_reputations(
		&mut self,
		bumps: BTreeMap<ParaId, HashMap<PeerId, Score>>,
	) -> Vec<ReputationUpdate> {
		let mut reported_updates = vec![];

		for (para, bumps_per_para) in bumps {
			for (peer_id, bump) in bumps_per_para.iter() {
				self.0.get_mut(&para).and_then(|per_para| {
					per_para.get_mut(peer_id).map(|score| *score = score.saturating_add(*bump))
				});
				reported_updates.push(ReputationUpdate {
					peer_id: *peer_id,
					para_id: para,
					value: *bump,
					kind: ReputationUpdateKind::Bump,
				});
			}

			if let Some(per_para) = self.0.get_mut(&para) {
				for (peer_id, value) in per_para {
					if !bumps_per_para.contains_key(peer_id) {
						*value = value.saturating_sub(1);

						reported_updates.push(ReputationUpdate {
							peer_id: *peer_id,
							para_id: para,
							value: 1,
							kind: ReputationUpdateKind::Slash,
						});
					}
				}
			}
		}

		reported_updates
	}
}
