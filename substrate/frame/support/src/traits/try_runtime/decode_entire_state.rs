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
		CountedStorageMapInstance, CountedStorageNMapInstance, Counter, Key as NMapKey,
		KeyGenerator, QueryKindTrait,
	},
	traits::{PartialStorageInfoTrait, StorageInfo},
	StorageHasher,
};
use alloc::{vec, vec::Vec};
use codec::{Decode, DecodeAll, FullCodec};
use impl_trait_for_tuples::impl_for_tuples;
use sp_core::Get;
use sp_metadata_ir::StorageHasherIR;

/// Decode the entire data under the given storage type.
///
/// For values, this is trivial. For all kinds of maps, it should decode all the values associated
/// with all keys existing in the map, and the keys themselves (see `is_reversible`).
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

/// Which part of a storage entry could not be decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UndecodableStoragePart {
	/// The map key.
	Key,
	/// The value.
	Value,
}

/// A storage entry could not be decoded.
#[derive(Clone, PartialEq, Eq)]
pub struct TryDecodeEntireStorageError {
	/// The key of the undecodable value.
	pub key: Vec<u8>,
	/// The raw value.
	pub raw: Option<Vec<u8>>,
	/// The storage info of the key.
	pub info: StorageInfo,
	/// Whether the key or the value is undecodable.
	pub part: UndecodableStoragePart,
}

impl core::fmt::Display for TryDecodeEntireStorageError {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		write!(
			f,
			"`{}::{}` key `{}` has an undecodable {}",
			&alloc::str::from_utf8(&self.info.pallet_name).unwrap_or("<invalid>"),
			&alloc::str::from_utf8(&self.info.storage_name).unwrap_or("<invalid>"),
			array_bytes::bytes2hex("0x", &self.key),
			match self.part {
				UndecodableStoragePart::Key => "key",
				UndecodableStoragePart::Value => "value",
			},
		)
	}
}

impl core::fmt::Debug for TryDecodeEntireStorageError {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		write!(
			f,
			"part: {:?} key: {} value: {} info: {:?}",
			self.part,
			array_bytes::bytes2hex("0x", &self.key),
			array_bytes::bytes2hex("0x", self.raw.clone().unwrap_or_default()),
			self.info
		)
	}
}

/// Decode the key parts following the storage prefix. Parts behind an opaque hasher are only
/// length-checked.
pub trait TryDecodeFinalKey {
	/// Decode the key parts, advancing `key` past the consumed bytes.
	fn try_decode_final_key(key: &mut &[u8]) -> Result<(), codec::Error>;

	/// Whether any key part is behind an opaque hasher.
	fn has_opaque_part() -> bool;
}

impl TryDecodeFinalKey for () {
	fn try_decode_final_key(_: &mut &[u8]) -> Result<(), codec::Error> {
		Ok(())
	}

	fn has_opaque_part() -> bool {
		false
	}
}

/// Whether `H` keeps the encoded key, i.e. implements [`crate::ReversibleStorageHasher`].
fn is_reversible<H: StorageHasher>() -> bool {
	match H::METADATA {
		StorageHasherIR::Identity |
		StorageHasherIR::Twox64Concat |
		StorageHasherIR::Blake2_128Concat => true,
		StorageHasherIR::Blake2_128 |
		StorageHasherIR::Blake2_256 |
		StorageHasherIR::Twox128 |
		StorageHasherIR::Twox256 => false,
	}
}

impl<H: StorageHasher, K: Decode> TryDecodeFinalKey for NMapKey<H, K> {
	fn try_decode_final_key(key: &mut &[u8]) -> Result<(), codec::Error> {
		let hash_len = H::max_len::<()>();
		if key.len() < hash_len {
			return Err("Storage key is shorter than the hash of its hasher".into());
		}
		*key = &key[hash_len..];
		if is_reversible::<H>() {
			K::decode(key)?;
		}
		Ok(())
	}

	fn has_opaque_part() -> bool {
		!is_reversible::<H>()
	}
}

#[impl_for_tuples(1, 18)]
impl TryDecodeFinalKey for Tuple {
	fn try_decode_final_key(key: &mut &[u8]) -> Result<(), codec::Error> {
		for_tuples!( #( Tuple::try_decode_final_key(key)?; )* );
		Ok(())
	}

	fn has_opaque_part() -> bool {
		for_tuples!( #( Tuple::has_opaque_part() )|* )
	}
}

/// Decode all the values based on the prefix of `info` to `V`, and their keys to `K`.
///
/// Basically, it decodes and sums up all the values who's key start with `info.prefix`. For values,
/// this would be the value itself. For all sorts of maps, this should be all map items in the
/// absence of key collision.
fn decode_storage_info<K: TryDecodeFinalKey, V: Decode>(
	info: StorageInfo,
) -> Result<usize, Vec<TryDecodeEntireStorageError>> {
	let mut decoded = 0;
	let mut errors = vec![];
	let mut entries = 0usize;
	let mut next_key = Some(info.prefix.clone());
	loop {
		match next_key {
			Some(key) if key.starts_with(&info.prefix) => {
				entries += 1;
				// Check both the key and the value, reporting each failure.
				if let Some(bytes) = sp_io::storage::get(&key) {
					let err = |part| TryDecodeEntireStorageError {
						key: key.clone(),
						raw: Some(bytes.to_vec()),
						info: info.clone(),
						part,
					};
					let mut key_rest = &key[info.prefix.len()..];
					if K::try_decode_final_key(&mut key_rest).is_err() || !key_rest.is_empty() {
						errors.push(err(UndecodableStoragePart::Key));
					}
					match <V as DecodeAll>::decode_all(&mut bytes.as_ref()) {
						Ok(_) => decoded += bytes.len(),
						Err(_) => errors.push(err(UndecodableStoragePart::Value)),
					}
				}
				next_key = sp_io::storage::next_key(&key);
			},
			_ => break,
		}
	}

	// Once per storage item, not per entry.
	if entries > 0 && K::has_opaque_part() {
		log::warn!(
			target: "runtime::try-decode-state",
			"`{}::{}`: {entries} key(s) behind an opaque hasher are only length-checked",
			alloc::str::from_utf8(&info.pallet_name).unwrap_or("<invalid>"),
			alloc::str::from_utf8(&info.storage_name).unwrap_or("<invalid>"),
		);
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
		decode_storage_info::<(), Value>(info)
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
		decode_storage_info::<NMapKey<Hasher, Key>, Value>(info)
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
	>
where
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
		let mut decoded = decode_storage_info::<(), Counter>(counter_info)?;
		decoded += decode_storage_info::<NMapKey<Hasher, Key>, Value>(map_info)?;
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
	>
where
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
		decode_storage_info::<(NMapKey<Hasher1, Key1>, NMapKey<Hasher2, Key2>), Value>(info)
	}
}

impl<Prefix, Key, Value, QueryKind, OnEmpty, MaxValues> TryDecodeEntireStorage
	for crate::storage::types::StorageNMap<Prefix, Key, Value, QueryKind, OnEmpty, MaxValues>
where
	Prefix: StorageInstance,
	Key: KeyGenerator + TryDecodeFinalKey,
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
		decode_storage_info::<Key, Value>(info)
	}
}

impl<Prefix, Key, Value, QueryKind, OnEmpty, MaxValues> TryDecodeEntireStorage
	for crate::storage::types::CountedStorageNMap<Prefix, Key, Value, QueryKind, OnEmpty, MaxValues>
where
	Prefix: CountedStorageNMapInstance,
	Key: KeyGenerator + TryDecodeFinalKey,
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

		let mut decoded = decode_storage_info::<(), Counter>(counter_info)?;
		decoded += decode_storage_info::<Key, Value>(map_info)?;
		Ok(decoded)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		storage::types::{self, CountedStorageMapInstance, CountedStorageNMapInstance, Key},
		Blake2_128, Blake2_128Concat, Identity, StorageHasher, Twox64Concat,
	};
	use codec::Encode;

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
			let errors = Map::try_decode_entire_state().unwrap_err();
			assert_eq!(errors.len(), 1);
			assert_eq!(errors[0].part, UndecodableStoragePart::Value);

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

	/// `OldKey` with the `V3` variant removed.
	#[derive(Encode, Decode, scale_info::TypeInfo, Clone, PartialEq, Eq, Debug)]
	enum NewKey {
		#[codec(index = 4)]
		V4(u32),
		#[codec(index = 5)]
		V5(u32),
	}

	#[derive(Encode)]
	enum OldKey {
		#[codec(index = 3)]
		V3(u32),
	}

	build_prefix!(EnumMapPrefix);
	type EnumMap = types::StorageMap<EnumMapPrefix, Blake2_128Concat, NewKey, u32>;

	build_prefix!(TwoxMapPrefix);
	type TwoxMap = types::StorageMap<TwoxMapPrefix, Twox64Concat, u32, u32>;

	build_prefix!(IdentityMapPrefix);
	type IdentityMap = types::StorageMap<IdentityMapPrefix, Identity, u32, u32>;

	build_prefix!(OpaqueMapPrefix);
	type OpaqueMap = types::StorageMap<OpaqueMapPrefix, Blake2_128, NewKey, u32>;

	build_prefix!(EnumDMapPrefix);
	type EnumDMap = types::StorageDoubleMap<EnumDMapPrefix, H, u32, Twox64Concat, NewKey, u32>;

	build_prefix!(EnumNMapPrefix);
	type EnumNMap =
		types::StorageNMap<EnumNMapPrefix, (Key<H, u8>, Key<Blake2_128, u8>, Key<H, NewKey>), u32>;

	build_prefix!(EnumCMapCounterPrefix);
	build_prefix!(EnumCMapPrefix);
	impl CountedStorageMapInstance for EnumCMapPrefix {
		type CounterPrefix = EnumCMapCounterPrefix;
	}
	type EnumCMap = types::CountedStorageMap<EnumCMapPrefix, H, NewKey, u32>;

	/// Store `42u32` under `P`'s prefix followed by `hashed_key`, returning the key.
	fn set_raw<P: StorageInstance>(hashed_key: &[u8]) -> Vec<u8> {
		let key = [&P::prefix_hash()[..], hashed_key].concat();
		sp_io::storage::set(&key, &42u32.encode());
		key
	}

	/// Assert a single undecodable-key error and return it.
	fn single_key_error(
		res: Result<usize, Vec<TryDecodeEntireStorageError>>,
	) -> TryDecodeEntireStorageError {
		let mut errors = res.unwrap_err();
		assert_eq!(errors.len(), 1);
		assert_eq!(errors[0].part, UndecodableStoragePart::Key);
		errors.remove(0)
	}

	#[test]
	fn undecodable_map_key_is_detected() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			EnumMap::insert(NewKey::V4(1), 42);
			EnumMap::insert(NewKey::V5(1), 42);
			assert_eq!(EnumMap::try_decode_entire_state(), Ok(8));

			// A removed variant: the value decodes, the key does not.
			let key = set_raw::<EnumMapPrefix>(&H::hash(&OldKey::V3(1).encode()));
			assert_eq!(single_key_error(EnumMap::try_decode_entire_state()).key, key);
		})
	}

	#[test]
	fn undecodable_key_and_value_are_both_reported() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			let key =
				[&EnumMapPrefix::prefix_hash()[..], &H::hash(&OldKey::V3(1).encode())].concat();
			// two bytes, cannot be decoded into u32.
			sp_io::storage::set(&key, &[0u8, 1]);

			let errors = EnumMap::try_decode_entire_state().unwrap_err();
			assert_eq!(
				errors.iter().map(|e| (e.part, e.key.clone())).collect::<Vec<_>>(),
				vec![
					(UndecodableStoragePart::Key, key.clone()),
					(UndecodableStoragePart::Value, key)
				],
			);
		})
	}

	#[test]
	fn map_key_with_trailing_or_missing_bytes_is_detected() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			// trailing bytes after a valid `u32` key.
			set_raw::<TwoxMapPrefix>(&[Twox64Concat::hash(&1u32.encode()), vec![0]].concat());
			single_key_error(TwoxMap::try_decode_entire_state());

			// key shorter than a `u32`.
			set_raw::<IdentityMapPrefix>(&[1, 2]);
			single_key_error(IdentityMap::try_decode_entire_state());

			// key shorter than the hash of the hasher.
			set_raw::<MapPrefix>(&[0u8; 10]);
			single_key_error(Map::try_decode_entire_state());
		});
	}

	#[test]
	fn opaque_hasher_keys_are_only_length_checked() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			OpaqueMap::insert(NewKey::V4(1), 42);
			// A removed variant is not detectable behind an opaque hasher...
			set_raw::<OpaqueMapPrefix>(&Blake2_128::hash(&OldKey::V3(1).encode()));
			assert_eq!(OpaqueMap::try_decode_entire_state(), Ok(8));

			// ...but a key of the wrong length is.
			set_raw::<OpaqueMapPrefix>(&[0u8; 17]);
			single_key_error(OpaqueMap::try_decode_entire_state());
		})
	}

	#[test]
	fn undecodable_double_map_key_is_detected() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			EnumDMap::insert(1, NewKey::V4(1), 42);
			assert_eq!(EnumDMap::try_decode_entire_state(), Ok(4));

			set_raw::<EnumDMapPrefix>(
				&[H::hash(&1u32.encode()), Twox64Concat::hash(&OldKey::V3(1).encode())].concat(),
			);
			single_key_error(EnumDMap::try_decode_entire_state());
		})
	}

	#[test]
	fn undecodable_n_map_key_is_detected() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			EnumNMap::insert((1, 2, NewKey::V5(3)), 42);
			assert_eq!(EnumNMap::try_decode_entire_state(), Ok(4));

			set_raw::<EnumNMapPrefix>(
				&[
					H::hash(&1u8.encode()),
					Blake2_128::hash(&2u8.encode()).to_vec(),
					H::hash(&OldKey::V3(3).encode()),
				]
				.concat(),
			);
			single_key_error(EnumNMap::try_decode_entire_state());
		})
	}

	#[test]
	fn undecodable_counted_map_key_is_detected() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			EnumCMap::insert(NewKey::V4(1), 42);
			assert_eq!(EnumCMap::try_decode_entire_state(), Ok(4 + 4));

			set_raw::<EnumCMapPrefix>(&H::hash(&OldKey::V3(1).encode()));
			single_key_error(EnumCMap::try_decode_entire_state());
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
