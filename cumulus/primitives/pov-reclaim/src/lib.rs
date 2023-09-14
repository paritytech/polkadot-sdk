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

#![cfg_attr(not(feature = "std"), no_std)]

use sp_externalities::ExternalitiesExt;

#[cfg(feature = "std")]
use sp_proof_size_ext::ProofSizeExt;

use sp_runtime_interface::runtime_interface;

#[runtime_interface]
pub trait PovReclaimHostFunctions {
	fn current_storage_proof_size(&mut self) -> u32 {
		match self.extension::<ProofSizeExt>() {
			Some(ext) => ext.current_storage_proof_size(),
			None => 0,
		}
	}
}

#[cfg(test)]
mod tests {
	use sp_core::Blake2Hasher;
	use sp_proof_size_ext::ProofSizeExt;
	use sp_state_machine::TestExternalities;
	use sp_trie::{recorder::Recorder, LayoutV1, PrefixedMemoryDB, TrieDBMutBuilder, TrieMut};

	use crate::pov_reclaim_host_functions;

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
			assert_eq!(pov_reclaim_host_functions::current_storage_proof_size(), 0);
			sp_io::storage::get(b"key1");
			assert_eq!(pov_reclaim_host_functions::current_storage_proof_size(), 175);
			sp_io::storage::get(b"key2");
			assert_eq!(pov_reclaim_host_functions::current_storage_proof_size(), 275);
		});
	}

	#[test]
	fn host_function_returns_zero_without_extension() {
		let (mut ext, _) = get_prepared_test_externalities();

		ext.execute_with(|| {
			assert_eq!(pov_reclaim_host_functions::current_storage_proof_size(), 0);
			sp_io::storage::get(b"key1");
			assert_eq!(pov_reclaim_host_functions::current_storage_proof_size(), 0);
			sp_io::storage::get(b"key2");
			assert_eq!(pov_reclaim_host_functions::current_storage_proof_size(), 0);
		});
	}
}
