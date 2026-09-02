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

//! A block's "additional data": a generic, keyed channel for data produced during block execution
//! and carried alongside the block, committed to by a single header digest.
//!
//! # Model
//!
//! Each block may carry an [`AdditionalData`] — a `BTreeMap<String, Vec<u8>>` mapping a namespaced
//! producer key to that producer's opaque bytes. This crate is deliberately generic: it knows
//! nothing about any particular producer (e.g. the relay-state reader lives in
//! `cumulus-primitives-additional-data`). It provides only:
//!
//! - the [`AdditionalData`] type and the per-value commitment [`hash_value`],
//! - the [`AdditionalDataFinalizer`] trait — one commitment per producer,
//! - the [`AdditionalDataExt`] externalities extension — a registry of finalizers keyed by a stable
//!   string identifier,
//! - the [`additional_data::finalize`] host function, which folds the registered finalizers' sub
//!   hashes into the 32-byte value a runtime deposits as `DigestItem::AdditionalData`.
//!
//! # Digest
//!
//! The digest is the blake2-256 of the SCALE-encoded `Vec` of per-finalizer sub-hashes, in
//! [`AdditionalDataExt`]'s (`BTreeMap`-key-ordered, hence deterministic) iteration order — see
//! [`hash_commitments`]. Each finalizer is responsible for its own commitment and for any
//! producer-specific validation it performs while computing it. The identifier a value is carried
//! under is not part of the digest.
//!
//! [`additional_data::finalize`] returns `None` when no [`AdditionalDataExt`] is registered (or it
//! is empty) — it runs from generic block finalization on every block, including contexts that
//! never produced any additional data, so a missing extension there means "nothing was recorded",
//! not an error.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{boxed::Box, collections::btree_map::BTreeMap, string::String, sync::Arc, vec::Vec};
use codec::Encode;
use sp_crypto_hashing::blake2_256;
use sp_runtime_interface::{pass_by::PassFatPointerAndWrite, runtime_interface};

#[cfg(feature = "std")]
use sp_externalities::{Extension, ExternalitiesExt};

/// A block's additional data: a map from a namespaced producer key to that producer's opaque bytes.
///
/// Carried per block through sync/DB/import; at the PoV level a `Vec` of these (one per bundled
/// block). `BTreeMap` gives a canonical (sorted-key) encoding, so hashing it is deterministic.
pub type AdditionalData = BTreeMap<String, Vec<u8>>;

/// The commitment to a single producer's value: blake2-256 of its raw bytes.
///
/// This is folded into the additional-data digest; it is the one commitment definition, the default
/// sub-hash an [`AdditionalDataFinalizer`] returns for the bytes it carries.
pub fn hash_value(value: &[u8]) -> [u8; 32] {
	blake2_256(value)
}

/// Combine a block's per-producer commitments into its single additional-data digest: blake2-256 of
/// the SCALE-encoded `Vec` of sub-hashes, or `None` for an empty set (nothing was recorded).
///
/// This is THE digest definition, reached from both directions: [`AdditionalDataExt::finalize`]
/// folds the registered finalizers' commitments through it, while code holding a carried
/// [`AdditionalData`] map folds [`hash_value`] of each entry through it (the PVF integrity check
/// and the non-executing import path). Callers must supply the commitments in a deterministic order
/// — `BTreeMap` iteration order for both directions. Producer identifiers (the map keys) are
/// deliberately not part of the digest.
pub fn hash_commitments(commitments: impl IntoIterator<Item = [u8; 32]>) -> Option<[u8; 32]> {
	let commitments: Vec<[u8; 32]> = commitments.into_iter().collect();
	(!commitments.is_empty()).then(|| blake2_256(&commitments.encode()))
}

/// A single producer's commitment to the value it carries in a block's additional data.
///
/// One finalizer is registered per producer in an [`AdditionalDataExt`], under the same string
/// identifier that namespaces the producer's entry in the [`AdditionalData`] map. Its
/// [`finalize`](AdditionalDataFinalizer::finalize) returns the 32-byte commitment (the default
/// being [`hash_value`] of the producer's bytes) folded into the block's additional-data digest;
/// `None` when the producer recorded nothing. The identifier orders the fold but is not itself
/// committed to.
pub trait AdditionalDataFinalizer: Send {
	/// This producer's 32-byte commitment for the current block, or `None` when it recorded
	/// nothing. MUST be idempotent.
	fn finalize(&self) -> Option<[u8; 32]>;
}

/// Lets a shared provider register as a finalizer: the build/import side wraps its (only-`Send`)
/// provider in an `Arc` (of a `Sync` cell) and registers a clone under `AdditionalDataExt` while
/// the same object serves reads under the producer's read extension.
impl<T: AdditionalDataFinalizer + Sync + ?Sized> AdditionalDataFinalizer for Arc<T> {
	fn finalize(&self) -> Option<[u8; 32]> {
		(**self).finalize()
	}
}

/// A getter for the additional-data map recorded while building a block.
///
/// The build-side recorder is moved into an externalities extension and consumed by the proposer,
/// so the collator keeps one of these — a closure sharing the same recorder — to retrieve the
/// recorded map after the block is built. Backend-free (hence `Send`) and idempotent.
#[cfg(feature = "std")]
pub type AdditionalDataGetter = Box<dyn Fn() -> Option<AdditionalData> + Send>;

#[cfg(feature = "std")]
sp_externalities::decl_extension! {
	/// Externalities extension: a registry of [`AdditionalDataFinalizer`]s keyed by producer
	/// identifier.
	///
	/// Register this before executing a block whose runtime produces additional data — on build, on
	/// `validate_block`, and on the generic block-import path. Producers register their own read
	/// extensions separately; this one exists purely to drive [`additional_data::finalize`].
	pub struct AdditionalDataExt(BTreeMap<String, Box<dyn AdditionalDataFinalizer>>);
}

#[cfg(feature = "std")]
impl AdditionalDataExt {
	/// Fold every registered finalizer's sub-hash into the block's additional-data digest via
	/// [`hash_commitments`]. `None` when no finalizer recorded anything. Iteration is over the
	/// `BTreeMap`, so the order is deterministic.
	pub fn finalize(&self) -> Option<[u8; 32]> {
		hash_commitments(self.0.values().filter_map(|f| f.finalize()))
	}
}

/// Builds the externalities extensions a carried [`AdditionalData`] map needs for re-execution.
///
/// Injected into the block importer (via `ClientConfig`) so the concrete, producer-specific
/// extensions (e.g. a relay-state reader plus its [`AdditionalDataFinalizer`]) can live outside
/// substrate: the importer registers whatever type-erased extensions this factory returns instead
/// of naming concrete types. Returns `None` when the carried data is malformed / cannot be turned
/// into the required extensions.
#[cfg(feature = "std")]
#[derive(Clone)]
pub struct CreateAdditionalDataExtensions(
	pub Arc<dyn Fn(&AdditionalData) -> Option<Vec<Box<dyn Extension>>> + Send + Sync>,
);

#[cfg(feature = "std")]
impl core::fmt::Debug for CreateAdditionalDataExtensions {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.write_str("CreateAdditionalDataExtensions")
	}
}

/// Runtime interface for committing a block's additional data.
///
/// `finalize` runs from generic block finalization on every block and returns `None` when no
/// [`AdditionalDataExt`] is registered (nothing was recorded).
#[runtime_interface]
pub trait AdditionalData {
	/// Finalize this block's additional data, writing the 32-byte digest hash into `hash_out` and
	/// returning `1`, or `0` when there is nothing to commit.
	///
	/// Runtime-side-allocation compatible (no guest allocation). Prefer the [`finalize`] wrapper,
	/// which reconstructs an `Option<[u8; 32]>`.
	///
	/// A missing extension is treated as "nothing recorded": this runs on every block — including
	/// blocks and test contexts that never produced additional data — so it must not panic on a
	/// missing extension.
	#[polkavm_index(243)]
	#[raw_api]
	fn finalize_into(&mut self, hash_out: PassFatPointerAndWrite<&mut [u8]>) -> u32 {
		match self.extension::<AdditionalDataExt>().and_then(|ext| ext.finalize()) {
			Some(h) => {
				hash_out[..32].copy_from_slice(&h);
				1
			},
			None => 0,
		}
	}

	/// Finalize and return the folded digest of everything recorded this block, or `None` when
	/// nothing was recorded. Idempotent. Ergonomic wrapper over [`finalize_into`].
	#[wrapper]
	fn finalize() -> Option<[u8; 32]> {
		let mut out = [0u8; 32];
		if finalize_into__raw(&mut out[..]) == 1 {
			Some(out)
		} else {
			None
		}
	}
}
