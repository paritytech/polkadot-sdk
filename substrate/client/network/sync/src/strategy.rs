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

//! [`SyncingStrategy`] is a proxy between [`crate::engine::SyncingEngine`]
//! and specific syncing algorithms.

pub mod chain_sync;
mod state;
mod state_sync;
pub mod warp;

use crate::{
	types::{BadPeer, OpaqueStateRequest, OpaqueStateResponse, SyncStatus},
	LOG_TARGET,
};
use chain_sync::{ChainSync, ChainSyncAction, ChainSyncMode};
use libp2p::PeerId;
use log::{error, info, warn};
use parking_lot::Mutex;
use prometheus_endpoint::Registry;
use sc_client_api::{BlockBackend, ProofProvider};
use sc_consensus::{BlockImportError, BlockImportStatus, IncomingBlock};
use sc_network_common::sync::{
	message::{BlockAnnounce, BlockData, BlockRequest},
	SyncMode,
};
use sp_blockchain::{Error as ClientError, HeaderBackend, HeaderMetadata};
use sp_consensus::BlockOrigin;
use sp_runtime::{
	traits::{Block as BlockT, Header, NumberFor},
	Justifications,
};
use state::{StateStrategy, StateStrategyAction};
use std::{collections::HashMap, sync::Arc};
use warp::{EncodedProof, WarpProofRequest, WarpSync, WarpSyncAction, WarpSyncConfig};

/// Corresponding `ChainSync` mode.
fn chain_sync_mode(sync_mode: SyncMode) -> ChainSyncMode {
	match sync_mode {
		SyncMode::Full => ChainSyncMode::Full,
		SyncMode::LightState { skip_proofs, storage_chain_mode } =>
			ChainSyncMode::LightState { skip_proofs, storage_chain_mode },
		SyncMode::Warp => ChainSyncMode::Full,
	}
}

/// Syncing configuration containing data for all strategies.
#[derive(Clone, Debug)]
pub struct SyncingConfig {
	/// Syncing mode.
	pub mode: SyncMode,
	/// The number of parallel downloads to guard against slow peers.
	pub max_parallel_downloads: u32,
	/// Maximum number of blocks to request.
	pub max_blocks_per_request: u32,
	/// Prometheus metrics registry.
	pub metrics_registry: Option<Registry>,
}

enum PeerStatus {
	Available,
	Reserved,
}

impl PeerStatus {
	fn is_available(&self) -> bool {
		matches!(self, PeerStatus::Available)
	}
}

#[derive(Clone, Default)]
pub struct PeerPool {
	peers: Arc<Mutex<HashMap<PeerId, PeerStatus>>>,
}

impl PeerPool {
	fn add_peer(&self, peer_id: PeerId) {
		self.peers.lock().insert(peer_id, PeerStatus::Available);
	}

	fn remove_peer(&self, peer_id: &PeerId) {
		self.peers.lock().remove(peer_id);
	}

	fn available_peers(&self) -> Vec<PeerId> {
		self.peers
			.lock()
			.iter()
			.filter_map(
				|(peer_id, status)| if status.is_available() { Some(*peer_id) } else { None },
			)
			.collect()
	}

	fn try_reserve_peer(&self, peer_id: &PeerId) -> bool {
		match self.peers.lock().get_mut(peer_id) {
			Some(peer_status) => match peer_status {
				PeerStatus::Available => {
					*peer_status = PeerStatus::Reserved;
					true
				},
				PeerStatus::Reserved => false,
			},
			None => {
				warn!(target: LOG_TARGET, "Trying to reserve unknown peer {peer_id}.");
				false
			},
		}
	}

	fn free_peer(&self, peer_id: &PeerId) {
		match self.peers.lock().get_mut(peer_id) {
			Some(peer_status) => match peer_status {
				PeerStatus::Available => {
					warn!(target: LOG_TARGET, "Trying to free available peer {peer_id}.")
				},
				PeerStatus::Reserved => {
					*peer_status = PeerStatus::Available;
				},
			},
			None => {
				warn!(target: LOG_TARGET, "Trying to free unknown peer {peer_id}.");
			},
		}
	}
}

#[derive(Debug)]
pub enum SyncingAction<B: BlockT> {
	/// Send block request to peer. Always implies dropping a stale block request to the same peer.
	SendBlockRequest { peer_id: PeerId, request: BlockRequest<B> },
	/// Drop stale block request.
	CancelBlockRequest { peer_id: PeerId },
	/// Send state request to peer.
	SendStateRequest { peer_id: PeerId, request: OpaqueStateRequest },
	/// Send warp proof request to peer.
	SendWarpProofRequest { peer_id: PeerId, request: WarpProofRequest<B> },
	/// Peer misbehaved. Disconnect, report it and cancel any requests to it.
	DropPeer(BadPeer),
	/// Import blocks.
	ImportBlocks { origin: BlockOrigin, blocks: Vec<IncomingBlock<B>> },
	/// Import justifications.
	ImportJustifications {
		peer_id: PeerId,
		hash: B::Hash,
		number: NumberFor<B>,
		justifications: Justifications,
	},
}

/// Proxy to specific syncing strategies.
pub struct SyncingStrategy<B: BlockT, Client> {
	config: SyncingConfig,
	client: Arc<Client>,
	warp: Option<WarpSync<B, Client>>,
	state: Option<StateStrategy<B>>,
	chain_sync: Option<ChainSync<B, Client>>,
	peer_pool: PeerPool,
	peer_best_blocks: HashMap<PeerId, (B::Hash, NumberFor<B>)>,
}

impl<B: BlockT, Client> SyncingStrategy<B, Client>
where
	B: BlockT,
	Client: HeaderBackend<B>
		+ BlockBackend<B>
		+ HeaderMetadata<B, Error = sp_blockchain::Error>
		+ ProofProvider<B>
		+ Send
		+ Sync
		+ 'static,
{
	/// Initialize a new syncing startegy.
	pub fn new(
		config: SyncingConfig,
		client: Arc<Client>,
		warp_sync_config: Option<WarpSyncConfig<B>>,
	) -> Result<Self, ClientError> {
		if let SyncMode::Warp = config.mode {
			let warp_sync_config = warp_sync_config
				.expect("Warp sync configuration must be supplied in warp sync mode.");
			let peer_pool: PeerPool = Default::default();
			let warp_sync = WarpSync::new(client.clone(), warp_sync_config, peer_pool.clone());
			Ok(Self {
				config,
				client,
				warp: Some(warp_sync),
				state: None,
				chain_sync: None,
				peer_pool,
				peer_best_blocks: Default::default(),
			})
		} else {
			let peer_pool: PeerPool = Default::default();
			let chain_sync = ChainSync::new(
				chain_sync_mode(config.mode),
				client.clone(),
				config.max_parallel_downloads,
				config.max_blocks_per_request,
				config.metrics_registry.clone(),
				peer_pool.clone(),
			)?;
			Ok(Self {
				config,
				client,
				warp: None,
				state: None,
				chain_sync: Some(chain_sync),
				peer_pool,
				peer_best_blocks: Default::default(),
			})
		}
	}

	/// Notify that a new peer has connected.
	pub fn add_peer(&mut self, peer_id: PeerId, best_hash: B::Hash, best_number: NumberFor<B>) {
		self.peer_pool.add_peer(peer_id);
		self.peer_best_blocks.insert(peer_id, (best_hash, best_number));

		self.warp.iter_mut().for_each(|s| s.add_peer(peer_id, best_hash, best_number));
		self.state.iter_mut().for_each(|s| s.add_peer(peer_id, best_hash, best_number));
		self.chain_sync
			.iter_mut()
			.for_each(|s| s.add_peer(peer_id, best_hash, best_number));
	}

	/// Notify that a peer has disconnected.
	pub fn remove_peer(&mut self, peer_id: &PeerId) {
		self.warp.iter_mut().for_each(|s| s.remove_peer(peer_id));
		self.state.iter_mut().for_each(|s| s.remove_peer(peer_id));
		self.chain_sync.iter_mut().for_each(|s| s.remove_peer(peer_id));

		self.peer_pool.remove_peer(peer_id);
		self.peer_best_blocks.remove(peer_id);
	}

	/// Submit a validated block announcement.
	///
	/// Returns new best hash & best number of the peer if they are updated.
	pub fn on_validated_block_announce(
		&mut self,
		is_best: bool,
		peer_id: PeerId,
		announce: &BlockAnnounce<B::Header>,
	) -> Option<(B::Hash, NumberFor<B>)> {
		// Only `ChainSync` handles block announcements non-trivially.
		let new_best = if let Some(ref mut chain_sync) = self.chain_sync {
			chain_sync.on_validated_block_announce(is_best, peer_id, announce)
		} else {
			Some((announce.header.hash(), *announce.header.number()))
		};
		if let Some(new_best) = new_best {
			if let Some(best) = self.peer_best_blocks.get_mut(&peer_id) {
				*best = new_best;
			}
		}
		new_best
	}

	/// Configure an explicit fork sync request in case external code has detected that there is a
	/// stale fork missing.
	pub fn set_sync_fork_request(
		&mut self,
		peers: Vec<PeerId>,
		hash: &B::Hash,
		number: NumberFor<B>,
	) {
		// Fork requests are only handled by `ChainSync`.
		if let Some(ref mut chain_sync) = self.chain_sync {
			chain_sync.set_sync_fork_request(peers.clone(), hash, number);
		}
	}

	/// Request extra justification.
	pub fn request_justification(&mut self, hash: &B::Hash, number: NumberFor<B>) {
		// Justifications can only be requested via `ChainSync`.
		if let Some(ref mut chain_sync) = self.chain_sync {
			chain_sync.request_justification(hash, number);
		}
	}

	/// Clear extra justification requests.
	pub fn clear_justification_requests(&mut self) {
		// Justification requests can only be cleared by `ChainSync`.
		if let Some(ref mut chain_sync) = self.chain_sync {
			chain_sync.clear_justification_requests();
		}
	}

	/// Report a justification import (successful or not).
	pub fn on_justification_import(&mut self, hash: B::Hash, number: NumberFor<B>, success: bool) {
		// Only `ChainSync` is interested in justification import.
		if let Some(ref mut chain_sync) = self.chain_sync {
			chain_sync.on_justification_import(hash, number, success);
		}
	}

	/// Process block response.
	pub fn on_block_response(
		&mut self,
		peer_id: PeerId,
		request: BlockRequest<B>,
		blocks: Vec<BlockData<B>>,
	) {
		// Only `WarpSync` and `ChainSync` handle block responses.
		if let Some(ref mut warp) = self.warp {
			warp.on_block_response(peer_id, request, blocks);
		} else if let Some(ref mut chain_sync) = self.chain_sync {
			chain_sync.on_block_response(peer_id, request, blocks);
		}
	}

	/// Process state response.
	pub fn on_state_response(&mut self, peer_id: PeerId, response: OpaqueStateResponse) {
		// Only `StateStrategy` and `ChainSync` handle state responses.
		if let Some(ref mut state) = self.state {
			state.on_state_response(peer_id, response);
		} else if let Some(ref mut chain_sync) = self.chain_sync {
			chain_sync.on_state_response(peer_id, response);
		}
	}

	/// Process warp proof response.
	pub fn on_warp_proof_response(&mut self, peer_id: &PeerId, response: EncodedProof) {
		// Only `WarpSync` handles warp proof responses.
		if let Some(ref mut warp) = self.warp {
			warp.on_warp_proof_response(peer_id, response);
		}
	}

	/// A batch of blocks have been processed, with or without errors.
	pub fn on_blocks_processed(
		&mut self,
		imported: usize,
		count: usize,
		results: Vec<(Result<BlockImportStatus<NumberFor<B>>, BlockImportError>, B::Hash)>,
	) {
		// Only `StateStrategy` and `ChainSync` are interested in block processing notifications.

		if let Some(ref mut state) = self.state {
			state.on_blocks_processed(imported, count, results);
		} else if let Some(ref mut chain_sync) = self.chain_sync {
			chain_sync.on_blocks_processed(imported, count, results);
		}
	}

	/// Notify a syncing strategy that a block has been finalized.
	pub fn on_block_finalized(&mut self, hash: &B::Hash, number: NumberFor<B>) {
		// Only `ChainSync` is interested in block finalization notifications.
		if let Some(ref mut chain_sync) = self.chain_sync {
			chain_sync.on_block_finalized(hash, number);
		}
	}

	/// Inform sync about a new best imported block.
	pub fn update_chain_info(&mut self, best_hash: &B::Hash, best_number: NumberFor<B>) {
		// This is relevant to `ChainSync` only.
		if let Some(ref mut chain_sync) = self.chain_sync {
			chain_sync.update_chain_info(best_hash, best_number);
		}
	}

	// Are we in major sync mode?
	pub fn is_major_syncing(&self) -> bool {
		self.warp.is_some() ||
			self.state.is_some() ||
			match self.chain_sync {
				Some(ref s) => s.status().state.is_major_syncing(),
				None => unreachable!("At least one syncing startegy is active; qed"),
			}
	}

	/// Get the number of peers known to the syncing strategy.
	pub fn num_peers(&self) -> usize {
		self.peer_best_blocks.len()
	}

	/// Returns the current sync status.
	pub fn status(&self) -> SyncStatus<B> {
		// This function presumes that startegies are executed serially and must be refactored
		// once we have parallel strategies.
		if let Some(ref warp) = self.warp {
			warp.status()
		} else if let Some(ref state) = self.state {
			state.status()
		} else if let Some(ref chain_sync) = self.chain_sync {
			chain_sync.status()
		} else {
			unreachable!("At least one syncing startegy is always active; qed")
		}
	}

	/// Get the total number of downloaded blocks.
	pub fn num_downloaded_blocks(&self) -> usize {
		if let Some(ref chain_sync) = self.chain_sync {
			chain_sync.num_downloaded_blocks()
		} else {
			0
		}
	}

	/// Get an estimate of the number of parallel sync requests.
	pub fn num_sync_requests(&self) -> usize {
		if let Some(ref chain_sync) = self.chain_sync {
			chain_sync.num_sync_requests()
		} else {
			0
		}
	}

	/// Report Prometheus metrics
	pub fn report_metrics(&self) {
		if let Some(ref chain_sync) = self.chain_sync {
			chain_sync.report_metrics();
		}
	}

	/// Let `WarpSync` know about target block header
	pub fn set_warp_sync_target_block_header(
		&mut self,
		target_header: B::Header,
	) -> Result<(), ()> {
		if let Some(ref mut warp) = self.warp {
			warp.set_target_block(target_header);
			Ok(())
		} else {
			error!(
				target: LOG_TARGET,
				"Cannot set warp sync target block: no warp sync strategy is active."
			);
			debug_assert!(false);
			Err(())
		}
	}

	/// Get actions that should be performed by the owner on the strategy's behalf
	#[must_use]
	pub fn actions(&mut self) -> Result<Vec<SyncingAction<B>>, ClientError> {
		// This function presumes that strategies are executed serially and must be refactored once
		// we have parallel startegies.
		if let Some(ref mut warp) = self.warp {
			let mut actions = Vec::new();
			for action in warp.actions() {
				actions.push(match action {
					WarpSyncAction::SendWarpProofRequest { peer_id, request } =>
						SyncingAction::SendWarpProofRequest { peer_id, request },
					WarpSyncAction::SendBlockRequest { peer_id, request } =>
						SyncingAction::SendBlockRequest { peer_id, request },
					WarpSyncAction::DropPeer(bad_peer) => SyncingAction::DropPeer(bad_peer),
					WarpSyncAction::Finished => {
						self.proceed_to_next()?;
						return Ok(actions)
					},
				});
			}
			Ok(actions)
		} else if let Some(ref mut state) = self.state {
			let mut actions = Vec::new();
			for action in state.actions() {
				actions.push(match action {
					StateStrategyAction::SendStateRequest { peer_id, request } =>
						SyncingAction::SendStateRequest { peer_id, request },
					StateStrategyAction::DropPeer(bad_peer) => SyncingAction::DropPeer(bad_peer),
					StateStrategyAction::ImportBlocks { origin, blocks } =>
						SyncingAction::ImportBlocks { origin, blocks },
					StateStrategyAction::Finished => {
						self.proceed_to_next()?;
						return Ok(actions)
					},
				});
			}
			Ok(actions)
		} else if let Some(ref mut chain_sync) = self.chain_sync {
			Ok(chain_sync
				.actions()
				.map(|action| match action {
					ChainSyncAction::SendBlockRequest { peer_id, request } =>
						SyncingAction::SendBlockRequest { peer_id, request },
					ChainSyncAction::CancelBlockRequest { peer_id } =>
						SyncingAction::CancelBlockRequest { peer_id },
					ChainSyncAction::SendStateRequest { peer_id, request } =>
						SyncingAction::SendStateRequest { peer_id, request },
					ChainSyncAction::DropPeer(bad_peer) => SyncingAction::DropPeer(bad_peer),
					ChainSyncAction::ImportBlocks { origin, blocks } =>
						SyncingAction::ImportBlocks { origin, blocks },
					ChainSyncAction::ImportJustifications {
						peer_id,
						hash,
						number,
						justifications,
					} => SyncingAction::ImportJustifications {
						peer_id,
						hash,
						number,
						justifications,
					},
				})
				.collect())
		} else {
			unreachable!("At least one syncing startegy is always active; qed")
		}
	}

	/// Proceed with the next strategy if the active one finished.
	pub fn proceed_to_next(&mut self) -> Result<(), ClientError> {
		// The strategies are switched as `WarpSync` -> `StateStartegy` -> `ChainSync`.
		if let Some(ref mut warp) = self.warp {
			match warp.take_result() {
				Some(res) => {
					info!(
						target: LOG_TARGET,
						"Warp sync finished, continuing with state sync."
					);
					let state_sync = StateStrategy::new(
						self.client.clone(),
						res.target_header,
						res.target_body,
						res.target_justifications,
						false,
						self.peer_best_blocks
							.iter()
							.map(|(peer_id, (_, best_number))| (*peer_id, *best_number)),
						self.peer_pool.clone(),
					);

					self.warp = None;
					self.state = Some(state_sync);
					Ok(())
				},
				None => {
					error!(
						target: LOG_TARGET,
						"Warp sync failed. Continuing with full sync."
					);
					let mut chain_sync = match ChainSync::new(
						chain_sync_mode(self.config.mode),
						self.client.clone(),
						self.config.max_parallel_downloads,
						self.config.max_blocks_per_request,
						self.config.metrics_registry.clone(),
						self.peer_pool.clone(),
					) {
						Ok(chain_sync) => chain_sync,
						Err(e) => {
							error!(target: LOG_TARGET, "Failed to start `ChainSync`.");
							return Err(e)
						},
					};
					// Let `ChainSync` know about connected peers.
					self.peer_best_blocks.iter().for_each(|(peer_id, (best_hash, best_number))| {
						chain_sync.add_peer(*peer_id, *best_hash, *best_number)
					});

					self.warp = None;
					self.chain_sync = Some(chain_sync);
					Ok(())
				},
			}
		} else if let Some(_) = &self.state {
			info!(target: LOG_TARGET, "State sync finished, continuing with block sync.");
			let mut chain_sync = match ChainSync::new(
				chain_sync_mode(self.config.mode),
				self.client.clone(),
				self.config.max_parallel_downloads,
				self.config.max_blocks_per_request,
				self.config.metrics_registry.clone(),
				self.peer_pool.clone(),
			) {
				Ok(chain_sync) => chain_sync,
				Err(e) => {
					error!(target: LOG_TARGET, "Failed to start `ChainSync`.");
					return Err(e);
				},
			};
			// Let `ChainSync` know about connected peers.
			self.peer_best_blocks.iter().for_each(|(peer_id, (best_hash, best_number))| {
				chain_sync.add_peer(*peer_id, *best_hash, *best_number)
			});

			self.state = None;
			self.chain_sync = Some(chain_sync);
			Ok(())
		} else {
			unreachable!("Only warp & state strategies can finish; qed")
		}
	}
}
