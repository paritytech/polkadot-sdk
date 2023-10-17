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

//! [`ChainSync`] trait.

use sc_network_common::sync::message::{BlockAnnounce, BlockRequest, BlockResponse};

use libp2p::PeerId;

use crate::types::{BadPeer, Metrics, PeerInfo, SyncStatus};
use sc_consensus::IncomingBlock;
use sp_consensus::BlockOrigin;
use sp_runtime::{
	traits::{Block as BlockT, NumberFor},
	Justifications,
};

/// Action that the parent of [`ChainSync`] should perform if we want to import blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportBlocksAction<B: BlockT> {
	pub origin: BlockOrigin,
	pub blocks: Vec<IncomingBlock<B>>,
}

/// Result of [`ChainSync::on_block_data`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnBlockData<Block: BlockT> {
	/// The block should be imported.
	Import(ImportBlocksAction<Block>),
	/// A new block request needs to be made to the given peer.
	Request(PeerId, BlockRequest<Block>),
	/// Continue processing events.
	Continue,
}

/// Result of [`ChainSync::on_block_justification`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnBlockJustification<Block: BlockT> {
	/// The justification needs no further handling.
	Nothing,
	/// The justification should be imported.
	Import {
		peer_id: PeerId,
		hash: Block::Hash,
		number: NumberFor<Block>,
		justifications: Justifications,
	},
}

/// Result of `ChainSync::on_state_data`.
#[derive(Debug)]
pub enum OnStateData<Block: BlockT> {
	/// The block and state that should be imported.
	Import(BlockOrigin, IncomingBlock<Block>),
	/// A new state request needs to be made to the given peer.
	Continue,
}

/// Something that represents the syncing strategy to download past and future blocks of the chain.
pub trait ChainSync<Block: BlockT>: Send {
	/// Returns the state of the sync of the given peer.
	///
	/// Returns `None` if the peer is unknown.
	fn peer_info(&self, who: &PeerId) -> Option<PeerInfo<Block>>;

	/// Returns the current sync status.
	fn status(&self) -> SyncStatus<Block>;

	/// Number of active forks requests. This includes
	/// requests that are pending or could be issued right away.
	fn num_sync_requests(&self) -> usize;

	/// Number of downloaded blocks.
	fn num_downloaded_blocks(&self) -> usize;

	/// Returns the current number of peers stored within this state machine.
	fn num_peers(&self) -> usize;

	/// Handle a new connected peer.
	///
	/// Call this method whenever we connect to a new peer.
	#[must_use]
	fn new_peer(
		&mut self,
		who: PeerId,
		best_hash: Block::Hash,
		best_number: NumberFor<Block>,
	) -> Result<Option<BlockRequest<Block>>, BadPeer>;

	/// Signal that a new best block has been imported.
	fn update_chain_info(&mut self, best_hash: &Block::Hash, best_number: NumberFor<Block>);

	/// Schedule a justification request for the given block.
	fn request_justification(&mut self, hash: &Block::Hash, number: NumberFor<Block>);

	/// Clear all pending justification requests.
	fn clear_justification_requests(&mut self);

	/// Request syncing for the given block from given set of peers.
	fn set_sync_fork_request(
		&mut self,
		peers: Vec<PeerId>,
		hash: &Block::Hash,
		number: NumberFor<Block>,
	);

	/// Handle a response from the remote to a block request that we made.
	///
	/// `request` must be the original request that triggered `response`.
	/// or `None` if data comes from the block announcement.
	///
	/// If this corresponds to a valid block, this outputs the block that
	/// must be imported in the import queue.
	#[must_use]
	fn on_block_data(
		&mut self,
		who: &PeerId,
		request: Option<BlockRequest<Block>>,
		response: BlockResponse<Block>,
	) -> Result<OnBlockData<Block>, BadPeer>;

	/// Handle a response from the remote to a justification request that we made.
	///
	/// `request` must be the original request that triggered `response`.
	#[must_use]
	fn on_block_justification(
		&mut self,
		who: PeerId,
		response: BlockResponse<Block>,
	) -> Result<OnBlockJustification<Block>, BadPeer>;

	/// Call this when a justification has been processed by the import queue,
	/// with or without errors.
	fn on_justification_import(
		&mut self,
		hash: Block::Hash,
		number: NumberFor<Block>,
		success: bool,
	);

	/// Notify about finalization of the given block.
	fn on_block_finalized(&mut self, hash: &Block::Hash, number: NumberFor<Block>);

	/// Notify about pre-validated block announcement.
	fn on_validated_block_announce(
		&mut self,
		is_best: bool,
		who: PeerId,
		announce: &BlockAnnounce<Block::Header>,
	);

	/// Call when a peer has disconnected.
	/// Canceled obsolete block request may result in some blocks being ready for
	/// import, so this functions checks for such blocks and returns them.
	#[must_use]
	fn peer_disconnected(&mut self, who: &PeerId) -> Option<ImportBlocksAction<Block>>;

	/// Return some key metrics.
	fn metrics(&self) -> Metrics;
}
