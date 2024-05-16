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

//! Provides a type that wraps another type and provides a default value.

use crate::traits::{Bounded, One, Zero};
use codec::{Compact, CompactAs, Decode, Encode, HasCompact, MaxEncodedLen};
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

/// A type that wraps another type and provides a default value.
///
/// Passes through arithmetical and many other operations to the inner value.
#[derive(Encode, Decode, TypeInfo, Debug, MaxEncodedLen)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TypeWithDefault<T, D: Get<T>>(T, PhantomData<D>);

impl<T, D: Get<T>> TypeWithDefault<T, D> {
	fn new(value: T) -> Self {
		Self(value, PhantomData)
	}
}

impl<T: Clone, D: Get<T>> Clone for TypeWithDefault<T, D> {
	fn clone(&self) -> Self {
		Self(self.0.clone(), PhantomData)
	}
}

impl<T: Copy, D: Get<T>> Copy for TypeWithDefault<T, D> {}

impl<T: PartialEq, D: Get<T>> PartialEq for TypeWithDefault<T, D> {
	fn eq(&self, other: &Self) -> bool {
		self.0 == other.0
	}
}

impl<T: Eq, D: Get<T>> Eq for TypeWithDefault<T, D> {}

impl<T: PartialOrd, D: Get<T>> PartialOrd for TypeWithDefault<T, D> {
	fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
		self.0.partial_cmp(&other.0)
	}
}

impl<T: Ord, D: Get<T>> Ord for TypeWithDefault<T, D> {
	fn cmp(&self, other: &Self) -> core::cmp::Ordering {
		self.0.cmp(&other.0)
	}
}

impl<T, D: Get<T>> Deref for TypeWithDefault<T, D> {
	type Target = T;
	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl<T, D: Get<T>> Default for TypeWithDefault<T, D> {
	fn default() -> Self {
		Self::new(D::get())
	}
}

impl<T: From<u16>, D: Get<T>> From<u16> for TypeWithDefault<T, D> {
	fn from(value: u16) -> Self {
		Self::new(value.into())
	}
}

impl<T: From<u32>, D: Get<T>> From<u32> for TypeWithDefault<T, D> {
	fn from(value: u32) -> Self {
		Self::new(value.into())
	}
}

impl<T: From<u64>, D: Get<T>> From<u64> for TypeWithDefault<T, D> {
	fn from(value: u64) -> Self {
		Self::new(value.into())
	}
}

impl<T: CheckedNeg, D: Get<T>> CheckedNeg for TypeWithDefault<T, D> {
	fn checked_neg(&self) -> Option<Self> {
		self.0.checked_neg().map(Self::new)
	}
}

impl<T: CheckedRem, D: Get<T>> CheckedRem for TypeWithDefault<T, D> {
	fn checked_rem(&self, rhs: &Self) -> Option<Self> {
		self.0.checked_rem(&rhs.0).map(Self::new)
	}
}

impl<T: CheckedShr, D: Get<T>> CheckedShr for TypeWithDefault<T, D> {
	fn checked_shr(&self, n: u32) -> Option<Self> {
		self.0.checked_shr(n).map(Self::new)
	}
}

impl<T: CheckedShl, D: Get<T>> CheckedShl for TypeWithDefault<T, D> {
	fn checked_shl(&self, n: u32) -> Option<Self> {
		self.0.checked_shl(n).map(Self::new)
	}
}

impl<T: Rem<Output = T>, D: Get<T>> Rem for TypeWithDefault<T, D> {
	type Output = Self;
	fn rem(self, rhs: Self) -> Self {
		Self::new(self.0 % rhs.0)
	}
}

impl<T: Rem<u32, Output = T>, D: Get<T>> Rem<u32> for TypeWithDefault<T, D> {
	type Output = Self;
	fn rem(self, rhs: u32) -> Self {
		Self::new(self.0 % (rhs.into()))
	}
}

impl<T: Shr<u32, Output = T>, D: Get<T>> Shr<u32> for TypeWithDefault<T, D> {
	type Output = Self;
	fn shr(self, rhs: u32) -> Self {
		Self::new(self.0 >> rhs)
	}
}

impl<T: Shr<usize, Output = T>, D: Get<T>> Shr<usize> for TypeWithDefault<T, D> {
	type Output = Self;
	fn shr(self, rhs: usize) -> Self {
		Self::new(self.0 >> rhs)
	}
}

impl<T: Shl<u32, Output = T>, D: Get<T>> Shl<u32> for TypeWithDefault<T, D> {
	type Output = Self;
	fn shl(self, rhs: u32) -> Self {
		Self::new(self.0 << rhs)
	}
}

impl<T: Shl<usize, Output = T>, D: Get<T>> Shl<usize> for TypeWithDefault<T, D> {
	type Output = Self;
	fn shl(self, rhs: usize) -> Self {
		Self::new(self.0 << rhs)
	}
}

impl<T: RemAssign, D: Get<T>> RemAssign for TypeWithDefault<T, D> {
	fn rem_assign(&mut self, rhs: Self) {
		self.0 %= rhs.0
	}
}

impl<T: DivAssign, D: Get<T>> DivAssign for TypeWithDefault<T, D> {
	fn div_assign(&mut self, rhs: Self) {
		self.0 /= rhs.0
	}
}

impl<T: MulAssign, D: Get<T>> MulAssign for TypeWithDefault<T, D> {
	fn mul_assign(&mut self, rhs: Self) {
		self.0 *= rhs.0
	}
}

impl<T: SubAssign, D: Get<T>> SubAssign for TypeWithDefault<T, D> {
	fn sub_assign(&mut self, rhs: Self) {
		self.0 -= rhs.0
	}
}

impl<T: AddAssign, D: Get<T>> AddAssign for TypeWithDefault<T, D> {
	fn add_assign(&mut self, rhs: Self) {
		self.0 += rhs.0
	}
}

impl<T: From<u8>, D: Get<T>> From<u8> for TypeWithDefault<T, D> {
	fn from(value: u8) -> Self {
		Self::new(value.into())
	}
}

impl<T: Display, D: Get<T>> Display for TypeWithDefault<T, D> {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl<T: TryFrom<u128>, D: Get<T>> TryFrom<u128> for TypeWithDefault<T, D> {
	type Error = <T as TryFrom<u128>>::Error;
	fn try_from(n: u128) -> Result<TypeWithDefault<T, D>, Self::Error> {
		T::try_from(n).map(Self::new)
	}
}

impl<T: TryFrom<usize>, D: Get<T>> TryFrom<usize> for TypeWithDefault<T, D> {
	type Error = <T as TryFrom<usize>>::Error;
	fn try_from(n: usize) -> Result<TypeWithDefault<T, D>, Self::Error> {
		T::try_from(n).map(Self::new)
	}
}

impl<T: TryInto<u8>, D: Get<T>> TryFrom<TypeWithDefault<T, D>> for u8 {
	type Error = <T as TryInto<u8>>::Error;
	fn try_from(value: TypeWithDefault<T, D>) -> Result<Self, Self::Error> {
		value.0.try_into()
	}
}

impl<T: TryInto<u16>, D: Get<T>> TryFrom<TypeWithDefault<T, D>> for u16 {
	type Error = <T as TryInto<u16>>::Error;
	fn try_from(value: TypeWithDefault<T, D>) -> Result<Self, Self::Error> {
		value.0.try_into()
	}
}

impl<T: TryInto<u32>, D: Get<T>> TryFrom<TypeWithDefault<T, D>> for u32 {
	type Error = <T as TryInto<u32>>::Error;
	fn try_from(value: TypeWithDefault<T, D>) -> Result<Self, Self::Error> {
		value.0.try_into()
	}
}

impl<T: TryInto<u64>, D: Get<T>> TryFrom<TypeWithDefault<T, D>> for u64 {
	type Error = <T as TryInto<u64>>::Error;
	fn try_from(value: TypeWithDefault<T, D>) -> Result<Self, Self::Error> {
		value.0.try_into()
	}
}

impl<T: TryInto<u128>, D: Get<T>> TryFrom<TypeWithDefault<T, D>> for u128 {
	type Error = <T as TryInto<u128>>::Error;
	fn try_from(value: TypeWithDefault<T, D>) -> Result<Self, Self::Error> {
		value.0.try_into()
	}
}

impl<T: TryInto<usize>, D: Get<T>> TryFrom<TypeWithDefault<T, D>> for usize {
	type Error = <T as TryInto<usize>>::Error;
	fn try_from(value: TypeWithDefault<T, D>) -> Result<Self, Self::Error> {
		value.0.try_into()
	}
}

impl<T: Zero + PartialEq, D: Get<T>> Zero for TypeWithDefault<T, D> {
	fn zero() -> Self {
		Self::new(T::zero())
	}

	fn is_zero(&self) -> bool {
		self.0 == T::zero()
	}
}

impl<T: Bounded, D: Get<T>> Bounded for TypeWithDefault<T, D> {
	fn min_value() -> Self {
		Self::new(T::min_value())
	}

	fn max_value() -> Self {
		Self::new(T::max_value())
	}
}

impl<T: PrimInt, D: Get<T>> PrimInt for TypeWithDefault<T, D> {
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
		Self::new(T::from_be(x.0))
	}

	fn from_le(x: Self) -> Self {
		Self::new(T::from_le(x.0))
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

impl<T: Saturating, D: Get<T>> Saturating for TypeWithDefault<T, D> {
	fn saturating_add(self, rhs: Self) -> Self {
		Self::new(self.0.saturating_add(rhs.0))
	}

	fn saturating_sub(self, rhs: Self) -> Self {
		Self::new(self.0.saturating_sub(rhs.0))
	}
}

impl<T: Div<Output = T>, D: Get<T>> Div for TypeWithDefault<T, D> {
	type Output = Self;
	fn div(self, rhs: Self) -> Self {
		Self::new(self.0 / rhs.0)
	}
}

impl<T: Mul<Output = T>, D: Get<T>> Mul for TypeWithDefault<T, D> {
	type Output = Self;
	fn mul(self, rhs: Self) -> Self {
		Self::new(self.0 * rhs.0)
	}
}

impl<T: CheckedDiv, D: Get<T>> CheckedDiv for TypeWithDefault<T, D> {
	fn checked_div(&self, rhs: &Self) -> Option<Self> {
		self.0.checked_div(&rhs.0).map(Self::new)
	}
}

impl<T: CheckedMul, D: Get<T>> CheckedMul for TypeWithDefault<T, D> {
	fn checked_mul(&self, rhs: &Self) -> Option<Self> {
		self.0.checked_mul(&rhs.0).map(Self::new)
	}
}

impl<T: Sub<Output = T>, D: Get<T>> Sub for TypeWithDefault<T, D> {
	type Output = Self;
	fn sub(self, rhs: Self) -> Self {
		Self::new(self.0 - rhs.0)
	}
}

impl<T: CheckedSub, D: Get<T>> CheckedSub for TypeWithDefault<T, D> {
	fn checked_sub(&self, rhs: &Self) -> Option<Self> {
		self.0.checked_sub(&rhs.0).map(Self::new)
	}
}

impl<T: Add<Output = T>, D: Get<T>> Add for TypeWithDefault<T, D> {
	type Output = Self;
	fn add(self, rhs: Self) -> Self {
		Self::new(self.0 + rhs.0)
	}
}

impl<T: CheckedAdd, D: Get<T>> CheckedAdd for TypeWithDefault<T, D> {
	fn checked_add(&self, rhs: &Self) -> Option<Self> {
		self.0.checked_add(&rhs.0).map(Self::new)
	}
}

impl<T: BitAnd<Output = T>, D: Get<T>> BitAnd for TypeWithDefault<T, D> {
	type Output = Self;
	fn bitand(self, rhs: Self) -> Self {
		Self::new(self.0 & rhs.0)
	}
}

impl<T: BitOr<Output = T>, D: Get<T>> BitOr for TypeWithDefault<T, D> {
	type Output = Self;
	fn bitor(self, rhs: Self) -> Self {
		Self::new(self.0 | rhs.0)
	}
}

impl<T: BitXor<Output = T>, D: Get<T>> BitXor for TypeWithDefault<T, D> {
	type Output = Self;
	fn bitxor(self, rhs: Self) -> Self {
		Self::new(self.0 ^ rhs.0)
	}
}

impl<T: One, D: Get<T>> One for TypeWithDefault<T, D> {
	fn one() -> Self {
		Self::new(T::one())
	}
}

impl<T: Not<Output = T>, D: Get<T>> Not for TypeWithDefault<T, D> {
	type Output = Self;
	fn not(self) -> Self {
		Self::new(self.0.not())
	}
}

impl<T: NumCast, D: Get<T>> NumCast for TypeWithDefault<T, D> {
	fn from<P: ToPrimitive>(n: P) -> Option<Self> {
		<T as NumCast>::from(n).map_or(None, |n| Some(Self::new(n)))
	}
}

impl<T: Num, D: Get<T>> Num for TypeWithDefault<T, D> {
	type FromStrRadixErr = <T as Num>::FromStrRadixErr;

	fn from_str_radix(s: &str, radix: u32) -> Result<Self, Self::FromStrRadixErr> {
		T::from_str_radix(s, radix).map(Self::new)
	}
}

impl<T: ToPrimitive, D: Get<T>> ToPrimitive for TypeWithDefault<T, D> {
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

impl<T, D: Get<T>> From<Compact<TypeWithDefault<T, D>>> for TypeWithDefault<T, D> {
	fn from(c: Compact<TypeWithDefault<T, D>>) -> Self {
		c.0
	}
}

impl<T: HasCompact, D: Get<T>> CompactAs for TypeWithDefault<T, D> {
	type As = T;

	fn encode_as(&self) -> &Self::As {
		&self.0
	}

	fn decode_from(val: Self::As) -> Result<Self, codec::Error> {
		Ok(Self::new(val))
	}
}
