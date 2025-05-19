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
mod backend;
mod connected;
mod db;

use futures::channel::oneshot;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::{
	validator_side_experimental::{
		common::{
			PeerInfo, PeerState, Score, CONNECTED_PEERS_LIMIT, CONNECTED_PEERS_PARA_LIMIT,
			INACTIVITY_DECAY, MAX_STARTUP_ANCESTRY_LOOKBACK, VALID_INCLUDED_CANDIDATE_BUMP,
		},
		error::{Error, Result},
	},
	LOG_TARGET,
};
pub use backend::Backend;
use connected::ConnectedPeers;
pub use db::Db;
use polkadot_node_network_protocol::{
	peer_set::{CollationVersion, PeerSet},
	PeerId,
};
use polkadot_node_subsystem::{
	messages::{ChainApiMessage, NetworkBridgeTxMessage},
	ActivatedLeaf, CollatorProtocolSenderTrait,
};
use polkadot_node_subsystem_util::{
	request_candidate_events, request_candidates_pending_availability, runtime::recv_runtime,
};
use polkadot_primitives::{
	vstaging::{CandidateDescriptorVersion, CandidateEvent},
	BlockNumber, CandidateHash, Hash, Id as ParaId,
};

pub struct ReputationUpdate {
	pub peer_id: PeerId,
	pub para_id: ParaId,
	pub value: Score,
	pub kind: ReputationUpdateKind,
}

pub enum ReputationUpdateKind {
	Bump,
	Slash,
}

#[derive(Debug, PartialEq)]
enum TryAcceptOutcome {
	Added,
	// This can hold more than one `PeerId` because before receiving the `Declare` message,
	// one peer can hold connection slots for multiple paraids.
	// The set can also be empty if this peer replaced some other peer's slot but that other peer
	// maintained a connection slot for another para (therefore not disconnected).
	// The number of peers in the set is bound to the number of scheduled paras.
	Replaced(HashSet<PeerId>),
	Rejected,
}

impl TryAcceptOutcome {
	fn combine(self, other: Self) -> Self {
		use TryAcceptOutcome::*;
		match (self, other) {
			(Added, Added) => Added,
			(Rejected, Rejected) => Rejected,
			(Added, Rejected) | (Rejected, Added) => Added,
			(Replaced(mut replaced_a), Replaced(replaced_b)) => {
				replaced_a.extend(replaced_b);
				Replaced(replaced_a)
			},
			(_, Replaced(replaced)) | (Replaced(replaced), _) => Replaced(replaced),
		}
	}
}

#[derive(Debug, PartialEq)]
enum DeclarationOutcome {
	Rejected,
	Switched(ParaId),
	Accepted,
}

pub struct PeerManager<B> {
	db: B,
	connected: ConnectedPeers,
}

impl<B: Backend> PeerManager<B> {
	/// Initialize the peer manager (called on subsystem startup, after the node finished syncing to
	/// the tip of the chain).
	pub async fn startup<Sender: CollatorProtocolSenderTrait>(
		sender: &mut Sender,
		scheduled_paras: BTreeSet<ParaId>,
	) -> Result<Self> {
		// Open the Db.
		let db = B::new().await;

		let mut instance = Self {
			db,
			connected: ConnectedPeers::new(
				scheduled_paras,
				CONNECTED_PEERS_LIMIT,
				CONNECTED_PEERS_PARA_LIMIT,
			),
		};

		let (latest_finalized_block_number, latest_finalized_block_hash) =
			get_latest_finalized_block(sender).await?;

		let processed_finalized_block_number =
			instance.db.processed_finalized_block_number().await.unwrap_or_default();

		let bumps = extract_reputation_bumps_on_new_finalized_block(
			sender,
			processed_finalized_block_number,
			(latest_finalized_block_number, latest_finalized_block_hash),
		)
		.await?;

		instance.db.process_bumps(latest_finalized_block_number, bumps, None).await;

		Ok(instance)
	}

	/// Handle a new block finality notification, by updating peer reputations.
	pub async fn update_reputations_on_new_finalized_block<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		(finalized_block_hash, finalized_block_number): (Hash, BlockNumber),
	) -> Result<()> {
		let processed_finalized_block_number =
			self.db.processed_finalized_block_number().await.unwrap_or_default();

		let bumps = extract_reputation_bumps_on_new_finalized_block(
			sender,
			processed_finalized_block_number,
			(finalized_block_number, finalized_block_hash),
		)
		.await?;

		let updates = self
			.db
			.process_bumps(
				finalized_block_number,
				bumps,
				Some(Score::new(INACTIVITY_DECAY).expect("INACTIVITY_DECAY is a valid score")),
			)
			.await;
		for update in updates {
			self.connected.update_reputation(update);
		}

		Ok(())
	}

	/// Process the registered paras and cleanup all data pertaining to any unregistered paras, if
	/// any. Should be called every N finalized block notifications, since it's expected that para
	/// deregistrations are rare.
	pub async fn registered_paras_update(&mut self, registered_paras: BTreeSet<ParaId>) {
		// Tell the DB to cleanup paras that are no longer registered. No need to clean up the
		// connected peers state, since it will get automatically cleaned up as the claim queue
		// gets rid of these stale assignments.
		self.db.prune_paras(registered_paras).await;
	}

	/// Process a potential change of the scheduled paras.
	pub async fn scheduled_paras_update<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		scheduled_paras: BTreeSet<ParaId>,
	) {
		let mut prev_scheduled_paras: BTreeSet<_> =
			self.connected.scheduled_paras().copied().collect();

		if prev_scheduled_paras == scheduled_paras {
			// Nothing to do if the scheduled paras didn't change.
			return
		}

		// Recreate the connected peers based on the new schedule and try populating it again based
		// on their reputations. Disconnect any peers that couldn't be kept
		let mut new_instance =
			ConnectedPeers::new(scheduled_paras, CONNECTED_PEERS_LIMIT, CONNECTED_PEERS_PARA_LIMIT);

		std::mem::swap(&mut new_instance, &mut self.connected);
		let prev_instance = new_instance;
		let (prev_peers, cached_scores) = prev_instance.consume();

		// Build a closure that can be used to first query the in-memory past reputations of the
		// peers before reaching for the DB.

		// Borrow these for use in the closure.
		let cached_scores = &cached_scores;
		let db = &self.db;
		let reputation_query_fn = |peer_id: PeerId, para_id: ParaId| async move {
			if let Some(cached_score) =
				cached_scores.get(&para_id).and_then(|per_para| per_para.get_score(&peer_id))
			{
				cached_score
			} else {
				db.query(&peer_id, &para_id).await.unwrap_or_default()
			}
		};

		// See which of the old peers we should keep.
		let mut peers_to_disconnect = HashSet::new();
		for (peer_id, peer_info) in prev_peers {
			let outcome = self.connected.try_accept(reputation_query_fn, peer_id, peer_info).await;

			match outcome {
				TryAcceptOutcome::Rejected => {
					peers_to_disconnect.insert(peer_id);
				},
				TryAcceptOutcome::Replaced(replaced_peer_ids) => {
					peers_to_disconnect.extend(replaced_peer_ids);
				},
				TryAcceptOutcome::Added => {},
			}
		}

		// Disconnect peers that couldn't be kept.
		self.disconnect_peers(sender, peers_to_disconnect).await;
	}

	/// Process a declaration message of a peer.
	pub async fn declared<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		peer_id: PeerId,
		para_id: ParaId,
	) {
		let Some(peer_info) = self.connected.peer_info(&peer_id).cloned() else { return };
		let outcome = self.connected.declared(peer_id, para_id);

		match outcome {
			DeclarationOutcome::Accepted => {
				gum::debug!(
					target: LOG_TARGET,
					?para_id,
					?peer_id,
					"Peer declared",
				);
			},
			DeclarationOutcome::Switched(old_para_id) => {
				gum::debug!(
					target: LOG_TARGET,
					?para_id,
					?old_para_id,
					?peer_id,
					"Peer switched collating paraid. Trying to accept it on the new one.",
				);

				self.try_accept_connection(sender, peer_id, peer_info).await;
			},
			DeclarationOutcome::Rejected => {
				gum::debug!(
					target: LOG_TARGET,
					?para_id,
					?peer_id,
					"Peer declared but rejected. Going to disconnect.",
				);

				self.disconnect_peers(sender, [peer_id].into_iter().collect()).await;
			},
		}
	}

	/// Slash a peer's reputation for this paraid.
	pub async fn slash_reputation(&mut self, peer_id: &PeerId, para_id: &ParaId, value: Score) {
		gum::debug!(
			target: LOG_TARGET,
			?peer_id,
			?para_id,
			?value,
			"Slashing peer's reputation",
		);

		self.db.slash(peer_id, para_id, value).await;
		self.connected.update_reputation(ReputationUpdate {
			peer_id: *peer_id,
			para_id: *para_id,
			value,
			kind: ReputationUpdateKind::Slash,
		});
	}

	/// Process a peer disconnected event coming from the network.
	pub fn disconnected(&mut self, peer_id: &PeerId) {
		self.connected.remove(peer_id);
	}

	/// A connection was made, triage it. Return whether or not is was kept.
	pub async fn try_accept_connection<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		peer_id: PeerId,
		peer_info: PeerInfo,
	) -> bool {
		let db = &self.db;
		let reputation_query_fn = |peer_id: PeerId, para_id: ParaId| async move {
			// Go straight to the DB. We only store in-memory the reputations of connected peers.
			db.query(&peer_id, &para_id).await.unwrap_or_default()
		};

		let outcome = self.connected.try_accept(reputation_query_fn, peer_id, peer_info).await;

		match outcome {
			TryAcceptOutcome::Added => true,
			TryAcceptOutcome::Replaced(other_peers) => {
				gum::trace!(
					target: LOG_TARGET,
					"Peer {:?} replaced the connection slots of other peers: {:?}",
					peer_id,
					&other_peers
				);
				self.disconnect_peers(sender, other_peers).await;
				true
			},
			TryAcceptOutcome::Rejected => {
				gum::debug!(
					target: LOG_TARGET,
					?peer_id,
					"Peer connection was rejected",
				);
				self.disconnect_peers(sender, [peer_id].into_iter().collect()).await;
				false
			},
		}
	}

	/// Retrieve the score of the connected peer. We assume the peer is declared for this paraid.
	pub fn connected_peer_score(&self, peer_id: &PeerId, para_id: &ParaId) -> Option<Score> {
		self.connected.peer_score(peer_id, para_id)
	}

	async fn disconnect_peers<Sender: CollatorProtocolSenderTrait>(
		&self,
		sender: &mut Sender,
		peers: HashSet<PeerId>,
	) {
		gum::trace!(
			target: LOG_TARGET,
			?peers,
			"Disconnecting peers",
		);

		sender
			.send_message(NetworkBridgeTxMessage::DisconnectPeers(
				peers.into_iter().collect(),
				PeerSet::Collation,
			))
			.await;
	}
}

async fn get_ancestors<Sender: CollatorProtocolSenderTrait>(
	sender: &mut Sender,
	k: usize,
	hash: Hash,
) -> Result<Vec<Hash>> {
	let (tx, rx) = oneshot::channel();
	sender
		.send_message(ChainApiMessage::Ancestors { hash, k, response_channel: tx })
		.await;

	Ok(rx.await.map_err(|_| Error::CanceledAncestors)??)
}

async fn get_latest_finalized_block<Sender: CollatorProtocolSenderTrait>(
	sender: &mut Sender,
) -> Result<(BlockNumber, Hash)> {
	let (tx, rx) = oneshot::channel();
	sender.send_message(ChainApiMessage::FinalizedBlockNumber(tx)).await;

	let block_number = rx.await.map_err(|_| Error::CanceledFinalizedBlockNumber)??;

	let (tx, rx) = oneshot::channel();
	sender.send_message(ChainApiMessage::FinalizedBlockHash(block_number, tx)).await;

	let block_hash = rx
		.await
		.map_err(|_| Error::CanceledFinalizedBlockHash)??
		.ok_or_else(|| Error::FinalizedBlockNotFound(block_number))?;

	Ok((block_number, block_hash))
}

async fn extract_reputation_bumps_on_new_finalized_block<Sender: CollatorProtocolSenderTrait>(
	sender: &mut Sender,
	processed_finalized_block_number: BlockNumber,
	(latest_finalized_block_number, latest_finalized_block_hash): (BlockNumber, Hash),
) -> Result<BTreeMap<ParaId, HashMap<PeerId, Score>>> {
	if latest_finalized_block_number < processed_finalized_block_number {
		// Shouldn't be possible, but in this case there is no other initialisation needed.
		gum::warn!(
			target: LOG_TARGET,
			latest_finalized_block_number,
			?latest_finalized_block_hash,
			"Peer manager stored finalized block number {} is higher than the latest finalized block.",
			processed_finalized_block_number,
		);
		return Ok(BTreeMap::new())
	}

	let ancestry_len = std::cmp::min(
		latest_finalized_block_number.saturating_sub(processed_finalized_block_number),
		MAX_STARTUP_ANCESTRY_LOOKBACK,
	);

	if ancestry_len == 0 {
		return Ok(BTreeMap::new())
	}

	let mut ancestors =
		get_ancestors(sender, ancestry_len as usize, latest_finalized_block_hash).await?;
	ancestors.push(latest_finalized_block_hash);
	ancestors.reverse();

	gum::trace!(
		target: LOG_TARGET,
		?latest_finalized_block_hash,
		processed_finalized_block_number,
		"Processing reputation bumps for finalized relay parent {} and its {} ancestors",
		latest_finalized_block_number,
		ancestry_len
	);

	let mut v2_candidates_per_rp: HashMap<Hash, BTreeMap<ParaId, HashSet<CandidateHash>>> =
		HashMap::with_capacity(ancestors.len());

	for i in 1..ancestors.len() {
		let rp = ancestors[i];
		let parent_rp = ancestors[i - 1];
		let candidate_events = recv_runtime(request_candidate_events(rp, sender).await).await?;

		for event in candidate_events {
			if let CandidateEvent::CandidateIncluded(receipt, _, _, _) = event {
				// Only v2 receipts can contain UMP signals.
				if receipt.descriptor.version() == CandidateDescriptorVersion::V2 {
					v2_candidates_per_rp
						.entry(parent_rp)
						.or_default()
						.entry(receipt.descriptor.para_id())
						.or_default()
						.insert(receipt.hash());
				}
			}
		}
	}

	// This could be removed if we implemented https://github.com/paritytech/polkadot-sdk/issues/7732.
	let mut updates: BTreeMap<ParaId, HashMap<PeerId, Score>> = BTreeMap::new();
	for (rp, per_para) in v2_candidates_per_rp {
		for (para_id, included_candidates) in per_para {
			let candidates_pending_availability =
				recv_runtime(request_candidates_pending_availability(rp, para_id, sender).await)
					.await?;

			for candidate in candidates_pending_availability {
				let candidate_hash = candidate.hash();
				if included_candidates.contains(&candidate_hash) {
					match candidate.commitments.ump_signals() {
						Ok(ump_signals) => {
							if let Some(approved_peer) = ump_signals.approved_peer() {
								match PeerId::from_bytes(approved_peer) {
									Ok(peer_id) => updates
										.entry(para_id)
										.or_default()
										.entry(peer_id)
										.or_default()
										.saturating_add(VALID_INCLUDED_CANDIDATE_BUMP),
									Err(err) => {
										// Collator sent an invalid peerid. It's only harming
										// itself.
										gum::debug!(
											target: LOG_TARGET,
											?candidate_hash,
											"UMP signal contains invalid ApprovedPeer id: {}",
											err
										);
									},
								}
							}
						},
						Err(err) => {
							// This should never happen, as the ump signals are checked during
							// on-chain backing.
							gum::warn!(
								target: LOG_TARGET,
								?candidate_hash,
								"Failed to parse UMP signals for included candidate: {}",
								err
							);
						},
					}
				}
			}
		}
	}

	Ok(updates)
}
