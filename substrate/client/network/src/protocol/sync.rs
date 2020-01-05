// Copyright 2017-2020 Parity Technologies (UK) Ltd.
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
use sp_blockchain::{Error as ClientError, Info as BlockchainInfo};
use sp_consensus::{BlockOrigin, BlockStatus,
	block_validation::{BlockAnnounceValidator, Validation},
	import_queue::{IncomingBlock, BlockImportResult, BlockImportError}
};
use crate::{
	config::{Roles, BoxFinalityProofRequestBuilder},
	message::{self, generic::FinalityProofRequest, BlockAnnounce, BlockAttributes, BlockRequest, BlockResponse,
	FinalityProofResponse},
};
use either::Either;
use extra_requests::ExtraRequests;
use libp2p::PeerId;
use log::{debug, trace, warn, info, error};
use sp_runtime::{
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

/// Maximum blocks to download ahead of any gap.
const MAX_DOWNLOAD_AHEAD: u32 = 2048;

/// We use a heuristic that with a high likelihood, by the time
/// `MAJOR_SYNC_BLOCKS` have been imported we'll be on the same
/// chain as (or at least closer to) the peer so we want to delay
/// the ancestor search to not waste time doing that when we are
/// so far behind.
const MAJOR_SYNC_BLOCKS: u8 = 5;

/// Number of recently announced blocks to track for each peer.
const ANNOUNCE_HISTORY_SIZE: usize = 64;

mod rep {
	use sc_peerset::ReputationChange as Rep;
	/// Reputation change when a peer sent us a message that led to a
	/// database read error.
	pub const BLOCKCHAIN_READ_ERROR: Rep = Rep::new(-(1 << 16), "DB Error");

	/// Reputation change when a peer sent us a status message with a different
	/// genesis than us.
	pub const GENESIS_MISMATCH: Rep = Rep::new(i32::min_value(), "Genesis mismatch");

	/// Reputation change for peers which send us a block with an incomplete header.
	pub const INCOMPLETE_HEADER: Rep = Rep::new(-(1 << 20), "Incomplete header");

	/// Reputation change for peers which send us a block which we fail to verify.
	pub const VERIFICATION_FAIL: Rep = Rep::new(-(1 << 20), "Block verification failed");

	/// Reputation change for peers which send us a known bad block.
	pub const BAD_BLOCK: Rep = Rep::new(-(1 << 29), "Bad block");

	/// Reputation change for peers which send us a block with bad justifications.
	pub const BAD_JUSTIFICATION: Rep = Rep::new(-(1 << 16), "Bad justification");

	/// Reputation change for peers which send us a block with bad finality proof.
	pub const BAD_FINALITY_PROOF: Rep = Rep::new(-(1 << 16), "Bad finality proof");

	/// Reputation change when a peer sent us invlid ancestry result.
	pub const UNKNOWN_ANCESTOR:Rep = Rep::new(-(1 << 16), "DB Error");
}

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
	/// The best block number that was successfully imported into the chain.
	/// This can not decrease.
	best_imported_number: NumberFor<B>,
	/// Finality proof handler.
	request_builder: Option<BoxFinalityProofRequestBuilder<B>>,
	/// Fork sync targets.
	fork_targets: HashMap<B::Hash, ForkTarget<B>>,
	/// A flag that caches idle state with no pending requests.
	is_idle: bool,
	/// A type to check incoming block announcements.
	block_announce_validator: Box<dyn BlockAnnounceValidator<B> + Send>,
	/// Maximum number of peers to ask the same blocks in parallel.
	max_parallel_downloads: u32,
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
pub struct BadPeer(pub PeerId, pub sc_peerset::ReputationChange);

impl fmt::Display for BadPeer {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "Bad peer {}; Reputation change: {:?}", self.0, self.1)
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
pub enum OnBlockAnnounce {
	/// The announcement does not require further handling.
	Nothing,
	/// The announcement header should be imported.
	ImportHeader,
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
		info: &BlockchainInfo<B>,
		request_builder: Option<BoxFinalityProofRequestBuilder<B>>,
		block_announce_validator: Box<dyn BlockAnnounceValidator<B> + Send>,
		max_parallel_downloads: u32,
	) -> Self {
		let mut required_block_attributes = BlockAttributes::HEADER | BlockAttributes::JUSTIFICATION;

		if role.is_full() {
			required_block_attributes |= BlockAttributes::BODY
		}

		ChainSync {
			client,
			peers: HashMap::new(),
			blocks: BlockCollection::new(),
			best_queued_hash: info.best_hash,
			best_queued_number: info.best_number,
			best_imported_number: info.best_number,
			extra_finality_proofs: ExtraRequests::new(),
			extra_justifications: ExtraRequests::new(),
			role,
			required_block_attributes,
			queue_blocks: Default::default(),
			request_builder,
			fork_targets: Default::default(),
			is_idle: false,
			block_announce_validator,
			max_parallel_downloads,
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
	pub fn new_peer(&mut self, who: PeerId, best_hash: B::Hash, best_number: NumberFor<B>)
		-> Result<Option<BlockRequest<B>>, BadPeer>
	{
		// There is nothing sync can get from the node that has no blockchain data.
		match self.block_status(&best_hash) {
			Err(e) => {
				debug!(target:"sync", "Error reading blockchain: {:?}", e);
				Err(BadPeer(who, rep::BLOCKCHAIN_READ_ERROR))
			}
			Ok(BlockStatus::KnownBad) => {
				info!("New peer with known bad best block {} ({}).", best_hash, best_number);
				Err(BadPeer(who, rep::BAD_BLOCK))
			}
			Ok(BlockStatus::Unknown) => {
				if best_number.is_zero() {
					info!("New peer with unknown genesis hash {} ({}).", best_hash, best_number);
					return Err(BadPeer(who, rep::GENESIS_MISMATCH));
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
						best_hash,
						best_number,
						state: PeerSyncState::Available,
						recently_announced: Default::default()
					});
					return Ok(None)
				}

				// If we are at genesis, just start downloading.
				if self.best_queued_number.is_zero() {
					debug!(target:"sync", "New peer with best hash {} ({}).", best_hash, best_number);
					self.peers.insert(who.clone(), PeerSync {
						common_number: Zero::zero(),
						best_hash,
						best_number,
						state: PeerSyncState::Available,
						recently_announced: Default::default(),
					});
					self.is_idle = false;
					return Ok(None)
				}

				let common_best = std::cmp::min(self.best_queued_number, best_number);

				debug!(target:"sync",
					"New peer with unknown best hash {} ({}), searching for common ancestor.",
					best_hash,
					best_number
				);

				self.peers.insert(who, PeerSync {
					common_number: Zero::zero(),
					best_hash,
					best_number,
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
				debug!(target:"sync", "New peer with known best hash {} ({}).", best_hash, best_number);
				self.peers.insert(who.clone(), PeerSync {
					common_number: best_number,
					best_hash,
					best_number,
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
	pub fn set_sync_fork_request(&mut self, mut peers: Vec<PeerId>, hash: &B::Hash, number: NumberFor<B>) {
		if peers.is_empty() {
			debug!(
				target: "sync",
				"Explicit sync request for block {:?} with no peers specified. \
				 Syncing from all connected peers {:?} instead.",
				hash, peers,
			);

			peers = self.peers.iter()
				// Only request blocks from peers who are ahead or on a par.
				.filter(|(_, peer)| peer.best_number >= number)
				.map(|(id, _)| id.clone())
				.collect();
		} else {
			debug!(target: "sync", "Explicit sync request for block {:?} with {:?}", hash, peers);
		}

		if self.is_known(&hash) {
			debug!(target: "sync", "Refusing to sync known hash {:?}", hash);
			return;
		}

		trace!(target: "sync", "Downloading requested old fork {:?}", hash);
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

		self.fork_targets
			.entry(hash.clone())
			.or_insert_with(|| ForkTarget {
				number,
				peers: Default::default(),
				parent_hash: None,
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
		let major_sync = self.status().state == SyncState::Downloading;
		let blocks = &mut self.blocks;
		let attrs = &self.required_block_attributes;
		let fork_targets = &mut self.fork_targets;
		let mut have_requests = false;
		let last_finalized = self.client.info().finalized_number;
		let best_queued = self.best_queued_number;
		let client = &self.client;
		let queue = &self.queue_blocks;
		let max_parallel = if major_sync { 1 } else { self.max_parallel_downloads };
		let iter = self.peers.iter_mut().filter_map(move |(id, peer)| {
			if !peer.state.is_available() {
				trace!(target: "sync", "Peer {} is busy", id);
				return None
			}
			if let Some((hash, req)) = fork_sync_request(
				id,
				fork_targets,
				best_queued,
				last_finalized,
				attrs,
				|hash| if queue.contains(hash) {
					BlockStatus::Queued
				} else {
					client.block_status(&BlockId::Hash(*hash)).unwrap_or(BlockStatus::Unknown)
				},
			) {
				trace!(target: "sync", "Downloading fork {:?} from {}", hash, id);
				peer.state = PeerSyncState::DownloadingStale(hash);
				have_requests = true;
				Some((id.clone(), req))
			} else if let Some((range, req)) = peer_block_request(
				id,
				peer,
				blocks,
				attrs,
				max_parallel,
				last_finalized
			) {
				peer.state = PeerSyncState::DownloadingNew(range.start);
				trace!(
					target: "sync",
					"New block request for {}, (best:{}, common:{}) {:?}",
					id,
					peer.best_number,
					peer.common_number,
					req,
				);
				have_requests = true;
				Some((id.clone(), req))
			} else {
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
									allow_missing_state: false,
									import_existing: false,
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
								allow_missing_state: true,
								import_existing: false,
							}
						}).collect()
					}
					PeerSyncState::AncestorSearch(num, state) => {
						let matching_hash = match (blocks.get(0), self.client.block_hash(*num)) {
							(Some(block), Ok(maybe_our_block_hash)) => {
								trace!(target: "sync", "Got ancestry block #{} ({}) from peer {}", num, block.hash, who);
								maybe_our_block_hash.filter(|x| x == &block.hash)
							},
							(None, _) => {
								debug!(target: "sync", "Invalid response when searching for ancestor from {}", who);
								return Err(BadPeer(who, rep::UNKNOWN_ANCESTOR))
							},
							(_, Err(e)) => {
								info!("Error answering legitimate blockchain query: {:?}", e);
								return Err(BadPeer(who, rep::BLOCKCHAIN_READ_ERROR))
							}
						};
						if matching_hash.is_some() && peer.common_number < *num {
							peer.common_number = *num;
						}
						if matching_hash.is_none() && num.is_zero() {
							trace!(target:"sync", "Ancestry search: genesis mismatch for peer {}", who);
							return Err(BadPeer(who, rep::GENESIS_MISMATCH))
						}
						if let Some((next_state, next_num)) = handle_ancestor_search_state(state, *num, matching_hash.is_some()) {
							peer.state = PeerSyncState::AncestorSearch(next_num, next_state);
							return Ok(OnBlockData::Request(who, ancestry_request::<B>(next_num)))
						} else {
							// Ancestry search is complete. Check if peer is on a stale fork unknown to us and
							// add it to sync targets if necessary.
							trace!(target: "sync", "Ancestry search complete. Ours={} ({}), Theirs={} ({}), Common={:?} ({})",
								self.best_queued_hash,
								self.best_queued_number,
								peer.best_hash,
								peer.best_number,
								matching_hash,
								peer.common_number,
							);
							if peer.common_number < peer.best_number
								&& peer.best_number < self.best_queued_number
							{
								trace!(target: "sync", "Added fork target {} for {}" , peer.best_hash, who);
								self.fork_targets
									.entry(peer.best_hash.clone())
									.or_insert_with(|| ForkTarget {
										number: peer.best_number,
										parent_hash: None,
										peers: Default::default(),
									})
								.peers.insert(who);
							}
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

		self.queue_blocks.extend(new_blocks.iter().map(|b| b.hash));

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
						target: "sync",
						"Invalid block justification provided by {}: requested: {:?} got: {:?}", who, hash, block.hash
					);
					return Err(BadPeer(who, rep::BAD_JUSTIFICATION));
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
				info!(
					target: "sync",
					"Invalid block finality proof provided: requested: {:?} got: {:?}",
					hash,
					resp.block
				);
				return Err(BadPeer(who, rep::BAD_FINALITY_PROOF));
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
							output.push(Err(BadPeer(peer, rep::BAD_JUSTIFICATION)));
						}
					}

					if aux.needs_finality_proof {
						trace!(target: "sync", "Block imported but requires finality proof {}: {:?}", number, hash);
						self.request_finality_proof(&hash, number);
					}

					if number > self.best_imported_number {
						self.best_imported_number = number;
					}
				},
				Err(BlockImportError::IncompleteHeader(who)) => {
					if let Some(peer) = who {
						info!("Peer sent block with incomplete header to import");
						output.push(Err(BadPeer(peer, rep::INCOMPLETE_HEADER)));
						output.extend(self.restart());
					}
				},
				Err(BlockImportError::VerificationFailed(who, e)) => {
					if let Some(peer) = who {
						info!("Verification failed from peer: {}", e);
						output.push(Err(BadPeer(peer, rep::VERIFICATION_FAIL)));
						output.extend(self.restart());
					}
				},
				Err(BlockImportError::BadBlock(who)) => {
					if let Some(peer) = who {
						info!("Bad block");
						output.push(Err(BadPeer(peer, rep::BAD_BLOCK)));
						output.extend(self.restart());
					}
				},
				Err(BlockImportError::MissingState) => {
					// This may happen if the chain we were requesting upon has been discarded
					// in the meantime becasue other chain has been finalized.
					// Don't mark it as bad as it still may be synced if explicitly requested.
					trace!(target: "sync", "Obsolete block");
				},
				Err(BlockImportError::UnknownParent) |
				Err(BlockImportError::Cancelled) |
				Err(BlockImportError::Other(_)) => {
					output.extend(self.restart());
				},
			};
		}

		for hash in hashes {
			self.queue_blocks.remove(&hash);
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
		if let Some(_) = self.fork_targets.remove(&hash) {
			trace!(target: "sync", "Completed fork sync {:?}", hash);
		}
		if number > self.best_queued_number {
			self.best_queued_number = number;
			self.best_queued_hash = *hash;
			// Update common blocks
			for (n, peer) in self.peers.iter_mut() {
				if let PeerSyncState::AncestorSearch(_, _) = peer.state {
					// Wait for ancestry search to complete first.
					continue;
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
		}
		self.is_idle = false;
	}

	/// Call when a node announces a new block.
	///
	/// If `OnBlockAnnounce::ImportHeader` is returned, then the caller MUST try to import passed
	/// header (call `on_block_data`). The network request isn't sent
	/// in this case. Both hash and header is passed as an optimization
	/// to avoid rehashing the header.
	pub fn on_block_announce(&mut self, who: PeerId, hash: B::Hash, announce: &BlockAnnounce<B::Header>, is_best: bool)
		-> OnBlockAnnounce
	{
		let header = &announce.header;
		let number = *header.number();
		debug!(target: "sync", "Received block announcement {:?} with number {:?} from {}", hash, number, who);
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
		if is_best {
			// update their best block
			peer.best_number = number;
			peer.best_hash = hash;
		}
		if let PeerSyncState::AncestorSearch(_, _) = peer.state {
			return OnBlockAnnounce::Nothing
		}
		// If the announced block is the best they have seen, our common number
		// is either one further ahead or it's the one they just announced, if we know about it.
		if is_best {
			if known {
				peer.common_number = number
			} else if header.parent_hash() == &self.best_queued_hash || known_parent {
				peer.common_number = number - One::one();
			}
		}
		self.is_idle = false;

		// known block case
		if known || self.is_already_downloading(&hash) {
			trace!(target: "sync", "Known block announce from {}: {}", who, hash);
			if let Some(target) = self.fork_targets.get_mut(&hash) {
				target.peers.insert(who);
			}
			return OnBlockAnnounce::Nothing
		}

		// Let external validator check the block announcement.
		let assoc_data = announce.data.as_ref().map_or(&[][..], |v| v.as_slice());
		match self.block_announce_validator.validate(&header, assoc_data) {
			Ok(Validation::Success) => (),
			Ok(Validation::Failure) => {
				debug!(target: "sync", "Block announcement validation of block {} from {} failed", hash, who);
				return OnBlockAnnounce::Nothing
			}
			Err(e) => {
				error!(target: "sync", "Block announcement validation errored: {}", e);
				return OnBlockAnnounce::Nothing
			}
		}

		if ancient_parent {
			trace!(target: "sync", "Ignored ancient block announced from {}: {} {:?}", who, hash, header);
			return OnBlockAnnounce::Nothing
		}

		let requires_additional_data = !self.role.is_light() || !known_parent;
		if !requires_additional_data {
			trace!(target: "sync", "Importing new header announced from {}: {} {:?}", who, hash, header);
			return OnBlockAnnounce::ImportHeader
		}

		if number <= self.best_queued_number {
			trace!(
				target: "sync",
				"Added sync target for block announced from {}: {} {:?}", who, hash, header
			);
			self.fork_targets
				.entry(hash.clone())
				.or_insert_with(|| ForkTarget {
					number,
					parent_hash: Some(header.parent_hash().clone()),
					peers: Default::default(),
				})
				.peers.insert(who);
		}

		OnBlockAnnounce::Nothing
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
	fn restart<'a>(&'a mut self) -> impl Iterator<Item = Result<(PeerId, BlockRequest<B>), BadPeer>> + 'a
	{
		self.queue_blocks.clear();
		self.blocks.clear();
		let info = self.client.info();
		self.best_queued_hash = info.best_hash;
		self.best_queued_number = std::cmp::max(info.best_number, self.best_imported_number);
		self.is_idle = false;
		debug!(target:"sync", "Restarted with {} ({})", self.best_queued_number, self.best_queued_hash);
		let old_peers = std::mem::replace(&mut self.peers, HashMap::new());
		old_peers.into_iter().filter_map(move |(id, p)| {
			match self.new_peer(id.clone(), p.best_hash, p.best_number) {
				Ok(None) => None,
				Ok(Some(x)) => Some(Ok((id, x))),
				Err(e) => Some(Err(e))
			}
		})
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
	max_parallel_downloads: u32,
	finalized: NumberFor<B>,
) -> Option<(Range<NumberFor<B>>, BlockRequest<B>)> {
	if peer.common_number < finalized {
		return None;
	}
	if let Some(range) = blocks.needed_blocks(
		id.clone(),
		MAX_BLOCKS_TO_REQUEST,
		peer.best_number,
		peer.common_number,
		max_parallel_downloads,
		MAX_DOWNLOAD_AHEAD,
	) {
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

/// Get pending fork sync targets for a peer.
fn fork_sync_request<B: BlockT>(
	id: &PeerId,
	targets: &mut HashMap<B::Hash, ForkTarget<B>>,
	best_num: NumberFor<B>,
	finalized: NumberFor<B>,
	attributes: &message::BlockAttributes,
	check_block: impl Fn(&B::Hash) -> BlockStatus,
) -> Option<(B::Hash, BlockRequest<B>)>
{
	targets.retain(|hash, r| if r.number > finalized {
		true
	} else {
		trace!(target: "sync", "Removed expired fork sync request {:?} (#{})", hash, r.number);
		false
	});
	for (hash, r) in targets {
		if !r.peers.contains(id) {
			continue
		}
		if r.number <= best_num {
			let parent_status = r.parent_hash.as_ref().map_or(BlockStatus::Unknown, check_block);
			let mut count = (r.number - finalized).saturated_into::<u32>(); // up to the last finalized block
			if parent_status != BlockStatus::Unknown {
				// request only single block
				count = 1;
			}
			trace!(target: "sync", "Downloading requested fork {:?} from {}, {} blocks", hash, id, count);
			return Some((hash.clone(), message::generic::BlockRequest {
				id: 0,
				fields: attributes.clone(),
				from: message::FromBlock::Hash(hash.clone()),
				to: None,
				direction: message::Direction::Descending,
				max: Some(count),
			}))
		}
	}
	None
}
