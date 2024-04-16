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

//! The [`CheckMetadataHash`] signed extension.
//!
//! The extension

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use frame_support::DebugNoBound;
use frame_system::Config;
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{DispatchInfoOf, SignedExtension},
	transaction_validity::{TransactionValidityError, UnknownTransaction},
};

/// Type that encodes `None` to an empty vec.
pub struct EncodeNoneToEmpty(Option<[u8; 32]>);

impl Encode for EncodeNoneToEmpty {
	fn encode(&self) -> Vec<u8> {
		match self.0 {
			Some(hash) => hash.encode(),
			None => Vec::new(),
		}
	}
}

impl TypeInfo for EncodeNoneToEmpty {
	type Identity = <[u8; 32] as TypeInfo>::Identity;

	fn type_info() -> scale_info::Type {
		<[u8; 32]>::type_info()
	}
}

/// Extension for optionally checking the metadata hash.
///
/// The metadata hash is cryptographical representation of the runtime metadata. This metadata hash
/// is build as described in [RFC78](https://polkadot-fellows.github.io/RFCs/approved/0078-merkleized-metadata.html).
/// This metadata hash should give users the confidence that what they build with an online wallet
/// is the same they are signing with their offline wallet and then applying on chain. To ensure
/// that the online wallet is not tricking the offline wallet into decoding and showing an incorrect
/// extrinsic, the offline wallet will include the metadata hash into the additional signed data and
/// the runtime will then do the same. If the metadata hash doesn't match, the signature
/// verification will fail and thus, the transaction will be rejected. The RFC contains more details
/// on how it works.
///
/// The extension adds one byte (the `mode`) to the size of the extrinsic. This one byte is
/// controlling if the metadata hash should be added to the signed data or not. Mode `0` means that
/// the metadata hash is not added and `1` means that it is added. Further values of `mode` are
/// reserved for future changes.
///
/// The metadata hash is read from the environment variable `RUNTIME_METADATA_HASH`. This
/// environment variable is for example set by the `substrate-wasm-builder` when the feature for
/// generating the metadata hash is enabled. If the environment variable is not set and `mode = 1`
/// is passed, the transaction is rejected with [`UnknownTransaction::CannotLookup`].
#[derive(Encode, Decode, Clone, Eq, PartialEq, TypeInfo, DebugNoBound)]
#[scale_info(skip_type_params(T))]
pub struct CheckMetadataHash<T> {
	_phantom: core::marker::PhantomData<T>,
	mode: u8,
}

impl<T> CheckMetadataHash<T> {
	/// Creates new `SignedExtension` to check metadata hash.
	pub fn new(enable: bool) -> Self {
		Self { _phantom: core::marker::PhantomData, mode: if enable { 1 } else { 0 } }
	}
}

impl<T: Config + Send + Sync> SignedExtension for CheckMetadataHash<T> {
	type AccountId = T::AccountId;
	type Call = <T as Config>::RuntimeCall;
	type AdditionalSigned = EncodeNoneToEmpty;
	type Pre = ();
	const IDENTIFIER: &'static str = "CheckMetadataHash";

	fn additional_signed(&self) -> Result<Self::AdditionalSigned, TransactionValidityError> {
		match self.mode {
			0 => Ok(EncodeNoneToEmpty(None)),
			1 => match option_env!("RUNTIME_METADATA_HASH") {
				Some(hash) => Ok(EncodeNoneToEmpty(Some(array_bytes::hex2array_unchecked(hash)))),
				None => Err(UnknownTransaction::CannotLookup.into()),
			},
			// Unknown `mode`, let's reject it.
			_ => Err(UnknownTransaction::CannotLookup.into()),
		}
	}

	fn pre_dispatch(
		self,
		who: &Self::AccountId,
		call: &Self::Call,
		info: &DispatchInfoOf<Self::Call>,
		len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		self.validate(who, call, info, len).map(|_| ())
	}
}
