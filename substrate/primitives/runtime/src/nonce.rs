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

use crate::traits::{Bounded, Nonce, One, Zero};
use codec::{Compact, CompactAs, Decode, Encode, MaxEncodedLen};
use core::{
	fmt::Display,
	marker::PhantomData,
	ops::{
		Add, AddAssign, BitAnd, BitOr, BitXor, Deref, Div, DivAssign, Mul, MulAssign, Not, Rem,
		RemAssign, Shl, Shr, Sub, SubAssign,
	},
};
use num_traits::{
	CheckedAdd, CheckedDiv, CheckedMul, CheckedNeg, CheckedRem, CheckedShl, CheckedShr, CheckedSub,
	Num, NumCast, PrimInt, Saturating, ToPrimitive,
};
use scale_info::TypeInfo;
use sp_core::Get;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Encode, Decode, TypeInfo, Debug, MaxEncodedLen)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct NonceWithDefault<D: Get<N>, N: Nonce>(N, PhantomData<D>);

impl<D: Get<N>, N: Nonce> NonceWithDefault<D, N> {
	pub fn new(value: N) -> Self {
		Self(value, PhantomData)
	}
}

impl<D: Get<N>, N: Nonce> Clone for NonceWithDefault<D, N> {
	fn clone(&self) -> Self {
		Self(self.0, PhantomData)
	}
}

impl<D: Get<N>, N: Nonce> Copy for NonceWithDefault<D, N> {}

impl<D: Get<N>, N: Nonce> PartialEq for NonceWithDefault<D, N> {
	fn eq(&self, other: &Self) -> bool {
		self.0 == other.0
	}
}

impl<D: Get<N>, N: Nonce> Eq for NonceWithDefault<D, N> {}

impl<D: Get<N>, N: Nonce> PartialOrd for NonceWithDefault<D, N> {
	fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
		self.0.partial_cmp(&other.0)
	}
}

impl<D: Get<N>, N: Nonce> Ord for NonceWithDefault<D, N> {
	fn cmp(&self, other: &Self) -> core::cmp::Ordering {
		self.0.cmp(&other.0)
	}
}

impl<D: Get<N>, N: Nonce> Deref for NonceWithDefault<D, N> {
	type Target = N;
	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl<D: Get<N>, N: Nonce> Default for NonceWithDefault<D, N> {
	fn default() -> Self {
		Self::new(D::get())
	}
}
impl<D: Get<N>, N: Nonce> From<u32> for NonceWithDefault<D, N> {
	fn from(value: u32) -> Self {
		Self::new(value.into())
	}
}
impl<D: Get<N>, N: Nonce> From<u16> for NonceWithDefault<D, N> {
	fn from(value: u16) -> Self {
		Self::new(value.into())
	}
}
impl<D: Get<N>, N: Nonce> CheckedNeg for NonceWithDefault<D, N> {
	fn checked_neg(&self) -> Option<Self> {
		self.0.checked_neg().map(Self::new)
	}
}
impl<D: Get<N>, N: Nonce> CheckedRem for NonceWithDefault<D, N> {
	fn checked_rem(&self, rhs: &Self) -> Option<Self> {
		self.0.checked_rem(&rhs.0).map(Self::new)
	}
}

impl<D: Get<N>, N: Nonce> CheckedShr for NonceWithDefault<D, N> {
	fn checked_shr(&self, n: u32) -> Option<Self> {
		self.0.checked_shr(n).map(Self::new)
	}
}

impl<D: Get<N>, N: Nonce> CheckedShl for NonceWithDefault<D, N> {
	fn checked_shl(&self, n: u32) -> Option<Self> {
		self.0.checked_shl(n).map(Self::new)
	}
}

impl<D: Get<N>, N: Nonce> Rem for NonceWithDefault<D, N> {
	type Output = Self;
	fn rem(self, rhs: Self) -> Self {
		Self::new(self.0 % rhs.0)
	}
}

impl<D: Get<N>, N: Nonce> Rem<u32> for NonceWithDefault<D, N> {
	type Output = Self;
	fn rem(self, rhs: u32) -> Self {
		Self::new(self.0 % (rhs.into()))
	}
}

impl<D: Get<N>, N: Nonce> Shr<u32> for NonceWithDefault<D, N> {
	type Output = Self;
	fn shr(self, rhs: u32) -> Self {
		Self::new(self.0 >> rhs)
	}
}

impl<D: Get<N>, N: Nonce> Shr<usize> for NonceWithDefault<D, N> {
	type Output = Self;
	fn shr(self, rhs: usize) -> Self {
		Self::new(self.0 >> rhs)
	}
}

impl<D: Get<N>, N: Nonce> Shl<u32> for NonceWithDefault<D, N> {
	type Output = Self;
	fn shl(self, rhs: u32) -> Self {
		Self::new(self.0 << rhs)
	}
}

impl<D: Get<N>, N: Nonce> Shl<usize> for NonceWithDefault<D, N> {
	type Output = Self;
	fn shl(self, rhs: usize) -> Self {
		Self::new(self.0 << rhs)
	}
}

impl<D: Get<N>, N: Nonce> RemAssign for NonceWithDefault<D, N> {
	fn rem_assign(&mut self, rhs: Self) {
		self.0 %= rhs.0
	}
}

impl<D: Get<N>, N: Nonce> DivAssign for NonceWithDefault<D, N> {
	fn div_assign(&mut self, rhs: Self) {
		self.0 /= rhs.0
	}
}

impl<D: Get<N>, N: Nonce> MulAssign for NonceWithDefault<D, N> {
	fn mul_assign(&mut self, rhs: Self) {
		self.0 *= rhs.0
	}
}

impl<D: Get<N>, N: Nonce> SubAssign for NonceWithDefault<D, N> {
	fn sub_assign(&mut self, rhs: Self) {
		self.0 -= rhs.0
	}
}

impl<D: Get<N>, N: Nonce> AddAssign for NonceWithDefault<D, N> {
	fn add_assign(&mut self, rhs: Self) {
		self.0 += rhs.0
	}
}

impl<D: Get<N>, N: Nonce> From<u8> for NonceWithDefault<D, N> {
	fn from(value: u8) -> Self {
		Self::new(value.into())
	}
}

impl<D: Get<N>, N: Nonce> Display for NonceWithDefault<D, N> {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl<D: Get<N>, N: Nonce> TryFrom<u64> for NonceWithDefault<D, N> {
	type Error = <N as TryFrom<u64>>::Error;
	fn try_from(n: u64) -> Result<NonceWithDefault<D, N>, Self::Error> {
		N::try_from(n).map(Self::new)
	}
}

impl<D: Get<N>, N: Nonce> TryFrom<u128> for NonceWithDefault<D, N> {
	type Error = <N as TryFrom<u128>>::Error;
	fn try_from(n: u128) -> Result<NonceWithDefault<D, N>, Self::Error> {
		N::try_from(n).map(Self::new)
	}
}

impl<D: Get<N>, N: Nonce> TryFrom<usize> for NonceWithDefault<D, N> {
	type Error = <N as TryFrom<usize>>::Error;
	fn try_from(n: usize) -> Result<NonceWithDefault<D, N>, Self::Error> {
		N::try_from(n).map(Self::new)
	}
}

impl<D: Get<N>, N: Nonce> TryFrom<NonceWithDefault<D, N>> for u8 {
	type Error = <N as TryInto<u8>>::Error;
	fn try_from(value: NonceWithDefault<D, N>) -> Result<Self, Self::Error> {
		value.0.try_into()
	}
}

impl<D: Get<N>, N: Nonce> TryFrom<NonceWithDefault<D, N>> for u16 {
	type Error = <N as TryInto<u16>>::Error;
	fn try_from(value: NonceWithDefault<D, N>) -> Result<Self, Self::Error> {
		value.0.try_into()
	}
}

impl<D: Get<N>, N: Nonce> TryFrom<NonceWithDefault<D, N>> for u32 {
	type Error = <N as TryInto<u32>>::Error;
	fn try_from(value: NonceWithDefault<D, N>) -> Result<Self, Self::Error> {
		value.0.try_into()
	}
}

impl<D: Get<N>, N: Nonce> TryFrom<NonceWithDefault<D, N>> for u64 {
	type Error = <N as TryInto<u64>>::Error;
	fn try_from(value: NonceWithDefault<D, N>) -> Result<Self, Self::Error> {
		value.0.try_into()
	}
}

impl<D: Get<N>, N: Nonce> TryFrom<NonceWithDefault<D, N>> for u128 {
	type Error = <N as TryInto<u128>>::Error;
	fn try_from(value: NonceWithDefault<D, N>) -> Result<Self, Self::Error> {
		value.0.try_into()
	}
}

impl<D: Get<N>, N: Nonce> TryFrom<NonceWithDefault<D, N>> for usize {
	type Error = <N as TryInto<usize>>::Error;
	fn try_from(value: NonceWithDefault<D, N>) -> Result<Self, Self::Error> {
		value.0.try_into()
	}
}

impl<D: Get<N>, N: Nonce> Zero for NonceWithDefault<D, N> {
	fn zero() -> Self {
		Self::new(N::zero())
	}

	fn is_zero(&self) -> bool {
		self.0 == N::zero()
	}
}

impl<D: Get<N>, N: Nonce> Bounded for NonceWithDefault<D, N> {
	fn min_value() -> Self {
		Self::new(N::min_value())
	}

	fn max_value() -> Self {
		Self::new(N::max_value())
	}
}

impl<D: Get<N>, N: Nonce> PrimInt for NonceWithDefault<D, N> {
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
		Self::new(N::from_be(x.0))
	}

	fn from_le(x: Self) -> Self {
		Self::new(N::from_le(x.0))
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
		Self::new(self.0.signed_shl(n))
	}

	fn signed_shr(self, n: u32) -> Self {
		Self::new(self.0.signed_shr(n))
	}

	fn unsigned_shl(self, n: u32) -> Self {
		Self::new(self.0.unsigned_shl(n))
	}

	fn unsigned_shr(self, n: u32) -> Self {
		Self::new(self.0.unsigned_shr(n))
	}

	fn pow(self, exp: u32) -> Self {
		Self::new(self.0.pow(exp))
	}
}

impl<D: Get<N>, N: Nonce> Saturating for NonceWithDefault<D, N> {
	fn saturating_add(self, rhs: Self) -> Self {
		Self::new(<N as Saturating>::saturating_add(*self, rhs.0))
	}

	fn saturating_sub(self, rhs: Self) -> Self {
		Self::new(<N as Saturating>::saturating_sub(*self, rhs.0))
	}
}

impl<D: Get<N>, N: Nonce> Div for NonceWithDefault<D, N> {
	type Output = Self;
	fn div(self, rhs: Self) -> Self {
		Self::new(self.0 / rhs.0)
	}
}

impl<D: Get<N>, N: Nonce> Mul for NonceWithDefault<D, N> {
	type Output = Self;
	fn mul(self, rhs: Self) -> Self {
		Self::new(self.0 * rhs.0)
	}
}

impl<D: Get<N>, N: Nonce> CheckedDiv for NonceWithDefault<D, N> {
	fn checked_div(&self, rhs: &Self) -> Option<Self> {
		self.0.checked_div(&rhs.0).map(Self::new)
	}
}

impl<D: Get<N>, N: Nonce> CheckedMul for NonceWithDefault<D, N> {
	fn checked_mul(&self, rhs: &Self) -> Option<Self> {
		self.0.checked_mul(&rhs.0).map(Self::new)
	}
}

impl<D: Get<N>, N: Nonce> Sub for NonceWithDefault<D, N> {
	type Output = Self;
	fn sub(self, rhs: Self) -> Self {
		Self::new(self.0 - rhs.0)
	}
}

impl<D: Get<N>, N: Nonce> CheckedSub for NonceWithDefault<D, N> {
	fn checked_sub(&self, rhs: &Self) -> Option<Self> {
		self.0.checked_sub(&rhs.0).map(Self::new)
	}
}

impl<D: Get<N>, N: Nonce> Add for NonceWithDefault<D, N> {
	type Output = Self;
	fn add(self, rhs: Self) -> Self {
		Self::new(self.0 + rhs.0)
	}
}

impl<D: Get<N>, N: Nonce> CheckedAdd for NonceWithDefault<D, N> {
	fn checked_add(&self, rhs: &Self) -> Option<Self> {
		self.0.checked_add(&rhs.0).map(Self::new)
	}
}

impl<D: Get<N>, N: Nonce> BitAnd for NonceWithDefault<D, N> {
	type Output = Self;
	fn bitand(self, rhs: Self) -> Self {
		Self::new(self.0 & rhs.0)
	}
}

impl<D: Get<N>, N: Nonce> BitOr for NonceWithDefault<D, N> {
	type Output = Self;
	fn bitor(self, rhs: Self) -> Self {
		Self::new(self.0 | rhs.0)
	}
}

impl<D: Get<N>, N: Nonce> BitXor for NonceWithDefault<D, N> {
	type Output = Self;
	fn bitxor(self, rhs: Self) -> Self {
		Self::new(self.0 ^ rhs.0)
	}
}

impl<D: Get<N>, N: Nonce> One for NonceWithDefault<D, N> {
	fn one() -> Self {
		Self::new(N::one())
	}
}

impl<D: Get<N>, N: Nonce> Not for NonceWithDefault<D, N> {
	type Output = Self;
	fn not(self) -> Self {
		Self::new(self.0.not())
	}
}

impl<D: Get<N>, N: Nonce> NumCast for NonceWithDefault<D, N> {
	fn from<T: ToPrimitive>(n: T) -> Option<Self> {
		<N as NumCast>::from(n).map_or(None, |n| Some(Self::new(n)))
	}
}

impl<D: Get<N>, N: Nonce> Num for NonceWithDefault<D, N> {
	type FromStrRadixErr = <N as Num>::FromStrRadixErr;

	fn from_str_radix(s: &str, radix: u32) -> Result<Self, Self::FromStrRadixErr> {
		N::from_str_radix(s, radix).map(Self::new)
	}
}

impl<D: Get<N>, N: Nonce> ToPrimitive for NonceWithDefault<D, N> {
	fn to_i64(&self) -> Option<i64> {
		self.0.to_i64()
	}

	fn to_u64(&self) -> Option<u64> {
		self.0.to_u64()
	}

	fn to_i128(&self) -> Option<i128> {
		self.0.to_i128()
	}

	fn to_u128(&self) -> Option<u128> {
		self.0.to_u128()
	}
}

impl<D: Get<N>, N: Nonce> From<Compact<NonceWithDefault<D, N>>> for NonceWithDefault<D, N> {
	fn from(c: Compact<NonceWithDefault<D, N>>) -> Self {
		c.0
	}
}

impl<D: Get<N>, N: Nonce> CompactAs for NonceWithDefault<D, N> {
	type As = N;

	fn encode_as(&self) -> &Self::As {
		&self.0
	}

	fn decode_from(val: Self::As) -> Result<Self, codec::Error> {
		Ok(Self::new(val))
	}
}
