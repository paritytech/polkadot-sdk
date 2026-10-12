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

//! Partial state: trie nodes of a block's state written to the database before the block itself
//! is imported.

use codec::{Decode, Encode};
use sp_core::Hasher;
use sp_runtime::{
	traits::{Block as BlockT, PartialStateFor},
	Justifications,
};
use sp_state_machine::{Backend, IterArgs};
use sp_storage::{ChildInfo, ChildType, PrefixedStorageKey};

/// The block whose state a state sync downloads: what the sync needs to import it once the state
/// is complete, and what a restart needs to resume the download.
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq)]
pub struct PartialStateTarget<Block: BlockT> {
	/// The header; its state root is the root the state is downloaded from.
	pub header: Block::Header,
	/// The body, when the sync has it (warp sync downloads it, fast sync does not).
	pub body: Option<Vec<Block::Extrinsic>>,
	/// The justifications, when the sync has them.
	pub justifications: Option<Justifications>,
}

/// A state sync attempt that was begun and not completed, as the database recorded it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResumablePartialState<Block: BlockT> {
	/// The block whose state was being downloaded.
	pub target: PartialStateTarget<Block>,
	/// Where the download continues: the cursor the last written chunk left, which is the start
	/// of the next range request. Empty when no chunk was written.
	pub cursor: Vec<Vec<u8>>,
	/// Every chunk was written: only the import of the target block is left. The cursor is then
	/// the one the last chunk with a range end left.
	pub complete: bool,
}

/// Writes verified state trie nodes of a block that is being state-synced.
///
/// An attempt is begun with `begin_partial_state`, advanced with one `import_partial_state` per
/// state response, and completed by the import of the target block with `ImportedState`, when
/// `Backend::complete_partial_state` checks that the state is complete and marks it present. The
/// nodes stay invisible to `state_at` until then. A chunk is the nodes of one verified range
/// proof with the range it covers, see `sp_trie::partial_state`: the nodes whose subtree starts
/// inside that range are written, so that the chunks of one download write each position of the
/// state exactly once. A chunk is written once per successful call; the same chunk may be
/// submitted again after an `Err`, and one submitted again after an `Ok`, or out of order, is
/// refused.
///
/// The database records the attempt: its target once, and the range end of every chunk, the
/// cursor the download continues from. After a restart `resumable_partial_state` returns them, so
/// the download can continue where it stopped.
///
/// A state that never completes is never left behind. An attempt only ever runs on a database
/// that holds no block state, so its nodes are the only content of the state column, and the
/// database discards them together with the records as soon as anything else needs that column:
/// a new attempt beginning (after a restart whose target is too old to resume, or a state sync
/// that gave up), the first block state committed by other means (a restart in another sync
/// mode), or the target block's import finding a node missing once every chunk was written. A
/// download that stalls, because every peer pruned its target or the node went offline, keeps its
/// records until one of those happens; nothing reads the nodes meanwhile.
pub trait PartialStateImport<Block: BlockT> {
	/// Record the target of a new attempt. A recorded attempt that was not completed is discarded
	/// first. Fails, recording nothing, when the database holds block state. On `Err` the attempt
	/// must not start.
	fn begin_partial_state(&self, target: PartialStateTarget<Block>) -> sp_blockchain::Result<()>;

	/// Write the nodes of `partial_state` that its range covers, and its range end as the cursor
	/// the download continues from, in one transaction; a chunk without an end marks the download
	/// complete. On `Err` nothing of this call was persisted. A chunk is refused when its block or
	/// state root is not the recorded target's, when its `start` is not the recorded cursor, or
	/// when the final chunk was already written.
	fn import_partial_state(
		&self,
		partial_state: PartialStateFor<Block>,
	) -> sp_blockchain::Result<()>;

	/// The attempt recorded by `begin_partial_state` and not completed, with the cursor its last
	/// written chunk left. `None` when nothing is recorded, also while the discard of an earlier
	/// attempt is unfinished.
	fn resumable_partial_state(
		&self,
	) -> sp_blockchain::Result<Option<ResumablePartialState<Block>>>;
}

/// Walks the whole state trie of `state`, child tries included; errors on the first missing node.
pub fn check_state_complete<H, B>(state: B) -> sp_blockchain::Result<()>
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
