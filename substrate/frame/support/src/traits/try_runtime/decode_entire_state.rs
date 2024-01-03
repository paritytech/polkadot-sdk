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

//! Types to check that the entire storage can be decoded.

use super::StorageInstance;
use crate::{
	storage::types::{
		CountedStorageMapInstance, CountedStorageNMapInstance, Counter, KeyGenerator,
		QueryKindTrait,
	},
	traits::{PartialStorageInfoTrait, StorageInfo},
	StorageHasher,
};
use codec::{Decode, DecodeAll, FullCodec};
use impl_trait_for_tuples::impl_for_tuples;
use sp_core::Get;
use sp_std::prelude::*;

/// Decode the entire data under the given storage type.
///
/// For values, this is trivial. For all kinds of maps, it should decode all the values associated
/// with all keys existing in the map.
///
/// Tuple implementations are provided and simply decode each type in the tuple, summing up the
/// decoded bytes if `Ok(_)`.
pub trait TryDecodeEntireStorage {
	/// Decode the entire data under the given storage, returning `Ok(bytes_decoded)` if success.
	fn try_decode_entire_state() -> Result<usize, Vec<TryDecodeEntireStorageError>>;
}

#[cfg_attr(all(not(feature = "tuples-96"), not(feature = "tuples-128")), impl_for_tuples(64))]
#[cfg_attr(all(feature = "tuples-96", not(feature = "tuples-128")), impl_for_tuples(96))]
#[cfg_attr(feature = "tuples-128", impl_for_tuples(128))]
impl TryDecodeEntireStorage for Tuple {
	fn try_decode_entire_state() -> Result<usize, Vec<TryDecodeEntireStorageError>> {
		let mut errors = Vec::new();
		let mut len = 0usize;

		for_tuples!(#(
			match Tuple::try_decode_entire_state() {
				Ok(bytes) => len += bytes,
				Err(errs) => errors.extend(errs),
			}
		)*);

		if errors.is_empty() {
			Ok(len)
		} else {
			Err(errors)
		}
	}
}

/// A value could not be decoded.
#[derive(Clone, PartialEq, Eq)]
pub struct TryDecodeEntireStorageError {
	/// The key of the undecodable value.
	pub key: Vec<u8>,
	/// The raw value.
	pub raw: Option<Vec<u8>>,
	/// The storage info of the key.
	pub info: StorageInfo,
}

impl core::fmt::Display for TryDecodeEntireStorageError {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		write!(
			f,
			"`{}::{}` key `{}` is undecodable",
			&sp_std::str::from_utf8(&self.info.pallet_name).unwrap_or("<invalid>"),
			&sp_std::str::from_utf8(&self.info.storage_name).unwrap_or("<invalid>"),
			array_bytes::bytes2hex("0x", &self.key)
		)
	}
}

impl core::fmt::Debug for TryDecodeEntireStorageError {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		write!(
			f,
			"key: {} value: {} info: {:?}",
			array_bytes::bytes2hex("0x", &self.key),
			array_bytes::bytes2hex("0x", self.raw.clone().unwrap_or_default()),
			self.info
		)
	}
}

/// Decode all the values based on the prefix of `info` to `V`.
///
/// Basically, it decodes and sums up all the values who's key start with `info.prefix`. For values,
/// this would be the value itself. For all sorts of maps, this should be all map items in the
/// absence of key collision.
fn decode_storage_info<V: Decode>(
	info: StorageInfo,
) -> Result<usize, Vec<TryDecodeEntireStorageError>> {
	let mut decoded = 0;

	let decode_key = |key: &[u8]| match sp_io::storage::get(key) {
		None => Ok(0),
		Some(bytes) => {
			let len = bytes.len();
			let _ = <V as DecodeAll>::decode_all(&mut bytes.as_ref()).map_err(|_| {
				TryDecodeEntireStorageError {
					key: key.to_vec(),
					raw: Some(bytes.to_vec()),
					info: info.clone(),
				}
			})?;

			Ok::<usize, _>(len)
		},
	};

	let mut errors = vec![];
	let mut next_key = Some(info.prefix.clone());
	loop {
		match next_key {
			Some(key) if key.starts_with(&info.prefix) => {
				match decode_key(&key) {
					Ok(bytes) => {
						decoded += bytes;
					},
					Err(e) => errors.push(e),
				};
				next_key = sp_io::storage::next_key(&key);
			},
			_ => break,
		}
	}

	if errors.is_empty() {
		Ok(decoded)
	} else {
		Err(errors)
	}
}

impl<Prefix, Value, QueryKind, OnEmpty> TryDecodeEntireStorage
	for crate::storage::types::StorageValue<Prefix, Value, QueryKind, OnEmpty>
where
	Prefix: StorageInstance,
	Value: FullCodec,
	QueryKind: QueryKindTrait<Value, OnEmpty>,
	OnEmpty: Get<QueryKind::Query> + 'static,
{
	fn try_decode_entire_state() -> Result<usize, Vec<TryDecodeEntireStorageError>> {
		let info = Self::partial_storage_info()
			.first()
			.cloned()
			.expect("Value has only one storage info; qed");
		decode_storage_info::<Value>(info)
	}
}

impl<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues> TryDecodeEntireStorage
	for crate::storage::types::StorageMap<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues>
where
	Prefix: StorageInstance,
	Hasher: StorageHasher,
	Key: FullCodec,
	Value: FullCodec,
	QueryKind: QueryKindTrait<Value, OnEmpty>,
	OnEmpty: Get<QueryKind::Query> + 'static,
	MaxValues: Get<Option<u32>>,
{
	fn try_decode_entire_state() -> Result<usize, Vec<TryDecodeEntireStorageError>> {
		let info = Self::partial_storage_info()
			.first()
			.cloned()
			.expect("Map has only one storage info; qed");
		decode_storage_info::<Value>(info)
	}
}

impl<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues> TryDecodeEntireStorage
	for crate::storage::types::CountedStorageMap<
		Prefix,
		Hasher,
		Key,
		Value,
		QueryKind,
		OnEmpty,
		MaxValues,
	> where
	Prefix: CountedStorageMapInstance,
	Hasher: StorageHasher,
	Key: FullCodec,
	Value: FullCodec,
	QueryKind: QueryKindTrait<Value, OnEmpty>,
	OnEmpty: Get<QueryKind::Query> + 'static,
	MaxValues: Get<Option<u32>>,
{
	fn try_decode_entire_state() -> Result<usize, Vec<TryDecodeEntireStorageError>> {
		let (map_info, counter_info) = match &Self::partial_storage_info()[..] {
			[a, b] => (a.clone(), b.clone()),
			_ => panic!("Counted map has two storage info items; qed"),
		};
		let mut decoded = decode_storage_info::<Counter>(counter_info)?;
		decoded += decode_storage_info::<Value>(map_info)?;
		Ok(decoded)
	}
}

impl<Prefix, Hasher1, Key1, Hasher2, Key2, Value, QueryKind, OnEmpty, MaxValues>
	TryDecodeEntireStorage
	for crate::storage::types::StorageDoubleMap<
		Prefix,
		Hasher1,
		Key1,
		Hasher2,
		Key2,
		Value,
		QueryKind,
		OnEmpty,
		MaxValues,
	> where
	Prefix: StorageInstance,
	Hasher1: StorageHasher,
	Key1: FullCodec,
	Hasher2: StorageHasher,
	Key2: FullCodec,
	Value: FullCodec,
	QueryKind: QueryKindTrait<Value, OnEmpty>,
	OnEmpty: Get<QueryKind::Query> + 'static,
	MaxValues: Get<Option<u32>>,
{
	fn try_decode_entire_state() -> Result<usize, Vec<TryDecodeEntireStorageError>> {
		let info = Self::partial_storage_info()
			.first()
			.cloned()
			.expect("Double-map has only one storage info; qed");
		decode_storage_info::<Value>(info)
	}
}

impl<Prefix, Key, Value, QueryKind, OnEmpty, MaxValues> TryDecodeEntireStorage
	for crate::storage::types::StorageNMap<Prefix, Key, Value, QueryKind, OnEmpty, MaxValues>
where
	Prefix: StorageInstance,
	Key: KeyGenerator,
	Value: FullCodec,
	QueryKind: QueryKindTrait<Value, OnEmpty>,
	OnEmpty: Get<QueryKind::Query> + 'static,
	MaxValues: Get<Option<u32>>,
{
	fn try_decode_entire_state() -> Result<usize, Vec<TryDecodeEntireStorageError>> {
		let info = Self::partial_storage_info()
			.first()
			.cloned()
			.expect("N-map has only one storage info; qed");
		decode_storage_info::<Value>(info)
	}
}

impl<Prefix, Key, Value, QueryKind, OnEmpty, MaxValues> TryDecodeEntireStorage
	for crate::storage::types::CountedStorageNMap<Prefix, Key, Value, QueryKind, OnEmpty, MaxValues>
where
	Prefix: CountedStorageNMapInstance,
	Key: KeyGenerator,
	Value: FullCodec,
	QueryKind: QueryKindTrait<Value, OnEmpty>,
	OnEmpty: Get<QueryKind::Query> + 'static,
	MaxValues: Get<Option<u32>>,
{
	fn try_decode_entire_state() -> Result<usize, Vec<TryDecodeEntireStorageError>> {
		let (map_info, counter_info) = match &Self::partial_storage_info()[..] {
			[a, b] => (a.clone(), b.clone()),
			_ => panic!("Counted NMap has two storage info items; qed"),
		};

		let mut decoded = decode_storage_info::<Counter>(counter_info)?;
		decoded += decode_storage_info::<Value>(map_info)?;
		Ok(decoded)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		storage::types::{self, CountedStorageMapInstance, CountedStorageNMapInstance, Key},
		Blake2_128Concat,
	};

	type H = Blake2_128Concat;

	macro_rules! build_prefix {
		($name:ident) => {
			struct $name;
			impl StorageInstance for $name {
				fn pallet_prefix() -> &'static str {
					"test_pallet"
				}
				const STORAGE_PREFIX: &'static str = stringify!($name);
			}
		};
	}

	build_prefix!(ValuePrefix);
	type Value = types::StorageValue<ValuePrefix, u32>;

	build_prefix!(MapPrefix);
	type Map = types::StorageMap<MapPrefix, H, u32, u32>;

	build_prefix!(CMapCounterPrefix);
	build_prefix!(CMapPrefix);
	impl CountedStorageMapInstance for CMapPrefix {
		type CounterPrefix = CMapCounterPrefix;
	}
	type CMap = types::CountedStorageMap<CMapPrefix, H, u8, u16>;

	build_prefix!(DMapPrefix);
	type DMap = types::StorageDoubleMap<DMapPrefix, H, u32, H, u32, u32>;

	build_prefix!(NMapPrefix);
	type NMap = types::StorageNMap<NMapPrefix, (Key<H, u8>, Key<H, u8>), u128>;

	build_prefix!(CountedNMapCounterPrefix);
	build_prefix!(CountedNMapPrefix);
	impl CountedStorageNMapInstance for CountedNMapPrefix {
		type CounterPrefix = CountedNMapCounterPrefix;
	}
	type CNMap = types::CountedStorageNMap<CountedNMapPrefix, (Key<H, u8>, Key<H, u8>), u128>;

	#[test]
	fn try_decode_entire_state_value_works() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			assert_eq!(Value::try_decode_entire_state(), Ok(0));

			Value::put(42);
			assert_eq!(Value::try_decode_entire_state(), Ok(4));

			Value::kill();
			assert_eq!(Value::try_decode_entire_state(), Ok(0));

			// two bytes, cannot be decoded into u32.
			sp_io::storage::set(&Value::hashed_key(), &[0u8, 1]);
			assert!(Value::try_decode_entire_state().is_err());
		})
	}

	#[test]
	fn try_decode_entire_state_map_works() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			assert_eq!(Map::try_decode_entire_state(), Ok(0));

			Map::insert(0, 42);
			assert_eq!(Map::try_decode_entire_state(), Ok(4));

			Map::insert(0, 42);
			assert_eq!(Map::try_decode_entire_state(), Ok(4));

			Map::insert(1, 42);
			assert_eq!(Map::try_decode_entire_state(), Ok(8));

			Map::remove(0);
			assert_eq!(Map::try_decode_entire_state(), Ok(4));

			// two bytes, cannot be decoded into u32.
			sp_io::storage::set(&Map::hashed_key_for(2), &[0u8, 1]);
			assert!(Map::try_decode_entire_state().is_err());
			assert_eq!(Map::try_decode_entire_state().unwrap_err().len(), 1);

			// multiple errs in the same map are be detected
			sp_io::storage::set(&Map::hashed_key_for(3), &[0u8, 1]);
			assert!(Map::try_decode_entire_state().is_err());
			assert_eq!(Map::try_decode_entire_state().unwrap_err().len(), 2);
		})
	}

	#[test]
	fn try_decode_entire_state_counted_map_works() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			// counter is not even initialized;
			assert_eq!(CMap::try_decode_entire_state(), Ok(0 + 0));

			let counter = 4;
			let value_size = std::mem::size_of::<u16>();

			CMap::insert(0, 42);
			assert_eq!(CMap::try_decode_entire_state(), Ok(value_size + counter));

			CMap::insert(0, 42);
			assert_eq!(CMap::try_decode_entire_state(), Ok(value_size + counter));

			CMap::insert(1, 42);
			assert_eq!(CMap::try_decode_entire_state(), Ok(value_size * 2 + counter));

			CMap::remove(0);
			assert_eq!(CMap::try_decode_entire_state(), Ok(value_size + counter));

			// counter is cleared again.
			let _ = CMap::clear(u32::MAX, None);
			assert_eq!(CMap::try_decode_entire_state(), Ok(0 + 0));

			// 1 bytes, cannot be decoded into u16.
			sp_io::storage::set(&CMap::hashed_key_for(2), &[0u8]);
			assert!(CMap::try_decode_entire_state().is_err());
		})
	}

	#[test]
	fn try_decode_entire_state_double_works() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			assert_eq!(DMap::try_decode_entire_state(), Ok(0));

			DMap::insert(0, 0, 42);
			assert_eq!(DMap::try_decode_entire_state(), Ok(4));

			DMap::insert(0, 0, 42);
			assert_eq!(DMap::try_decode_entire_state(), Ok(4));

			DMap::insert(0, 1, 42);
			assert_eq!(DMap::try_decode_entire_state(), Ok(8));

			DMap::insert(1, 0, 42);
			assert_eq!(DMap::try_decode_entire_state(), Ok(12));

			DMap::remove(0, 0);
			assert_eq!(DMap::try_decode_entire_state(), Ok(8));

			// two bytes, cannot be decoded into u32.
			sp_io::storage::set(&DMap::hashed_key_for(1, 1), &[0u8, 1]);
			assert!(DMap::try_decode_entire_state().is_err());
		})
	}

	#[test]
	fn try_decode_entire_state_n_map_works() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			assert_eq!(NMap::try_decode_entire_state(), Ok(0));

			let value_size = std::mem::size_of::<u128>();

			NMap::insert((0u8, 0), 42);
			assert_eq!(NMap::try_decode_entire_state(), Ok(value_size));

			NMap::insert((0, 0), 42);
			assert_eq!(NMap::try_decode_entire_state(), Ok(value_size));

			NMap::insert((0, 1), 42);
			assert_eq!(NMap::try_decode_entire_state(), Ok(value_size * 2));

			NMap::insert((1, 0), 42);
			assert_eq!(NMap::try_decode_entire_state(), Ok(value_size * 3));

			NMap::remove((0, 0));
			assert_eq!(NMap::try_decode_entire_state(), Ok(value_size * 2));

			// two bytes, cannot be decoded into u128.
			sp_io::storage::set(&NMap::hashed_key_for((1, 1)), &[0u8, 1]);
			assert!(NMap::try_decode_entire_state().is_err());
		})
	}

	#[test]
	fn try_decode_entire_state_counted_n_map_works() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			sp_io::TestExternalities::new_empty().execute_with(|| {
				assert_eq!(NMap::try_decode_entire_state(), Ok(0));

				let value_size = std::mem::size_of::<u128>();
				let counter = 4;

				CNMap::insert((0u8, 0), 42);
				assert_eq!(CNMap::try_decode_entire_state(), Ok(value_size + counter));

				CNMap::insert((0, 0), 42);
				assert_eq!(CNMap::try_decode_entire_state(), Ok(value_size + counter));

				CNMap::insert((0, 1), 42);
				assert_eq!(CNMap::try_decode_entire_state(), Ok(value_size * 2 + counter));

				CNMap::insert((1, 0), 42);
				assert_eq!(CNMap::try_decode_entire_state(), Ok(value_size * 3 + counter));

				CNMap::remove((0, 0));
				assert_eq!(CNMap::try_decode_entire_state(), Ok(value_size * 2 + counter));

				// two bytes, cannot be decoded into u128.
				sp_io::storage::set(&CNMap::hashed_key_for((1, 1)), &[0u8, 1]);
				assert!(CNMap::try_decode_entire_state().is_err());
			})
		})
	}

	#[test]
	fn extra_bytes_are_rejected() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			assert_eq!(Map::try_decode_entire_state(), Ok(0));

			// 6bytes, too many to fit in u32, should be rejected.
			sp_io::storage::set(&Map::hashed_key_for(2), &[0u8, 1, 3, 4, 5, 6]);
			assert!(Map::try_decode_entire_state().is_err());
		})
	}

	#[test]
	fn try_decode_entire_state_tuple_of_storage_works() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			assert_eq!(<(Value, Map) as TryDecodeEntireStorage>::try_decode_entire_state(), Ok(0));

			Value::put(42);
			assert_eq!(<(Value, Map) as TryDecodeEntireStorage>::try_decode_entire_state(), Ok(4));

			Map::insert(0, 42);
			assert_eq!(<(Value, Map) as TryDecodeEntireStorage>::try_decode_entire_state(), Ok(8));
		});
	}
}
