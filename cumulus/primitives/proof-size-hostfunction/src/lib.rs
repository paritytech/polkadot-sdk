// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Tools for reclaiming PoV weight in parachain runtimes.
//!
//! # Registered-only versions
//!
//! Three host functions have a new version with a body of its own:
//!
//! - `sp_io::storage::append` version 2: [`sp_externalities::Externalities::storage_append_v2`]
//!   instead of [`sp_externalities::Externalities::storage_append`].
//! - `sp_io::default_child_storage::root` version 3:
//!   [`sp_externalities::Externalities::child_storage_root_v3`] instead of
//!   [`sp_externalities::Externalities::child_storage_root_with_version`].
//! - [`storage_proof_size`](storage_proof_size::storage_proof_size) version 2:
//!   [`storage_proof_size_v2`] instead of [`storage_proof_size_v1`].
//!
//! The old bodies stay. Old runtimes call them forever, blocks in a chain's history must import
//! bit-for-bit, and for a given runtime the collator and the PVF must run the same body. The
//! `validate_block` shims of `cumulus-pallet-parachain-system` replace the runtime's bare host
//! function call, so each shim must call the body of the version that call means.
//!
//! The new versions are registered with `register_only`: nodes provide them, but runtimes keep
//! calling the old versions. The three markers are intended to be dropped together, in the same
//! release that switches the shims `host_storage_append`, `host_default_child_storage_root` and
//! `host_storage_proof_size` to the new bodies. The estimation of `storage_proof_size` version 2
//! is only sound with the child root semantics of `default_child_storage::root` version 3, and
//! collator and PVF agree on every block of the estimation fuzzer only with all three new bodies.
//!
//! `cumulus-proof-size-hostfunction-fuzz` tests both sides: `tests/v1_compat.rs` compares the old
//! bodies with a trace binary built on master, and the estimation runner (`tests/smoke.rs`) runs
//! the new bodies.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
use sp_externalities::{Externalities, ExternalitiesExt};

use sp_runtime_interface::runtime_interface;

#[cfg(feature = "std")]
use sp_trie::proof_size_extension::ProofSizeExt;

pub const PROOF_RECORDING_DISABLED: u64 = u64::MAX;

/// Body of version 1 of [`storage_proof_size::storage_proof_size`]: the size of the proof recorded
/// so far, or [`PROOF_RECORDING_DISABLED`] without [`ProofSizeExt`]. It must stay as it is, since
/// weight reclaim writes its result into the state and old runtimes call it forever.
///
///  See [Registered-only versions](crate#registered-only-versions).
#[cfg(feature = "std")]
pub fn storage_proof_size_v1(mut ext: &mut dyn Externalities) -> u64 {
	ext.extension::<ProofSizeExt>()
		.map_or(PROOF_RECORDING_DISABLED, |e| e.storage_proof_size())
}

/// Body of version 2 of [`storage_proof_size::storage_proof_size`]. It first records the trie nodes
/// the storage root at the end of the block will read for the writes so far
/// ([`Externalities::record_proof_for_dirty_keys`]), then reads the size as
/// [`storage_proof_size_v1`] does.
///
/// See [Registered-only versions](crate#registered-only-versions).
#[cfg(feature = "std")]
pub fn storage_proof_size_v2(mut ext: &mut dyn Externalities) -> u64 {
	if ext.extension::<ProofSizeExt>().is_some() {
		ext.record_proof_for_dirty_keys();
	}
	storage_proof_size_v1(ext)
}

/// Interface that provides access to the current storage proof size.
///
/// Should return the current storage proof size if [`ProofSizeExt`] is registered. Otherwise, needs
/// to return u64::MAX.
#[runtime_interface]
pub trait StorageProofSize {
	/// Returns the current storage proof size.
	fn storage_proof_size(&mut self) -> u64 {
		storage_proof_size_v1(*self)
	}

	/// Returns the current storage proof size, including an estimate of the trie nodes the storage
	/// root will read for the writes so far, see
	/// [`storage_proof_size_v2`](crate::storage_proof_size_v2).
	///
	/// Registered only: dropping the marker goes together with switching `host_storage_proof_size`
	/// in `validate_block` and with dropping the markers of the other registered-only versions, see
	/// [Registered-only versions](crate#registered-only-versions).
	#[version(2, register_only)]
	fn storage_proof_size(&mut self) -> u64 {
		storage_proof_size_v2(*self)
	}
}

#[cfg(test)]
mod tests {
	use sp_core::Blake2Hasher;
	use sp_state_machine::TestExternalities;
	use sp_trie::{
		proof_size_extension::ProofSizeExt, recorder::Recorder, LayoutV1, PrefixedMemoryDB,
		TrieDBMutBuilder, TrieMut,
	};

	use crate::{storage_proof_size, PROOF_RECORDING_DISABLED};

	const TEST_DATA: &[(&[u8], &[u8])] = &[(b"key1", &[1; 64]), (b"key2", &[2; 64])];

	type TestLayout = LayoutV1<sp_core::Blake2Hasher>;

	fn get_prepared_test_externalities() -> (TestExternalities<Blake2Hasher>, Recorder<Blake2Hasher>)
	{
		let mut db = PrefixedMemoryDB::default();
		let mut root = Default::default();

		{
			let mut trie = TrieDBMutBuilder::<TestLayout>::new(&mut db, &mut root).build();
			for (k, v) in TEST_DATA {
				trie.insert(k, v).expect("Inserts data");
			}
		}

		let recorder: sp_trie::recorder::Recorder<Blake2Hasher> = Default::default();
		let trie_backend = sp_state_machine::TrieBackendBuilder::new(db, root)
			.with_recorder(recorder.clone())
			.build();

		let mut ext: TestExternalities<Blake2Hasher> = TestExternalities::default();
		ext.backend = trie_backend;
		(ext, recorder)
	}

	#[test]
	fn host_function_returns_size_from_recorder() {
		let (mut ext, recorder) = get_prepared_test_externalities();
		ext.register_extension(ProofSizeExt::new(recorder));

		ext.execute_with(|| {
			assert_eq!(storage_proof_size::storage_proof_size(), 0);
			sp_io::storage::get(b"key1");
			assert_eq!(storage_proof_size::storage_proof_size(), 175);
			sp_io::storage::get(b"key2");
			assert_eq!(storage_proof_size::storage_proof_size(), 275);
			sp_io::storage::get(b"key2");
			assert_eq!(storage_proof_size::storage_proof_size(), 275);
		});
	}

	#[test]
	fn host_function_does_not_estimate_dirty_keys() {
		let (mut ext, recorder) = get_prepared_test_externalities();
		ext.register_extension(ProofSizeExt::new(recorder.clone()));

		ext.execute_with(|| {
			sp_io::storage::set(b"key1", &[3; 64]);
			sp_io::storage::clear(b"key2");
			let recorded = recorder.estimate_encoded_size() as u64;
			assert_eq!(storage_proof_size::storage_proof_size(), recorded);

			sp_externalities::with_externalities(|ext| ext.record_proof_for_dirty_keys());
			assert!(storage_proof_size::storage_proof_size() > recorded);
		});
	}

	#[test]
	fn host_function_returns_max_without_extension() {
		let (mut ext, _) = get_prepared_test_externalities();

		ext.execute_with(|| {
			assert_eq!(storage_proof_size::storage_proof_size(), PROOF_RECORDING_DISABLED);
			sp_io::storage::get(b"key1");
			assert_eq!(storage_proof_size::storage_proof_size(), PROOF_RECORDING_DISABLED);
			sp_io::storage::get(b"key2");
			assert_eq!(storage_proof_size::storage_proof_size(), PROOF_RECORDING_DISABLED);
		});
	}
}
