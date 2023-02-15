// Copyright 2019-2022 Parity Technologies (UK) Ltd.
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

//! Helpers for generating message storage proofs, that are used by tests and by benchmarks.

#![cfg(any(feature = "runtime-benchmarks", test))]

use crate::messages::{BridgedChain, HashOf, HasherOf, MessageBridge};

use bp_messages::{
	storage_keys, LaneId, MessageKey, MessageNonce, MessagePayload, OutboundLaneData,
};
use bp_runtime::{record_all_trie_keys, RawStorageProof, StorageProofSize};
use codec::Encode;
use sp_core::Hasher;
use sp_std::{ops::RangeInclusive, prelude::*};
use sp_trie::{trie_types::TrieDBMutBuilderV1, LayoutV1, MemoryDB, TrieMut};

/// Simple and correct message data encode function.
pub(crate) fn encode_all_messages(_: MessageNonce, m: &MessagePayload) -> Option<Vec<u8>> {
	Some(m.encode())
}

/// Simple and correct outbound lane data encode function.
pub(crate) fn encode_lane_data(d: &OutboundLaneData) -> Vec<u8> {
	d.encode()
}

/// Prepare storage proof of given messages.
///
/// Returns state trie root and nodes with prepared messages.
pub(crate) fn prepare_messages_storage_proof<B>(
	lane: LaneId,
	message_nonces: RangeInclusive<MessageNonce>,
	outbound_lane_data: Option<OutboundLaneData>,
	size: StorageProofSize,
	message_payload: MessagePayload,
	encode_message: impl Fn(MessageNonce, &MessagePayload) -> Option<Vec<u8>>,
	encode_outbound_lane_data: impl Fn(&OutboundLaneData) -> Vec<u8>,
) -> (HashOf<BridgedChain<B>>, RawStorageProof)
where
	B: MessageBridge,
	HashOf<BridgedChain<B>>: Copy + Default,
{
	// prepare Bridged chain storage with messages and (optionally) outbound lane state
	let message_count = message_nonces.end().saturating_sub(*message_nonces.start()) + 1;
	let mut storage_keys = Vec::with_capacity(message_count as usize + 1);
	let mut root = Default::default();
	let mut mdb = MemoryDB::default();
	{
		let mut trie =
			TrieDBMutBuilderV1::<HasherOf<BridgedChain<B>>>::new(&mut mdb, &mut root).build();

		// insert messages
		for nonce in message_nonces {
			let message_key = MessageKey { lane_id: lane, nonce };
			let message_payload = match encode_message(nonce, &message_payload) {
				Some(message_payload) => message_payload,
				None => continue,
			};
			let storage_key = storage_keys::message_key(
				B::BRIDGED_MESSAGES_PALLET_NAME,
				&message_key.lane_id,
				message_key.nonce,
			)
			.0;
			trie.insert(&storage_key, &message_payload)
				.map_err(|_| "TrieMut::insert has failed")
				.expect("TrieMut::insert should not fail in benchmarks");
			storage_keys.push(storage_key);
		}

		// insert outbound lane state
		if let Some(outbound_lane_data) = outbound_lane_data.as_ref().map(encode_outbound_lane_data)
		{
			let storage_key =
				storage_keys::outbound_lane_data_key(B::BRIDGED_MESSAGES_PALLET_NAME, &lane).0;
			trie.insert(&storage_key, &outbound_lane_data)
				.map_err(|_| "TrieMut::insert has failed")
				.expect("TrieMut::insert should not fail in benchmarks");
			storage_keys.push(storage_key);
		}
	}
	root = grow_trie(root, &mut mdb, size);

	// generate storage proof to be delivered to This chain
	let storage_proof = record_all_trie_keys::<LayoutV1<HasherOf<BridgedChain<B>>>, _>(&mdb, &root)
		.map_err(|_| "record_all_trie_keys has failed")
		.expect("record_all_trie_keys should not fail in benchmarks");

	(root, storage_proof)
}

/// Populate trie with dummy keys+values until trie has at least given size.
pub fn grow_trie<H: Hasher>(
	mut root: H::Out,
	mdb: &mut MemoryDB<H>,
	trie_size: StorageProofSize,
) -> H::Out {
	let (iterations, leaf_size, minimal_trie_size) = match trie_size {
		StorageProofSize::Minimal(_) => return root,
		StorageProofSize::HasLargeLeaf(size) => (1, size, size),
		StorageProofSize::HasExtraNodes(size) => (8, 1, size),
	};

	let mut key_index = 0;
	loop {
		// generate storage proof to be delivered to This chain
		let storage_proof = record_all_trie_keys::<LayoutV1<H>, _>(mdb, &root)
			.map_err(|_| "record_all_trie_keys has failed")
			.expect("record_all_trie_keys should not fail in benchmarks");
		let size: usize = storage_proof.iter().map(|n| n.len()).sum();
		if size > minimal_trie_size as _ {
			return root
		}

		let mut trie = TrieDBMutBuilderV1::<H>::from_existing(mdb, &mut root).build();
		for _ in 0..iterations {
			trie.insert(&key_index.encode(), &vec![42u8; leaf_size as _])
				.map_err(|_| "TrieMut::insert has failed")
				.expect("TrieMut::insert should not fail in benchmarks");
			key_index += 1;
		}
		trie.commit();
	}
}
