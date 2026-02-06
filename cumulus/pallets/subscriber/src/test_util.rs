// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use codec::Encode;
use cumulus_primitives_core::ParaId;

#[cfg(test)]
mod std_proof_builder {
	use super::*;
	use cumulus_pallet_parachain_system::RelayChainStateProof;
	use sp_runtime::StateVersion;
	use sp_state_machine::{Backend, TrieBackendBuilder};
	use sp_trie::{PrefixedMemoryDB, StorageProof};

	/// Build a relay chain state proof with child trie data for multiple publishers.
	pub fn build_sproof_with_child_data(
		publishers: &[(ParaId, Vec<(Vec<u8>, Vec<u8>)>)],
	) -> RelayChainStateProof {
		use sp_runtime::traits::HashingFor;

		let (db, root) = PrefixedMemoryDB::<HashingFor<polkadot_primitives::Block>>::default_with_root();
		let state_version = StateVersion::default();
		let mut backend = TrieBackendBuilder::new(db, root).build();

		let mut all_proofs = vec![];
		let mut main_trie_updates = vec![];

		// Process each publisher
		for (publisher_para_id, child_data) in publishers {
			let child_info = sp_core::storage::ChildInfo::new_default(&(b"pubsub", *publisher_para_id).encode());

			// Insert child trie data
			let child_kv: Vec<_> = child_data.iter().map(|(k, v)| (k.clone(), Some(v.clone()))).collect();
			backend.insert(vec![(Some(child_info.clone()), child_kv)], state_version);

			// Get child trie root and prepare to insert it in main trie
			let child_root = backend.child_storage_root(&child_info, core::iter::empty(), state_version).0;
			let prefixed_key = child_info.prefixed_storage_key();
			main_trie_updates.push((prefixed_key.to_vec(), Some(child_root.encode())));

			// Prove child trie keys
			let child_keys: Vec<_> = child_data.iter().map(|(k, _)| k.clone()).collect();
			if !child_keys.is_empty() {
				let child_proof = sp_state_machine::prove_child_read_on_trie_backend(&backend, &child_info, child_keys)
					.expect("prove child read");
				all_proofs.push(child_proof);
			}
		}

		// Insert all child roots in main trie
		backend.insert(vec![(None, main_trie_updates.clone())], state_version);
		let root = *backend.root();

		// Prove all child roots in main trie
		let main_keys: Vec<_> = main_trie_updates.iter().map(|(k, _)| k.clone()).collect();
		let main_proof = sp_state_machine::prove_read_on_trie_backend(&backend, main_keys)
			.expect("prove read");
		all_proofs.push(main_proof);

		// Merge all proofs
		let proof = StorageProof::merge(all_proofs);

		RelayChainStateProof::new(ParaId::from(100), root, proof).expect("valid proof")
	}
}

#[cfg(test)]
pub use std_proof_builder::build_sproof_with_child_data;

/// no_std-compatible proof builder for benchmarks
#[cfg(feature = "runtime-benchmarks")]
pub mod bench_proof_builder {
	use super::*;
	use alloc::vec::Vec;
	use cumulus_pallet_parachain_system::RelayChainStateProof;
	use sp_runtime::traits::BlakeTwo256;
	use sp_trie::{trie_types::TrieDBMutBuilderV1, recorder_ext::RecorderExt, LayoutV1, MemoryDB, Recorder, StorageProof, TrieDBBuilder, TrieMut};
	use trie_db::Trie;

	/// Record all trie keys 
	fn record_all_trie_keys<L: trie_db::TrieConfiguration, DB>(
		db: &DB,
		root: &sp_trie::TrieHash<L>,
	) -> Result<alloc::vec::Vec<alloc::vec::Vec<u8>>, sp_std::boxed::Box<sp_trie::TrieError<L>>>
	where
		DB: hash_db::HashDBRef<L::Hash, trie_db::DBValue>,
	{
		let mut recorder = Recorder::<L>::new();
		let trie = TrieDBBuilder::<L>::new(db, root).with_recorder(&mut recorder).build();
		for x in trie.iter()? {
			let (key, _) = x?;
			trie.get(&key)?;
		}
		Ok(recorder.into_raw_storage_proof())
	}

	/// Build relay chain state proof with child trie data
	pub fn build_sproof_with_child_data(
		publishers: &[(ParaId, Vec<(Vec<u8>, Vec<u8>)>)],
	) -> RelayChainStateProof {
		use polkadot_primitives::Hash as RelayHash;
		use sp_trie::empty_trie_root;

		// Build child tries and collect roots
		let mut child_roots = alloc::vec::Vec::new();
		let mut all_nodes = alloc::vec::Vec::new();

		for (publisher_para_id, child_data) in publishers {
			use hash_db::{HashDB, EMPTY_PREFIX};

			let empty_root = empty_trie_root::<LayoutV1<BlakeTwo256>>();
			let mut child_root = empty_root;
			let mut child_mdb = MemoryDB::<BlakeTwo256>::new(&[]);
			// Insert empty trie node so TrieDBMut can find it
			child_mdb.insert(EMPTY_PREFIX, &[0u8]);

			{
				let mut child_trie = TrieDBMutBuilderV1::<BlakeTwo256>::new(&mut child_mdb, &mut child_root).build();
				for (key, value) in child_data {
					child_trie.insert(key, value).expect("insert in bench");
				}
			}

			// Collect child trie nodes
			let child_nodes = record_all_trie_keys::<LayoutV1<BlakeTwo256>, _>(&child_mdb, &child_root)
				.expect("record child trie");
			all_nodes.extend(child_nodes);

			// Store child root for main trie
			let child_info = sp_core::storage::ChildInfo::new_default(&(b"pubsub", *publisher_para_id).encode());
			let prefixed_key = child_info.prefixed_storage_key();
			child_roots.push((prefixed_key.to_vec(), child_root.encode()));
		}

		// Build main trie with child roots
		use hash_db::{HashDB, EMPTY_PREFIX};

		let empty_root = empty_trie_root::<LayoutV1<BlakeTwo256>>();
		let mut main_root = empty_root;
		let mut main_mdb = MemoryDB::<BlakeTwo256>::new(&[]);
		// Insert empty trie node so TrieDBMut can find it
		main_mdb.insert(EMPTY_PREFIX, &[0u8]);

		{
			let mut main_trie = TrieDBMutBuilderV1::<BlakeTwo256>::new(&mut main_mdb, &mut main_root).build();
			for (key, value) in &child_roots {
				main_trie.insert(key, value).expect("insert in bench");
			}
		}

		// Collect main trie nodes
		let main_nodes = record_all_trie_keys::<LayoutV1<BlakeTwo256>, _>(&main_mdb, &main_root)
			.expect("record main trie");
		all_nodes.extend(main_nodes);

		let proof = StorageProof::new(all_nodes);
		let root: RelayHash = main_root.into();

		RelayChainStateProof::new(ParaId::from(100), root, proof).expect("valid proof")
	}
}
