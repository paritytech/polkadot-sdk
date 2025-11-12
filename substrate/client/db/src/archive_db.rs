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

use std::{borrow::Borrow, marker::PhantomData, sync::Arc};

use array_bytes::Hex;
use codec::{Decode, Encode};
use sc_client_api::ChildInfo;
use sp_core::Hasher;
use sp_database::{Database, DatabaseWithSeekableIterator};
use sp_runtime::traits::{BlakeTwo256, BlockNumber, HashingFor, Header};
use sp_state_machine::{
	BackendTransaction, ChildStorageCollection, DefaultError, IterArgs, StorageCollection,
	StorageKey, StorageValue, UsageInfo,
};
use sp_trie::MerkleValue;

use crate::{columns, BlockT, DbHash, StateBackend, StateMachineStats, StorageDb};

pub(crate) struct ArchiveDb<Block: BlockT> {
	db: Arc<dyn DatabaseWithSeekableIterator<DbHash>>,
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
		db: Arc<dyn DatabaseWithSeekableIterator<DbHash>>,
		parent_hash: Option<Block::Hash>,
		block_number: <<Block as BlockT>::Header as Header>::Number,
	) -> Self {
		Self { db, parent_hash, block_number }
	}

	pub(crate) fn storage(&self, key: &[u8]) -> Result<Option<StorageValue>, DefaultError> {
		let full_key = FullStorageKey::new(key, self.block_number);
		let mut iter = self
			.db
			.seekable_iter(columns::ARCHIVE)
			.expect("Archive column space must exist if ArchiveDb exists");
		iter.seek_prev(full_key.as_ref());

		if let Some((found_key, value)) = iter.get() {
			let found_key = FullStorageKey::<<Block::Header as Header>::Number>::from(found_key);
			if found_key.key() == key {
				let value = match Option::<Vec<u8>>::decode(&mut value.as_slice()) {
					Ok(value) => value,
					Err(e) => return Err(format!("Archive value decode error: {:?}", e)),
				};
				return Ok(value);
			}
		}
		Ok(None)
	}

	pub(crate) fn storage_hash(
		&self,
		key: &[u8],
	) -> Result<Option<<HashingFor<Block> as hash_db::Hasher>::Out>, DefaultError> {
		let result = self.storage(key)?;
		Ok(result.map(|res| HashingFor::<Block>::hash(&res)))
	}

	pub(crate) fn child_storage(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<StorageValue>, DefaultError> {
		let mut prefix_key = child_info.storage_key().to_owned();
		prefix_key.extend_from_slice(key);
		self.storage(&prefix_key)
	}

	pub(crate) fn child_storage_hash(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<<HashingFor<Block> as hash_db::Hasher>::Out>, DefaultError> {
		let mut prefix_key = child_info.storage_key().to_owned();
		prefix_key.extend_from_slice(key);
		self.storage_hash(&prefix_key)
	}

	pub(crate) fn exists_storage(&self, key: &[u8]) -> Result<bool, DefaultError> {
		Ok(self.storage(key)?.is_some())
	}

	pub(crate) fn exists_child_storage(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<bool, DefaultError> {
		let mut prefix_key = child_info.storage_key().to_owned();
		prefix_key.extend_from_slice(key);
		Ok(self.exists_storage(&prefix_key)?)
	}

	fn make_next_lexicographic_key(key: &[u8]) -> Vec<u8> {
		let mut next_key = key.to_owned();
		next_key.push(0);
		next_key
	}

	pub(crate) fn next_storage_key(&self, key: &[u8]) -> Result<Option<StorageKey>, DefaultError> {
		let mut key = key.to_owned();
		loop {
			let next_key = Self::make_next_lexicographic_key(&key);
			let next_key = FullStorageKey::new(&next_key, self.block_number);
			let mut iter = self
				.db
				.seekable_iter(columns::ARCHIVE)
				.expect("Archive column space must exist if ArchiveDb exists");
			println!("Seek: {}, {}", next_key.key().hex("0x"), next_key.number());
			iter.seek(next_key.as_ref());

			if let Some((next_key, _)) = iter.get() {
				let next_key = FullStorageKey::<<Block::Header as Header>::Number>::from(next_key);
				println!("Found next key: {}, {}", next_key.key().hex("0x"), next_key.number());
				if next_key.number() != self.block_number {
					// this key points at a state older or newer than the current state,
					// we need the state either equal to or exactly preceding the current state
					println!("The found key is located at a non-current state, check if it's present in the current state");
					println!("Seek prev: {}, {}", next_key.key().hex("0x"), next_key.number());
					iter.seek_prev(FullStorageKey::new(next_key.key(), self.block_number).as_ref());
				}
				if let Some((next_key, encoded_value)) = iter.get() {
					let next_key =
						FullStorageKey::<<Block::Header as Header>::Number>::from(next_key);
					println!("Found next key: {}, {}", next_key.key().hex("0x"), next_key.number());
					if next_key.key() == key {
						// the found key does not appear at the current state, continue searching
						key = next_key.key().to_owned();
						println!("The found key is not present at the current state, continue");
						continue;
					} else {
						let value = match Option::<Vec<u8>>::decode(&mut encoded_value.as_slice()) {
							Ok(value) => value,
							Err(e) => return Err(format!("Archive value decode error: {:?}", e)),
						};
						if value.is_some() {
							return Ok(Some(next_key.key().to_owned()));
						} else {
							// the found key is deleted at the current state, continue
							// searching
							key = next_key.key().to_owned();
							println!("The found key is deleted at the current state, continue");
							continue;
						}
					}
				} else {
					unreachable!("Either hit the previous key here or find a suitable next key");
				}
			} else {
				// no keys in database greater than the provided key
				return Ok(None);
			}
		}
	}

	pub(crate) fn next_child_storage_key(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<StorageKey>, DefaultError> {
		let mut prefixed_key = child_info.storage_key().to_owned();
		prefixed_key.extend_from_slice(key);
		let next_key = self.next_storage_key(&prefixed_key)?;
		let next_key = match next_key {
			Some(key) => key,
			None => return Ok(None),
		};
		if next_key.starts_with(child_info.storage_key()) {
			Ok(Some(next_key))
		} else {
			Ok(None)
		}
	}

	pub(crate) fn raw_iter(&self, args: IterArgs) -> Result<RawIter<Block>, DefaultError> {
		Ok(RawIter::new(args))
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

enum RawIterState<K> {
	New,
	Iter(K),
	Complete,
}

pub(crate) struct RawIter<Block: BlockT> {
	state: RawIterState<FullStorageKey<'static, <Block::Header as Header>::Number>>,
	_phantom: std::marker::PhantomData<Block>,
}

impl<'a, Block: BlockT> RawIter<Block> {
	pub(crate) fn new(args: IterArgs) -> RawIter<Block> {
		RawIter {
			state: RawIterState::New,
			_phantom: Default::default(),
		}
	}

	pub(crate) fn next_key(
		&mut self,
		backend: &ArchiveDb<Block>,
	) -> Option<Result<StorageKey, DefaultError>> {
		// match self.state {
		// 	RawIterState::New => {
		// 		if let Some(key) = RawIterState::Iter(backend.next_storage_key(&[])?) {
					
		// 		}
		// 	},
		// 	RawIterState::Iter(_) => todo!(),
		// 	RawIterState::Complete => todo!(),
		// }
		None
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

#[derive(Clone)]
pub enum FullStorageKey<'a, BlockNumber> {
	Owned(Vec<u8>, PhantomData<BlockNumber>),
	Ref(&'a [u8], PhantomData<BlockNumber>),
}

impl<'a, BlockNumber> From<&'a [u8]> for FullStorageKey<'a, BlockNumber> {
	fn from(value: &'a [u8]) -> Self {
		FullStorageKey::Ref(value, PhantomData::default())
	}
}

impl<'a, BlockNumber> From<Vec<u8>> for FullStorageKey<'a, BlockNumber> {
	fn from(value: Vec<u8>) -> Self {
		FullStorageKey::Owned(value, PhantomData::default())
	}
}

impl<'a, BlockNumber> AsRef<[u8]> for FullStorageKey<'a, BlockNumber> {
	fn as_ref(&self) -> &[u8] {
		match self {
			FullStorageKey::Owned(items, _) => items.as_ref(),
			FullStorageKey::Ref(items, _) => items,
		}
	}
}

impl<'a, BlockNumber> Into<Vec<u8>> for FullStorageKey<'a, BlockNumber> {
	fn into(self) -> Vec<u8> {
		match self {
			FullStorageKey::Owned(items, _) => items,
			FullStorageKey::Ref(items, _) => items.to_vec(),
		}
	}
}

impl<'a, BlockNumber: Encode + Decode> FullStorageKey<'a, BlockNumber> {
	pub fn new(key: &[u8], number: BlockNumber) -> FullStorageKey<'static, BlockNumber> {
		let mut full_key = Vec::with_capacity(key.len() + number.encoded_size());
		full_key.extend_from_slice(&key[..]);
		number.encode_to(&mut &mut full_key);
		FullStorageKey::Owned(full_key, PhantomData::default())
	}

	pub fn key(&self) -> &[u8] {
		let key_len = self.as_ref().len() -
			BlockNumber::encoded_fixed_size()
				.expect("Variable length block numbers can't be used for archive storage");
		&self.as_ref()[..key_len]
	}

	pub fn number(&self) -> BlockNumber {
		let key_len = self.as_ref().len() -
			BlockNumber::encoded_fixed_size()
				.expect("Variable length block numbers can't be used for archive storage");
		BlockNumber::decode(&mut &self.as_ref()[key_len..])
			.expect("BlockNumber must be encoded correctly")
	}

	pub fn key_and_number(&self) -> (&[u8], BlockNumber) {
		(self.key(), self.number())
	}
}

impl<'a, BlockNumber> PartialEq for FullStorageKey<'a, BlockNumber> {
	fn eq(&self, other: &Self) -> bool {
		self.as_ref() == other.as_ref()
	}
}

impl<'a, BlockNumber> Eq for FullStorageKey<'a, BlockNumber> {}

impl<'a, BlockNumber: std::fmt::Display + Encode + Decode + PartialOrd> PartialOrd
	for FullStorageKey<'a, BlockNumber>
{
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		println!(
			"Cmp {}|{} with {}|{}: {:?}",
			self.key().hex("0x"),
			self.number(),
			other.key().hex("0x"),
			other.number(),
			self.key_and_number().partial_cmp(&other.key_and_number())
		);
		self.key_and_number().partial_cmp(&other.key_and_number())
	}
}

impl<'a, BlockNumber: std::fmt::Display + Encode + Decode + Ord> Ord
	for FullStorageKey<'a, BlockNumber>
{
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		println!(
			"Cmp {}|{} with {}|{}: {:?}",
			self.key().hex("0x"),
			self.number(),
			other.key().hex("0x"),
			other.number(),
			self.key_and_number().cmp(&other.key_and_number())
		);
		self.key_and_number().cmp(&other.key_and_number())
	}
}

impl<'a, BlockNumber: Clone + Ord + std::fmt::Display + Send + Sync + Encode + Decode>
	sp_database::GenericKey for FullStorageKey<'a, BlockNumber>
{
	type Key<'b> = FullStorageKey<'b, BlockNumber>;
}

#[cfg(test)]
mod tests {
	use super::*;

	use crate::columns::ARCHIVE;

	use sp_database::{Change, MemDb, Transaction};
	use sp_runtime::testing::{Block, MockCallU64, TestXt};

	type TestBlock = Block<TestXt<MockCallU64, ()>>;

	#[test]
	fn set_get() {
		let mut mem_db = Arc::new(MemDb::<FullStorageKey<u64>>::new());
		mem_db.commit(Transaction(vec![
			Change::<sp_core::H256>::Set(
				ARCHIVE,
				FullStorageKey::new(&[1, 2, 3], 4u64).into(),
				Some(vec![4, 2]).encode(),
			),
			Change::<sp_core::H256>::Set(
				ARCHIVE,
				FullStorageKey::new(&[1, 2, 3], 6u64).into(),
				Some(vec![5, 2]).encode(),
			),
		]));
		let archive_db =
			ArchiveDb::<TestBlock>::new(mem_db.clone(), Some(sp_core::H256::default()), 5);
		assert_eq!(archive_db.storage(&[1, 2, 3]), Ok(Some(vec![4u8, 2u8])));

		let archive_db = ArchiveDb::<TestBlock>::new(mem_db, Some(sp_core::H256::default()), 7);
		assert_eq!(archive_db.storage(&[1, 2, 3]), Ok(Some(vec![5u8, 2u8])));
	}

	#[test]
	fn next_storage_key() {
		let mut mem_db = Arc::new(MemDb::<FullStorageKey<'static, u64>>::new());
		mem_db.commit(Transaction(vec![
			Change::<sp_core::H256>::Set(
				ARCHIVE,
				FullStorageKey::new(&[1, 2, 3], 5u64).into(),
				Some(vec![1u8]).encode(),
			),
			Change::<sp_core::H256>::Set(
				ARCHIVE,
				FullStorageKey::new(&[1, 2, 4], 2u64).into(),
				Some(vec![2u8]).encode(),
			),
			Change::<sp_core::H256>::Set(
				ARCHIVE,
				FullStorageKey::new(&[1, 2, 4], 3u64).into(),
				None::<Vec<u8>>.encode(),
			),
			Change::<sp_core::H256>::Set(
				ARCHIVE,
				FullStorageKey::new(&[1, 2, 4], 6u64).into(),
				Some(vec![3u8]).encode(),
			),
			Change::<sp_core::H256>::Set(
				ARCHIVE,
				FullStorageKey::new(&[1, 2, 5], 1u64).into(),
				Some(vec![4u8]).encode(),
			),
			Change::<sp_core::H256>::Set(
				ARCHIVE,
				FullStorageKey::new(&[1, 2, 5], 5u64).into(),
				None::<Vec<u8>>.encode(),
			),
			Change::<sp_core::H256>::Set(
				ARCHIVE,
				FullStorageKey::new(&[1, 2, 5], 6u64).into(),
				Some(vec![5u8]).encode(),
			),
			Change::<sp_core::H256>::Set(
				ARCHIVE,
				FullStorageKey::new(&[1, 2, 6], 1u64).into(),
				Some(vec![6u8]).encode(),
			),
			Change::<sp_core::H256>::Set(
				ARCHIVE,
				FullStorageKey::new(&[1, 2, 6], 4u64).into(),
				Some(vec![7u8]).encode(),
			),
			Change::<sp_core::H256>::Set(
				ARCHIVE,
				FullStorageKey::new(&[1, 2, 6], 5u64).into(),
				Some(vec![8u8]).encode(),
			),
			Change::<sp_core::H256>::Set(
				ARCHIVE,
				FullStorageKey::new(&[1, 2, 6], 6u64).into(),
				None::<Vec<u8>>.encode(),
			),
		]));
		let archive_db =
			ArchiveDb::<TestBlock>::new(mem_db.clone(), Some(sp_core::H256::default()), 5);
		assert_eq!(archive_db.next_storage_key(&[1, 2, 3]), Ok(Some(vec![1, 2, 6])));
	}
}
