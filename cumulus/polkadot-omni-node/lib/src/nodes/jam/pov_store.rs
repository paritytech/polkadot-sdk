// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! The PoV store: per parachain block, everything its PoV is rebuilt from.
//!
//! A work package carries its author's randomised signature, so nobody — the author included —
//! can recompute a block's package; a block whose package was lost can only travel again in a new
//! one. Its PoV needs the block, its parent header, its additional-data map and the storage proof
//! the block executes with. The proof only exists while the block executes, so the entry is
//! recorded then: by the import path for every block it re-executes, and by the collation task for
//! every block this collator authored.

use cumulus_primitives_core::JamParent;
use jam_types::ExtrinsicSpec;
use schnellru::{ByLength, LruMap};
use sp_additional_data::AdditionalData;
use sp_runtime::traits::{Block as BlockT, NumberFor};
use sp_trie::CompactProof;
use std::sync::{Arc, Mutex};

/// Capacity of the PoV store, in blocks.
pub(crate) const BLOCK_POVS_CAPACITY: u32 = 64;

/// What one block's PoV is rebuilt from: every input of [`build_pov`](super::package::build_pov),
/// plus the block's place in the chain and the JAM anchors it was built against.
pub(crate) struct BlockPov<Block: BlockT> {
	/// The block itself.
	pub(crate) block: Block,
	/// The header of the block's parent, which the PoV carries.
	pub(crate) parent_header: Block::Header,
	/// The block's parent, whose header the PoV carries.
	pub(crate) parent_hash: Block::Hash,
	/// The block's number, kept for pruning and logging.
	pub(crate) number: NumberFor<Block>,
	/// The PoV spec the rebuild must reproduce.
	pub(crate) spec: ExtrinsicSpec,
	/// The `JamParent` digest the block carries: anchor, lookup anchor and their slots.
	pub(crate) jam_parent: JamParent,
	/// The compact proof the block executes with: recorded by import re-execution, or kept from
	/// authoring for a block this collator built.
	pub(crate) compact_proof: CompactProof,
	/// The additional-data map the block carried.
	pub(crate) additional_data: AdditionalData,
}

impl<Block: BlockT> Clone for BlockPov<Block> {
	fn clone(&self) -> Self {
		Self {
			block: self.block.clone(),
			parent_header: self.parent_header.clone(),
			parent_hash: self.parent_hash,
			number: self.number,
			spec: self.spec.clone(),
			jam_parent: self.jam_parent,
			compact_proof: self.compact_proof.clone(),
			additional_data: self.additional_data.clone(),
		}
	}
}

/// The PoV store, keyed by the block each entry belongs to and bounded: when it is full the least
/// recently used entry goes.
#[derive(Clone)]
pub(crate) struct BlockPovs<Block: BlockT>(
	Arc<Mutex<LruMap<Block::Hash, BlockPov<Block>, ByLength>>>,
);

impl<Block: BlockT> BlockPovs<Block> {
	/// A store holding at most `capacity` blocks.
	pub(crate) fn new(capacity: u32) -> Self {
		Self(Arc::new(Mutex::new(LruMap::new(ByLength::new(capacity)))))
	}

	fn map(&self) -> std::sync::MutexGuard<'_, LruMap<Block::Hash, BlockPov<Block>, ByLength>> {
		self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
	}

	/// Record `pov` for `hash`, evicting the least recently used entry if full.
	pub(crate) fn insert(&self, hash: Block::Hash, pov: BlockPov<Block>) {
		self.map().insert(hash, pov);
	}

	/// Retrieve a clone of the entry for `hash`, promoting it to most-recently-used.
	/// `None` when the block was never recorded or has been evicted.
	pub(crate) fn get(&self, hash: &Block::Hash) -> Option<BlockPov<Block>> {
		self.map().get(hash).cloned()
	}
}
