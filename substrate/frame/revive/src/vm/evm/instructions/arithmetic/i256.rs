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

use core::cmp::Ordering;
use sp_core::U256;

/// Represents the sign of a 256-bit signed integer value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(i8)]
pub enum Sign {
	// Same as `cmp::Ordering`
	/// Negative value sign
	Minus = -1,
	/// Zero value sign
	Zero = 0,
	#[allow(dead_code)] // "constructed" with `mem::transmute` in `i256_sign` below
	/// Positive value sign
	Plus = 1,
}

/// The minimum negative value for a 256-bit signed integer.
pub const MIN_NEGATIVE_VALUE: U256 =
	U256([0x0000000000000000, 0x0000000000000000, 0x0000000000000000, 0x8000000000000000]);

const FLIPH_BITMASK_U64: u64 = 0x7FFF_FFFF_FFFF_FFFF;

/// Determines the sign of a 256-bit signed integer.
#[inline]
pub fn i256_sign(val: &U256) -> Sign {
	if val.bit(255) {
		Sign::Minus
	} else {
		// SAFETY: false == 0 == Zero, true == 1 == Plus
		unsafe { core::mem::transmute::<bool, Sign>(!val.is_zero()) }
	}
}

/// Determines the sign of a 256-bit signed integer and converts it to its absolute value.
#[inline]
pub fn i256_sign_compl(val: &mut U256) -> Sign {
	let sign = i256_sign(val);
	if sign == Sign::Minus {
		two_compl_mut(val);
	}
	sign
}

#[inline]
fn u256_remove_sign(val: &mut U256) {
	// Clear the sign bit by masking the highest bit
	let limbs = val.0;
	*val = U256([limbs[0], limbs[1], limbs[2], limbs[3] & FLIPH_BITMASK_U64]);
}

/// Computes the two's complement of a U256 value in place.
#[inline]
pub fn two_compl_mut(op: &mut U256) {
	*op = two_compl(*op);
}

/// Computes the two's complement of a U256 value.
#[inline]
pub fn two_compl(op: U256) -> U256 {
	(!op).overflowing_add(U256::from(1)).0
}

/// Compares two 256-bit signed integers.
#[inline]
pub fn i256_cmp(first: &U256, second: &U256) -> Ordering {
	let first_sign = i256_sign(first);
	let second_sign = i256_sign(second);
	match first_sign.cmp(&second_sign) {
		// Note: Adding `if first_sign != Sign::Zero` to short circuit zero comparisons performs
		// slower on average, as of #582
		Ordering::Equal => first.cmp(second),
		o => o,
	}
}

/// Performs signed division of two 256-bit integers.
#[inline]
pub fn i256_div(mut first: U256, mut second: U256) -> U256 {
	let second_sign = i256_sign_compl(&mut second);
	if second_sign == Sign::Zero {
		return U256::zero();
	}

	let first_sign = i256_sign_compl(&mut first);
	if first == MIN_NEGATIVE_VALUE && second == U256::from(1) {
		return two_compl(MIN_NEGATIVE_VALUE);
	}

	// Necessary overflow checks are done above, perform the division
	let mut d = first / second;

	// Set sign bit to zero
	u256_remove_sign(&mut d);

	// Two's complement only if the signs are different
	// Note: This condition has better codegen than an exhaustive match, as of #582
	if (first_sign == Sign::Minus && second_sign != Sign::Minus) ||
		(second_sign == Sign::Minus && first_sign != Sign::Minus)
	{
		two_compl(d)
	} else {
		d
	}
}

/// Performs signed modulo of two 256-bit integers.
#[inline]
pub fn i256_mod(mut first: U256, mut second: U256) -> U256 {
	let first_sign = i256_sign_compl(&mut first);
	if first_sign == Sign::Zero {
		return U256::zero();
	}

	let second_sign = i256_sign_compl(&mut second);
	if second_sign == Sign::Zero {
		return U256::zero();
	}

	let mut r = first % second;

	// Set sign bit to zero
	u256_remove_sign(&mut r);

	if first_sign == Sign::Minus {
		two_compl(r)
	} else {
		r
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use alloy_core::primitives;
	use proptest::proptest;

	fn alloy_u256(limbs: [u64; 4]) -> primitives::U256 {
		primitives::U256::from_limbs(limbs)
	}

	#[test]
	fn test_i256_sign() {
		proptest!(|(a: [u64; 4])| {
			let ours = i256_sign(&U256(a));
			let theirs = revm::interpreter::instructions::i256::i256_sign(&alloy_u256(a));
			assert_eq!(ours as i8, theirs as i8);
		});
	}

	#[test]
	fn test_i256_sign_compl() {
		proptest!(|(a: [u64; 4])| {
			let mut ours_in = U256(a);
			let ours = i256_sign_compl(&mut ours_in);
			let mut theirs_in = alloy_u256(a);
			let theirs = revm::interpreter::instructions::i256::i256_sign_compl(&mut theirs_in);
			assert_eq!(&ours_in.0, theirs_in.as_limbs());
			assert_eq!(ours as u8, theirs as u8);
		});
	}

	#[test]
	fn test_two_compl() {
		proptest!(|(a: [u64; 4])| {
			let ours = two_compl(U256(a));
			let theirs = revm::interpreter::instructions::i256::two_compl(alloy_u256(a));
			assert_eq!(&ours.0, theirs.as_limbs());
		});
	}

	#[test]
	fn test_two_compl_mut() {
		proptest!(|(limbs: [u64; 4])| {
			let mut ours = U256(limbs);
			two_compl_mut(&mut ours);
			let theirs_value = alloy_core::primitives::U256::from_limbs(limbs);
			let theirs = theirs_value.wrapping_neg();
			assert_eq!(&ours.0, theirs.as_limbs());
		});
	}
	#[test]
	fn test_i256_cmp() {
		proptest!(|(a: [u64; 4], b: [u64; 4])| {
			let first = U256(a);
			let second = U256(b);
			let ours = i256_cmp(&first, &second);
			let theirs = revm::interpreter::instructions::i256::i256_cmp(&alloy_u256(a), &alloy_u256(b));
			assert_eq!(ours, theirs);
		});
	}

	#[test]
	fn test_i256_div() {
		proptest!(|(a: [u64; 4], b: [u64; 4])| {
			let ours = i256_div(U256(a), U256(b));
			let theirs = revm::interpreter::instructions::i256::i256_div(alloy_u256(a), alloy_u256(b));
			assert_eq!(&ours.0, theirs.as_limbs());
		});
	}

	#[test]
	fn test_i256_mod() {
		proptest!(|(a: [u64; 4], b: [u64; 4])| {
			let ours = i256_mod(U256(a), U256(b));
			let theirs = revm::interpreter::instructions::i256::i256_mod(alloy_u256(a), alloy_u256(b));
			assert_eq!(&ours.0, theirs.as_limbs());
		});
	}
}
