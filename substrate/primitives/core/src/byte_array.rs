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
#[derive(
	Clone, Copy, Encode, Decode, MaxEncodedLen, TypeInfo, Eq, PartialEq, PartialOrd, Ord, Hash,
)]
pub struct ByteArray<const N: usize, M = ()> {
	/// Inner raw array
	pub inner: [u8; N],
	marker: PhantomData<fn() -> M>,
}

impl<const N: usize, M> Default for ByteArray<N, M> {
	fn default() -> Self {
		Self { inner: [0_u8; N], marker: PhantomData }
	}
}

impl<const N: usize, M> PassByInner for ByteArray<N, M> {
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

impl<const N: usize, M> PassBy for ByteArray<N, M> {
	type PassBy = pass_by::Inner<Self, [u8; N]>;
}

impl<const N: usize, M> AsRef<[u8]> for ByteArray<N, M> {
	fn as_ref(&self) -> &[u8] {
		&self.inner[..]
	}
}

impl<const N: usize, M> AsMut<[u8]> for ByteArray<N, M> {
	fn as_mut(&mut self) -> &mut [u8] {
		&mut self.inner[..]
	}
}

impl<const N: usize, M> From<ByteArray<N, M>> for [u8; N] {
	fn from(v: ByteArray<N, M>) -> [u8; N] {
		v.inner
	}
}

impl<const N: usize, M> AsRef<[u8; N]> for ByteArray<N, M> {
	fn as_ref(&self) -> &[u8; N] {
		&self.inner
	}
}

impl<const N: usize, M> From<[u8; N]> for ByteArray<N, M> {
	fn from(value: [u8; N]) -> Self {
		Self::from_raw(value)
	}
}

impl<const N: usize, M> TryFrom<&[u8]> for ByteArray<N, M> {
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

impl<const N: usize, M> UncheckedFrom<[u8; N]> for ByteArray<N, M> {
	fn unchecked_from(data: [u8; N]) -> Self {
		Self::from_raw(data)
	}
}

impl<const N: usize, M> core::ops::Deref for ByteArray<N, M> {
	type Target = [u8];

	fn deref(&self) -> &Self::Target {
		&self.inner
	}
}

impl<const N: usize, M> ByteArray<N, M> {
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

impl<const N: usize, M> ByteArray<N, M> {
	/// Size of the byte array.
	pub const LEN: usize = N;
}

impl<const N: usize, M> crate::ByteArray for ByteArray<N, M> {
	const LEN: usize = N;
}

impl<const N: usize, M> FromEntropy for ByteArray<N, M> {
	fn from_entropy(input: &mut impl codec::Input) -> Result<Self, codec::Error> {
		let mut result = Self::default();
		input.read(result.as_mut())?;
		Ok(result)
	}
}

impl<M> From<ByteArray<32, M>> for H256 {
	fn from(x: ByteArray<32, M>) -> H256 {
		H256::from(x.inner)
	}
}

impl<M> From<ByteArray<64, M>> for H512 {
	fn from(x: ByteArray<64, M>) -> H512 {
		H512::from(x.inner)
	}
}

impl<M> ByteArray<32, M> {
	/// A new instance from an H256.
	pub fn from_h256(x: H256) -> Self {
		Self::from_raw(x.into())
	}
}

impl<M> ByteArray<64, M> {
	/// A new instance from an H512.
	pub fn from_h512(x: H512) -> Self {
		Self::from_raw(x.into())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	struct Marker<const I: u8 = 0>;

	type Foo = ByteArray<32, Marker>;
	type Bar = ByteArray<32, Marker<1>>;

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

		// Different Maker!
		// print_bar(&foo);
	}
}
