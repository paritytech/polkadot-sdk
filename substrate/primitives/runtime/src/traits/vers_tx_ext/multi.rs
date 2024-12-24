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

//! Types and trait to aggregate multiple versioned transaction extension pipelines.

use crate::{
	traits::{
		DecodeWithVersion, DispatchInfoOf, DispatchOriginOf, Dispatchable, InvalidVersion,
		PostDispatchInfoOf, TxExtLineAtVers, VersTxExtLine, VersTxExtLineMetadataBuilder,
		VersTxExtLineVersion, VersTxExtLineWeight,
	},
	transaction_validity::{TransactionSource, TransactionValidityError, ValidTransaction},
};
use alloc::vec::Vec;
use codec::Encode;
use core::fmt::Debug;
use scale_info::TypeInfo;
use sp_weights::Weight;

/// An item in [`MultiVersion`]. It represents a transaction extension pipeline of a specific
/// single version.
pub trait MultiVersionItem {
	/// The version of the transaction extension pipeline.
	const VERSION: Option<u8>;
}

impl MultiVersionItem for InvalidVersion {
	const VERSION: Option<u8> = None;
}

impl<const VERSION: u8, Extension> MultiVersionItem for TxExtLineAtVers<VERSION, Extension> {
	const VERSION: Option<u8> = Some(VERSION);
}

/// An implementation of [`VersTxExtLine`] that aggregated multiple transaction extension pipeline
/// of different versions.
///
/// Each variant have its own version, duplicated version must be avoided, only the first used
/// version will be effective other duplicated version will be ignored.
///
/// TODO TODO: example
#[allow(private_interfaces)]
#[derive(Clone, Debug, TypeInfo)]
pub enum MultiVersion<A, B = InvalidVersion> {
	/// The first aggregated transaction extension pipeline of a specific version.
	A(A),
	/// The second aggregated transaction extension pipeline of a specific version.
	B(B),
}

impl<A: VersTxExtLineVersion, B: VersTxExtLineVersion> VersTxExtLineVersion for MultiVersion<A, B> {
	fn version(&self) -> u8 {
		match self {
			MultiVersion::A(a) => a.version(),
			MultiVersion::B(b) => b.version(),
		}
	}
}

impl<A: Encode, B: Encode> Encode for MultiVersion<A, B> {
	fn size_hint(&self) -> usize {
		match self {
			MultiVersion::A(a) => a.size_hint(),
			MultiVersion::B(b) => b.size_hint(),
		}
	}
	fn encode(&self) -> Vec<u8> {
		match self {
			MultiVersion::A(a) => a.encode(),
			MultiVersion::B(b) => b.encode(),
		}
	}
	fn encode_to<T: codec::Output + ?Sized>(&self, dest: &mut T) {
		match self {
			MultiVersion::A(a) => a.encode_to(dest),
			MultiVersion::B(b) => b.encode_to(dest),
		}
	}
	fn encoded_size(&self) -> usize {
		match self {
			MultiVersion::A(a) => a.encoded_size(),
			MultiVersion::B(b) => b.encoded_size(),
		}
	}
	fn using_encoded<R, F: FnOnce(&[u8]) -> R>(&self, f: F) -> R {
		match self {
			MultiVersion::A(a) => a.using_encoded(f),
			MultiVersion::B(b) => b.using_encoded(f),
		}
	}
}

impl<A: DecodeWithVersion + MultiVersionItem, B: DecodeWithVersion + MultiVersionItem>
	DecodeWithVersion for MultiVersion<A, B>
{
	fn decode_with_version<I: codec::Input>(
		extension_version: u8,
		input: &mut I,
	) -> Result<Self, codec::Error> {
		if A::VERSION == Some(extension_version) {
			Ok(MultiVersion::A(A::decode_with_version(extension_version, input)?))
		} else if B::VERSION == Some(extension_version) {
			Ok(MultiVersion::B(B::decode_with_version(extension_version, input)?))
		} else {
			Err(codec::Error::from("Invalid extension version"))
		}
	}
}

impl<A, B, Call: Dispatchable> VersTxExtLineWeight<Call> for MultiVersion<A, B>
where
	A: VersTxExtLineWeight<Call> + MultiVersionItem,
	B: VersTxExtLineWeight<Call> + MultiVersionItem,
{
	fn weight(&self, call: &Call) -> Weight {
		match self {
			MultiVersion::A(a) => a.weight(call),
			MultiVersion::B(b) => b.weight(call),
		}
	}
}

impl<A, B, Call: Dispatchable> VersTxExtLine<Call> for MultiVersion<A, B>
where
	A: VersTxExtLine<Call> + MultiVersionItem,
	B: VersTxExtLine<Call> + MultiVersionItem,
{
	fn build_metadata(builder: &mut VersTxExtLineMetadataBuilder) {
		A::build_metadata(builder);
		B::build_metadata(builder);
	}
	fn validate_only(
		&self,
		origin: DispatchOriginOf<Call>,
		call: &Call,
		info: &DispatchInfoOf<Call>,
		len: usize,
		source: TransactionSource,
	) -> Result<ValidTransaction, TransactionValidityError> {
		match self {
			MultiVersion::A(a) => a.validate_only(origin, call, info, len, source),
			MultiVersion::B(b) => b.validate_only(origin, call, info, len, source),
		}
	}
	fn dispatch_transaction(
		self,
		origin: DispatchOriginOf<Call>,
		call: Call,
		info: &DispatchInfoOf<Call>,
		len: usize,
	) -> crate::ApplyExtrinsicResultWithInfo<PostDispatchInfoOf<Call>> {
		match self {
			MultiVersion::A(a) => a.dispatch_transaction(origin, call, info, len),
			MultiVersion::B(b) => b.dispatch_transaction(origin, call, info, len),
		}
	}
}
