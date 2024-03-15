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

//! Generic byte array which can be specialized with a marker type.

use crate::{
	crypto::{FromEntropy, UncheckedFrom},
	hash::{H256, H512},
};

use codec::{Decode, Encode, MaxEncodedLen};
use core::marker::PhantomData;
use scale_info::TypeInfo;
use sp_runtime_interface::pass_by::{self, PassBy, PassByInner};

/// Generic byte array.
///
/// The type is generic over a constant length `N` and a "tag" `T` which
/// can be used to specialize the byte array without using newtypes.
#[derive(Encode, Decode, MaxEncodedLen)]
pub struct ByteArray<const N: usize, T = ()> {
	/// Inner raw array
	pub inner: [u8; N],
	marker: PhantomData<fn() -> T>,
}

impl<const N: usize, T> Copy for ByteArray<N, T> {}

impl<const N: usize, T> Clone for ByteArray<N, T> {
	fn clone(&self) -> Self {
		Self { inner: self.inner, marker: PhantomData }
	}
}

impl<const N: usize, T> TypeInfo for ByteArray<N, T> {
	type Identity = [u8; N];

	fn type_info() -> scale_info::Type {
		Self::Identity::type_info()
	}
}

impl<const N: usize, T> PartialOrd for ByteArray<N, T> {
	fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
		self.inner.partial_cmp(&other.inner)
	}
}

impl<const N: usize, T> Ord for ByteArray<N, T> {
	fn cmp(&self, other: &Self) -> core::cmp::Ordering {
		self.inner.cmp(&other.inner)
	}
}

impl<const N: usize, T> PartialEq for ByteArray<N, T> {
	fn eq(&self, other: &Self) -> bool {
		self.inner.eq(&other.inner)
	}
}

impl<const N: usize, T> core::hash::Hash for ByteArray<N, T> {
	fn hash<H: scale_info::prelude::hash::Hasher>(&self, state: &mut H) {
		self.inner.hash(state)
	}
}

impl<const N: usize, T> Eq for ByteArray<N, T> {}

impl<const N: usize, T> Default for ByteArray<N, T> {
	fn default() -> Self {
		Self { inner: [0_u8; N], marker: PhantomData }
	}
}

impl<const N: usize, T> PassByInner for ByteArray<N, T> {
	type Inner = [u8; N];

	fn into_inner(self) -> Self::Inner {
		self.inner
	}

	fn inner(&self) -> &Self::Inner {
		&self.inner
	}

	fn from_inner(inner: Self::Inner) -> Self {
		Self { inner, marker: PhantomData }
	}
}

impl<const N: usize, T> PassBy for ByteArray<N, T> {
	type PassBy = pass_by::Inner<Self, [u8; N]>;
}

impl<const N: usize, T> AsRef<[u8]> for ByteArray<N, T> {
	fn as_ref(&self) -> &[u8] {
		&self.inner[..]
	}
}

impl<const N: usize, T> AsMut<[u8]> for ByteArray<N, T> {
	fn as_mut(&mut self) -> &mut [u8] {
		&mut self.inner[..]
	}
}

impl<const N: usize, T> From<ByteArray<N, T>> for [u8; N] {
	fn from(v: ByteArray<N, T>) -> [u8; N] {
		v.inner
	}
}

impl<const N: usize, T> AsRef<[u8; N]> for ByteArray<N, T> {
	fn as_ref(&self) -> &[u8; N] {
		&self.inner
	}
}

impl<const N: usize, T> From<[u8; N]> for ByteArray<N, T> {
	fn from(value: [u8; N]) -> Self {
		Self::from_raw(value)
	}
}

impl<const N: usize, T> TryFrom<&[u8]> for ByteArray<N, T> {
	type Error = ();

	fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
		if data.len() != N {
			return Err(())
		}
		let mut r = [0u8; N];
		r.copy_from_slice(data);
		Ok(Self::unchecked_from(r))
	}
}

impl<const N: usize, T> UncheckedFrom<[u8; N]> for ByteArray<N, T> {
	fn unchecked_from(data: [u8; N]) -> Self {
		Self::from_raw(data)
	}
}

impl<const N: usize, T> core::ops::Deref for ByteArray<N, T> {
	type Target = [u8];

	fn deref(&self) -> &Self::Target {
		&self.inner
	}
}

impl<const N: usize, T> ByteArray<N, T> {
	/// Construct from raw array.
	pub fn from_raw(inner: [u8; N]) -> Self {
		Self { inner, marker: PhantomData }
	}

	/// Construct from raw array.
	pub fn to_raw(self) -> [u8; N] {
		self.inner
	}

	/// Return a slice filled with raw data.
	pub fn as_array_ref(&self) -> &[u8; N] {
		self.as_ref()
	}
}

impl<const N: usize, T> ByteArray<N, T> {
	/// Size of the byte array.
	pub const LEN: usize = N;
}

impl<const N: usize, T> crate::ByteArray for ByteArray<N, T> {
	const LEN: usize = N;
}

impl<const N: usize, T> FromEntropy for ByteArray<N, T> {
	fn from_entropy(input: &mut impl codec::Input) -> Result<Self, codec::Error> {
		let mut result = Self::default();
		input.read(result.as_mut())?;
		Ok(result)
	}
}

impl<T> From<ByteArray<32, T>> for H256 {
	fn from(x: ByteArray<32, T>) -> H256 {
		H256::from(x.inner)
	}
}

impl<T> From<ByteArray<64, T>> for H512 {
	fn from(x: ByteArray<64, T>) -> H512 {
		H512::from(x.inner)
	}
}

impl<T> UncheckedFrom<H256> for ByteArray<32, T> {
	fn unchecked_from(x: H256) -> Self {
		Self::from_h256(x)
	}
}

impl<T> ByteArray<32, T> {
	/// A new instance from an H256.
	pub fn from_h256(x: H256) -> Self {
		Self::from_raw(x.into())
	}
}

impl<T> ByteArray<64, T> {
	/// A new instance from an H512.
	pub fn from_h512(x: H512) -> Self {
		Self::from_raw(x.into())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	struct Tag<const I: u8 = 0>;

	type Foo = ByteArray<32, Tag>;
	type Bar = ByteArray<32, Tag<1>>;

	fn print_foo(f: &Foo) {
		println!("{:02x?}", f.inner());
	}

	fn print_bar(f: &Bar) {
		println!("{:02x?}", f.inner());
	}

	#[test]
	fn byte_array_works() {
		let foo = Foo::default();
		let bar = Bar::default();

		print_foo(&foo);
		print_bar(&bar);

		// Different Tag!
		// print_bar(&foo);
	}
}
