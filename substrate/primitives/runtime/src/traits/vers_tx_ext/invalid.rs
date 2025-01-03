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

//! Implementation of versioned transaction extension pipeline that is always invalid.

use crate::{
	traits::{
		DecodeWithVersion, DispatchInfoOf, DispatchOriginOf, Dispatchable, PostDispatchInfoOf,
		VersTxExtLine, VersTxExtLineMetadataBuilder, VersTxExtLineVersion, VersTxExtLineWeight,
	},
	transaction_validity::{
		InvalidTransaction, TransactionSource, TransactionValidityError, ValidTransaction,
	},
};
use codec::Encode;
use core::fmt::Debug;
use scale_info::TypeInfo;
use sp_weights::Weight;

/// An implementation of [`VersTxExtLine`] that consider any version invalid.
#[derive(Encode, Debug, Clone, Eq, PartialEq, TypeInfo)]
pub struct InvalidVersion;

impl DecodeWithVersion for InvalidVersion {
	fn decode_with_version<I: codec::Input>(
		_extension_version: u8,
		_input: &mut I,
	) -> Result<Self, codec::Error> {
		Err(codec::Error::from("Invalid extension version"))
	}
}

impl<Call: Dispatchable> VersTxExtLine<Call> for InvalidVersion {
	fn build_metadata(_builder: &mut VersTxExtLineMetadataBuilder) {
		// Do nothing.
	}
	fn validate_only(
		&self,
		_origin: DispatchOriginOf<Call>,
		_call: &Call,
		_info: &DispatchInfoOf<Call>,
		_len: usize,
		_source: TransactionSource,
	) -> Result<ValidTransaction, TransactionValidityError> {
		Err(TransactionValidityError::Invalid(InvalidTransaction::Custom(0)))
	}
	fn dispatch_transaction(
		self,
		_origin: DispatchOriginOf<Call>,
		_call: Call,
		_info: &DispatchInfoOf<Call>,
		_len: usize,
	) -> crate::ApplyExtrinsicResultWithInfo<PostDispatchInfoOf<Call>> {
		Err(TransactionValidityError::Invalid(InvalidTransaction::Custom(0)).into())
	}
}

impl VersTxExtLineVersion for InvalidVersion {
	fn version(&self) -> u8 {
		0
	}
}

impl<Call: Dispatchable> VersTxExtLineWeight<Call> for InvalidVersion {
	fn weight(&self, _call: &Call) -> Weight {
		Weight::zero()
	}
}
