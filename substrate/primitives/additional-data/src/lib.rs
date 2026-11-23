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

//! Per-block additional data: a keyed blob carried next to the block body and committed to by a
//! single header digest.
//!
//! The crate is producer-agnostic. While a block is built, a producer fills one key of the
//! [`AdditionalData`] map and registers an [`AdditionalDataFinalizer`] in [`AdditionalDataExt`].
//! At the end of the block the runtime calls [`finalize`], which folds every registered
//! commitment through [`hash_commitments`] into the value deposited as
//! `DigestItem::AdditionalData`. Code holding a carried map recomputes that value with
//! [`hash_additional_data`].

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{collections::btree_map::BTreeMap, string::String, sync::Arc, vec::Vec};
use codec::Encode;
use sp_crypto_hashing::blake2_256;
use sp_runtime_interface::{pass_by::PassPointerAndWrite, runtime_interface};

#[cfg(feature = "std")]
use alloc::boxed::Box;
#[cfg(feature = "std")]
use sp_externalities::ExternalitiesExt;

/// A block's additional data, keyed by producer.
pub type AdditionalData = BTreeMap<String, Vec<u8>>;

/// The commitment to one producer's value: blake2-256 of its bytes.
pub fn hash_value(value: &[u8]) -> [u8; 32] {
	blake2_256(value)
}

/// Hash the commitments, in the given order; `None` when there are none.
///
/// Pass them in map key order, so build and import produce the same digest.
pub fn hash_commitments(commitments: impl IntoIterator<Item = [u8; 32]>) -> Option<[u8; 32]> {
	let mut encoded = Vec::new();
	commitments.into_iter().for_each(|c| c.encode_to(&mut encoded));
	(!encoded.is_empty()).then(|| blake2_256(&encoded))
}

/// The digest value of a carried map, equal to what [`AdditionalDataExt::finalize`] produced when
/// the block was built.
pub fn hash_additional_data(data: &AdditionalData) -> Option<[u8; 32]> {
	hash_commitments(data.values().map(|value| hash_value(value)))
}

/// One producer's commitment to the value it carries in the current block.
pub trait AdditionalDataFinalizer: Send {
	/// The commitment, or `None` when the producer recorded nothing. Must be idempotent.
	fn finalize(&self) -> Option<[u8; 32]>;
}

impl<T: AdditionalDataFinalizer + Sync + ?Sized> AdditionalDataFinalizer for Arc<T> {
	fn finalize(&self) -> Option<[u8; 32]> {
		(**self).finalize()
	}
}

#[cfg(feature = "std")]
sp_externalities::decl_extension! {
	/// Externalities extension holding the registered finalizers, keyed by producer.
	///
	/// Register it before executing a block whose runtime produces additional data.
	pub struct AdditionalDataExt(BTreeMap<String, Box<dyn AdditionalDataFinalizer>>);
}

#[cfg(feature = "std")]
impl AdditionalDataExt {
	/// Fold the registered finalizers' commitments in key order, or `None` when nothing was
	/// recorded.
	pub fn finalize(&self) -> Option<[u8; 32]> {
		hash_commitments(self.0.values().filter_map(|finalizer| finalizer.finalize()))
	}
}

/// Host side of committing a block's additional data.
#[runtime_interface]
pub trait AdditionalDataHost {
	/// Write the digest value into `hash_out` and return whether there was anything to commit.
	/// A missing extension counts as nothing recorded.
	fn finalize_into(&mut self, hash_out: PassPointerAndWrite<&mut [u8; 32], 32>) -> bool {
		match self.extension::<AdditionalDataExt>().and_then(|ext| ext.finalize()) {
			Some(hash) => {
				*hash_out = hash;
				true
			},
			None => false,
		}
	}
}

/// The digest value of everything recorded this block, or `None` when nothing was recorded.
pub fn finalize() -> Option<[u8; 32]> {
	let mut out = [0u8; 32];
	additional_data_host::finalize_into(&mut out).then_some(out)
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_io::TestExternalities;

	struct Fixed(Option<[u8; 32]>);

	impl AdditionalDataFinalizer for Fixed {
		fn finalize(&self) -> Option<[u8; 32]> {
			self.0
		}
	}

	fn ext_from(entries: &[(&str, Option<[u8; 32]>)]) -> AdditionalDataExt {
		AdditionalDataExt(
			entries
				.iter()
				.map(|(key, commitment)| {
					(
						key.to_string(),
						Box::new(Fixed(*commitment)) as Box<dyn AdditionalDataFinalizer>,
					)
				})
				.collect(),
		)
	}

	#[test]
	fn hash_commitments_hashes_the_concatenation() {
		let a = hash_value(b"a");
		let b = hash_value(b"b");
		assert_eq!(hash_commitments([a, b]), Some(blake2_256(&[a, b].concat())));
		assert_eq!(hash_commitments([a, b]), hash_commitments([a, b]));
		assert_ne!(hash_commitments([a, b]), hash_commitments([b, a]));
		assert_ne!(hash_commitments([a]), hash_commitments([a, a]));
	}

	#[test]
	fn empty_set_has_no_commitment() {
		assert_eq!(hash_commitments(core::iter::empty()), None);
		assert_eq!(hash_additional_data(&AdditionalData::new()), None);
		assert_eq!(ext_from(&[]).finalize(), None);
		assert_eq!(ext_from(&[("a", None), ("b", None)]).finalize(), None);
	}

	#[test]
	fn finalizers_fold_in_key_order() {
		let a = [1u8; 32];
		let b = [2u8; 32];
		let ext = ext_from(&[("b", Some(b)), ("a", Some(a))]);
		assert_eq!(ext.finalize(), hash_commitments([a, b]));
		assert_eq!(ext.finalize(), ext.finalize());
	}

	#[test]
	fn silent_finalizers_are_skipped() {
		let a = [1u8; 32];
		let ext = ext_from(&[("a", Some(a)), ("b", None)]);
		assert_eq!(ext.finalize(), hash_commitments([a]));
	}

	#[test]
	fn shared_finalizer_delegates() {
		let shared = Arc::new(Fixed(Some([3u8; 32])));
		let ext = AdditionalDataExt(
			[("a".to_string(), Box::new(shared.clone()) as Box<dyn AdditionalDataFinalizer>)]
				.into_iter()
				.collect(),
		);
		assert_eq!(ext.finalize(), hash_commitments([[3u8; 32]]));
	}

	#[test]
	fn carried_map_replays_the_build_commitment() {
		let data: AdditionalData =
			[("x".to_string(), b"xx".to_vec()), ("y".to_string(), b"yyy".to_vec())]
				.into_iter()
				.collect();
		let built = AdditionalDataExt(
			data.iter()
				.map(|(key, value)| {
					(
						key.clone(),
						Box::new(Fixed(Some(hash_value(value))))
							as Box<dyn AdditionalDataFinalizer>,
					)
				})
				.collect(),
		);
		assert!(built.finalize().is_some());
		assert_eq!(hash_additional_data(&data), built.finalize());

		let mut corrupted = data.clone();
		corrupted.insert("x".to_string(), b"xy".to_vec());
		assert_ne!(hash_additional_data(&corrupted), built.finalize());
	}

	#[test]
	fn host_finalize_without_extension_commits_nothing() {
		TestExternalities::default().execute_with(|| {
			assert_eq!(finalize(), None);
		});
	}

	#[test]
	fn host_finalize_folds_the_registered_extension() {
		let a = [1u8; 32];
		let b = [2u8; 32];
		let mut t = TestExternalities::default();
		t.register_extension(ext_from(&[("b", Some(b)), ("a", Some(a))]));
		t.execute_with(|| {
			assert_eq!(finalize(), hash_commitments([a, b]));
			assert_eq!(finalize(), hash_commitments([a, b]));
		});
	}
}
