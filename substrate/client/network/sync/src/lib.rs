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
	schema::v1::StateResponse,
	state::StateSync,
	warp::{WarpProofImportResult, WarpSync, WarpSyncConfig},
};

use codec::Encode;
use extra_requests::ExtraRequests;
use libp2p::PeerId;
use log::{debug, error, info, trace, warn};

use sc_client_api::{BlockBackend, ProofProvider};
use sc_consensus::{BlockImportError, BlockImportStatus, IncomingBlock};
use sc_network::types::ProtocolName;
use sc_network_common::sync::{
	message::{
		BlockAnnounce, BlockAttributes, BlockData, BlockRequest, BlockResponse, Direction,
		FromBlock,
	},
	warp::{EncodedProof, WarpProofRequest, WarpSyncPhase, WarpSyncProgress},
	BadPeer, ChainSync as ChainSyncT, ImportBlocksAction, Metrics, OnBlockData,
	OnBlockJustification, OnStateData, OpaqueStateRequest, OpaqueStateResponse, PeerInfo, SyncMode,
	SyncState, SyncStatus,
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

pub use service::chain_sync::SyncingService;

mod block_announce_validator;
mod extra_requests;
mod futures_stream;
mod pending_responses;
mod schema;

pub mod block_relay_protocol;
pub mod block_request_handler;
pub mod blocks;
pub mod engine;
pub mod mock;
pub mod service;
pub mod state;
pub mod state_request_handler;
pub mod warp;
pub mod warp_request_handler;

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

/// Maximum blocks per response.
pub(crate) const MAX_BLOCKS_IN_RESPONSE: usize = 128;

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

/// Action that [`engine::SyncingEngine`] should perform after reporting imported blocks with
/// [`ChainSync::on_blocks_processed`].
enum BlockRequestAction<B: BlockT> {
	/// Send block request to peer. Always implies dropping a stale block request to the same peer.
	SendRequest { peer_id: PeerId, request: BlockRequest<B> },
	/// Drop stale block request.
	RemoveStale { peer_id: PeerId },
}

/// Action that [`engine::SyncingEngine`] should perform if we want to import justifications.
struct ImportJustificationsAction<B: BlockT> {
	peer_id: PeerId,
	hash: B::Hash,
	number: NumberFor<B>,
	justifications: Justifications,
}

/// Action that [`engine::SyncingEngine`] should perform on behalf of [`ChainSync`]
/// after reporting block response with [`ChainSync::on_block_response`].
enum OnBlockResponse<B: BlockT> {
	/// Nothing to do
	Nothing,
	/// Perform block request.
	SendBlockRequest { peer_id: PeerId, request: BlockRequest<B> },
	/// Import blocks.
	ImportBlocks(ImportBlocksAction<B>),
	/// Import justifications.
	ImportJustifications(ImportJustificationsAction<B>),
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
	/// Handle for communicating with `NetworkService`
	network_service: service::network::NetworkServiceHandle,
	/// Protocol name used for block announcements
	block_announce_protocol_name: ProtocolName,
}

/// All the data we have about a Peer that we are trying to sync with
#[derive(Debug, Clone)]
pub struct PeerSync<B: BlockT> {
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
pub enum PeerSyncState<B: BlockT> {
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

impl<B, Client> ChainSyncT<B> for ChainSync<B, Client>
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
	fn peer_info(&self, who: &PeerId) -> Option<PeerInfo<B>> {
		self.peers
			.get(who)
			.map(|p| PeerInfo { best_hash: p.best_hash, best_number: p.best_number })
	}

	/// Returns the current sync status.
	fn status(&self) -> SyncStatus<B> {
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

	fn num_sync_requests(&self) -> usize {
		self.fork_targets
			.values()
			.filter(|f| f.number <= self.best_queued_number)
			.count()
	}

	fn num_downloaded_blocks(&self) -> usize {
		self.downloaded_blocks
	}

	fn num_peers(&self) -> usize {
		self.peers.len()
	}

	#[must_use]
	fn new_peer(
		&mut self,
		who: PeerId,
		best_hash: B::Hash,
		best_number: NumberFor<B>,
	) -> Result<Option<BlockRequest<B>>, BadPeer> {
		// There is nothing sync can get from the node that has no blockchain data.
		match self.block_status(&best_hash) {
			Err(e) => {
				debug!(target:LOG_TARGET, "Error reading blockchain: {e}");
				Err(BadPeer(who, rep::BLOCKCHAIN_READ_ERROR))
			},
			Ok(BlockStatus::KnownBad) => {
				info!("💔 New peer with known bad best block {} ({}).", best_hash, best_number);
				Err(BadPeer(who, rep::BAD_BLOCK))
			},
			Ok(BlockStatus::Unknown) => {
				if best_number.is_zero() {
					info!("💔 New peer with unknown genesis hash {} ({}).", best_hash, best_number);
					return Err(BadPeer(who, rep::GENESIS_MISMATCH))
				}

				// If there are more than `MAJOR_SYNC_BLOCKS` in the import queue then we have
				// enough to do in the import queue that it's not worth kicking off
				// an ancestor search, which is what we do in the next match case below.
				if self.queue_blocks.len() > MAJOR_SYNC_BLOCKS.into() {
					debug!(
						target:LOG_TARGET,
						"New peer with unknown best hash {} ({}), assuming common block.",
						self.best_queued_hash,
						self.best_queued_number
					);
					self.peers.insert(
						who,
						PeerSync {
							peer_id: who,
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
						"New peer with best hash {best_hash} ({best_number}).",
					);

					(PeerSyncState::Available, None)
				} else {
					let common_best = std::cmp::min(self.best_queued_number, best_number);

					debug!(
						target:LOG_TARGET,
						"New peer with unknown best hash {} ({}), searching for common ancestor.",
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

				self.allowed_requests.add(&who);
				self.peers.insert(
					who,
					PeerSync {
						peer_id: who,
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
					"New peer with known best hash {best_hash} ({best_number}).",
				);
				self.peers.insert(
					who,
					PeerSync {
						peer_id: who,
						common_number: std::cmp::min(self.best_queued_number, best_number),
						best_hash,
						best_number,
						state: PeerSyncState::Available,
					},
				);
				self.allowed_requests.add(&who);
				Ok(None)
			},
		}
	}

	fn update_chain_info(&mut self, best_hash: &B::Hash, best_number: NumberFor<B>) {
		self.on_block_queued(best_hash, best_number);
	}

	fn request_justification(&mut self, hash: &B::Hash, number: NumberFor<B>) {
		let client = &self.client;
		self.extra_justifications
			.schedule((*hash, number), |base, block| is_descendent_of(&**client, base, block))
	}

	fn clear_justification_requests(&mut self) {
		self.extra_justifications.reset();
	}

	// The implementation is similar to `on_validated_block_announce` with unknown parent hash.
	fn set_sync_fork_request(
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

	#[must_use]
	fn on_block_data(
		&mut self,
		who: &PeerId,
		request: Option<BlockRequest<B>>,
		response: BlockResponse<B>,
	) -> Result<OnBlockData<B>, BadPeer> {
		self.downloaded_blocks += response.blocks.len();
		let mut gap = false;
		let new_blocks: Vec<IncomingBlock<B>> = if let Some(peer) = self.peers.get_mut(who) {
			let mut blocks = response.blocks;
			if request.as_ref().map_or(false, |r| r.direction == Direction::Descending) {
				trace!(target: LOG_TARGET, "Reversing incoming block list");
				blocks.reverse()
			}
			self.allowed_requests.add(who);
			if let Some(request) = request {
				match &mut peer.state {
					PeerSyncState::DownloadingNew(_) => {
						self.blocks.clear_peer_download(who);
						peer.state = PeerSyncState::Available;
						if let Some(start_block) =
							validate_blocks::<B>(&blocks, who, Some(request))?
						{
							self.blocks.insert(start_block, blocks, *who);
						}
						self.ready_blocks()
					},
					PeerSyncState::DownloadingGap(_) => {
						peer.state = PeerSyncState::Available;
						if let Some(gap_sync) = &mut self.gap_sync {
							gap_sync.blocks.clear_peer_download(who);
							if let Some(start_block) =
								validate_blocks::<B>(&blocks, who, Some(request))?
							{
								gap_sync.blocks.insert(start_block, blocks, *who);
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
							debug!(target: LOG_TARGET, "Unexpected gap block response from {who}");
							return Err(BadPeer(*who, rep::NO_BLOCK))
						}
					},
					PeerSyncState::DownloadingStale(_) => {
						peer.state = PeerSyncState::Available;
						if blocks.is_empty() {
							debug!(target: LOG_TARGET, "Empty block response from {who}");
							return Err(BadPeer(*who, rep::NO_BLOCK))
						}
						validate_blocks::<B>(&blocks, who, Some(request))?;
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
									origin: Some(*who),
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
									who,
								);
								maybe_our_block_hash.filter(|x| x == &block.hash)
							},
							(None, _) => {
								debug!(
									target: LOG_TARGET,
									"Invalid response when searching for ancestor from {who}",
								);
								return Err(BadPeer(*who, rep::UNKNOWN_ANCESTOR))
							},
							(_, Err(e)) => {
								info!(
									target: LOG_TARGET,
									"❌ Error answering legitimate blockchain query: {e}",
								);
								return Err(BadPeer(*who, rep::BLOCKCHAIN_READ_ERROR))
							},
						};
						if matching_hash.is_some() {
							if *start < self.best_queued_number &&
								self.best_queued_number <= peer.best_number
							{
								// We've made progress on this chain since the search was started.
								// Opportunistically set common number to updated number
								// instead of the one that started the search.
								peer.common_number = self.best_queued_number;
							} else if peer.common_number < *current {
								peer.common_number = *current;
							}
						}
						if matching_hash.is_none() && current.is_zero() {
							trace!(
								target:LOG_TARGET,
								"Ancestry search: genesis mismatch for peer {who}",
							);
							return Err(BadPeer(*who, rep::GENESIS_MISMATCH))
						}
						if let Some((next_state, next_num)) =
							handle_ancestor_search_state(state, *current, matching_hash.is_some())
						{
							peer.state = PeerSyncState::AncestorSearch {
								current: next_num,
								start: *start,
								state: next_state,
							};
							return Ok(OnBlockData::Request(*who, ancestry_request::<B>(next_num)))
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
									who,
								);
								self.fork_targets
									.entry(peer.best_hash)
									.or_insert_with(|| ForkTarget {
										number: peer.best_number,
										parent_hash: None,
										peers: Default::default(),
									})
									.peers
									.insert(*who);
							}
							peer.state = PeerSyncState::Available;
							Vec::new()
						}
					},
					PeerSyncState::DownloadingWarpTargetBlock => {
						peer.state = PeerSyncState::Available;
						if let Some(warp_sync) = &mut self.warp_sync {
							if blocks.len() == 1 {
								validate_blocks::<B>(&blocks, who, Some(request))?;
								match warp_sync.import_target_block(
									blocks.pop().expect("`blocks` len checked above."),
								) {
									warp::TargetBlockImportResult::Success =>
										return Ok(OnBlockData::Continue),
									warp::TargetBlockImportResult::BadResponse =>
										return Err(BadPeer(*who, rep::VERIFICATION_FAIL)),
								}
							} else if blocks.is_empty() {
								debug!(target: LOG_TARGET, "Empty block response from {who}");
								return Err(BadPeer(*who, rep::NO_BLOCK))
							} else {
								debug!(
									target: LOG_TARGET,
									"Too many blocks ({}) in warp target block response from {}",
									blocks.len(),
									who,
								);
								return Err(BadPeer(*who, rep::NOT_REQUESTED))
							}
						} else {
							debug!(
								target: LOG_TARGET,
								"Logic error: we think we are downloading warp target block from {}, but no warp sync is happening.",
								who,
							);
							return Ok(OnBlockData::Continue)
						}
					},
					PeerSyncState::Available |
					PeerSyncState::DownloadingJustification(..) |
					PeerSyncState::DownloadingState |
					PeerSyncState::DownloadingWarpProof => Vec::new(),
				}
			} else {
				// When request.is_none() this is a block announcement. Just accept blocks.
				validate_blocks::<B>(&blocks, who, None)?;
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
							origin: Some(*who),
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
			return Err(BadPeer(*who, rep::NOT_REQUESTED))
		};

		Ok(OnBlockData::Import(self.validate_and_queue_blocks(new_blocks, gap)))
	}

	#[must_use]
	fn on_block_justification(
		&mut self,
		who: PeerId,
		response: BlockResponse<B>,
	) -> Result<OnBlockJustification<B>, BadPeer> {
		let peer = if let Some(peer) = self.peers.get_mut(&who) {
			peer
		} else {
			error!(
				target: LOG_TARGET,
				"💔 Called on_block_justification with a peer ID of an unknown peer",
			);
			return Ok(OnBlockJustification::Nothing)
		};

		self.allowed_requests.add(&who);
		if let PeerSyncState::DownloadingJustification(hash) = peer.state {
			peer.state = PeerSyncState::Available;

			// We only request one justification at a time
			let justification = if let Some(block) = response.blocks.into_iter().next() {
				if hash != block.hash {
					warn!(
						target: LOG_TARGET,
						"💔 Invalid block justification provided by {}: requested: {:?} got: {:?}",
						who,
						hash,
						block.hash,
					);
					return Err(BadPeer(who, rep::BAD_JUSTIFICATION))
				}

				block
					.justifications
					.or_else(|| legacy_justification_mapping(block.justification))
			} else {
				// we might have asked the peer for a justification on a block that we assumed it
				// had but didn't (regardless of whether it had a justification for it or not).
				trace!(
					target: LOG_TARGET,
					"Peer {who:?} provided empty response for justification request {hash:?}",
				);

				None
			};

			if let Some((peer_id, hash, number, justifications)) =
				self.extra_justifications.on_response(who, justification)
			{
				return Ok(OnBlockJustification::Import { peer_id, hash, number, justifications })
			}
		}

		Ok(OnBlockJustification::Nothing)
	}

	fn on_justification_import(&mut self, hash: B::Hash, number: NumberFor<B>, success: bool) {
		let finalization_result = if success { Ok((hash, number)) } else { Err(()) };
		self.extra_justifications
			.try_finalize_root((hash, number), finalization_result, true);
		self.allowed_requests.set_all();
	}

	fn on_block_finalized(&mut self, hash: &B::Hash, number: NumberFor<B>) {
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

	fn on_validated_block_announce(
		&mut self,
		is_best: bool,
		who: PeerId,
		announce: &BlockAnnounce<B::Header>,
	) {
		let number = *announce.header.number();
		let hash = announce.header.hash();
		let parent_status =
			self.block_status(announce.header.parent_hash()).unwrap_or(BlockStatus::Unknown);
		let known_parent = parent_status != BlockStatus::Unknown;
		let ancient_parent = parent_status == BlockStatus::InChainPruned;

		let known = self.is_known(&hash);
		let peer = if let Some(peer) = self.peers.get_mut(&who) {
			peer
		} else {
			error!(target: LOG_TARGET, "💔 Called `on_validated_block_announce` with a bad peer ID");
			return
		};

		if let PeerSyncState::AncestorSearch { .. } = peer.state {
			trace!(target: LOG_TARGET, "Peer {} is in the ancestor search state.", who);
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
				self.update_peer_common_number(&who, number);
			} else if announce.header.parent_hash() == &self.best_queued_hash ||
				known_parent && self.best_queued_number >= number
			{
				self.update_peer_common_number(&who, number.saturating_sub(One::one()));
			}
		}
		self.allowed_requests.add(&who);

		// known block case
		if known || self.is_already_downloading(&hash) {
			trace!(target: "sync", "Known block announce from {}: {}", who, hash);
			if let Some(target) = self.fork_targets.get_mut(&hash) {
				target.peers.insert(who);
			}
			return
		}

		if ancient_parent {
			trace!(
				target: "sync",
				"Ignored ancient block announced from {}: {} {:?}",
				who,
				hash,
				announce.header,
			);
			return
		}

		if self.status().state == SyncState::Idle {
			trace!(
				target: "sync",
				"Added sync target for block announced from {}: {} {:?}",
				who,
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
				.insert(who);
		}
	}

	#[must_use]
	fn peer_disconnected(&mut self, who: &PeerId) -> Option<ImportBlocksAction<B>> {
		self.blocks.clear_peer_download(who);
		if let Some(gap_sync) = &mut self.gap_sync {
			gap_sync.blocks.clear_peer_download(who)
		}
		self.peers.remove(who);
		self.extra_justifications.peer_disconnected(who);
		self.allowed_requests.set_all();
		self.fork_targets.retain(|_, target| {
			target.peers.remove(who);
			!target.peers.is_empty()
		});

		let blocks = self.ready_blocks();

		(!blocks.is_empty()).then(|| self.validate_and_queue_blocks(blocks, false))
	}

	fn metrics(&self) -> Metrics {
		Metrics {
			queued_blocks: self.queue_blocks.len().try_into().unwrap_or(std::u32::MAX),
			fork_targets: self.fork_targets.len().try_into().unwrap_or(std::u32::MAX),
			justifications: self.extra_justifications.metrics(),
		}
	}
}

impl<B, Client> ChainSync<B, Client>
where
	Self: ChainSyncT<B>,
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
		block_announce_protocol_name: ProtocolName,
		max_parallel_downloads: u32,
		max_blocks_per_request: u32,
		warp_sync_config: Option<WarpSyncConfig<B>>,
		network_service: service::network::NetworkServiceHandle,
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
			network_service,
			warp_sync_config,
			warp_sync_target_block_header: None,
			block_announce_protocol_name,
		};

		sync.reset_sync_start_point()?;
		Ok(sync)
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

	fn validate_and_queue_blocks(
		&mut self,
		mut new_blocks: Vec<IncomingBlock<B>>,
		gap: bool,
	) -> ImportBlocksAction<B> {
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

		ImportBlocksAction { origin, blocks: new_blocks }
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
	fn restart(&mut self) -> impl Iterator<Item = Result<BlockRequestAction<B>, BadPeer>> + '_ {
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

		old_peers.into_iter().filter_map(move |(peer_id, mut p)| {
			// peers that were downloading justifications
			// should be kept in that state.
			if let PeerSyncState::DownloadingJustification(_) = p.state {
				// We make sure our commmon number is at least something we have.
				p.common_number = self.best_queued_number;
				self.peers.insert(peer_id, p);
				return None
			}

			// handle peers that were in other states.
			match self.new_peer(peer_id, p.best_hash, p.best_number) {
				// since the request is not a justification, remove it from pending responses
				Ok(None) => Some(Ok(BlockRequestAction::RemoveStale { peer_id })),
				// update the request if the new one is available
				Ok(Some(request)) => Some(Ok(BlockRequestAction::SendRequest { peer_id, request })),
				// this implies that we need to drop pending response from the peer
				Err(e) => Some(Err(e)),
			}
		})
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

	/// Is the peer know to the sync state machine?
	pub fn is_peer_known(&self, peer_id: &PeerId) -> bool {
		self.peers.contains_key(peer_id)
	}

	/// Get the set of downloaded blocks that are ready to be queued for import.
	fn ready_blocks(&mut self) -> Vec<IncomingBlock<B>> {
		let start_block = self.best_queued_number + One::one();

		// Verify that the parent of the first available block is in the chain.
		// If not, we are downloading from a fork. In this case, wait until
		// the start block has a parent on chain.
		let parent_on_chain =
			self.blocks.first_ready_block_header(start_block).map_or(false, |hdr| {
				std::matches!(
					self.block_status(hdr.parent_hash()).unwrap_or(BlockStatus::Unknown),
					BlockStatus::InChainWithState |
						BlockStatus::InChainPruned |
						BlockStatus::Queued
				)
			});

		if !parent_on_chain {
			return vec![]
		}

		self.blocks
			.ready_blocks(start_block)
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

	/// Set warp sync target block externally in case we skip warp proof downloading.
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

	/// Process blocks received in a response.
	#[must_use]
	pub(crate) fn on_block_response(
		&mut self,
		peer_id: PeerId,
		request: BlockRequest<B>,
		blocks: Vec<BlockData<B>>,
	) -> OnBlockResponse<B> {
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

		if request.fields == BlockAttributes::JUSTIFICATION {
			match self.on_block_justification(peer_id, block_response) {
				Ok(OnBlockJustification::Nothing) => OnBlockResponse::Nothing,
				Ok(OnBlockJustification::Import { peer_id, hash, number, justifications }) =>
					OnBlockResponse::ImportJustifications(ImportJustificationsAction {
						peer_id,
						hash,
						number,
						justifications,
					}),
				Err(BadPeer(id, repu)) => {
					self.network_service
						.disconnect_peer(id, self.block_announce_protocol_name.clone());
					self.network_service.report_peer(id, repu);
					OnBlockResponse::Nothing
				},
			}
		} else {
			match self.on_block_data(&peer_id, Some(request), block_response) {
				Ok(OnBlockData::Import(action)) => OnBlockResponse::ImportBlocks(action),
				Ok(OnBlockData::Request(peer_id, request)) =>
					OnBlockResponse::SendBlockRequest { peer_id, request },
				Ok(OnBlockData::Continue) => OnBlockResponse::Nothing,
				Err(BadPeer(id, repu)) => {
					self.network_service
						.disconnect_peer(id, self.block_announce_protocol_name.clone());
					self.network_service.report_peer(id, repu);
					OnBlockResponse::Nothing
				},
			}
		}
	}

	/// Process state received in a response.
	#[must_use]
	pub fn on_state_response(
		&mut self,
		peer_id: PeerId,
		response: OpaqueStateResponse,
	) -> Option<ImportBlocksAction<B>> {
		match self.on_state_data(&peer_id, response) {
			Ok(OnStateData::Import(origin, block)) =>
				Some(ImportBlocksAction { origin, blocks: vec![block] }),
			Ok(OnStateData::Continue) => None,
			Err(BadPeer(id, repu)) => {
				self.network_service
					.disconnect_peer(id, self.block_announce_protocol_name.clone());
				self.network_service.report_peer(id, repu);
				None
			},
		}
	}

	pub fn on_warp_sync_response(&mut self, peer_id: PeerId, response: EncodedProof) {
		if let Err(BadPeer(id, repu)) = self.on_warp_sync_data(&peer_id, response) {
			self.network_service
				.disconnect_peer(id, self.block_announce_protocol_name.clone());
			self.network_service.report_peer(id, repu);
		}
	}

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

	fn on_state_data(
		&mut self,
		who: &PeerId,
		response: OpaqueStateResponse,
	) -> Result<OnStateData<B>, BadPeer> {
		let response: Box<StateResponse> = response.0.downcast().map_err(|_error| {
			error!(
				target: LOG_TARGET,
				"Failed to downcast opaque state response, this is an implementation bug."
			);

			BadPeer(*who, rep::BAD_RESPONSE)
		})?;

		if let Some(peer) = self.peers.get_mut(who) {
			if let PeerSyncState::DownloadingState = peer.state {
				peer.state = PeerSyncState::Available;
				self.allowed_requests.set_all();
			}
		}
		let import_result = if let Some(sync) = &mut self.state_sync {
			debug!(
				target: LOG_TARGET,
				"Importing state data from {} with {} keys, {} proof nodes.",
				who,
				response.entries.len(),
				response.proof.len(),
			);
			sync.import(*response)
		} else if let Some(sync) = &mut self.warp_sync {
			debug!(
				target: LOG_TARGET,
				"Importing state data from {} with {} keys, {} proof nodes.",
				who,
				response.entries.len(),
				response.proof.len(),
			);
			sync.import_state(*response)
		} else {
			debug!(target: LOG_TARGET, "Ignored obsolete state response from {who}");
			return Err(BadPeer(*who, rep::NOT_REQUESTED))
		};

		match import_result {
			state::ImportResult::Import(hash, header, state, body, justifications) => {
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
				Ok(OnStateData::Import(origin, block))
			},
			state::ImportResult::Continue => Ok(OnStateData::Continue),
			state::ImportResult::BadResponse => {
				debug!(target: LOG_TARGET, "Bad state data received from {who}");
				Err(BadPeer(*who, rep::BAD_BLOCK))
			},
		}
	}

	fn on_warp_sync_data(&mut self, who: &PeerId, response: EncodedProof) -> Result<(), BadPeer> {
		if let Some(peer) = self.peers.get_mut(who) {
			if let PeerSyncState::DownloadingWarpProof = peer.state {
				peer.state = PeerSyncState::Available;
				self.allowed_requests.set_all();
			}
		}
		let import_result = if let Some(sync) = &mut self.warp_sync {
			debug!(
				target: LOG_TARGET,
				"Importing warp proof data from {}, {} bytes.",
				who,
				response.0.len(),
			);
			sync.import_warp_proof(response)
		} else {
			debug!(target: LOG_TARGET, "Ignored obsolete warp sync response from {who}");
			return Err(BadPeer(*who, rep::NOT_REQUESTED))
		};

		match import_result {
			WarpProofImportResult::Success => Ok(()),
			WarpProofImportResult::BadResponse => {
				debug!(target: LOG_TARGET, "Bad proof data received from {who}");
				Err(BadPeer(*who, rep::BAD_BLOCK))
			},
		}
	}

	/// A batch of blocks have been processed, with or without errors.
	///
	/// Call this when a batch of blocks have been processed by the import
	/// queue, with or without errors. If an error is returned, the pending response
	/// from the peer must be dropped.
	#[must_use]
	fn on_blocks_processed(
		&mut self,
		imported: usize,
		count: usize,
		results: Vec<(Result<BlockImportStatus<NumberFor<B>>, BlockImportError>, B::Hash)>,
	) -> Box<dyn Iterator<Item = Result<BlockRequestAction<B>, BadPeer>>> {
		trace!(target: LOG_TARGET, "Imported {imported} of {count}");

		let mut output = Vec::new();

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
				Ok(BlockImportStatus::ImportedKnown(number, who)) =>
					if let Some(peer) = who {
						self.update_peer_common_number(&peer, number);
					},
				Ok(BlockImportStatus::ImportedUnknown(number, aux, who)) => {
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
						if let Some(ref peer) = who {
							warn!("💔 Sent block with bad justification to import");
							output.push(Err(BadPeer(*peer, rep::BAD_JUSTIFICATION)));
						}
					}

					if let Some(peer) = who {
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
						output.extend(self.restart());
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
						output.extend(self.restart());
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
				Err(BlockImportError::IncompleteHeader(who)) =>
					if let Some(peer) = who {
						warn!(
							target: LOG_TARGET,
							"💔 Peer sent block with incomplete header to import",
						);
						output.push(Err(BadPeer(peer, rep::INCOMPLETE_HEADER)));
						output.extend(self.restart());
					},
				Err(BlockImportError::VerificationFailed(who, e)) => {
					let extra_message =
						who.map_or_else(|| "".into(), |peer| format!(" received from ({peer})"));

					warn!(
						target: LOG_TARGET,
						"💔 Verification failed for block {hash:?}{extra_message}: {e:?}",
					);

					if let Some(peer) = who {
						output.push(Err(BadPeer(peer, rep::VERIFICATION_FAIL)));
					}

					output.extend(self.restart());
				},
				Err(BlockImportError::BadBlock(who)) =>
					if let Some(peer) = who {
						warn!(
							target: LOG_TARGET,
							"💔 Block {hash:?} received from peer {peer} has been blacklisted",
						);
						output.push(Err(BadPeer(peer, rep::BAD_BLOCK)));
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
					output.extend(self.restart());
				},
				Err(BlockImportError::Cancelled) => {},
			};
		}

		self.allowed_requests.set_all();
		Box::new(output.into_iter())
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
pub enum AncestorSearchState<B: BlockT> {
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
	who: &PeerId,
	request: Option<BlockRequest<Block>>,
) -> Result<Option<NumberFor<Block>>, BadPeer> {
	if let Some(request) = request {
		if Some(blocks.len() as _) > request.max {
			debug!(
				target: LOG_TARGET,
				"Received more blocks than requested from {}. Expected in maximum {:?}, got {}.",
				who,
				request.max,
				blocks.len(),
			);

			return Err(BadPeer(*who, rep::NOT_REQUESTED))
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

			return Err(BadPeer(*who, rep::NOT_REQUESTED))
		}

		if request.fields.contains(BlockAttributes::HEADER) &&
			blocks.iter().any(|b| b.header.is_none())
		{
			trace!(
				target: LOG_TARGET,
				"Missing requested header for a block in response from {who}.",
			);

			return Err(BadPeer(*who, rep::BAD_RESPONSE))
		}

		if request.fields.contains(BlockAttributes::BODY) && blocks.iter().any(|b| b.body.is_none())
		{
			trace!(
				target: LOG_TARGET,
				"Missing requested body for a block in response from {who}.",
			);

			return Err(BadPeer(*who, rep::BAD_RESPONSE))
		}
	}

	for b in blocks {
		if let Some(header) = &b.header {
			let hash = header.hash();
			if hash != b.hash {
				debug!(
					target:LOG_TARGET,
					"Bad header received from {}. Expected hash {:?}, got {:?}",
					who,
					b.hash,
					hash,
				);
				return Err(BadPeer(*who, rep::BAD_BLOCK))
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
					who,
					expected,
					got,
				);
				return Err(BadPeer(*who, rep::BAD_BLOCK))
			}
		}
	}

	Ok(blocks.first().and_then(|b| b.header.as_ref()).map(|h| *h.number()))
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::service::network::NetworkServiceProvider;
	use futures::executor::block_on;
	use sc_block_builder::BlockBuilderProvider;
	use sc_network_common::sync::message::{BlockAnnounce, BlockData, BlockState, FromBlock};
	use sp_blockchain::HeaderBackend;
	use substrate_test_runtime_client::{
		runtime::{Block, Hash, Header},
		BlockBuilderExt, ClientBlockImportExt, ClientExt, DefaultTestClientBuilderExt, TestClient,
		TestClientBuilder, TestClientBuilderExt,
	};

	#[test]
	fn processes_empty_response_on_justification_request_for_unknown_block() {
		// if we ask for a justification for a given block to a peer that doesn't know that block
		// (different from not having a justification), the peer will reply with an empty response.
		// internally we should process the response as the justification not being available.

		let client = Arc::new(TestClientBuilder::new().build());
		let peer_id = PeerId::random();

		let (_chain_sync_network_provider, chain_sync_network_handle) =
			NetworkServiceProvider::new();
		let mut sync = ChainSync::new(
			SyncMode::Full,
			client.clone(),
			ProtocolName::from("test-block-announce-protocol"),
			1,
			64,
			None,
			chain_sync_network_handle,
		)
		.unwrap();

		let (a1_hash, a1_number) = {
			let a1 = client.new_block(Default::default()).unwrap().build().unwrap().block;
			(a1.hash(), *a1.header.number())
		};

		// add a new peer with the same best block
		sync.new_peer(peer_id, a1_hash, a1_number).unwrap();

		// and request a justification for the block
		sync.request_justification(&a1_hash, a1_number);

		// the justification request should be scheduled to that peer
		assert!(sync
			.justification_requests()
			.iter()
			.any(|(who, request)| { *who == peer_id && request.from == FromBlock::Hash(a1_hash) }));

		// there are no extra pending requests
		assert_eq!(sync.extra_justifications.pending_requests().count(), 0);

		// there's one in-flight extra request to the expected peer
		assert!(sync.extra_justifications.active_requests().any(|(who, (hash, number))| {
			*who == peer_id && *hash == a1_hash && *number == a1_number
		}));

		// if the peer replies with an empty response (i.e. it doesn't know the block),
		// the active request should be cleared.
		assert_eq!(
			sync.on_block_justification(peer_id, BlockResponse::<Block> { id: 0, blocks: vec![] }),
			Ok(OnBlockJustification::Nothing),
		);

		// there should be no in-flight requests
		assert_eq!(sync.extra_justifications.active_requests().count(), 0);

		// and the request should now be pending again, waiting for reschedule
		assert!(sync
			.extra_justifications
			.pending_requests()
			.any(|(hash, number)| { *hash == a1_hash && *number == a1_number }));
	}

	#[test]
	fn restart_doesnt_affect_peers_downloading_finality_data() {
		let mut client = Arc::new(TestClientBuilder::new().build());
		let (_chain_sync_network_provider, chain_sync_network_handle) =
			NetworkServiceProvider::new();

		let mut sync = ChainSync::new(
			SyncMode::Full,
			client.clone(),
			ProtocolName::from("test-block-announce-protocol"),
			1,
			64,
			None,
			chain_sync_network_handle,
		)
		.unwrap();

		let peer_id1 = PeerId::random();
		let peer_id2 = PeerId::random();
		let peer_id3 = PeerId::random();

		let mut new_blocks = |n| {
			for _ in 0..n {
				let block = client.new_block(Default::default()).unwrap().build().unwrap().block;
				block_on(client.import(BlockOrigin::Own, block.clone())).unwrap();
			}

			let info = client.info();
			(info.best_hash, info.best_number)
		};

		let (b1_hash, b1_number) = new_blocks(50);

		// add 2 peers at blocks that we don't have locally
		sync.new_peer(peer_id1, Hash::random(), 42).unwrap();
		sync.new_peer(peer_id2, Hash::random(), 10).unwrap();

		// we wil send block requests to these peers
		// for these blocks we don't know about
		assert!(sync
			.block_requests()
			.into_iter()
			.all(|(p, _)| { p == peer_id1 || p == peer_id2 }));

		// add a new peer at a known block
		sync.new_peer(peer_id3, b1_hash, b1_number).unwrap();

		// we request a justification for a block we have locally
		sync.request_justification(&b1_hash, b1_number);

		// the justification request should be scheduled to the
		// new peer which is at the given block
		assert!(sync.justification_requests().iter().any(|(p, r)| {
			*p == peer_id3 &&
				r.fields == BlockAttributes::JUSTIFICATION &&
				r.from == FromBlock::Hash(b1_hash)
		}));

		assert_eq!(
			sync.peers.get(&peer_id3).unwrap().state,
			PeerSyncState::DownloadingJustification(b1_hash),
		);

		// we restart the sync state
		let block_requests = sync.restart();

		// which should make us send out block requests to the first two peers
		assert!(block_requests.map(|r| r.unwrap()).all(|event| match event {
			BlockRequestAction::SendRequest { peer_id, .. } =>
				peer_id == peer_id1 || peer_id == peer_id2,
			BlockRequestAction::RemoveStale { .. } => false,
		}));

		// peer 3 should be unaffected it was downloading finality data
		assert_eq!(
			sync.peers.get(&peer_id3).unwrap().state,
			PeerSyncState::DownloadingJustification(b1_hash),
		);

		// Set common block to something that we don't have (e.g. failed import)
		sync.peers.get_mut(&peer_id3).unwrap().common_number = 100;
		let _ = sync.restart().count();
		assert_eq!(sync.peers.get(&peer_id3).unwrap().common_number, 50);
	}

	/// Send a block annoucnement for the given `header`.
	fn send_block_announce(
		header: Header,
		peer_id: PeerId,
		sync: &mut ChainSync<Block, TestClient>,
	) {
		let announce = BlockAnnounce {
			header: header.clone(),
			state: Some(BlockState::Best),
			data: Some(Vec::new()),
		};

		sync.on_validated_block_announce(true, peer_id, &announce);
	}

	/// Create a block response from the given `blocks`.
	fn create_block_response(blocks: Vec<Block>) -> BlockResponse<Block> {
		BlockResponse::<Block> {
			id: 0,
			blocks: blocks
				.into_iter()
				.map(|b| BlockData::<Block> {
					hash: b.hash(),
					header: Some(b.header().clone()),
					body: Some(b.deconstruct().1),
					indexed_body: None,
					receipt: None,
					message_queue: None,
					justification: None,
					justifications: None,
				})
				.collect(),
		}
	}

	/// Get a block request from `sync` and check that is matches the expected request.
	fn get_block_request(
		sync: &mut ChainSync<Block, TestClient>,
		from: FromBlock<Hash, u64>,
		max: u32,
		peer: &PeerId,
	) -> BlockRequest<Block> {
		let requests = sync.block_requests();

		log::trace!(target: LOG_TARGET, "Requests: {requests:?}");

		assert_eq!(1, requests.len());
		assert_eq!(*peer, requests[0].0);

		let request = requests[0].1.clone();

		assert_eq!(from, request.from);
		assert_eq!(Some(max), request.max);
		request
	}

	/// Build and import a new best block.
	fn build_block(client: &mut Arc<TestClient>, at: Option<Hash>, fork: bool) -> Block {
		let at = at.unwrap_or_else(|| client.info().best_hash);

		let mut block_builder = client.new_block_at(at, Default::default(), false).unwrap();

		if fork {
			block_builder.push_storage_change(vec![1, 2, 3], Some(vec![4, 5, 6])).unwrap();
		}

		let block = block_builder.build().unwrap().block;

		block_on(client.import(BlockOrigin::Own, block.clone())).unwrap();
		block
	}

	/// This test is a regression test as observed on a real network.
	///
	/// The node is connected to multiple peers. Both of these peers are having a best block (1)
	/// that is below our best block (3). Now peer 2 announces a fork of block 3 that we will
	/// request from peer 2. After importing the fork, peer 2 and then peer 1 will announce block 4.
	/// But as peer 1 in our view is still at block 1, we will request block 2 (which we already
	/// have) from it. In the meanwhile peer 2 sends us block 4 and 3 and we send another request
	/// for block 2 to peer 2. Peer 1 answers with block 2 and then peer 2. This will need to
	/// succeed, as we have requested block 2 from both peers.
	#[test]
	fn do_not_report_peer_on_block_response_for_block_request() {
		sp_tracing::try_init_simple();

		let mut client = Arc::new(TestClientBuilder::new().build());
		let (_chain_sync_network_provider, chain_sync_network_handle) =
			NetworkServiceProvider::new();

		let mut sync = ChainSync::new(
			SyncMode::Full,
			client.clone(),
			ProtocolName::from("test-block-announce-protocol"),
			5,
			64,
			None,
			chain_sync_network_handle,
		)
		.unwrap();

		let peer_id1 = PeerId::random();
		let peer_id2 = PeerId::random();

		let mut client2 = client.clone();
		let mut build_block_at = |at, import| {
			let mut block_builder = client2.new_block_at(at, Default::default(), false).unwrap();
			// Make sure we generate a different block as fork
			block_builder.push_storage_change(vec![1, 2, 3], Some(vec![4, 5, 6])).unwrap();

			let block = block_builder.build().unwrap().block;

			if import {
				block_on(client2.import(BlockOrigin::Own, block.clone())).unwrap();
			}

			block
		};

		let block1 = build_block(&mut client, None, false);
		let block2 = build_block(&mut client, None, false);
		let block3 = build_block(&mut client, None, false);
		let block3_fork = build_block_at(block2.hash(), false);

		// Add two peers which are on block 1.
		sync.new_peer(peer_id1, block1.hash(), 1).unwrap();
		sync.new_peer(peer_id2, block1.hash(), 1).unwrap();

		// Tell sync that our best block is 3.
		sync.update_chain_info(&block3.hash(), 3);

		// There should be no requests.
		assert!(sync.block_requests().is_empty());

		// Let peer2 announce a fork of block 3
		send_block_announce(block3_fork.header().clone(), peer_id2, &mut sync);

		// Import and tell sync that we now have the fork.
		block_on(client.import(BlockOrigin::Own, block3_fork.clone())).unwrap();
		sync.update_chain_info(&block3_fork.hash(), 3);

		let block4 = build_block_at(block3_fork.hash(), false);

		// Let peer2 announce block 4 and check that sync wants to get the block.
		send_block_announce(block4.header().clone(), peer_id2, &mut sync);

		let request = get_block_request(&mut sync, FromBlock::Hash(block4.hash()), 2, &peer_id2);

		// Peer1 announces the same block, but as the common block is still `1`, sync will request
		// block 2 again.
		send_block_announce(block4.header().clone(), peer_id1, &mut sync);

		let request2 = get_block_request(&mut sync, FromBlock::Number(2), 1, &peer_id1);

		let response = create_block_response(vec![block4.clone(), block3_fork.clone()]);
		let res = sync.on_block_data(&peer_id2, Some(request), response).unwrap();

		// We should not yet import the blocks, because there is still an open request for fetching
		// block `2` which blocks the import.
		assert!(
			matches!(res, OnBlockData::Import(ImportBlocksAction{ origin: _, blocks }) if blocks.is_empty())
		);

		let request3 = get_block_request(&mut sync, FromBlock::Number(2), 1, &peer_id2);

		let response = create_block_response(vec![block2.clone()]);
		let res = sync.on_block_data(&peer_id1, Some(request2), response).unwrap();
		assert!(matches!(
			res,
			OnBlockData::Import(ImportBlocksAction{ origin: _, blocks })
				if blocks.iter().all(|b| [2, 3, 4].contains(b.header.as_ref().unwrap().number()))
		));

		let response = create_block_response(vec![block2.clone()]);
		let res = sync.on_block_data(&peer_id2, Some(request3), response).unwrap();
		// Nothing to import
		assert!(
			matches!(res, OnBlockData::Import(ImportBlocksAction{ origin: _, blocks }) if blocks.is_empty())
		);
	}

	fn unwrap_from_block_number(from: FromBlock<Hash, u64>) -> u64 {
		if let FromBlock::Number(from) = from {
			from
		} else {
			panic!("Expected a number!");
		}
	}

	/// A regression test for a behavior we have seen on a live network.
	///
	/// The scenario is that the node is doing a full resync and is connected to some node that is
	/// doing a major sync as well. This other node that is doing a major sync will finish before
	/// our node and send a block announcement message, but we don't have seen any block
	/// announcement from this node in its sync process. Meaning our common number didn't change. It
	/// is now expected that we start an ancestor search to find the common number.
	#[test]
	fn do_ancestor_search_when_common_block_to_best_qeued_gap_is_to_big() {
		sp_tracing::try_init_simple();

		let blocks = {
			let mut client = Arc::new(TestClientBuilder::new().build());
			(0..MAX_DOWNLOAD_AHEAD * 2)
				.map(|_| build_block(&mut client, None, false))
				.collect::<Vec<_>>()
		};

		let mut client = Arc::new(TestClientBuilder::new().build());
		let (_chain_sync_network_provider, chain_sync_network_handle) =
			NetworkServiceProvider::new();
		let info = client.info();

		let mut sync = ChainSync::new(
			SyncMode::Full,
			client.clone(),
			ProtocolName::from("test-block-announce-protocol"),
			5,
			64,
			None,
			chain_sync_network_handle,
		)
		.unwrap();

		let peer_id1 = PeerId::random();
		let peer_id2 = PeerId::random();

		let best_block = blocks.last().unwrap().clone();
		let max_blocks_to_request = sync.max_blocks_per_request;
		// Connect the node we will sync from
		sync.new_peer(peer_id1, best_block.hash(), *best_block.header().number())
			.unwrap();
		sync.new_peer(peer_id2, info.best_hash, 0).unwrap();

		let mut best_block_num = 0;
		while best_block_num < MAX_DOWNLOAD_AHEAD {
			let request = get_block_request(
				&mut sync,
				FromBlock::Number(max_blocks_to_request as u64 + best_block_num as u64),
				max_blocks_to_request as u32,
				&peer_id1,
			);

			let from = unwrap_from_block_number(request.from.clone());

			let mut resp_blocks = blocks[best_block_num as usize..from as usize].to_vec();
			resp_blocks.reverse();

			let response = create_block_response(resp_blocks.clone());

			let res = sync.on_block_data(&peer_id1, Some(request), response).unwrap();
			assert!(matches!(
				res,
				OnBlockData::Import(ImportBlocksAction{ origin: _, blocks }) if blocks.len() == max_blocks_to_request as usize
			),);

			best_block_num += max_blocks_to_request as u32;

			let _ = sync.on_blocks_processed(
				max_blocks_to_request as usize,
				max_blocks_to_request as usize,
				resp_blocks
					.iter()
					.rev()
					.map(|b| {
						(
							Ok(BlockImportStatus::ImportedUnknown(
								*b.header().number(),
								Default::default(),
								Some(peer_id1),
							)),
							b.hash(),
						)
					})
					.collect(),
			);

			resp_blocks
				.into_iter()
				.rev()
				.for_each(|b| block_on(client.import_as_final(BlockOrigin::Own, b)).unwrap());
		}

		// "Wait" for the queue to clear
		sync.queue_blocks.clear();

		// Let peer2 announce that it finished syncing
		send_block_announce(best_block.header().clone(), peer_id2, &mut sync);

		let (peer1_req, peer2_req) =
			sync.block_requests().into_iter().fold((None, None), |res, req| {
				if req.0 == peer_id1 {
					(Some(req.1), res.1)
				} else if req.0 == peer_id2 {
					(res.0, Some(req.1))
				} else {
					panic!("Unexpected req: {:?}", req)
				}
			});

		// We should now do an ancestor search to find the correct common block.
		let peer2_req = peer2_req.unwrap();
		assert_eq!(Some(1), peer2_req.max);
		assert_eq!(FromBlock::Number(best_block_num as u64), peer2_req.from);

		let response = create_block_response(vec![blocks[(best_block_num - 1) as usize].clone()]);
		let res = sync.on_block_data(&peer_id2, Some(peer2_req), response).unwrap();
		assert!(matches!(
			res,
			OnBlockData::Import(ImportBlocksAction{ origin: _, blocks }) if blocks.is_empty()
		),);

		let peer1_from = unwrap_from_block_number(peer1_req.unwrap().from);

		// As we are on the same chain, we should directly continue with requesting blocks from
		// peer 2 as well.
		get_block_request(
			&mut sync,
			FromBlock::Number(peer1_from + max_blocks_to_request as u64),
			max_blocks_to_request as u32,
			&peer_id2,
		);
	}

	/// A test that ensures that we can sync a huge fork.
	///
	/// The following scenario:
	/// A peer connects to us and we both have the common block 512. The last finalized is 2048.
	/// Our best block is 4096. The peer send us a block announcement with 4097 from a fork.
	///
	/// We will first do an ancestor search to find the common block. After that we start to sync
	/// the fork and finish it ;)
	#[test]
	fn can_sync_huge_fork() {
		sp_tracing::try_init_simple();

		let (_chain_sync_network_provider, chain_sync_network_handle) =
			NetworkServiceProvider::new();
		let mut client = Arc::new(TestClientBuilder::new().build());
		let blocks = (0..MAX_BLOCKS_TO_LOOK_BACKWARDS * 4)
			.map(|_| build_block(&mut client, None, false))
			.collect::<Vec<_>>();

		let fork_blocks = {
			let mut client = Arc::new(TestClientBuilder::new().build());
			let fork_blocks = blocks[..MAX_BLOCKS_TO_LOOK_BACKWARDS as usize * 2]
				.into_iter()
				.inspect(|b| block_on(client.import(BlockOrigin::Own, (*b).clone())).unwrap())
				.cloned()
				.collect::<Vec<_>>();

			fork_blocks
				.into_iter()
				.chain(
					(0..MAX_BLOCKS_TO_LOOK_BACKWARDS * 2 + 1)
						.map(|_| build_block(&mut client, None, true)),
				)
				.collect::<Vec<_>>()
		};

		let info = client.info();

		let mut sync = ChainSync::new(
			SyncMode::Full,
			client.clone(),
			ProtocolName::from("test-block-announce-protocol"),
			5,
			64,
			None,
			chain_sync_network_handle,
		)
		.unwrap();

		let finalized_block = blocks[MAX_BLOCKS_TO_LOOK_BACKWARDS as usize * 2 - 1].clone();
		let just = (*b"TEST", Vec::new());
		client.finalize_block(finalized_block.hash(), Some(just)).unwrap();
		sync.update_chain_info(&info.best_hash, info.best_number);

		let peer_id1 = PeerId::random();

		let common_block = blocks[MAX_BLOCKS_TO_LOOK_BACKWARDS as usize / 2].clone();
		// Connect the node we will sync from
		sync.new_peer(peer_id1, common_block.hash(), *common_block.header().number())
			.unwrap();

		send_block_announce(fork_blocks.last().unwrap().header().clone(), peer_id1, &mut sync);

		let mut request =
			get_block_request(&mut sync, FromBlock::Number(info.best_number), 1, &peer_id1);

		// Do the ancestor search
		loop {
			let block = &fork_blocks[unwrap_from_block_number(request.from.clone()) as usize - 1];
			let response = create_block_response(vec![block.clone()]);

			let on_block_data = sync.on_block_data(&peer_id1, Some(request), response).unwrap();
			request = if let OnBlockData::Request(_peer, request) = on_block_data {
				request
			} else {
				// We found the ancenstor
				break
			};

			log::trace!(target: LOG_TARGET, "Request: {request:?}");
		}

		// Now request and import the fork.
		let mut best_block_num = *finalized_block.header().number() as u32;
		let max_blocks_to_request = sync.max_blocks_per_request;
		while best_block_num < *fork_blocks.last().unwrap().header().number() as u32 - 1 {
			let request = get_block_request(
				&mut sync,
				FromBlock::Number(max_blocks_to_request as u64 + best_block_num as u64),
				max_blocks_to_request as u32,
				&peer_id1,
			);

			let from = unwrap_from_block_number(request.from.clone());

			let mut resp_blocks = fork_blocks[best_block_num as usize..from as usize].to_vec();
			resp_blocks.reverse();

			let response = create_block_response(resp_blocks.clone());

			let res = sync.on_block_data(&peer_id1, Some(request), response).unwrap();
			assert!(matches!(
				res,
				OnBlockData::Import(ImportBlocksAction{ origin: _, blocks }) if blocks.len() == sync.max_blocks_per_request as usize
			),);

			best_block_num += sync.max_blocks_per_request as u32;

			let _ = sync.on_blocks_processed(
				max_blocks_to_request as usize,
				max_blocks_to_request as usize,
				resp_blocks
					.iter()
					.rev()
					.map(|b| {
						(
							Ok(BlockImportStatus::ImportedUnknown(
								*b.header().number(),
								Default::default(),
								Some(peer_id1),
							)),
							b.hash(),
						)
					})
					.collect(),
			);

			resp_blocks
				.into_iter()
				.rev()
				.for_each(|b| block_on(client.import(BlockOrigin::Own, b)).unwrap());
		}

		// Request the tip
		get_block_request(
			&mut sync,
			FromBlock::Hash(fork_blocks.last().unwrap().hash()),
			1,
			&peer_id1,
		);
	}

	#[test]
	fn syncs_fork_without_duplicate_requests() {
		sp_tracing::try_init_simple();

		let (_chain_sync_network_provider, chain_sync_network_handle) =
			NetworkServiceProvider::new();
		let mut client = Arc::new(TestClientBuilder::new().build());
		let blocks = (0..MAX_BLOCKS_TO_LOOK_BACKWARDS * 4)
			.map(|_| build_block(&mut client, None, false))
			.collect::<Vec<_>>();

		let fork_blocks = {
			let mut client = Arc::new(TestClientBuilder::new().build());
			let fork_blocks = blocks[..MAX_BLOCKS_TO_LOOK_BACKWARDS as usize * 2]
				.into_iter()
				.inspect(|b| block_on(client.import(BlockOrigin::Own, (*b).clone())).unwrap())
				.cloned()
				.collect::<Vec<_>>();

			fork_blocks
				.into_iter()
				.chain(
					(0..MAX_BLOCKS_TO_LOOK_BACKWARDS * 2 + 1)
						.map(|_| build_block(&mut client, None, true)),
				)
				.collect::<Vec<_>>()
		};

		let info = client.info();

		let mut sync = ChainSync::new(
			SyncMode::Full,
			client.clone(),
			ProtocolName::from("test-block-announce-protocol"),
			5,
			64,
			None,
			chain_sync_network_handle,
		)
		.unwrap();

		let finalized_block = blocks[MAX_BLOCKS_TO_LOOK_BACKWARDS as usize * 2 - 1].clone();
		let just = (*b"TEST", Vec::new());
		client.finalize_block(finalized_block.hash(), Some(just)).unwrap();
		sync.update_chain_info(&info.best_hash, info.best_number);

		let peer_id1 = PeerId::random();

		let common_block = blocks[MAX_BLOCKS_TO_LOOK_BACKWARDS as usize / 2].clone();
		// Connect the node we will sync from
		sync.new_peer(peer_id1, common_block.hash(), *common_block.header().number())
			.unwrap();

		send_block_announce(fork_blocks.last().unwrap().header().clone(), peer_id1, &mut sync);

		let mut request =
			get_block_request(&mut sync, FromBlock::Number(info.best_number), 1, &peer_id1);

		// Do the ancestor search
		loop {
			let block = &fork_blocks[unwrap_from_block_number(request.from.clone()) as usize - 1];
			let response = create_block_response(vec![block.clone()]);

			let on_block_data = sync.on_block_data(&peer_id1, Some(request), response).unwrap();
			request = if let OnBlockData::Request(_peer, request) = on_block_data {
				request
			} else {
				// We found the ancenstor
				break
			};

			log::trace!(target: LOG_TARGET, "Request: {request:?}");
		}

		// Now request and import the fork.
		let mut best_block_num = *finalized_block.header().number() as u32;
		let max_blocks_to_request = sync.max_blocks_per_request;

		let mut request = get_block_request(
			&mut sync,
			FromBlock::Number(max_blocks_to_request as u64 + best_block_num as u64),
			max_blocks_to_request as u32,
			&peer_id1,
		);
		let last_block_num = *fork_blocks.last().unwrap().header().number() as u32 - 1;
		while best_block_num < last_block_num {
			let from = unwrap_from_block_number(request.from.clone());

			let mut resp_blocks = fork_blocks[best_block_num as usize..from as usize].to_vec();
			resp_blocks.reverse();

			let response = create_block_response(resp_blocks.clone());

			let res = sync.on_block_data(&peer_id1, Some(request.clone()), response).unwrap();
			assert!(matches!(
				res,
				OnBlockData::Import(ImportBlocksAction{ origin: _, blocks }) if blocks.len() == max_blocks_to_request as usize
			),);

			best_block_num += max_blocks_to_request as u32;

			if best_block_num < last_block_num {
				// make sure we're not getting a duplicate request in the time before the blocks are
				// processed
				request = get_block_request(
					&mut sync,
					FromBlock::Number(max_blocks_to_request as u64 + best_block_num as u64),
					max_blocks_to_request as u32,
					&peer_id1,
				);
			}

			let mut notify_imported: Vec<_> = resp_blocks
				.iter()
				.rev()
				.map(|b| {
					(
						Ok(BlockImportStatus::ImportedUnknown(
							*b.header().number(),
							Default::default(),
							Some(peer_id1),
						)),
						b.hash(),
					)
				})
				.collect();

			// The import queue may send notifications in batches of varying size. So we simulate
			// this here by splitting the batch into 2 notifications.
			let max_blocks_to_request = sync.max_blocks_per_request;
			let second_batch = notify_imported.split_off(notify_imported.len() / 2);
			let _ = sync.on_blocks_processed(
				max_blocks_to_request as usize,
				max_blocks_to_request as usize,
				notify_imported,
			);

			let _ = sync.on_blocks_processed(
				max_blocks_to_request as usize,
				max_blocks_to_request as usize,
				second_batch,
			);

			resp_blocks
				.into_iter()
				.rev()
				.for_each(|b| block_on(client.import(BlockOrigin::Own, b)).unwrap());
		}

		// Request the tip
		get_block_request(
			&mut sync,
			FromBlock::Hash(fork_blocks.last().unwrap().hash()),
			1,
			&peer_id1,
		);
	}

	#[test]
	fn removes_target_fork_on_disconnect() {
		sp_tracing::try_init_simple();
		let (_chain_sync_network_provider, chain_sync_network_handle) =
			NetworkServiceProvider::new();
		let mut client = Arc::new(TestClientBuilder::new().build());
		let blocks = (0..3).map(|_| build_block(&mut client, None, false)).collect::<Vec<_>>();

		let mut sync = ChainSync::new(
			SyncMode::Full,
			client.clone(),
			ProtocolName::from("test-block-announce-protocol"),
			1,
			64,
			None,
			chain_sync_network_handle,
		)
		.unwrap();

		let peer_id1 = PeerId::random();
		let common_block = blocks[1].clone();
		// Connect the node we will sync from
		sync.new_peer(peer_id1, common_block.hash(), *common_block.header().number())
			.unwrap();

		// Create a "new" header and announce it
		let mut header = blocks[0].header().clone();
		header.number = 4;
		send_block_announce(header, peer_id1, &mut sync);
		assert!(sync.fork_targets.len() == 1);

		let _ = sync.peer_disconnected(&peer_id1);
		assert!(sync.fork_targets.len() == 0);
	}

	#[test]
	fn can_import_response_with_missing_blocks() {
		sp_tracing::try_init_simple();
		let (_chain_sync_network_provider, chain_sync_network_handle) =
			NetworkServiceProvider::new();
		let mut client2 = Arc::new(TestClientBuilder::new().build());
		let blocks = (0..4).map(|_| build_block(&mut client2, None, false)).collect::<Vec<_>>();

		let empty_client = Arc::new(TestClientBuilder::new().build());

		let mut sync = ChainSync::new(
			SyncMode::Full,
			empty_client.clone(),
			ProtocolName::from("test-block-announce-protocol"),
			1,
			64,
			None,
			chain_sync_network_handle,
		)
		.unwrap();

		let peer_id1 = PeerId::random();
		let best_block = blocks[3].clone();
		sync.new_peer(peer_id1, best_block.hash(), *best_block.header().number())
			.unwrap();

		sync.peers.get_mut(&peer_id1).unwrap().state = PeerSyncState::Available;
		sync.peers.get_mut(&peer_id1).unwrap().common_number = 0;

		// Request all missing blocks and respond only with some.
		let request =
			get_block_request(&mut sync, FromBlock::Hash(best_block.hash()), 4, &peer_id1);
		let response =
			create_block_response(vec![blocks[3].clone(), blocks[2].clone(), blocks[1].clone()]);
		sync.on_block_data(&peer_id1, Some(request.clone()), response).unwrap();
		assert_eq!(sync.best_queued_number, 0);

		// Request should only contain the missing block.
		let request = get_block_request(&mut sync, FromBlock::Number(1), 1, &peer_id1);
		let response = create_block_response(vec![blocks[0].clone()]);
		sync.on_block_data(&peer_id1, Some(request), response).unwrap();
		assert_eq!(sync.best_queued_number, 4);
	}
	#[test]
	fn ancestor_search_repeat() {
		let state = AncestorSearchState::<Block>::BinarySearch(1, 3);
		assert!(handle_ancestor_search_state(&state, 2, true).is_none());
	}

	#[test]
	fn sync_restart_removes_block_but_not_justification_requests() {
		let mut client = Arc::new(TestClientBuilder::new().build());
		let (_chain_sync_network_provider, chain_sync_network_handle) =
			NetworkServiceProvider::new();
		let mut sync = ChainSync::new(
			SyncMode::Full,
			client.clone(),
			ProtocolName::from("test-block-announce-protocol"),
			1,
			64,
			None,
			chain_sync_network_handle,
		)
		.unwrap();

		let peers = vec![PeerId::random(), PeerId::random()];

		let mut new_blocks = |n| {
			for _ in 0..n {
				let block = client.new_block(Default::default()).unwrap().build().unwrap().block;
				block_on(client.import(BlockOrigin::Own, block.clone())).unwrap();
			}

			let info = client.info();
			(info.best_hash, info.best_number)
		};

		let (b1_hash, b1_number) = new_blocks(50);

		// add new peer and request blocks from them
		sync.new_peer(peers[0], Hash::random(), 42).unwrap();

		// we don't actually perform any requests, just keep track of peers waiting for a response
		let mut pending_responses = HashSet::new();

		// we wil send block requests to these peers
		// for these blocks we don't know about
		for (peer, _request) in sync.block_requests() {
			// "send" request
			pending_responses.insert(peer);
		}

		// add a new peer at a known block
		sync.new_peer(peers[1], b1_hash, b1_number).unwrap();

		// we request a justification for a block we have locally
		sync.request_justification(&b1_hash, b1_number);

		// the justification request should be scheduled to the
		// new peer which is at the given block
		let mut requests = sync.justification_requests();
		assert_eq!(requests.len(), 1);
		let (peer, _request) = requests.remove(0);
		// "send" request
		assert!(pending_responses.insert(peer));

		assert!(!std::matches!(
			sync.peers.get(&peers[0]).unwrap().state,
			PeerSyncState::DownloadingJustification(_),
		));
		assert_eq!(
			sync.peers.get(&peers[1]).unwrap().state,
			PeerSyncState::DownloadingJustification(b1_hash),
		);
		assert_eq!(pending_responses.len(), 2);

		// restart sync
		let request_events = sync.restart().collect::<Vec<_>>();
		for event in request_events.iter() {
			match event.as_ref().unwrap() {
				BlockRequestAction::RemoveStale { peer_id } => {
					pending_responses.remove(&peer_id);
				},
				BlockRequestAction::SendRequest { peer_id, .. } => {
					// we drop obsolete response, but don't register a new request, it's checked in
					// the `assert!` below
					pending_responses.remove(&peer_id);
				},
			}
		}
		assert!(request_events.iter().any(|event| {
			match event.as_ref().unwrap() {
				BlockRequestAction::RemoveStale { .. } => false,
				BlockRequestAction::SendRequest { peer_id, .. } => peer_id == &peers[0],
			}
		}));

		assert_eq!(pending_responses.len(), 1);
		assert!(pending_responses.contains(&peers[1]));
		assert_eq!(
			sync.peers.get(&peers[1]).unwrap().state,
			PeerSyncState::DownloadingJustification(b1_hash),
		);
		let _ = sync.peer_disconnected(&peers[1]);
		pending_responses.remove(&peers[1]);
		assert_eq!(pending_responses.len(), 0);
	}

	#[test]
	fn syncs_fork_with_partial_response_extends_tip() {
		sp_tracing::try_init_simple();

		// Set up: the two chains share the first 15 blocks before
		// diverging. The other(canonical) chain fork is longer.
		let max_blocks_per_request = 64;
		let common_ancestor = 15;
		let non_canonical_chain_length = common_ancestor + 3;
		let canonical_chain_length = common_ancestor + max_blocks_per_request + 10;

		let (_chain_sync_network_provider, chain_sync_network_handle) =
			NetworkServiceProvider::new();
		let mut client = Arc::new(TestClientBuilder::new().build());

		// Blocks on the non-canonical chain.
		let non_canonical_blocks = (0..non_canonical_chain_length)
			.map(|_| build_block(&mut client, None, false))
			.collect::<Vec<_>>();

		// Blocks on the canonical chain.
		let canonical_blocks = {
			let mut client = Arc::new(TestClientBuilder::new().build());
			let common_blocks = non_canonical_blocks[..common_ancestor as usize]
				.into_iter()
				.inspect(|b| block_on(client.import(BlockOrigin::Own, (*b).clone())).unwrap())
				.cloned()
				.collect::<Vec<_>>();

			common_blocks
				.into_iter()
				.chain(
					(0..(canonical_chain_length - common_ancestor as u32))
						.map(|_| build_block(&mut client, None, true)),
				)
				.collect::<Vec<_>>()
		};

		let mut sync = ChainSync::new(
			SyncMode::Full,
			client.clone(),
			ProtocolName::from("test-block-announce-protocol"),
			1,
			max_blocks_per_request,
			None,
			chain_sync_network_handle,
		)
		.unwrap();

		// Connect the node we will sync from
		let peer_id = PeerId::random();
		let canonical_tip = canonical_blocks.last().unwrap().clone();
		let mut request = sync
			.new_peer(peer_id, canonical_tip.hash(), *canonical_tip.header().number())
			.unwrap()
			.unwrap();
		assert_eq!(FromBlock::Number(client.info().best_number), request.from);
		assert_eq!(Some(1), request.max);

		// Do the ancestor search
		loop {
			let block =
				&canonical_blocks[unwrap_from_block_number(request.from.clone()) as usize - 1];
			let response = create_block_response(vec![block.clone()]);

			let on_block_data = sync.on_block_data(&peer_id, Some(request), response).unwrap();
			request = if let OnBlockData::Request(_peer, request) = on_block_data {
				request
			} else {
				// We found the ancestor
				break
			};

			log::trace!(target: LOG_TARGET, "Request: {request:?}");
		}

		// The response for the 64 blocks is returned in two parts:
		// part 1: last 61 blocks [19..79], part 2: first 3 blocks [16-18].
		// Even though the  first part extends the current chain ending at 18,
		// it should not result in an import yet.
		let resp_1_from = common_ancestor as u64 + max_blocks_per_request as u64;
		let resp_2_from = common_ancestor as u64 + 3;

		// No import expected.
		let request = get_block_request(
			&mut sync,
			FromBlock::Number(resp_1_from),
			max_blocks_per_request as u32,
			&peer_id,
		);

		let from = unwrap_from_block_number(request.from.clone());
		let mut resp_blocks = canonical_blocks[18..from as usize].to_vec();
		resp_blocks.reverse();
		let response = create_block_response(resp_blocks.clone());
		let res = sync.on_block_data(&peer_id, Some(request), response).unwrap();
		assert!(matches!(
			res,
			OnBlockData::Import(ImportBlocksAction{ origin: _, blocks }) if blocks.is_empty()
		),);

		// Gap filled, expect max_blocks_per_request being imported now.
		let request = get_block_request(&mut sync, FromBlock::Number(resp_2_from), 3, &peer_id);
		let mut resp_blocks = canonical_blocks[common_ancestor as usize..18].to_vec();
		resp_blocks.reverse();
		let response = create_block_response(resp_blocks.clone());
		let res = sync.on_block_data(&peer_id, Some(request), response).unwrap();
		let to_import: Vec<_> = match &res {
			OnBlockData::Import(ImportBlocksAction { origin: _, blocks }) => {
				assert_eq!(blocks.len(), sync.max_blocks_per_request as usize);
				blocks
					.iter()
					.map(|b| {
						let num = *b.header.as_ref().unwrap().number() as usize;
						canonical_blocks[num - 1].clone()
					})
					.collect()
			},
			_ => {
				panic!("Unexpected response: {res:?}");
			},
		};

		let _ = sync.on_blocks_processed(
			max_blocks_per_request as usize,
			resp_blocks.len(),
			resp_blocks
				.iter()
				.rev()
				.map(|b| {
					(
						Ok(BlockImportStatus::ImportedUnknown(
							*b.header().number(),
							Default::default(),
							Some(peer_id),
						)),
						b.hash(),
					)
				})
				.collect(),
		);
		to_import.into_iter().for_each(|b| {
			assert!(matches!(client.block(*b.header.parent_hash()), Ok(Some(_))));
			block_on(client.import(BlockOrigin::Own, b)).unwrap();
		});
		let expected_number = common_ancestor as u32 + max_blocks_per_request as u32;
		assert_eq!(sync.best_queued_number as u32, expected_number);
		assert_eq!(sync.best_queued_hash, canonical_blocks[expected_number as usize - 1].hash());
		// Sync rest of the chain.
		let request =
			get_block_request(&mut sync, FromBlock::Hash(canonical_tip.hash()), 10_u32, &peer_id);
		let mut resp_blocks = canonical_blocks
			[(canonical_chain_length - 10) as usize..canonical_chain_length as usize]
			.to_vec();
		resp_blocks.reverse();
		let response = create_block_response(resp_blocks.clone());
		let res = sync.on_block_data(&peer_id, Some(request), response).unwrap();
		assert!(matches!(
			res,
			OnBlockData::Import(ImportBlocksAction{ origin: _, blocks }) if blocks.len() == 10 as usize
		),);
		let _ = sync.on_blocks_processed(
			max_blocks_per_request as usize,
			resp_blocks.len(),
			resp_blocks
				.iter()
				.rev()
				.map(|b| {
					(
						Ok(BlockImportStatus::ImportedUnknown(
							*b.header().number(),
							Default::default(),
							Some(peer_id),
						)),
						b.hash(),
					)
				})
				.collect(),
		);
		resp_blocks.into_iter().rev().for_each(|b| {
			assert!(matches!(client.block(*b.header.parent_hash()), Ok(Some(_))));
			block_on(client.import(BlockOrigin::Own, b)).unwrap();
		});
		let expected_number = canonical_chain_length as u32;
		assert_eq!(sync.best_queued_number as u32, expected_number);
		assert_eq!(sync.best_queued_hash, canonical_blocks[expected_number as usize - 1].hash());
	}

	#[test]
	fn syncs_fork_with_partial_response_does_not_extend_tip() {
		sp_tracing::try_init_simple();

		// Set up: the two chains share the first 15 blocks before
		// diverging. The other(canonical) chain fork is longer.
		let max_blocks_per_request = 64;
		let common_ancestor = 15;
		let non_canonical_chain_length = common_ancestor + 3;
		let canonical_chain_length = common_ancestor + max_blocks_per_request + 10;

		let (_chain_sync_network_provider, chain_sync_network_handle) =
			NetworkServiceProvider::new();
		let mut client = Arc::new(TestClientBuilder::new().build());

		// Blocks on the non-canonical chain.
		let non_canonical_blocks = (0..non_canonical_chain_length)
			.map(|_| build_block(&mut client, None, false))
			.collect::<Vec<_>>();

		// Blocks on the canonical chain.
		let canonical_blocks = {
			let mut client = Arc::new(TestClientBuilder::new().build());
			let common_blocks = non_canonical_blocks[..common_ancestor as usize]
				.into_iter()
				.inspect(|b| block_on(client.import(BlockOrigin::Own, (*b).clone())).unwrap())
				.cloned()
				.collect::<Vec<_>>();

			common_blocks
				.into_iter()
				.chain(
					(0..(canonical_chain_length - common_ancestor as u32))
						.map(|_| build_block(&mut client, None, true)),
				)
				.collect::<Vec<_>>()
		};

		let mut sync = ChainSync::new(
			SyncMode::Full,
			client.clone(),
			ProtocolName::from("test-block-announce-protocol"),
			1,
			max_blocks_per_request,
			None,
			chain_sync_network_handle,
		)
		.unwrap();

		// Connect the node we will sync from
		let peer_id = PeerId::random();
		let canonical_tip = canonical_blocks.last().unwrap().clone();
		let mut request = sync
			.new_peer(peer_id, canonical_tip.hash(), *canonical_tip.header().number())
			.unwrap()
			.unwrap();
		assert_eq!(FromBlock::Number(client.info().best_number), request.from);
		assert_eq!(Some(1), request.max);

		// Do the ancestor search
		loop {
			let block =
				&canonical_blocks[unwrap_from_block_number(request.from.clone()) as usize - 1];
			let response = create_block_response(vec![block.clone()]);

			let on_block_data = sync.on_block_data(&peer_id, Some(request), response).unwrap();
			request = if let OnBlockData::Request(_peer, request) = on_block_data {
				request
			} else {
				// We found the ancestor
				break
			};

			log::trace!(target: LOG_TARGET, "Request: {request:?}");
		}

		// The response for the 64 blocks is returned in two parts:
		// part 1: last 62 blocks [18..79], part 2: first 2 blocks [16-17].
		// Even though the  first part extends the current chain ending at 18,
		// it should not result in an import yet.
		let resp_1_from = common_ancestor as u64 + max_blocks_per_request as u64;
		let resp_2_from = common_ancestor as u64 + 2;

		// No import expected.
		let request = get_block_request(
			&mut sync,
			FromBlock::Number(resp_1_from),
			max_blocks_per_request as u32,
			&peer_id,
		);

		let from = unwrap_from_block_number(request.from.clone());
		let mut resp_blocks = canonical_blocks[17..from as usize].to_vec();
		resp_blocks.reverse();
		let response = create_block_response(resp_blocks.clone());
		let res = sync.on_block_data(&peer_id, Some(request), response).unwrap();
		assert!(matches!(
			res,
			OnBlockData::Import(ImportBlocksAction{ origin: _, blocks }) if blocks.is_empty()
		),);

		// Gap filled, expect max_blocks_per_request being imported now.
		let request = get_block_request(&mut sync, FromBlock::Number(resp_2_from), 2, &peer_id);
		let mut resp_blocks = canonical_blocks[common_ancestor as usize..17].to_vec();
		resp_blocks.reverse();
		let response = create_block_response(resp_blocks.clone());
		let res = sync.on_block_data(&peer_id, Some(request), response).unwrap();
		let to_import: Vec<_> = match &res {
			OnBlockData::Import(ImportBlocksAction { origin: _, blocks }) => {
				assert_eq!(blocks.len(), sync.max_blocks_per_request as usize);
				blocks
					.iter()
					.map(|b| {
						let num = *b.header.as_ref().unwrap().number() as usize;
						canonical_blocks[num - 1].clone()
					})
					.collect()
			},
			_ => {
				panic!("Unexpected response: {res:?}");
			},
		};

		let _ = sync.on_blocks_processed(
			max_blocks_per_request as usize,
			resp_blocks.len(),
			resp_blocks
				.iter()
				.rev()
				.map(|b| {
					(
						Ok(BlockImportStatus::ImportedUnknown(
							*b.header().number(),
							Default::default(),
							Some(peer_id),
						)),
						b.hash(),
					)
				})
				.collect(),
		);
		to_import.into_iter().for_each(|b| {
			assert!(matches!(client.block(*b.header.parent_hash()), Ok(Some(_))));
			block_on(client.import(BlockOrigin::Own, b)).unwrap();
		});
		let expected_number = common_ancestor as u32 + max_blocks_per_request as u32;
		assert_eq!(sync.best_queued_number as u32, expected_number);
		assert_eq!(sync.best_queued_hash, canonical_blocks[expected_number as usize - 1].hash());
		// Sync rest of the chain.
		let request =
			get_block_request(&mut sync, FromBlock::Hash(canonical_tip.hash()), 10_u32, &peer_id);
		let mut resp_blocks = canonical_blocks
			[(canonical_chain_length - 10) as usize..canonical_chain_length as usize]
			.to_vec();
		resp_blocks.reverse();
		let response = create_block_response(resp_blocks.clone());
		let res = sync.on_block_data(&peer_id, Some(request), response).unwrap();
		assert!(matches!(
			res,
			OnBlockData::Import(ImportBlocksAction{ origin: _, blocks }) if blocks.len() == 10 as usize
		),);
		let _ = sync.on_blocks_processed(
			max_blocks_per_request as usize,
			resp_blocks.len(),
			resp_blocks
				.iter()
				.rev()
				.map(|b| {
					(
						Ok(BlockImportStatus::ImportedUnknown(
							*b.header().number(),
							Default::default(),
							Some(peer_id),
						)),
						b.hash(),
					)
				})
				.collect(),
		);
		resp_blocks.into_iter().rev().for_each(|b| {
			assert!(matches!(client.block(*b.header.parent_hash()), Ok(Some(_))));
			block_on(client.import(BlockOrigin::Own, b)).unwrap();
		});
		let expected_number = canonical_chain_length as u32;
		assert_eq!(sync.best_queued_number as u32, expected_number);
		assert_eq!(sync.best_queued_hash, canonical_blocks[expected_number as usize - 1].hash());
	}
}
