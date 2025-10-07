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
use alloy_core::primitives::ruint::algorithms;
use sp_core::U256;

pub trait Modular {
	/// Compute $\mod{\mathtt{self}}_{\mathtt{modulus}}$.
	///
	/// Returns zero if the modulus is zero.
	// FEATURE: Reduce larger bit-sizes to smaller ones.
	#[must_use]
	fn reduce_mod(self, modulus: Self) -> Self;

	/// Compute $\mod{\mathtt{self} + \mathtt{rhs}}_{\mathtt{modulus}}$.
	///
	/// Returns zero if the modulus is zero.
	#[must_use]
	fn add_mod(self, rhs: Self, modulus: Self) -> Self;

	/// Compute $\mod{\mathtt{self} ⋅ \mathtt{rhs}}_{\mathtt{modulus}}$.
	///
	/// Returns zero if the modulus is zero.
	///
	/// See [`mul_redc`](Self::mul_redc) for a faster variant at the cost of
	/// some pre-computation.
	#[must_use]
	fn mul_mod(self, rhs: Self, modulus: Self) -> Self;
}

impl Modular for U256 {
	fn reduce_mod(mut self, modulus: Self) -> Self {
		if modulus.is_zero() {
			return Self::zero();
		}
		if self >= modulus {
			self %= modulus;
		}
		self
	}

	fn add_mod(self, rhs: Self, modulus: Self) -> Self {
		if modulus.is_zero() {
			return Self::zero();
		}
		// Reduce inputs
		let lhs = self.reduce_mod(modulus);
		let rhs = rhs.reduce_mod(modulus);

		// Compute the sum and conditionally subtract modulus once.
		let (mut result, overflow) = lhs.overflowing_add(rhs);
		if overflow || result >= modulus {
			result = result.overflowing_sub(modulus).0;
		}
		result
	}

	fn mul_mod(self, rhs: Self, mut modulus: Self) -> Self {
		if modulus.is_zero() {
			return Self::zero();
		}

		// Allocate at least `nlimbs(2 * BITS)` limbs to store the product. This array
		// casting is a workaround for `generic_const_exprs` not being stable.
		let mut product = [[0u64; 2]; 4];
		let product_len = 8;
		// SAFETY: `[[u64; 2]; 4] = [u64; 8]`.
		let product = unsafe {
			core::slice::from_raw_parts_mut(product.as_mut_ptr().cast::<u64>(), product_len)
		};

		// Compute full product.
		let overflow = algorithms::addmul(product, &self.0, &rhs.0);
		debug_assert!(!overflow, "addmul overflowed for 256-bit inputs");

		// Compute modulus using `div_rem`.
		// This stores the remainder in the divisor, `modulus`.
		algorithms::div(product, &mut modulus.0);

		modulus
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
	fn test_reduce_mod() {
		proptest!(|(a: [u64; 4], m: [u64; 4])| {
			let ours = U256(a).reduce_mod(U256(m));
			let theirs = alloy_u256(a).reduce_mod(alloy_u256(m));
			assert_eq!(&ours.0, theirs.as_limbs());
		});
	}

	#[test]
	fn test_add_mod() {
		proptest!(|(a: [u64; 4], b: [u64; 4], m: [u64; 4])| {
			let ours = U256(a).add_mod(U256(b), U256(m));
			let theirs = alloy_u256(a).add_mod(alloy_u256(b), alloy_u256(m));
			assert_eq!(&ours.0, theirs.as_limbs());
		});
	}

	#[test]
	fn test_mul_mod() {
		proptest!(|(a: [u64; 4], b: [u64; 4], m: [u64; 4])| {
			let ours = U256(a).mul_mod(U256(b), U256(m));
			let theirs = alloy_u256(a).mul_mod(alloy_u256(b), alloy_u256(m));
			assert_eq!(&ours.0, theirs.as_limbs());
		});
	}
}
