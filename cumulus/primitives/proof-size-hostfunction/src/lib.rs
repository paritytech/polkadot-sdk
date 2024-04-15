// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Tools for reclaiming PoV weight in parachain runtimes.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
use sp_externalities::ExternalitiesExt;

use sp_runtime_interface::runtime_interface;

#[cfg(feature = "std")]
use sp_trie::proof_size_extension::ProofSizeExt;

pub const PROOF_RECORDING_DISABLED: u64 = u64::MAX;

/// Interface that provides access to the current storage proof size.
///
/// Should return the current storage proof size if [`ProofSizeExt`] is registered. Otherwise, needs
/// to return u64::MAX.
#[runtime_interface]
pub trait StorageProofSize {
	/// Returns the current storage proof size.
	fn storage_proof_size(&mut self) -> u64 {
		self.extension::<ProofSizeExt>()
			.map_or(PROOF_RECORDING_DISABLED, |e| e.storage_proof_size())
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
