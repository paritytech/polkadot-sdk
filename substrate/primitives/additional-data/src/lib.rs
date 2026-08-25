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

//! Host functions and extension for per-block additional data collection.
//!
//! # Overview
//!
//! This crate provides the primitives for chains that want to attach arbitrary opaque data to a
//! block header in a consensus-critical way. The pattern mirrors
//! `cumulus-primitives-proof-size-hostfunction` and the `RecordingProofSizeProvider` /
//! `ReplayProofSizeProvider` pair in `sp-trie`.
//!
//! ## Canonical encoding
//!
//! All layers of the stack (runtime, node, sync, database, PoV) agree on the same byte sequence:
//!
//! ```text
//! blob := items.encode()          // SCALE-encoding of Vec<Vec<u8>>
//! hash := blake2_256(&blob)       // deposited into the block header digest
//! ```
//!
//! [`encode_items`] and [`hash_blob`] implement these two steps.
//!
//! ## Host functions
//!
//! [`additional_data::push`] and [`additional_data::finalize`] are the runtime-interface host
//! functions called by the runtime during block execution. Both **panic** (never silently no-op)
//! when the [`AdditionalDataExt`] extension is absent, because the resulting hash is
//! consensus-critical and silent divergence is unacceptable.
//!
//! ## Providers
//!
//! - [`RecordingAdditionalDataProvider`]: used during **block building**. A **new instance must be
//!   constructed for every block-building attempt** — instances are never reused across blocks.
//!   After the block is built, [`RecordingAdditionalDataProvider::take_data`] returns the cached
//!   blob.
//! - [`ReplayAdditionalDataProvider`]: used during **block import / validation**. Constructed
//!   directly from the authoritative blob received from the network; `push` is a discarding no-op
//!   and `finalize` always returns `Some(hash_blob(&blob))`.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
#[cfg(not(feature = "std"))]
use alloc::boxed::Box;
use codec::Encode;
use sp_crypto_hashing::blake2_256;
use sp_externalities::ExternalitiesExt;
use sp_runtime_interface::{
	pass_by::{AllocateAndReturnByCodec, PassFatPointerAndRead},
	runtime_interface,
};

/// Encode a slice of items into the canonical additional-data blob.
///
/// `blob := items.encode()` — one SCALE encoding of the whole `Vec<Vec<u8>>`.
/// This is the single encoding agreed on by all layers of the stack.
pub fn encode_items(items: &[Vec<u8>]) -> Vec<u8> {
	items.encode()
}

/// Hash a blob with blake2-256.
///
/// `hash := blake2_256(blob)` — the 32-byte value deposited into the block header digest.
pub fn hash_blob(blob: &[u8]) -> [u8; 32] {
	blake2_256(blob)
}

/// Provider trait for additional-data accumulation and finalization.
///
/// Two implementations are provided:
/// - [`RecordingAdditionalDataProvider`] for block building.
/// - [`ReplayAdditionalDataProvider`] for block import / validation.
pub trait AdditionalDataProvider: Send + Sync {
	/// Append an item to the accumulator.
	///
	/// # Panics
	///
	/// Implementations MUST panic if called after [`Self::finalize`] has been called with at
	/// least one item previously pushed (i.e., once the accumulator is frozen).
	fn push(&self, item: Vec<u8>);

	/// Finalize the accumulator and return the hash of all pushed items.
	///
	/// Returns `None` when no items were ever pushed — a legitimate per-block state meaning this
	/// block carries no additional data, distinct from "extension missing entirely".
	/// Returns `Some(hash)` when at least one item was pushed. MUST be idempotent.
	fn finalize(&self) -> Option<[u8; 32]>;
}

sp_externalities::decl_extension! {
	/// Externalities extension that wraps an [`AdditionalDataProvider`].
	///
	/// Register this extension in the externalities before executing a block that calls
	/// [`additional_data::push`] or [`additional_data::finalize`].
	pub struct AdditionalDataExt(Box<dyn AdditionalDataProvider>);
}

/// Runtime interface for per-block additional data collection.
///
/// Both methods **panic** when [`AdditionalDataExt`] is not registered in the externalities.
/// Unlike `storage_proof_size`'s graceful fallback, this hash is consensus-critical (deposited
/// into the header digest), so a missing extension must fail loudly rather than silently diverge.
#[runtime_interface]
pub trait AdditionalData {
	/// Append `item` to the per-block accumulator.
	///
	/// # Panics
	///
	/// - If [`AdditionalDataExt`] is not registered in the externalities.
	/// - If [`finalize`](Self::finalize) has already been called on the underlying provider.
	fn push(&mut self, item: PassFatPointerAndRead<Vec<u8>>) {
		self.extension::<AdditionalDataExt>()
			.expect(
				"AdditionalDataExt extension not registered; \
				 this host function is consensus-critical and cannot silently diverge",
			)
			.0
			.push(item);
	}

	/// Finalize the accumulator and return the hash of all items pushed so far.
	///
	/// Returns `None` when the extension is registered but no items were ever pushed.
	/// Returns `Some(hash)` when at least one item was pushed. Idempotent.
	///
	/// # Panics
	///
	/// If [`AdditionalDataExt`] is not registered in the externalities.
	fn finalize(&mut self) -> AllocateAndReturnByCodec<Option<[u8; 32]>> {
		self.extension::<AdditionalDataExt>()
			.expect(
				"AdditionalDataExt extension not registered; \
				 this host function is consensus-critical and cannot silently diverge",
			)
			.0
			.finalize()
	}
}

// ── std-only provider implementations ──────────────────────────────────────────────────────────

#[cfg(feature = "std")]
mod std_impl {
	use super::{encode_items, hash_blob, AdditionalDataProvider};
	use parking_lot::Mutex;
	use std::sync::Arc;

	struct RecordingInner {
		items: Vec<Vec<u8>>,
		/// Cached `(blob, hash)` set on the first call to `finalize` with ≥1 items.
		cached: Option<(Vec<u8>, [u8; 32])>,
	}

	/// Block-building provider: collects items pushed during execution and exposes the final blob.
	///
	/// # Lifecycle
	///
	/// **A new instance MUST be constructed for every block-building attempt.** Instances are
	/// never reused across blocks. After the block is sealed, call
	/// [`RecordingAdditionalDataProvider::take_data`] to retrieve the cached blob, then discard
	/// the instance.
	#[derive(Clone)]
	pub struct RecordingAdditionalDataProvider {
		inner: Arc<Mutex<RecordingInner>>,
	}

	impl RecordingAdditionalDataProvider {
		/// Create a fresh recorder for a new block-building attempt.
		pub fn new() -> Self {
			Self { inner: Arc::new(Mutex::new(RecordingInner { items: Vec::new(), cached: None })) }
		}

		/// Return the cached blob from `finalize`, or `None` if finalize has not been called yet
		/// or no items were ever pushed.
		pub fn take_data(&self) -> Option<Vec<u8>> {
			self.inner.lock().cached.as_ref().map(|(blob, _)| blob.clone())
		}
	}

	impl Default for RecordingAdditionalDataProvider {
		fn default() -> Self {
			Self::new()
		}
	}

	impl AdditionalDataProvider for RecordingAdditionalDataProvider {
		fn push(&self, item: Vec<u8>) {
			let mut inner = self.inner.lock();
			if inner.cached.is_some() {
				panic!("cannot push additional data after finalize");
			}
			inner.items.push(item);
		}

		fn finalize(&self) -> Option<[u8; 32]> {
			let mut inner = self.inner.lock();
			// Idempotent: return cached hash on repeated calls.
			if let Some((_, hash)) = inner.cached {
				return Some(hash);
			}
			if inner.items.is_empty() {
				return None;
			}
			let blob = encode_items(&inner.items);
			let hash = hash_blob(&blob);
			inner.cached = Some((blob, hash));
			Some(hash)
		}
	}

	/// Block-import / validation provider: re-hashes from the authoritative blob.
	///
	/// Constructed directly from the blob bytes received from the network or database. `push`
	/// calls are discarding no-ops — correctness is guaranteed by the pre-loaded blob, not by
	/// re-accumulation. `finalize` always returns `Some(hash_blob(&blob))`.
	pub struct ReplayAdditionalDataProvider(Vec<u8>);

	impl ReplayAdditionalDataProvider {
		/// Construct from the authoritative blob (as received from the network or the database).
		pub fn new(blob: Vec<u8>) -> Self {
			Self(blob)
		}
	}

	impl AdditionalDataProvider for ReplayAdditionalDataProvider {
		fn push(&self, _item: Vec<u8>) {
			// No-op: replay ignores pushed items; correctness is guaranteed by the
			// pre-loaded authoritative blob, not by re-accumulation.
		}

		fn finalize(&self) -> Option<[u8; 32]> {
			Some(hash_blob(&self.0))
		}
	}
}

#[cfg(feature = "std")]
pub use std_impl::{RecordingAdditionalDataProvider, ReplayAdditionalDataProvider};

// ── Tests ───────────────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
	use super::*;
	use sp_io::TestExternalities;

	fn recording_ext() -> (TestExternalities, RecordingAdditionalDataProvider) {
		let provider = RecordingAdditionalDataProvider::new();
		let mut ext = TestExternalities::default();
		ext.register_extension(AdditionalDataExt(Box::new(provider.clone())));
		(ext, provider)
	}

	/// (a) Two `push` calls then `finalize` returns `Some(hash_blob(&encode_items(&items)))`.
	#[test]
	fn push_twice_then_finalize_returns_correct_hash() {
		let (mut ext, _) = recording_ext();
		ext.execute_with(|| {
			additional_data::push(vec![1u8, 2, 3]);
			additional_data::push(vec![4u8, 5, 6]);
			let hash = additional_data::finalize();

			let items: Vec<Vec<u8>> = vec![vec![1, 2, 3], vec![4, 5, 6]];
			assert_eq!(hash, Some(hash_blob(&encode_items(&items))));
		});
	}

	/// (b) `finalize` called twice returns the identical `Some(hash)`.
	#[test]
	fn finalize_is_idempotent() {
		let (mut ext, _) = recording_ext();
		ext.execute_with(|| {
			additional_data::push(vec![42u8]);
			let hash1 = additional_data::finalize();
			let hash2 = additional_data::finalize();
			assert!(hash1.is_some());
			assert_eq!(hash1, hash2);
		});
	}

	/// (c) `push` after `finalize` panics.
	#[test]
	#[should_panic(expected = "cannot push additional data after finalize")]
	fn push_after_finalize_panics() {
		let (mut ext, _) = recording_ext();
		ext.execute_with(|| {
			additional_data::push(vec![1u8]);
			let _ = additional_data::finalize();
			additional_data::push(vec![2u8]); // must panic
		});
	}

	/// (d1) `push` with no extension registered panics.
	#[test]
	#[should_panic(expected = "AdditionalDataExt extension not registered")]
	fn push_without_extension_panics() {
		TestExternalities::default().execute_with(|| {
			additional_data::push(vec![1u8]);
		});
	}

	/// (d2) `finalize` with no extension registered panics.
	#[test]
	#[should_panic(expected = "AdditionalDataExt extension not registered")]
	fn finalize_without_extension_panics() {
		TestExternalities::default().execute_with(|| {
			let _ = additional_data::finalize();
		});
	}

	/// (e) `finalize` with extension registered but nothing pushed returns `None`.
	#[test]
	fn finalize_with_no_items_returns_none() {
		let (mut ext, _) = recording_ext();
		ext.execute_with(|| {
			assert_eq!(additional_data::finalize(), None);
		});
	}

	/// (f) `ReplayAdditionalDataProvider::new(blob)` returns `Some(hash_blob(&blob))` matching
	///     what a `RecordingAdditionalDataProvider` fed the same items produces.
	#[test]
	fn replay_matches_recording() {
		let items: Vec<Vec<u8>> = vec![vec![10u8, 20], vec![30u8, 40]];
		let blob = encode_items(&items);

		// Recording side.
		let (mut ext1, _) = recording_ext();
		let recording_hash = ext1.execute_with(|| {
			for item in &items {
				additional_data::push(item.clone());
			}
			additional_data::finalize()
		});

		// Replay side.
		let replay = ReplayAdditionalDataProvider::new(blob.clone());
		let mut ext2 = TestExternalities::default();
		ext2.register_extension(AdditionalDataExt(Box::new(replay)));
		let replay_hash = ext2.execute_with(|| additional_data::finalize());

		assert_eq!(recording_hash, Some(hash_blob(&blob)));
		assert_eq!(recording_hash, replay_hash);
	}

	/// (g) A freshly-constructed `RecordingAdditionalDataProvider` after a prior instance was
	///     finalized does NOT panic on its first `push` (lifecycle isolation).
	#[test]
	fn fresh_recording_provider_does_not_panic_after_prior_finalize() {
		// First instance: push + finalize.
		let (mut ext1, _) = recording_ext();
		ext1.execute_with(|| {
			additional_data::push(vec![1u8]);
			let _ = additional_data::finalize();
		});

		// Second instance: completely fresh — must NOT panic.
		let (mut ext2, _) = recording_ext();
		ext2.execute_with(|| {
			additional_data::push(vec![2u8]);
			assert!(additional_data::finalize().is_some());
		});
	}
}
