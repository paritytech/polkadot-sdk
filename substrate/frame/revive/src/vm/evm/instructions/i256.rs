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
use revm::primitives::U256;

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

#[cfg(test)]
/// The maximum positive value for a 256-bit signed integer.
pub const MAX_POSITIVE_VALUE: U256 = U256::from_limbs([
	0xffffffffffffffff,
	0xffffffffffffffff,
	0xffffffffffffffff,
	0x7fffffffffffffff,
]);

/// The minimum negative value for a 256-bit signed integer.
pub const MIN_NEGATIVE_VALUE: U256 = U256::from_limbs([
	0x0000000000000000,
	0x0000000000000000,
	0x0000000000000000,
	0x8000000000000000,
]);

const FLIPH_BITMASK_U64: u64 = 0x7FFF_FFFF_FFFF_FFFF;

/// Determines the sign of a 256-bit signed integer.
#[inline]
pub fn i256_sign(val: &U256) -> Sign {
	if val.bit(U256::BITS - 1) {
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
	// SAFETY: U256 does not have any padding bytes
	unsafe {
		val.as_limbs_mut()[3] &= FLIPH_BITMASK_U64;
	}
}

/// Computes the two's complement of a U256 value in place.
#[inline]
pub fn two_compl_mut(op: &mut U256) {
	*op = two_compl(*op);
}

/// Computes the two's complement of a U256 value.
#[inline]
pub fn two_compl(op: U256) -> U256 {
	op.wrapping_neg()
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
		return U256::ZERO;
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
		return U256::ZERO;
	}

	let second_sign = i256_sign_compl(&mut second);
	if second_sign == Sign::Zero {
		return U256::ZERO;
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
	use core::num::Wrapping;
	use revm::primitives::uint;

	#[test]
	fn div_i256() {
		// Sanity checks based on i8. Notice that we need to use `Wrapping` here because
		// Rust will prevent the overflow by default whereas the EVM does not.
		assert_eq!(Wrapping(i8::MIN) / Wrapping(-1), Wrapping(i8::MIN));
		assert_eq!(i8::MAX / -1, -i8::MAX);

		uint! {
			assert_eq!(i256_div(MIN_NEGATIVE_VALUE, -1_U256), MIN_NEGATIVE_VALUE);
			assert_eq!(i256_div(MIN_NEGATIVE_VALUE, 1_U256), MIN_NEGATIVE_VALUE);
			assert_eq!(i256_div(MAX_POSITIVE_VALUE, 1_U256), MAX_POSITIVE_VALUE);
			assert_eq!(i256_div(MAX_POSITIVE_VALUE, -1_U256), -1_U256 * MAX_POSITIVE_VALUE);
			assert_eq!(i256_div(100_U256, -1_U256), -100_U256);
			assert_eq!(i256_div(100_U256, 2_U256), 50_U256);
		}
	}
	#[test]
	fn test_i256_sign() {
		uint! {
			assert_eq!(i256_sign(&0_U256), Sign::Zero);
			assert_eq!(i256_sign(&1_U256), Sign::Plus);
			assert_eq!(i256_sign(&-1_U256), Sign::Minus);
			assert_eq!(i256_sign(&MIN_NEGATIVE_VALUE), Sign::Minus);
			assert_eq!(i256_sign(&MAX_POSITIVE_VALUE), Sign::Plus);
		}
	}

	#[test]
	fn test_i256_sign_compl() {
		uint! {
			let mut zero = 0_U256;
			let mut positive = 1_U256;
			let mut negative = -1_U256;
			assert_eq!(i256_sign_compl(&mut zero), Sign::Zero);
			assert_eq!(i256_sign_compl(&mut positive), Sign::Plus);
			assert_eq!(i256_sign_compl(&mut negative), Sign::Minus);
		}
	}

	#[test]
	fn test_two_compl() {
		uint! {
			assert_eq!(two_compl(0_U256), 0_U256);
			assert_eq!(two_compl(1_U256), -1_U256);
			assert_eq!(two_compl(-1_U256), 1_U256);
			assert_eq!(two_compl(2_U256), -2_U256);
			assert_eq!(two_compl(-2_U256), 2_U256);

			// Two's complement of the min value is itself.
			assert_eq!(two_compl(MIN_NEGATIVE_VALUE), MIN_NEGATIVE_VALUE);
		}
	}

	#[test]
	fn test_two_compl_mut() {
		uint! {
			let mut value = 1_U256;
			two_compl_mut(&mut value);
			assert_eq!(value, -1_U256);
		}
	}

	#[test]
	fn test_i256_cmp() {
		uint! {
			assert_eq!(i256_cmp(&1_U256, &2_U256), Ordering::Less);
			assert_eq!(i256_cmp(&2_U256, &2_U256), Ordering::Equal);
			assert_eq!(i256_cmp(&3_U256, &2_U256), Ordering::Greater);
			assert_eq!(i256_cmp(&-1_U256, &-1_U256), Ordering::Equal);
			assert_eq!(i256_cmp(&-1_U256, &-2_U256), Ordering::Greater);
			assert_eq!(i256_cmp(&-1_U256, &0_U256), Ordering::Less);
			assert_eq!(i256_cmp(&-2_U256, &2_U256), Ordering::Less);
		}
	}

	#[test]
	fn test_i256_div() {
		uint! {
			assert_eq!(i256_div(1_U256, 0_U256), 0_U256);
			assert_eq!(i256_div(0_U256, 1_U256), 0_U256);
			assert_eq!(i256_div(0_U256, -1_U256), 0_U256);
			assert_eq!(i256_div(MIN_NEGATIVE_VALUE, 1_U256), MIN_NEGATIVE_VALUE);
			assert_eq!(i256_div(4_U256, 2_U256), 2_U256);
			assert_eq!(i256_div(MIN_NEGATIVE_VALUE, MIN_NEGATIVE_VALUE), 1_U256);
			assert_eq!(i256_div(2_U256, -1_U256), -2_U256);
			assert_eq!(i256_div(-2_U256, -1_U256), 2_U256);
		}
	}

	#[test]
	fn test_i256_mod() {
		uint! {
			assert_eq!(i256_mod(0_U256, 1_U256), 0_U256);
			assert_eq!(i256_mod(1_U256, 0_U256), 0_U256);
			assert_eq!(i256_mod(4_U256, 2_U256), 0_U256);
			assert_eq!(i256_mod(3_U256, 2_U256), 1_U256);
			assert_eq!(i256_mod(MIN_NEGATIVE_VALUE, 1_U256), 0_U256);
			assert_eq!(i256_mod(2_U256, 2_U256), 0_U256);
			assert_eq!(i256_mod(2_U256, 3_U256), 2_U256);
			assert_eq!(i256_mod(-2_U256, 3_U256), -2_U256);
			assert_eq!(i256_mod(2_U256, -3_U256), 2_U256);
			assert_eq!(i256_mod(-2_U256, -3_U256), -2_U256);
		}
	}
}
