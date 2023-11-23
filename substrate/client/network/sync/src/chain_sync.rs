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

//! Contains the state of the chain synchronization process
//!
//! At any given point in time, a running node tries as much as possible to be at the head of the
//! chain. This module handles the logic of which blocks to request from remotes, and processing
//! responses. It yields blocks to check and potentially move to the database.
//!
//! # Usage
//!
//! The `ChainSync` struct maintains the state of the block requests. Whenever something happens on
//! the network, or whenever a block has been successfully verified, call the appropriate method in
//! order to update it.

use crate::{
	blocks::BlockCollection,
	extra_requests::ExtraRequests,
	schema::v1::StateResponse,
	state::{ImportResult, StateSync},
	types::{
		BadPeer, Metrics, OpaqueStateRequest, OpaqueStateResponse, PeerInfo, SyncMode, SyncState,
		SyncStatus,
	},
	warp::{
		self, EncodedProof, WarpProofImportResult, WarpProofRequest, WarpSync, WarpSyncConfig,
		WarpSyncPhase, WarpSyncProgress,
	},
};

use codec::Encode;
use libp2p::PeerId;
use log::{debug, error, info, trace, warn};

use sc_client_api::{BlockBackend, ProofProvider};
use sc_consensus::{BlockImportError, BlockImportStatus, IncomingBlock};
use sc_network_common::sync::message::{
	BlockAnnounce, BlockAttributes, BlockData, BlockRequest, BlockResponse, Direction, FromBlock,
};
use sp_arithmetic::traits::Saturating;
use sp_blockchain::{Error as ClientError, HeaderBackend, HeaderMetadata};
use sp_consensus::{BlockOrigin, BlockStatus};
use sp_runtime::{
	traits::{
		Block as BlockT, CheckedSub, Hash, HashingFor, Header as HeaderT, NumberFor, One,
		SaturatedConversion, Zero,
	},
	EncodedJustification, Justifications,
};

use std::{
	collections::{HashMap, HashSet},
	ops::Range,
	sync::Arc,
};

#[cfg(test)]
mod test;

/// Log target for this file.
const LOG_TARGET: &'static str = "sync";

/// Maximum blocks to store in the import queue.
const MAX_IMPORTING_BLOCKS: usize = 2048;

/// Maximum blocks to download ahead of any gap.
const MAX_DOWNLOAD_AHEAD: u32 = 2048;

/// Maximum blocks to look backwards. The gap is the difference between the highest block and the
/// common block of a node.
const MAX_BLOCKS_TO_LOOK_BACKWARDS: u32 = MAX_DOWNLOAD_AHEAD / 2;

/// Pick the state to sync as the latest finalized number minus this.
const STATE_SYNC_FINALITY_THRESHOLD: u32 = 8;

/// We use a heuristic that with a high likelihood, by the time
/// `MAJOR_SYNC_BLOCKS` have been imported we'll be on the same
/// chain as (or at least closer to) the peer so we want to delay
/// the ancestor search to not waste time doing that when we are
/// so far behind.
const MAJOR_SYNC_BLOCKS: u8 = 5;

/// Number of peers that need to be connected before warp sync is started.
const MIN_PEERS_TO_START_WARP_SYNC: usize = 3;

mod rep {
	use sc_network::ReputationChange as Rep;
	/// Reputation change when a peer sent us a message that led to a
	/// database read error.
	pub const BLOCKCHAIN_READ_ERROR: Rep = Rep::new(-(1 << 16), "DB Error");

	/// Reputation change when a peer sent us a status message with a different
	/// genesis than us.
	pub const GENESIS_MISMATCH: Rep = Rep::new(i32::MIN, "Genesis mismatch");

	/// Reputation change for peers which send us a block with an incomplete header.
	pub const INCOMPLETE_HEADER: Rep = Rep::new(-(1 << 20), "Incomplete header");

	/// Reputation change for peers which send us a block which we fail to verify.
	pub const VERIFICATION_FAIL: Rep = Rep::new(-(1 << 29), "Block verification failed");

	/// Reputation change for peers which send us a known bad block.
	pub const BAD_BLOCK: Rep = Rep::new(-(1 << 29), "Bad block");

	/// Peer did not provide us with advertised block data.
	pub const NO_BLOCK: Rep = Rep::new(-(1 << 29), "No requested block data");

	/// Reputation change for peers which send us non-requested block data.
	pub const NOT_REQUESTED: Rep = Rep::new(-(1 << 29), "Not requested block data");

	/// Reputation change for peers which send us a block with bad justifications.
	pub const BAD_JUSTIFICATION: Rep = Rep::new(-(1 << 16), "Bad justification");

	/// Reputation change when a peer sent us invlid ancestry result.
	pub const UNKNOWN_ANCESTOR: Rep = Rep::new(-(1 << 16), "DB Error");

	/// Peer response data does not have requested bits.
	pub const BAD_RESPONSE: Rep = Rep::new(-(1 << 12), "Incomplete response");
}

enum AllowedRequests {
	Some(HashSet<PeerId>),
	All,
}

impl AllowedRequests {
	fn add(&mut self, id: &PeerId) {
		if let Self::Some(ref mut set) = self {
			set.insert(*id);
		}
	}

	fn take(&mut self) -> Self {
		std::mem::take(self)
	}

	fn set_all(&mut self) {
		*self = Self::All;
	}

	fn contains(&self, id: &PeerId) -> bool {
		match self {
			Self::Some(set) => set.contains(id),
			Self::All => true,
		}
	}

	fn is_empty(&self) -> bool {
		match self {
			Self::Some(set) => set.is_empty(),
			Self::All => false,
		}
	}

	fn clear(&mut self) {
		std::mem::take(self);
	}
}

impl Default for AllowedRequests {
	fn default() -> Self {
		Self::Some(HashSet::default())
	}
}

struct GapSync<B: BlockT> {
	blocks: BlockCollection<B>,
	best_queued_number: NumberFor<B>,
	target: NumberFor<B>,
}

/// Action that the parent of [`ChainSync`] should perform after reporting a network or block event.
#[derive(Debug)]
pub enum ChainSyncAction<B: BlockT> {
	/// Send block request to peer. Always implies dropping a stale block request to the same peer.
	SendBlockRequest { peer_id: PeerId, request: BlockRequest<B> },
	/// Drop stale block request.
	CancelBlockRequest { peer_id: PeerId },
	/// Send state request to peer.
	SendStateRequest { peer_id: PeerId, request: OpaqueStateRequest },
	/// Send warp proof request to peer.
	SendWarpProofRequest { peer_id: PeerId, request: WarpProofRequest<B> },
	/// Peer misbehaved. Disconnect, report it and cancel the block request to it.
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

/// The main data structure which contains all the state for a chains
/// active syncing strategy.
pub struct ChainSync<B: BlockT, Client> {
	/// Chain client.
	client: Arc<Client>,
	/// The active peers that we are using to sync and their PeerSync status
	peers: HashMap<PeerId, PeerSync<B>>,
	/// A `BlockCollection` of blocks that are being downloaded from peers
	blocks: BlockCollection<B>,
	/// The best block number in our queue of blocks to import
	best_queued_number: NumberFor<B>,
	/// The best block hash in our queue of blocks to import
	best_queued_hash: B::Hash,
	/// Current mode (full/light)
	mode: SyncMode,
	/// Any extra justification requests.
	extra_justifications: ExtraRequests<B>,
	/// A set of hashes of blocks that are being downloaded or have been
	/// downloaded and are queued for import.
	queue_blocks: HashSet<B::Hash>,
	/// Fork sync targets.
	fork_targets: HashMap<B::Hash, ForkTarget<B>>,
	/// A set of peers for which there might be potential block requests
	allowed_requests: AllowedRequests,
	/// Maximum number of peers to ask the same blocks in parallel.
	max_parallel_downloads: u32,
	/// Maximum blocks per request.
	max_blocks_per_request: u32,
	/// Total number of downloaded blocks.
	downloaded_blocks: usize,
	/// State sync in progress, if any.
	state_sync: Option<StateSync<B, Client>>,
	/// Warp sync in progress, if any.
	warp_sync: Option<WarpSync<B, Client>>,
	/// Warp sync configuration.
	///
	/// Will be `None` after `self.warp_sync` is `Some(_)`.
	warp_sync_config: Option<WarpSyncConfig<B>>,
	/// A temporary storage for warp sync target block until warp sync is initialized.
	warp_sync_target_block_header: Option<B::Header>,
	/// Enable importing existing blocks. This is used used after the state download to
	/// catch up to the latest state while re-importing blocks.
	import_existing: bool,
	/// Gap download process.
	gap_sync: Option<GapSync<B>>,
	/// Pending actions.
	actions: Vec<ChainSyncAction<B>>,
}

/// All the data we have about a Peer that we are trying to sync with
#[derive(Debug, Clone)]
pub(crate) struct PeerSync<B: BlockT> {
	/// Peer id of this peer.
	pub peer_id: PeerId,
	/// The common number is the block number that is a common point of
	/// ancestry for both our chains (as far as we know).
	pub common_number: NumberFor<B>,
	/// The hash of the best block that we've seen for this peer.
	pub best_hash: B::Hash,
	/// The number of the best block that we've seen for this peer.
	pub best_number: NumberFor<B>,
	/// The state of syncing this peer is in for us, generally categories
	/// into `Available` or "busy" with something as defined by `PeerSyncState`.
	pub state: PeerSyncState<B>,
}

impl<B: BlockT> PeerSync<B> {
	/// Update the `common_number` iff `new_common > common_number`.
	fn update_common_number(&mut self, new_common: NumberFor<B>) {
		if self.common_number < new_common {
			trace!(
				target: LOG_TARGET,
				"Updating peer {} common number from={} => to={}.",
				self.peer_id,
				self.common_number,
				new_common,
			);
			self.common_number = new_common;
		}
	}
}

struct ForkTarget<B: BlockT> {
	number: NumberFor<B>,
	parent_hash: Option<B::Hash>,
	peers: HashSet<PeerId>,
}

/// The state of syncing between a Peer and ourselves.
///
/// Generally two categories, "busy" or `Available`. If busy, the enum
/// defines what we are busy with.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum PeerSyncState<B: BlockT> {
	/// Available for sync requests.
	Available,
	/// Searching for ancestors the Peer has in common with us.
	AncestorSearch { start: NumberFor<B>, current: NumberFor<B>, state: AncestorSearchState<B> },
	/// Actively downloading new blocks, starting from the given Number.
	DownloadingNew(NumberFor<B>),
	/// Downloading a stale block with given Hash. Stale means that it is a
	/// block with a number that is lower than our best number. It might be
	/// from a fork and not necessarily already imported.
	DownloadingStale(B::Hash),
	/// Downloading justification for given block hash.
	DownloadingJustification(B::Hash),
	/// Downloading state.
	DownloadingState,
	/// Downloading warp proof.
	DownloadingWarpProof,
	/// Downloading warp sync target block.
	DownloadingWarpTargetBlock,
	/// Actively downloading block history after warp sync.
	DownloadingGap(NumberFor<B>),
}

impl<B: BlockT> PeerSyncState<B> {
	pub fn is_available(&self) -> bool {
		matches!(self, Self::Available)
	}
}

impl<B, Client> ChainSync<B, Client>
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
	/// Create a new instance.
	pub fn new(
		mode: SyncMode,
		client: Arc<Client>,
		max_parallel_downloads: u32,
		max_blocks_per_request: u32,
		warp_sync_config: Option<WarpSyncConfig<B>>,
	) -> Result<Self, ClientError> {
		let mut sync = Self {
			client,
			peers: HashMap::new(),
			blocks: BlockCollection::new(),
			best_queued_hash: Default::default(),
			best_queued_number: Zero::zero(),
			extra_justifications: ExtraRequests::new("justification"),
			mode,
			queue_blocks: Default::default(),
			fork_targets: Default::default(),
			allowed_requests: Default::default(),
			max_parallel_downloads,
			max_blocks_per_request,
			downloaded_blocks: 0,
			state_sync: None,
			warp_sync: None,
			import_existing: false,
			gap_sync: None,
			warp_sync_config,
			warp_sync_target_block_header: None,
			actions: Vec::new(),
		};

		sync.reset_sync_start_point()?;
		Ok(sync)
	}

	/// Get peer's best hash & number.
	pub fn peer_info(&self, peer_id: &PeerId) -> Option<PeerInfo<B>> {
		self.peers
			.get(peer_id)
			.map(|p| PeerInfo { best_hash: p.best_hash, best_number: p.best_number })
	}

	/// Returns the current sync status.
	pub fn status(&self) -> SyncStatus<B> {
		let median_seen = self.median_seen();
		let best_seen_block =
			median_seen.and_then(|median| (median > self.best_queued_number).then_some(median));
		let sync_state = if let Some(target) = median_seen {
			// A chain is classified as downloading if the provided best block is
			// more than `MAJOR_SYNC_BLOCKS` behind the best block or as importing
			// if the same can be said about queued blocks.
			let best_block = self.client.info().best_number;
			if target > best_block && target - best_block > MAJOR_SYNC_BLOCKS.into() {
				// If target is not queued, we're downloading, otherwise importing.
				if target > self.best_queued_number {
					SyncState::Downloading { target }
				} else {
					SyncState::Importing { target }
				}
			} else {
				SyncState::Idle
			}
		} else {
			SyncState::Idle
		};

		let warp_sync_progress = match (&self.warp_sync, &self.mode, &self.gap_sync) {
			(_, _, Some(gap_sync)) => Some(WarpSyncProgress {
				phase: WarpSyncPhase::DownloadingBlocks(gap_sync.best_queued_number),
				total_bytes: 0,
			}),
			(None, SyncMode::Warp, _) => Some(WarpSyncProgress {
				phase: WarpSyncPhase::AwaitingPeers {
					required_peers: MIN_PEERS_TO_START_WARP_SYNC,
				},
				total_bytes: 0,
			}),
			(Some(sync), _, _) => Some(sync.progress()),
			_ => None,
		};

		SyncStatus {
			state: sync_state,
			best_seen_block,
			num_peers: self.peers.len() as u32,
			num_connected_peers: 0u32,
			queued_blocks: self.queue_blocks.len() as u32,
			state_sync: self.state_sync.as_ref().map(|s| s.progress()),
			warp_sync: warp_sync_progress,
		}
	}

	/// Get an estimate of the number of parallel sync requests.
	pub fn num_sync_requests(&self) -> usize {
		self.fork_targets
			.values()
			.filter(|f| f.number <= self.best_queued_number)
			.count()
	}

	/// Get the total number of downloaded blocks.
	pub fn num_downloaded_blocks(&self) -> usize {
		self.downloaded_blocks
	}

	/// Get the number of peers known to the syncing state machine.
	pub fn num_peers(&self) -> usize {
		self.peers.len()
	}

	/// Notify syncing state machine that a new sync peer has connected.
	pub fn new_peer(&mut self, peer_id: PeerId, best_hash: B::Hash, best_number: NumberFor<B>) {
		match self.new_peer_inner(peer_id, best_hash, best_number) {
			Ok(Some(request)) =>
				self.actions.push(ChainSyncAction::SendBlockRequest { peer_id, request }),
			Ok(None) => {},
			Err(bad_peer) => self.actions.push(ChainSyncAction::DropPeer(bad_peer)),
		}
	}

	#[must_use]
	fn new_peer_inner(
		&mut self,
		peer_id: PeerId,
		best_hash: B::Hash,
		best_number: NumberFor<B>,
	) -> Result<Option<BlockRequest<B>>, BadPeer> {
		// There is nothing sync can get from the node that has no blockchain data.
		match self.block_status(&best_hash) {
			Err(e) => {
				debug!(target:LOG_TARGET, "Error reading blockchain: {e}");
				Err(BadPeer(peer_id, rep::BLOCKCHAIN_READ_ERROR))
			},
			Ok(BlockStatus::KnownBad) => {
				info!(
					"💔 New peer {peer_id} with known bad best block {best_hash} ({best_number})."
				);
				Err(BadPeer(peer_id, rep::BAD_BLOCK))
			},
			Ok(BlockStatus::Unknown) => {
				if best_number.is_zero() {
					info!(
						"💔 New peer {} with unknown genesis hash {} ({}).",
						peer_id, best_hash, best_number,
					);
					return Err(BadPeer(peer_id, rep::GENESIS_MISMATCH))
				}

				// If there are more than `MAJOR_SYNC_BLOCKS` in the import queue then we have
				// enough to do in the import queue that it's not worth kicking off
				// an ancestor search, which is what we do in the next match case below.
				if self.queue_blocks.len() > MAJOR_SYNC_BLOCKS.into() {
					debug!(
						target:LOG_TARGET,
						"New peer {} with unknown best hash {} ({}), assuming common block.",
						peer_id,
						self.best_queued_hash,
						self.best_queued_number
					);
					self.peers.insert(
						peer_id,
						PeerSync {
							peer_id,
							common_number: self.best_queued_number,
							best_hash,
							best_number,
							state: PeerSyncState::Available,
						},
					);
					return Ok(None)
				}

				// If we are at genesis, just start downloading.
				let (state, req) = if self.best_queued_number.is_zero() {
					debug!(
						target:LOG_TARGET,
						"New peer {peer_id} with best hash {best_hash} ({best_number}).",
					);

					(PeerSyncState::Available, None)
				} else {
					let common_best = std::cmp::min(self.best_queued_number, best_number);

					debug!(
						target:LOG_TARGET,
						"New peer {} with unknown best hash {} ({}), searching for common ancestor.",
						peer_id,
						best_hash,
						best_number
					);

					(
						PeerSyncState::AncestorSearch {
							current: common_best,
							start: self.best_queued_number,
							state: AncestorSearchState::ExponentialBackoff(One::one()),
						},
						Some(ancestry_request::<B>(common_best)),
					)
				};

				self.allowed_requests.add(&peer_id);
				self.peers.insert(
					peer_id,
					PeerSync {
						peer_id,
						common_number: Zero::zero(),
						best_hash,
						best_number,
						state,
					},
				);

				if let SyncMode::Warp = self.mode {
					if self.peers.len() >= MIN_PEERS_TO_START_WARP_SYNC && self.warp_sync.is_none()
					{
						log::debug!(target: LOG_TARGET, "Starting warp state sync.");

						if let Some(config) = self.warp_sync_config.take() {
							let mut warp_sync = WarpSync::new(self.client.clone(), config);
							if let Some(header) = self.warp_sync_target_block_header.take() {
								warp_sync.set_target_block(header);
							}
							self.warp_sync = Some(warp_sync);
						}
					}
				}
				Ok(req)
			},
			Ok(BlockStatus::Queued) |
			Ok(BlockStatus::InChainWithState) |
			Ok(BlockStatus::InChainPruned) => {
				debug!(
					target: LOG_TARGET,
					"New peer {peer_id} with known best hash {best_hash} ({best_number}).",
				);
				self.peers.insert(
					peer_id,
					PeerSync {
						peer_id,
						common_number: std::cmp::min(self.best_queued_number, best_number),
						best_hash,
						best_number,
						state: PeerSyncState::Available,
					},
				);
				self.allowed_requests.add(&peer_id);
				Ok(None)
			},
		}
	}

	/// Inform sync about a new best imported block.
	pub fn update_chain_info(&mut self, best_hash: &B::Hash, best_number: NumberFor<B>) {
		self.on_block_queued(best_hash, best_number);
	}

	/// Request extra justification.
	pub fn request_justification(&mut self, hash: &B::Hash, number: NumberFor<B>) {
		let client = &self.client;
		self.extra_justifications
			.schedule((*hash, number), |base, block| is_descendent_of(&**client, base, block))
	}

	/// Clear extra justification requests.
	pub fn clear_justification_requests(&mut self) {
		self.extra_justifications.reset();
	}

	/// Configure an explicit fork sync request in case external code has detected that there is a
	/// stale fork missing.
	///
	/// Note that this function should not be used for recent blocks.
	/// Sync should be able to download all the recent forks normally.
	///
	/// Passing empty `peers` set effectively removes the sync request.
	// The implementation is similar to `on_validated_block_announce` with unknown parent hash.
	pub fn set_sync_fork_request(
		&mut self,
		mut peers: Vec<PeerId>,
		hash: &B::Hash,
		number: NumberFor<B>,
	) {
		if peers.is_empty() {
			peers = self
				.peers
				.iter()
				// Only request blocks from peers who are ahead or on a par.
				.filter(|(_, peer)| peer.best_number >= number)
				.map(|(id, _)| *id)
				.collect();

			debug!(
				target: LOG_TARGET,
				"Explicit sync request for block {hash:?} with no peers specified. \
				Syncing from these peers {peers:?} instead.",
			);
		} else {
			debug!(
				target: LOG_TARGET,
				"Explicit sync request for block {hash:?} with {peers:?}",
			);
		}

		if self.is_known(hash) {
			debug!(target: LOG_TARGET, "Refusing to sync known hash {hash:?}");
			return
		}

		trace!(target: LOG_TARGET, "Downloading requested old fork {hash:?}");
		for peer_id in &peers {
			if let Some(peer) = self.peers.get_mut(peer_id) {
				if let PeerSyncState::AncestorSearch { .. } = peer.state {
					continue
				}

				if number > peer.best_number {
					peer.best_number = number;
					peer.best_hash = *hash;
				}
				self.allowed_requests.add(peer_id);
			}
		}

		self.fork_targets
			.entry(*hash)
			.or_insert_with(|| ForkTarget { number, peers: Default::default(), parent_hash: None })
			.peers
			.extend(peers);
	}

	/// Submit a block response for processing.
	#[must_use]
	fn on_block_data(
		&mut self,
		peer_id: &PeerId,
		request: Option<BlockRequest<B>>,
		response: BlockResponse<B>,
	) -> Result<(), BadPeer> {
		self.downloaded_blocks += response.blocks.len();
		let mut gap = false;
		let new_blocks: Vec<IncomingBlock<B>> = if let Some(peer) = self.peers.get_mut(peer_id) {
			let mut blocks = response.blocks;
			if request.as_ref().map_or(false, |r| r.direction == Direction::Descending) {
				trace!(target: LOG_TARGET, "Reversing incoming block list");
				blocks.reverse()
			}
			self.allowed_requests.add(peer_id);
			if let Some(request) = request {
				match &mut peer.state {
					PeerSyncState::DownloadingNew(_) => {
						self.blocks.clear_peer_download(peer_id);
						peer.state = PeerSyncState::Available;
						if let Some(start_block) =
							validate_blocks::<B>(&blocks, peer_id, Some(request))?
						{
							self.blocks.insert(start_block, blocks, *peer_id);
						}
						self.ready_blocks()
					},
					PeerSyncState::DownloadingGap(_) => {
						peer.state = PeerSyncState::Available;
						if let Some(gap_sync) = &mut self.gap_sync {
							gap_sync.blocks.clear_peer_download(peer_id);
							if let Some(start_block) =
								validate_blocks::<B>(&blocks, peer_id, Some(request))?
							{
								gap_sync.blocks.insert(start_block, blocks, *peer_id);
							}
							gap = true;
							let blocks: Vec<_> = gap_sync
								.blocks
								.ready_blocks(gap_sync.best_queued_number + One::one())
								.into_iter()
								.map(|block_data| {
									let justifications =
										block_data.block.justifications.or_else(|| {
											legacy_justification_mapping(
												block_data.block.justification,
											)
										});
									IncomingBlock {
										hash: block_data.block.hash,
										header: block_data.block.header,
										body: block_data.block.body,
										indexed_body: block_data.block.indexed_body,
										justifications,
										origin: block_data.origin,
										allow_missing_state: true,
										import_existing: self.import_existing,
										skip_execution: true,
										state: None,
									}
								})
								.collect();
							debug!(
								target: LOG_TARGET,
								"Drained {} gap blocks from {}",
								blocks.len(),
								gap_sync.best_queued_number,
							);
							blocks
						} else {
							debug!(target: LOG_TARGET, "Unexpected gap block response from {peer_id}");
							return Err(BadPeer(*peer_id, rep::NO_BLOCK))
						}
					},
					PeerSyncState::DownloadingStale(_) => {
						peer.state = PeerSyncState::Available;
						if blocks.is_empty() {
							debug!(target: LOG_TARGET, "Empty block response from {peer_id}");
							return Err(BadPeer(*peer_id, rep::NO_BLOCK))
						}
						validate_blocks::<B>(&blocks, peer_id, Some(request))?;
						blocks
							.into_iter()
							.map(|b| {
								let justifications = b
									.justifications
									.or_else(|| legacy_justification_mapping(b.justification));
								IncomingBlock {
									hash: b.hash,
									header: b.header,
									body: b.body,
									indexed_body: None,
									justifications,
									origin: Some(*peer_id),
									allow_missing_state: true,
									import_existing: self.import_existing,
									skip_execution: self.skip_execution(),
									state: None,
								}
							})
							.collect()
					},
					PeerSyncState::AncestorSearch { current, start, state } => {
						let matching_hash = match (blocks.get(0), self.client.hash(*current)) {
							(Some(block), Ok(maybe_our_block_hash)) => {
								trace!(
									target: LOG_TARGET,
									"Got ancestry block #{} ({}) from peer {}",
									current,
									block.hash,
									peer_id,
								);
								maybe_our_block_hash.filter(|x| x == &block.hash)
							},
							(None, _) => {
								debug!(
									target: LOG_TARGET,
									"Invalid response when searching for ancestor from {peer_id}",
								);
								return Err(BadPeer(*peer_id, rep::UNKNOWN_ANCESTOR))
							},
							(_, Err(e)) => {
								info!(
									target: LOG_TARGET,
									"❌ Error answering legitimate blockchain query: {e}",
								);
								return Err(BadPeer(*peer_id, rep::BLOCKCHAIN_READ_ERROR))
							},
						};
						if matching_hash.is_some() {
							if *start < self.best_queued_number &&
								self.best_queued_number <= peer.best_number
							{
								// We've made progress on this chain since the search was started.
								// Opportunistically set common number to updated number
								// instead of the one that started the search.
								trace!(
									target: LOG_TARGET,
									"Ancestry search: opportunistically updating peer {} common number from={} => to={}.",
									*peer_id,
									peer.common_number,
									self.best_queued_number,
								);
								peer.common_number = self.best_queued_number;
							} else if peer.common_number < *current {
								trace!(
									target: LOG_TARGET,
									"Ancestry search: updating peer {} common number from={} => to={}.",
									*peer_id,
									peer.common_number,
									*current,
								);
								peer.common_number = *current;
							}
						}
						if matching_hash.is_none() && current.is_zero() {
							trace!(
								target:LOG_TARGET,
								"Ancestry search: genesis mismatch for peer {peer_id}",
							);
							return Err(BadPeer(*peer_id, rep::GENESIS_MISMATCH))
						}
						if let Some((next_state, next_num)) =
							handle_ancestor_search_state(state, *current, matching_hash.is_some())
						{
							peer.state = PeerSyncState::AncestorSearch {
								current: next_num,
								start: *start,
								state: next_state,
							};
							let request = ancestry_request::<B>(next_num);
							self.actions.push(ChainSyncAction::SendBlockRequest {
								peer_id: *peer_id,
								request,
							});
							return Ok(())
						} else {
							// Ancestry search is complete. Check if peer is on a stale fork unknown
							// to us and add it to sync targets if necessary.
							trace!(
								target: LOG_TARGET,
								"Ancestry search complete. Ours={} ({}), Theirs={} ({}), Common={:?} ({})",
								self.best_queued_hash,
								self.best_queued_number,
								peer.best_hash,
								peer.best_number,
								matching_hash,
								peer.common_number,
							);
							if peer.common_number < peer.best_number &&
								peer.best_number < self.best_queued_number
							{
								trace!(
									target: LOG_TARGET,
									"Added fork target {} for {}",
									peer.best_hash,
									peer_id,
								);
								self.fork_targets
									.entry(peer.best_hash)
									.or_insert_with(|| ForkTarget {
										number: peer.best_number,
										parent_hash: None,
										peers: Default::default(),
									})
									.peers
									.insert(*peer_id);
							}
							peer.state = PeerSyncState::Available;
							return Ok(())
						}
					},
					PeerSyncState::DownloadingWarpTargetBlock => {
						peer.state = PeerSyncState::Available;
						if let Some(warp_sync) = &mut self.warp_sync {
							if blocks.len() == 1 {
								validate_blocks::<B>(&blocks, peer_id, Some(request))?;
								match warp_sync.import_target_block(
									blocks.pop().expect("`blocks` len checked above."),
								) {
									warp::TargetBlockImportResult::Success => return Ok(()),
									warp::TargetBlockImportResult::BadResponse =>
										return Err(BadPeer(*peer_id, rep::VERIFICATION_FAIL)),
								}
							} else if blocks.is_empty() {
								debug!(target: LOG_TARGET, "Empty block response from {peer_id}");
								return Err(BadPeer(*peer_id, rep::NO_BLOCK))
							} else {
								debug!(
									target: LOG_TARGET,
									"Too many blocks ({}) in warp target block response from {}",
									blocks.len(),
									peer_id,
								);
								return Err(BadPeer(*peer_id, rep::NOT_REQUESTED))
							}
						} else {
							debug!(
								target: LOG_TARGET,
								"Logic error: we think we are downloading warp target block from {}, but no warp sync is happening.",
								peer_id,
							);
							return Ok(())
						}
					},
					PeerSyncState::Available |
					PeerSyncState::DownloadingJustification(..) |
					PeerSyncState::DownloadingState |
					PeerSyncState::DownloadingWarpProof => Vec::new(),
				}
			} else {
				// When request.is_none() this is a block announcement. Just accept blocks.
				validate_blocks::<B>(&blocks, peer_id, None)?;
				blocks
					.into_iter()
					.map(|b| {
						let justifications = b
							.justifications
							.or_else(|| legacy_justification_mapping(b.justification));
						IncomingBlock {
							hash: b.hash,
							header: b.header,
							body: b.body,
							indexed_body: None,
							justifications,
							origin: Some(*peer_id),
							allow_missing_state: true,
							import_existing: false,
							skip_execution: true,
							state: None,
						}
					})
					.collect()
			}
		} else {
			// We don't know of this peer, so we also did not request anything from it.
			return Err(BadPeer(*peer_id, rep::NOT_REQUESTED))
		};

		self.validate_and_queue_blocks(new_blocks, gap);

		Ok(())
	}

	/// Submit a justification response for processing.
	#[must_use]
	fn on_block_justification(
		&mut self,
		peer_id: PeerId,
		response: BlockResponse<B>,
	) -> Result<(), BadPeer> {
		let peer = if let Some(peer) = self.peers.get_mut(&peer_id) {
			peer
		} else {
			error!(
				target: LOG_TARGET,
				"💔 Called on_block_justification with a peer ID of an unknown peer",
			);
			return Ok(())
		};

		self.allowed_requests.add(&peer_id);
		if let PeerSyncState::DownloadingJustification(hash) = peer.state {
			peer.state = PeerSyncState::Available;

			// We only request one justification at a time
			let justification = if let Some(block) = response.blocks.into_iter().next() {
				if hash != block.hash {
					warn!(
						target: LOG_TARGET,
						"💔 Invalid block justification provided by {}: requested: {:?} got: {:?}",
						peer_id,
						hash,
						block.hash,
					);
					return Err(BadPeer(peer_id, rep::BAD_JUSTIFICATION))
				}

				block
					.justifications
					.or_else(|| legacy_justification_mapping(block.justification))
			} else {
				// we might have asked the peer for a justification on a block that we assumed it
				// had but didn't (regardless of whether it had a justification for it or not).
				trace!(
					target: LOG_TARGET,
					"Peer {peer_id:?} provided empty response for justification request {hash:?}",
				);

				None
			};

			if let Some((peer_id, hash, number, justifications)) =
				self.extra_justifications.on_response(peer_id, justification)
			{
				self.actions.push(ChainSyncAction::ImportJustifications {
					peer_id,
					hash,
					number,
					justifications,
				});
				return Ok(())
			}
		}

		Ok(())
	}

	/// Report a justification import (successful or not).
	pub fn on_justification_import(&mut self, hash: B::Hash, number: NumberFor<B>, success: bool) {
		let finalization_result = if success { Ok((hash, number)) } else { Err(()) };
		self.extra_justifications
			.try_finalize_root((hash, number), finalization_result, true);
		self.allowed_requests.set_all();
	}

	/// Notify sync that a block has been finalized.
	pub fn on_block_finalized(&mut self, hash: &B::Hash, number: NumberFor<B>) {
		let client = &self.client;
		let r = self.extra_justifications.on_block_finalized(hash, number, |base, block| {
			is_descendent_of(&**client, base, block)
		});

		if let SyncMode::LightState { skip_proofs, .. } = &self.mode {
			if self.state_sync.is_none() && !self.peers.is_empty() && self.queue_blocks.is_empty() {
				// Finalized a recent block.
				let mut heads: Vec<_> = self.peers.values().map(|peer| peer.best_number).collect();
				heads.sort();
				let median = heads[heads.len() / 2];
				if number + STATE_SYNC_FINALITY_THRESHOLD.saturated_into() >= median {
					if let Ok(Some(header)) = self.client.header(*hash) {
						log::debug!(
							target: LOG_TARGET,
							"Starting state sync for #{number} ({hash})",
						);
						self.state_sync = Some(StateSync::new(
							self.client.clone(),
							header,
							None,
							None,
							*skip_proofs,
						));
						self.allowed_requests.set_all();
					}
				}
			}
		}

		if let Err(err) = r {
			warn!(
				target: LOG_TARGET,
				"💔 Error cleaning up pending extra justification data requests: {err}",
			);
		}
	}

	/// Submit a validated block announcement.
	pub fn on_validated_block_announce(
		&mut self,
		is_best: bool,
		peer_id: PeerId,
		announce: &BlockAnnounce<B::Header>,
	) {
		let number = *announce.header.number();
		let hash = announce.header.hash();
		let parent_status =
			self.block_status(announce.header.parent_hash()).unwrap_or(BlockStatus::Unknown);
		let known_parent = parent_status != BlockStatus::Unknown;
		let ancient_parent = parent_status == BlockStatus::InChainPruned;

		let known = self.is_known(&hash);
		let peer = if let Some(peer) = self.peers.get_mut(&peer_id) {
			peer
		} else {
			error!(target: LOG_TARGET, "💔 Called `on_validated_block_announce` with a bad peer ID");
			return
		};

		if let PeerSyncState::AncestorSearch { .. } = peer.state {
			trace!(target: LOG_TARGET, "Peer {} is in the ancestor search state.", peer_id);
			return
		}

		if is_best {
			// update their best block
			peer.best_number = number;
			peer.best_hash = hash;
		}

		// If the announced block is the best they have and is not ahead of us, our common number
		// is either one further ahead or it's the one they just announced, if we know about it.
		if is_best {
			if known && self.best_queued_number >= number {
				self.update_peer_common_number(&peer_id, number);
			} else if announce.header.parent_hash() == &self.best_queued_hash ||
				known_parent && self.best_queued_number >= number
			{
				self.update_peer_common_number(&peer_id, number.saturating_sub(One::one()));
			}
		}
		self.allowed_requests.add(&peer_id);

		// known block case
		if known || self.is_already_downloading(&hash) {
			trace!(target: "sync", "Known block announce from {}: {}", peer_id, hash);
			if let Some(target) = self.fork_targets.get_mut(&hash) {
				target.peers.insert(peer_id);
			}
			return
		}

		if ancient_parent {
			trace!(
				target: "sync",
				"Ignored ancient block announced from {}: {} {:?}",
				peer_id,
				hash,
				announce.header,
			);
			return
		}

		if self.status().state == SyncState::Idle {
			trace!(
				target: "sync",
				"Added sync target for block announced from {}: {} {:?}",
				peer_id,
				hash,
				announce.summary(),
			);
			self.fork_targets
				.entry(hash)
				.or_insert_with(|| ForkTarget {
					number,
					parent_hash: Some(*announce.header.parent_hash()),
					peers: Default::default(),
				})
				.peers
				.insert(peer_id);
		}
	}

	/// Notify that a sync peer has disconnected.
	pub fn peer_disconnected(&mut self, peer_id: &PeerId) {
		self.blocks.clear_peer_download(peer_id);
		if let Some(gap_sync) = &mut self.gap_sync {
			gap_sync.blocks.clear_peer_download(peer_id)
		}
		self.peers.remove(peer_id);
		self.extra_justifications.peer_disconnected(peer_id);
		self.allowed_requests.set_all();
		self.fork_targets.retain(|_, target| {
			target.peers.remove(peer_id);
			!target.peers.is_empty()
		});

		let blocks = self.ready_blocks();

		if !blocks.is_empty() {
			self.validate_and_queue_blocks(blocks, false);
		}
	}

	/// Get prometheus metrics.
	pub fn metrics(&self) -> Metrics {
		Metrics {
			queued_blocks: self.queue_blocks.len().try_into().unwrap_or(std::u32::MAX),
			fork_targets: self.fork_targets.len().try_into().unwrap_or(std::u32::MAX),
			justifications: self.extra_justifications.metrics(),
		}
	}

	/// Returns the median seen block number.
	fn median_seen(&self) -> Option<NumberFor<B>> {
		let mut best_seens = self.peers.values().map(|p| p.best_number).collect::<Vec<_>>();

		if best_seens.is_empty() {
			None
		} else {
			let middle = best_seens.len() / 2;

			// Not the "perfect median" when we have an even number of peers.
			Some(*best_seens.select_nth_unstable(middle).1)
		}
	}

	fn required_block_attributes(&self) -> BlockAttributes {
		match self.mode {
			SyncMode::Full =>
				BlockAttributes::HEADER | BlockAttributes::JUSTIFICATION | BlockAttributes::BODY,
			SyncMode::LightState { storage_chain_mode: false, .. } | SyncMode::Warp =>
				BlockAttributes::HEADER | BlockAttributes::JUSTIFICATION | BlockAttributes::BODY,
			SyncMode::LightState { storage_chain_mode: true, .. } =>
				BlockAttributes::HEADER |
					BlockAttributes::JUSTIFICATION |
					BlockAttributes::INDEXED_BODY,
		}
	}

	fn skip_execution(&self) -> bool {
		match self.mode {
			SyncMode::Full => false,
			SyncMode::LightState { .. } => true,
			SyncMode::Warp => true,
		}
	}

	fn validate_and_queue_blocks(&mut self, mut new_blocks: Vec<IncomingBlock<B>>, gap: bool) {
		let orig_len = new_blocks.len();
		new_blocks.retain(|b| !self.queue_blocks.contains(&b.hash));
		if new_blocks.len() != orig_len {
			debug!(
				target: LOG_TARGET,
				"Ignoring {} blocks that are already queued",
				orig_len - new_blocks.len(),
			);
		}

		let origin = if !gap && !self.status().state.is_major_syncing() {
			BlockOrigin::NetworkBroadcast
		} else {
			BlockOrigin::NetworkInitialSync
		};

		if let Some((h, n)) = new_blocks
			.last()
			.and_then(|b| b.header.as_ref().map(|h| (&b.hash, *h.number())))
		{
			trace!(
				target:LOG_TARGET,
				"Accepted {} blocks ({:?}) with origin {:?}",
				new_blocks.len(),
				h,
				origin,
			);
			self.on_block_queued(h, n)
		}
		self.queue_blocks.extend(new_blocks.iter().map(|b| b.hash));

		self.actions.push(ChainSyncAction::ImportBlocks { origin, blocks: new_blocks })
	}

	fn update_peer_common_number(&mut self, peer_id: &PeerId, new_common: NumberFor<B>) {
		if let Some(peer) = self.peers.get_mut(peer_id) {
			peer.update_common_number(new_common);
		}
	}

	/// Called when a block has been queued for import.
	///
	/// Updates our internal state for best queued block and then goes
	/// through all peers to update our view of their state as well.
	fn on_block_queued(&mut self, hash: &B::Hash, number: NumberFor<B>) {
		if self.fork_targets.remove(hash).is_some() {
			trace!(target: LOG_TARGET, "Completed fork sync {hash:?}");
		}
		if let Some(gap_sync) = &mut self.gap_sync {
			if number > gap_sync.best_queued_number && number <= gap_sync.target {
				gap_sync.best_queued_number = number;
			}
		}
		if number > self.best_queued_number {
			self.best_queued_number = number;
			self.best_queued_hash = *hash;
			// Update common blocks
			for (n, peer) in self.peers.iter_mut() {
				if let PeerSyncState::AncestorSearch { .. } = peer.state {
					// Wait for ancestry search to complete first.
					continue
				}
				let new_common_number =
					if peer.best_number >= number { number } else { peer.best_number };
				trace!(
					target: LOG_TARGET,
					"Updating peer {} info, ours={}, common={}->{}, their best={}",
					n,
					number,
					peer.common_number,
					new_common_number,
					peer.best_number,
				);
				peer.common_number = new_common_number;
			}
		}
		self.allowed_requests.set_all();
	}

	/// Restart the sync process. This will reset all pending block requests and return an iterator
	/// of new block requests to make to peers. Peers that were downloading finality data (i.e.
	/// their state was `DownloadingJustification`) are unaffected and will stay in the same state.
	fn restart(&mut self) {
		self.blocks.clear();
		if let Err(e) = self.reset_sync_start_point() {
			warn!(target: LOG_TARGET, "💔  Unable to restart sync: {e}");
		}
		self.allowed_requests.set_all();
		debug!(
			target: LOG_TARGET,
			"Restarted with {} ({})",
			self.best_queued_number,
			self.best_queued_hash,
		);
		let old_peers = std::mem::take(&mut self.peers);

		old_peers.into_iter().for_each(|(peer_id, mut p)| {
			// peers that were downloading justifications
			// should be kept in that state.
			if let PeerSyncState::DownloadingJustification(_) = p.state {
				// We make sure our commmon number is at least something we have.
				trace!(
					target: LOG_TARGET,
					"Keeping peer {} after restart, updating common number from={} => to={} (our best).",
					peer_id,
					p.common_number,
					self.best_queued_number,
				);
				p.common_number = self.best_queued_number;
				self.peers.insert(peer_id, p);
				return
			}

			// handle peers that were in other states.
			let action = match self.new_peer_inner(peer_id, p.best_hash, p.best_number) {
				// since the request is not a justification, remove it from pending responses
				Ok(None) => ChainSyncAction::CancelBlockRequest { peer_id },
				// update the request if the new one is available
				Ok(Some(request)) => ChainSyncAction::SendBlockRequest { peer_id, request },
				// this implies that we need to drop pending response from the peer
				Err(bad_peer) => ChainSyncAction::DropPeer(bad_peer),
			};

			self.actions.push(action);
		});
	}

	/// Find a block to start sync from. If we sync with state, that's the latest block we have
	/// state for.
	fn reset_sync_start_point(&mut self) -> Result<(), ClientError> {
		let info = self.client.info();
		if matches!(self.mode, SyncMode::LightState { .. }) && info.finalized_state.is_some() {
			warn!(
				target: LOG_TARGET,
				"Can't use fast sync mode with a partially synced database. Reverting to full sync mode."
			);
			self.mode = SyncMode::Full;
		}
		if matches!(self.mode, SyncMode::Warp) && info.finalized_state.is_some() {
			warn!(
				target: LOG_TARGET,
				"Can't use warp sync mode with a partially synced database. Reverting to full sync mode."
			);
			self.mode = SyncMode::Full;
		}
		self.import_existing = false;
		self.best_queued_hash = info.best_hash;
		self.best_queued_number = info.best_number;

		if self.mode == SyncMode::Full &&
			self.client.block_status(info.best_hash)? != BlockStatus::InChainWithState
		{
			self.import_existing = true;
			// Latest state is missing, start with the last finalized state or genesis instead.
			if let Some((hash, number)) = info.finalized_state {
				debug!(target: LOG_TARGET, "Starting from finalized state #{number}");
				self.best_queued_hash = hash;
				self.best_queued_number = number;
			} else {
				debug!(target: LOG_TARGET, "Restarting from genesis");
				self.best_queued_hash = Default::default();
				self.best_queued_number = Zero::zero();
			}
		}

		if let Some((start, end)) = info.block_gap {
			debug!(target: LOG_TARGET, "Starting gap sync #{start} - #{end}");
			self.gap_sync = Some(GapSync {
				best_queued_number: start - One::one(),
				target: end,
				blocks: BlockCollection::new(),
			});
		}
		trace!(
			target: LOG_TARGET,
			"Restarted sync at #{} ({:?})",
			self.best_queued_number,
			self.best_queued_hash,
		);
		Ok(())
	}

	/// What is the status of the block corresponding to the given hash?
	fn block_status(&self, hash: &B::Hash) -> Result<BlockStatus, ClientError> {
		if self.queue_blocks.contains(hash) {
			return Ok(BlockStatus::Queued)
		}
		self.client.block_status(*hash)
	}

	/// Is the block corresponding to the given hash known?
	fn is_known(&self, hash: &B::Hash) -> bool {
		self.block_status(hash).ok().map_or(false, |s| s != BlockStatus::Unknown)
	}

	/// Is any peer downloading the given hash?
	fn is_already_downloading(&self, hash: &B::Hash) -> bool {
		self.peers
			.iter()
			.any(|(_, p)| p.state == PeerSyncState::DownloadingStale(*hash))
	}

	/// Get the set of downloaded blocks that are ready to be queued for import.
	fn ready_blocks(&mut self) -> Vec<IncomingBlock<B>> {
		self.blocks
			.ready_blocks(self.best_queued_number + One::one())
			.into_iter()
			.map(|block_data| {
				let justifications = block_data
					.block
					.justifications
					.or_else(|| legacy_justification_mapping(block_data.block.justification));
				IncomingBlock {
					hash: block_data.block.hash,
					header: block_data.block.header,
					body: block_data.block.body,
					indexed_body: block_data.block.indexed_body,
					justifications,
					origin: block_data.origin,
					allow_missing_state: true,
					import_existing: self.import_existing,
					skip_execution: self.skip_execution(),
					state: None,
				}
			})
			.collect()
	}

	/// Set the warp sync target block externally in case we skip warp proofs downloading.
	pub fn set_warp_sync_target_block(&mut self, header: B::Header) {
		if let Some(ref mut warp_sync) = self.warp_sync {
			warp_sync.set_target_block(header);
		} else {
			self.warp_sync_target_block_header = Some(header);
		}
	}

	/// Generate block request for downloading of the target block body during warp sync.
	fn warp_target_block_request(&mut self) -> Option<(PeerId, BlockRequest<B>)> {
		let sync = &self.warp_sync.as_ref()?;

		if self.allowed_requests.is_empty() ||
			sync.is_complete() ||
			self.peers
				.iter()
				.any(|(_, peer)| peer.state == PeerSyncState::DownloadingWarpTargetBlock)
		{
			// Only one pending warp target block request is allowed.
			return None
		}

		if let Some((target_number, request)) = sync.next_target_block_request() {
			// Find a random peer that has a block with the target number.
			for (id, peer) in self.peers.iter_mut() {
				if peer.state.is_available() && peer.best_number >= target_number {
					trace!(target: LOG_TARGET, "New warp target block request for {id}");
					peer.state = PeerSyncState::DownloadingWarpTargetBlock;
					self.allowed_requests.clear();
					return Some((*id, request))
				}
			}
		}

		None
	}

	/// Submit blocks received in a response.
	pub fn on_block_response(
		&mut self,
		peer_id: PeerId,
		request: BlockRequest<B>,
		blocks: Vec<BlockData<B>>,
	) {
		let block_response = BlockResponse::<B> { id: request.id, blocks };

		let blocks_range = || match (
			block_response
				.blocks
				.first()
				.and_then(|b| b.header.as_ref().map(|h| h.number())),
			block_response.blocks.last().and_then(|b| b.header.as_ref().map(|h| h.number())),
		) {
			(Some(first), Some(last)) if first != last => format!(" ({}..{})", first, last),
			(Some(first), Some(_)) => format!(" ({})", first),
			_ => Default::default(),
		};
		trace!(
			target: LOG_TARGET,
			"BlockResponse {} from {} with {} blocks {}",
			block_response.id,
			peer_id,
			block_response.blocks.len(),
			blocks_range(),
		);

		let res = if request.fields == BlockAttributes::JUSTIFICATION {
			self.on_block_justification(peer_id, block_response)
		} else {
			self.on_block_data(&peer_id, Some(request), block_response)
		};

		if let Err(bad_peer) = res {
			self.actions.push(ChainSyncAction::DropPeer(bad_peer));
		}
	}

	/// Submit a state received in a response.
	pub fn on_state_response(&mut self, peer_id: PeerId, response: OpaqueStateResponse) {
		if let Err(bad_peer) = self.on_state_data(&peer_id, response) {
			self.actions.push(ChainSyncAction::DropPeer(bad_peer));
		}
	}

	/// Get justification requests scheduled by sync to be sent out.
	fn justification_requests(&mut self) -> Vec<(PeerId, BlockRequest<B>)> {
		let peers = &mut self.peers;
		let mut matcher = self.extra_justifications.matcher();
		std::iter::from_fn(move || {
			if let Some((peer, request)) = matcher.next(peers) {
				peers
					.get_mut(&peer)
					.expect(
						"`Matcher::next` guarantees the `PeerId` comes from the given peers; qed",
					)
					.state = PeerSyncState::DownloadingJustification(request.0);
				let req = BlockRequest::<B> {
					id: 0,
					fields: BlockAttributes::JUSTIFICATION,
					from: FromBlock::Hash(request.0),
					direction: Direction::Ascending,
					max: Some(1),
				};
				Some((peer, req))
			} else {
				None
			}
		})
		.collect()
	}

	/// Get block requests scheduled by sync to be sent out.
	fn block_requests(&mut self) -> Vec<(PeerId, BlockRequest<B>)> {
		if self.mode == SyncMode::Warp {
			return self
				.warp_target_block_request()
				.map_or_else(|| Vec::new(), |req| Vec::from([req]))
		}

		if self.allowed_requests.is_empty() || self.state_sync.is_some() {
			return Vec::new()
		}

		if self.queue_blocks.len() > MAX_IMPORTING_BLOCKS {
			trace!(target: LOG_TARGET, "Too many blocks in the queue.");
			return Vec::new()
		}
		let is_major_syncing = self.status().state.is_major_syncing();
		let attrs = self.required_block_attributes();
		let blocks = &mut self.blocks;
		let fork_targets = &mut self.fork_targets;
		let last_finalized =
			std::cmp::min(self.best_queued_number, self.client.info().finalized_number);
		let best_queued = self.best_queued_number;
		let client = &self.client;
		let queue = &self.queue_blocks;
		let allowed_requests = self.allowed_requests.take();
		let max_parallel = if is_major_syncing { 1 } else { self.max_parallel_downloads };
		let max_blocks_per_request = self.max_blocks_per_request;
		let gap_sync = &mut self.gap_sync;
		self.peers
			.iter_mut()
			.filter_map(move |(&id, peer)| {
				if !peer.state.is_available() || !allowed_requests.contains(&id) {
					return None
				}

				// If our best queued is more than `MAX_BLOCKS_TO_LOOK_BACKWARDS` blocks away from
				// the common number, the peer best number is higher than our best queued and the
				// common number is smaller than the last finalized block number, we should do an
				// ancestor search to find a better common block. If the queue is full we wait till
				// all blocks are imported though.
				if best_queued.saturating_sub(peer.common_number) >
					MAX_BLOCKS_TO_LOOK_BACKWARDS.into() &&
					best_queued < peer.best_number &&
					peer.common_number < last_finalized &&
					queue.len() <= MAJOR_SYNC_BLOCKS.into()
				{
					trace!(
						target: LOG_TARGET,
						"Peer {:?} common block {} too far behind of our best {}. Starting ancestry search.",
						id,
						peer.common_number,
						best_queued,
					);
					let current = std::cmp::min(peer.best_number, best_queued);
					peer.state = PeerSyncState::AncestorSearch {
						current,
						start: best_queued,
						state: AncestorSearchState::ExponentialBackoff(One::one()),
					};
					Some((id, ancestry_request::<B>(current)))
				} else if let Some((range, req)) = peer_block_request(
					&id,
					peer,
					blocks,
					attrs,
					max_parallel,
					max_blocks_per_request,
					last_finalized,
					best_queued,
				) {
					peer.state = PeerSyncState::DownloadingNew(range.start);
					trace!(
						target: LOG_TARGET,
						"New block request for {}, (best:{}, common:{}) {:?}",
						id,
						peer.best_number,
						peer.common_number,
						req,
					);
					Some((id, req))
				} else if let Some((hash, req)) = fork_sync_request(
					&id,
					fork_targets,
					best_queued,
					last_finalized,
					attrs,
					|hash| {
						if queue.contains(hash) {
							BlockStatus::Queued
						} else {
							client.block_status(*hash).unwrap_or(BlockStatus::Unknown)
						}
					},
					max_blocks_per_request,
				) {
					trace!(target: LOG_TARGET, "Downloading fork {hash:?} from {id}");
					peer.state = PeerSyncState::DownloadingStale(hash);
					Some((id, req))
				} else if let Some((range, req)) = gap_sync.as_mut().and_then(|sync| {
					peer_gap_block_request(
						&id,
						peer,
						&mut sync.blocks,
						attrs,
						sync.target,
						sync.best_queued_number,
						max_blocks_per_request,
					)
				}) {
					peer.state = PeerSyncState::DownloadingGap(range.start);
					trace!(
						target: LOG_TARGET,
						"New gap block request for {}, (best:{}, common:{}) {:?}",
						id,
						peer.best_number,
						peer.common_number,
						req,
					);
					Some((id, req))
				} else {
					None
				}
			})
			.collect()
	}

	/// Get a state request scheduled by sync to be sent out (if any).
	fn state_request(&mut self) -> Option<(PeerId, OpaqueStateRequest)> {
		if self.allowed_requests.is_empty() {
			return None
		}
		if (self.state_sync.is_some() || self.warp_sync.is_some()) &&
			self.peers.iter().any(|(_, peer)| peer.state == PeerSyncState::DownloadingState)
		{
			// Only one pending state request is allowed.
			return None
		}
		if let Some(sync) = &self.state_sync {
			if sync.is_complete() {
				return None
			}

			for (id, peer) in self.peers.iter_mut() {
				if peer.state.is_available() && peer.common_number >= sync.target_block_num() {
					peer.state = PeerSyncState::DownloadingState;
					let request = sync.next_request();
					trace!(target: LOG_TARGET, "New StateRequest for {}: {:?}", id, request);
					self.allowed_requests.clear();
					return Some((*id, OpaqueStateRequest(Box::new(request))))
				}
			}
		}
		if let Some(sync) = &self.warp_sync {
			if sync.is_complete() {
				return None
			}
			if let (Some(request), Some(target)) =
				(sync.next_state_request(), sync.target_block_number())
			{
				for (id, peer) in self.peers.iter_mut() {
					if peer.state.is_available() && peer.best_number >= target {
						trace!(target: LOG_TARGET, "New StateRequest for {id}: {request:?}");
						peer.state = PeerSyncState::DownloadingState;
						self.allowed_requests.clear();
						return Some((*id, OpaqueStateRequest(Box::new(request))))
					}
				}
			}
		}
		None
	}

	/// Get a warp proof request scheduled by sync to be sent out (if any).
	fn warp_sync_request(&mut self) -> Option<(PeerId, WarpProofRequest<B>)> {
		if let Some(sync) = &self.warp_sync {
			if self.allowed_requests.is_empty() ||
				sync.is_complete() ||
				self.peers
					.iter()
					.any(|(_, peer)| peer.state == PeerSyncState::DownloadingWarpProof)
			{
				// Only one pending state request is allowed.
				return None
			}
			if let Some(request) = sync.next_warp_proof_request() {
				let mut targets: Vec<_> = self.peers.values().map(|p| p.best_number).collect();
				if !targets.is_empty() {
					targets.sort();
					let median = targets[targets.len() / 2];
					// Find a random peer that is synced as much as peer majority.
					for (id, peer) in self.peers.iter_mut() {
						if peer.state.is_available() && peer.best_number >= median {
							trace!(target: LOG_TARGET, "New WarpProofRequest for {id}");
							peer.state = PeerSyncState::DownloadingWarpProof;
							self.allowed_requests.clear();
							return Some((*id, request))
						}
					}
				}
			}
		}
		None
	}

	#[must_use]
	fn on_state_data(
		&mut self,
		peer_id: &PeerId,
		response: OpaqueStateResponse,
	) -> Result<(), BadPeer> {
		let response: Box<StateResponse> = response.0.downcast().map_err(|_error| {
			error!(
				target: LOG_TARGET,
				"Failed to downcast opaque state response, this is an implementation bug."
			);

			BadPeer(*peer_id, rep::BAD_RESPONSE)
		})?;

		if let Some(peer) = self.peers.get_mut(peer_id) {
			if let PeerSyncState::DownloadingState = peer.state {
				peer.state = PeerSyncState::Available;
				self.allowed_requests.set_all();
			}
		}
		let import_result = if let Some(sync) = &mut self.state_sync {
			debug!(
				target: LOG_TARGET,
				"Importing state data from {} with {} keys, {} proof nodes.",
				peer_id,
				response.entries.len(),
				response.proof.len(),
			);
			sync.import(*response)
		} else if let Some(sync) = &mut self.warp_sync {
			debug!(
				target: LOG_TARGET,
				"Importing state data from {} with {} keys, {} proof nodes.",
				peer_id,
				response.entries.len(),
				response.proof.len(),
			);
			sync.import_state(*response)
		} else {
			debug!(target: LOG_TARGET, "Ignored obsolete state response from {peer_id}");
			return Err(BadPeer(*peer_id, rep::NOT_REQUESTED))
		};

		match import_result {
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
					skip_execution: self.skip_execution(),
					state: Some(state),
				};
				debug!(target: LOG_TARGET, "State download is complete. Import is queued");
				self.actions.push(ChainSyncAction::ImportBlocks { origin, blocks: vec![block] });
				Ok(())
			},
			ImportResult::Continue => Ok(()),
			ImportResult::BadResponse => {
				debug!(target: LOG_TARGET, "Bad state data received from {peer_id}");
				Err(BadPeer(*peer_id, rep::BAD_BLOCK))
			},
		}
	}

	/// Submit a warp proof response received.
	pub fn on_warp_sync_response(&mut self, peer_id: &PeerId, response: EncodedProof) {
		if let Some(peer) = self.peers.get_mut(peer_id) {
			if let PeerSyncState::DownloadingWarpProof = peer.state {
				peer.state = PeerSyncState::Available;
				self.allowed_requests.set_all();
			}
		}
		let import_result = if let Some(sync) = &mut self.warp_sync {
			debug!(
				target: LOG_TARGET,
				"Importing warp proof data from {}, {} bytes.",
				peer_id,
				response.0.len(),
			);
			sync.import_warp_proof(response)
		} else {
			debug!(target: LOG_TARGET, "Ignored obsolete warp sync response from {peer_id}");
			self.actions
				.push(ChainSyncAction::DropPeer(BadPeer(*peer_id, rep::NOT_REQUESTED)));
			return
		};

		match import_result {
			WarpProofImportResult::Success => {},
			WarpProofImportResult::BadResponse => {
				debug!(target: LOG_TARGET, "Bad proof data received from {peer_id}");
				self.actions.push(ChainSyncAction::DropPeer(BadPeer(*peer_id, rep::BAD_BLOCK)));
			},
		}
	}

	/// A batch of blocks have been processed, with or without errors.
	///
	/// Call this when a batch of blocks have been processed by the import
	/// queue, with or without errors. If an error is returned, the pending response
	/// from the peer must be dropped.
	pub fn on_blocks_processed(
		&mut self,
		imported: usize,
		count: usize,
		results: Vec<(Result<BlockImportStatus<NumberFor<B>>, BlockImportError>, B::Hash)>,
	) {
		trace!(target: LOG_TARGET, "Imported {imported} of {count}");

		let mut has_error = false;
		for (_, hash) in &results {
			self.queue_blocks.remove(hash);
			self.blocks.clear_queued(hash);
			if let Some(gap_sync) = &mut self.gap_sync {
				gap_sync.blocks.clear_queued(hash);
			}
		}
		for (result, hash) in results {
			if has_error {
				break
			}

			has_error |= result.is_err();

			match result {
				Ok(BlockImportStatus::ImportedKnown(number, peer_id)) =>
					if let Some(peer) = peer_id {
						self.update_peer_common_number(&peer, number);
					},
				Ok(BlockImportStatus::ImportedUnknown(number, aux, peer_id)) => {
					if aux.clear_justification_requests {
						trace!(
							target: LOG_TARGET,
							"Block imported clears all pending justification requests {number}: {hash:?}",
						);
						self.clear_justification_requests();
					}

					if aux.needs_justification {
						trace!(
							target: LOG_TARGET,
							"Block imported but requires justification {number}: {hash:?}",
						);
						self.request_justification(&hash, number);
					}

					if aux.bad_justification {
						if let Some(ref peer) = peer_id {
							warn!("💔 Sent block with bad justification to import");
							self.actions.push(ChainSyncAction::DropPeer(BadPeer(
								*peer,
								rep::BAD_JUSTIFICATION,
							)));
						}
					}

					if let Some(peer) = peer_id {
						self.update_peer_common_number(&peer, number);
					}
					let state_sync_complete =
						self.state_sync.as_ref().map_or(false, |s| s.target() == hash);
					if state_sync_complete {
						info!(
							target: LOG_TARGET,
							"State sync is complete ({} MiB), restarting block sync.",
							self.state_sync.as_ref().map_or(0, |s| s.progress().size / (1024 * 1024)),
						);
						self.state_sync = None;
						self.mode = SyncMode::Full;
						self.restart();
					}
					let warp_sync_complete = self
						.warp_sync
						.as_ref()
						.map_or(false, |s| s.target_block_hash() == Some(hash));
					if warp_sync_complete {
						info!(
							target: LOG_TARGET,
							"Warp sync is complete ({} MiB), restarting block sync.",
							self.warp_sync.as_ref().map_or(0, |s| s.progress().total_bytes / (1024 * 1024)),
						);
						self.warp_sync = None;
						self.mode = SyncMode::Full;
						self.restart();
					}
					let gap_sync_complete =
						self.gap_sync.as_ref().map_or(false, |s| s.target == number);
					if gap_sync_complete {
						info!(
							target: LOG_TARGET,
							"Block history download is complete."
						);
						self.gap_sync = None;
					}
				},
				Err(BlockImportError::IncompleteHeader(peer_id)) =>
					if let Some(peer) = peer_id {
						warn!(
							target: LOG_TARGET,
							"💔 Peer sent block with incomplete header to import",
						);
						self.actions
							.push(ChainSyncAction::DropPeer(BadPeer(peer, rep::INCOMPLETE_HEADER)));
						self.restart();
					},
				Err(BlockImportError::VerificationFailed(peer_id, e)) => {
					let extra_message = peer_id
						.map_or_else(|| "".into(), |peer| format!(" received from ({peer})"));

					warn!(
						target: LOG_TARGET,
						"💔 Verification failed for block {hash:?}{extra_message}: {e:?}",
					);

					if let Some(peer) = peer_id {
						self.actions
							.push(ChainSyncAction::DropPeer(BadPeer(peer, rep::VERIFICATION_FAIL)));
					}

					self.restart();
				},
				Err(BlockImportError::BadBlock(peer_id)) =>
					if let Some(peer) = peer_id {
						warn!(
							target: LOG_TARGET,
							"💔 Block {hash:?} received from peer {peer} has been blacklisted",
						);
						self.actions.push(ChainSyncAction::DropPeer(BadPeer(peer, rep::BAD_BLOCK)));
					},
				Err(BlockImportError::MissingState) => {
					// This may happen if the chain we were requesting upon has been discarded
					// in the meantime because other chain has been finalized.
					// Don't mark it as bad as it still may be synced if explicitly requested.
					trace!(target: LOG_TARGET, "Obsolete block {hash:?}");
				},
				e @ Err(BlockImportError::UnknownParent) | e @ Err(BlockImportError::Other(_)) => {
					warn!(target: LOG_TARGET, "💔 Error importing block {hash:?}: {}", e.unwrap_err());
					self.state_sync = None;
					self.warp_sync = None;
					self.restart();
				},
				Err(BlockImportError::Cancelled) => {},
			};
		}

		self.allowed_requests.set_all();
	}

	/// Get pending actions to perform.
	#[must_use]
	pub fn actions(&mut self) -> impl Iterator<Item = ChainSyncAction<B>> {
		let block_requests = self
			.block_requests()
			.into_iter()
			.map(|(peer_id, request)| ChainSyncAction::SendBlockRequest { peer_id, request });
		self.actions.extend(block_requests);

		let justification_requests = self
			.justification_requests()
			.into_iter()
			.map(|(peer_id, request)| ChainSyncAction::SendBlockRequest { peer_id, request });
		self.actions.extend(justification_requests);

		let state_request = self
			.state_request()
			.into_iter()
			.map(|(peer_id, request)| ChainSyncAction::SendStateRequest { peer_id, request });
		self.actions.extend(state_request);

		let warp_proof_request = self
			.warp_sync_request()
			.into_iter()
			.map(|(peer_id, request)| ChainSyncAction::SendWarpProofRequest { peer_id, request });
		self.actions.extend(warp_proof_request);

		std::mem::take(&mut self.actions).into_iter()
	}

	/// A version of `actions()` that doesn't schedule extra requests. For testing only.
	#[cfg(test)]
	#[must_use]
	fn take_actions(&mut self) -> impl Iterator<Item = ChainSyncAction<B>> {
		std::mem::take(&mut self.actions).into_iter()
	}
}

// This is purely during a backwards compatible transitionary period and should be removed
// once we can assume all nodes can send and receive multiple Justifications
// The ID tag is hardcoded here to avoid depending on the GRANDPA crate.
// See: https://github.com/paritytech/substrate/issues/8172
fn legacy_justification_mapping(
	justification: Option<EncodedJustification>,
) -> Option<Justifications> {
	justification.map(|just| (*b"FRNK", just).into())
}

/// Request the ancestry for a block. Sends a request for header and justification for the given
/// block number. Used during ancestry search.
fn ancestry_request<B: BlockT>(block: NumberFor<B>) -> BlockRequest<B> {
	BlockRequest::<B> {
		id: 0,
		fields: BlockAttributes::HEADER | BlockAttributes::JUSTIFICATION,
		from: FromBlock::Number(block),
		direction: Direction::Ascending,
		max: Some(1),
	}
}

/// The ancestor search state expresses which algorithm, and its stateful parameters, we are using
/// to try to find an ancestor block
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum AncestorSearchState<B: BlockT> {
	/// Use exponential backoff to find an ancestor, then switch to binary search.
	/// We keep track of the exponent.
	ExponentialBackoff(NumberFor<B>),
	/// Using binary search to find the best ancestor.
	/// We keep track of left and right bounds.
	BinarySearch(NumberFor<B>, NumberFor<B>),
}

/// This function handles the ancestor search strategy used. The goal is to find a common point
/// that both our chains agree on that is as close to the tip as possible.
/// The way this works is we first have an exponential backoff strategy, where we try to step
/// forward until we find a block hash mismatch. The size of the step doubles each step we take.
///
/// When we've found a block hash mismatch we then fall back to a binary search between the two
/// last known points to find the common block closest to the tip.
fn handle_ancestor_search_state<B: BlockT>(
	state: &AncestorSearchState<B>,
	curr_block_num: NumberFor<B>,
	block_hash_match: bool,
) -> Option<(AncestorSearchState<B>, NumberFor<B>)> {
	let two = <NumberFor<B>>::one() + <NumberFor<B>>::one();
	match state {
		AncestorSearchState::ExponentialBackoff(next_distance_to_tip) => {
			let next_distance_to_tip = *next_distance_to_tip;
			if block_hash_match && next_distance_to_tip == One::one() {
				// We found the ancestor in the first step so there is no need to execute binary
				// search.
				return None
			}
			if block_hash_match {
				let left = curr_block_num;
				let right = left + next_distance_to_tip / two;
				let middle = left + (right - left) / two;
				Some((AncestorSearchState::BinarySearch(left, right), middle))
			} else {
				let next_block_num =
					curr_block_num.checked_sub(&next_distance_to_tip).unwrap_or_else(Zero::zero);
				let next_distance_to_tip = next_distance_to_tip * two;
				Some((
					AncestorSearchState::ExponentialBackoff(next_distance_to_tip),
					next_block_num,
				))
			}
		},
		AncestorSearchState::BinarySearch(mut left, mut right) => {
			if left >= curr_block_num {
				return None
			}
			if block_hash_match {
				left = curr_block_num;
			} else {
				right = curr_block_num;
			}
			assert!(right >= left);
			let middle = left + (right - left) / two;
			if middle == curr_block_num {
				None
			} else {
				Some((AncestorSearchState::BinarySearch(left, right), middle))
			}
		},
	}
}

/// Get a new block request for the peer if any.
fn peer_block_request<B: BlockT>(
	id: &PeerId,
	peer: &PeerSync<B>,
	blocks: &mut BlockCollection<B>,
	attrs: BlockAttributes,
	max_parallel_downloads: u32,
	max_blocks_per_request: u32,
	finalized: NumberFor<B>,
	best_num: NumberFor<B>,
) -> Option<(Range<NumberFor<B>>, BlockRequest<B>)> {
	if best_num >= peer.best_number {
		// Will be downloaded as alternative fork instead.
		return None
	} else if peer.common_number < finalized {
		trace!(
			target: LOG_TARGET,
			"Requesting pre-finalized chain from {:?}, common={}, finalized={}, peer best={}, our best={}",
			id, peer.common_number, finalized, peer.best_number, best_num,
		);
	}
	let range = blocks.needed_blocks(
		*id,
		max_blocks_per_request,
		peer.best_number,
		peer.common_number,
		max_parallel_downloads,
		MAX_DOWNLOAD_AHEAD,
	)?;

	// The end is not part of the range.
	let last = range.end.saturating_sub(One::one());

	let from = if peer.best_number == last {
		FromBlock::Hash(peer.best_hash)
	} else {
		FromBlock::Number(last)
	};

	let request = BlockRequest::<B> {
		id: 0,
		fields: attrs,
		from,
		direction: Direction::Descending,
		max: Some((range.end - range.start).saturated_into::<u32>()),
	};

	Some((range, request))
}

/// Get a new block request for the peer if any.
fn peer_gap_block_request<B: BlockT>(
	id: &PeerId,
	peer: &PeerSync<B>,
	blocks: &mut BlockCollection<B>,
	attrs: BlockAttributes,
	target: NumberFor<B>,
	common_number: NumberFor<B>,
	max_blocks_per_request: u32,
) -> Option<(Range<NumberFor<B>>, BlockRequest<B>)> {
	let range = blocks.needed_blocks(
		*id,
		max_blocks_per_request,
		std::cmp::min(peer.best_number, target),
		common_number,
		1,
		MAX_DOWNLOAD_AHEAD,
	)?;

	// The end is not part of the range.
	let last = range.end.saturating_sub(One::one());
	let from = FromBlock::Number(last);

	let request = BlockRequest::<B> {
		id: 0,
		fields: attrs,
		from,
		direction: Direction::Descending,
		max: Some((range.end - range.start).saturated_into::<u32>()),
	};
	Some((range, request))
}

/// Get pending fork sync targets for a peer.
fn fork_sync_request<B: BlockT>(
	id: &PeerId,
	targets: &mut HashMap<B::Hash, ForkTarget<B>>,
	best_num: NumberFor<B>,
	finalized: NumberFor<B>,
	attributes: BlockAttributes,
	check_block: impl Fn(&B::Hash) -> BlockStatus,
	max_blocks_per_request: u32,
) -> Option<(B::Hash, BlockRequest<B>)> {
	targets.retain(|hash, r| {
		if r.number <= finalized {
			trace!(
				target: LOG_TARGET,
				"Removed expired fork sync request {:?} (#{})",
				hash,
				r.number,
			);
			return false
		}
		if check_block(hash) != BlockStatus::Unknown {
			trace!(
				target: LOG_TARGET,
				"Removed obsolete fork sync request {:?} (#{})",
				hash,
				r.number,
			);
			return false
		}
		true
	});
	for (hash, r) in targets {
		if !r.peers.contains(&id) {
			continue
		}
		// Download the fork only if it is behind or not too far ahead our tip of the chain
		// Otherwise it should be downloaded in full sync mode.
		if r.number <= best_num ||
			(r.number - best_num).saturated_into::<u32>() < max_blocks_per_request as u32
		{
			let parent_status = r.parent_hash.as_ref().map_or(BlockStatus::Unknown, check_block);
			let count = if parent_status == BlockStatus::Unknown {
				(r.number - finalized).saturated_into::<u32>() // up to the last finalized block
			} else {
				// request only single block
				1
			};
			trace!(
				target: LOG_TARGET,
				"Downloading requested fork {hash:?} from {id}, {count} blocks",
			);
			return Some((
				*hash,
				BlockRequest::<B> {
					id: 0,
					fields: attributes,
					from: FromBlock::Hash(*hash),
					direction: Direction::Descending,
					max: Some(count),
				},
			))
		} else {
			trace!(target: LOG_TARGET, "Fork too far in the future: {:?} (#{})", hash, r.number);
		}
	}
	None
}

/// Returns `true` if the given `block` is a descendent of `base`.
fn is_descendent_of<Block, T>(
	client: &T,
	base: &Block::Hash,
	block: &Block::Hash,
) -> sp_blockchain::Result<bool>
where
	Block: BlockT,
	T: HeaderMetadata<Block, Error = sp_blockchain::Error> + ?Sized,
{
	if base == block {
		return Ok(false)
	}

	let ancestor = sp_blockchain::lowest_common_ancestor(client, *block, *base)?;

	Ok(ancestor.hash == *base)
}

/// Validate that the given `blocks` are correct.
/// Returns the number of the first block in the sequence.
///
/// It is expected that `blocks` are in ascending order.
fn validate_blocks<Block: BlockT>(
	blocks: &Vec<BlockData<Block>>,
	peer_id: &PeerId,
	request: Option<BlockRequest<Block>>,
) -> Result<Option<NumberFor<Block>>, BadPeer> {
	if let Some(request) = request {
		if Some(blocks.len() as _) > request.max {
			debug!(
				target: LOG_TARGET,
				"Received more blocks than requested from {}. Expected in maximum {:?}, got {}.",
				peer_id,
				request.max,
				blocks.len(),
			);

			return Err(BadPeer(*peer_id, rep::NOT_REQUESTED))
		}

		let block_header =
			if request.direction == Direction::Descending { blocks.last() } else { blocks.first() }
				.and_then(|b| b.header.as_ref());

		let expected_block = block_header.as_ref().map_or(false, |h| match request.from {
			FromBlock::Hash(hash) => h.hash() == hash,
			FromBlock::Number(n) => h.number() == &n,
		});

		if !expected_block {
			debug!(
				target: LOG_TARGET,
				"Received block that was not requested. Requested {:?}, got {:?}.",
				request.from,
				block_header,
			);

			return Err(BadPeer(*peer_id, rep::NOT_REQUESTED))
		}

		if request.fields.contains(BlockAttributes::HEADER) &&
			blocks.iter().any(|b| b.header.is_none())
		{
			trace!(
				target: LOG_TARGET,
				"Missing requested header for a block in response from {peer_id}.",
			);

			return Err(BadPeer(*peer_id, rep::BAD_RESPONSE))
		}

		if request.fields.contains(BlockAttributes::BODY) && blocks.iter().any(|b| b.body.is_none())
		{
			trace!(
				target: LOG_TARGET,
				"Missing requested body for a block in response from {peer_id}.",
			);

			return Err(BadPeer(*peer_id, rep::BAD_RESPONSE))
		}
	}

	for b in blocks {
		if let Some(header) = &b.header {
			let hash = header.hash();
			if hash != b.hash {
				debug!(
					target:LOG_TARGET,
					"Bad header received from {}. Expected hash {:?}, got {:?}",
					peer_id,
					b.hash,
					hash,
				);
				return Err(BadPeer(*peer_id, rep::BAD_BLOCK))
			}
		}
		if let (Some(header), Some(body)) = (&b.header, &b.body) {
			let expected = *header.extrinsics_root();
			let got = HashingFor::<Block>::ordered_trie_root(
				body.iter().map(Encode::encode).collect(),
				sp_runtime::StateVersion::V0,
			);
			if expected != got {
				debug!(
					target:LOG_TARGET,
					"Bad extrinsic root for a block {} received from {}. Expected {:?}, got {:?}",
					b.hash,
					peer_id,
					expected,
					got,
				);
				return Err(BadPeer(*peer_id, rep::BAD_BLOCK))
			}
		}
	}

	Ok(blocks.first().and_then(|b| b.header.as_ref()).map(|h| *h.number()))
}
