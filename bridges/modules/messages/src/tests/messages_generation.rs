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

use bp_messages::{
	storage_keys, ChainWithMessages, InboundLaneData, MessageKey, MessageNonce, MessagePayload,
	OutboundLaneData,
};
use bp_runtime::{
	grow_storage_value, record_all_trie_keys, AccountIdOf, Chain, HashOf, HasherOf,
	RawStorageProof, UnverifiedStorageProofParams,
};
use codec::Encode;
use sp_std::{ops::RangeInclusive, prelude::*};
use sp_trie::{trie_types::TrieDBMutBuilderV1, LayoutV1, MemoryDB, TrieMut};

/// Dummy message generation function.
pub fn generate_dummy_message(_: MessageNonce) -> MessagePayload {
	vec![42]
}

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
#[allow(clippy::too_many_arguments)]
pub fn prepare_messages_storage_proof<
	BridgedChain: Chain,
	ThisChain: ChainWithMessages,
	LaneId: Encode + Copy,
>(
	lane: LaneId,
	message_nonces: RangeInclusive<MessageNonce>,
	outbound_lane_data: Option<OutboundLaneData>,
	proof_params: UnverifiedStorageProofParams,
	generate_message: impl Fn(MessageNonce) -> MessagePayload,
	encode_message: impl Fn(MessageNonce, &MessagePayload) -> Option<Vec<u8>>,
	encode_outbound_lane_data: impl Fn(&OutboundLaneData) -> Vec<u8>,
	add_duplicate_key: bool,
	add_unused_key: bool,
) -> (HashOf<BridgedChain>, RawStorageProof)
where
	HashOf<BridgedChain>: Copy + Default,
{
	// prepare Bridged chain storage with messages and (optionally) outbound lane state
	let message_count = message_nonces.end().saturating_sub(*message_nonces.start()) + 1;
	let mut storage_keys = Vec::with_capacity(message_count as usize + 1);
	let mut root = Default::default();
	let mut mdb = MemoryDB::default();
	{
		let mut trie =
			TrieDBMutBuilderV1::<HasherOf<BridgedChain>>::new(&mut mdb, &mut root).build();

		// insert messages
		for (i, nonce) in message_nonces.into_iter().enumerate() {
			let message_key = MessageKey { lane_id: lane, nonce };
			let message_payload = match encode_message(nonce, &generate_message(nonce)) {
				Some(message_payload) =>
					if i == 0 {
						grow_storage_value(message_payload, &proof_params)
					} else {
						message_payload
					},
				None => continue,
			};
			let storage_key = storage_keys::message_key(
				ThisChain::WITH_CHAIN_MESSAGES_PALLET_NAME,
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
			let storage_key = storage_keys::outbound_lane_data_key(
				ThisChain::WITH_CHAIN_MESSAGES_PALLET_NAME,
				&lane,
			)
			.0;
			trie.insert(&storage_key, &outbound_lane_data)
				.map_err(|_| "TrieMut::insert has failed")
				.expect("TrieMut::insert should not fail in benchmarks");
			storage_keys.push(storage_key);
		}
	}

	// generate storage proof to be delivered to This chain
	let mut storage_proof =
		record_all_trie_keys::<LayoutV1<HasherOf<BridgedChain>>, _>(&mdb, &root)
			.map_err(|_| "record_all_trie_keys has failed")
			.expect("record_all_trie_keys should not fail in benchmarks");

	if add_duplicate_key {
		assert!(!storage_proof.is_empty());
		let node = storage_proof.pop().unwrap();
		storage_proof.push(node.clone());
		storage_proof.push(node);
	}

	if add_unused_key {
		storage_proof.push(b"unused_value".to_vec());
	}

	(root, storage_proof)
}

/// Prepare storage proof of given messages delivery.
///
/// Returns state trie root and nodes with prepared messages.
pub fn prepare_message_delivery_storage_proof<
	BridgedChain: Chain,
	ThisChain: ChainWithMessages,
	LaneId: Encode,
>(
	lane: LaneId,
	inbound_lane_data: InboundLaneData<AccountIdOf<ThisChain>>,
	proof_params: UnverifiedStorageProofParams,
) -> (HashOf<BridgedChain>, RawStorageProof)
where
	HashOf<BridgedChain>: Copy + Default,
{
	// prepare Bridged chain storage with inbound lane state
	let storage_key =
		storage_keys::inbound_lane_data_key(ThisChain::WITH_CHAIN_MESSAGES_PALLET_NAME, &lane).0;
	let mut root = Default::default();
	let mut mdb = MemoryDB::default();
	{
		let mut trie =
			TrieDBMutBuilderV1::<HasherOf<BridgedChain>>::new(&mut mdb, &mut root).build();
		let inbound_lane_data = grow_storage_value(inbound_lane_data.encode(), &proof_params);
		trie.insert(&storage_key, &inbound_lane_data)
			.map_err(|_| "TrieMut::insert has failed")
			.expect("TrieMut::insert should not fail in benchmarks");
	}

	// generate storage proof to be delivered to This chain
	let storage_proof = record_all_trie_keys::<LayoutV1<HasherOf<BridgedChain>>, _>(&mdb, &root)
		.map_err(|_| "record_all_trie_keys has failed")
		.expect("record_all_trie_keys should not fail in benchmarks");

	(root, storage_proof)
}
