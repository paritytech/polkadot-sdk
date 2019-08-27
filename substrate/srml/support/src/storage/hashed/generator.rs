// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Abstract storage to use on HashedStorage trait. Please refer to the
//! [top level docs](../../index.html) for more detailed documentation about storage traits and functions.

use crate::codec::{self, Encode};
use crate::rstd::{prelude::{Vec, Box}, iter::FromIterator};
#[cfg(feature = "std")]
use crate::storage::unhashed::generator::UnhashedStorage;
use crate::traits::{StorageDefault, Len};
use runtime_io::{twox_64, twox_128, blake2_128, twox_256, blake2_256};

pub trait StorageHasher: 'static {
	type Output: AsRef<[u8]>;
	fn hash(x: &[u8]) -> Self::Output;
}

/// Hash storage keys with `concat(twox64(key), key)`
pub struct Twox64Concat;
impl StorageHasher for Twox64Concat {
	type Output = Vec<u8>;
	fn hash(x: &[u8]) -> Vec<u8> {
		twox_64(x)
			.into_iter()
			.chain(x.into_iter())
			.cloned()
			.collect::<Vec<_>>()
	}
}

#[test]
fn test_twox_64_concat() {
	let r = Twox64Concat::hash(b"foo");
	assert_eq!(r.split_at(8), (&twox_128(b"foo")[..8], &b"foo"[..]))
}

/// Hash storage keys with blake2 128
pub struct Blake2_128;
impl StorageHasher for Blake2_128 {
	type Output = [u8; 16];
	fn hash(x: &[u8]) -> [u8; 16] {
		blake2_128(x)
	}
}

/// Hash storage keys with blake2 256
pub struct Blake2_256;
impl StorageHasher for Blake2_256 {
	type Output = [u8; 32];
	fn hash(x: &[u8]) -> [u8; 32] {
		blake2_256(x)
	}
}

/// Hash storage keys with twox 128
pub struct Twox128;
impl StorageHasher for Twox128 {
	type Output = [u8; 16];
	fn hash(x: &[u8]) -> [u8; 16] {
		twox_128(x)
	}
}

/// Hash storage keys with twox 256
pub struct Twox256;
impl StorageHasher for Twox256 {
	type Output = [u8; 32];
	fn hash(x: &[u8]) -> [u8; 32] {
		twox_256(x)
	}
}

/// Abstraction around storage.
pub trait HashedStorage<H: StorageHasher> {
	/// true if the key exists in storage.
	fn exists(&self, key: &[u8]) -> bool;

	/// Load the bytes of a key from storage. Can panic if the type is incorrect.
	fn get<T: codec::Decode>(&self, key: &[u8]) -> Option<T>;

	/// Load the bytes of a key from storage. Can panic if the type is incorrect. Will panic if
	/// it's not there.
	fn require<T: codec::Decode>(&self, key: &[u8]) -> T {
		self.get(key).expect("Required values must be in storage")
	}

	/// Load the bytes of a key from storage. Can panic if the type is incorrect. The type's
	/// default is returned if it's not there.
	fn get_or_default<T: codec::Decode + Default>(&self, key: &[u8]) -> T {
		self.get(key).unwrap_or_default()
	}

	/// Put a value in under a key.
	fn put<T: codec::Encode>(&mut self, key: &[u8], val: &T);

	/// Remove the bytes of a key from storage.
	fn kill(&mut self, key: &[u8]);

	/// Take a value from storage, deleting it after reading.
	fn take<T: codec::Decode>(&mut self, key: &[u8]) -> Option<T> {
		let value = self.get(key);
		self.kill(key);
		value
	}

	/// Take a value from storage, deleting it after reading.
	fn take_or_panic<T: codec::Decode>(&mut self, key: &[u8]) -> T {
		self.take(key).expect("Required values must be in storage")
	}

	/// Take a value from storage, deleting it after reading.
	fn take_or_default<T: codec::Decode + Default>(&mut self, key: &[u8]) -> T {
		self.take(key).unwrap_or_default()
	}

	/// Get a Vec of bytes from storage.
	fn get_raw(&self, key: &[u8]) -> Option<Vec<u8>>;

	/// Put a raw byte slice into storage.
	fn put_raw(&mut self, key: &[u8], value: &[u8]);
}

// We use a construct like this during when genesis storage is being built.
#[cfg(feature = "std")]
impl<H: StorageHasher> HashedStorage<H> for sr_primitives::StorageOverlay {
	fn exists(&self, key: &[u8]) -> bool {
		UnhashedStorage::exists(self, &H::hash(key).as_ref())
	}

	fn get<T: codec::Decode>(&self, key: &[u8]) -> Option<T> {
		UnhashedStorage::get(self, &H::hash(key).as_ref())
	}

	fn put<T: codec::Encode>(&mut self, key: &[u8], val: &T) {
		UnhashedStorage::put(self, &H::hash(key).as_ref(), val)
	}

	fn kill(&mut self, key: &[u8]) {
		UnhashedStorage::kill(self, &H::hash(key).as_ref())
	}

	fn get_raw(&self, key: &[u8]) -> Option<Vec<u8>> {
		UnhashedStorage::get_raw(self, &H::hash(key).as_ref())
	}

	fn put_raw(&mut self, key: &[u8], value: &[u8]) {
		UnhashedStorage::put_raw(self, &H::hash(key).as_ref(), value)
	}
}

/// A strongly-typed value kept in storage.
pub trait StorageValue<T: codec::Codec> {
	/// The type that get/take returns.
	type Query;
	/// Something that can provide the default value of this storage type.
	type Default: StorageDefault<T>;


	/// Get the storage key.
	fn key() -> &'static [u8];

	/// true if the value is defined in storage.
	fn exists<S: HashedStorage<Twox128>>(storage: &S) -> bool {
		storage.exists(Self::key())
	}

	/// Load the value from the provided storage instance.
	fn get<S: HashedStorage<Twox128>>(storage: &S) -> Self::Query;

	/// Take a value from storage, removing it afterwards.
	fn take<S: HashedStorage<Twox128>>(storage: &mut S) -> Self::Query;

	/// Store a value under this key into the provided storage instance.
	fn put<S: HashedStorage<Twox128>>(val: &T, storage: &mut S) {
		storage.put(Self::key(), val)
	}

	/// Store a value under this key into the provided storage instance; this can take any reference
	/// type that derefs to `T` (and has `Encode` implemented).
	/// Store a value under this key into the provided storage instance.
	fn put_ref<Arg: ?Sized + Encode, S: HashedStorage<Twox128>>(val: &Arg, storage: &mut S) where T: AsRef<Arg> {
		val.using_encoded(|b| storage.put_raw(Self::key(), b))
	}

	/// Mutate this value
	fn mutate<R, F: FnOnce(&mut Self::Query) -> R, S: HashedStorage<Twox128>>(f: F, storage: &mut S) -> R;

	/// Clear the storage value.
	fn kill<S: HashedStorage<Twox128>>(storage: &mut S) {
		storage.kill(Self::key())
	}

	/// Append the given items to the value in the storage.
	///
	/// `T` is required to implement `codec::EncodeAppend`.
	fn append<'a, S, I, R>(
		items: R,
		storage: &mut S,
	) -> Result<(), &'static str> where
		S: HashedStorage<Twox128>,
		I: 'a + codec::Encode,
		T: codec::EncodeAppend<Item=I>,
		R: IntoIterator<Item=&'a I>,
		R::IntoIter: ExactSizeIterator,
	{
		let new_val = <T as codec::EncodeAppend>::append(
			// if the key exists, directly append to it.
			storage.get_raw(Self::key()).unwrap_or_else(|| {
				// otherwise, try and read a proper __provided__ default.
				Self::Default::default().map(|v| v.encode())
					// or just use the Rust's `default()` value.
					.unwrap_or_default()
			}),
			items,
		).map_err(|_| "Could not append given item")?;
		storage.put_raw(Self::key(), &new_val);
		Ok(())
	}

	/// Safely append the given items to the value in the storage. If a codec error occurs, then the
	/// old (presumably corrupt) value is replaced with the given `items`.
	///
	/// `T` is required to implement `codec::EncodeAppend`.
	fn append_or_put<'a, S, I, R>(
		items: R,
		storage: &mut S,
	) where
		S: HashedStorage<Twox128>,
		I: 'a + codec::Encode + Clone,
		T: codec::EncodeAppend<Item=I> + FromIterator<I>,
		R: IntoIterator<Item=&'a I> + Clone,
		R::IntoIter: ExactSizeIterator,
	{
		Self::append(items.clone(), storage)
			.unwrap_or_else(|_| Self::put(&items.into_iter().cloned().collect(), storage));
	}

	/// Read the length of the value in a fast way, without decoding the entire value.
	///
	/// `T` is required to implement `Codec::DecodeLength`.
	///
	/// Note that `0` is returned as the default value if no encoded value exists at the given key.
	/// Therefore, this function cannot be used as a sign of _existence_. use the `::exists()`
	/// function for this purpose.
	fn decode_len<S: HashedStorage<Twox128>>(storage: &mut S) -> Result<usize, &'static str>
		where T: codec::DecodeLength, T: Len
	{
		// attempt to get the length directly.
		if let Some(k) = storage.get_raw(Self::key()) {
			<T as codec::DecodeLength>::len(&k).map_err(|e| e.what())
		} else {
			Ok(Self::Default::default().map(|v| v.len()).unwrap_or(0))
		}
	}
}

/// A strongly-typed map in storage.
pub trait StorageMap<K: codec::Codec, V: codec::Codec> {
	/// The type that get/take returns.
	type Query;
	/// Hasher type
	type Hasher: StorageHasher;
	/// Something that can provide the default value of this storage type.
	type Default: StorageDefault<V>;

	/// Get the prefix key in storage.
	fn prefix() -> &'static [u8];

	/// Get the storage key used to fetch a value corresponding to a specific key.
	fn key_for(x: &K) -> Vec<u8>;

	/// true if the value is defined in storage.
	fn exists<S: HashedStorage<Self::Hasher>>(key: &K, storage: &S) -> bool {
		storage.exists(&Self::key_for(key)[..])
	}

	/// Load the value associated with the given key from the map.
	fn get<S: HashedStorage<Self::Hasher>>(key: &K, storage: &S) -> Self::Query;

	/// Take the value under a key.
	fn take<S: HashedStorage<Self::Hasher>>(key: &K, storage: &mut S) -> Self::Query;

	/// Swap the values of two keys.
	fn swap<S: HashedStorage<Self::Hasher>>(key1: &K, key2: &K, storage: &mut S) {
		let k1 = Self::key_for(key1);
		let k2 = Self::key_for(key2);
		let v1 = storage.get_raw(&k1[..]);
		if let Some(val) = storage.get_raw(&k2[..]) {
			storage.put_raw(&k1[..], &val[..]);
		} else {
			storage.kill(&k1[..])
		}
		if let Some(val) = v1 {
			storage.put_raw(&k2[..], &val[..]);
		} else {
			storage.kill(&k2[..])
		}
	}

	/// Store a value to be associated with the given key from the map.
	fn insert<S: HashedStorage<Self::Hasher>>(key: &K, val: &V, storage: &mut S) {
		storage.put(&Self::key_for(key)[..], val);
	}

	/// Store a value under this key into the provided storage instance; this can take any reference
	/// type that derefs to `T` (and has `Encode` implemented).
	/// Store a value under this key into the provided storage instance.
	fn insert_ref<Arg: ?Sized + Encode, S: HashedStorage<Self::Hasher>>(
		key: &K,
		val: &Arg,
		storage: &mut S
	) where V: AsRef<Arg> {
		val.using_encoded(|b| storage.put_raw(&Self::key_for(key)[..], b))
	}

	/// Remove the value under a key.
	fn remove<S: HashedStorage<Self::Hasher>>(key: &K, storage: &mut S) {
		storage.kill(&Self::key_for(key)[..]);
	}

	/// Mutate the value under a key.
	fn mutate<R, F: FnOnce(&mut Self::Query) -> R, S: HashedStorage<Self::Hasher>>(key: &K, f: F, storage: &mut S) -> R;
}

/// A `StorageMap` with enumerable entries.
pub trait EnumerableStorageMap<K: codec::Codec, V: codec::Codec>: StorageMap<K, V> {
	/// Return current head element.
	fn head<S: HashedStorage<Self::Hasher>>(storage: &S) -> Option<K>;

	/// Enumerate all elements in the map.
	fn enumerate<'a, S: HashedStorage<Self::Hasher>>(
		storage: &'a S
	) -> Box<dyn Iterator<Item = (K, V)> + 'a> where K: 'a, V: 'a;
}

/// A `StorageMap` with appendable entries.
pub trait AppendableStorageMap<K: codec::Codec, V: codec::Codec>: StorageMap<K, V> {
	/// Append the given items to the value in the storage.
	///
	/// `V` is required to implement `codec::EncodeAppend`.
	fn append<'a, S, I, R>(
		key : &K,
		items: R,
		storage: &mut S,
	) -> Result<(), &'static str> where
		S: HashedStorage<Self::Hasher>,
		I: 'a + codec::Encode,
		V: codec::EncodeAppend<Item=I>,
		R: IntoIterator<Item=&'a I> + Clone,
		R::IntoIter: ExactSizeIterator,
	{
		let k = Self::key_for(key);
		let new_val = <V as codec::EncodeAppend>::append(
			storage.get_raw(&k[..]).unwrap_or_else(|| {
				// otherwise, try and read a proper __provided__ default.
				Self::Default::default().map(|v| v.encode())
					// or just use the default value.
					.unwrap_or_default()
			}),
			items,
		).map_err(|_| "Could not append given item")?;
		storage.put_raw(&k[..], &new_val);
		Ok(())
	}

	/// Safely append the given items to the value in the storage. If a codec error occurs, then the
	/// old (presumably corrupt) value is replaced with the given `items`.
	///
	/// `T` is required to implement `codec::EncodeAppend`.
	fn append_or_insert<'a, S, I, R>(
		key : &K,
		items: R,
		storage: &mut S,
	) where
		S: HashedStorage<Self::Hasher>,
		I: 'a + codec::Encode + Clone,
		V: codec::EncodeAppend<Item=I> + crate::rstd::iter::FromIterator<I>,
		R: IntoIterator<Item=&'a I> + Clone,
		R::IntoIter: ExactSizeIterator,
	{
		Self::append(key, items.clone(), storage)
			.unwrap_or_else(|_| Self::insert(key, &items.into_iter().cloned().collect(), storage));
	}
}

/// A storage map with a decodable length.
pub trait DecodeLengthStorageMap<K: codec::Codec, V: codec::Codec>: StorageMap<K, V> {
	/// Read the length of the value in a fast way, without decoding the entire value.
	///
	/// `T` is required to implement `Codec::DecodeLength`.
	///
	/// Note that `0` is returned as the default value if no encoded value exists at the given key.
	/// Therefore, this function cannot be used as a sign of _existence_. use the `::exists()`
	/// function for this purpose.
	fn decode_len<S: HashedStorage<Self::Hasher>>(key: &K, storage: &mut S) -> Result<usize, &'static str>
		where V: codec::DecodeLength, V: Len
	{
		let k = Self::key_for(key);
		if let Some(v) = storage.get_raw(&k[..]) {
			<V as codec::DecodeLength>::len(&v).map_err(|e| e.what())
		} else {
			Ok(Self::Default::default().map(|v| v.len()).unwrap_or(0))
		}
	}
}
