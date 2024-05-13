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

#![cfg_attr(not(feature = "std"), no_std)]

//! The [`CheckMetadataHash`] signed extension.
//!
//! The extension for optionally checking the metadata hash. For information how it works and what
//! it does exactly, see the docs of [`CheckMetadataHash`].
//!
//! # Integration
//!
//! As any signed extension you will need to add it to your runtime signed extensions:
#![doc = docify::embed!("src/tests.rs", add_metadata_hash_extension)]
//! As the extension requires the `RUNTIME_METADATA_HASH` environment variable to be present at
//! compile time, it requires a little bit more setup. To have this environment variable available
//! at compile time required to tell the `substrate-wasm-builder` to do so:
#![doc = docify::embed!("src/tests.rs", enable_metadata_hash_in_wasm_builder)]
//! As generating the metadata hash requires to compile the runtime twice, it is
//! recommended to only enable the metadata hash generation when doing a build for a release or when
//! you want to test this feature.

extern crate alloc;
/// For our tests
extern crate self as frame_metadata_hash_extension;

use alloc::vec::Vec;
use codec::{Decode, Encode};
use frame_support::DebugNoBound;
use frame_system::Config;
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{DispatchInfoOf, SignedExtension},
	transaction_validity::{TransactionValidityError, UnknownTransaction},
};

#[cfg(test)]
mod tests;

/// Type that encodes `None` to an empty vec.
#[derive(Debug, PartialEq)]
pub struct EncodeNoneToEmpty(pub Option<[u8; 32]>);

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

/// The mode of [`CheckMetadataHash`].
#[derive(Decode, Encode, PartialEq, Debug, TypeInfo, Clone, Copy, Eq)]
enum Mode {
	Disabled,
	Enabled,
}

/// Wrapper around the metadata hash and from where to get it from.
#[derive(Default, Debug, PartialEq, Clone, Copy, Eq)]
enum MetadataHash {
	/// Fetch it from the `RUNTIME_METADATA_HASH` env variable at compile time.
	#[default]
	FetchFromEnv,
	/// Use the given metadata hash.
	Custom([u8; 32]),
}

impl MetadataHash {
	/// Returns the metadata hash.
	fn hash(&self) -> Option<[u8; 32]> {
		match self {
			Self::FetchFromEnv =>
				option_env!("RUNTIME_METADATA_HASH").map(array_bytes::hex2array_unchecked),
			Self::Custom(hash) => Some(*hash),
		}
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
	mode: Mode,
	#[codec(skip)]
	metadata_hash: MetadataHash,
}

impl<T> CheckMetadataHash<T> {
	/// Creates new `SignedExtension` to check metadata hash.
	pub fn new(enable: bool) -> Self {
		Self {
			_phantom: core::marker::PhantomData,
			mode: if enable { Mode::Enabled } else { Mode::Disabled },
			metadata_hash: MetadataHash::FetchFromEnv,
		}
	}

	/// Create an instance that uses the given `metadata_hash`.
	///
	/// This is useful for testing the extension.
	pub fn new_with_custom_hash(metadata_hash: [u8; 32]) -> Self {
		Self {
			_phantom: core::marker::PhantomData,
			mode: Mode::Enabled,
			metadata_hash: MetadataHash::Custom(metadata_hash),
		}
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
			Mode::Disabled => Ok(EncodeNoneToEmpty(None)),
			Mode::Enabled => match self.metadata_hash.hash() {
				Some(hash) => Ok(EncodeNoneToEmpty(Some(hash))),
				None => Err(UnknownTransaction::CannotLookup.into()),
			},
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
