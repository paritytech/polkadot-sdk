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

//! Everything required to run benchmarks of messages module, based on
//! `bridge_runtime_common::messages` implementation.

#![cfg(feature = "runtime-benchmarks")]

use crate::{
	messages::{
		source::FromBridgedChainMessagesDeliveryProof, target::FromBridgedChainMessagesProof,
		AccountIdOf, BalanceOf, BridgedChain, CallOf, HashOf, MessageBridge, ThisChain,
	},
	messages_generation::{
		encode_all_messages, encode_lane_data, grow_trie, prepare_messages_storage_proof,
	},
};

use bp_messages::storage_keys;
use bp_runtime::{record_all_trie_keys, StorageProofSize};
use codec::Encode;
use frame_support::{dispatch::GetDispatchInfo, weights::Weight};
use pallet_bridge_messages::benchmarking::{MessageDeliveryProofParams, MessageProofParams};
use sp_core::Hasher;
use sp_runtime::traits::{Header, MaybeSerializeDeserialize, Zero};
use sp_std::{fmt::Debug, prelude::*};
use sp_trie::{trie_types::TrieDBMutBuilderV1, LayoutV1, MemoryDB, Recorder, TrieMut};

/// Prepare proof of messages for the `receive_messages_proof` call.
///
/// In addition to returning valid messages proof, environment is prepared to verify this message
/// proof.
pub fn prepare_message_proof<R, BI, FI, B, BH, BHH>(
	params: MessageProofParams,
) -> (FromBridgedChainMessagesProof<HashOf<BridgedChain<B>>>, Weight)
where
	R: frame_system::Config<AccountId = AccountIdOf<ThisChain<B>>>
		+ pallet_bridge_grandpa::Config<FI>,
	R::BridgedChain: bp_runtime::Chain<Hash = HashOf<BridgedChain<B>>, Header = BH>,
	B: MessageBridge,
	BI: 'static,
	FI: 'static,
	BH: Header<Hash = HashOf<BridgedChain<B>>>,
	BHH: Hasher<Out = HashOf<BridgedChain<B>>>,
	AccountIdOf<ThisChain<B>>: PartialEq + sp_std::fmt::Debug,
	AccountIdOf<BridgedChain<B>>: From<[u8; 32]>,
	BalanceOf<ThisChain<B>>: Debug + MaybeSerializeDeserialize,
	CallOf<ThisChain<B>>: From<frame_system::Call<R>> + GetDispatchInfo,
	HashOf<BridgedChain<B>>: Copy + Default,
{
	let message_payload = match params.size {
		StorageProofSize::Minimal(ref size) => vec![0u8; *size as _],
		_ => vec![],
	};

	// finally - prepare storage proof and update environment
	let (state_root, storage_proof) = prepare_messages_storage_proof::<B>(
		params.lane,
		params.message_nonces.clone(),
		params.outbound_lane_data,
		params.size,
		message_payload,
		encode_all_messages,
		encode_lane_data,
	);
	let (_, bridged_header_hash) = insert_header_to_grandpa_pallet::<R, FI>(state_root);

	(
		FromBridgedChainMessagesProof {
			bridged_header_hash,
			storage_proof,
			lane: params.lane,
			nonces_start: *params.message_nonces.start(),
			nonces_end: *params.message_nonces.end(),
		},
		Weight::zero(),
	)
}

/// Prepare proof of messages delivery for the `receive_messages_delivery_proof` call.
pub fn prepare_message_delivery_proof<R, FI, B, BH, BHH>(
	params: MessageDeliveryProofParams<AccountIdOf<ThisChain<B>>>,
) -> FromBridgedChainMessagesDeliveryProof<HashOf<BridgedChain<B>>>
where
	R: pallet_bridge_grandpa::Config<FI>,
	R::BridgedChain: bp_runtime::Chain<Hash = HashOf<BridgedChain<B>>, Header = BH>,
	FI: 'static,
	B: MessageBridge,
	BH: Header<Hash = HashOf<BridgedChain<B>>>,
	BHH: Hasher<Out = HashOf<BridgedChain<B>>>,
	HashOf<BridgedChain<B>>: Copy + Default,
{
	// prepare Bridged chain storage with inbound lane state
	let storage_key =
		storage_keys::inbound_lane_data_key(B::BRIDGED_MESSAGES_PALLET_NAME, &params.lane).0;
	let mut root = Default::default();
	let mut mdb = MemoryDB::default();
	{
		let mut trie = TrieDBMutBuilderV1::<BHH>::new(&mut mdb, &mut root).build();
		trie.insert(&storage_key, &params.inbound_lane_data.encode())
			.map_err(|_| "TrieMut::insert has failed")
			.expect("TrieMut::insert should not fail in benchmarks");
	}
	root = grow_trie(root, &mut mdb, params.size);

	// generate storage proof to be delivered to This chain
	let mut proof_recorder = Recorder::<LayoutV1<BHH>>::new();
	record_all_trie_keys::<LayoutV1<BHH>, _>(&mdb, &root, &mut proof_recorder)
		.map_err(|_| "record_all_trie_keys has failed")
		.expect("record_all_trie_keys should not fail in benchmarks");
	let storage_proof = proof_recorder.drain().into_iter().map(|n| n.data.to_vec()).collect();

	// finally insert header with given state root to our storage
	let (_, bridged_header_hash) = insert_header_to_grandpa_pallet::<R, FI>(root);

	FromBridgedChainMessagesDeliveryProof {
		bridged_header_hash: bridged_header_hash.into(),
		storage_proof,
		lane: params.lane,
	}
}

/// Insert header to the bridge GRANDPA pallet.
pub(crate) fn insert_header_to_grandpa_pallet<R, GI>(
	state_root: bp_runtime::HashOf<R::BridgedChain>,
) -> (bp_runtime::BlockNumberOf<R::BridgedChain>, bp_runtime::HashOf<R::BridgedChain>)
where
	R: pallet_bridge_grandpa::Config<GI>,
	GI: 'static,
	R::BridgedChain: bp_runtime::Chain,
{
	let bridged_block_number = Zero::zero();
	let bridged_header = bp_runtime::HeaderOf::<R::BridgedChain>::new(
		bridged_block_number,
		Default::default(),
		state_root,
		Default::default(),
		Default::default(),
	);
	let bridged_header_hash = bridged_header.hash();
	pallet_bridge_grandpa::initialize_for_benchmarks::<R, GI>(bridged_header);
	(bridged_block_number, bridged_header_hash)
}
