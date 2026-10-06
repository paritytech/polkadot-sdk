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

//! Partial state: trie nodes of a block's state written to the database before the block itself
//! is imported.

use hash_db::Hasher;
use sp_runtime::traits::{Block as BlockT, PartialStateFor};
use sp_state_machine::{Backend, IterArgs};
use sp_storage::{ChildInfo, ChildType, PrefixedStorageKey};

/// Writes verified state trie nodes of a block that is being state-synced.
///
/// State sync calls this once per state response, before it moves its download cursor. The
/// block itself is not imported: the nodes stay invisible to `state_at` until the block is
/// imported with `ImportedState`, which checks that the state is complete and marks it
/// present. A chunk is the nodes of one verified range proof, see `sp_trie::partial_state`.
/// Nodes already written, or not awaited by the target's walker, are ignored, so the same chunk
/// may be submitted again after an `Err`.
pub trait PartialStateImport<Block: BlockT> {
	/// Write the nodes of `partial_state`. On `Err` nothing of this call was persisted.
	fn import_partial_state(
		&self,
		partial_state: PartialStateFor<Block>,
	) -> sp_blockchain::Result<()>;
}

/// Checks that database has all state trie nodes for given state root.
pub fn check_have_complete_state<H, B>(state: B) -> sp_blockchain::Result<()>
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
