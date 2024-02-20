// Copyright (C) Parity Technologies (UK) Ltd.
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

use crate::messages::{AccountIdOf, BridgedChain, HashOf, HasherOf, MessageBridge, ThisChain};

use bp_messages::{
	storage_keys, InboundLaneData, LaneId, MessageKey, MessageNonce, MessagePayload,
	OutboundLaneData,
};
use bp_runtime::{record_all_trie_keys, RawStorageProof, StorageProofSize};
use codec::Encode;
use sp_std::{ops::RangeInclusive, prelude::*};
use sp_trie::{trie_types::TrieDBMutBuilderV1, LayoutV1, MemoryDB, TrieMut};

/// Simple and correct message data encode function.
pub fn encode_all_messages(_: MessageNonce, m: &MessagePayload) -> Option<Vec<u8>> {
	Some(m.encode())
}

/// Simple and correct outbound lane data encode function.
pub fn encode_lane_data(d: &OutboundLaneData) -> Vec<u8> {
	d.encode()
}

/// Prepare storage proof of given messages.
///
/// Returns state trie root and nodes with prepared messages.
pub fn prepare_messages_storage_proof<B>(
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
		for (i, nonce) in message_nonces.into_iter().enumerate() {
			let message_key = MessageKey { lane_id: lane, nonce };
			let message_payload = match encode_message(nonce, &message_payload) {
				Some(message_payload) =>
					if i == 0 {
						grow_trie_leaf_value(message_payload, size)
					} else {
						message_payload
					},
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

	// generate storage proof to be delivered to This chain
	let storage_proof = record_all_trie_keys::<LayoutV1<HasherOf<BridgedChain<B>>>, _>(&mdb, &root)
		.map_err(|_| "record_all_trie_keys has failed")
		.expect("record_all_trie_keys should not fail in benchmarks");
	(root, storage_proof)
}

/// Prepare storage proof of given messages delivery.
///
/// Returns state trie root and nodes with prepared messages.
pub fn prepare_message_delivery_storage_proof<B>(
	lane: LaneId,
	inbound_lane_data: InboundLaneData<AccountIdOf<ThisChain<B>>>,
	size: StorageProofSize,
) -> (HashOf<BridgedChain<B>>, RawStorageProof)
where
	B: MessageBridge,
{
	// prepare Bridged chain storage with inbound lane state
	let storage_key = storage_keys::inbound_lane_data_key(B::BRIDGED_MESSAGES_PALLET_NAME, &lane).0;
	let mut root = Default::default();
	let mut mdb = MemoryDB::default();
	{
		let mut trie =
			TrieDBMutBuilderV1::<HasherOf<BridgedChain<B>>>::new(&mut mdb, &mut root).build();
		let inbound_lane_data = grow_trie_leaf_value(inbound_lane_data.encode(), size);
		trie.insert(&storage_key, &inbound_lane_data)
			.map_err(|_| "TrieMut::insert has failed")
			.expect("TrieMut::insert should not fail in benchmarks");
	}

	// generate storage proof to be delivered to This chain
	let storage_proof = record_all_trie_keys::<LayoutV1<HasherOf<BridgedChain<B>>>, _>(&mdb, &root)
		.map_err(|_| "record_all_trie_keys has failed")
		.expect("record_all_trie_keys should not fail in benchmarks");

	(root, storage_proof)
}

/// Add extra data to the trie leaf value so that it'll be of given size.
pub fn grow_trie_leaf_value(mut value: Vec<u8>, size: StorageProofSize) -> Vec<u8> {
	match size {
		StorageProofSize::Minimal(_) => (),
		StorageProofSize::HasLargeLeaf(size) if size as usize > value.len() => {
			value.extend(sp_std::iter::repeat(42u8).take(size as usize - value.len()));
		},
		StorageProofSize::HasLargeLeaf(_) => (),
	}
	value
}
