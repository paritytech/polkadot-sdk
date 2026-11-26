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

//! Relay chain state proofs for tests and benchmarks.
//!
//! Built with `sp_trie` directly, since `cumulus-test-relay-sproof-builder` needs `std` and the
//! benchmarks also run in the wasm runtime.
//!
//! TODO: move to a `no_std` helper shared with other runtime benchmarks that build relay chain or
//! parachain proofs, such as `bridges/bin/runtime-common/src/parachains_benchmarking.rs`.

use alloc::vec::Vec;
use cumulus_pallet_parachain_system::RelayChainStateProof;
use cumulus_primitives_core::relay_chain;
use sp_runtime::traits::BlakeTwo256;
use sp_trie::{LayoutV1, Recorder, StorageProof, Trie, TrieDBBuilder, TrieDBMutBuilder, TrieMut};

type Layout = LayoutV1<BlakeTwo256>;

/// A relay chain state proof over a trie holding `entries`, with the nodes read for
/// `proven_keys`, as a collator records them.
///
/// A key of `entries` missing from `proven_keys` may not be readable from the proof.
pub fn relay_state_proof(
	entries: &[(&[u8], Vec<u8>)],
	proven_keys: &[&[u8]],
) -> RelayChainStateProof {
	let mut db = StorageProof::empty().into_memory_db::<BlakeTwo256>();
	let mut root = relay_chain::Hash::default();
	{
		let mut trie = TrieDBMutBuilder::<Layout>::new(&mut db, &mut root).build();
		for (key, value) in entries {
			trie.insert(key, value).expect("an in-memory trie accepts every insert; qed");
		}
	}
	let mut recorder = Recorder::<Layout>::new();
	{
		let trie = TrieDBBuilder::<Layout>::new(&db, &root).with_recorder(&mut recorder).build();
		for key in proven_keys {
			trie.get(key)
				.expect("the trie was just built in memory, so every node is there; qed");
		}
	}
	let nodes = recorder.drain().into_iter().map(|record| record.data).collect::<Vec<_>>();
	RelayChainStateProof::new(0.into(), root, StorageProof::new(nodes))
		.expect("the proof holds the root it was built from; qed")
}

/// A relay chain state proof that holds `randomness` as the epoch randomness.
pub fn with_epoch_randomness(randomness: [u8; 32]) -> RelayChainStateProof {
	let key = relay_chain::well_known_keys::ONE_EPOCH_AGO_RANDOMNESS;
	relay_state_proof(&[(key, codec::Encode::encode(&randomness))], &[key])
}
