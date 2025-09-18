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

use sp_core::U256;

pub trait Bits {
	/// Returns whether a specific bit is set.
	///
	/// Returns `false` if `index` exceeds the bit width of the number.
	#[must_use]
	fn bit(&self, index: usize) -> bool;

	/// Arithmetic shift right by `rhs` bits.
	#[must_use]
	fn arithmetic_shr(self, rhs: usize) -> Self;
}

impl Bits for U256 {
	fn bit(&self, index: usize) -> bool {
		const BITS: usize = 256;
		if index >= BITS {
			return false;
		}
		let (limbs, bits) = (index / 64, index % 64);
		self.0[limbs] & (1 << bits) != 0
	}

	fn arithmetic_shr(self, rhs: usize) -> Self {
		const BITS: usize = 256;
		if BITS == 0 {
			return Self::zero();
		}
		let sign = self.bit(BITS - 1);
		let mut r = self >> rhs;
		if sign {
			r |= U256::MAX << BITS.saturating_sub(rhs);
		}
		r
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use core::cmp::min;
	use proptest::proptest;

	// #[test]
	// fn test_arithmetic_shr() {
	// 	proptest!(|(limbs: [u64; 4], shift in 0..=258)| {
	// 		let value = U256(limbs);
	// 		let shifted = value.arithmetic_shr(shift);
	// 		let sign_bit = value.bit(255);
	// 		if sign_bit {
	// 			// For negative numbers, check that the sign is preserved
	// 			assert_eq!(shifted.leading_ones(), min(256, shift));
	// 		} else {
	// 			// For positive numbers, should behave like logical shift
	// 			assert_eq!(shifted.leading_zeros(), min(256, value.leading_zeros() + shift));
	// 		}
	// 	});
	// }

	#[test]
	fn test_arithmetic_shr_positive() {
		// Test positive number (MSB = 0)
		let value = U256::from(0x7FFFFFFFu64);
		let result = value.arithmetic_shr(4);
		let expected = U256::from(0x07FFFFFFu64);
		assert_eq!(result, expected);
	}

	#[test]
	fn test_arithmetic_shr_negative() {
		// Test negative number (MSB = 1)
		let value = U256::MAX; // All bits set
		let result = value.arithmetic_shr(4);
		// Should still be all bits set
		assert_eq!(result, U256::MAX);
	}

	#[test]
	fn test_arithmetic_shr_large_shift() {
		// Test shift larger than bit width
		let value = U256::MAX;
		let result = value.arithmetic_shr(300);
		assert_eq!(result, U256::MAX);

		let positive = U256::from(123u64);
		let result_pos = positive.arithmetic_shr(300);
		assert_eq!(result_pos, U256::zero());
	}
}
