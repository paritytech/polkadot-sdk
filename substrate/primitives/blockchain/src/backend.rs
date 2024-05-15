// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Substrate blockchain trait

use log::warn;
use parking_lot::RwLock;
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, Header as HeaderT, NumberFor, Zero},
	Justifications,
};
use std::collections::{btree_map::BTreeMap, btree_set::BTreeSet};

use crate::header_metadata::HeaderMetadata;

use crate::{
	error::{Error, Result},
	tree_route, TreeRoute,
};

/// Blockchain database header backend. Does not perform any validation.
pub trait HeaderBackend<Block: BlockT>: Send + Sync {
	/// Get block header. Returns `None` if block is not found.
	fn header(&self, hash: Block::Hash) -> Result<Option<Block::Header>>;
	/// Get blockchain info.
	fn info(&self) -> Info<Block>;
	/// Get block status.
	fn status(&self, hash: Block::Hash) -> Result<BlockStatus>;
	/// Get block number by hash. Returns `None` if the header is not in the chain.
	fn number(
		&self,
		hash: Block::Hash,
	) -> Result<Option<<<Block as BlockT>::Header as HeaderT>::Number>>;
	/// Get block hash by number. Returns `None` if the header is not in the chain.
	fn hash(&self, number: NumberFor<Block>) -> Result<Option<Block::Hash>>;

	/// Convert an arbitrary block ID into a block hash.
	fn block_hash_from_id(&self, id: &BlockId<Block>) -> Result<Option<Block::Hash>> {
		match *id {
			BlockId::Hash(h) => Ok(Some(h)),
			BlockId::Number(n) => self.hash(n),
		}
	}

	/// Convert an arbitrary block ID into a block hash.
	fn block_number_from_id(&self, id: &BlockId<Block>) -> Result<Option<NumberFor<Block>>> {
		match *id {
			BlockId::Hash(h) => self.number(h),
			BlockId::Number(n) => Ok(Some(n)),
		}
	}

	/// Get block header. Returns `UnknownBlock` error if block is not found.
	fn expect_header(&self, hash: Block::Hash) -> Result<Block::Header> {
		self.header(hash)?
			.ok_or_else(|| Error::UnknownBlock(format!("Expect header: {}", hash)))
	}

	/// Convert an arbitrary block ID into a block number. Returns `UnknownBlock` error if block is
	/// not found.
	fn expect_block_number_from_id(&self, id: &BlockId<Block>) -> Result<NumberFor<Block>> {
		self.block_number_from_id(id).and_then(|n| {
			n.ok_or_else(|| Error::UnknownBlock(format!("Expect block number from id: {}", id)))
		})
	}

	/// Convert an arbitrary block ID into a block hash. Returns `UnknownBlock` error if block is
	/// not found.
	fn expect_block_hash_from_id(&self, id: &BlockId<Block>) -> Result<Block::Hash> {
		self.block_hash_from_id(id).and_then(|h| {
			h.ok_or_else(|| Error::UnknownBlock(format!("Expect block hash from id: {}", id)))
		})
	}
}

/// Handles stale forks.
pub trait ForkBackend<Block: BlockT>:
	HeaderMetadata<Block> + HeaderBackend<Block> + Send + Sync
{
	/// Returns block hashes for provided fork heads. It skips the fork if when blocks are missing
	/// (e.g. warp-sync) and internal `tree_route` function fails.
	///
	/// Example:
	///  G --- A1 --- A2 --- A3 --- A4           ( < fork1 )
	///                       \-----C4 --- C5    ( < fork2 )
	/// We finalize A3 and call expand_fork(C5). Result = (C5,C4).
	fn expand_forks(
		&self,
		fork_heads: &[Block::Hash],
	) -> std::result::Result<BTreeSet<Block::Hash>, Error> {
		let mut expanded_forks = BTreeSet::new();
		for fork_head in fork_heads {
			match tree_route(self, *fork_head, self.info().finalized_hash) {
				Ok(tree_route) => {
					for block in tree_route.retracted() {
						expanded_forks.insert(block.hash);
					}
					continue
				},
				Err(_) => {
					// There are cases when blocks are missing (e.g. warp-sync).
				},
			}
		}

		Ok(expanded_forks)
	}
}

impl<Block, T> ForkBackend<Block> for T
where
	Block: BlockT,
	T: HeaderMetadata<Block> + HeaderBackend<Block> + Send + Sync,
{
}

/// Blockchain database backend. Does not perform any validation.
pub trait Backend<Block: BlockT>:
	HeaderBackend<Block> + HeaderMetadata<Block, Error = Error>
{
	/// Get block body. Returns `None` if block is not found.
	fn body(&self, hash: Block::Hash) -> Result<Option<Vec<<Block as BlockT>::Extrinsic>>>;
	/// Get block justifications. Returns `None` if no justification exists.
	fn justifications(&self, hash: Block::Hash) -> Result<Option<Justifications>>;
	/// Get last finalized block hash.
	fn last_finalized(&self) -> Result<Block::Hash>;

	/// Returns hashes of all blocks that are leaves of the block tree.
	/// in other words, that have no children, are chain heads.
	/// Results must be ordered best (longest, highest) chain first.
	fn leaves(&self) -> Result<Vec<Block::Hash>>;

	/// Return hashes of all blocks that are children of the block with `parent_hash`.
	fn children(&self, parent_hash: Block::Hash) -> Result<Vec<Block::Hash>>;

	/// Get the most recent block hash of the longest chain that contains
	/// a block with the given `base_hash`.
	///
	/// The search space is always limited to blocks which are in the finalized
	/// chain or descendants of it.
	///
	/// Returns `Ok(None)` if `base_hash` is not found in search space.
	// TODO: document time complexity of this, see [#1444](https://github.com/paritytech/substrate/issues/1444)
	fn longest_containing(
		&self,
		base_hash: Block::Hash,
		import_lock: &RwLock<()>,
	) -> Result<Option<Block::Hash>> {
		let Some(base_header) = self.header(base_hash)? else { return Ok(None) };

		let leaves = {
			// ensure no blocks are imported during this code block.
			// an import could trigger a reorg which could change the canonical chain.
			// we depend on the canonical chain staying the same during this code block.
			let _import_guard = import_lock.read();
			let info = self.info();
			if info.finalized_number > *base_header.number() {
				// `base_header` is on a dead fork.
				return Ok(None)
			}
			self.leaves()?
		};

		// for each chain. longest chain first. shortest last
		for leaf_hash in leaves {
			let mut current_hash = leaf_hash;
			// go backwards through the chain (via parent links)
			loop {
				if current_hash == base_hash {
					return Ok(Some(leaf_hash))
				}

				let current_header = self
					.header(current_hash)?
					.ok_or_else(|| Error::MissingHeader(current_hash.to_string()))?;

				// stop search in this chain once we go below the target's block number
				if current_header.number() < base_header.number() {
					break
				}

				current_hash = *current_header.parent_hash();
			}
		}

		// header may be on a dead fork -- the only leaves that are considered are
		// those which can still be finalized.
		//
		// FIXME #1558 only issue this warning when not on a dead fork
		warn!(
			"Block {:?} exists in chain but not found when following all leaves backwards",
			base_hash,
		);

		Ok(None)
	}

	/// Get single indexed transaction by content hash. Note that this will only fetch transactions
	/// that are indexed by the runtime with `storage_index_transaction`.
	fn indexed_transaction(&self, hash: Block::Hash) -> Result<Option<Vec<u8>>>;

	/// Check if indexed transaction exists.
	fn has_indexed_transaction(&self, hash: Block::Hash) -> Result<bool> {
		Ok(self.indexed_transaction(hash)?.is_some())
	}

	fn block_indexed_body(&self, hash: Block::Hash) -> Result<Option<Vec<Vec<u8>>>>;

	/// Returns all leaves that will be displaced after the block finalization.
	fn displaced_leaves_after_finalizing(
		&self,
		finalized_block_hash: Block::Hash,
		finalized_block_number: NumberFor<Block>,
	) -> std::result::Result<DisplacedLeavesAfterFinalization<Block>, Error> {
		let mut result = DisplacedLeavesAfterFinalization::default();

		if finalized_block_number == Zero::zero() {
			return Ok(result)
		}

		// For each leaf determine whether it belongs to a non-canonical branch.
		for leaf_hash in self.leaves()? {
			let leaf_block_header = self.expect_header(leaf_hash)?;
			let leaf_number = *leaf_block_header.number();

			let leaf_tree_route = match tree_route(self, leaf_hash, finalized_block_hash) {
				Ok(tree_route) => tree_route,
				Err(Error::UnknownBlock(_)) => {
					// Sometimes routes can't be calculated. E.g. after warp sync.
					continue;
				},
				Err(e) => Err(e)?,
			};

			// Is it a stale fork?
			let needs_pruning = leaf_tree_route.common_block().hash != finalized_block_hash;

			if needs_pruning {
				result.displaced_leaves.insert(leaf_hash, leaf_number);
				result.tree_routes.insert(leaf_hash, leaf_tree_route);
			}
		}

		Ok(result)
	}
}

/// Result of  [`Backend::displaced_leaves_after_finalizing`].
#[derive(Clone, Debug)]
pub struct DisplacedLeavesAfterFinalization<Block: BlockT> {
	/// A collection of hashes and block numbers for displaced leaves.
	pub displaced_leaves: BTreeMap<Block::Hash, NumberFor<Block>>,

	/// A collection of tree routes from the leaves to finalized block.
	pub tree_routes: BTreeMap<Block::Hash, TreeRoute<Block>>,
}

impl<Block: BlockT> Default for DisplacedLeavesAfterFinalization<Block> {
	fn default() -> Self {
		Self { displaced_leaves: Default::default(), tree_routes: Default::default() }
	}
}

impl<Block: BlockT> DisplacedLeavesAfterFinalization<Block> {
	/// Returns a collection of hashes for the displaced leaves.
	pub fn hashes(&self) -> impl Iterator<Item = Block::Hash> + '_ {
		self.displaced_leaves.keys().cloned()
	}
}

/// Blockchain info
#[derive(Debug, Eq, PartialEq)]
pub struct Info<Block: BlockT> {
	/// Best block hash.
	pub best_hash: Block::Hash,
	/// Best block number.
	pub best_number: <<Block as BlockT>::Header as HeaderT>::Number,
	/// Genesis block hash.
	pub genesis_hash: Block::Hash,
	/// The head of the finalized chain.
	pub finalized_hash: Block::Hash,
	/// Last finalized block number.
	pub finalized_number: <<Block as BlockT>::Header as HeaderT>::Number,
	/// Last finalized state.
	pub finalized_state: Option<(Block::Hash, <<Block as BlockT>::Header as HeaderT>::Number)>,
	/// Number of concurrent leave forks.
	pub number_leaves: usize,
	/// Missing blocks after warp sync. (start, end).
	pub block_gap: Option<(NumberFor<Block>, NumberFor<Block>)>,
}

/// Block status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockStatus {
	/// Already in the blockchain.
	InChain,
	/// Not in the queue or the blockchain.
	Unknown,
}
