// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Everything required to run benchmarks of parachains finality module.

#![cfg(feature = "runtime-benchmarks")]

use crate::messages_benchmarking::{grow_trie, insert_header_to_grandpa_pallet};

use bp_parachains::parachain_head_storage_key_at_source;
use bp_polkadot_core::parachains::{ParaHash, ParaHead, ParaHeadsProof, ParaId};
use bp_runtime::{record_all_trie_keys, StorageProofSize};
use codec::Encode;
use frame_support::traits::Get;
use pallet_bridge_parachains::{RelayBlockHash, RelayBlockHasher, RelayBlockNumber};
use sp_std::prelude::*;
use sp_trie::{trie_types::TrieDBMutBuilderV1, LayoutV1, MemoryDB, Recorder, TrieMut};

/// Prepare proof of messages for the `receive_messages_proof` call.
///
/// In addition to returning valid messages proof, environment is prepared to verify this message
/// proof.
pub fn prepare_parachain_heads_proof<R, PI>(
	parachains: &[ParaId],
	parachain_head_size: u32,
	size: StorageProofSize,
) -> (RelayBlockNumber, RelayBlockHash, ParaHeadsProof, Vec<(ParaId, ParaHash)>)
where
	R: pallet_bridge_parachains::Config<PI>
		+ pallet_bridge_grandpa::Config<R::BridgesGrandpaPalletInstance>,
	PI: 'static,
	<R as pallet_bridge_grandpa::Config<R::BridgesGrandpaPalletInstance>>::BridgedChain:
		bp_runtime::Chain<BlockNumber = RelayBlockNumber, Hash = RelayBlockHash>,
{
	let parachain_head = ParaHead(vec![0u8; parachain_head_size as usize]);

	// insert all heads to the trie
	let mut parachain_heads = Vec::with_capacity(parachains.len());
	let mut storage_keys = Vec::with_capacity(parachains.len());
	let mut state_root = Default::default();
	let mut mdb = MemoryDB::default();
	{
		let mut trie =
			TrieDBMutBuilderV1::<RelayBlockHasher>::new(&mut mdb, &mut state_root).build();

		// insert parachain heads
		for parachain in parachains {
			let storage_key =
				parachain_head_storage_key_at_source(R::ParasPalletName::get(), *parachain);
			trie.insert(&storage_key.0, &parachain_head.encode())
				.map_err(|_| "TrieMut::insert has failed")
				.expect("TrieMut::insert should not fail in benchmarks");
			storage_keys.push(storage_key);
			parachain_heads.push((*parachain, parachain_head.hash()))
		}
	}
	state_root = grow_trie(state_root, &mut mdb, size);

	// generate heads storage proof
	let mut proof_recorder = Recorder::<LayoutV1<RelayBlockHasher>>::new();
	record_all_trie_keys::<LayoutV1<RelayBlockHasher>, _>(&mdb, &state_root, &mut proof_recorder)
		.map_err(|_| "record_all_trie_keys has failed")
		.expect("record_all_trie_keys should not fail in benchmarks");
	let proof = proof_recorder.drain().into_iter().map(|n| n.data.to_vec()).collect();

	let (relay_block_number, relay_block_hash) =
		insert_header_to_grandpa_pallet::<R, R::BridgesGrandpaPalletInstance>(state_root);

	(relay_block_number, relay_block_hash, ParaHeadsProof(proof), parachain_heads)
}
