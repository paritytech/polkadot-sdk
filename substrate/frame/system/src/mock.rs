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

use crate::{self as frame_system, *};
use frame_support::{derive_impl, parameter_types};
use sp_runtime::{BuildStorage, Perbill};
use core::{fmt::Display, ops::{Add, AddAssign, BitAnd, BitOr, BitXor, Deref, Div, DivAssign, Mul, MulAssign, Not, Rem, RemAssign, Shl, Shr, Sub, SubAssign}};
use num_traits::{CheckedAdd, CheckedDiv, CheckedMul, CheckedNeg, CheckedRem, CheckedShl, CheckedShr, CheckedSub, Num, NumCast, PrimInt, Saturating, ToPrimitive};
use sp_runtime::{Deserialize, Serialize};
use codec::{Compact, CompactAs, Decode, Encode, MaxEncodedLen};

type Block = mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
	}
);

const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);
const MAX_BLOCK_WEIGHT: Weight = Weight::from_parts(1024, u64::MAX);

parameter_types! {
	pub Version: RuntimeVersion = RuntimeVersion {
		spec_name: sp_version::create_runtime_str!("test"),
		impl_name: sp_version::create_runtime_str!("system-test"),
		authoring_version: 1,
		spec_version: 1,
		impl_version: 1,
		apis: sp_version::create_apis_vec!([]),
		transaction_version: 1,
		state_version: 1,
	};
	pub const DbWeight: RuntimeDbWeight = RuntimeDbWeight {
		read: 10,
		write: 100,
	};
	pub RuntimeBlockWeights: limits::BlockWeights = limits::BlockWeights::builder()
		.base_block(Weight::from_parts(10, 0))
		.for_class(DispatchClass::all(), |weights| {
			weights.base_extrinsic = Weight::from_parts(5, 0);
		})
		.for_class(DispatchClass::Normal, |weights| {
			weights.max_total = Some(NORMAL_DISPATCH_RATIO * MAX_BLOCK_WEIGHT);
		})
		.for_class(DispatchClass::Operational, |weights| {
			weights.base_extrinsic = Weight::from_parts(10, 0);
			weights.max_total = Some(MAX_BLOCK_WEIGHT);
			weights.reserved = Some(
				MAX_BLOCK_WEIGHT - NORMAL_DISPATCH_RATIO * MAX_BLOCK_WEIGHT
			);
		})
		.avg_block_initialization(Perbill::from_percent(0))
		.build_or_panic();
	pub RuntimeBlockLength: limits::BlockLength =
		limits::BlockLength::max_with_normal_ratio(1024, NORMAL_DISPATCH_RATIO);
}

parameter_types! {
	pub static Killed: Vec<u64> = vec![];
}

pub struct RecordKilled;
impl OnKilledAccount<u64> for RecordKilled {
	fn on_killed_account(who: &u64) {
		Killed::mutate(|r| r.push(*who))
	}
}

#[derive(Encode, Decode, Copy, Clone, PartialOrd, Ord, Eq, PartialEq, TypeInfo, Debug, MaxEncodedLen, Serialize, Deserialize)]
pub struct Nonce(u64);

impl Deref for Nonce {
	type Target = u64;
	fn deref(&self) -> &Self::Target {
		&self.0
	}
}
impl Default for Nonce {
	fn default() -> Self {
		Self(System::block_number())
	}
}
impl From<u32> for Nonce {
	fn from(value: u32) -> Self {
		Self(value as u64)
	}
}
impl From<u16> for Nonce {
	fn from(value: u16) -> Self {
		Self(value as u64)
	}
}
impl CheckedNeg for Nonce {
	fn checked_neg(&self) -> Option<Self> {
		self.0.checked_neg().map(Self)
	}
}
impl CheckedRem for Nonce {
	fn checked_rem(&self, rhs: &Self) -> Option<Self> {
		self.0.checked_rem(rhs.0).map(Self)
	}
}

impl CheckedShr for Nonce {
	fn checked_shr(&self, n: u32) -> Option<Self> {
		self.0.checked_shr(n).map(Self)
	}
}

impl CheckedShl for Nonce {
	fn checked_shl(&self, n: u32) -> Option<Self> {
		self.0.checked_shl(n).map(Self)
	}
}

impl Rem for Nonce {
	type Output = Self;
	fn rem(self, rhs: Self) -> Self {
		Self(self.0 % rhs.0)
	}
}

impl Rem<u32> for Nonce {
	type Output = Self;
	fn rem(self, rhs: u32) -> Self {
		Self(self.0 % (rhs as u64))
	}
}

impl Shr<u32> for Nonce {
	type Output = Self;
	fn shr(self, rhs: u32) -> Self {
		Self(self.0 >> rhs)
	}
}

impl Shr<usize> for Nonce {
	type Output = Self;
	fn shr(self, rhs: usize) -> Self {
		Self(self.0 >> rhs)
	}
}

impl Shl<u32> for Nonce {
	type Output = Self;
	fn shl(self, rhs: u32) -> Self {
		Self(self.0 << rhs)
	}
}

impl Shl<usize> for Nonce {
	type Output = Self;
	fn shl(self, rhs: usize) -> Self {
		Self(self.0 << rhs)
	}
}

impl RemAssign for Nonce {
	fn rem_assign(&mut self, rhs: Self) {
		self.0 %= rhs.0
	}
}

impl DivAssign for Nonce {
	fn div_assign(&mut self, rhs: Self) {
		self.0 /= rhs.0
	}
}

impl MulAssign for Nonce {
	fn mul_assign(&mut self, rhs: Self) {
		self.0 *= rhs.0
	}
}

impl SubAssign for Nonce {
	fn sub_assign(&mut self, rhs: Self) {
		self.0 -= rhs.0
	}
}

impl AddAssign for Nonce {
	fn add_assign(&mut self, rhs: Self) {
		self.0 += rhs.0
	}
}

impl From<u8> for Nonce {
	fn from(value: u8) -> Self {
		Self(value as u64)
	}
}

impl Display for Nonce {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl From<Nonce> for u32 {
	fn from(n: Nonce) -> u32 {
		n.0 as u32
	}
}

impl From<Nonce> for u16 {
	fn from(n: Nonce) -> u16 {
		n.0 as u16
	}
}

impl From<Nonce> for u128 {
	fn from(n: Nonce) -> u128 {
		n.0 as u128
	}
}

impl From<Nonce> for usize {
	fn from(n: Nonce) -> usize {
		n.0 as usize
	}
}

impl From<u64> for Nonce {
	fn from(n: u64) -> Nonce {
		Nonce(n)
	}
}

impl From<u128> for Nonce {
	fn from(n: u128) -> Nonce {
		Nonce(n as u64)
	}
}

impl From<usize> for Nonce {
	fn from(n: usize) -> Nonce {
		Nonce(n as u64)
	}
}

impl From<Nonce> for u8 {
	fn from(n: Nonce) -> u8 {
		n.0 as u8
	}
}

impl From<Nonce> for u64 {
	fn from(n: Nonce) -> u64 {
		n.0
	}
}

impl Zero for Nonce {
	fn zero() -> Self {
		Nonce(0)
	}

	fn is_zero(&self) -> bool {
		self.0 == 0
	}
}

impl Bounded for Nonce {
	fn min_value() -> Self {
		Nonce(u64::min_value())
	}

	fn max_value() -> Self {
		Nonce(u64::max_value())
	}
}

impl PrimInt for Nonce {
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
		Nonce(self.0.rotate_left(n))
	}

	fn rotate_right(self, n: u32) -> Self {
		Nonce(self.0.rotate_right(n))
	}

	fn swap_bytes(self) -> Self {
		Nonce(self.0.swap_bytes())
	}

	fn from_be(x: Self) -> Self {
		Nonce(u64::from_be(x.0))
	}

	fn from_le(x: Self) -> Self {
		Nonce(u64::from_le(x.0))
	}

	fn to_be(self) -> Self {
		Nonce(self.0.to_be())
	}

	fn to_le(self) -> Self {
		Nonce(self.0.to_le())
	}

	fn count_zeros(self) -> u32 {
		self.0.count_zeros()
	}
	
	fn signed_shl(self, n: u32) -> Self {
		Nonce(self.0.wrapping_shl(n))
	}

	fn signed_shr(self, n: u32) -> Self {
		Nonce(self.0.wrapping_shr(n))
	}

	fn unsigned_shl(self, n: u32) -> Self {
		Nonce(self.0.wrapping_shl(n))
	}

	fn unsigned_shr(self, n: u32) -> Self {
		Nonce(self.0.wrapping_shr(n))
	}

	fn pow(self, exp: u32) -> Self {
		Nonce(self.0.pow(exp))
	}
}

impl Saturating for Nonce {
	fn saturating_add(self, rhs: Self) -> Self {
		Nonce(self.0.saturating_add(rhs.0))
	}

	fn saturating_sub(self, rhs: Self) -> Self {
		Nonce(self.0.saturating_sub(rhs.0))
	}
}

impl Div for Nonce {
	type Output = Self;
	fn div(self, rhs: Self) -> Self {
		Nonce(self.0 / rhs.0)
	}
}

impl Mul for Nonce {
	type Output = Self;
	fn mul(self, rhs: Self) -> Self {
		Nonce(self.0 * rhs.0)
	}
}

impl CheckedDiv for Nonce {
	fn checked_div(&self, rhs: &Self) -> Option<Self> {
		self.0.checked_div(rhs.0).map(Self)
	}
}

impl CheckedMul for Nonce {
	fn checked_mul(&self, rhs: &Self) -> Option<Self> {
		self.0.checked_mul(rhs.0).map(Self)
	}
}

impl Sub for Nonce {
	type Output = Self;
	fn sub(self, rhs: Self) -> Self {
		Nonce(self.0 - rhs.0)
	}
}

impl CheckedSub for Nonce {
	fn checked_sub(&self, rhs: &Self) -> Option<Self> {
		self.0.checked_sub(rhs.0).map(Self)
	}
}

impl Add for Nonce {
	type Output = Self;
	fn add(self, rhs: Self) -> Self {
		Nonce(self.0 + rhs.0)
	}
}

impl CheckedAdd for Nonce {
	fn checked_add(&self, rhs: &Self) -> Option<Self> {
		self.0.checked_add(rhs.0).map(Self)
	}
}

impl BitAnd for Nonce {
	type Output = Self;
	fn bitand(self, rhs: Self) -> Self {
		Nonce(self.0 & rhs.0)
	}
}

impl BitOr for Nonce {
	type Output = Self;
	fn bitor(self, rhs: Self) -> Self {
		Nonce(self.0 | rhs.0)
	}
}

impl BitXor for Nonce {
	type Output = Self;
	fn bitxor(self, rhs: Self) -> Self {
		Nonce(self.0 ^ rhs.0)
	}
}

impl One for Nonce {
	fn one() -> Self {
		Nonce(1)
	}
}

impl Not for Nonce {
	type Output = Self;
	fn not(self) -> Self {
		Nonce(!self.0)
	}
}

impl NumCast for Nonce {
	fn from<T: ToPrimitive>(n: T) -> Option<Self> {
		n.to_u64().map(Nonce)
	}
}

impl Num for Nonce {
	type FromStrRadixErr = <u64 as Num>::FromStrRadixErr;

	fn from_str_radix(s: &str, radix: u32) -> Result<Self, Self::FromStrRadixErr> {
		u64::from_str_radix(s, radix).map(Nonce)
	}
}

impl ToPrimitive for Nonce {
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

impl From<Compact<Nonce>> for Nonce {
	fn from(c: Compact<Nonce>) -> Self {
		c.0
	}
}

impl CompactAs for Nonce {
	type As = u64;

	fn encode_as(&self) -> &Self::As {
		&self.0
	}

	fn decode_from(val: Self::As) -> Result<Self, codec::Error> {
		Ok(Nonce(val))
	}
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl Config for Test {
	type BlockWeights = RuntimeBlockWeights;
	type BlockLength = RuntimeBlockLength;
	type Block = Block;
	type Version = Version;
	type AccountData = u32;
	type OnKilledAccount = RecordKilled;
	type MultiBlockMigrator = MockedMigrator;
	type Nonce = Nonce;
}

parameter_types! {
	pub static Ongoing: bool = false;
}

pub struct MockedMigrator;
impl frame_support::migrations::MultiStepMigrator for MockedMigrator {
	fn ongoing() -> bool {
		Ongoing::get()
	}

	fn step() -> Weight {
		Weight::zero()
	}
}

pub type SysEvent = frame_system::Event<Test>;

/// A simple call, which one doesn't matter.
pub const CALL: &<Test as Config>::RuntimeCall =
	&RuntimeCall::System(frame_system::Call::set_heap_pages { pages: 0u64 });

/// Create new externalities for `System` module tests.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext: sp_io::TestExternalities =
		RuntimeGenesisConfig::default().build_storage().unwrap().into();
	// Add to each test the initial weight of a block
	ext.execute_with(|| {
		System::register_extra_weight_unchecked(
			<Test as crate::Config>::BlockWeights::get().base_block,
			DispatchClass::Mandatory,
		)
	});
	ext
}
