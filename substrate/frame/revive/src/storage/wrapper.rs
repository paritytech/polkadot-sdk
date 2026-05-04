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

//! Helpers for separating execution-facing values from their storage codec.
//!
//! Runtime code often wants to operate on a rich execution type while keeping the on-chain
//! representation stable and deliberately small. This module provides a wrapper that stores both
//! forms together: the execution value is exposed to pallet code, while SCALE encoding, decoding,
//! metadata, and size limits are delegated to the storage representation.
//!
//! The wrapper is intentionally narrow. It does not perform configurable conversions and it does
//! not attempt to behave like a storage item. Its only job is to keep the execution value and
//! storage value synchronized at the boundary where an execution type is encoded into or decoded
//! from storage.
//!
//! # Note
//!
//! This module has `#[allow(unused)]` on some of the functionality that it offers. This is because
//! we're providing this abstraction as something which will be used now in the storage versioning
//! effort and also into the future. Therefore, some of the functions and methods available may not
//! be used today but they're still provided in case they're needed in the future or found to be
//! useful.

use core::{
	cmp::Ordering,
	convert::Infallible,
	default::Default,
	fmt::{Debug, Formatter, Result as FmtResult},
	mem::MaybeUninit,
	ops::Deref,
};

use alloc::vec::Vec;
use codec::{Decode, DecodeFinished, Encode, EncodeLike, MaxEncodedLen, Output};
use scale_info::TypeInfo;

/// Wraps an execution value while encoding and decoding it as another type.
///
/// `ExecutionType` is the type pallet code should use during execution. `StorageType` is the type
/// that defines the stable SCALE representation stored on-chain. The wrapper keeps a cached
/// `StorageType` value so that direct calls to [`Encode`], [`MaxEncodedLen`], and [`TypeInfo`] are
/// all based on the storage type.
///
/// Callers should construct the wrapper through [`Self::new`] and mutate the execution value
/// through [`Self::mutate`] or [`Self::try_mutate`]. Those methods refresh the cached storage value
/// after the mutation completes.
#[derive(Clone, Copy)]
pub struct StorageCodecWrapper<ExecutionType, StorageType> {
	inner: ExecutionType,
	codable: StorageType,
}

impl<ExecutionType, StorageType> StorageCodecWrapper<ExecutionType, StorageType> {
	/// Builds a wrapper from an execution value.
	///
	/// The storage representation is derived immediately and cached. This makes the encoded form
	/// independent from the execution type's own codec impls.
	#[allow(unused)]
	pub fn new(inner: ExecutionType) -> Self
	where
		ExecutionType: Clone + Into<StorageType>,
	{
		Self { codable: inner.clone().into(), inner }
	}

	/// Mutates the execution value and refreshes the cached storage value.
	///
	/// The storage value is refreshed even when the mutator returns an error. This keeps the
	/// wrapper internally consistent if the mutator made a partial change before reporting failure.
	#[allow(unused)]
	pub fn try_mutate<E>(
		&mut self,
		mut mutator: impl FnMut(&mut ExecutionType) -> Result<(), E>,
	) -> Result<(), E>
	where
		ExecutionType: Clone + Into<StorageType>,
	{
		let result = mutator(&mut self.inner);
		self.codable = self.inner.clone().into();
		result
	}

	/// Mutates the execution value and refreshes the cached storage value.
	///
	/// Use this for infallible mutations. Fallible mutations should use [`Self::try_mutate`] so the
	/// caller can handle the original error.
	#[allow(unused)]
	pub fn mutate(&mut self, mut mutator: impl FnMut(&mut ExecutionType))
	where
		ExecutionType: Clone + Into<StorageType>,
	{
		let result = self.try_mutate::<Infallible>(|inner| Ok(mutator(inner)));
		match result {
			Ok(()) => {},
			Err(error) => match error {},
		}
	}

	/// Returns the execution value used by pallet code.
	#[allow(unused)]
	pub fn as_inner(&self) -> &ExecutionType {
		&self.inner
	}

	/// Consumes the wrapper and returns the execution value.
	#[allow(unused)]
	pub fn into_inner(self) -> ExecutionType {
		self.inner
	}

	/// Consumes the wrapper and returns both synchronized representations.
	///
	/// This is mainly useful at boundary points that need to inspect or test the exact storage
	/// representation produced for an execution value.
	#[allow(unused)]
	pub fn destructure(self) -> (ExecutionType, StorageType) {
		(self.inner, self.codable)
	}
}

impl<ExecutionType, StorageType> Default for StorageCodecWrapper<ExecutionType, StorageType>
where
	ExecutionType: Default + Clone + Into<StorageType>,
{
	fn default() -> Self {
		Self::new(ExecutionType::default())
	}
}

impl<ExecutionType, StorageType> Debug for StorageCodecWrapper<ExecutionType, StorageType>
where
	ExecutionType: Debug,
{
	fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
		Debug::fmt(&self.inner, f)
	}
}

impl<ExecutionType, StorageType, OtherExecutionType, OtherStorageType>
	PartialEq<StorageCodecWrapper<OtherExecutionType, OtherStorageType>>
	for StorageCodecWrapper<ExecutionType, StorageType>
where
	ExecutionType: PartialEq<OtherExecutionType>,
{
	fn eq(&self, other: &StorageCodecWrapper<OtherExecutionType, OtherStorageType>) -> bool {
		<ExecutionType as PartialEq<OtherExecutionType>>::eq(&self.inner, &other.inner)
	}

	// We skip the clippy `partialeq_ne_impl` lint here as we want to forward everything from this
	// trait to the inner type.
	#[allow(clippy::partialeq_ne_impl)]
	fn ne(&self, other: &StorageCodecWrapper<OtherExecutionType, OtherStorageType>) -> bool {
		<ExecutionType as PartialEq<OtherExecutionType>>::ne(&self.inner, &other.inner)
	}
}

impl<ExecutionType, StorageType> Eq for StorageCodecWrapper<ExecutionType, StorageType> where
	ExecutionType: Eq
{
}

impl<ExecutionType, StorageType, OtherExecutionType, OtherStorageType>
	PartialOrd<StorageCodecWrapper<OtherExecutionType, OtherStorageType>>
	for StorageCodecWrapper<ExecutionType, StorageType>
where
	ExecutionType: PartialOrd<OtherExecutionType>,
{
	fn partial_cmp(
		&self,
		other: &StorageCodecWrapper<OtherExecutionType, OtherStorageType>,
	) -> Option<Ordering> {
		<ExecutionType as PartialOrd<OtherExecutionType>>::partial_cmp(&self.inner, &other.inner)
	}

	fn lt(&self, other: &StorageCodecWrapper<OtherExecutionType, OtherStorageType>) -> bool {
		<ExecutionType as PartialOrd<OtherExecutionType>>::lt(&self.inner, &other.inner)
	}

	fn le(&self, other: &StorageCodecWrapper<OtherExecutionType, OtherStorageType>) -> bool {
		<ExecutionType as PartialOrd<OtherExecutionType>>::le(&self.inner, &other.inner)
	}

	fn gt(&self, other: &StorageCodecWrapper<OtherExecutionType, OtherStorageType>) -> bool {
		<ExecutionType as PartialOrd<OtherExecutionType>>::gt(&self.inner, &other.inner)
	}

	fn ge(&self, other: &StorageCodecWrapper<OtherExecutionType, OtherStorageType>) -> bool {
		<ExecutionType as PartialOrd<OtherExecutionType>>::ge(&self.inner, &other.inner)
	}
}

impl<ExecutionType, StorageType> Ord for StorageCodecWrapper<ExecutionType, StorageType>
where
	ExecutionType: Ord,
{
	fn cmp(&self, other: &Self) -> Ordering {
		<ExecutionType as Ord>::cmp(&self.inner, &other.inner)
	}

	fn max(self, other: Self) -> Self
	where
		Self: Sized,
	{
		match <ExecutionType as Ord>::cmp(&self.inner, &other.inner) {
			Ordering::Less => other,
			Ordering::Equal | Ordering::Greater => self,
		}
	}

	fn min(self, other: Self) -> Self
	where
		Self: Sized,
	{
		match <ExecutionType as Ord>::cmp(&self.inner, &other.inner) {
			Ordering::Greater => other,
			Ordering::Less | Ordering::Equal => self,
		}
	}

	fn clamp(self, min: Self, max: Self) -> Self
	where
		Self: Sized,
	{
		assert!(min <= max);
		if self < min {
			min
		} else if self > max {
			max
		} else {
			self
		}
	}
}

impl<ExecutionType, StorageType> Deref for StorageCodecWrapper<ExecutionType, StorageType> {
	type Target = ExecutionType;

	fn deref(&self) -> &Self::Target {
		&self.inner
	}
}

impl<ExecutionType, StorageType> Encode for StorageCodecWrapper<ExecutionType, StorageType>
where
	StorageType: Encode,
{
	fn size_hint(&self) -> usize {
		<StorageType as Encode>::size_hint(&self.codable)
	}

	fn encode_to<T: Output + ?Sized>(&self, dest: &mut T) {
		<StorageType as Encode>::encode_to(&self.codable, dest)
	}

	fn encode(&self) -> Vec<u8> {
		<StorageType as Encode>::encode(&self.codable)
	}

	fn using_encoded<R, F: FnOnce(&[u8]) -> R>(&self, f: F) -> R {
		<StorageType as Encode>::using_encoded(&self.codable, f)
	}

	fn encoded_size(&self) -> usize {
		<StorageType as Encode>::encoded_size(&self.codable)
	}
}

impl<ExecutionType, StorageType> MaxEncodedLen for StorageCodecWrapper<ExecutionType, StorageType>
where
	StorageType: MaxEncodedLen,
{
	fn max_encoded_len() -> usize {
		<StorageType as MaxEncodedLen>::max_encoded_len()
	}
}

impl<ExecutionType, StorageType> EncodeLike for StorageCodecWrapper<ExecutionType, StorageType> where
	Self: Encode
{
}

impl<ExecutionType, StorageType> EncodeLike<StorageType>
	for StorageCodecWrapper<ExecutionType, StorageType>
where
	Self: Encode,
	StorageType: Encode,
{
}

impl<ExecutionType, StorageType> EncodeLike<StorageCodecWrapper<ExecutionType, StorageType>>
	for Vec<u8>
where
	Self: EncodeLike<StorageType>,
	StorageType: Encode,
{
}

impl<ExecutionType, StorageType> EncodeLike<StorageCodecWrapper<ExecutionType, StorageType>>
	for &Vec<u8>
where
	Vec<u8>: EncodeLike<StorageType>,
	StorageType: Encode,
{
}

impl<ExecutionType, StorageType> EncodeLike<StorageCodecWrapper<ExecutionType, StorageType>>
	for &[u8]
where
	Vec<u8>: EncodeLike<StorageType>,
	StorageType: Encode,
{
}

impl<ExecutionType, StorageType> EncodeLike<StorageCodecWrapper<ExecutionType, StorageType>>
	for pallet_revive_types::common::Bytes
where
	Self: EncodeLike<StorageType>,
	StorageType: Encode,
{
}

impl<ExecutionType, StorageType> EncodeLike<StorageCodecWrapper<ExecutionType, StorageType>>
	for &pallet_revive_types::common::Bytes
where
	pallet_revive_types::common::Bytes: EncodeLike<StorageType>,
	StorageType: Encode,
{
}

impl<StorageType> From<StorageCodecWrapper<Vec<u8>, StorageType>> for Vec<u8> {
	fn from(value: StorageCodecWrapper<Vec<u8>, StorageType>) -> Self {
		value.into_inner()
	}
}

impl<StorageType> From<StorageCodecWrapper<Vec<u8>, StorageType>> for crate::evm::Bytes {
	fn from(value: StorageCodecWrapper<Vec<u8>, StorageType>) -> Self {
		Vec::<u8>::from(value).into()
	}
}

impl<StorageType> PartialEq<Vec<u8>> for StorageCodecWrapper<Vec<u8>, StorageType> {
	fn eq(&self, other: &Vec<u8>) -> bool {
		self.as_inner() == other
	}
}

impl<StorageType> PartialEq<StorageCodecWrapper<Vec<u8>, StorageType>> for Vec<u8> {
	fn eq(&self, other: &StorageCodecWrapper<Vec<u8>, StorageType>) -> bool {
		self == other.as_inner()
	}
}

impl<ExecutionType, StorageType> Decode for StorageCodecWrapper<ExecutionType, StorageType>
where
	StorageType: Decode + Into<ExecutionType> + Clone,
{
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let codable = <StorageType as Decode>::decode(input)?;
		Ok(Self { inner: codable.clone().into(), codable })
	}

	fn decode_into<I: codec::Input>(
		input: &mut I,
		dst: &mut MaybeUninit<Self>,
	) -> Result<DecodeFinished, codec::Error> {
		let mut codable = MaybeUninit::<StorageType>::uninit();
		<StorageType as Decode>::decode_into(input, &mut codable)?;

		// SAFETY: StorageType has been decoded above.
		let codable = unsafe { codable.assume_init() };

		let this = Self { inner: codable.clone().into(), codable };
		dst.write(this);

		// SAFETY: We've written the decoded value to `dst` so calling this is safe.
		unsafe { Ok(DecodeFinished::assert_decoding_finished()) }
	}

	fn skip<I: codec::Input>(input: &mut I) -> Result<(), codec::Error> {
		<StorageType as Decode>::skip(input)
	}

	fn encoded_fixed_size() -> Option<usize> {
		<StorageType as Decode>::encoded_fixed_size()
	}
}

impl<ExecutionType, StorageType> TypeInfo for StorageCodecWrapper<ExecutionType, StorageType>
where
	StorageType: TypeInfo,
{
	type Identity = <StorageType as TypeInfo>::Identity;

	fn type_info() -> scale_info::Type {
		<StorageType as TypeInfo>::type_info()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use pallet_revive_types::{common::Bytes, storage::PristineCodeV1};

	fn assert_encodes_like_pristine_code_wrapper<T>()
	where
		T: EncodeLike<StorageCodecWrapper<Vec<u8>, PristineCodeV1>>,
	{
	}

	#[test]
	fn raw_byte_types_encode_like_pristine_code_storage_wrapper() {
		// Arrange
		type WrappedPristineCode = StorageCodecWrapper<Vec<u8>, PristineCodeV1>;

		// Act
		assert_encodes_like_pristine_code_wrapper::<Vec<u8>>();
		assert_encodes_like_pristine_code_wrapper::<&Vec<u8>>();
		assert_encodes_like_pristine_code_wrapper::<&[u8]>();
		assert_encodes_like_pristine_code_wrapper::<Bytes>();
		assert_encodes_like_pristine_code_wrapper::<&Bytes>();

		// Assert
		assert_encodes_like_pristine_code_wrapper::<WrappedPristineCode>();
	}
}
