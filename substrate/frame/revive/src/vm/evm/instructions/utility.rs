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

use revm::primitives::{Address, B256, U256};

/// Pushes an arbitrary length slice of bytes onto the stack, padding the last word with zeros
/// if necessary.
///
/// # Panics
///
/// Panics if slice is longer than 32 bytes.
#[inline]
pub fn cast_slice_to_u256(slice: &[u8], dest: &mut U256) {
	if slice.is_empty() {
		return;
	}
	assert!(slice.len() <= 32, "slice too long");

	let n_words = slice.len().div_ceil(32);

	// SAFETY: Length checked above.
	unsafe {
		//let dst = self.data.as_mut_ptr().add(self.data.len()).cast::<u64>();
		//self.data.set_len(new_len);
		let dst = dest.as_limbs_mut().as_mut_ptr();

		let mut i = 0;

		// Write full words
		let words = slice.chunks_exact(32);
		let partial_last_word = words.remainder();
		for word in words {
			// Note: We unroll `U256::from_be_bytes` here to write directly into the buffer,
			// instead of creating a 32 byte array on the stack and then copying it over.
			for l in word.rchunks_exact(8) {
				dst.add(i).write(u64::from_be_bytes(l.try_into().unwrap()));
				i += 1;
			}
		}

		if partial_last_word.is_empty() {
			return;
		}

		// Write limbs of partial last word
		let limbs = partial_last_word.rchunks_exact(8);
		let partial_last_limb = limbs.remainder();
		for l in limbs {
			dst.add(i).write(u64::from_be_bytes(l.try_into().unwrap()));
			i += 1;
		}

		// Write partial last limb by padding with zeros
		if !partial_last_limb.is_empty() {
			let mut tmp = [0u8; 8];
			tmp[8 - partial_last_limb.len()..].copy_from_slice(partial_last_limb);
			dst.add(i).write(u64::from_be_bytes(tmp));
			i += 1;
		}

		debug_assert_eq!(i.div_ceil(4), n_words, "wrote too much");

		// Zero out upper bytes of last word
		let m = i % 4; // 32 / 8
		if m != 0 {
			dst.add(i).write_bytes(0, 4 - m);
		}
	}
}

/// Trait for converting types into U256 values.
pub trait IntoU256 {
	/// Converts the implementing type into a U256 value.
	fn into_u256(self) -> U256;
}

impl IntoU256 for Address {
	fn into_u256(self) -> U256 {
		self.into_word().into_u256()
	}
}

impl IntoU256 for B256 {
	fn into_u256(self) -> U256 {
		U256::from_be_bytes(self.0)
	}
}

/// Trait for converting types into Address values.
pub trait IntoAddress {
	/// Converts the implementing type into an Address value.
	fn into_address(self) -> Address;
}

impl IntoAddress for U256 {
	fn into_address(self) -> Address {
		Address::from_word(B256::from(self.to_be_bytes()))
	}
}

#[cfg(test)]
mod tests {
	use revm::primitives::address;

	use super::*;

	#[test]
	fn test_into_u256() {
		let addr = address!("0x0000000000000000000000000000000000000001");
		let u256 = addr.into_u256();
		assert_eq!(u256, U256::from(0x01));
		assert_eq!(u256.into_address(), addr);
	}
}
