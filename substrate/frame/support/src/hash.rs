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

//! Hash utilities.

use codec::Codec;
use sp_std::prelude::Vec;
use sp_io::hashing::{blake2_128, blake2_256, twox_64, twox_128, twox_256};

// This trait must be kept coherent with frame-support-procedural HasherKind usage
pub trait Hashable: Sized {
	fn blake2_128(&self) -> [u8; 16];
	fn blake2_256(&self) -> [u8; 32];
	fn blake2_128_concat(&self) -> Vec<u8>;
	fn twox_128(&self) -> [u8; 16];
	fn twox_256(&self) -> [u8; 32];
	fn twox_64_concat(&self) -> Vec<u8>;
	fn identity(&self) -> Vec<u8>;
}

impl<T: Codec> Hashable for T {
	fn blake2_128(&self) -> [u8; 16] {
		self.using_encoded(blake2_128)
	}
	fn blake2_256(&self) -> [u8; 32] {
		self.using_encoded(blake2_256)
	}
	fn blake2_128_concat(&self) -> Vec<u8> {
		self.using_encoded(Blake2_128Concat::hash)
	}
	fn twox_128(&self) -> [u8; 16] {
		self.using_encoded(twox_128)
	}
	fn twox_256(&self) -> [u8; 32] {
		self.using_encoded(twox_256)
	}
	fn twox_64_concat(&self) -> Vec<u8> {
		self.using_encoded(Twox64Concat::hash)
	}
	fn identity(&self) -> Vec<u8> { self.encode() }
}

/// Hasher to use to hash keys to insert to storage.
pub trait StorageHasher: 'static {
	type Output: AsRef<[u8]>;
	fn hash(x: &[u8]) -> Self::Output;
}

/// Hasher to use to hash keys to insert to storage.
pub trait ReversibleStorageHasher: StorageHasher {
	fn reverse(x: &[u8]) -> &[u8];
}

/// Store the key directly.
pub struct Identity;
impl StorageHasher for Identity {
	type Output = Vec<u8>;
	fn hash(x: &[u8]) -> Vec<u8> {
		x.to_vec()
	}
}
impl ReversibleStorageHasher for Identity {
	fn reverse(x: &[u8]) -> &[u8] {
		x
	}
}

/// Hash storage keys with `concat(twox64(key), key)`
pub struct Twox64Concat;
impl StorageHasher for Twox64Concat {
	type Output = Vec<u8>;
	fn hash(x: &[u8]) -> Vec<u8> {
		twox_64(x)
			.iter()
			.chain(x.into_iter())
			.cloned()
			.collect::<Vec<_>>()
	}
}
impl ReversibleStorageHasher for Twox64Concat {
	fn reverse(x: &[u8]) -> &[u8] {
		if x.len() < 8 {
			crate::debug::error!("Invalid reverse: hash length too short");
			return &[]
		}
		&x[8..]
	}
}

/// Hash storage keys with `concat(blake2_128(key), key)`
pub struct Blake2_128Concat;
impl StorageHasher for Blake2_128Concat {
	type Output = Vec<u8>;
	fn hash(x: &[u8]) -> Vec<u8> {
		blake2_128(x)
			.iter()
			.chain(x.into_iter())
			.cloned()
			.collect::<Vec<_>>()
	}
}
impl ReversibleStorageHasher for Blake2_128Concat {
	fn reverse(x: &[u8]) -> &[u8] {
		if x.len() < 16 {
			crate::debug::error!("Invalid reverse: hash length too short");
			return &[]
		}
		&x[16..]
	}
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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_twox_64_concat() {
		let r = Twox64Concat::hash(b"foo");
		assert_eq!(r.split_at(8), (&twox_128(b"foo")[..8], &b"foo"[..]))
	}

	#[test]
	fn test_blake2_128_concat() {
		let r = Blake2_128Concat::hash(b"foo");
		assert_eq!(r.split_at(16), (&blake2_128(b"foo")[..], &b"foo"[..]))
	}
}
