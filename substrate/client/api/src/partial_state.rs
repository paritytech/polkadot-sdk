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

//! Partial state: the trie nodes of a block's state, written to the database chunk by chunk
//! before the block itself is imported.

use codec::{Decode, Encode};
use sp_core::Hasher;
use sp_runtime::{
	traits::{Block as BlockT, PartialStateFor},
	Justifications,
};
use sp_state_machine::{Backend, IterArgs};
use sp_storage::{ChildInfo, ChildType, PrefixedStorageKey};

/// The block a state sync downloads the state of. Enough to import the block once its state is
/// complete, and to resume the download after a restart.
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq)]
pub struct PartialStateTarget<Block: BlockT> {
	/// The header. Its state root is where the download starts.
	pub header: Block::Header,
	/// The body, if the sync has it. Warp sync downloads it, fast sync does not.
	pub body: Option<Vec<Block::Extrinsic>>,
	/// The justifications, if the sync has them.
	pub justifications: Option<Justifications>,
}

/// A state sync that was begun and not completed, as the database recorded it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResumablePartialState<Block: BlockT> {
	/// The block whose state was being downloaded.
	pub target: PartialStateTarget<Block>,
	/// Where to continue: the start of the next range request. Empty before the first chunk.
	pub cursor: Vec<Vec<u8>>,
	/// Every chunk is written, only the import of the target block is left.
	pub complete: bool,
}

/// Writes the verified state of a block that is being state-synced, one chunk at a time.
///
/// `begin_partial_state` records the target, `import_partial_state` writes one chunk per state
/// response, and importing the target block with `ImportedState` ends the sync: the client checks
/// that the state is complete ([`PartialStateStore::check_complete`]) and marks the block's
/// import operation as carrying it
/// ([`BlockImportOperation::mark_state_synced`](crate::backend::BlockImportOperation::mark_state_synced)).
/// Until then `state_at` cannot see the nodes. A chunk is the nodes of one verified range proof
/// plus the range it covers, see `sp_trie::partial_state`.
///
/// The client implements this on top of the [`PartialStateStore`] its backend hands out, adding
/// the import lock. A backend without a store cannot state-sync, and every method fails.
///
/// The database records the target and the cursor every chunk leaves, so after a restart
/// `resumable_partial_state` says where to continue.
///
/// A state that never completes is not left behind. A sync only runs on a database with no block
/// state, so its nodes are all the state column holds, and the database drops them, records
/// included, as soon as anything else needs that column: a new sync beginning over it (a safety
/// net, the syncing strategies resume a recorded sync instead), the first block state committed
/// by other means (a restart in another sync mode), or the target import finding a node missing
/// after the last chunk. A download that stalls, because every peer pruned the target or the node
/// went offline, keeps its records until one of those happens. Nothing reads the nodes meanwhile.
pub trait PartialStateImport<Block: BlockT> {
	/// Record the target of a new sync. An earlier sync that never completed is discarded first.
	/// Fails, and records nothing, if the database already holds block state.
	fn begin_partial_state(&self, target: PartialStateTarget<Block>) -> sp_blockchain::Result<()>;

	/// Write the chunk's nodes and its range end as the new cursor, in one transaction. A chunk
	/// with no end marks the download complete. On `Err` nothing was written and the same chunk
	/// may be sent again. A chunk for another block or state root, one not starting at the
	/// cursor, or one after the final chunk is refused.
	fn import_partial_state(
		&self,
		partial_state: &PartialStateFor<Block>,
	) -> sp_blockchain::Result<()>;

	/// The sync recorded by `begin_partial_state` and not completed, with its cursor. `None` if
	/// nothing is recorded, or while an earlier sync is still being discarded.
	fn resumable_partial_state(
		&self,
	) -> sp_blockchain::Result<Option<ResumablePartialState<Block>>>;
}

/// The backend side of a state sync: where the chunks go and what the database remembers of
/// them. Taken from [`Backend::partial_state`](crate::Backend::partial_state). The store shares
/// the backend's database and locks: its nodes land in the state column the backend reads, and
/// the commit of the target block, marked with
/// [`BlockImportOperation::mark_state_synced`](crate::backend::BlockImportOperation::mark_state_synced),
/// completes what the store began.
pub trait PartialStateStore<Block: BlockT>: Send + Sync {
	/// Record the target of a new state sync. An earlier sync that never completed is discarded
	/// first, nodes and records, and a discard left unfinished is finished. Fails, and records
	/// nothing, if the database holds block state.
	fn begin(&self, target: PartialStateTarget<Block>) -> sp_blockchain::Result<()>;

	/// Write one chunk: the nodes its range covers, plus the range end as the cursor the download
	/// continues from, in one transaction. A chunk for another block or state root, one not
	/// starting at the cursor, or one after the final chunk is refused with nothing written. Each
	/// node must be written once per position, or reference counting breaks, and a range written
	/// over a hole would never complete. The cursor is kept in the database alone, so a restart
	/// continues from it.
	fn import(&self, chunk: &PartialStateFor<Block>) -> sp_blockchain::Result<()>;

	/// The recorded state sync that was not completed. `None` if nothing is recorded, or while an
	/// earlier sync is still being discarded.
	fn resumable(&self) -> sp_blockchain::Result<Option<ResumablePartialState<Block>>>;

	/// Check that every node of the state trie at `state_root` is in the database, child tries
	/// included. `state_root` must be the recorded target's, else fails and leaves everything as
	/// it was. If a node is missing, fails and discards the sync: a state still incomplete after
	/// the last chunk will never complete. Marking the target's import operation presumes this
	/// passed; the mark itself walks nothing.
	fn check_complete(&self, state_root: Block::Hash) -> sp_blockchain::Result<()>;
}

/// Walks the whole state trie at `state`, child tries included. Errors on the first missing node.
pub fn check_trie_complete<H, B>(state: B) -> sp_blockchain::Result<()>
where
	H: Hasher,
	B: Backend<H>,
{
	let map_err = |e| sp_blockchain::Error::Storage(format!("{e}"));
	for kv in state.pairs(IterArgs::default()).map_err(map_err)? {
		let (key, _value) = kv.map_err(map_err)?;
		let child_info = match ChildType::from_prefixed_key(PrefixedStorageKey::new_ref(&key)) {
			Some((ChildType::ParentKeyId, key)) => ChildInfo::new_default(key),
			None => continue,
		};
		let mut iter_args = IterArgs::default();
		iter_args.child_info = Some(child_info);
		for kv in state.pairs(iter_args).map_err(map_err)? {
			let _ = kv.map_err(map_err)?;
		}
	}
	Ok(())
}
