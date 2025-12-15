// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

#![cfg(test)]

use codec::Encode;
use cumulus_pallet_parachain_system::RelayChainStateProof;
use cumulus_primitives_core::ParaId;
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
