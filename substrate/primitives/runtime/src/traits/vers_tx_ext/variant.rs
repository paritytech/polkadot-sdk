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

//! Implementation of versioned transaction extension pipeline that aggregate a version 0 and
//! other versions.

use crate::{
	generic::ExtensionVersion,
	traits::{
		AsTransactionAuthorizedOrigin, DecodeWithVersion, DispatchInfoOf, DispatchTransaction,
		Dispatchable, PostDispatchInfoOf, TransactionExtension, VersTxExtLine,
		VersTxExtLineMetadataBuilder, VersTxExtLineVersion, VersTxExtLineWeight,
	},
	transaction_validity::TransactionSource,
};
use alloc::vec::Vec;
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::RuntimeDebug;
use sp_weights::Weight;

/// Version 0 of the transaction extension version used to construct the inherited
/// implication for legacy transactions.
const EXTENSION_V0_VERSION: ExtensionVersion = 0;

/// A versioned transaction extension pipeline defined with 2 variants: one for the version 0 and
/// one for other versions.
///
/// The generic `ExtensionOtherVersions` must not re-define a transaction extension pipeline for the
/// version 0, it will be ignored and overwritten by `ExtensionV0`.
/// TODO TODO: find good name. or keep it private anyway.
#[derive(PartialEq, Eq, Clone, RuntimeDebug, TypeInfo)]
pub enum ExtensionVariant<ExtensionV0, ExtensionOtherVersions> {
	/// A transaction extension pipeline for the version 0.
	V0(ExtensionV0),
	/// A transaction extension pipeline for other versions.
	Other(ExtensionOtherVersions),
}

impl<ExtensionV0, ExtensionOtherVersions: VersTxExtLineVersion> VersTxExtLineVersion
	for ExtensionVariant<ExtensionV0, ExtensionOtherVersions>
{
	fn version(&self) -> u8 {
		match self {
			ExtensionVariant::V0(_) => EXTENSION_V0_VERSION,
			ExtensionVariant::Other(ext) => ext.version(),
		}
	}
}

impl<ExtensionV0: Encode, ExtensionOtherVersions: Encode> Encode
	for ExtensionVariant<ExtensionV0, ExtensionOtherVersions>
{
	fn encode(&self) -> Vec<u8> {
		match self {
			ExtensionVariant::V0(ext) => ext.encode(),
			ExtensionVariant::Other(ext) => ext.encode(),
		}
	}
	fn size_hint(&self) -> usize {
		match self {
			ExtensionVariant::V0(ext) => ext.size_hint(),
			ExtensionVariant::Other(ext) => ext.size_hint(),
		}
	}
	fn encode_to<T: codec::Output + ?Sized>(&self, dest: &mut T) {
		match self {
			ExtensionVariant::V0(ext) => ext.encode_to(dest),
			ExtensionVariant::Other(ext) => ext.encode_to(dest),
		}
	}
	fn encoded_size(&self) -> usize {
		match self {
			ExtensionVariant::V0(ext) => ext.encoded_size(),
			ExtensionVariant::Other(ext) => ext.encoded_size(),
		}
	}
	fn using_encoded<R, F: FnOnce(&[u8]) -> R>(&self, f: F) -> R {
		match self {
			ExtensionVariant::V0(ext) => ext.using_encoded(f),
			ExtensionVariant::Other(ext) => ext.using_encoded(f),
		}
	}
}

impl<ExtensionV0: Decode, ExtensionOtherVersions: DecodeWithVersion> DecodeWithVersion
	for ExtensionVariant<ExtensionV0, ExtensionOtherVersions>
{
	fn decode_with_version<I: codec::Input>(
		extension_version: u8,
		input: &mut I,
	) -> Result<Self, codec::Error> {
		match extension_version {
			EXTENSION_V0_VERSION => Ok(ExtensionVariant::V0(Decode::decode(input)?)),
			_ => Ok(ExtensionVariant::Other(DecodeWithVersion::decode_with_version(
				extension_version,
				input,
			)?)),
		}
	}
}

impl<
		Call: Dispatchable + Encode,
		ExtensionV0: TransactionExtension<Call>,
		ExtensionOtherVersions: VersTxExtLine<Call>,
	> VersTxExtLine<Call> for ExtensionVariant<ExtensionV0, ExtensionOtherVersions>
where
	<Call as Dispatchable>::RuntimeOrigin: AsTransactionAuthorizedOrigin,
{
	fn build_metadata(builder: &mut VersTxExtLineMetadataBuilder) {
		ExtensionOtherVersions::build_metadata(builder);
	}
	fn validate_only(
		&self,
		origin: super::DispatchOriginOf<Call>,
		call: &Call,
		info: &DispatchInfoOf<Call>,
		len: usize,
		source: TransactionSource,
	) -> Result<
		crate::transaction_validity::ValidTransaction,
		crate::transaction_validity::TransactionValidityError,
	> {
		match self {
			ExtensionVariant::V0(ext) => ext
				.validate_only(origin, call, info, len, source, EXTENSION_V0_VERSION)
				.map(|x| x.0),
			ExtensionVariant::Other(ext) => ext.validate_only(origin, call, info, len, source),
		}
	}
	fn dispatch_transaction(
		self,
		origin: super::DispatchOriginOf<Call>,
		call: Call,
		info: &DispatchInfoOf<Call>,
		len: usize,
	) -> crate::ApplyExtrinsicResultWithInfo<PostDispatchInfoOf<Call>> {
		match self {
			ExtensionVariant::V0(ext) =>
				ext.dispatch_transaction(origin, call, info, len, EXTENSION_V0_VERSION),
			ExtensionVariant::Other(ext) => ext.dispatch_transaction(origin, call, info, len),
		}
	}
}

impl<
		Call: Dispatchable + Encode,
		ExtensionV0: TransactionExtension<Call>,
		ExtensionOtherVersions: VersTxExtLineWeight<Call>,
	> VersTxExtLineWeight<Call> for ExtensionVariant<ExtensionV0, ExtensionOtherVersions>
{
	fn weight(&self, call: &Call) -> Weight {
		match self {
			ExtensionVariant::V0(ext) => ext.weight(call),
			ExtensionVariant::Other(ext) => ext.weight(call),
		}
	}
}
