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
	crypto::{CryptoType, Derive, FromEntropy, Public, Signature, UncheckedFrom},
	hash::{H256, H512},
};

use codec::{Decode, Encode, MaxEncodedLen};
use core::marker::PhantomData;
use scale_info::TypeInfo;

use sp_runtime_interface::pass_by::{self, PassBy, PassByInner};

#[cfg(feature = "serde")]
use crate::crypto::Ss58Codec;
#[cfg(feature = "serde")]
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

#[cfg(all(not(feature = "std"), feature = "serde"))]
use sp_std::alloc::{format, string::String};

pub use public_bytes::*;
pub use signature_bytes::*;

/// Generic byte array holding some crypto-related raw data.
///
/// The type is generic over a constant length `N` and a "tag" `T` which
/// can be used to specialize the byte array without requiring newtypes.
///
/// The tag `T` is held in a `PhantomData<fn() ->T>`, a trick allowing
/// `CryptoBytes` to be `Send` and `Sync` regardless of `T` properties
/// ([ref](https://doc.rust-lang.org/nomicon/phantom-data.html#table-of-phantomdata-patterns)).
#[derive(Encode, Decode, MaxEncodedLen)]
#[repr(transparent)]
pub struct CryptoBytes<const N: usize, T = ()>(pub [u8; N], PhantomData<fn() -> T>);

impl<const N: usize, T> Copy for CryptoBytes<N, T> {}

impl<const N: usize, T> Clone for CryptoBytes<N, T> {
	fn clone(&self) -> Self {
		Self(self.0, PhantomData)
	}
}

impl<const N: usize, T> TypeInfo for CryptoBytes<N, T> {
	type Identity = [u8; N];

	fn type_info() -> scale_info::Type {
		Self::Identity::type_info()
	}
}

impl<const N: usize, T> PartialOrd for CryptoBytes<N, T> {
	fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
		self.0.partial_cmp(&other.0)
	}
}

impl<const N: usize, T> Ord for CryptoBytes<N, T> {
	fn cmp(&self, other: &Self) -> core::cmp::Ordering {
		self.0.cmp(&other.0)
	}
}

impl<const N: usize, T> PartialEq for CryptoBytes<N, T> {
	fn eq(&self, other: &Self) -> bool {
		self.0.eq(&other.0)
	}
}

impl<const N: usize, T> core::hash::Hash for CryptoBytes<N, T> {
	fn hash<H: scale_info::prelude::hash::Hasher>(&self, state: &mut H) {
		self.0.hash(state)
	}
}

impl<const N: usize, T> Eq for CryptoBytes<N, T> {}

impl<const N: usize, T> Default for CryptoBytes<N, T> {
	fn default() -> Self {
		Self([0_u8; N], PhantomData)
	}
}

impl<const N: usize, T> PassByInner for CryptoBytes<N, T> {
	type Inner = [u8; N];

	fn into_inner(self) -> Self::Inner {
		self.0
	}

	fn inner(&self) -> &Self::Inner {
		&self.0
	}

	fn from_inner(inner: Self::Inner) -> Self {
		Self(inner, PhantomData)
	}
}

impl<const N: usize, T> PassBy for CryptoBytes<N, T> {
	type PassBy = pass_by::Inner<Self, [u8; N]>;
}

impl<const N: usize, T> AsRef<[u8]> for CryptoBytes<N, T> {
	fn as_ref(&self) -> &[u8] {
		&self.0[..]
	}
}

impl<const N: usize, T> AsMut<[u8]> for CryptoBytes<N, T> {
	fn as_mut(&mut self) -> &mut [u8] {
		&mut self.0[..]
	}
}

impl<const N: usize, T> From<CryptoBytes<N, T>> for [u8; N] {
	fn from(v: CryptoBytes<N, T>) -> [u8; N] {
		v.0
	}
}

impl<const N: usize, T> AsRef<[u8; N]> for CryptoBytes<N, T> {
	fn as_ref(&self) -> &[u8; N] {
		&self.0
	}
}

impl<const N: usize, T> AsMut<[u8; N]> for CryptoBytes<N, T> {
	fn as_mut(&mut self) -> &mut [u8; N] {
		&mut self.0
	}
}

impl<const N: usize, T> From<[u8; N]> for CryptoBytes<N, T> {
	fn from(value: [u8; N]) -> Self {
		Self::from_raw(value)
	}
}

impl<const N: usize, T> TryFrom<&[u8]> for CryptoBytes<N, T> {
	type Error = ();

	fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
		if data.len() != N {
			return Err(())
		}
		let mut r = [0u8; N];
		r.copy_from_slice(data);
		Ok(Self::from_raw(r))
	}
}

impl<const N: usize, T> UncheckedFrom<[u8; N]> for CryptoBytes<N, T> {
	fn unchecked_from(data: [u8; N]) -> Self {
		Self::from_raw(data)
	}
}

impl<const N: usize, T> core::ops::Deref for CryptoBytes<N, T> {
	type Target = [u8];

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl<const N: usize, T> CryptoBytes<N, T> {
	/// Construct from raw array.
	pub fn from_raw(inner: [u8; N]) -> Self {
		Self(inner, PhantomData)
	}

	/// Construct from raw array.
	pub fn to_raw(self) -> [u8; N] {
		self.0
	}

	/// Return a slice filled with raw data.
	pub fn as_array_ref(&self) -> &[u8; N] {
		&self.0
	}
}

impl<const N: usize, T> crate::ByteArray for CryptoBytes<N, T> {
	const LEN: usize = N;
}

impl<const N: usize, T> FromEntropy for CryptoBytes<N, T> {
	fn from_entropy(input: &mut impl codec::Input) -> Result<Self, codec::Error> {
		let mut result = Self::default();
		input.read(result.as_mut())?;
		Ok(result)
	}
}

impl<T> From<CryptoBytes<32, T>> for H256 {
	fn from(x: CryptoBytes<32, T>) -> H256 {
		H256::from(x.0)
	}
}

impl<T> From<CryptoBytes<64, T>> for H512 {
	fn from(x: CryptoBytes<64, T>) -> H512 {
		H512::from(x.0)
	}
}

impl<T> UncheckedFrom<H256> for CryptoBytes<32, T> {
	fn unchecked_from(x: H256) -> Self {
		Self::from_h256(x)
	}
}

impl<T> CryptoBytes<32, T> {
	/// A new instance from an H256.
	pub fn from_h256(x: H256) -> Self {
		Self::from_raw(x.into())
	}
}

impl<T> CryptoBytes<64, T> {
	/// A new instance from an H512.
	pub fn from_h512(x: H512) -> Self {
		Self::from_raw(x.into())
	}
}

mod public_bytes {
	use super::*;

	/// Tag used for generic public key bytes.
	pub struct PublicTag;

	/// Generic encoded public key.
	pub type PublicBytes<const N: usize, SubTag> = CryptoBytes<N, (PublicTag, SubTag)>;

	impl<const N: usize, SubTag> Derive for PublicBytes<N, SubTag> where Self: CryptoType {}

	impl<const N: usize, SubTag> Public for PublicBytes<N, SubTag> where Self: CryptoType {}

	impl<const N: usize, SubTag> sp_std::fmt::Debug for PublicBytes<N, SubTag>
	where
		Self: CryptoType,
	{
		#[cfg(feature = "std")]
		fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
			let s = self.to_ss58check();
			write!(f, "{} ({}...)", crate::hexdisplay::HexDisplay::from(&self.as_ref()), &s[0..8])
		}

		#[cfg(not(feature = "std"))]
		fn fmt(&self, _: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
			Ok(())
		}
	}

	#[cfg(feature = "std")]
	impl<const N: usize, SubTag> std::fmt::Display for PublicBytes<N, SubTag>
	where
		Self: CryptoType,
	{
		fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
			write!(f, "{}", self.to_ss58check())
		}
	}

	#[cfg(feature = "std")]
	impl<const N: usize, SubTag> std::str::FromStr for PublicBytes<N, SubTag>
	where
		Self: CryptoType,
	{
		type Err = crate::crypto::PublicError;

		fn from_str(s: &str) -> Result<Self, Self::Err> {
			Self::from_ss58check(s)
		}
	}

	#[cfg(feature = "serde")]
	impl<const N: usize, SubTag> Serialize for PublicBytes<N, SubTag>
	where
		Self: CryptoType,
	{
		fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
		where
			S: Serializer,
		{
			serializer.serialize_str(&self.to_ss58check())
		}
	}

	#[cfg(feature = "serde")]
	impl<'de, const N: usize, SubTag> Deserialize<'de> for PublicBytes<N, SubTag>
	where
		Self: CryptoType,
	{
		fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
		where
			D: Deserializer<'de>,
		{
			Self::from_ss58check(&String::deserialize(deserializer)?)
				.map_err(|e| de::Error::custom(format!("{:?}", e)))
		}
	}
}

mod signature_bytes {
	use super::*;

	/// Tag used for generic signature bytes.
	pub struct SignatureTag;

	/// Generic encoded signature.
	pub type SignatureBytes<const N: usize, SubTag> = CryptoBytes<N, (SignatureTag, SubTag)>;

	impl<const N: usize, SubTag> Signature for SignatureBytes<N, SubTag> where Self: CryptoType {}

	#[cfg(feature = "serde")]
	impl<const N: usize, SubTag> Serialize for SignatureBytes<N, SubTag>
	where
		Self: CryptoType,
	{
		fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
		where
			S: Serializer,
		{
			serializer.serialize_str(&array_bytes::bytes2hex("", self))
		}
	}

	#[cfg(feature = "serde")]
	impl<'de, const N: usize, SubTag> Deserialize<'de> for SignatureBytes<N, SubTag>
	where
		Self: CryptoType,
	{
		fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
		where
			D: Deserializer<'de>,
		{
			let signature_hex = array_bytes::hex2bytes(&String::deserialize(deserializer)?)
				.map_err(|e| de::Error::custom(format!("{:?}", e)))?;
			Self::try_from(signature_hex.as_ref())
				.map_err(|e| de::Error::custom(format!("{:?}", e)))
		}
	}

	impl<const N: usize, SubTag> sp_std::fmt::Debug for SignatureBytes<N, SubTag>
	where
		Self: CryptoType,
	{
		#[cfg(feature = "std")]
		fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
			write!(f, "{}", crate::hexdisplay::HexDisplay::from(&&self.0[..]))
		}

		#[cfg(not(feature = "std"))]
		fn fmt(&self, _: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
			Ok(())
		}
	}
}
