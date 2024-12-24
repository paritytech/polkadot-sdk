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

//! The traits and primitive types for versioned transaction extension pipelines.

use crate::{
	scale_info::StaticTypeInfo,
	traits::{
		DispatchInfoOf, DispatchOriginOf, Dispatchable, PostDispatchInfoOf,
		TransactionExtensionMetadata,
	},
	transaction_validity::{TransactionSource, TransactionValidityError, ValidTransaction},
};
use alloc::vec::Vec;
use codec::Encode;
use core::fmt::Debug;
use sp_weights::Weight;

mod at_vers;
mod invalid;
mod multi;
mod variant;
pub use at_vers::*;
pub use invalid::*;
pub use multi::*;
pub use variant::*;

/// The weight for an instance of a versioned transaction extension pipeline and a call.
///
/// This trait is part of [`VersTxExtLine`]. It is defined independently to allow implementation to
/// rely only on it without bounding the whole trait [`VersTxExtLine`]. This is used by
/// [`crate::generic::UncheckedExtrinsic`] to be backward compatible with its previous version.
pub trait VersTxExtLineWeight<Call: Dispatchable> {
	/// Return the pre dispatch weight for the given versioned transaction extension pipeline and
	/// call.
	fn weight(&self, call: &Call) -> Weight;
}

/// The version for an instance of a versioned transaction extension pipeline.
///
/// This trait is part of [`VersTxExtLine`]. It is defined independently to allow implementation to
/// rely only on it without bounding the whole trait [`VersTxExtLine`]. This is used by
/// [`crate::generic::UncheckedExtrinsic`] to be backward compatible with its previous version.
pub trait VersTxExtLineVersion {
	/// Return the version for the given versioned transaction extension pipeline.
	fn version(&self) -> u8;
}

/// A versioned transaction extension pipeline.
///
/// This defines multiple version of a transaction extensions pipeline.
pub trait VersTxExtLine<Call: Dispatchable>:
	Encode
	+ DecodeWithVersion
	+ Debug
	+ StaticTypeInfo
	+ Send
	+ Sync
	+ Clone
	+ VersTxExtLineWeight<Call>
	+ VersTxExtLineVersion
{
	/// Build the metadata for the versioned transaction extension pipeline.
	fn build_metadata(builder: &mut VersTxExtLineMetadataBuilder);

	/// Validate a transaction.
	fn validate_only(
		&self,
		origin: DispatchOriginOf<Call>,
		call: &Call,
		info: &DispatchInfoOf<Call>,
		len: usize,
		source: TransactionSource,
	) -> Result<ValidTransaction, TransactionValidityError>;

	/// Dispatch a transaction.
	fn dispatch_transaction(
		self,
		origin: DispatchOriginOf<Call>,
		call: Call,
		info: &DispatchInfoOf<Call>,
		len: usize,
	) -> crate::ApplyExtrinsicResultWithInfo<PostDispatchInfoOf<Call>>;
}

/// A type that can be decoded from a specific version and a [`codec::Input`].
pub trait DecodeWithVersion: Sized {
	/// Decode the type from the given version and input.
	fn decode_with_version<I: codec::Input>(
		extension_version: u8,
		input: &mut I,
	) -> Result<Self, codec::Error>;
}

/// A type to build the metadata for the versioned transaction extension pipeline.
pub struct VersTxExtLineMetadataBuilder {
	/// The transaction extension pipeline by version and its list of items as vec of index into
	/// other field `in_versions`.
	pub by_version: Vec<(u8, Vec<u32>)>,
	/// The list of all transaction extension item used.
	pub in_versions: Vec<TransactionExtensionMetadata>,
}

impl VersTxExtLineMetadataBuilder {
	/// Create a new empty metadata builder.
	pub fn new() -> Self {
		Self { by_version: Vec::new(), in_versions: Vec::new() }
	}

	/// A function to add a versioned transaction extension to the metadata builder.
	pub fn push_versioned_extension(
		&mut self,
		ext_version: u8,
		ext_items: Vec<TransactionExtensionMetadata>,
	) {
		debug_assert!(
			self.by_version.iter().all(|(v, _)| *v != ext_version),
			"Duplicate definition for transaction extension version: {}",
			ext_version
		);

		let mut ext_item_indices = Vec::with_capacity(ext_items.len());
		for ext_item in ext_items {
			let ext_item_index =
				match self.in_versions.iter().position(|ext| ext.identifier == ext_item.identifier)
				{
					Some(index) => index,
					None => {
						self.in_versions.push(ext_item);
						self.in_versions.len() - 1
					},
				};
			ext_item_indices.push(ext_item_index as u32);
		}
		self.by_version.push((ext_version, ext_item_indices));
	}
}
