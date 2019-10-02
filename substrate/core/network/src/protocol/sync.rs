// Copyright 2017-2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.
//
// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

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
//!

use blocks::BlockCollection;
use client::{ClientInfo, error::Error as ClientError};
use consensus::{BlockOrigin, BlockStatus,
	block_validation::{BlockAnnounceValidator, Validation},
	import_queue::{IncomingBlock, BlockImportResult, BlockImportError}
};
use crate::{
	config::{Roles, BoxFinalityProofRequestBuilder},
	message::{self, generic::FinalityProofRequest, BlockAnnounce, BlockAttributes, BlockRequest, BlockResponse,
	FinalityProofResponse},
	protocol
};
use either::Either;
use extra_requests::ExtraRequests;
use libp2p::PeerId;
use log::{debug, trace, warn, info, error};
use sr_primitives::{
	Justification,
	generic::BlockId,
	traits::{Block as BlockT, Header, NumberFor, Zero, One, CheckedSub, SaturatedConversion}
};
use std::{fmt, ops::Range, collections::{HashMap, HashSet, VecDeque}, sync::Arc};

mod blocks;
mod extra_requests;

/// Maximum blocks to request in a single packet.
const MAX_BLOCKS_TO_REQUEST: usize = 128;

/// Maximum blocks to store in the import queue.
const MAX_IMPORTING_BLOCKS: usize = 2048;

/// We use a heuristic that with a high likelihood, by the time
/// `MAJOR_SYNC_BLOCKS` have been imported we'll be on the same
/// chain as (or at least closer to) the peer so we want to delay
/// the ancestor search to not waste time doing that when we are
/// so far behind.
const MAJOR_SYNC_BLOCKS: u8 = 5;

/// Number of recently announced blocks to track for each peer.
const ANNOUNCE_HISTORY_SIZE: usize = 64;

/// Max number of blocks to download for unknown forks.
const MAX_UNKNOWN_FORK_DOWNLOAD_LEN: u32 = 32;

/// Reputation change when a peer sent us a status message that led to a
/// database read error.
const BLOCKCHAIN_STATUS_READ_ERROR_REPUTATION_CHANGE: i32 = -(1 << 16);

/// Reputation change when a peer failed to answer our legitimate ancestry
/// block search.
const ANCESTRY_BLOCK_ERROR_REPUTATION_CHANGE: i32 = -(1 << 9);

/// Reputation change when a peer sent us a status message with a different
/// genesis than us.
const GENESIS_MISMATCH_REPUTATION_CHANGE: i32 = i32::min_value() + 1;

/// Reputation change for peers which send us a block with an incomplete header.
const INCOMPLETE_HEADER_REPUTATION_CHANGE: i32 = -(1 << 20);

/// Reputation change for peers which send us a block which we fail to verify.
const VERIFICATION_FAIL_REPUTATION_CHANGE: i32 = -(1 << 20);

/// Reputation change for peers which send us a bad block.
const BAD_BLOCK_REPUTATION_CHANGE: i32 = -(1 << 29);

/// Reputation change for peers which send us a block with bad justifications.
const BAD_JUSTIFICATION_REPUTATION_CHANGE: i32 = -(1 << 16);

/// The main data structure which contains all the state for a chains
/// active syncing strategy.
pub struct ChainSync<B: BlockT> {
	/// Chain client.
	client: Arc<dyn crate::chain::Client<B>>,
	/// The active peers that we are using to sync and their PeerSync status
	peers: HashMap<PeerId, PeerSync<B>>,
	/// A `BlockCollection` of blocks that are being downloaded from peers
	blocks: BlockCollection<B>,
	/// The best block number in our queue of blocks to import
	best_queued_number: NumberFor<B>,
	/// The best block hash in our queue of blocks to import
	best_queued_hash: B::Hash,
	/// The role of this node, e.g. light or full
	role: Roles,
	/// What block attributes we require for this node, usually derived from
	/// what role we are, but could be customized
	required_block_attributes: message::BlockAttributes,
	/// Any extra finality proof requests.
	extra_finality_proofs: ExtraRequests<B>,
	/// Any extra justification requests.
	extra_justifications: ExtraRequests<B>,
	/// A set of hashes of blocks that are being downloaded or have been
	/// downloaded and are queued for import.
	queue_blocks: HashSet<B::Hash>,
	/// The best block number that we are currently importing.
	best_importing_number: NumberFor<B>,
	/// Finality proof handler.
	request_builder: Option<BoxFinalityProofRequestBuilder<B>>,
	/// Explicit sync requests.
	sync_requests: HashMap<B::Hash, SyncRequest<B>>,
	/// A flag that caches idle state with no pending requests.
	is_idle: bool,
	/// A type to check incoming block announcements.
	block_announce_validator: Box<dyn BlockAnnounceValidator<B> + Send>
}

/// All the data we have about a Peer that we are trying to sync with
#[derive(Debug, Clone)]
pub struct PeerSync<B: BlockT> {
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
	/// A queue of blocks that this peer has announced to us, should only
	/// contain `ANNOUNCE_HISTORY_SIZE` entries.
	pub recently_announced: VecDeque<B::Hash>
}

/// The sync status of a peer we are trying to sync with
#[derive(Debug)]
pub struct PeerInfo<B: BlockT> {
	/// Their best block hash.
	pub best_hash: B::Hash,
	/// Their best block number.
	pub best_number: NumberFor<B>
}

struct SyncRequest<B: BlockT> {
	number: NumberFor<B>,
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
	AncestorSearch(NumberFor<B>, AncestorSearchState<B>),
	/// Actively downloading new blocks, starting from the given Number.
	DownloadingNew(NumberFor<B>),
	/// Downloading a stale block with given Hash. Stale means that it is a
	/// block with a number that is lower than our best number. It might be
	/// from a fork and not necessarily already imported.
	DownloadingStale(B::Hash),
	/// Downloading justification for given block hash.
	DownloadingJustification(B::Hash),
	/// Downloading finality proof for given block hash.
	DownloadingFinalityProof(B::Hash)
}

impl<B: BlockT> PeerSyncState<B> {
	pub fn is_available(&self) -> bool {
		if let PeerSyncState::Available = self {
			true
		} else {
			false
		}
	}
}

/// Reported sync state.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum SyncState {
	/// Initial sync is complete, keep-up sync is active.
	Idle,
	/// Actively catching up with the chain.
	Downloading
}

/// Syncing status and statistics.
#[derive(Clone)]
pub struct Status<B: BlockT> {
	/// Current global sync state.
	pub state: SyncState,
	/// Target sync block number.
	pub best_seen_block: Option<NumberFor<B>>,
	/// Number of peers participating in syncing.
	pub num_peers: u32,
	/// Number of blocks queued for import
	pub queued_blocks: u32,
}

/// A peer did not behave as expected and should be reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadPeer(pub PeerId, pub i32);

impl fmt::Display for BadPeer {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "bad peer {}; reputation change: {}", self.0, self.1)
	}
}

impl std::error::Error for BadPeer {}

/// Result of [`ChainSync::on_block_data`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnBlockData<B: BlockT> {
	/// The block should be imported.
	Import(BlockOrigin, Vec<IncomingBlock<B>>),
	/// A new block request needs to be made to the given peer.
	Request(PeerId, BlockRequest<B>)
}

/// Result of [`ChainSync::on_block_announce`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnBlockAnnounce<B: BlockT> {
	/// The announcement does not require further handling.
	Nothing,
	/// The announcement header should be imported.
	ImportHeader,
	/// Another block request to the given peer is necessary.
	Request(PeerId, BlockRequest<B>)
}

/// Result of [`ChainSync::on_block_justification`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnBlockJustification<B: BlockT> {
	/// The justification needs no further handling.
	Nothing,
	/// The justification should be imported.
	Import {
		peer: PeerId,
		hash: B::Hash,
		number: NumberFor<B>,
		justification: Justification
	}
}

/// Result of [`ChainSync::on_block_finality_proof`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnBlockFinalityProof<B: BlockT> {
	/// The proof needs no further handling.
	Nothing,
	/// The proof should be imported.
	Import {
		peer: PeerId,
		hash: B::Hash,
		number: NumberFor<B>,
		proof: Vec<u8>
	}
}

impl<B: BlockT> ChainSync<B> {
	/// Create a new instance.
	pub fn new(
		role: Roles,
		client: Arc<dyn crate::chain::Client<B>>,
		info: &ClientInfo<B>,
		request_builder: Option<BoxFinalityProofRequestBuilder<B>>,
		block_announce_validator: Box<dyn BlockAnnounceValidator<B> + Send>
	) -> Self {
		let mut required_block_attributes = BlockAttributes::HEADER | BlockAttributes::JUSTIFICATION;

		if role.is_full() {
			required_block_attributes |= BlockAttributes::BODY
		}

		ChainSync {
			client,
			peers: HashMap::new(),
			blocks: BlockCollection::new(),
			best_queued_hash: info.chain.best_hash,
			best_queued_number: info.chain.best_number,
			extra_finality_proofs: ExtraRequests::new(),
			extra_justifications: ExtraRequests::new(),
			role,
			required_block_attributes,
			queue_blocks: Default::default(),
			best_importing_number: Zero::zero(),
			request_builder,
			sync_requests: Default::default(),
			is_idle: false,
			block_announce_validator,
		}
	}

	/// Returns the state of the sync of the given peer.
	///
	/// Returns `None` if the peer is unknown.
	pub fn peer_info(&self, who: &PeerId) -> Option<PeerInfo<B>> {
		self.peers.get(who).map(|p| PeerInfo { best_hash: p.best_hash, best_number: p.best_number })
	}

	/// Returns the current sync status.
	pub fn status(&self) -> Status<B> {
		let best_seen = self.peers.values().max_by_key(|p| p.best_number).map(|p| p.best_number);
		let sync_state =
			if let Some(n) = best_seen {
				// A chain is classified as downloading if the provided best block is
				// more than `MAJOR_SYNC_BLOCKS` behind the best queued block.
				if n > self.best_queued_number && n - self.best_queued_number > MAJOR_SYNC_BLOCKS.into() {
					SyncState::Downloading
				} else {
					SyncState::Idle
				}
			} else {
				SyncState::Idle
			};

		Status {
			state: sync_state,
			best_seen_block: best_seen,
			num_peers: self.peers.len() as u32,
			queued_blocks: self.queue_blocks.len() as u32,
		}
	}

	/// Handle a new connected peer.
	///
	/// Call this method whenever we connect to a new peer.
	pub fn new_peer(&mut self, who: PeerId, info: protocol::PeerInfo<B>) -> Result<Option<BlockRequest<B>>, BadPeer> {
		// There is nothing sync can get from the node that has no blockchain data.
		if !info.roles.is_full() {
			return Ok(None)
		}
		match self.block_status(&info.best_hash) {
			Err(e) => {
				debug!(target:"sync", "Error reading blockchain: {:?}", e);
				Err(BadPeer(who, BLOCKCHAIN_STATUS_READ_ERROR_REPUTATION_CHANGE))
			}
			Ok(BlockStatus::KnownBad) => {
				info!("New peer with known bad best block {} ({}).", info.best_hash, info.best_number);
				Err(BadPeer(who, i32::min_value()))
			}
			Ok(BlockStatus::Unknown) => {
				if info.best_number.is_zero() {
					info!("New peer with unknown genesis hash {} ({}).", info.best_hash, info.best_number);
					return Err(BadPeer(who, i32::min_value()))
				}
				// If there are more than `MAJOR_SYNC_BLOCKS` in the import queue then we have
				// enough to do in the import queue that it's not worth kicking off
				// an ancestor search, which is what we do in the next match case below.
				if self.queue_blocks.len() > MAJOR_SYNC_BLOCKS.into() {
					debug!(
						target:"sync",
						"New peer with unknown best hash {} ({}), assuming common block.",
						self.best_queued_hash,
						self.best_queued_number
					);
					self.peers.insert(who, PeerSync {
						common_number: self.best_queued_number,
						best_hash: info.best_hash,
						best_number: info.best_number,
						state: PeerSyncState::Available,
						recently_announced: Default::default()
					});
					return Ok(None)
				}

				// If we are at genesis, just start downloading.
				if self.best_queued_number.is_zero() {
					debug!(target:"sync", "New peer with best hash {} ({}).", info.best_hash, info.best_number);
					self.peers.insert(who.clone(), PeerSync {
						common_number: Zero::zero(),
						best_hash: info.best_hash,
						best_number: info.best_number,
						state: PeerSyncState::Available,
						recently_announced: Default::default(),
					});
					return Ok(self.select_new_blocks(who).map(|(_, req)| req))
				}

				let common_best = std::cmp::min(self.best_queued_number, info.best_number);

				debug!(target:"sync",
					"New peer with unknown best hash {} ({}), searching for common ancestor.",
					info.best_hash,
					info.best_number
				);

				self.peers.insert(who, PeerSync {
					common_number: Zero::zero(),
					best_hash: info.best_hash,
					best_number: info.best_number,
					state: PeerSyncState::AncestorSearch(
						common_best,
						AncestorSearchState::ExponentialBackoff(One::one())
					),
					recently_announced: Default::default()
				});
				self.is_idle = false;

				Ok(Some(ancestry_request::<B>(common_best)))
			}
			Ok(BlockStatus::Queued) | Ok(BlockStatus::InChainWithState) | Ok(BlockStatus::InChainPruned) => {
				debug!(target:"sync", "New peer with known best hash {} ({}).", info.best_hash, info.best_number);
				self.peers.insert(who.clone(), PeerSync {
					common_number: info.best_number,
					best_hash: info.best_hash,
					best_number: info.best_number,
					state: PeerSyncState::Available,
					recently_announced: Default::default(),
				});
				self.is_idle = false;
				Ok(None)
			}
		}
	}

	/// Signal that `best_header` has been queued for import and update the
	/// `ChainSync` state with that information.
	pub fn update_chain_info(&mut self, best_header: &B::Header) {
		self.on_block_queued(&best_header.hash(), *best_header.number())
	}

	/// Schedule a justification request for the given block.
	pub fn request_justification(&mut self, hash: &B::Hash, number: NumberFor<B>) {
		let client = &self.client;
		self.extra_justifications.schedule((*hash, number), |base, block| {
			client.is_descendent_of(base, block)
		})
	}

	/// Schedule a finality proof request for the given block.
	pub fn request_finality_proof(&mut self, hash: &B::Hash, number: NumberFor<B>) {
		let client = &self.client;
		self.extra_finality_proofs.schedule((*hash, number), |base, block| {
			client.is_descendent_of(base, block)
		})
	}

	/// Request syncing for the given block from given set of peers.
	// The implementation is similar to on_block_announce with unknown parent hash.
	pub fn set_sync_fork_request(&mut self, peers: Vec<PeerId>, hash: &B::Hash, number: NumberFor<B>) {
		if peers.is_empty() {
			if let Some(_) = self.sync_requests.remove(hash) {
				debug!(target: "sync", "Cleared sync request for block {:?} with {:?}", hash, peers);
			}
			return;
		}
		debug!(target: "sync", "Explicit sync request for block {:?} with {:?}", hash, peers);
		if self.is_known(&hash) {
			debug!(target: "sync", "Refusing to sync known hash {:?}", hash);
			return;
		}

		let block_status = self.client.block_status(&BlockId::Number(number - One::one()))
			.unwrap_or(BlockStatus::Unknown);
		if block_status == BlockStatus::InChainPruned {
			trace!(target: "sync", "Refusing to sync ancient block {:?}", hash);
			return;
		}

		self.is_idle = false;
		for peer_id in &peers {
			if let Some(peer) = self.peers.get_mut(peer_id) {
				if let PeerSyncState::AncestorSearch(_, _) = peer.state {
					continue;
				}

				if number > peer.best_number {
					peer.best_number = number;
					peer.best_hash = hash.clone();
				}
			}
		}

		self.sync_requests
			.entry(hash.clone())
			.or_insert_with(|| SyncRequest {
				number,
				peers: Default::default(),
			})
			.peers.extend(peers);
	}

	/// Get an iterator over all scheduled justification requests.
	pub fn justification_requests(&mut self) -> impl Iterator<Item = (PeerId, BlockRequest<B>)> + '_ {
		let peers = &mut self.peers;
		let mut matcher = self.extra_justifications.matcher();
		std::iter::from_fn(move || {
			if let Some((peer, request)) = matcher.next(&peers) {
				peers.get_mut(&peer)
					.expect("`Matcher::next` guarantees the `PeerId` comes from the given peers; qed")
					.state = PeerSyncState::DownloadingJustification(request.0);
				let req = message::generic::BlockRequest {
					id: 0,
					fields: BlockAttributes::JUSTIFICATION,
					from: message::FromBlock::Hash(request.0),
					to: None,
					direction: message::Direction::Ascending,
					max: Some(1)
				};
				Some((peer, req))
			} else {
				None
			}
		})
	}

	/// Get an iterator over all scheduled finality proof requests.
	pub fn finality_proof_requests(&mut self) -> impl Iterator<Item = (PeerId, FinalityProofRequest<B::Hash>)> + '_ {
		let peers = &mut self.peers;
		let request_builder = &mut self.request_builder;
		let mut matcher = self.extra_finality_proofs.matcher();
		std::iter::from_fn(move || {
			if let Some((peer, request)) = matcher.next(&peers) {
				peers.get_mut(&peer)
					.expect("`Matcher::next` guarantees the `PeerId` comes from the given peers; qed")
					.state = PeerSyncState::DownloadingFinalityProof(request.0);
				let req = message::generic::FinalityProofRequest {
					id: 0,
					block: request.0,
					request: request_builder.as_mut()
						.map(|builder| builder.build_request_data(&request.0))
						.unwrap_or_default()
				};
				Some((peer, req))
			} else {
				None
			}
		})
	}

	/// Get an iterator over all block requests of all peers.
	pub fn block_requests(&mut self) -> impl Iterator<Item = (PeerId, BlockRequest<B>)> + '_ {
		if self.is_idle {
			return Either::Left(std::iter::empty())
		}
		if self.queue_blocks.len() > MAX_IMPORTING_BLOCKS {
			trace!(target: "sync", "Too many blocks in the queue.");
			return Either::Left(std::iter::empty())
		}
		let blocks = &mut self.blocks;
		let attrs = &self.required_block_attributes;
		let sync_requests = &self.sync_requests;
		let mut have_requests = false;
		let last_finalized = self.client.info().chain.finalized_number;
		let best_queued = self.best_queued_number;
		let iter = self.peers.iter_mut().filter_map(move |(id, peer)| {
			if !peer.state.is_available() {
				trace!(target: "sync", "Peer {} is busy", id);
				return None
			}
			if let Some((hash, req)) = explicit_sync_request(id, sync_requests, best_queued, last_finalized, attrs) {
				trace!(target: "sync", "Downloading explicitly requested block {:?} from {}", hash, id);
				peer.state = PeerSyncState::DownloadingStale(hash);
				have_requests = true;
				Some((id.clone(), req))
			} else if let Some((range, req)) = peer_block_request(id, peer, blocks, attrs) {
				peer.state = PeerSyncState::DownloadingNew(range.start);
				trace!(target: "sync", "New block request for {}", id);
				have_requests = true;
				Some((id.clone(), req))
			} else {
				trace!(target: "sync", "No new block request for {}", id);
				None
			}
		});
		if !have_requests {
			self.is_idle = true;
		}
		Either::Right(iter)
	}

	/// Handle a response from the remote to a block request that we made.
	///
	/// `request` must be the original request that triggered `response`.
	///
	/// If this corresponds to a valid block, this outputs the block that
	/// must be imported in the import queue.
	pub fn on_block_data
		(&mut self, who: PeerId, request: BlockRequest<B>, response: BlockResponse<B>) -> Result<OnBlockData<B>, BadPeer>
	{
		let new_blocks: Vec<IncomingBlock<B>> =
			if let Some(peer) = self.peers.get_mut(&who) {
				let mut blocks = response.blocks;
				if request.direction == message::Direction::Descending {
					trace!(target: "sync", "Reversing incoming block list");
					blocks.reverse()
				}
				self.is_idle = false;
				match &mut peer.state {
					PeerSyncState::DownloadingNew(start_block) => {
						self.blocks.clear_peer_download(&who);
						self.blocks.insert(*start_block, blocks, who);
						peer.state = PeerSyncState::Available;
						self.blocks
							.drain(self.best_queued_number + One::one())
							.into_iter()
							.map(|block_data| {
								IncomingBlock {
									hash: block_data.block.hash,
									header: block_data.block.header,
									body: block_data.block.body,
									justification: block_data.block.justification,
									origin: block_data.origin,
								}
							}).collect()
					}
					PeerSyncState::DownloadingStale(_) => {
						peer.state = PeerSyncState::Available;
						blocks.into_iter().map(|b| {
							IncomingBlock {
								hash: b.hash,
								header: b.header,
								body: b.body,
								justification: b.justification,
								origin: Some(who.clone()),
							}
						}).collect()
					}
					PeerSyncState::AncestorSearch(num, state) => {
						let block_hash_match = match (blocks.get(0), self.client.block_hash(*num)) {
							(Some(block), Ok(maybe_our_block_hash)) => {
								trace!(target: "sync", "Got ancestry block #{} ({}) from peer {}", num, block.hash, who);
								maybe_our_block_hash.map_or(false, |x| x == block.hash)
							},
							(None, _) => {
								debug!(target: "sync", "Invalid response when searching for ancestor from {}", who);
								return Err(BadPeer(who, i32::min_value()))
							},
							(_, Err(e)) => {
								info!("Error answering legitimate blockchain query: {:?}", e);
								return Err(BadPeer(who, ANCESTRY_BLOCK_ERROR_REPUTATION_CHANGE))
							}
						};
						if block_hash_match && peer.common_number < *num {
							peer.common_number = *num;
						}
						if !block_hash_match && num.is_zero() {
							trace!(target:"sync", "Ancestry search: genesis mismatch for peer {}", who);
							return Err(BadPeer(who, GENESIS_MISMATCH_REPUTATION_CHANGE))
						}
						if let Some((next_state, next_num)) = handle_ancestor_search_state(state, *num, block_hash_match) {
							peer.state = PeerSyncState::AncestorSearch(next_num, next_state);
							return Ok(OnBlockData::Request(who, ancestry_request::<B>(next_num)))
						} else {
							peer.state = PeerSyncState::Available;
							Vec::new()
						}
					}

					| PeerSyncState::Available
					| PeerSyncState::DownloadingJustification(..)
					| PeerSyncState::DownloadingFinalityProof(..) => Vec::new()
				}
			} else {
				Vec::new()
			};

		let is_recent = new_blocks.first()
			.map(|block| {
				self.peers.iter().any(|(_, peer)| peer.recently_announced.contains(&block.hash))
			})
			.unwrap_or(false);

		let origin =
			if is_recent {
				BlockOrigin::NetworkBroadcast
			} else {
				BlockOrigin::NetworkInitialSync
			};

		if let Some((h, n)) = new_blocks.last().and_then(|b| b.header.as_ref().map(|h| (&b.hash, *h.number()))) {
			trace!(target:"sync", "Accepted {} blocks ({:?}) with origin {:?}", new_blocks.len(), h, origin);
			self.on_block_queued(h, n)
		}

		let new_best_importing_number = new_blocks.last()
			.and_then(|b| b.header.as_ref().map(|h| *h.number()))
			.unwrap_or_else(|| Zero::zero());

		self.queue_blocks.extend(new_blocks.iter().map(|b| b.hash));

		self.best_importing_number = std::cmp::max(new_best_importing_number, self.best_importing_number);

		Ok(OnBlockData::Import(origin, new_blocks))
	}

	/// Handle a response from the remote to a justification request that we made.
	///
	/// `request` must be the original request that triggered `response`.
	///
	/// Returns `Some` if this produces a justification that must be imported
	/// into the import queue.
	pub fn on_block_justification
		(&mut self, who: PeerId, response: BlockResponse<B>) -> Result<OnBlockJustification<B>, BadPeer>
	{
		let peer =
			if let Some(peer) = self.peers.get_mut(&who) {
				peer
			} else {
				error!(target: "sync", "Called on_block_justification with a bad peer ID");
				return Ok(OnBlockJustification::Nothing)
			};

		self.is_idle = false;
		if let PeerSyncState::DownloadingJustification(hash) = peer.state {
			peer.state = PeerSyncState::Available;

			// We only request one justification at a time
			if let Some(block) = response.blocks.into_iter().next() {
				if hash != block.hash {
					info!(
						"Invalid block justification provided by {}: requested: {:?} got: {:?}", who, hash, block.hash
					);
					return Err(BadPeer(who, i32::min_value()))
				}
				if let Some((peer, hash, number, j)) = self.extra_justifications.on_response(who, block.justification) {
					return Ok(OnBlockJustification::Import { peer, hash, number, justification: j })
				}
			} else {
				// we might have asked the peer for a justification on a block that we thought it had
				// (regardless of whether it had a justification for it or not).
				trace!(target: "sync", "Peer {:?} provided empty response for justification request {:?}", who, hash)
			}
		}

		Ok(OnBlockJustification::Nothing)
	}

	/// Handle new finality proof data.
	pub fn on_block_finality_proof
		(&mut self, who: PeerId, resp: FinalityProofResponse<B::Hash>) -> Result<OnBlockFinalityProof<B>, BadPeer>
	{
		let peer =
			if let Some(peer) = self.peers.get_mut(&who) {
				peer
			} else {
				error!(target: "sync", "Called on_block_finality_proof_data with a bad peer ID");
				return Ok(OnBlockFinalityProof::Nothing)
			};

		self.is_idle = false;
		if let PeerSyncState::DownloadingFinalityProof(hash) = peer.state {
			peer.state = PeerSyncState::Available;

			// We only request one finality proof at a time.
			if hash != resp.block {
				info!("Invalid block finality proof provided: requested: {:?} got: {:?}", hash, resp.block);
				return Err(BadPeer(who, i32::min_value()))
			}

			if let Some((peer, hash, number, p)) = self.extra_finality_proofs.on_response(who, resp.proof) {
				return Ok(OnBlockFinalityProof::Import { peer, hash, number, proof: p })
			}
		}

		Ok(OnBlockFinalityProof::Nothing)
	}

	/// A batch of blocks have been processed, with or without errors.
	///
	/// Call this when a batch of blocks have been processed by the import
	/// queue, with or without errors.
	///
	/// `peer_info` is passed in case of a restart.
	pub fn on_blocks_processed<'a>(
		&'a mut self,
		imported: usize,
		count: usize,
		results: Vec<(Result<BlockImportResult<NumberFor<B>>, BlockImportError>, B::Hash)>,
		mut peer_info: impl FnMut(&PeerId) -> Option<protocol::PeerInfo<B>>
	) -> impl Iterator<Item = Result<(PeerId, BlockRequest<B>), BadPeer>> + 'a {
		trace!(target: "sync", "Imported {} of {}", imported, count);

		let mut output = Vec::new();

		let mut has_error = false;
		let mut hashes = vec![];
		for (result, hash) in results {
			hashes.push(hash);

			if has_error {
				continue;
			}

			if result.is_err() {
				has_error = true;
			}

			match result {
				Ok(BlockImportResult::ImportedKnown(_number)) => {}
				Ok(BlockImportResult::ImportedUnknown(number, aux, who)) => {
					if aux.clear_justification_requests {
						trace!(
							target: "sync",
							"Block imported clears all pending justification requests {}: {:?}",
							number,
							hash
						);
						self.extra_justifications.reset()
					}

					if aux.needs_justification {
						trace!(target: "sync", "Block imported but requires justification {}: {:?}", number, hash);
						self.request_justification(&hash, number);
					}

					if aux.bad_justification {
						if let Some(peer) = who {
							info!("Sent block with bad justification to import");
							output.push(Err(BadPeer(peer, BAD_JUSTIFICATION_REPUTATION_CHANGE)));
						}
					}

					if aux.needs_finality_proof {
						trace!(target: "sync", "Block imported but requires finality proof {}: {:?}", number, hash);
						self.request_finality_proof(&hash, number);
					}
				},
				Err(BlockImportError::IncompleteHeader(who)) => {
					if let Some(peer) = who {
						info!("Peer sent block with incomplete header to import");
						output.push(Err(BadPeer(peer, INCOMPLETE_HEADER_REPUTATION_CHANGE)));
						output.extend(self.restart(&mut peer_info));
					}
				},
				Err(BlockImportError::VerificationFailed(who, e)) => {
					if let Some(peer) = who {
						info!("Verification failed from peer: {}", e);
						output.push(Err(BadPeer(peer, VERIFICATION_FAIL_REPUTATION_CHANGE)));
						output.extend(self.restart(&mut peer_info));
					}
				},
				Err(BlockImportError::BadBlock(who)) => {
					if let Some(peer) = who {
						info!("Bad block");
						output.push(Err(BadPeer(peer, BAD_BLOCK_REPUTATION_CHANGE)));
						output.extend(self.restart(&mut peer_info));
					}
				},
				Err(BlockImportError::UnknownParent) |
				Err(BlockImportError::Cancelled) |
				Err(BlockImportError::Other(_)) => {
					output.extend(self.restart(&mut peer_info));
				},
			};
		}

		for hash in hashes {
			self.queue_blocks.remove(&hash);
		}
		if has_error {
			self.best_importing_number = Zero::zero()
		}

		self.is_idle = false;
		output.into_iter()
	}

	/// Call this when a justification has been processed by the import queue,
	/// with or without errors.
	pub fn on_justification_import(&mut self, hash: B::Hash, number: NumberFor<B>, success: bool) {
		let finalization_result = if success { Ok((hash, number)) } else { Err(()) };
		self.extra_justifications.try_finalize_root((hash, number), finalization_result, true);
		self.is_idle = false;
	}

	pub fn on_finality_proof_import(&mut self, req: (B::Hash, NumberFor<B>), res: Result<(B::Hash, NumberFor<B>), ()>) {
		self.extra_finality_proofs.try_finalize_root(req, res, true);
		self.is_idle = false;
	}

	/// Notify about finalization of the given block.
	pub fn on_block_finalized(&mut self, hash: &B::Hash, number: NumberFor<B>) {
		let client = &self.client;
		let r = self.extra_finality_proofs.on_block_finalized(hash, number, |base, block| {
			client.is_descendent_of(base, block)
		});

		if let Err(err) = r {
			warn!(target: "sync", "Error cleaning up pending extra finality proof data requests: {:?}", err)
		}

		let client = &self.client;
		let r = self.extra_justifications.on_block_finalized(hash, number, |base, block| {
			client.is_descendent_of(base, block)
		});

		if let Err(err) = r {
			warn!(target: "sync", "Error cleaning up pending extra justification data requests: {:?}", err);
		}
	}

	/// Called when a block has been queued for import.
	///
	/// Updates our internal state for best queued block and then goes
	/// through all peers to update our view of their state as well.
	fn on_block_queued(&mut self, hash: &B::Hash, number: NumberFor<B>) {
		if number > self.best_queued_number {
			self.best_queued_number = number;
			self.best_queued_hash = *hash;
		}
		if let Some(_) = self.sync_requests.remove(&hash) {
			trace!(target: "sync", "Completed explicit sync request {:?}", hash);
		}
		// Update common blocks
		for (n, peer) in self.peers.iter_mut() {
			if let PeerSyncState::AncestorSearch(_, _) = peer.state {
				// Abort search.
				peer.state = PeerSyncState::Available;
			}
			let new_common_number = if peer.best_number >= number {
				number
			} else {
				peer.best_number
			};
			trace!(
				target: "sync",
				"Updating peer {} info, ours={}, common={}->{}, their best={}",
				n,
				number,
				peer.common_number,
				new_common_number,
				peer.best_number,
			);
			peer.common_number = new_common_number;
		}
		self.is_idle = false;
	}

	/// Call when a node announces a new block.
	///
	/// If true is returned, then the caller MUST try to import passed
	/// header (call `on_block_data`). The network request isn't sent
	/// in this case. Both hash and header is passed as an optimization
	/// to avoid rehashing the header.
	pub fn on_block_announce(&mut self, who: PeerId, hash: B::Hash, announce: &BlockAnnounce<B::Header>, is_best: bool)
		-> OnBlockAnnounce<B>
	{
		let header = &announce.header;
		let number = *header.number();
		debug!(target: "sync", "Received block announcement with number {:?}", number);
		if number.is_zero() {
			warn!(target: "sync", "Ignored genesis block (#0) announcement from {}: {}", who, hash);
			return OnBlockAnnounce::Nothing
		}
		let parent_status = self.block_status(header.parent_hash()).ok().unwrap_or(BlockStatus::Unknown);
		let known_parent = parent_status != BlockStatus::Unknown;
		let ancient_parent = parent_status == BlockStatus::InChainPruned;

		let known = self.is_known(&hash);
		let peer = if let Some(peer) = self.peers.get_mut(&who) {
			peer
		} else {
			error!(target: "sync", "Called on_block_announce with a bad peer ID");
			return OnBlockAnnounce::Nothing
		};
		while peer.recently_announced.len() >= ANNOUNCE_HISTORY_SIZE {
			peer.recently_announced.pop_front();
		}
		peer.recently_announced.push_back(hash.clone());
		if is_best && number > peer.best_number {
			// update their best block
			peer.best_number = number;
			peer.best_hash = hash;
		}
		if let PeerSyncState::AncestorSearch(_, _) = peer.state {
			return OnBlockAnnounce::Nothing
		}
		// If the announced block is the best they have seen, our common number
		// is either one further ahead or it's the one they just announced, if we know about it.
		if known && is_best {
			peer.common_number = number
		} else if header.parent_hash() == &self.best_queued_hash || known_parent {
			peer.common_number = number - One::one();
		}
		self.is_idle = false;

		// known block case
		if known || self.is_already_downloading(&hash) {
			trace!(target: "sync", "Known block announce from {}: {}", who, hash);
			return OnBlockAnnounce::Nothing
		}

		// Let external validator check the block announcement.
		let assoc_data = announce.data.as_ref().map_or(&[][..], |v| v.as_slice());
		match self.block_announce_validator.validate(&header, assoc_data) {
			Ok(Validation::Success) => (),
			Ok(Validation::Failure) => {
				debug!(target: "sync", "block announcement validation of block {} from {} failed", hash, who);
				return OnBlockAnnounce::Nothing
			}
			Err(e) => {
				error!(target: "sync", "block announcement validation errored: {}", e);
				return OnBlockAnnounce::Nothing
			}
		}

		// stale block case
		let requires_additional_data = !self.role.is_light();
		if number <= self.best_queued_number {
			if !(known_parent || self.is_already_downloading(header.parent_hash())) {
				let block_status = self.client.block_status(&BlockId::Number(*header.number()))
					.unwrap_or(BlockStatus::Unknown);
				if block_status == BlockStatus::InChainPruned {
					trace!(
						target: "sync",
						"Ignored unknown ancient block announced from {}: {} {:?}", who, hash, header
					);
					return OnBlockAnnounce::Nothing
				}
				trace!(
					target: "sync",
					"Considering new unknown stale block announced from {}: {} {:?}", who, hash, header
				);
				if let Some(request) = self.download_unknown_stale(&who, &hash) {
					if requires_additional_data {
						return OnBlockAnnounce::Request(who, request)
					} else {
						return OnBlockAnnounce::ImportHeader
					}
				} else {
					return OnBlockAnnounce::Nothing
				}
			} else {
				if ancient_parent {
					trace!(target: "sync", "Ignored ancient stale block announced from {}: {} {:?}", who, hash, header);
					return OnBlockAnnounce::Nothing
				}
				if let Some(request) = self.download_stale(&who, &hash) {
					if requires_additional_data {
						return OnBlockAnnounce::Request(who, request)
					} else {
						return OnBlockAnnounce::ImportHeader
					}
				} else {
					return OnBlockAnnounce::Nothing
				}
			}
		}

		if ancient_parent {
			trace!(target: "sync", "Ignored ancient block announced from {}: {} {:?}", who, hash, header);
			return OnBlockAnnounce::Nothing
		}

		trace!(target: "sync", "Considering new block announced from {}: {} {:?}", who, hash, header);

		let (range, request) = match self.select_new_blocks(who.clone()) {
			Some((range, request)) => (range, request),
			None => return OnBlockAnnounce::Nothing
		};

		let is_required_data_available = !requires_additional_data
			&& range.end - range.start == One::one()
			&& range.start == *header.number();

		if !is_required_data_available {
			return OnBlockAnnounce::Request(who, request)
		}

		OnBlockAnnounce::ImportHeader
	}

	/// Call when a peer has disconnected.
	pub fn peer_disconnected(&mut self, who: PeerId) {
		self.blocks.clear_peer_download(&who);
		self.peers.remove(&who);
		self.extra_justifications.peer_disconnected(&who);
		self.extra_finality_proofs.peer_disconnected(&who);
		self.is_idle = false;
	}

	/// Restart the sync process.
	fn restart<'a, F>
		(&'a mut self, mut peer_info: F) -> impl Iterator<Item = Result<(PeerId, BlockRequest<B>), BadPeer>> + 'a
		where F: FnMut(&PeerId) -> Option<protocol::PeerInfo<B>> + 'a
	{
		self.queue_blocks.clear();
		self.best_importing_number = Zero::zero();
		self.blocks.clear();
		let info = self.client.info();
		self.best_queued_hash = info.chain.best_hash;
		self.best_queued_number = info.chain.best_number;
		self.is_idle = false;
		debug!(target:"sync", "Restarted with {} ({})", self.best_queued_number, self.best_queued_hash);
		let old_peers = std::mem::replace(&mut self.peers, HashMap::new());
		old_peers.into_iter().filter_map(move |(id, _)| {
			let info = peer_info(&id)?;
			match self.new_peer(id.clone(), info) {
				Ok(None) => None,
				Ok(Some(x)) => Some(Ok((id, x))),
				Err(e) => Some(Err(e))
			}
		})
	}

	/// Download old block with known parent.
	fn download_stale(&mut self, who: &PeerId, hash: &B::Hash) -> Option<BlockRequest<B>> {
		let peer = self.peers.get_mut(who)?;
		if !peer.state.is_available() {
			return None
		}
		peer.state = PeerSyncState::DownloadingStale(*hash);
		Some(message::generic::BlockRequest {
			id: 0,
			fields: self.required_block_attributes.clone(),
			from: message::FromBlock::Hash(*hash),
			to: None,
			direction: message::Direction::Ascending,
			max: Some(1),
		})
	}

	/// Download old block with unknown parent.
	fn download_unknown_stale(&mut self, who: &PeerId, hash: &B::Hash) -> Option<BlockRequest<B>> {
		let peer = self.peers.get_mut(who)?;
		if !peer.state.is_available() {
			return None
		}
		peer.state = PeerSyncState::DownloadingStale(*hash);
		Some(message::generic::BlockRequest {
			id: 0,
			fields: self.required_block_attributes.clone(),
			from: message::FromBlock::Hash(*hash),
			to: None,
			direction: message::Direction::Descending,
			max: Some(MAX_UNKNOWN_FORK_DOWNLOAD_LEN),
		})
	}

	/// Select a range of new blocks to download from the given peer.
	fn select_new_blocks(&mut self, who: PeerId) -> Option<(Range<NumberFor<B>>, BlockRequest<B>)> {
		// when there are too many blocks in the queue => do not try to download new blocks
		if self.queue_blocks.len() > MAX_IMPORTING_BLOCKS {
			trace!(target: "sync", "Too many blocks in the queue.");
			return None
		}

		let peer = self.peers.get_mut(&who)?;

		if !peer.state.is_available() {
			trace!(target: "sync", "Peer {} is busy", who);
			return None
		}

		trace!(
			target: "sync",
			"Considering new block download from {}, common block is {}, best is {:?}",
			who,
			peer.common_number,
			peer.best_number
		);

		if let Some((range, req)) = peer_block_request(&who, peer, &mut self.blocks, &self.required_block_attributes) {
			trace!(target: "sync", "Requesting blocks from {}, ({} to {})", who, range.start, range.end);
			peer.state = PeerSyncState::DownloadingNew(range.start);
			Some((range, req))
		} else {
			trace!(target: "sync", "Nothing to request from {}", who);
			None
		}
	}

	/// What is the status of the block corresponding to the given hash?
	fn block_status(&self, hash: &B::Hash) -> Result<BlockStatus, ClientError> {
		if self.queue_blocks.contains(hash) {
			return Ok(BlockStatus::Queued)
		}
		self.client.block_status(&BlockId::Hash(*hash))
	}

	/// Is the block corresponding to the given hash known?
	fn is_known(&self, hash: &B::Hash) -> bool {
		self.block_status(hash).ok().map_or(false, |s| s != BlockStatus::Unknown)
	}

	/// Is any peer downloading the given hash?
	fn is_already_downloading(&self, hash: &B::Hash) -> bool {
		self.peers.iter().any(|(_, p)| p.state == PeerSyncState::DownloadingStale(*hash))
	}
}

/// Request the ancestry for a block. Sends a request for header and justification for the given
/// block number. Used during ancestry search.
fn ancestry_request<B: BlockT>(block: NumberFor<B>) -> BlockRequest<B> {
	message::generic::BlockRequest {
		id: 0,
		fields: BlockAttributes::HEADER | BlockAttributes::JUSTIFICATION,
		from: message::FromBlock::Number(block),
		to: None,
		direction: message::Direction::Ascending,
		max: Some(1)
	}
}

/// The ancestor search state expresses which algorithm, and its stateful parameters, we are using to
/// try to find an ancestor block
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
	block_hash_match: bool
) -> Option<(AncestorSearchState<B>, NumberFor<B>)> {
	let two = <NumberFor<B>>::one() + <NumberFor<B>>::one();
	match state {
		AncestorSearchState::ExponentialBackoff(next_distance_to_tip) => {
			let next_distance_to_tip = *next_distance_to_tip;
			if block_hash_match && next_distance_to_tip == One::one() {
				// We found the ancestor in the first step so there is no need to execute binary search.
				return None;
			}
			if block_hash_match {
				let left = curr_block_num;
				let right = left + next_distance_to_tip / two;
				let middle = left + (right - left) / two;
				Some((AncestorSearchState::BinarySearch(left, right), middle))
			} else {
				let next_block_num = curr_block_num.checked_sub(&next_distance_to_tip)
					.unwrap_or_else(Zero::zero);
				let next_distance_to_tip = next_distance_to_tip * two;
				Some((AncestorSearchState::ExponentialBackoff(next_distance_to_tip), next_block_num))
			}
		}
		AncestorSearchState::BinarySearch(mut left, mut right) => {
			if left >= curr_block_num {
				return None;
			}
			if block_hash_match {
				left = curr_block_num;
			} else {
				right = curr_block_num;
			}
			assert!(right >= left);
			let middle = left + (right - left) / two;
			Some((AncestorSearchState::BinarySearch(left, right), middle))
		}
	}
}

/// Get a new block request for the peer if any.
fn peer_block_request<B: BlockT>(
	id: &PeerId,
	peer: &PeerSync<B>,
	blocks: &mut BlockCollection<B>,
	attrs: &message::BlockAttributes,
) -> Option<(Range<NumberFor<B>>, BlockRequest<B>)> {
	if let Some(range) = blocks.needed_blocks(id.clone(), MAX_BLOCKS_TO_REQUEST, peer.best_number, peer.common_number) {
		let request = message::generic::BlockRequest {
			id: 0,
			fields: attrs.clone(),
			from: message::FromBlock::Number(range.start),
			to: None,
			direction: message::Direction::Ascending,
			max: Some((range.end - range.start).saturated_into::<u32>())
		};
		Some((range, request))
	} else {
		None
	}
}

/// Get pending explicit sync request for a peer.
fn explicit_sync_request<B: BlockT>(
	id: &PeerId,
	requests: &HashMap<B::Hash, SyncRequest<B>>,
	best_num: NumberFor<B>,
	finalized: NumberFor<B>,
	attributes: &message::BlockAttributes,
) -> Option<(B::Hash, BlockRequest<B>)>
{
	for (hash, r) in requests {
		if !r.peers.contains(id) {
			continue
		}
		if r.number <= best_num {
			trace!(target: "sync", "Downloading requested fork {:?} from {}", hash, id);
			return Some((hash.clone(), message::generic::BlockRequest {
				id: 0,
				fields: attributes.clone(),
				from: message::FromBlock::Hash(hash.clone()),
				to: None,
				direction: message::Direction::Descending,
				max: Some((r.number - finalized).saturated_into::<u32>()), // up to the last finalized block
			}))
		}
	}
	None
}
