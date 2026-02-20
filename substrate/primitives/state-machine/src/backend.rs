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

//! State machine backends. These manage the code and storage of contracts.

#[cfg(feature = "std")]
use crate::trie_backend::TrieBackend;
use crate::{
	trie_backend_essence::TrieBackendStorage, ChildStorageCollection, StorageCollection,
	StorageKey, StorageValue, UsageInfo,
};
use alloc::vec::Vec;
use codec::Encode;
use core::marker::PhantomData;
use hash_db::Hasher;
#[cfg(feature = "std")]
use nomt::Overlay as NomtOverlay;
use sp_core::storage::{ChildInfo, StateVersion, TrackedStorageKey};
#[cfg(feature = "std")]
use sp_core::traits::RuntimeCode;
use sp_trie::{MerkleValue, PrefixedMemoryDB, RandomState};

/// A struct containing arguments for iterating over the storage.
#[derive(Default)]
#[non_exhaustive]
pub struct IterArgs<'a> {
	/// The prefix of the keys over which to iterate.
	pub prefix: Option<&'a [u8]>,

	/// The prefix from which to start the iteration from.
	///
	/// This is inclusive and the iteration will include the key which is specified here.
	pub start_at: Option<&'a [u8]>,

	/// If this is `true` then the iteration will *not* include
	/// the key specified in `start_at`, if there is such a key.
	pub start_at_exclusive: bool,

	/// The info of the child trie over which to iterate over.
	pub child_info: Option<ChildInfo>,

	/// Whether to stop iteration when a missing trie node is reached.
	///
	/// When a missing trie node is reached the iterator will:
	///   - return an error if this is set to `false` (default)
	///   - return `None` if this is set to `true`
	pub stop_on_incomplete_database: bool,
}

/// A trait for a raw storage iterator.
pub trait StorageIterator<H>
where
	H: Hasher,
{
	/// The state backend over which the iterator is iterating.
	type Backend;

	/// The error type.
	type Error;

	/// Fetches the next key from the storage.
	fn next_key(
		&mut self,
		backend: &Self::Backend,
	) -> Option<core::result::Result<StorageKey, Self::Error>>;

	/// Fetches the next key and value from the storage.
	fn next_pair(
		&mut self,
		backend: &Self::Backend,
	) -> Option<core::result::Result<(StorageKey, StorageValue), Self::Error>>;

	/// Returns whether the end of iteration was reached without an error.
	fn was_complete(&self) -> bool;
}

/// An iterator over storage keys and values.
pub struct PairsIter<'a, H, I>
where
	H: Hasher,
	I: StorageIterator<H>,
{
	backend: Option<&'a I::Backend>,
	raw_iter: I,
	_phantom: PhantomData<H>,
}

impl<'a, H, I> Iterator for PairsIter<'a, H, I>
where
	H: Hasher,
	I: StorageIterator<H>,
{
	type Item = Result<(Vec<u8>, Vec<u8>), <I as StorageIterator<H>>::Error>;
	fn next(&mut self) -> Option<Self::Item> {
		self.raw_iter.next_pair(self.backend.as_ref()?)
	}
}

impl<'a, H, I> Default for PairsIter<'a, H, I>
where
	H: Hasher,
	I: StorageIterator<H> + Default,
{
	fn default() -> Self {
		Self {
			backend: Default::default(),
			raw_iter: Default::default(),
			_phantom: Default::default(),
		}
	}
}

impl<'a, H, I> PairsIter<'a, H, I>
where
	H: Hasher,
	I: StorageIterator<H> + Default,
{
	#[cfg(feature = "std")]
	pub(crate) fn was_complete(&self) -> bool {
		self.raw_iter.was_complete()
	}
}

/// An iterator over storage keys.
pub struct KeysIter<'a, H, I>
where
	H: Hasher,
	I: StorageIterator<H>,
{
	backend: Option<&'a I::Backend>,
	raw_iter: I,
	_phantom: PhantomData<H>,
}

impl<'a, H, I> Iterator for KeysIter<'a, H, I>
where
	H: Hasher,
	I: StorageIterator<H>,
{
	type Item = Result<Vec<u8>, <I as StorageIterator<H>>::Error>;
	fn next(&mut self) -> Option<Self::Item> {
		self.raw_iter.next_key(self.backend.as_ref()?)
	}
}

impl<'a, H, I> Default for KeysIter<'a, H, I>
where
	H: Hasher,
	I: StorageIterator<H> + Default,
{
	fn default() -> Self {
		Self {
			backend: Default::default(),
			raw_iter: Default::default(),
			_phantom: Default::default(),
		}
	}
}

/// Trie Backend Transaction type.
pub type TrieBackendTransaction<H> = PrefixedMemoryDB<H>;

/// Nomt Backend Transaction type.
#[cfg(feature = "std")]
pub struct NomtBackendTransaction {
	/// Nomt Overlay which contains the db transaction.
	pub transaction: NomtOverlay,
}

/// The transaction type used by [`Backend`].
///
/// This transaction contains all the changes that need to be applied to the backend to create the
/// state for a new block.
pub enum InnerBackendTransaction<H>
where
	H: Hasher,
{
	/// Trie specific backend transaction.
	Trie(TrieBackendTransaction<H>),
	/// Nomt specific backend transaction.
	#[cfg(feature = "std")]
	Nomt(NomtBackendTransaction),
}

/// A Backend transaction which contains the modifications needed to be performed
/// to the db to reflect runtime changes.
pub struct BackendTransaction<H: Hasher> {
	inner: Option<InnerBackendTransaction<H>>,
}

impl<H: Hasher> core::clone::Clone for BackendTransaction<H> {
	fn clone(&self) -> Self {
		Self { inner: self.inner.clone() }
	}
}

impl<H: Hasher> core::clone::Clone for InnerBackendTransaction<H> {
	fn clone(&self) -> Self {
		match self {
			InnerBackendTransaction::Trie(trie_transaction) =>
				InnerBackendTransaction::Trie(trie_transaction.clone()),
			#[cfg(feature = "std")]
			InnerBackendTransaction::Nomt(_) => unimplemented!(),
		}
	}
}

impl<H: Hasher> BackendTransaction<H> {
	/// Dummy backend transaction used as in memory nomt backend output.
	pub fn dummy() -> Self {
		Self { inner: None }
	}

	/// Creates a BackendTransaction from a TrieBackendTransaction.
	pub fn new_trie_transaction(trie_transaction: TrieBackendTransaction<H>) -> Self {
		Self { inner: Some(InnerBackendTransaction::Trie(trie_transaction)) }
	}

	/// Creates a BackendTransaction from a NomtBackendTransaction.
	#[cfg(feature = "std")]
	pub fn new_nomt_transaction(nomt_transaction: NomtBackendTransaction) -> Self {
		Self { inner: Some(InnerBackendTransaction::Nomt(nomt_transaction)) }
	}

	/// Regardless of the previous state of the BackendTransaction
	/// update it with the provided TrieBackendTransaction.
	pub fn set_trie_transaction(&mut self, trie_transaction: TrieBackendTransaction<H>) {
		self.inner = Some(InnerBackendTransaction::Trie(trie_transaction));
	}

	/// Regardless of the previous state of the BackendTransaction
	/// update it with the provided NomtBackendTransaction.
	#[cfg(feature = "std")]
	pub fn set_nomt_transaction(&mut self, nomt_transaction: NomtBackendTransaction) {
		self.inner = Some(InnerBackendTransaction::Nomt(nomt_transaction));
	}

	/// Extract the TrieBackendTransaction from the BackendTransaction.
	pub fn trie_transaction(self) -> TrieBackendTransaction<H> {
		match self.inner {
			Some(InnerBackendTransaction::Trie(trie_transaction)) => trie_transaction,
			_ => unreachable!(),
		}
	}

	/// Extract the TrieBackendTransaction from the NomtBackendTransaction.
	#[cfg(feature = "std")]
	pub fn nomt_transaction(self) -> NomtBackendTransaction {
		match self.inner {
			Some(InnerBackendTransaction::Nomt(nomt_transaction)) => nomt_transaction,
			_ => unreachable!(),
		}
	}

	/// Consolidate the current BackendTransaction initizling it with the provided one
	/// or join them, expecting them to be aligned.
	pub fn consolidate(&mut self, other: Self) {
		if let Some(InnerBackendTransaction::Trie(new_trie_transaction)) = other.inner {
			let trie_transaction = match core::mem::take(self).inner {
				Some(InnerBackendTransaction::Trie(mut trie_transaction)) => {
					trie_transaction.consolidate(new_trie_transaction);
					trie_transaction
				},
				// PANIC: Trie transaction cannot consolidate a Nomt transaction.
				Some(_) => unreachable!(),
				None => new_trie_transaction,
			};
			self.inner = Some(InnerBackendTransaction::Trie(trie_transaction));
		} else {
			#[cfg(feature = "std")]
			{
				assert!(self.inner.is_none());
				self.inner = Some(InnerBackendTransaction::Nomt(other.nomt_transaction()));
			}
			// NOTE: nothing happens if consolidate is called on top of
			// an in memory nomt backend.
			// #[cfg(not(feature = "std"))]
			// unreachable!()
		}
	}

	/// Extract the key from the transaction, treating it as Storage.
	pub fn get(&self, _key: &H::Out, _prefix: hash_db::Prefix) -> Option<sp_trie::DBValue> {
		panic!("BackendTransaction is not expected to used as Storage");
	}
}

impl<H: Hasher> Default for BackendTransaction<H> {
	fn default() -> Self {
		Self { inner: None }
	}
}

pub struct DummyProofSizeProvider;

impl sp_trie::ProofSizeProvider for DummyProofSizeProvider {
	fn estimate_encoded_size(&self) -> usize {
		0
	}
}

/// A state backend is used to read state data and can have changes committed
/// to it.
///
/// The clone operation (if implemented) should be cheap.
pub trait Backend<H: Hasher>: core::fmt::Debug {
	/// An error type when fetching data is not possible.
	type Error: super::Error;

	/// Type of trie backend storage.
	type TrieBackendStorage: TrieBackendStorage<H>;

	/// Type of the raw storage iterator.
	type RawIter: StorageIterator<H, Backend = Self, Error = Self::Error>;

	/// Get keyed storage or None if there is nothing associated.
	fn storage(&self, key: &[u8]) -> Result<Option<StorageValue>, Self::Error>;

	/// Get keyed storage value hash or None if there is nothing associated.
	fn storage_hash(&self, key: &[u8]) -> Result<Option<H::Out>, Self::Error>;

	/// Get the merkle value or None if there is nothing associated.
	fn closest_merkle_value(&self, key: &[u8]) -> Result<Option<MerkleValue<H::Out>>, Self::Error>;

	/// Get the child merkle value or None if there is nothing associated.
	fn child_closest_merkle_value(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<MerkleValue<H::Out>>, Self::Error>;

	/// Get child keyed child storage or None if there is nothing associated.
	fn child_storage(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<StorageValue>, Self::Error>;

	/// Get child keyed storage value hash or None if there is nothing associated.
	fn child_storage_hash(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<H::Out>, Self::Error>;

	/// true if a key exists in storage.
	fn exists_storage(&self, key: &[u8]) -> Result<bool, Self::Error> {
		Ok(self.storage_hash(key)?.is_some())
	}

	/// true if a key exists in child storage.
	fn exists_child_storage(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<bool, Self::Error> {
		Ok(self.child_storage_hash(child_info, key)?.is_some())
	}

	/// Return the next key in storage in lexicographic order or `None` if there is no value.
	fn next_storage_key(&self, key: &[u8]) -> Result<Option<StorageKey>, Self::Error>;

	/// Return the next key in child storage in lexicographic order or `None` if there is no value.
	fn next_child_storage_key(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<StorageKey>, Self::Error>;

	/// Calculate the storage root, with given delta over what is already stored in
	/// the backend, and produce a "transaction" that can be used to commit.
	/// Does not include child storage updates.
	fn storage_root<'a>(
		&self,
		delta: impl Iterator<Item = (&'a [u8], Option<&'a [u8]>)>,
		state_version: StateVersion,
	) -> (H::Out, BackendTransaction<H>)
	where
		H::Out: Ord;

	/// Calculate the child storage root, with given delta over what is already stored in
	/// the backend, and produce a "transaction" that can be used to commit. The second argument
	/// is true if child storage root equals default storage root.
	fn child_storage_root<'a>(
		&self,
		child_info: &ChildInfo,
		delta: impl Iterator<Item = (&'a [u8], Option<&'a [u8]>)>,
		state_version: StateVersion,
	) -> Option<(H::Out, bool, BackendTransaction<H>)>
	where
		H::Out: Ord;

	/// Returns a lifetimeless raw storage iterator.
	fn raw_iter(&self, args: IterArgs) -> Result<Self::RawIter, Self::Error>;

	/// Get an iterator over key/value pairs.
	fn pairs<'a>(&'a self, args: IterArgs) -> Result<PairsIter<'a, H, Self::RawIter>, Self::Error> {
		Ok(PairsIter {
			backend: Some(self),
			raw_iter: self.raw_iter(args)?,
			_phantom: Default::default(),
		})
	}

	/// Get an iterator over keys.
	fn keys<'a>(&'a self, args: IterArgs) -> Result<KeysIter<'a, H, Self::RawIter>, Self::Error> {
		Ok(KeysIter {
			backend: Some(self),
			raw_iter: self.raw_iter(args)?,
			_phantom: Default::default(),
		})
	}

	/// Calculate the storage root, with given delta over what is already stored
	/// in the backend, and produce a "transaction" that can be used to commit.
	/// Does include child storage updates.
	fn full_storage_root<'a>(
		&self,
		delta: impl Iterator<Item = (&'a [u8], Option<&'a [u8]>)>,
		child_deltas: impl Iterator<
			Item = (&'a ChildInfo, impl Iterator<Item = (&'a [u8], Option<&'a [u8]>)>),
		>,
		state_version: StateVersion,
	) -> (H::Out, BackendTransaction<H>)
	where
		H::Out: Ord + Encode,
	{
		let mut txs = BackendTransaction::default();
		let mut child_roots: Vec<_> = Default::default();
		// child first
		for (child_info, child_delta) in child_deltas {
			let Some((child_root, empty, child_txs)) =
				self.child_storage_root(child_info, child_delta, state_version)
			else {
				continue
			};
			let prefixed_storage_key = child_info.prefixed_storage_key();
			txs.consolidate(child_txs);
			if empty {
				child_roots.push((prefixed_storage_key.into_inner(), None));
			} else {
				child_roots.push((prefixed_storage_key.into_inner(), Some(child_root.encode())));
			}
		}
		let (root, parent_txs) = self.storage_root(
			delta
				.map(|(k, v)| (k, v.as_ref().map(|v| &v[..])))
				.chain(child_roots.iter().map(|(k, v)| (&k[..], v.as_ref().map(|v| &v[..])))),
			state_version,
		);
		txs.consolidate(parent_txs);

		(root, txs)
	}

	/// Register stats from overlay of state machine.
	///
	/// By default nothing is registered.
	fn register_overlay_stats(&self, _stats: &crate::stats::StateMachineStats);

	/// Query backend usage statistics (i/o, memory)
	///
	/// Not all implementations are expected to be able to do this. In the
	/// case when they don't, empty statistics is returned.
	fn usage_info(&self) -> UsageInfo;

	/// Wipe the state database.
	fn wipe(&self) -> Result<(), Self::Error> {
		unimplemented!()
	}

	/// Commit given transaction to storage.
	fn commit(
		&self,
		_: H::Out,
		_: BackendTransaction<H>,
		_: StorageCollection,
		_: ChildStorageCollection,
	) -> Result<(), Self::Error> {
		unimplemented!()
	}

	/// Get the read/write count of the db
	fn read_write_count(&self) -> (u32, u32, u32, u32) {
		unimplemented!()
	}

	/// Get the read/write count of the db
	fn reset_read_write_count(&self) {
		unimplemented!()
	}

	/// Get the whitelist for tracking db reads/writes
	fn get_whitelist(&self) -> Vec<TrackedStorageKey> {
		Default::default()
	}

	/// Update the whitelist for tracking db reads/writes
	fn set_whitelist(&self, _: Vec<TrackedStorageKey>) {}

	/// Estimate proof size
	fn proof_size(&self) -> Option<u32> {
		unimplemented!()
	}

	/// Extend storage info for benchmarking db
	fn get_read_and_written_keys(&self) -> Vec<(Vec<u8>, u32, u32, bool)> {
		unimplemented!()
	}
}

/// Something that can be converted into a [`TrieBackend`].
#[cfg(feature = "std")]
pub trait AsTrieBackend<H: Hasher, C = sp_trie::cache::LocalTrieCache<H>> {
	/// Type of trie backend storage.
	type TrieBackendStorage: TrieBackendStorage<H>;

	/// Return the type as [`TrieBackend`].
	fn as_trie_backend(&self) -> &TrieBackend<Self::TrieBackendStorage, H, C>;
}

#[cfg(feature = "std")]
pub trait AsStateBackend<H: Hasher> {
	type TrieBackendStorage: TrieBackendStorage<H>;

	fn as_state_backend(&self) -> &crate::state_backend::StateBackend<Self::TrieBackendStorage, H>;
}

/// Wrapper to create a [`RuntimeCode`] from a type that implements [`Backend`].
#[cfg(feature = "std")]
pub struct BackendRuntimeCode<'a, B, H> {
	backend: &'a B,
	_marker: PhantomData<H>,
}

#[cfg(feature = "std")]
impl<'a, B: Backend<H>, H: Hasher> sp_core::traits::FetchRuntimeCode
	for BackendRuntimeCode<'a, B, H>
{
	fn fetch_runtime_code(&self) -> Option<std::borrow::Cow<'_, [u8]>> {
		self.backend
			.storage(sp_core::storage::well_known_keys::CODE)
			.ok()
			.flatten()
			.map(Into::into)
	}
}

#[cfg(feature = "std")]
impl<'a, B: Backend<H>, H: Hasher> BackendRuntimeCode<'a, B, H>
where
	H::Out: Encode,
{
	/// Create a new instance.
	pub fn new(backend: &'a B) -> Self {
		Self { backend, _marker: PhantomData }
	}

	/// Return the [`RuntimeCode`] build from the wrapped `backend`.
	pub fn runtime_code(&self) -> Result<RuntimeCode<'_>, &'static str> {
		let hash = self
			.backend
			.storage_hash(sp_core::storage::well_known_keys::CODE)
			.ok()
			.flatten()
			.ok_or("`:code` hash not found")?
			.encode();
		let heap_pages = self
			.backend
			.storage(sp_core::storage::well_known_keys::HEAP_PAGES)
			.ok()
			.flatten()
			.and_then(|d| codec::Decode::decode(&mut &d[..]).ok());

		Ok(RuntimeCode { code_fetcher: self, hash, heap_pages })
	}
}
