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

//! Type to define a versioned transaction extension pipeline for a specific version.

use crate::{
	traits::{
		AsTransactionAuthorizedOrigin, DecodeWithVersion, DispatchInfoOf, DispatchOriginOf,
		DispatchTransaction, Dispatchable, PostDispatchInfoOf, TransactionExtension, VersTxExtLine,
		VersTxExtLineMetadataBuilder, VersTxExtLineVersion, VersTxExtLineWeight,
	},
	transaction_validity::{TransactionSource, TransactionValidityError, ValidTransaction},
};
use codec::{Decode, Encode};
use core::fmt::Debug;
use scale_info::TypeInfo;
use sp_weights::Weight;

/// A transaction extension pipeline defined for a single version.
#[derive(Encode, Clone, Debug, TypeInfo)]
pub struct TxExtLineAtVers<const VERSION: u8, Extension> {
	/// The transaction extension pipeline for the version `VERSION`.
	pub extension: Extension,
}

impl<const VERSION: u8, Extension> TxExtLineAtVers<VERSION, Extension> {
	/// Create a new versioned extension.
	pub fn new(extension: Extension) -> Self {
		Self { extension }
	}
}

impl<const VERSION: u8, Extension: Decode> DecodeWithVersion
	for TxExtLineAtVers<VERSION, Extension>
{
	fn decode_with_version<I: codec::Input>(
		extension_version: u8,
		input: &mut I,
	) -> Result<Self, codec::Error> {
		if extension_version == VERSION {
			Ok(TxExtLineAtVers { extension: Extension::decode(input)? })
		} else {
			Err(codec::Error::from("Invalid extension version"))
		}
	}
}

impl<const VERSION: u8, Extension> VersTxExtLineVersion for TxExtLineAtVers<VERSION, Extension> {
	fn version(&self) -> u8 {
		VERSION
	}
}

impl<
		const VERSION: u8,
		Call: Dispatchable<RuntimeOrigin: AsTransactionAuthorizedOrigin> + Encode,
		Extension: TransactionExtension<Call>,
	> VersTxExtLine<Call> for TxExtLineAtVers<VERSION, Extension>
{
	fn build_metadata(builder: &mut VersTxExtLineMetadataBuilder) {
		builder.push_versioned_extension(VERSION, Extension::metadata());
	}
	fn validate_only(
		&self,
		origin: DispatchOriginOf<Call>,
		call: &Call,
		info: &DispatchInfoOf<Call>,
		len: usize,
		source: TransactionSource,
	) -> Result<ValidTransaction, TransactionValidityError> {
		self.extension
			.validate_only(origin, call, info, len, source, VERSION)
			.map(|x| x.0)
	}
	fn dispatch_transaction(
		self,
		origin: DispatchOriginOf<Call>,
		call: Call,
		info: &DispatchInfoOf<Call>,
		len: usize,
	) -> crate::ApplyExtrinsicResultWithInfo<PostDispatchInfoOf<Call>> {
		self.extension.dispatch_transaction(origin, call, info, len, VERSION)
	}
}

impl<const VERSION: u8, Call: Dispatchable, Extension: TransactionExtension<Call>>
	VersTxExtLineWeight<Call> for TxExtLineAtVers<VERSION, Extension>
{
	fn weight(&self, call: &Call) -> Weight {
		self.extension.weight(call)
	}
}
