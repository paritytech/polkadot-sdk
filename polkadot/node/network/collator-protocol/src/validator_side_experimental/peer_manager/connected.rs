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
	common::{PeerInfo, PeerState, Score},
	peer_manager::{DeclarationOutcome, ReputationUpdate, ReputationUpdateKind, TryAcceptOutcome},
};
use polkadot_node_network_protocol::PeerId;
use polkadot_primitives::Id as ParaId;
use std::{
	cmp::Ordering,
	collections::{BTreeMap, BTreeSet, HashMap},
	future::Future,
};

/// Keeps track of connected peers, together with relevant info such as their procotol versions,
/// declared paraids and reputations.
pub struct ConnectedPeers {
	per_para: BTreeMap<ParaId, PerPara>,
	peer_info: HashMap<PeerId, PeerInfo>,
}

impl ConnectedPeers {
	/// Create a new ConnectedPeers object.
	pub fn new(scheduled_paras: BTreeSet<ParaId>, overall_limit: u16, per_para_limit: u16) -> Self {
		let limit = std::cmp::min(
			(overall_limit)
				.checked_div(
					scheduled_paras
						.len()
						.try_into()
						.expect("Nr of scheduled paras on a core should always fit in a u16"),
				)
				.unwrap_or(0),
			per_para_limit,
		);

		let mut per_para = BTreeMap::new();
		for para_id in scheduled_paras {
			per_para.insert(para_id, PerPara::new(limit));
		}

		Self { per_para, peer_info: Default::default() }
	}

	/// Update the reputation of a peer for a specific paraid. This needs to also be persisted to
	/// the reputation database, but changes are duplicated to this in-memory store of connected
	/// peers.
	pub fn update_reputation(&mut self, update: ReputationUpdate) {
		let Some(per_para) = self.per_para.get_mut(&update.para_id) else { return };
		if u16::from(update.value) == 0 {
			return
		}
		per_para.update_reputation(update);
	}

	/// Try accepting a new peer. The connection must have already been established, but here we
	/// decide whether to keep it or not. We don't know for which paraid the peer will collate.
	pub async fn try_accept<
		RepQueryFn: Fn(PeerId, ParaId) -> QueryFut,
		QueryFut: Future<Output = Score>,
	>(
		&mut self,
		reputation_query_fn: RepQueryFn,
		peer_id: PeerId,
		peer_info: PeerInfo,
	) -> TryAcceptOutcome {
		if self.contains(&peer_id) {
			return TryAcceptOutcome::Added
		}

		let mut outcome = TryAcceptOutcome::Rejected;

		match peer_info.state() {
			PeerState::Collating(para_id) => {
				let past_reputation = reputation_query_fn(peer_id, *para_id).await;
				if let Some(per_para) = self.per_para.get_mut(&para_id) {
					let res = per_para.try_accept(peer_id, past_reputation);
					outcome = outcome.combine(res);
				}
			},
			PeerState::Connected =>
				for (para_id, per_para) in self.per_para.iter_mut() {
					let past_reputation = reputation_query_fn(peer_id, *para_id).await;
					let res = per_para.try_accept(peer_id, past_reputation);
					outcome = outcome.combine(res);
				},
		}

		if !matches!(outcome, TryAcceptOutcome::Rejected) {
			self.peer_info.insert(peer_id, peer_info);
		}

		outcome
	}

	/// Remove the peer. Should be called when we see a peer being disconnected.
	pub fn remove(&mut self, peer: &PeerId) {
		for per_para in self.per_para.values_mut() {
			per_para.remove(peer);
		}

		self.peer_info.remove(peer);
	}

	/// Process a peer's declaration of intention to collate for this paraid.
	pub fn declared(&mut self, peer_id: PeerId, para_id: ParaId) -> DeclarationOutcome {
		let mut outcome = DeclarationOutcome::Rejected;

		let Some(peer_info) = self.peer_info.get_mut(&peer_id) else { return outcome };

		match peer_info.state() {
			PeerState::Connected => {
				for (para, per_para) in self.per_para.iter_mut() {
					if para == &para_id && per_para.contains(&peer_id) {
						outcome = DeclarationOutcome::Accepted;
					} else {
						// We remove the reserved slots from all other paras.
						per_para.remove(&peer_id);
					}
				}
			},
			PeerState::Collating(old_para_id) if old_para_id == &para_id => {
				// Redundant, already collating for this para.
				outcome = DeclarationOutcome::Accepted;
			},
			PeerState::Collating(old_para_id) => {
				if let Some(old_per_para) = self.per_para.get_mut(&old_para_id) {
					old_per_para.remove(&peer_id);
				}
				if let Some(per_para) = self.per_para.get(&para_id) {
					if per_para.contains(&peer_id) {
						outcome = DeclarationOutcome::Switched(*old_para_id);
					}
				}
			},
		}

		if !matches!(outcome, DeclarationOutcome::Rejected) {
			peer_info.set_state(PeerState::Collating(para_id));
		}

		outcome
	}

	/// Get a reference to the peer's info, if connected.
	pub fn peer_info(&self, peer_id: &PeerId) -> Option<&PeerInfo> {
		self.peer_info.get(&peer_id)
	}

	pub fn peer_score(&self, peer_id: &PeerId, para_id: &ParaId) -> Option<Score> {
		self.per_para.get(para_id).and_then(|per_para| per_para.get_score(peer_id))
	}

	/// Consume self and return the relevant information for building the next instance.
	pub fn consume(self) -> (HashMap<PeerId, PeerInfo>, BTreeMap<ParaId, PerPara>) {
		(self.peer_info, self.per_para)
	}

	/// Return an iterator over the scheduled paraids.
	pub fn scheduled_paras<'a>(&'a self) -> impl Iterator<Item = &'a ParaId> + 'a {
		self.per_para.keys()
	}

	fn contains(&self, peer_id: &PeerId) -> bool {
		self.peer_info.contains_key(peer_id)
	}
}

/// Per-para connected peers store. Acts as a handy in-memory cache of connected peer scores for a
/// specific paraid.
pub struct PerPara {
	// Don't accept more than this number of connected peers for this para.
	limit: u16,
	// A min-heap would be more efficient for getting the min (constant) but modifying the score
	// would be linear, so use a BST which achieves logarithmic performance for all ops.
	sorted_scores: BTreeSet<OrderedPeerScoreEntry>,
	// This is needed so that we can quickly access the ordered entry. Also has the nice benefit of
	// being an in-memory copy of the reputation DB for the connected peers.
	per_peer_score: HashMap<PeerId, Score>,
}

impl PerPara {
	/// Get the peer's score, if any.
	pub fn get_score(&self, peer_id: &PeerId) -> Option<Score> {
		self.per_peer_score.get(peer_id).map(|s| *s)
	}

	fn new(limit: u16) -> Self {
		Self { limit, sorted_scores: BTreeSet::default(), per_peer_score: HashMap::default() }
	}

	fn try_accept(&mut self, peer_id: PeerId, score: Score) -> TryAcceptOutcome {
		// If we've got enough room, add it. Otherwise, see if it has a higher reputation than any
		// other connected peer.
		if self.sorted_scores.len() < (self.limit as usize) {
			self.sorted_scores.insert(OrderedPeerScoreEntry { peer_id, score });
			TryAcceptOutcome::Added
		} else {
			let Some(min_score) = self.sorted_scores.first() else {
				// Cannot really happen since previous arm would have matched, unless the limit is
				// 0.
				return TryAcceptOutcome::Rejected
			};

			if min_score.score >= score {
				TryAcceptOutcome::Rejected
			} else {
				let Some(replaced) = self.sorted_scores.pop_first() else {
					// Cannot really happen since we already know there's some entry with a lower
					// score than ours.
					return TryAcceptOutcome::Rejected
				};
				self.per_peer_score.remove(&replaced.peer_id);

				self.sorted_scores.insert(OrderedPeerScoreEntry { peer_id, score });
				self.per_peer_score.insert(peer_id, score);
				TryAcceptOutcome::Replaced(vec![replaced.peer_id])
			}
		}
	}

	fn update_reputation(&mut self, update: ReputationUpdate) {
		let Some(score) = self.per_peer_score.get_mut(&update.peer_id) else {
			// If the peer is not connected we don't care to update anything besides the DB.
			return
		};

		self.sorted_scores
			.remove(&OrderedPeerScoreEntry { peer_id: update.peer_id, score: *score });

		match update.kind {
			ReputationUpdateKind::Bump => score.saturating_add(update.value.into()),
			ReputationUpdateKind::Slash => score.saturating_sub(update.value.into()),
		};

		self.sorted_scores
			.insert(OrderedPeerScoreEntry { peer_id: update.peer_id, score: *score });
	}

	fn remove(&mut self, peer_id: &PeerId) {
		let Some(score) = self.per_peer_score.remove(&peer_id) else { return };

		self.sorted_scores.remove(&OrderedPeerScoreEntry { peer_id: *peer_id, score });
	}

	fn contains(&self, peer_id: &PeerId) -> bool {
		self.per_peer_score.contains_key(peer_id)
	}
}

#[derive(PartialEq, Eq)]
struct OrderedPeerScoreEntry {
	peer_id: PeerId,
	score: Score,
}

impl Ord for OrderedPeerScoreEntry {
	fn cmp(&self, other: &Self) -> Ordering {
		self.score.cmp(&other.score)
	}
}

impl PartialOrd for OrderedPeerScoreEntry {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}
