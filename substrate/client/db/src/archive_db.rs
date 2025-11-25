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

use std::{marker::PhantomData, sync::Arc};

use array_bytes::Hex;
use codec::{Decode, Encode};
use sc_client_api::ChildInfo;
use sp_core::Hasher;
use sp_database::{DatabaseWithSeekableIterator, Transaction};
use sp_runtime::{
	traits::{HashingFor, Header},
	Storage,
};
use sp_state_machine::{
	ChildStorageCollection, DefaultError, IterArgs, StorageCollection, StorageKey, StorageValue,
};

use crate::{columns, BlockT, DbHash};

pub(crate) fn compare_keys<B>(
	key1: &[u8],
	key2: &[u8],
) -> std::cmp::Ordering where B: Encode + Decode + Ord {
	let key1 = FullStorageKey::<B>::from(key1);
	let key2 = FullStorageKey::<B>::from(key2);
	key1.cmp(&key2)
}

pub struct ArchiveDb<Block: BlockT> {
	db: Arc<dyn DatabaseWithSeekableIterator<DbHash>>,
	parent_hash: Option<Block::Hash>,
	block_number: <<Block as BlockT>::Header as Header>::Number,
}

impl<B: BlockT> std::fmt::Debug for ArchiveDb<B> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("ArchiveDb").field("parent_hash", &self.parent_hash).finish()
	}
}

// Simply concatenets child storage key with key
// This could be troublesome if a child storage key could be a prefix of another child storage key,
// but ChildInfo's documentation mentions it should not happen
fn make_child_storage_key(info: &ChildInfo, key: &[u8]) -> Vec<u8> {
	let mut prefixed_key = Vec::with_capacity(info.storage_key().len() + key.len());
	prefixed_key.extend_from_slice(info.storage_key());
	prefixed_key.extend_from_slice(key);
	prefixed_key
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum StorageType {
	Main = 0,
	Child = 1,
}

impl<Block: BlockT> ArchiveDb<Block> {
	pub(crate) fn new(
		db: Arc<dyn DatabaseWithSeekableIterator<DbHash>>,
		parent_hash: Option<Block::Hash>,
		block_number: <<Block as BlockT>::Header as Header>::Number,
	) -> Self {
		Self { db, parent_hash, block_number }
	}

	/// Note that for StorageType::Child, child prefix should be appended to key
	fn storage(
		&self,
		storage_type: StorageType,
		key: &[u8],
	) -> Result<Option<StorageValue>, DefaultError> {
		let full_key = FullStorageKey::new(key, storage_type, self.block_number);
		let mut iter = self
			.db
			.seekable_iter(columns::ARCHIVE)
			.expect("Archive column space must exist if ArchiveDb exists");
		iter.seek_prev(full_key.as_ref());

		let res = {
			if let Some((found_key, value)) = iter.get() {
				let found_key =
					FullStorageKey::<<Block::Header as Header>::Number>::from(found_key);
				if found_key.key() == key {
					let value = match Option::<Vec<u8>>::decode(&mut value.as_slice()) {
						Ok(value) => value,
						Err(e) => return Err(format!("Archive value decode error: {:?}", e)),
					};
					Ok(value)
				} else {
					Ok(None)
				}
			} else {
				Ok(None)
			}
		};
		log::trace!("Archive storage query result: {} is {:?}", key.hex("0x"), res);
		res
	}

	pub(crate) fn storage_hash(
		&self,
		key: &[u8],
	) -> Result<Option<<HashingFor<Block> as hash_db::Hasher>::Out>, DefaultError> {
		let result = self.storage(StorageType::Main, key)?;
		Ok(result.map(|res| HashingFor::<Block>::hash(&res)))
	}

	pub(crate) fn main_storage(&self, key: &[u8]) -> Result<Option<StorageValue>, DefaultError> {
		self.storage(StorageType::Main, key)
	}

	pub(crate) fn child_storage(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<StorageValue>, DefaultError> {
		self.storage(StorageType::Child, &make_child_storage_key(child_info, key))
	}

	pub(crate) fn child_storage_hash(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<<HashingFor<Block> as hash_db::Hasher>::Out>, DefaultError> {
		self.storage_hash(&make_child_storage_key(child_info, key))
	}

	pub(crate) fn add_new_storage(
		transaction: &mut Transaction<DbHash>,
		storage: Storage,
		block_number: <Block::Header as Header>::Number,
	) {
		for (key, value) in storage.top {
			let full_key = FullStorageKey::new(&key, StorageType::Main, block_number);
			log::trace!(
				"Archive storage new pair: {} is {:?}",
				key.hex("0x"),
				value.as_slice().hex("0x")
			);

			transaction.set_from_vec(columns::ARCHIVE, full_key.as_ref(), Some(value).encode());
		}

		for (child_key, child_storage) in storage.children_default {
			let info = ChildInfo::new_default_from_vec(child_key);
			for (key, value) in child_storage.data {
				let full_key = FullStorageKey::new(
					&make_child_storage_key(&info, &key),
					StorageType::Child,
					block_number,
				);
				log::trace!(
					"Archive child storage {} new pair: {} is {:?}",
					info.storage_key().hex("0x"),
					key.hex("0x"),
					value.as_slice().hex("0x")
				);

				transaction.set_from_vec(columns::ARCHIVE, full_key.as_ref(), Some(value).encode());
			}
		}
	}

	pub(crate) fn update_storage(
		transaction: &mut Transaction<DbHash>,
		storage: StorageCollection,
		block_number: <Block::Header as Header>::Number,
	) {
		for (key, value) in storage {
			let full_key = FullStorageKey::new(&key, StorageType::Main, block_number);
			log::trace!(
				"Archive storage updated pair: {} is {:?}",
				key.hex("0x"),
				value.as_ref().map(|v| v.hex("0x"))
			);
			transaction.set_from_vec(columns::ARCHIVE, full_key.as_ref(), value.encode());
		}
	}

	pub(crate) fn update_child_storage(
		transaction: &mut Transaction<DbHash>,
		storage: ChildStorageCollection,
		block_number: <Block::Header as Header>::Number,
	) {
		for (child_key, storage) in storage {
			let info = ChildInfo::new_default_from_vec(child_key);
			for (key, value) in storage {
				let full_key = FullStorageKey::new(
					&make_child_storage_key(&info, &key),
					StorageType::Child,
					block_number,
				);
				log::trace!(
					"Archive child storage {} updated pair: {} is {:?}",
					info.storage_key().hex("0x"),
					key.hex("0x"),
					value.as_ref().map(|v| v.hex("0x"))
				);

				transaction.set_from_vec(columns::ARCHIVE, full_key.as_ref(), value.encode());
			}
		}
	}

	/// Since a key is an arbitrary sequence of bytes, the closest key that is greater than the
	/// given 'key' is 'key' + 0
	fn make_next_lexicographic_key(key: &[u8]) -> Vec<u8> {
		let mut next_key = key.to_owned();
		next_key.push(0);
		next_key
	}

	/// Note that for StorageType::Child, child prefix should be appended to key
	fn next_storage_key(
		&self,
		storage_type: StorageType,
		key: &[u8],
	) -> Result<Option<StorageKey>, DefaultError> {
		let mut key = key.to_owned();
		loop {
			let next_key = Self::make_next_lexicographic_key(&key);
			let next_key = FullStorageKey::new(&next_key, storage_type, self.block_number);
			let mut iter = self
				.db
				.seekable_iter(columns::ARCHIVE)
				.expect("Archive column space must exist if ArchiveDb exists");
			iter.seek(next_key.as_ref());

			if let Some((next_key, _)) = iter.get() {
				let next_key = FullStorageKey::<<Block::Header as Header>::Number>::from(next_key);
				// since child storage keys are ordered after main storage keys, if we iterated to a
				// child storage key, we passed all main storage keys
				if next_key.storage_type() != storage_type {
					return Ok(None);
				}
				if next_key.number() != self.block_number {
					// this key points at a state older or newer than the current state,
					// we need the state either equal to or exactly preceding the current state
					iter.seek_prev(
						FullStorageKey::new(next_key.key(), storage_type, self.block_number)
							.as_ref(),
					);
				}
				if let Some((next_key, encoded_value)) = iter.get() {
					let next_key =
						FullStorageKey::<<Block::Header as Header>::Number>::from(next_key);
					if next_key.key() == key {
						// the found key does not appear at the current state, continue searching
						key = next_key.key().to_owned();
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

	pub(crate) fn next_main_storage_key(
		&self,
		key: &[u8],
	) -> Result<Option<StorageKey>, DefaultError> {
		self.next_storage_key(StorageType::Main, key)
	}

	pub(crate) fn next_child_storage_key(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<StorageKey>, DefaultError> {
		self.next_storage_key(StorageType::Child, &make_child_storage_key(child_info, key))
	}

	pub(crate) fn raw_iter(&self, args: IterArgs) -> Result<RawIter<Block>, DefaultError> {
		Ok(RawIter::new(args))
	}
}

enum RawIterState {
	New { start_at: Option<Vec<u8>>, start_at_exclusive: bool },
	Iter(Vec<u8>),
	Complete,
}

pub struct RawIter<Block: BlockT> {
	storage_type: StorageType,
	prefix: Vec<u8>,
	state: RawIterState,
	_phantom: std::marker::PhantomData<Block>,
}

impl<Block: BlockT> RawIter<Block> {
	pub(crate) fn new(args: IterArgs) -> RawIter<Block> {
		let start = args.start_at.map(|v| v.to_owned());
		let child_prefix = args.child_info.as_ref().map(|info| info.storage_key()).unwrap_or(&[]);
		let mut full_prefix =
			Vec::with_capacity(child_prefix.len() + args.prefix.map(|p| p.len()).unwrap_or(0));
		if let Some(info) = &args.child_info {
			full_prefix.extend_from_slice(info.storage_key());
		}
		if let Some(prefix) = args.prefix {
			full_prefix.extend_from_slice(&prefix);
		}

		let start = if let Some(start) = start {
			if let Some(info) = &args.child_info {
				Some(make_child_storage_key(info, &start))
			} else {
				Some(start)
			}
		} else {
			None
		};

		RawIter {
			prefix: full_prefix,
			state: RawIterState::New {
				start_at: start,
				start_at_exclusive: args.start_at_exclusive,
			},
			storage_type: match args.child_info {
				Some(_) => StorageType::Child,
				None => StorageType::Main,
			},
			_phantom: Default::default(),
		}
	}

	pub fn next_key(
		&mut self,
		backend: &ArchiveDb<Block>,
	) -> Option<Result<StorageKey, DefaultError>> {
		self.state = match self.next_state(backend) {
			Ok(s) => s,
			Err(e) => return Some(Err(e)),
		};
		match &self.state {
			RawIterState::New { .. } => unreachable!(), // because we just got the next state
			RawIterState::Iter(key) => Some(Ok(key.clone())),
			RawIterState::Complete => None,
		}
	}

	pub fn next_pair(
		&mut self,
		backend: &ArchiveDb<Block>,
	) -> Option<Result<(StorageKey, StorageValue), DefaultError>> {
		match self.next_key(backend)? {
			Ok(key) => match backend.storage(self.storage_type, &key) {
				Ok(Some(value)) => Some(Ok((key, value))),
				Ok(None) => unreachable!(), // because why would next key return it
				Err(e) => Some(Err(e)),
			},
			Err(e) => Some(Err(e)),
		}
	}

	fn check_for_completion(&self, key: Option<Vec<u8>>) -> RawIterState {
		if let Some(key) = key {
			if key.starts_with(&self.prefix) {
				RawIterState::Iter(key.into())
			} else {
				RawIterState::Complete
			}
		} else {
			RawIterState::Complete
		}
	}

	fn next_state(&self, backend: &ArchiveDb<Block>) -> Result<RawIterState, DefaultError> {
		Ok(match &self.state {
			RawIterState::New { start_at, start_at_exclusive } =>
				if let Some(start_at) = start_at {
					if !*start_at_exclusive {
						if backend.storage(self.storage_type, &start_at)?.is_some() {
							RawIterState::Iter(start_at.clone().into())
						} else {
							let next_key =
								backend.next_storage_key(self.storage_type, &start_at)?;
							self.check_for_completion(next_key)
						}
					} else {
						let next_key = backend.next_storage_key(self.storage_type, &start_at)?;
						backend.next_storage_key(self.storage_type, &start_at)?;
						self.check_for_completion(next_key)
					}
				} else {
					if backend.storage(self.storage_type, &self.prefix)?.is_some() {
						RawIterState::Iter(self.prefix.clone().into())
					} else {
						let next_key = backend.next_storage_key(self.storage_type, &self.prefix)?;
						self.check_for_completion(next_key)
					}
				},
			RawIterState::Iter(current_key) => {
				let next_key = backend.next_storage_key(self.storage_type, current_key.as_ref())?;
				self.check_for_completion(next_key)
			},
			RawIterState::Complete => RawIterState::Complete,
		})
	}

	pub fn was_complete(&self) -> bool {
		matches!(self.state, RawIterState::Complete)
	}
}

#[derive(Clone)]
enum FullStorageKey<'a, BlockNumber> {
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
	pub fn new(
		key: &[u8],
		storage_type: StorageType,
		number: BlockNumber,
	) -> FullStorageKey<'static, BlockNumber> {
		let mut full_key = Vec::with_capacity(key.len() + 1 + number.encoded_size());
		full_key.push(storage_type as u8);
		full_key.extend_from_slice(&key[..]);
		number.encode_to(&mut &mut full_key);
		FullStorageKey::Owned(full_key, PhantomData::default())
	}

	pub fn key(&self) -> &[u8] {
		let key_end = self.as_ref().len() - self.number_size();
		&self.as_ref()[1..key_end]
	}

	pub fn number(&self) -> BlockNumber {
		let type_and_key_len = self.as_ref().len() - self.number_size();
		BlockNumber::decode(&mut &self.as_ref()[type_and_key_len..])
			.expect("BlockNumber must be encoded correctly")
	}

	pub fn storage_type(&self) -> StorageType {
		let slice = self.as_ref();
		match slice[0] {
			0 => StorageType::Main,
			1 => StorageType::Child,
			_ => panic!("Broken archive storage key"),
		}
	}

	pub fn as_tuple(&self) -> (StorageType, &[u8], BlockNumber) {
		(self.storage_type(), self.key(), self.number())
	}

	fn number_size(&self) -> usize {
		BlockNumber::encoded_fixed_size()
			.expect("Variable length block numbers can't be used for archive storage")
	}
}

impl<'a, BlockNumber> PartialEq for FullStorageKey<'a, BlockNumber> {
	fn eq(&self, other: &Self) -> bool {
		self.as_ref() == other.as_ref()
	}
}

impl<'a, BlockNumber> Eq for FullStorageKey<'a, BlockNumber> {}

impl<'a, BlockNumber: Encode + Decode + PartialOrd> PartialOrd
	for FullStorageKey<'a, BlockNumber>
{
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		self.as_tuple().partial_cmp(&other.as_tuple())
	}
}

impl<'a, BlockNumber: Encode + Decode + Ord> Ord
	for FullStorageKey<'a, BlockNumber>
{
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		self.as_tuple().cmp(&other.as_tuple())
	}
}

impl<'a, BlockNumber: Encode + Decode + Ord> sp_database::MemDbComparator for FullStorageKey<'a, BlockNumber> {
	fn cmp(k1: &[u8], k2: &[u8]) -> std::cmp::Ordering {
		compare_keys::<BlockNumber>(k1, k2)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use crate::columns::ARCHIVE;

	use sp_database::{Change, Database, MemDb, Transaction};
	use sp_runtime::testing::{Block, MockCallU64, TestXt};

	type TestBlock = Block<TestXt<MockCallU64, ()>>;

	#[test]
	fn full_key_encoded_correctly() {
		let key = FullStorageKey::new(&[2, 3, 4], StorageType::Child, 5);
		assert_eq!(key.as_tuple(), (StorageType::Child, [2u8, 3u8, 4u8].as_slice(), 5));
		assert_eq!(key.as_ref(), &[1, 2, 3, 4, 5, 0, 0, 0]);
	}

	fn create_db<H: Clone + AsRef<[u8]>>(
		changes: Vec<(StorageType, &[u8], u64, Option<&[u8]>)>,
	) -> Arc<MemDb<FullStorageKey<'static, u64>>> {
		let db = Arc::new(MemDb::<FullStorageKey<u64>>::new());
		db.commit(Transaction(
			changes
				.into_iter()
				.map(|(storage_type, key, block, value)| {
					Change::<H>::Set(
						ARCHIVE,
						FullStorageKey::new(key, storage_type, block).into(),
						value.encode(),
					)
				})
				.collect(),
		))
		.unwrap();
		db
	}

	#[test]
	fn set_get() {
		let mem_db = create_db::<sp_core::H256>(vec![
			(StorageType::Main, &[1, 2, 3], 4u64, Some(&[4, 2])),
			(StorageType::Main, &[1, 2, 3], 6u64, Some(&[5, 2])),
		]);

		let archive_db =
			ArchiveDb::<TestBlock>::new(mem_db.clone(), Some(sp_core::H256::default()), 5);
		assert_eq!(archive_db.main_storage(&[1, 2, 3]), Ok(Some(vec![4u8, 2u8])));

		let archive_db = ArchiveDb::<TestBlock>::new(mem_db, Some(sp_core::H256::default()), 7);
		assert_eq!(archive_db.main_storage(&[1, 2, 3]), Ok(Some(vec![5u8, 2u8])));
	}

	#[test]
	fn next_storage_key() {
		let mem_db = create_db::<sp_core::H256>(vec![
			(StorageType::Main, &[1, 2, 3], 5u64, Some(&[1])),
			(StorageType::Main, &[1, 2, 4], 2u64, Some(&[2])),
			(StorageType::Main, &[1, 2, 4], 3u64, None),
			(StorageType::Main, &[1, 2, 4], 6u64, Some(&[3])),
			(StorageType::Main, &[1, 2, 5], 1u64, Some(&[4])),
			(StorageType::Main, &[1, 2, 5], 5u64, None),
			(StorageType::Main, &[1, 2, 5], 6u64, Some(&[5])),
			(StorageType::Main, &[1, 2, 6], 1u64, Some(&[6])),
			(StorageType::Main, &[1, 2, 6], 4u64, Some(&[7])),
			(StorageType::Main, &[1, 2, 6], 5u64, Some(&[8])),
			(StorageType::Main, &[1, 2, 6], 6u64, None),
		]);
		let archive_db =
			ArchiveDb::<TestBlock>::new(mem_db.clone(), Some(sp_core::H256::default()), 5);
		assert_eq!(archive_db.next_main_storage_key(&[1, 2, 3]), Ok(Some(vec![1, 2, 6])));
	}

	#[test]
	fn raw_iter_next_key() {
		let mem_db = create_db::<sp_core::H256>(vec![
			(StorageType::Main, &[1, 2, 3], 5u64, Some(&[1])),
			(StorageType::Main, &[1, 2, 4], 2u64, Some(&[2])),
			(StorageType::Main, &[1, 2, 4], 3u64, None),
			(StorageType::Main, &[1, 2, 4], 6u64, Some(&[3])),
			(StorageType::Main, &[1, 2, 5], 1u64, Some(&[4])),
			(StorageType::Main, &[1, 2, 5], 5u64, None),
			(StorageType::Main, &[1, 2, 5], 6u64, Some(&[5])),
			(StorageType::Main, &[1, 2, 6], 1u64, Some(&[6])),
			(StorageType::Main, &[1, 2, 6], 4u64, Some(&[7])),
			(StorageType::Main, &[1, 2, 6], 5u64, Some(&[8])),
			(StorageType::Main, &[1, 2, 6], 6u64, None),
		]);
		let archive_db =
			ArchiveDb::<TestBlock>::new(mem_db.clone(), Some(sp_core::H256::default()), 5);

		let mut args = IterArgs::default();
		args.start_at = Some(&[1, 2, 3]);
		args.start_at_exclusive = true;
		let mut iter = archive_db.raw_iter(args).unwrap();
		assert_eq!(iter.next_key(&archive_db), Some(Ok(vec![1, 2, 6])));

		let mut args = IterArgs::default();
		args.start_at = Some(&[1, 2, 3]);
		args.start_at_exclusive = false;
		let mut iter = archive_db.raw_iter(args).unwrap();
		assert_eq!(iter.next_key(&archive_db), Some(Ok(vec![1, 2, 3])));
	}

	#[test]
	fn raw_iter_prefix() {
		let mem_db = create_db::<sp_core::H256>(vec![
			(StorageType::Main, &[1, 2, 3], 1u64, Some(&[1])),
			(StorageType::Main, &[1, 3, 1], 1u64, Some(&[2])),
			(StorageType::Main, &[1, 3, 2], 1u64, Some(&[3])),
			(StorageType::Main, &[1, 4, 1], 1u64, Some(&[4])),
			(StorageType::Main, &[1, 3, 1], 2u64, None),
			(StorageType::Main, &[1, 3, 3], 2u64, Some(&[5])),
			(StorageType::Child, &[1, 3, 2], 3u64, Some(&[6])),
			(StorageType::Child, &[1, 3, 4], 3u64, Some(&[6])),
		]);
		let archive_db =
			ArchiveDb::<TestBlock>::new(mem_db.clone(), Some(sp_core::H256::default()), 3);

		let mut args = IterArgs::default();
		args.prefix = Some(&[1, 3]);
		let mut iter = archive_db.raw_iter(args).unwrap();

		assert_eq!(iter.next_key(&archive_db), Some(Ok(vec![1, 3, 2])));
		assert_eq!(iter.next_key(&archive_db), Some(Ok(vec![1, 3, 3])));
		assert_eq!(iter.next_key(&archive_db), None);
	}

	#[test]
	fn raw_iter_child_storage() {
		let mem_db = create_db::<sp_core::H256>(vec![
			(StorageType::Child, &[1, 2, 3], 1u64, Some(&[1])),
			(StorageType::Main, &[1, 3], 1u64, Some(&[2])),
			(StorageType::Child, &[1, 3, 1], 1u64, Some(&[3])),
			(StorageType::Child, &[1, 3, 2], 1u64, Some(&[4])),
			(StorageType::Child, &[1, 4, 1], 1u64, Some(&[5])),
			(StorageType::Main, &[1, 3], 3u64, Some(&[6])),
			(StorageType::Child, &[1, 3, 1], 2u64, None),
			(StorageType::Child, &[1, 3, 3], 2u64, Some(&[7])),
		]);
		let archive_db =
			ArchiveDb::<TestBlock>::new(mem_db.clone(), Some(sp_core::H256::default()), 3);

		let mut args = IterArgs::default();
		args.child_info = Some(ChildInfo::new_default_from_vec(vec![1, 3]));
		let mut iter = archive_db.raw_iter(args).unwrap();

		assert_eq!(iter.next_key(&archive_db), Some(Ok(vec![1, 3, 2])));
		assert_eq!(iter.next_key(&archive_db), Some(Ok(vec![1, 3, 3])));
		assert_eq!(iter.next_key(&archive_db), None);
	}
}
