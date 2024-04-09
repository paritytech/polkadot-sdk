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

use core::{fmt::Display, marker::PhantomData, ops::{Add, AddAssign, BitAnd, BitOr, BitXor, Deref, Div, DivAssign, Mul, MulAssign, Not, Rem, RemAssign, Shl, Shr, Sub, SubAssign}};
use codec::{Compact, CompactAs, Decode, Encode, MaxEncodedLen};
use num_traits::{CheckedAdd, CheckedDiv, CheckedMul, CheckedNeg, CheckedRem, CheckedShl, CheckedShr, CheckedSub, Num, NumCast, PrimInt, Saturating, ToPrimitive};
use scale_info::TypeInfo;
use sp_core::Get;
use crate::traits::{Bounded, One, Zero};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Encode, Decode, TypeInfo, Debug, MaxEncodedLen)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct NonceWithDefault<D: Get<u64>>(
    u64,
    PhantomData<D>,
);

impl<D: Get<u64>> NonceWithDefault<D> {
    pub fn new(value: u64) -> Self {
        Self(value, PhantomData)
    }
}

impl<D: Get<u64>> Clone for NonceWithDefault<D> {
    fn clone(&self) -> Self {
        Self(self.0, PhantomData)
    }
}

impl<D: Get<u64>> Copy for NonceWithDefault<D> {}

impl<D: Get<u64>> PartialEq for NonceWithDefault<D> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<D: Get<u64>> Eq for NonceWithDefault<D> {}

impl<D: Get<u64>> PartialOrd for NonceWithDefault<D> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl<D: Get<u64>> Ord for NonceWithDefault<D> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl<D: Get<u64>> Deref for NonceWithDefault<D> {
	type Target = u64;
	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl<D: Get<u64>> Default for NonceWithDefault<D> {
	fn default() -> Self {
		// Self::new(System::block_number())
        Self::new(D::get())
	}
}
impl<D: Get<u64>> From<u32> for NonceWithDefault<D> {
	fn from(value: u32) -> Self {
		Self::new(value as u64)
	}
}
impl<D: Get<u64>> From<u16> for NonceWithDefault<D> {
	fn from(value: u16) -> Self {
		Self::new(value as u64)
	}
}
impl<D: Get<u64>> CheckedNeg for NonceWithDefault<D> {
	fn checked_neg(&self) -> Option<Self> {
		self.0.checked_neg().map(Self::new)
	}
}
impl<D: Get<u64>> CheckedRem for NonceWithDefault<D> {
	fn checked_rem(&self, rhs: &Self) -> Option<Self> {
		self.0.checked_rem(rhs.0).map(Self::new)
	}
}

impl<D: Get<u64>> CheckedShr for NonceWithDefault<D> {
	fn checked_shr(&self, n: u32) -> Option<Self> {
		self.0.checked_shr(n).map(Self::new)
	}
}

impl<D: Get<u64>> CheckedShl for NonceWithDefault<D> {
	fn checked_shl(&self, n: u32) -> Option<Self> {
		self.0.checked_shl(n).map(Self::new)
	}
}

impl<D: Get<u64>> Rem for NonceWithDefault<D> {
	type Output = Self;
	fn rem(self, rhs: Self) -> Self {
		Self::new(self.0 % rhs.0)
	}
}

impl<D: Get<u64>> Rem<u32> for NonceWithDefault<D> {
	type Output = Self;
	fn rem(self, rhs: u32) -> Self {
		Self::new(self.0 % (rhs as u64))
	}
}

impl<D: Get<u64>> Shr<u32> for NonceWithDefault<D> {
	type Output = Self;
	fn shr(self, rhs: u32) -> Self {
		Self::new(self.0 >> rhs)
	}
}

impl<D: Get<u64>> Shr<usize> for NonceWithDefault<D> {
	type Output = Self;
	fn shr(self, rhs: usize) -> Self {
		Self::new(self.0 >> rhs)
	}
}

impl<D: Get<u64>> Shl<u32> for NonceWithDefault<D> {
	type Output = Self;
	fn shl(self, rhs: u32) -> Self {
		Self::new(self.0 << rhs)
	}
}

impl<D: Get<u64>> Shl<usize> for NonceWithDefault<D> {
	type Output = Self;
	fn shl(self, rhs: usize) -> Self {
		Self::new(self.0 << rhs)
	}
}

impl<D: Get<u64>> RemAssign for NonceWithDefault<D> {
	fn rem_assign(&mut self, rhs: Self) {
		self.0 %= rhs.0
	}
}

impl<D: Get<u64>> DivAssign for NonceWithDefault<D> {
	fn div_assign(&mut self, rhs: Self) {
		self.0 /= rhs.0
	}
}

impl<D: Get<u64>> MulAssign for NonceWithDefault<D> {
	fn mul_assign(&mut self, rhs: Self) {
		self.0 *= rhs.0
	}
}

impl<D: Get<u64>> SubAssign for NonceWithDefault<D> {
	fn sub_assign(&mut self, rhs: Self) {
		self.0 -= rhs.0
	}
}

impl<D: Get<u64>> AddAssign for NonceWithDefault<D> {
	fn add_assign(&mut self, rhs: Self) {
		self.0 += rhs.0
	}
}

impl<D: Get<u64>> From<u8> for NonceWithDefault<D> {
	fn from(value: u8) -> Self {
		Self::new(value as u64)
	}
}

impl<D: Get<u64>> Display for NonceWithDefault<D> {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl<D: Get<u64>> From<NonceWithDefault<D>> for u32 {
	fn from(n: NonceWithDefault<D>) -> u32 {
		n.0 as u32
	}
}

impl<D: Get<u64>> From<NonceWithDefault<D>> for u16 {
	fn from(n: NonceWithDefault<D>) -> u16 {
		n.0 as u16
	}
}

impl<D: Get<u64>> From<NonceWithDefault<D>> for u128 {
	fn from(n: NonceWithDefault<D>) -> u128 {
		n.0 as u128
	}
}

impl<D: Get<u64>> From<NonceWithDefault<D>> for usize {
	fn from(n: NonceWithDefault<D>) -> usize {
		n.0 as usize
	}
}

impl<D: Get<u64>> From<u64> for NonceWithDefault<D> {
	fn from(n: u64) -> NonceWithDefault<D> {
		Self::new(n)
	}
}

impl<D: Get<u64>> From<u128> for NonceWithDefault<D> {
	fn from(n: u128) -> NonceWithDefault<D> {
		Self::new(n as u64)
	}
}

impl<D: Get<u64>> From<usize> for NonceWithDefault<D> {
	fn from(n: usize) -> NonceWithDefault<D> {
		Self::new(n as u64)
	}
}

impl<D: Get<u64>> From<NonceWithDefault<D>> for u8 {
	fn from(n: NonceWithDefault<D>) -> u8 {
		n.0 as u8
	}
}

impl<D: Get<u64>> From<NonceWithDefault<D>> for u64 {
	fn from(n: NonceWithDefault<D>) -> u64 {
		n.0
	}
}

impl<D: Get<u64>> Zero for NonceWithDefault<D> {
	fn zero() -> Self {
		Self::new(0)
	}

	fn is_zero(&self) -> bool {
		self.0 == 0
	}
}

impl<D: Get<u64>> Bounded for NonceWithDefault<D> {
	fn min_value() -> Self {
		Self::new(u64::min_value())
	}

	fn max_value() -> Self {
		Self::new(u64::max_value())
	}
}

impl<D: Get<u64>> PrimInt for NonceWithDefault<D> {
	fn count_ones(self) -> u32 {
		self.0.count_ones()
	}

	fn leading_zeros(self) -> u32 {
		self.0.leading_zeros()
	}

	fn trailing_zeros(self) -> u32 {
		self.0.trailing_zeros()
	}

	fn rotate_left(self, n: u32) -> Self {
		Self::new(self.0.rotate_left(n))
	}

	fn rotate_right(self, n: u32) -> Self {
		Self::new(self.0.rotate_right(n))
	}

	fn swap_bytes(self) -> Self {
		Self::new(self.0.swap_bytes())
	}

	fn from_be(x: Self) -> Self {
		Self::new(u64::from_be(x.0))
	}

	fn from_le(x: Self) -> Self {
		Self::new(u64::from_le(x.0))
	}

	fn to_be(self) -> Self {
		Self::new(self.0.to_be())
	}

	fn to_le(self) -> Self {
		Self::new(self.0.to_le())
	}

	fn count_zeros(self) -> u32 {
		self.0.count_zeros()
	}
	
	fn signed_shl(self, n: u32) -> Self {
		Self::new(self.0.wrapping_shl(n))
	}

	fn signed_shr(self, n: u32) -> Self {
		Self::new(self.0.wrapping_shr(n))
	}

	fn unsigned_shl(self, n: u32) -> Self {
		Self::new(self.0.wrapping_shl(n))
	}

	fn unsigned_shr(self, n: u32) -> Self {
		Self::new(self.0.wrapping_shr(n))
	}

	fn pow(self, exp: u32) -> Self {
		Self::new(self.0.pow(exp))
	}
}

impl<D: Get<u64>> Saturating for NonceWithDefault<D> {
	fn saturating_add(self, rhs: Self) -> Self {
		Self::new(self.0.saturating_add(rhs.0))
	}

	fn saturating_sub(self, rhs: Self) -> Self {
		Self::new(self.0.saturating_sub(rhs.0))
	}
}

impl<D: Get<u64>> Div for NonceWithDefault<D> {
	type Output = Self;
	fn div(self, rhs: Self) -> Self {
		Self::new(self.0 / rhs.0)
	}
}

impl<D: Get<u64>> Mul for NonceWithDefault<D> {
	type Output = Self;
	fn mul(self, rhs: Self) -> Self {
		Self::new(self.0 * rhs.0)
	}
}

impl<D: Get<u64>> CheckedDiv for NonceWithDefault<D> {
	fn checked_div(&self, rhs: &Self) -> Option<Self> {
		self.0.checked_div(rhs.0).map(Self::new)
	}
}

impl<D: Get<u64>> CheckedMul for NonceWithDefault<D> {
	fn checked_mul(&self, rhs: &Self) -> Option<Self> {
		self.0.checked_mul(rhs.0).map(Self::new)
	}
}

impl<D: Get<u64>> Sub for NonceWithDefault<D> {
	type Output = Self;
	fn sub(self, rhs: Self) -> Self {
		Self::new(self.0 - rhs.0)
	}
}

impl<D: Get<u64>> CheckedSub for NonceWithDefault<D> {
	fn checked_sub(&self, rhs: &Self) -> Option<Self> {
		self.0.checked_sub(rhs.0).map(Self::new)
	}
}

impl<D: Get<u64>> Add for NonceWithDefault<D> {
	type Output = Self;
	fn add(self, rhs: Self) -> Self {
		Self::new(self.0 + rhs.0)
	}
}

impl<D: Get<u64>> CheckedAdd for NonceWithDefault<D> {
	fn checked_add(&self, rhs: &Self) -> Option<Self> {
		self.0.checked_add(rhs.0).map(Self::new)
	}
}

impl<D: Get<u64>> BitAnd for NonceWithDefault<D> {
	type Output = Self;
	fn bitand(self, rhs: Self) -> Self {
		Self::new(self.0 & rhs.0)
	}
}

impl<D: Get<u64>> BitOr for NonceWithDefault<D> {
	type Output = Self;
	fn bitor(self, rhs: Self) -> Self {
		Self::new(self.0 | rhs.0)
	}
}

impl<D: Get<u64>> BitXor for NonceWithDefault<D> {
	type Output = Self;
	fn bitxor(self, rhs: Self) -> Self {
		Self::new(self.0 ^ rhs.0)
	}
}

impl<D: Get<u64>> One for NonceWithDefault<D> {
	fn one() -> Self {
		Self::new(1)
	}
}

impl<D: Get<u64>> Not for NonceWithDefault<D> {
	type Output = Self;
	fn not(self) -> Self {
		Self::new(!self.0)
	}
}

impl<D: Get<u64>> NumCast for NonceWithDefault<D> {
	fn from<T: ToPrimitive>(n: T) -> Option<Self> {
		n.to_u64().map(Self::new)
	}
}

impl<D: Get<u64>> Num for NonceWithDefault<D> {
	type FromStrRadixErr = <u64 as Num>::FromStrRadixErr;

	fn from_str_radix(s: &str, radix: u32) -> Result<Self, Self::FromStrRadixErr> {
		u64::from_str_radix(s, radix).map(Self::new)
	}
}

impl<D: Get<u64>> ToPrimitive for NonceWithDefault<D> {
	fn to_i64(&self) -> Option<i64> {
		self.0.to_i64()
	}

	fn to_u64(&self) -> Option<u64> {
		Some(self.0)
	}

	fn to_i128(&self) -> Option<i128> {
		self.0.to_i128()
	}

	fn to_u128(&self) -> Option<u128> {
		Some(self.0 as u128)
	}
}

impl<D: Get<u64>> From<Compact<NonceWithDefault<D>>> for NonceWithDefault<D> {
	fn from(c: Compact<NonceWithDefault<D>>) -> Self {
		c.0
	}
}

impl<D: Get<u64>> CompactAs for NonceWithDefault<D> {
	type As = u64;

	fn encode_as(&self) -> &Self::As {
		&self.0
	}

	fn decode_from(val: Self::As) -> Result<Self, codec::Error> {
		Ok(Self::new(val))
	}
}
