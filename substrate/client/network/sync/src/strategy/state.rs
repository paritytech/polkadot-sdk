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

//! State sync strategy.

use crate::{
	schema::v1::StateResponse,
	strategy::state_sync::{ImportResult, StateSync},
	types::{BadPeer, OpaqueStateRequest, OpaqueStateResponse, SyncState, SyncStatus},
	LOG_TARGET,
};
use libp2p::PeerId;
use log::{debug, error, info, trace};
use sc_client_api::ProofProvider;
use sc_consensus::{BlockImportError, BlockImportStatus, IncomingBlock};
use sp_consensus::BlockOrigin;
use sp_runtime::{
	traits::{Block as BlockT, NumberFor},
	Justifications, SaturatedConversion,
};
use std::{collections::HashMap, sync::Arc};

mod rep {
	use sc_network::ReputationChange as Rep;

	/// Peer response data does not have requested bits.
	pub const BAD_RESPONSE: Rep = Rep::new(-(1 << 12), "Incomplete response");

	/// Reputation change for peers which send us a known bad state.
	pub const BAD_STATE: Rep = Rep::new(-(1 << 29), "Bad state");
}

/// Action that should be performed on [`StateStrategy`]'s behalf.
pub enum StateStrategyAction<B: BlockT> {
	/// Send state request to peer.
	SendStateRequest { peer_id: PeerId, request: OpaqueStateRequest },
	/// Disconnect and report peer.
	DropPeer(BadPeer),
	/// Import blocks.
	ImportBlocks { origin: BlockOrigin, blocks: Vec<IncomingBlock<B>> },
	/// State sync has finished.
	Finished,
}

enum PeerState {
	Available,
	DownloadingState,
}

impl PeerState {
	fn is_available(&self) -> bool {
		matches!(self, PeerState::Available)
	}
}

struct Peer<B: BlockT> {
	best_number: NumberFor<B>,
	state: PeerState,
}

/// Syncing strategy that downloads and imports a recent state directly.
pub struct StateStrategy<B: BlockT, Client> {
	state_sync: StateSync<B, Client>,
	peers: HashMap<PeerId, Peer<B>>,
	actions: Vec<StateStrategyAction<B>>,
}

impl<B, Client> StateStrategy<B, Client>
where
	B: BlockT,
	Client: ProofProvider<B> + Send + Sync + 'static,
{
	// Create a new instance.
	pub fn new(
		client: Arc<Client>,
		target_header: B::Header,
		target_body: Option<Vec<B::Extrinsic>>,
		target_justifications: Option<Justifications>,
		skip_proof: bool,
		initial_peers: impl Iterator<Item = (PeerId, NumberFor<B>)>,
	) -> Self {
		let peers = initial_peers
			.map(|(peer_id, best_number)| {
				(peer_id, Peer { best_number, state: PeerState::Available })
			})
			.collect();
		Self {
			state_sync: StateSync::new(
				client,
				target_header,
				target_body,
				target_justifications,
				skip_proof,
			),
			peers,
			actions: Vec::new(),
		}
	}

	/// Notify that a new peer has connected.
	pub fn add_peer(&mut self, peer_id: PeerId, _best_hash: B::Hash, best_number: NumberFor<B>) {
		self.peers.insert(peer_id, Peer { best_number, state: PeerState::Available });
	}

	/// Notify that a peer has disconnected.
	pub fn remove_peer(&mut self, peer_id: &PeerId) {
		self.peers.remove(peer_id);
	}

	/// Process state response.
	pub fn on_state_response(&mut self, peer_id: PeerId, response: OpaqueStateResponse) {
		if let Err(bad_peer) = self.on_state_response_inner(peer_id, response) {
			self.actions.push(StateStrategyAction::DropPeer(bad_peer));
		}
	}

	fn on_state_response_inner(
		&mut self,
		peer_id: PeerId,
		response: OpaqueStateResponse,
	) -> Result<(), BadPeer> {
		if let Some(peer) = self.peers.get_mut(&peer_id) {
			peer.state = PeerState::Available;
		}

		let response: Box<StateResponse> = response.0.downcast().map_err(|_error| {
			error!(
				target: LOG_TARGET,
				"Failed to downcast opaque state response, this is an implementation bug."
			);

			BadPeer(peer_id, rep::BAD_RESPONSE)
		})?;

		debug!(
			target: LOG_TARGET,
			"Importing state data from {} with {} keys, {} proof nodes.",
			peer_id,
			response.entries.len(),
			response.proof.len(),
		);

		match self.state_sync.import(*response) {
			ImportResult::Import(hash, header, state, body, justifications) => {
				let origin = BlockOrigin::NetworkInitialSync;
				let block = IncomingBlock {
					hash,
					header: Some(header),
					body,
					indexed_body: None,
					justifications,
					origin: None,
					allow_missing_state: true,
					import_existing: true,
					skip_execution: true,
					state: Some(state),
				};
				debug!(target: LOG_TARGET, "State download is complete. Import is queued");
				self.actions
					.push(StateStrategyAction::ImportBlocks { origin, blocks: vec![block] });
				Ok(())
			},
			ImportResult::Continue => Ok(()),
			ImportResult::BadResponse => {
				debug!(target: LOG_TARGET, "Bad state data received from {peer_id}");
				Err(BadPeer(peer_id, rep::BAD_STATE))
			},
		}
	}

	/// A batch of blocks have been processed, with or without errors.
	///
	/// Normally this should be called when target block with state is imported.
	pub fn on_blocks_processed(
		&mut self,
		imported: usize,
		count: usize,
		results: Vec<(Result<BlockImportStatus<NumberFor<B>>, BlockImportError>, B::Hash)>,
	) {
		trace!(target: LOG_TARGET, "State sync: imported {imported} of {count}.");

		let results = results
			.into_iter()
			.filter_map(|(result, hash)| {
				if hash == self.state_sync.target() {
					Some(result)
				} else {
					debug!(
						target: LOG_TARGET,
						"Unexpected block processed: {hash} with result {result:?}.",
					);
					None
				}
			})
			.collect::<Vec<_>>();

		if !results.is_empty() {
			// We processed the target block
			results.iter().filter_map(|result| result.as_ref().err()).for_each(|e| {
				error!(
					target: LOG_TARGET,
					"Failed to import target block with state: {e:?}."
				);
			});
			match results.into_iter().any(|result| result.is_ok()) {
				true => {
					info!(
						target: LOG_TARGET,
						"State sync is complete ({} MiB), continuing with block sync.",
						self.state_sync.progress().size / (1024 * 1024),
					);
				},
				false => {
					error!(
						target: LOG_TARGET,
						"State sync failed. Falling back to full sync.",
					);
					// TODO: Test this scenario.
				},
			}
			self.actions.push(StateStrategyAction::Finished);
		}
	}

	/// Produce state request.
	fn state_request(&mut self) -> Option<(PeerId, OpaqueStateRequest)> {
		if self.state_sync.is_complete() {
			return None
		}

		if self
			.peers
			.iter()
			.any(|(_, peer)| matches!(peer.state, PeerState::DownloadingState))
		{
			// Only one state request at a time is possible.
			return None
		}

		let peer_id = self
			.schedule_next_peer(PeerState::DownloadingState, self.state_sync.target_block_num())?;
		let request = self.state_sync.next_request();
		trace!(
			target: LOG_TARGET,
			"New state request to {peer_id}: {request:?}.",
		);
		Some((peer_id, OpaqueStateRequest(Box::new(request))))
	}

	fn schedule_next_peer(
		&mut self,
		new_state: PeerState,
		min_best_number: NumberFor<B>,
	) -> Option<PeerId> {
		let mut targets: Vec<_> = self.peers.values().map(|p| p.best_number).collect();
		if !targets.is_empty() {
			targets.sort();
			let median = targets[targets.len() / 2];
			let threshold = std::cmp::max(median, min_best_number);
			// Find a random peer that is synced as much as peer majority and is above
			// `min_best_number`.
			for (peer_id, peer) in self.peers.iter_mut() {
				if peer.state.is_available() && peer.best_number >= threshold {
					peer.state = new_state;
					return Some(*peer_id)
				}
			}
		}
		None
	}

	/// Returns the current sync status.
	pub fn status(&self) -> SyncStatus<B> {
		SyncStatus {
			state: if self.state_sync.is_complete() {
				SyncState::Idle
			} else {
				SyncState::Downloading { target: self.state_sync.target_block_num() }
			},
			best_seen_block: Some(self.state_sync.target_block_num()),
			num_peers: self.peers.len().saturated_into(),
			num_connected_peers: self.peers.len().saturated_into(),
			queued_blocks: 0,
			state_sync: Some(self.state_sync.progress()),
			warp_sync: None,
		}
	}

	/// Get the number of peers known to syncing.
	pub fn num_peers(&self) -> usize {
		self.peers.len()
	}

	/// Get actions that should be performed by the owner on [`WarpSync`]'s behalf
	#[must_use]
	pub fn actions(&mut self) -> impl Iterator<Item = StateStrategyAction<B>> {
		let state_request = self
			.state_request()
			.into_iter()
			.map(|(peer_id, request)| StateStrategyAction::SendStateRequest { peer_id, request });
		self.actions.extend(state_request);

		std::mem::take(&mut self.actions).into_iter()
	}
}
