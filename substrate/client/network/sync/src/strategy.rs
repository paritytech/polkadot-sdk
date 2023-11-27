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

use crate::{
	chain_sync::{ChainSync, ChainSyncAction, ChainSyncMode},
	types::{BadPeer, OpaqueStateRequest, OpaqueStateResponse, SyncStatus},
	warp::{EncodedProof, WarpProofRequest, WarpSync, WarpSyncAction, WarpSyncConfig},
};
use libp2p::PeerId;
use log::error;
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
use std::sync::Arc;

/// Log target for this file.
const LOG_TARGET: &'static str = "sync";

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
pub struct SyncingConfig {
	/// Syncing mode.
	pub mode: SyncMode,
	/// The number of parallel downloads to guard against slow peers.
	pub max_parallel_downloads: u32,
	/// Maximum number of blocks to request.
	pub max_blocks_per_request: u32,
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
	/// Syncing strategy has finished.
	Finished,
}

/// Proxy to specific syncing strategies.
pub enum SyncingStrategy<B: BlockT, Client> {
	WarpSyncStrategy(WarpSync<B, Client>),
	ChainSyncStrategy(ChainSync<B, Client>),
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
		config: &SyncingConfig,
		client: Arc<Client>,
		warp_sync_config: Option<WarpSyncConfig<B>>,
	) -> Result<Self, ClientError> {
		if let SyncMode::Warp = config.mode {
			let warp_sync_config = warp_sync_config
				.expect("Warp sync configuration must be supplied in warp sync mode.");
			Ok(Self::WarpSyncStrategy(WarpSync::new(client.clone(), warp_sync_config)))
		} else {
			Ok(Self::ChainSyncStrategy(ChainSync::new(
				chain_sync_mode(config.mode),
				client.clone(),
				config.max_parallel_downloads,
				config.max_blocks_per_request,
			)?))
		}
	}

	/// Notify that a new peer has connected.
	pub fn new_peer(&mut self, peer_id: PeerId, best_hash: B::Hash, best_number: NumberFor<B>) {
		match self {
			SyncingStrategy::WarpSyncStrategy(strategy) =>
				strategy.new_peer(peer_id, best_hash, best_number),
			SyncingStrategy::ChainSyncStrategy(strategy) =>
				strategy.new_peer(peer_id, best_hash, best_number),
		}
	}

	/// Notify that a peer has disconnected.
	pub fn peer_disconnected(&mut self, peer_id: &PeerId) {
		match self {
			SyncingStrategy::WarpSyncStrategy(strategy) => strategy.peer_disconnected(peer_id),
			SyncingStrategy::ChainSyncStrategy(strategy) => strategy.peer_disconnected(peer_id),
		}
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
		match self {
			SyncingStrategy::WarpSyncStrategy(_) =>
				Some((announce.header.hash(), *announce.header.number())),
			SyncingStrategy::ChainSyncStrategy(strategy) =>
				strategy.on_validated_block_announce(is_best, peer_id, announce),
		}
	}

	/// Configure an explicit fork sync request in case external code has detected that there is a
	/// stale fork missing.
	pub fn set_sync_fork_request(
		&mut self,
		peers: Vec<PeerId>,
		hash: &B::Hash,
		number: NumberFor<B>,
	) {
		match self {
			SyncingStrategy::WarpSyncStrategy(_) => {},
			SyncingStrategy::ChainSyncStrategy(strategy) =>
				strategy.set_sync_fork_request(peers, hash, number),
		}
	}

	/// Request extra justification.
	pub fn request_justification(&mut self, hash: &B::Hash, number: NumberFor<B>) {
		match self {
			SyncingStrategy::WarpSyncStrategy(_) => {},
			SyncingStrategy::ChainSyncStrategy(strategy) =>
				strategy.request_justification(hash, number),
		}
	}

	/// Clear extra justification requests.
	pub fn clear_justification_requests(&mut self) {
		match self {
			SyncingStrategy::WarpSyncStrategy(_) => {},
			SyncingStrategy::ChainSyncStrategy(strategy) => strategy.clear_justification_requests(),
		}
	}

	/// Report a justification import (successful or not).
	pub fn on_justification_import(&mut self, hash: B::Hash, number: NumberFor<B>, success: bool) {
		match self {
			SyncingStrategy::WarpSyncStrategy(_) => {},
			SyncingStrategy::ChainSyncStrategy(strategy) =>
				strategy.on_justification_import(hash, number, success),
		}
	}

	/// Process block response.
	pub fn on_block_response(
		&mut self,
		peer_id: PeerId,
		request: BlockRequest<B>,
		blocks: Vec<BlockData<B>>,
	) {
		match self {
			SyncingStrategy::WarpSyncStrategy(strategy) =>
				strategy.on_block_response(peer_id, request, blocks),
			SyncingStrategy::ChainSyncStrategy(strategy) =>
				strategy.on_block_response(peer_id, request, blocks),
		}
	}

	/// Process state response.
	pub fn on_state_response(&mut self, peer_id: PeerId, response: OpaqueStateResponse) {
		match self {
			SyncingStrategy::WarpSyncStrategy(strategy) =>
				strategy.on_state_response(peer_id, response),
			SyncingStrategy::ChainSyncStrategy(strategy) =>
				strategy.on_state_response(peer_id, response),
		}
	}

	///  Process warp proof response.
	pub fn on_warp_proof_response(&mut self, peer_id: &PeerId, response: EncodedProof) {
		match self {
			SyncingStrategy::WarpSyncStrategy(strategy) =>
				strategy.on_warp_proof_response(peer_id, response),
			SyncingStrategy::ChainSyncStrategy(_) => {},
		}
	}

	/// A batch of blocks have been processed, with or without errors.
	pub fn on_blocks_processed(
		&mut self,
		imported: usize,
		count: usize,
		results: Vec<(Result<BlockImportStatus<NumberFor<B>>, BlockImportError>, B::Hash)>,
	) {
		match self {
			SyncingStrategy::WarpSyncStrategy(strategy) =>
				strategy.on_blocks_processed(imported, count, results),
			SyncingStrategy::ChainSyncStrategy(strategy) =>
				strategy.on_blocks_processed(imported, count, results),
		}
	}

	/// Notify a syncing strategy that a block has been finalized.
	pub fn on_block_finalized(&mut self, hash: &B::Hash, number: NumberFor<B>) {
		match self {
			SyncingStrategy::WarpSyncStrategy(_) => {},
			SyncingStrategy::ChainSyncStrategy(strategy) =>
				strategy.on_block_finalized(hash, number),
		}
	}

	/// Inform sync about a new best imported block.
	pub fn update_chain_info(&mut self, best_hash: &B::Hash, best_number: NumberFor<B>) {
		match self {
			SyncingStrategy::WarpSyncStrategy(_) => {},
			SyncingStrategy::ChainSyncStrategy(strategy) =>
				strategy.update_chain_info(best_hash, best_number),
		}
	}

	// Are we in major sync mode?
	pub fn is_major_syncing(&self) -> bool {
		match self {
			SyncingStrategy::WarpSyncStrategy(strategy) =>
				strategy.status().state.is_major_syncing(),
			SyncingStrategy::ChainSyncStrategy(strategy) =>
				strategy.status().state.is_major_syncing(),
		}
	}

	/// Get the number of peers known to the syncing strategy.
	pub fn num_peers(&self) -> usize {
		match self {
			SyncingStrategy::WarpSyncStrategy(strategy) => strategy.num_peers(),
			SyncingStrategy::ChainSyncStrategy(strategy) => strategy.num_peers(),
		}
	}

	/// Returns the current sync status.
	pub fn status(&self) -> SyncStatus<B> {
		match self {
			SyncingStrategy::WarpSyncStrategy(strategy) => strategy.status(),
			SyncingStrategy::ChainSyncStrategy(strategy) => strategy.status(),
		}
	}

	/// Get the total number of downloaded blocks.
	pub fn num_downloaded_blocks(&self) -> usize {
		match self {
			SyncingStrategy::WarpSyncStrategy(_) => 0,
			SyncingStrategy::ChainSyncStrategy(strategy) => strategy.num_downloaded_blocks(),
		}
	}

	/// Get an estimate of the number of parallel sync requests.
	pub fn num_sync_requests(&self) -> usize {
		match self {
			SyncingStrategy::WarpSyncStrategy(_) => 0,
			SyncingStrategy::ChainSyncStrategy(strategy) => strategy.num_sync_requests(),
		}
	}

	/// Get actions that should be performed by the owner on the strategy's behalf
	#[must_use]
	pub fn actions(&mut self) -> Box<dyn Iterator<Item = SyncingAction<B>>> {
		match self {
			SyncingStrategy::WarpSyncStrategy(strategy) =>
				Box::new(strategy.actions().map(|action| match action {
					WarpSyncAction::SendWarpProofRequest { peer_id, request } =>
						SyncingAction::SendWarpProofRequest { peer_id, request },
					WarpSyncAction::SendBlockRequest { peer_id, request } =>
						SyncingAction::SendBlockRequest { peer_id, request },
					WarpSyncAction::SendStateRequest { peer_id, request } =>
						SyncingAction::SendStateRequest { peer_id, request },
					WarpSyncAction::DropPeer(bad_peer) => SyncingAction::DropPeer(bad_peer),
					WarpSyncAction::ImportBlocks { origin, blocks } =>
						SyncingAction::ImportBlocks { origin, blocks },
					WarpSyncAction::Finished => SyncingAction::Finished,
				})),
			SyncingStrategy::ChainSyncStrategy(strategy) =>
				Box::new(strategy.actions().map(|action| match action {
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
				})),
		}
	}

	pub fn switch_to_next(
		&mut self,
		config: &SyncingConfig,
		client: Arc<Client>,
		connected_peers: impl Iterator<Item = (PeerId, B::Hash, NumberFor<B>)>,
	) {
		match self {
			Self::WarpSyncStrategy(_) => {
				// `ChainSyncStrategy` continues `WarpSyncStrategy`.
				let mut chain_sync = match ChainSync::new(
					chain_sync_mode(config.mode),
					client,
					config.max_parallel_downloads,
					config.max_blocks_per_request,
				) {
					Ok(chain_sync) => chain_sync,
					Err(e) => {
						error!(target: LOG_TARGET, "Failed to start `ChainSync` due to error: {e}.");
						panic!("Failed to start `ChainSync` due to error: {e}.");
					},
				};
				// Let `ChainSync` know about connected peers.
				connected_peers.into_iter().for_each(|(peer_id, best_hash, best_number)| {
					chain_sync.new_peer(peer_id, best_hash, best_number)
				});

				*self = Self::ChainSyncStrategy(chain_sync);
			},
			Self::ChainSyncStrategy(_) => {
				error!(target: LOG_TARGET, "`ChainSyncStrategy` is final startegy, cannot switch to next.");
				debug_assert!(false);
			},
		}
	}
}
