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

//! Storage map type. Implements StorageMap, StorageIterableMap, StoragePrefixedMap traits and their
//! methods directly.

use crate::{
	storage::{
		self, storage_prefix,
		types::{OptionQuery, QueryKindTrait, StorageEntryMetadataBuilder},
		unhashed, KeyLenOf, KeyPrefixIterator, PrefixIterator, StorageAppend, StorageDecodeLength,
		StoragePrefixedMap, StorageTryAppend, TryAppendMap,
	},
	traits::{Get, GetDefault, StorageInfo, StorageInstance},
	Never, StorageHasher, Twox128,
};
use alloc::{vec, vec::Vec};
use codec::{Decode, Encode, EncodeLike, FullCodec, MaxEncodedLen};
use frame_support::storage::StorageDecodeNonDedupLength;
use sp_arithmetic::traits::SaturatedConversion;
use sp_metadata_ir::{StorageEntryMetadataIR, StorageEntryTypeIR};

/// A type representing a *map* in storage. A *storage map* is a mapping of keys to values of a
/// given type stored on-chain.
///
/// For general information regarding the `#[pallet::storage]` attribute, refer to
/// [`crate::pallet_macros::storage`].
///
/// # Example
///
/// ```
/// #[frame_support::pallet]
/// mod pallet {
///     # use frame_support::pallet_prelude::*;
///     # #[pallet::config]
///     # pub trait Config: frame_system::Config {}
///     # #[pallet::pallet]
///     # pub struct Pallet<T>(_);
/// 	/// A kitchen-sink StorageMap, with all possible additional attributes.
///     #[pallet::storage]
/// 	#[pallet::getter(fn foo)]
/// 	#[pallet::storage_prefix = "OtherFoo"]
/// 	#[pallet::unbounded]
///     pub type Foo<T> = StorageMap<
/// 		_,
/// 		Blake2_128Concat,
/// 		u32,
/// 		u32,
/// 		ValueQuery
/// 	>;
///
/// 	/// Alternative named syntax.
///     #[pallet::storage]
///     pub type Bar<T> = StorageMap<
/// 		Hasher = Blake2_128Concat,
/// 		Key = u32,
/// 		Value = u32,
/// 		QueryKind = ValueQuery
/// 	>;
/// }
/// ```
pub struct StorageMap<
	Prefix,
	Hasher,
	Key,
	Value,
	QueryKind = OptionQuery,
	OnEmpty = GetDefault,
	MaxValues = GetDefault,
>(core::marker::PhantomData<(Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues)>);

impl<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues> Get<u32>
	for KeyLenOf<StorageMap<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues>>
where
	Prefix: StorageInstance,
	Hasher: crate::hash::StorageHasher,
	Key: FullCodec + MaxEncodedLen,
{
	fn get() -> u32 {
		// The `max_len` of the key hash plus the pallet prefix and storage prefix (which both are
		// hashed with `Twox128`).
		let z = Hasher::max_len::<Key>() + Twox128::max_len::<()>() * 2;
		z as u32
	}
}

impl<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues>
	StorageMap<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues>
where
	Prefix: StorageInstance,
	Hasher: crate::hash::StorageHasher,
	Key: FullCodec,
	Value: FullCodec,
	QueryKind: QueryKindTrait<Value, OnEmpty>,
	OnEmpty: Get<QueryKind::Query> + 'static,
	MaxValues: Get<Option<u32>>,
{
	/// Pallet prefix. Used for generating final key.
	pub fn pallet_prefix() -> &'static [u8] {
		Prefix::pallet_prefix().as_bytes()
	}
	/// Storage prefix. Used for generating final key.
	pub fn storage_prefix() -> &'static [u8] {
		Prefix::STORAGE_PREFIX.as_bytes()
	}
	/// The full prefix; just the hash of `pallet_prefix` concatenated to the hash of
	/// `storage_prefix`.
	pub fn prefix_hash() -> [u8; 32] {
		Prefix::prefix_hash()
	}
	/// Convert an optional value retrieved from storage to the type queried.
	pub fn from_optional_value_to_query(v: Option<Value>) -> QueryKind::Query {
		QueryKind::from_optional_value_to_query(v)
	}
	/// Convert a query to an optional value into storage.
	pub fn from_query_to_optional_value(v: QueryKind::Query) -> Option<Value> {
		QueryKind::from_query_to_optional_value(v)
	}
	/// Generate the full key used in top storage.
	pub fn storage_map_final_key<KeyArg>(key: KeyArg) -> Vec<u8>
	where
		KeyArg: EncodeLike<Key>,
	{
		let storage_prefix = storage_prefix(Self::pallet_prefix(), Self::storage_prefix());
		let key_hashed = key.using_encoded(Hasher::hash);

		let mut final_key = Vec::with_capacity(storage_prefix.len() + key_hashed.as_ref().len());

		final_key.extend_from_slice(&storage_prefix);
		final_key.extend_from_slice(key_hashed.as_ref());

		final_key
	}
}

impl<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues> StoragePrefixedMap<Value>
	for StorageMap<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues>
where
	Prefix: StorageInstance,
	Hasher: crate::hash::StorageHasher,
	Key: FullCodec,
	Value: FullCodec,
	QueryKind: QueryKindTrait<Value, OnEmpty>,
	OnEmpty: Get<QueryKind::Query> + 'static,
	MaxValues: Get<Option<u32>>,
{
	fn pallet_prefix() -> &'static [u8] {
		Self::pallet_prefix()
	}
	fn storage_prefix() -> &'static [u8] {
		Self::storage_prefix()
	}
}

impl<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues>
	crate::storage::StorageMap<Key, Value>
	for StorageMap<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues>
where
	Prefix: StorageInstance,
	Hasher: crate::hash::StorageHasher,
	Key: FullCodec,
	Value: FullCodec,
	QueryKind: QueryKindTrait<Value, OnEmpty>,
	OnEmpty: Get<QueryKind::Query> + 'static,
	MaxValues: Get<Option<u32>>,
{
	type Query = QueryKind::Query;

	fn hashed_key_for<KeyArg: EncodeLike<Key>>(key: KeyArg) -> Vec<u8> {
		Self::storage_map_final_key(key)
	}

	fn contains_key<KeyArg: EncodeLike<Key>>(key: KeyArg) -> bool {
		unhashed::exists(Self::storage_map_final_key(key).as_ref())
	}

	fn get<KeyArg: EncodeLike<Key>>(key: KeyArg) -> Self::Query {
		Self::from_optional_value_to_query(unhashed::get(&Self::storage_map_final_key(key)))
	}

	fn set<KeyArg: EncodeLike<Key>>(key: KeyArg, query: Self::Query) {
		match Self::from_query_to_optional_value(query) {
			Some(v) => Self::insert(key, v),
			None => Self::remove(key),
		}
	}

	fn try_get<KeyArg: EncodeLike<Key>>(key: KeyArg) -> Result<Value, ()> {
		unhashed::get(Self::storage_map_final_key(key).as_ref()).ok_or(())
	}

	fn swap<KeyArg1: EncodeLike<Key>, KeyArg2: EncodeLike<Key>>(key1: KeyArg1, key2: KeyArg2) {
		let k1 = Self::storage_map_final_key(key1);
		let k2 = Self::storage_map_final_key(key2);

		let v1 = unhashed::get_raw(k1.as_ref());
		if let Some(val) = unhashed::get_raw(k2.as_ref()) {
			unhashed::put_raw(k1.as_ref(), &val);
		} else {
			unhashed::kill(k1.as_ref())
		}
		if let Some(val) = v1 {
			unhashed::put_raw(k2.as_ref(), &val);
		} else {
			unhashed::kill(k2.as_ref())
		}
	}

	fn insert<KeyArg: EncodeLike<Key>, ValArg: EncodeLike<Value>>(key: KeyArg, val: ValArg) {
		unhashed::put(Self::storage_map_final_key(key).as_ref(), &val)
	}

	fn remove<KeyArg: EncodeLike<Key>>(key: KeyArg) {
		unhashed::kill(Self::storage_map_final_key(key).as_ref())
	}

	fn mutate<KeyArg: EncodeLike<Key>, R, F: FnOnce(&mut Self::Query) -> R>(
		key: KeyArg,
		f: F,
	) -> R {
		Self::try_mutate(key, |v| Ok::<R, Never>(f(v)))
			.expect("`Never` can not be constructed; qed")
	}

	fn try_mutate<KeyArg: EncodeLike<Key>, R, E, F: FnOnce(&mut Self::Query) -> Result<R, E>>(
		key: KeyArg,
		f: F,
	) -> Result<R, E> {
		let final_key = Self::storage_map_final_key(key);
		let mut val = Self::from_optional_value_to_query(unhashed::get(final_key.as_ref()));

		let ret = f(&mut val);
		if ret.is_ok() {
			match Self::from_query_to_optional_value(val) {
				Some(ref val) => unhashed::put(final_key.as_ref(), &val),
				None => unhashed::kill(final_key.as_ref()),
			}
		}
		ret
	}

	fn mutate_exists<KeyArg: EncodeLike<Key>, R, F: FnOnce(&mut Option<Value>) -> R>(
		key: KeyArg,
		f: F,
	) -> R {
		Self::try_mutate_exists(key, |v| Ok::<R, Never>(f(v)))
			.expect("`Never` can not be constructed; qed")
	}

	fn try_mutate_exists<
		KeyArg: EncodeLike<Key>,
		R,
		E,
		F: FnOnce(&mut Option<Value>) -> Result<R, E>,
	>(
		key: KeyArg,
		f: F,
	) -> Result<R, E> {
		let final_key = Self::storage_map_final_key(key);
		let mut val = unhashed::get(final_key.as_ref());

		let ret = f(&mut val);
		if ret.is_ok() {
			match val {
				Some(ref val) => unhashed::put(final_key.as_ref(), &val),
				None => unhashed::kill(final_key.as_ref()),
			}
		}
		ret
	}

	fn take<KeyArg: EncodeLike<Key>>(key: KeyArg) -> Self::Query {
		let key = Self::storage_map_final_key(key);
		let value = unhashed::take(key.as_ref());
		Self::from_optional_value_to_query(value)
	}

	fn append<Item, EncodeLikeItem, EncodeLikeKey>(key: EncodeLikeKey, item: EncodeLikeItem)
	where
		EncodeLikeKey: EncodeLike<Key>,
		Item: Encode,
		EncodeLikeItem: EncodeLike<Item>,
		Value: StorageAppend<Item>,
	{
		let key = Self::storage_map_final_key(key);
		sp_io::storage::append(&key, item.encode());
	}

	fn migrate_key<OldHasher: StorageHasher, KeyArg: EncodeLike<Key>>(
		key: KeyArg,
	) -> Option<Value> {
		let old_key = {
			let storage_prefix = storage_prefix(Self::pallet_prefix(), Self::storage_prefix());
			let key_hashed = key.using_encoded(OldHasher::hash);

			let mut final_key =
				Vec::with_capacity(storage_prefix.len() + key_hashed.as_ref().len());

			final_key.extend_from_slice(&storage_prefix);
			final_key.extend_from_slice(key_hashed.as_ref());

			final_key
		};
		unhashed::take(old_key.as_ref()).map(|value| {
			unhashed::put(Self::storage_map_final_key(key).as_ref(), &value);
			value
		})
	}
}

impl<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues>
	storage::IterableStorageMap<Key, Value>
	for StorageMap<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues>
where
	Prefix: StorageInstance,
	Hasher: crate::ReversibleStorageHasher,
	Key: FullCodec,
	Value: FullCodec,
	QueryKind: QueryKindTrait<Value, OnEmpty>,
	OnEmpty: Get<QueryKind::Query> + 'static,
	MaxValues: Get<Option<u32>>,
{
	type Iterator = PrefixIterator<(Key, Value)>;
	type KeyIterator = KeyPrefixIterator<Key>;

	/// Enumerate all elements in the map.
	fn iter() -> Self::Iterator {
		let prefix = Self::prefix_hash().to_vec();
		PrefixIterator {
			prefix: prefix.clone(),
			previous_key: prefix,
			drain: false,
			closure: |raw_key_without_prefix, mut raw_value| {
				let mut key_material = Hasher::reverse(raw_key_without_prefix);
				Ok((Key::decode(&mut key_material)?, Value::decode(&mut raw_value)?))
			},
			phantom: Default::default(),
		}
	}

	/// Enumerate all elements in the map after a given key.
	fn iter_from(starting_raw_key: Vec<u8>) -> Self::Iterator {
		let mut iter = Self::iter();
		iter.set_last_raw_key(starting_raw_key);
		iter
	}

	/// Enumerate all keys in the map.
	fn iter_keys() -> Self::KeyIterator {
		let prefix = Self::prefix_hash().to_vec();
		KeyPrefixIterator {
			prefix: prefix.clone(),
			previous_key: prefix,
			drain: false,
			closure: |raw_key_without_prefix| {
				let mut key_material = Hasher::reverse(raw_key_without_prefix);
				Key::decode(&mut key_material)
			},
		}
	}

	/// Enumerate all keys in the map after a given key.
	fn iter_keys_from(starting_raw_key: Vec<u8>) -> Self::KeyIterator {
		let mut iter = Self::iter_keys();
		iter.set_last_raw_key(starting_raw_key);
		iter
	}

	/// Enumerate all elements in the map.
	fn drain() -> Self::Iterator {
		let mut iterator = Self::iter();
		iterator.drain = true;
		iterator
	}

	fn translate<O: Decode, F: FnMut(Key, O) -> Option<Value>>(mut f: F) {
		let mut previous_key = None;
		loop {
			previous_key = Self::translate_next(previous_key, &mut f);
			if previous_key.is_none() {
				break;
			}
		}
	}

	fn translate_next<O: Decode, F: FnMut(Key, O) -> Option<Value>>(
		previous_key: Option<Vec<u8>>,
		mut f: F,
	) -> Option<Vec<u8>> {
		let prefix = Self::prefix_hash().to_vec();
		let previous_key = previous_key.unwrap_or_else(|| prefix.clone());

		let current_key =
			sp_io::storage::next_key(&previous_key).filter(|n| n.starts_with(&prefix))?;

		let value = match unhashed::get::<O>(&current_key) {
			Some(value) => value,
			None => {
				crate::defensive!("Invalid translate: fail to decode old value");
				return Some(current_key);
			},
		};

		let mut key_material = Hasher::reverse(&current_key[prefix.len()..]);
		let key = match Key::decode(&mut key_material) {
			Ok(key) => key,
			Err(_) => {
				crate::defensive!("Invalid translate: fail to decode key");
				return Some(current_key);
			},
		};

		match f(key, value) {
			Some(new) => unhashed::put::<Value>(&current_key, &new),
			None => unhashed::kill(&current_key),
		}

		Some(current_key)
	}
}

impl<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues>
	StorageMap<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues>
where
	Prefix: StorageInstance,
	Hasher: crate::hash::StorageHasher,
	Key: FullCodec,
	Value: FullCodec,
	QueryKind: QueryKindTrait<Value, OnEmpty>,
	OnEmpty: Get<QueryKind::Query> + 'static,
	MaxValues: Get<Option<u32>>,
{
	/// Get the storage key used to fetch a value corresponding to a specific key.
	pub fn hashed_key_for<KeyArg: EncodeLike<Key>>(key: KeyArg) -> Vec<u8> {
		<Self as crate::storage::StorageMap<Key, Value>>::hashed_key_for(key)
	}

	/// Does the value (explicitly) exist in storage?
	pub fn contains_key<KeyArg: EncodeLike<Key>>(key: KeyArg) -> bool {
		<Self as crate::storage::StorageMap<Key, Value>>::contains_key(key)
	}

	/// Load the value associated with the given key from the map.
	pub fn get<KeyArg: EncodeLike<Key>>(key: KeyArg) -> QueryKind::Query {
		<Self as crate::storage::StorageMap<Key, Value>>::get(key)
	}

	/// Try to get the value for the given key from the map.
	///
	/// Returns `Ok` if it exists, `Err` if not.
	pub fn try_get<KeyArg: EncodeLike<Key>>(key: KeyArg) -> Result<Value, ()> {
		<Self as crate::storage::StorageMap<Key, Value>>::try_get(key)
	}

	/// Swap the values of two keys.
	pub fn swap<KeyArg1: EncodeLike<Key>, KeyArg2: EncodeLike<Key>>(key1: KeyArg1, key2: KeyArg2) {
		<Self as crate::storage::StorageMap<Key, Value>>::swap(key1, key2)
	}

	/// Store or remove the value to be associated with `key` so that `get` returns the `query`.
	pub fn set<KeyArg: EncodeLike<Key>>(key: KeyArg, q: QueryKind::Query) {
		<Self as crate::storage::StorageMap<Key, Value>>::set(key, q)
	}

	/// Store a value to be associated with the given key from the map.
	pub fn insert<KeyArg: EncodeLike<Key>, ValArg: EncodeLike<Value>>(key: KeyArg, val: ValArg) {
		<Self as crate::storage::StorageMap<Key, Value>>::insert(key, val)
	}

	/// Remove the value under a key.
	pub fn remove<KeyArg: EncodeLike<Key>>(key: KeyArg) {
		<Self as crate::storage::StorageMap<Key, Value>>::remove(key)
	}

	/// Mutate the value under a key.
	pub fn mutate<KeyArg: EncodeLike<Key>, R, F: FnOnce(&mut QueryKind::Query) -> R>(
		key: KeyArg,
		f: F,
	) -> R {
		<Self as crate::storage::StorageMap<Key, Value>>::mutate(key, f)
	}

	/// Mutate the item, only if an `Ok` value is returned.
	pub fn try_mutate<KeyArg, R, E, F>(key: KeyArg, f: F) -> Result<R, E>
	where
		KeyArg: EncodeLike<Key>,
		F: FnOnce(&mut QueryKind::Query) -> Result<R, E>,
	{
		<Self as crate::storage::StorageMap<Key, Value>>::try_mutate(key, f)
	}

	/// Mutate the value under a key iff it exists. Do nothing and return the default value if not.
	pub fn mutate_extant<KeyArg: EncodeLike<Key>, R: Default, F: FnOnce(&mut Value) -> R>(
		key: KeyArg,
		f: F,
	) -> R {
		<Self as crate::storage::StorageMap<Key, Value>>::mutate_extant(key, f)
	}

	/// Mutate the value under a key. Deletes the item if mutated to a `None`.
	pub fn mutate_exists<KeyArg: EncodeLike<Key>, R, F: FnOnce(&mut Option<Value>) -> R>(
		key: KeyArg,
		f: F,
	) -> R {
		<Self as crate::storage::StorageMap<Key, Value>>::mutate_exists(key, f)
	}

	/// Mutate the item, only if an `Ok` value is returned. Deletes the item if mutated to a `None`.
	/// `f` will always be called with an option representing if the storage item exists (`Some<V>`)
	/// or if the storage item does not exist (`None`), independent of the `QueryType`.
	pub fn try_mutate_exists<KeyArg, R, E, F>(key: KeyArg, f: F) -> Result<R, E>
	where
		KeyArg: EncodeLike<Key>,
		F: FnOnce(&mut Option<Value>) -> Result<R, E>,
	{
		<Self as crate::storage::StorageMap<Key, Value>>::try_mutate_exists(key, f)
	}

	/// Take the value under a key.
	pub fn take<KeyArg: EncodeLike<Key>>(key: KeyArg) -> QueryKind::Query {
		<Self as crate::storage::StorageMap<Key, Value>>::take(key)
	}

	/// Append the given items to the value in the storage.
	///
	/// `Value` is required to implement `codec::EncodeAppend`.
	///
	/// # Warning
	///
	/// If the storage item is not encoded properly, the storage will be overwritten
	/// and set to `[item]`. Any default value set for the storage item will be ignored
	/// on overwrite.
	pub fn append<Item, EncodeLikeItem, EncodeLikeKey>(key: EncodeLikeKey, item: EncodeLikeItem)
	where
		EncodeLikeKey: EncodeLike<Key>,
		Item: Encode,
		EncodeLikeItem: EncodeLike<Item>,
		Value: StorageAppend<Item>,
	{
		<Self as crate::storage::StorageMap<Key, Value>>::append(key, item)
	}

	/// Read the length of the storage value without decoding the entire value under the
	/// given `key`.
	///
	/// `Value` is required to implement [`StorageDecodeLength`].
	///
	/// If the value does not exists or it fails to decode the length, `None` is returned.
	/// Otherwise `Some(len)` is returned.
	///
	/// # Warning
	///
	/// `None` does not mean that `get()` does not return a value. The default value is completely
	/// ignored by this function.
	pub fn decode_len<KeyArg: EncodeLike<Key>>(key: KeyArg) -> Option<usize>
	where
		Value: StorageDecodeLength,
	{
		<Self as crate::storage::StorageMap<Key, Value>>::decode_len(key)
	}

	/// Read the length of the storage value without decoding the entire value.
	///
	/// `Value` is required to implement [`StorageDecodeNonDedupLength`].
	///
	/// If the value does not exists or it fails to decode the length, `None` is returned.
	/// Otherwise `Some(len)` is returned.
	///
	/// # Warning
	///
	///  - `None` does not mean that `get()` does not return a value. The default value is
	///    completely
	/// ignored by this function.
	///
	/// - The value returned is the non-deduplicated length of the underlying Vector in storage.This
	/// means that any duplicate items are included.
	pub fn decode_non_dedup_len<KeyArg: EncodeLike<Key>>(key: KeyArg) -> Option<usize>
	where
		Value: StorageDecodeNonDedupLength,
	{
		<Self as crate::storage::StorageMap<Key, Value>>::decode_non_dedup_len(key)
	}

	/// Migrate an item with the given `key` from a defunct `OldHasher` to the current hasher.
	///
	/// If the key doesn't exist, then it's a no-op. If it does, then it returns its value.
	pub fn migrate_key<OldHasher: crate::hash::StorageHasher, KeyArg: EncodeLike<Key>>(
		key: KeyArg,
	) -> Option<Value> {
		<Self as crate::storage::StorageMap<Key, Value>>::migrate_key::<OldHasher, _>(key)
	}

	/// Remove all values of the storage in the overlay and up to `limit` in the backend.
	///
	/// All values in the client overlay will be deleted, if there is some `limit` then up to
	/// `limit` values are deleted from the client backend, if `limit` is none then all values in
	/// the client backend are deleted.
	///
	/// # Note
	///
	/// Calling this multiple times per block with a `limit` set leads always to the same keys being
	/// removed and the same result being returned. This happens because the keys to delete in the
	/// overlay are not taken into account when deleting keys in the backend.
	#[deprecated = "Use `clear` instead"]
	pub fn remove_all(limit: Option<u32>) -> sp_io::KillStorageResult {
		#[allow(deprecated)]
		<Self as crate::storage::StoragePrefixedMap<Value>>::remove_all(limit)
	}

	/// Attempt to remove all items from the map.
	///
	/// Returns [`MultiRemovalResults`](sp_io::MultiRemovalResults) to inform about the result. Once
	/// the resultant `maybe_cursor` field is `None`, then no further items remain to be deleted.
	///
	/// NOTE: After the initial call for any given map, it is important that no further items
	/// are inserted into the map. If so, then the map may not be empty when the resultant
	/// `maybe_cursor` is `None`.
	///
	/// # Limit
	///
	/// A `limit` must always be provided through in order to cap the maximum
	/// amount of deletions done in a single call. This is one fewer than the
	/// maximum number of backend iterations which may be done by this operation and as such
	/// represents the maximum number of backend deletions which may happen. A `limit` of zero
	/// implies that no keys will be deleted, though there may be a single iteration done.
	///
	/// # Cursor
	///
	/// A *cursor* may be passed in to this operation with `maybe_cursor`. `None` should only be
	/// passed once (in the initial call) for any given storage map. Subsequent calls
	/// operating on the same map should always pass `Some`, and this should be equal to the
	/// previous call result's `maybe_cursor` field.
	pub fn clear(limit: u32, maybe_cursor: Option<&[u8]>) -> sp_io::MultiRemovalResults {
		<Self as crate::storage::StoragePrefixedMap<Value>>::clear(limit, maybe_cursor)
	}

	/// Iter over all value of the storage.
	///
	/// NOTE: If a value failed to decode because storage is corrupted then it is skipped.
	pub fn iter_values() -> crate::storage::PrefixIterator<Value> {
		<Self as crate::storage::StoragePrefixedMap<Value>>::iter_values()
	}

	/// Translate the values of all elements by a function `f`, in the map in no particular order.
	///
	/// By returning `None` from `f` for an element, you'll remove it from the map.
	///
	/// NOTE: If a value fail to decode because storage is corrupted then it is skipped.
	///
	/// # Warning
	///
	/// This function must be used with care, before being updated the storage still contains the
	/// old type, thus other calls (such as `get`) will fail at decoding it.
	///
	/// # Usage
	///
	/// This would typically be called inside the module implementation of on_runtime_upgrade.
	pub fn translate_values<OldValue: Decode, F: FnMut(OldValue) -> Option<Value>>(f: F) {
		<Self as crate::storage::StoragePrefixedMap<Value>>::translate_values(f)
	}

	/// Try and append the given item to the value in the storage.
	///
	/// Is only available if `Value` of the storage implements [`StorageTryAppend`].
	pub fn try_append<KArg, Item, EncodeLikeItem>(key: KArg, item: EncodeLikeItem) -> Result<(), ()>
	where
		KArg: EncodeLike<Key> + Clone,
		Item: Encode,
		EncodeLikeItem: EncodeLike<Item>,
		Value: StorageTryAppend<Item>,
	{
		<Self as crate::storage::TryAppendMap<Key, Value, Item>>::try_append(key, item)
	}
}

impl<Prefix, Hasher, Key, Value, I, QueryKind, OnEmpty, MaxValues> TryAppendMap<Key, Value, I>
	for StorageMap<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues>
where
	Prefix: StorageInstance,
	Hasher: crate::hash::StorageHasher,
	Key: FullCodec,
	Value: FullCodec + StorageTryAppend<I>,
	QueryKind: QueryKindTrait<Value, OnEmpty>,
	OnEmpty: Get<QueryKind::Query> + 'static,
	MaxValues: Get<Option<u32>>,
	I: Encode,
{
	fn try_append<LikeK: EncodeLike<Key> + Clone, LikeI: EncodeLike<I>>(
		key: LikeK,
		item: LikeI,
	) -> Result<(), ()> {
		let bound = Value::bound();
		let current = Self::decode_len(key.clone()).unwrap_or_default();
		if current < bound {
			let key = Self::storage_map_final_key(key);
			sp_io::storage::append(&key, item.encode());
			Ok(())
		} else {
			Err(())
		}
	}
}

impl<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues>
	StorageMap<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues>
where
	Prefix: StorageInstance,
	Hasher: crate::hash::StorageHasher + crate::ReversibleStorageHasher,
	Key: FullCodec,
	Value: FullCodec,
	QueryKind: QueryKindTrait<Value, OnEmpty>,
	OnEmpty: Get<QueryKind::Query> + 'static,
	MaxValues: Get<Option<u32>>,
{
	/// Enumerate all elements in the map in no particular order.
	///
	/// If you alter the map while doing this, you'll get undefined results.
	pub fn iter() -> crate::storage::PrefixIterator<(Key, Value)> {
		<Self as crate::storage::IterableStorageMap<Key, Value>>::iter()
	}

	/// Enumerate all elements in the map after a specified `starting_raw_key` in no
	/// particular order.
	///
	/// If you alter the map while doing this, you'll get undefined results.
	pub fn iter_from(starting_raw_key: Vec<u8>) -> crate::storage::PrefixIterator<(Key, Value)> {
		<Self as crate::storage::IterableStorageMap<Key, Value>>::iter_from(starting_raw_key)
	}

	/// Enumerate all elements in the map after a specified `starting_key` in no
	/// particular order.
	///
	/// If you alter the map while doing this, you'll get undefined results.
	pub fn iter_from_key(
		starting_key: impl EncodeLike<Key>,
	) -> crate::storage::PrefixIterator<(Key, Value)> {
		Self::iter_from(Self::hashed_key_for(starting_key))
	}

	/// Enumerate all keys in the map in no particular order.
	///
	/// If you alter the map while doing this, you'll get undefined results.
	pub fn iter_keys() -> crate::storage::KeyPrefixIterator<Key> {
		<Self as crate::storage::IterableStorageMap<Key, Value>>::iter_keys()
	}

	/// Enumerate all keys in the map after a specified `starting_raw_key` in no particular
	/// order.
	///
	/// If you alter the map while doing this, you'll get undefined results.
	pub fn iter_keys_from(starting_raw_key: Vec<u8>) -> crate::storage::KeyPrefixIterator<Key> {
		<Self as crate::storage::IterableStorageMap<Key, Value>>::iter_keys_from(starting_raw_key)
	}

	/// Enumerate all keys in the map after a specified `starting_key` in no particular
	/// order.
	///
	/// If you alter the map while doing this, you'll get undefined results.
	pub fn iter_keys_from_key(
		starting_key: impl EncodeLike<Key>,
	) -> crate::storage::KeyPrefixIterator<Key> {
		Self::iter_keys_from(Self::hashed_key_for(starting_key))
	}

	/// Remove all elements from the map and iterate through them in no particular order.
	///
	/// If you add elements to the map while doing this, you'll get undefined results.
	pub fn drain() -> crate::storage::PrefixIterator<(Key, Value)> {
		<Self as crate::storage::IterableStorageMap<Key, Value>>::drain()
	}

	/// Translate the values of all elements by a function `f`, in the map in no particular order.
	///
	/// By returning `None` from `f` for an element, you'll remove it from the map.
	///
	/// NOTE: If a value fails to decode because storage is corrupted, then it will log an error and
	/// be skipped in production, or panic in development.
	pub fn translate<O: Decode, F: FnMut(Key, O) -> Option<Value>>(f: F) {
		<Self as crate::storage::IterableStorageMap<Key, Value>>::translate(f)
	}
}

impl<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues> StorageEntryMetadataBuilder
	for StorageMap<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues>
where
	Prefix: StorageInstance,
	Hasher: crate::hash::StorageHasher,
	Key: FullCodec + scale_info::StaticTypeInfo,
	Value: FullCodec + scale_info::StaticTypeInfo,
	QueryKind: QueryKindTrait<Value, OnEmpty>,
	OnEmpty: Get<QueryKind::Query> + 'static,
	MaxValues: Get<Option<u32>>,
{
	fn build_metadata(docs: Vec<&'static str>, entries: &mut Vec<StorageEntryMetadataIR>) {
		let docs = if cfg!(feature = "no-metadata-docs") { vec![] } else { docs };

		let entry = StorageEntryMetadataIR {
			name: Prefix::STORAGE_PREFIX,
			modifier: QueryKind::METADATA,
			ty: StorageEntryTypeIR::Map {
				hashers: vec![Hasher::METADATA],
				key: scale_info::meta_type::<Key>(),
				value: scale_info::meta_type::<Value>(),
			},
			default: OnEmpty::get().encode(),
			docs,
		};

		entries.push(entry);
	}
}

impl<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues> crate::traits::StorageInfoTrait
	for StorageMap<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues>
where
	Prefix: StorageInstance,
	Hasher: crate::hash::StorageHasher,
	Key: FullCodec + MaxEncodedLen,
	Value: FullCodec + MaxEncodedLen,
	QueryKind: QueryKindTrait<Value, OnEmpty>,
	OnEmpty: Get<QueryKind::Query> + 'static,
	MaxValues: Get<Option<u32>>,
{
	fn storage_info() -> Vec<StorageInfo> {
		vec![StorageInfo {
			pallet_name: Self::pallet_prefix().to_vec(),
			storage_name: Self::storage_prefix().to_vec(),
			prefix: Self::final_prefix().to_vec(),
			max_values: MaxValues::get(),
			max_size: Some(
				Hasher::max_len::<Key>()
					.saturating_add(Value::max_encoded_len())
					.saturated_into(),
			),
		}]
	}
}

/// It doesn't require to implement `MaxEncodedLen` and give no information for `max_size`.
impl<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues>
	crate::traits::PartialStorageInfoTrait
	for StorageMap<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues>
where
	Prefix: StorageInstance,
	Hasher: crate::hash::StorageHasher,
	Key: FullCodec,
	Value: FullCodec,
	QueryKind: QueryKindTrait<Value, OnEmpty>,
	OnEmpty: Get<QueryKind::Query> + 'static,
	MaxValues: Get<Option<u32>>,
{
	fn partial_storage_info() -> Vec<StorageInfo> {
		vec![StorageInfo {
			pallet_name: Self::pallet_prefix().to_vec(),
			storage_name: Self::storage_prefix().to_vec(),
			prefix: Self::final_prefix().to_vec(),
			max_values: MaxValues::get(),
			max_size: None,
		}]
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::{
		hash::*,
		storage::{
			types::{
				test::{frame_system, key_after_prefix, key_before_prefix, Runtime},
				ValueQuery,
			},
			IterableStorageMap,
		},
	};
	use sp_io::{hashing::twox_128, TestExternalities};
	use sp_metadata_ir::{StorageEntryModifierIR, StorageEntryTypeIR, StorageHasherIR};

	struct Prefix;
	impl StorageInstance for Prefix {
		fn pallet_prefix() -> &'static str {
			"test"
		}
		const STORAGE_PREFIX: &'static str = "foo";
	}

	struct ADefault;
	impl crate::traits::Get<u32> for ADefault {
		fn get() -> u32 {
			97
		}
	}

	#[test]
	fn keylenof_works() {
		// Works with Blake2_128Concat.
		type A = StorageMap<Prefix, Blake2_128Concat, u32, u32>;
		let size = 16 * 2 // Two Twox128
			+ 16 + 4; // Blake2_128Concat = hash + key
		assert_eq!(KeyLenOf::<A>::get(), size);

		// Works with Blake2_256.
		type B = StorageMap<Prefix, Blake2_256, u32, u32>;
		let size = 16 * 2 // Two Twox128
			+ 32; // Blake2_256
		assert_eq!(KeyLenOf::<B>::get(), size);

		// Works with Twox64Concat.
		type C = StorageMap<Prefix, Twox64Concat, u32, u32>;
		let size = 16 * 2 // Two Twox128
			+ 8 + 4; // Twox64Concat = hash + key
		assert_eq!(KeyLenOf::<C>::get(), size);
	}

	#[test]
	fn test() {
		type A = StorageMap<Prefix, Blake2_128Concat, u16, u32, OptionQuery>;
		type AValueQueryWithAnOnEmpty =
			StorageMap<Prefix, Blake2_128Concat, u16, u32, ValueQuery, ADefault>;
		type B = StorageMap<Prefix, Blake2_256, u16, u32, ValueQuery>;
		type C = StorageMap<Prefix, Blake2_128Concat, u16, u8, ValueQuery>;
		type WithLen = StorageMap<Prefix, Blake2_128Concat, u16, Vec<u32>>;

		TestExternalities::default().execute_with(|| {
			let mut k: Vec<u8> = vec![];
			k.extend(&twox_128(b"test"));
			k.extend(&twox_128(b"foo"));
			k.extend(&3u16.blake2_128_concat());
			assert_eq!(A::hashed_key_for(3).to_vec(), k);

			assert_eq!(A::contains_key(3), false);
			assert_eq!(A::get(3), None);
			assert_eq!(AValueQueryWithAnOnEmpty::get(3), 97);

			A::insert(3, 10);
			assert_eq!(A::contains_key(3), true);
			assert_eq!(A::get(3), Some(10));
			assert_eq!(A::try_get(3), Ok(10));
			assert_eq!(AValueQueryWithAnOnEmpty::get(3), 10);

			A::swap(3, 2);
			assert_eq!(A::contains_key(3), false);
			assert_eq!(A::contains_key(2), true);
			assert_eq!(A::get(3), None);
			assert_eq!(A::try_get(3), Err(()));
			assert_eq!(AValueQueryWithAnOnEmpty::get(3), 97);
			assert_eq!(A::get(2), Some(10));
			assert_eq!(AValueQueryWithAnOnEmpty::get(2), 10);

			A::remove(2);
			assert_eq!(A::contains_key(2), false);
			assert_eq!(A::get(2), None);

			AValueQueryWithAnOnEmpty::mutate(2, |v| *v = *v * 2);
			AValueQueryWithAnOnEmpty::mutate(2, |v| *v = *v * 2);
			assert_eq!(AValueQueryWithAnOnEmpty::contains_key(2), true);
			assert_eq!(AValueQueryWithAnOnEmpty::get(2), 97 * 4);

			A::remove(2);
			let _: Result<(), ()> = AValueQueryWithAnOnEmpty::try_mutate(2, |v| {
				*v = *v * 2;
				Ok(())
			});
			let _: Result<(), ()> = AValueQueryWithAnOnEmpty::try_mutate(2, |v| {
				*v = *v * 2;
				Ok(())
			});
			assert_eq!(A::contains_key(2), true);
			assert_eq!(A::get(2), Some(97 * 4));

			A::remove(2);
			let _: Result<(), ()> = AValueQueryWithAnOnEmpty::try_mutate(2, |v| {
				*v = *v * 2;
				Err(())
			});
			assert_eq!(A::contains_key(2), false);

			A::remove(2);
			AValueQueryWithAnOnEmpty::mutate_exists(2, |v| {
				assert!(v.is_none());
				*v = Some(10);
			});
			assert_eq!(A::contains_key(2), true);
			assert_eq!(A::get(2), Some(10));
			AValueQueryWithAnOnEmpty::mutate_exists(2, |v| {
				*v = Some(v.unwrap() * 10);
			});
			assert_eq!(A::contains_key(2), true);
			assert_eq!(A::get(2), Some(100));

			A::remove(2);
			let _: Result<(), ()> = AValueQueryWithAnOnEmpty::try_mutate_exists(2, |v| {
				assert!(v.is_none());
				*v = Some(10);
				Ok(())
			});
			assert_eq!(A::contains_key(2), true);
			assert_eq!(A::get(2), Some(10));
			let _: Result<(), ()> = AValueQueryWithAnOnEmpty::try_mutate_exists(2, |v| {
				*v = Some(v.unwrap() * 10);
				Ok(())
			});
			assert_eq!(A::contains_key(2), true);
			assert_eq!(A::get(2), Some(100));
			let _: Result<(), ()> = AValueQueryWithAnOnEmpty::try_mutate_exists(2, |v| {
				*v = Some(v.unwrap() * 10);
				Err(())
			});
			assert_eq!(A::contains_key(2), true);
			assert_eq!(A::get(2), Some(100));

			A::insert(2, 10);
			assert_eq!(A::take(2), Some(10));
			assert_eq!(A::contains_key(2), false);
			assert_eq!(AValueQueryWithAnOnEmpty::take(2), 97);
			assert_eq!(A::contains_key(2), false);

			// Set non-existing.
			B::set(30, 100);

			assert_eq!(B::contains_key(30), true);
			assert_eq!(B::get(30), 100);
			assert_eq!(B::try_get(30), Ok(100));

			// Set existing.
			B::set(30, 101);

			assert_eq!(B::contains_key(30), true);
			assert_eq!(B::get(30), 101);
			assert_eq!(B::try_get(30), Ok(101));

			// Set non-existing.
			A::set(30, Some(100));

			assert_eq!(A::contains_key(30), true);
			assert_eq!(A::get(30), Some(100));
			assert_eq!(A::try_get(30), Ok(100));

			// Set existing.
			A::set(30, Some(101));

			assert_eq!(A::contains_key(30), true);
			assert_eq!(A::get(30), Some(101));
			assert_eq!(A::try_get(30), Ok(101));

			// Unset existing.
			A::set(30, None);

			assert_eq!(A::contains_key(30), false);
			assert_eq!(A::get(30), None);
			assert_eq!(A::try_get(30), Err(()));

			// Unset non-existing.
			A::set(31, None);

			assert_eq!(A::contains_key(31), false);
			assert_eq!(A::get(31), None);
			assert_eq!(A::try_get(31), Err(()));

			B::insert(2, 10);
			assert_eq!(A::migrate_key::<Blake2_256, _>(2), Some(10));
			assert_eq!(A::contains_key(2), true);
			assert_eq!(A::get(2), Some(10));

			A::insert(3, 10);
			A::insert(4, 10);
			let _ = A::clear(u32::max_value(), None);
			assert_eq!(A::contains_key(3), false);
			assert_eq!(A::contains_key(4), false);

			A::insert(3, 10);
			A::insert(4, 10);
			assert_eq!(A::iter_values().collect::<Vec<_>>(), vec![10, 10]);

			C::insert(3, 10);
			C::insert(4, 10);
			A::translate_values::<u8, _>(|v| Some((v * 2).into()));
			assert_eq!(A::iter().collect::<Vec<_>>(), vec![(4, 20), (3, 20)]);

			A::insert(3, 10);
			A::insert(4, 10);
			assert_eq!(A::iter().collect::<Vec<_>>(), vec![(4, 10), (3, 10)]);
			assert_eq!(A::drain().collect::<Vec<_>>(), vec![(4, 10), (3, 10)]);
			assert_eq!(A::iter().collect::<Vec<_>>(), vec![]);

			C::insert(3, 10);
			C::insert(4, 10);
			A::translate::<u8, _>(|k, v| Some((k * v as u16).into()));
			assert_eq!(A::iter().collect::<Vec<_>>(), vec![(4, 40), (3, 30)]);

			let translate_next = |k: u16, v: u8| Some((v as u16 / k).into());
			let k = A::translate_next::<u8, _>(None, translate_next);
			let k = A::translate_next::<u8, _>(k, translate_next);
			assert_eq!(None, A::translate_next::<u8, _>(k, translate_next));
			assert_eq!(A::iter().collect::<Vec<_>>(), vec![(4, 10), (3, 10)]);

			let _ = A::translate_next::<u8, _>(None, |_, _| None);
			assert_eq!(A::iter().collect::<Vec<_>>(), vec![(3, 10)]);

			let mut entries = vec![];
			A::build_metadata(vec![], &mut entries);
			AValueQueryWithAnOnEmpty::build_metadata(vec![], &mut entries);
			assert_eq!(
				entries,
				vec![
					StorageEntryMetadataIR {
						name: "foo",
						modifier: StorageEntryModifierIR::Optional,
						ty: StorageEntryTypeIR::Map {
							hashers: vec![StorageHasherIR::Blake2_128Concat],
							key: scale_info::meta_type::<u16>(),
							value: scale_info::meta_type::<u32>(),
						},
						default: Option::<u32>::None.encode(),
						docs: vec![],
					},
					StorageEntryMetadataIR {
						name: "foo",
						modifier: StorageEntryModifierIR::Default,
						ty: StorageEntryTypeIR::Map {
							hashers: vec![StorageHasherIR::Blake2_128Concat],
							key: scale_info::meta_type::<u16>(),
							value: scale_info::meta_type::<u32>(),
						},
						default: 97u32.encode(),
						docs: vec![],
					}
				]
			);

			let _ = WithLen::clear(u32::max_value(), None);
			assert_eq!(WithLen::decode_len(3), None);
			WithLen::append(0, 10);
			assert_eq!(WithLen::decode_len(0), Some(1));
		})
	}

	#[test]
	fn map_iter_from() {
		sp_io::TestExternalities::default().execute_with(|| {
			use crate::hash::Identity;
			#[crate::storage_alias]
			type MyMap = StorageMap<MyModule, Identity, u64, u64>;

			MyMap::insert(1, 10);
			MyMap::insert(2, 20);
			MyMap::insert(3, 30);
			MyMap::insert(4, 40);
			MyMap::insert(5, 50);

			let starting_raw_key = MyMap::storage_map_final_key(3);
			let iter = MyMap::iter_from(starting_raw_key);
			assert_eq!(iter.collect::<Vec<_>>(), vec![(4, 40), (5, 50)]);

			let starting_raw_key = MyMap::storage_map_final_key(2);
			let iter = MyMap::iter_keys_from(starting_raw_key);
			assert_eq!(iter.collect::<Vec<_>>(), vec![3, 4, 5]);
		});
	}

	#[cfg(debug_assertions)]
	#[test]
	#[should_panic]
	fn map_translate_with_bad_key_in_debug_mode() {
		sp_io::TestExternalities::default().execute_with(|| {
			type Map = frame_system::Map<Runtime>;
			let prefix = Map::prefix_hash().to_vec();

			// Wrong key
			unhashed::put(&[prefix.clone(), vec![1, 2, 3]].concat(), &3u64.encode());

			// debug_assert should cause a
			Map::translate(|_k1, v: u64| Some(v * 2));
			assert_eq!(Map::iter().collect::<Vec<_>>(), vec![(3, 6), (0, 0), (2, 4), (1, 2)]);
		})
	}

	#[cfg(debug_assertions)]
	#[test]
	#[should_panic]
	fn map_translate_with_bad_value_in_debug_mode() {
		sp_io::TestExternalities::default().execute_with(|| {
			type Map = frame_system::Map<Runtime>;
			let prefix = Map::prefix_hash().to_vec();

			// Wrong value
			unhashed::put(
				&[prefix.clone(), crate::Blake2_128Concat::hash(&6u16.encode())].concat(),
				&vec![1],
			);

			Map::translate(|_k1, v: u64| Some(v * 2));
			assert_eq!(Map::iter().collect::<Vec<_>>(), vec![(3, 6), (0, 0), (2, 4), (1, 2)]);
		})
	}

	#[cfg(not(debug_assertions))]
	#[test]
	fn map_translate_with_bad_key_in_production_mode() {
		sp_io::TestExternalities::default().execute_with(|| {
			type Map = frame_system::Map<Runtime>;
			let prefix = Map::prefix_hash().to_vec();

			// Wrong key
			unhashed::put(&[prefix.clone(), vec![1, 2, 3]].concat(), &3u64.encode());

			Map::translate(|_k1, v: u64| Some(v * 2));
			assert_eq!(Map::iter().collect::<Vec<_>>(), vec![]);
		})
	}

	#[cfg(not(debug_assertions))]
	#[test]
	fn map_translate_with_bad_value_in_production_mode() {
		sp_io::TestExternalities::default().execute_with(|| {
			type Map = frame_system::Map<Runtime>;
			let prefix = Map::prefix_hash().to_vec();

			// Wrong value
			unhashed::put(
				&[prefix.clone(), crate::Blake2_128Concat::hash(&6u16.encode())].concat(),
				&vec![1],
			);

			Map::translate(|_k1, v: u64| Some(v * 2));
			assert_eq!(Map::iter().collect::<Vec<_>>(), vec![]);
		})
	}

	#[test]
	fn map_reversible_reversible_iteration() {
		sp_io::TestExternalities::default().execute_with(|| {
			type Map = frame_system::Map<Runtime>;

			// All map iterator
			let prefix = Map::prefix_hash().to_vec();

			unhashed::put(&key_before_prefix(prefix.clone()), &1u64);
			unhashed::put(&key_after_prefix(prefix.clone()), &1u64);

			for i in 0..4 {
				Map::insert(i as u16, i as u64);
			}

			assert_eq!(Map::iter().collect::<Vec<_>>(), vec![(3, 3), (0, 0), (2, 2), (1, 1)]);

			assert_eq!(Map::iter_keys().collect::<Vec<_>>(), vec![3, 0, 2, 1]);

			assert_eq!(Map::iter_values().collect::<Vec<_>>(), vec![3, 0, 2, 1]);

			assert_eq!(Map::drain().collect::<Vec<_>>(), vec![(3, 3), (0, 0), (2, 2), (1, 1)]);

			assert_eq!(Map::iter().collect::<Vec<_>>(), vec![]);
			assert_eq!(unhashed::get(&key_before_prefix(prefix.clone())), Some(1u64));
			assert_eq!(unhashed::get(&key_after_prefix(prefix.clone())), Some(1u64));

			// Translate
			let prefix = Map::prefix_hash().to_vec();

			unhashed::put(&key_before_prefix(prefix.clone()), &1u64);
			unhashed::put(&key_after_prefix(prefix.clone()), &1u64);
			for i in 0..4 {
				Map::insert(i as u16, i as u64);
			}

			Map::translate(|_k1, v: u64| Some(v * 2));
			assert_eq!(Map::iter().collect::<Vec<_>>(), vec![(3, 6), (0, 0), (2, 4), (1, 2)]);
		})
	}
}
