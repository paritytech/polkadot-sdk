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

use std::sync::Arc;

use codec::Encode;
use sc_client_api::ChildInfo;
use sp_core::Hasher;
use sp_database::OrderedDatabase;
use sp_runtime::traits::{BlakeTwo256, BlockNumber, HashingFor, Header};
use sp_state_machine::{
	BackendTransaction, ChildStorageCollection, DefaultError, IterArgs, StorageCollection,
	StorageKey, StorageValue, UsageInfo,
};
use sp_trie::MerkleValue;

use crate::{columns, BlockT, DbHash, StateBackend, StateMachineStats, StorageDb};

pub(crate) struct ArchiveDb<Block: BlockT> {
	db: Arc<dyn OrderedDatabase<DbHash>>,
	parent_hash: Option<Block::Hash>,
	block_number: <<Block as BlockT>::Header as Header>::Number,
}

impl<B: BlockT> std::fmt::Debug for ArchiveDb<B> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("ArchiveDb").field("parent_hash", &self.parent_hash).finish()
	}
}

impl<Block: BlockT> ArchiveDb<Block> {
	pub(crate) fn new(
		db: Arc<dyn OrderedDatabase<DbHash>>,
		parent_hash: Option<Block::Hash>,
		block_number: <<Block as BlockT>::Header as Header>::Number,
	) -> Self {
		Self { db, parent_hash, block_number }
	}

	pub(crate) fn storage(&self, key: &[u8]) -> Result<Option<StorageValue>, DefaultError> {
		let full_key = make_full_key(key, self.block_number);
		Ok(self.db.get(columns::ARCHIVE, &full_key))
	}

	pub(crate) fn storage_hash(
		&self,
		key: &[u8],
	) -> Result<Option<<HashingFor<Block> as hash_db::Hasher>::Out>, DefaultError> {
		let full_key = make_full_key(key, self.block_number);

		if let Some(value) = self.db.get(columns::ARCHIVE, &full_key) {
			Ok(Some(HashingFor::<Block>::hash(&value)))
		} else {
			Ok(None)
		}
	}

	pub(crate) fn child_storage(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<StorageValue>, DefaultError> {
		todo!()
	}

	pub(crate) fn child_storage_hash(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<<HashingFor<Block> as hash_db::Hasher>::Out>, DefaultError> {
		todo!()
	}

	pub(crate) fn exists_storage(&self, key: &[u8]) -> Result<bool, DefaultError> {
		todo!()
	}

	pub(crate) fn exists_child_storage(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<bool, DefaultError> {
		todo!()
	}

	pub(crate) fn next_storage_key(&self, key: &[u8]) -> Result<Option<StorageKey>, DefaultError> {
		todo!()
	}

	pub(crate) fn next_child_storage_key(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<StorageKey>, DefaultError> {
		todo!()
	}

	pub(crate) fn raw_iter(&self, args: IterArgs) -> Result<RawIter<Block>, DefaultError> {
		todo!()
	}

	pub(crate) fn register_overlay_stats(&self, _stats: &crate::StateMachineStats) {
		todo!()
	}

	pub(crate) fn usage_info(&self) -> UsageInfo {
		todo!()
	}

	pub(crate) fn wipe(&self) -> Result<(), DefaultError> {
		unimplemented!()
	}

	pub(crate) fn commit(
		&self,
		_: <HashingFor<Block> as Hasher>::Out,
		_: BackendTransaction<HashingFor<Block>>,
		_: StorageCollection,
		_: ChildStorageCollection,
	) -> Result<(), DefaultError> {
		unimplemented!()
	}

	pub(crate) fn read_write_count(&self) -> (u32, u32, u32, u32) {
		unimplemented!()
	}

	pub(crate) fn reset_read_write_count(&self) {
		unimplemented!()
	}

	pub(crate) fn get_read_and_written_keys(&self) -> Vec<(Vec<u8>, u32, u32, bool)> {
		unimplemented!()
	}
}

pub struct RawIter<Block: BlockT> {
	_phantom: std::marker::PhantomData<Block>,
}

impl<Block: BlockT> RawIter<Block> {
	pub(crate) fn next_key(
		&mut self,
		backend: &ArchiveDb<Block>,
	) -> Option<Result<StorageKey, DefaultError>> {
		unimplemented!()
	}

	pub(crate) fn next_pair(
		&mut self,
		backend: &ArchiveDb<Block>,
	) -> Option<Result<(StorageKey, StorageValue), DefaultError>> {
		unimplemented!()
	}

	pub(crate) fn was_complete(&self) -> bool {
		unimplemented!()
	}
}

pub(crate) fn make_full_key(key: &[u8], number: impl Encode) -> Vec<u8> {
	let mut full_key = Vec::with_capacity(key.len() + number.encoded_size());
	full_key.extend_from_slice(&key[..]);
	number.encode_to(&mut &mut full_key);
	full_key
}
