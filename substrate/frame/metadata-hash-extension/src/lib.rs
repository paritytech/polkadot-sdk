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

//! The [`CheckMetadataHash`] transaction extension.
//!
//! The extension for optionally checking the metadata hash. For information how it works and what
//! it does exactly, see the docs of [`CheckMetadataHash`].
//!
//! # Integration
//!
//! As any transaction extension you will need to add it to your runtime transaction extensions:
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

use codec::{Decode, Encode};
use frame_support::{pallet_prelude::Weight, DebugNoBound};
use frame_system::Config;
use scale_info::TypeInfo;
use sp_runtime::{
	impl_tx_ext_default,
	traits::TransactionExtension,
	transaction_validity::{TransactionValidityError, UnknownTransaction},
};

#[cfg(test)]
mod tests;

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

const RUNTIME_METADATA: Option<[u8; 32]> = if let Some(hex) = option_env!("RUNTIME_METADATA_HASH") {
	match const_hex::const_decode_to_array(hex.as_bytes()) {
		Ok(hex) => Some(hex),
		Err(_) => panic!(
			"Invalid RUNTIME_METADATA_HASH environment variable: it must be a 32 \
			bytes value in hexadecimal: e.g. 0x123ABCabd...123ABCabc. Upper case or lower case, \
			0x prefix is optional."
		),
	}
} else {
	None
};

impl MetadataHash {
	/// Returns the metadata hash.
	fn hash(&self) -> Option<[u8; 32]> {
		match self {
			Self::FetchFromEnv => RUNTIME_METADATA,
			Self::Custom(hash) => Some(*hash),
		}
	}
}

/// Extension for optionally verifying the metadata hash.
///
/// The metadata hash is cryptographical representation of the runtime metadata. This metadata hash
/// is build as described in [RFC78](https://polkadot-fellows.github.io/RFCs/approved/0078-merkleized-metadata.html).
/// This metadata hash should give users the confidence that what they build with an online wallet
/// is the same they are signing with their offline wallet and then applying on chain. To ensure
/// that the online wallet is not tricking the offline wallet into decoding and showing an incorrect
/// extrinsic, the offline wallet will include the metadata hash into the extension implicit and
/// the runtime will then do the same. If the metadata hash doesn't match, the signature
/// verification will fail and thus, the transaction will be rejected. The RFC contains more details
/// on how it works.
///
/// The extension adds one byte (the `mode`) to the size of the extrinsic. This one byte is
/// controlling if the metadata hash should be added to the implicit or not. Mode `0` means that
/// the metadata hash is not added and thus, `None` is added to the implicit. Mode `1` means that
/// the metadata hash is added and thus, `Some(metadata_hash)` is added to the implicit. Further
/// values of `mode` are reserved for future changes.
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
	/// Creates new `TransactionExtension` to check metadata hash.
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

impl<T: Config + Send + Sync> TransactionExtension<T::RuntimeCall> for CheckMetadataHash<T> {
	const IDENTIFIER: &'static str = "CheckMetadataHash";
	type Implicit = Option<[u8; 32]>;
	fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
		let signed = match self.mode {
			Mode::Disabled => None,
			Mode::Enabled => match self.metadata_hash.hash() {
				Some(hash) => Some(hash),
				None => return Err(UnknownTransaction::CannotLookup.into()),
			},
		};

		log::debug!(
			target: "runtime::metadata-hash",
			"CheckMetadataHash::implicit => {:?}",
			signed.as_ref().map(|h| array_bytes::bytes2hex("0x", h)),
		);

		Ok(signed)
	}
	type Val = ();
	type Pre = ();

	fn weight(&self, _: &T::RuntimeCall) -> Weight {
		// The weight is the weight of implicit, it consists of a few match operation, it is
		// negligible.
		Weight::zero()
	}

	impl_tx_ext_default!(T::RuntimeCall; validate prepare);
}
