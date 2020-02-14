// Copyright 2017-2020 Parity Technologies (UK) Ltd.
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

//! Some utilities for helping access storage with arbitrary key types.

use sp_std::prelude::*;
use codec::{Encode, Decode};
use crate::{StorageHasher, Twox128};

/// Utility to iterate through raw items in storage.
pub struct StorageIterator<T> {
	prefix: [u8; 32],
	previous_key: Vec<u8>,
	drain: bool,
	_phantom: ::sp_std::marker::PhantomData<T>,
}

impl<T> StorageIterator<T> {
	/// Construct iterator to iterate over map items in `module` for the map called `item`.
	pub fn new(module: &[u8], item: &[u8]) -> Self {
		let mut prefix = [0u8; 32];
		prefix[0..16].copy_from_slice(&Twox128::hash(module));
		prefix[16..32].copy_from_slice(&Twox128::hash(item));
		Self { prefix, previous_key: prefix[..].to_vec(), drain: false, _phantom: Default::default() }
	}
	/// Mutate this iterator into a draining iterator; items iterated are removed from storage.
	pub fn drain(mut self) -> Self {
		self.drain = true;
		self
	}
}

impl<T: Decode + Sized> Iterator for StorageIterator<T> {
	type Item = (Vec<u8>, T);

	fn next(&mut self) -> Option<(Vec<u8>, T)> {
		loop {
			let maybe_next = sp_io::storage::next_key(&self.previous_key)
				.filter(|n| n.starts_with(&self.prefix));
			break match maybe_next {
				Some(next) => {
					self.previous_key = next.clone();
					let maybe_value = frame_support::storage::unhashed::get::<T>(&next);
					match maybe_value {
						Some(value) => {
							if self.drain {
								frame_support::storage::unhashed::kill(&next);
							}
							Some((self.previous_key[32..].to_vec(), value))
						}
						None => continue,
					}
				}
				None => None,
			}
		}
	}
}

/// Get a particular value in storage by the `module`, the map's `item` name and the key `hash`.
pub fn get_storage_value<T: Decode + Sized>(module: &[u8], item: &[u8], hash: &[u8]) -> Option<T> {
	let mut key = vec![0u8; 32 + hash.len()];
	key[0..16].copy_from_slice(&Twox128::hash(module));
	key[16..32].copy_from_slice(&Twox128::hash(item));
	key[32..].copy_from_slice(hash);
	frame_support::storage::unhashed::get::<T>(&key)
}

/// Get a particular value in storage by the `module`, the map's `item` name and the key `hash`.
pub fn take_storage_value<T: Decode + Sized>(module: &[u8], item: &[u8], hash: &[u8]) -> Option<T> {
	let mut key = vec![0u8; 32 + hash.len()];
	key[0..16].copy_from_slice(&Twox128::hash(module));
	key[16..32].copy_from_slice(&Twox128::hash(item));
	key[32..].copy_from_slice(hash);
	frame_support::storage::unhashed::take::<T>(&key)
}

/// Put a particular value into storage by the `module`, the map's `item` name and the key `hash`.
pub fn put_storage_value<T: Encode>(module: &[u8], item: &[u8], hash: &[u8], value: T) {
	let mut key = vec![0u8; 32 + hash.len()];
	key[0..16].copy_from_slice(&Twox128::hash(module));
	key[16..32].copy_from_slice(&Twox128::hash(item));
	key[32..].copy_from_slice(hash);
	frame_support::storage::unhashed::put(&key, &value);
}
