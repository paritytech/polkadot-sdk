// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Generating test data for bridges with remote GRANDPA chains.

use crate::test_data::{prepare_inbound_xcm, XcmAsPlainPayload};

use bp_messages::{
	source_chain::FromBridgedChainMessagesDeliveryProof,
	target_chain::FromBridgedChainMessagesProof, ChainWithMessages, LaneState, MessageNonce,
	UnrewardedRelayersState,
};
use bp_runtime::{AccountIdOf, BlockNumberOf, Chain, HeaderOf, UnverifiedStorageProofParams};
use bp_test_utils::make_default_justification;
use codec::Encode;
use pallet_bridge_grandpa::{BridgedChain, BridgedHeader};
use sp_runtime::traits::Header as HeaderT;
use xcm::latest::prelude::*;

use crate::test_cases::helpers::InboundRelayerId;
use bp_header_chain::{justification::GrandpaJustification, ChainWithGrandpa};
use bp_messages::{DeliveredMessages, InboundLaneData, UnrewardedRelayer};
use bp_runtime::HashOf;
use pallet_bridge_messages::{
	messages_generation::{
		encode_all_messages, encode_lane_data, prepare_message_delivery_storage_proof,
		prepare_messages_storage_proof,
	},
	BridgedChainOf, LaneIdOf,
};
use sp_runtime::DigestItem;

/// Prepare a batch call with bridged GRANDPA finality and message proof.
pub fn make_complex_relayer_delivery_batch<Runtime, GPI, MPI>(
	bridged_header: BridgedHeader<Runtime, GPI>,
	bridged_justification: GrandpaJustification<BridgedHeader<Runtime, GPI>>,
	message_proof: FromBridgedChainMessagesProof<
		HashOf<BridgedChain<Runtime, GPI>>,
		LaneIdOf<Runtime, MPI>,
	>,
	relayer_id_at_bridged_chain: InboundRelayerId<Runtime, MPI>,
) -> pallet_utility::Call<Runtime>
where
	Runtime: pallet_bridge_grandpa::Config<GPI>
		+ pallet_bridge_messages::Config<MPI, InboundPayload = XcmAsPlainPayload>
		+ pallet_utility::Config,
	GPI: 'static,
	MPI: 'static,
	<Runtime as pallet_utility::Config>::RuntimeCall: From<pallet_bridge_grandpa::Call<Runtime, GPI>>
		+ From<pallet_bridge_messages::Call<Runtime, MPI>>,
	BridgedChainOf<Runtime, MPI>: Chain<Hash = HashOf<BridgedChain<Runtime, GPI>>>,
{
	let submit_grandpa = pallet_bridge_grandpa::Call::<Runtime, GPI>::submit_finality_proof {
		finality_target: Box::new(bridged_header),
		justification: bridged_justification,
	};
	let submit_message = pallet_bridge_messages::Call::<Runtime, MPI>::receive_messages_proof {
		relayer_id_at_bridged_chain,
		proof: Box::new(message_proof),
		messages_count: 1,
		dispatch_weight: Weight::from_parts(1000000000, 0),
	};
	pallet_utility::Call::<Runtime>::batch_all {
		calls: vec![submit_grandpa.into(), submit_message.into()],
	}
}

/// Prepare a batch call with bridged GRANDPA finality and message delivery proof.
pub fn make_complex_relayer_confirmation_batch<Runtime, GPI, MPI>(
	bridged_header: BridgedHeader<Runtime, GPI>,
	bridged_justification: GrandpaJustification<BridgedHeader<Runtime, GPI>>,
	message_delivery_proof: FromBridgedChainMessagesDeliveryProof<
		HashOf<BridgedChain<Runtime, GPI>>,
		LaneIdOf<Runtime, MPI>,
	>,
	relayers_state: UnrewardedRelayersState,
) -> pallet_utility::Call<Runtime>
where
	Runtime: pallet_bridge_grandpa::Config<GPI>
		+ pallet_bridge_messages::Config<MPI, OutboundPayload = XcmAsPlainPayload>
		+ pallet_utility::Config,
	GPI: 'static,
	MPI: 'static,
	<Runtime as pallet_utility::Config>::RuntimeCall: From<pallet_bridge_grandpa::Call<Runtime, GPI>>
		+ From<pallet_bridge_messages::Call<Runtime, MPI>>,
	BridgedChainOf<Runtime, MPI>: Chain<Hash = HashOf<BridgedChain<Runtime, GPI>>>,
{
	let submit_grandpa = pallet_bridge_grandpa::Call::<Runtime, GPI>::submit_finality_proof {
		finality_target: Box::new(bridged_header),
		justification: bridged_justification,
	};
	let submit_message_delivery_proof =
		pallet_bridge_messages::Call::<Runtime, MPI>::receive_messages_delivery_proof {
			proof: message_delivery_proof,
			relayers_state,
		};
	pallet_utility::Call::<Runtime>::batch_all {
		calls: vec![submit_grandpa.into(), submit_message_delivery_proof.into()],
	}
}

/// Prepare a call with message proof.
pub fn make_standalone_relayer_delivery_call<Runtime, GPI, MPI>(
	message_proof: FromBridgedChainMessagesProof<
		HashOf<BridgedChain<Runtime, GPI>>,
		LaneIdOf<Runtime, MPI>,
	>,
	relayer_id_at_bridged_chain: InboundRelayerId<Runtime, MPI>,
) -> Runtime::RuntimeCall
where
	Runtime: pallet_bridge_grandpa::Config<GPI>
		+ pallet_bridge_messages::Config<MPI, InboundPayload = XcmAsPlainPayload>,
	MPI: 'static,
	Runtime::RuntimeCall: From<pallet_bridge_messages::Call<Runtime, MPI>>,
	BridgedChainOf<Runtime, MPI>: Chain<Hash = HashOf<BridgedChain<Runtime, GPI>>>,
{
	pallet_bridge_messages::Call::<Runtime, MPI>::receive_messages_proof {
		relayer_id_at_bridged_chain,
		proof: Box::new(message_proof),
		messages_count: 1,
		dispatch_weight: Weight::from_parts(1000000000, 0),
	}
	.into()
}

/// Prepare a call with message delivery proof.
pub fn make_standalone_relayer_confirmation_call<Runtime, GPI, MPI>(
	message_delivery_proof: FromBridgedChainMessagesDeliveryProof<
		HashOf<BridgedChain<Runtime, GPI>>,
		LaneIdOf<Runtime, MPI>,
	>,
	relayers_state: UnrewardedRelayersState,
) -> Runtime::RuntimeCall
where
	Runtime: pallet_bridge_grandpa::Config<GPI>
		+ pallet_bridge_messages::Config<MPI, OutboundPayload = XcmAsPlainPayload>,
	MPI: 'static,
	Runtime::RuntimeCall: From<pallet_bridge_messages::Call<Runtime, MPI>>,
	BridgedChainOf<Runtime, MPI>: Chain<Hash = HashOf<BridgedChain<Runtime, GPI>>>,
{
	pallet_bridge_messages::Call::<Runtime, MPI>::receive_messages_delivery_proof {
		proof: message_delivery_proof,
		relayers_state,
	}
	.into()
}

/// Prepare storage proofs of messages, stored at the (bridged) source GRANDPA chain.
pub fn make_complex_relayer_delivery_proofs<BridgedChain, ThisChainWithMessages, LaneId>(
	lane_id: LaneId,
	xcm_message: Xcm<()>,
	message_nonce: MessageNonce,
	message_destination: Junctions,
	header_number: BlockNumberOf<BridgedChain>,
	is_minimal_call: bool,
) -> (
	HeaderOf<BridgedChain>,
	GrandpaJustification<HeaderOf<BridgedChain>>,
	FromBridgedChainMessagesProof<HashOf<BridgedChain>, LaneId>,
)
where
	BridgedChain: ChainWithGrandpa,
	ThisChainWithMessages: ChainWithMessages,
	LaneId: Copy + Encode,
{
	// prepare message
	let message_payload = prepare_inbound_xcm(xcm_message, message_destination);
	// prepare storage proof containing message
	let (state_root, storage_proof) =
		prepare_messages_storage_proof::<BridgedChain, ThisChainWithMessages, LaneId>(
			lane_id,
			message_nonce..=message_nonce,
			None,
			UnverifiedStorageProofParams::from_db_size(message_payload.len() as u32),
			|_| message_payload.clone(),
			encode_all_messages,
			encode_lane_data,
			false,
			false,
		);

	let (header, justification) = make_complex_bridged_grandpa_header_proof::<BridgedChain>(
		state_root,
		header_number,
		is_minimal_call,
	);

	let message_proof = FromBridgedChainMessagesProof {
		bridged_header_hash: header.hash(),
		storage_proof,
		lane: lane_id,
		nonces_start: message_nonce,
		nonces_end: message_nonce,
	};

	(header, justification, message_proof)
}

/// Prepare storage proofs of message confirmations, stored at the (bridged) target GRANDPA chain.
pub fn make_complex_relayer_confirmation_proofs<
	BridgedChain,
	ThisChainWithMessages,
	InnerXcmRuntimeCall,
	LaneId,
>(
	lane_id: LaneId,
	header_number: BlockNumberOf<BridgedChain>,
	relayer_id_at_this_chain: AccountIdOf<ThisChainWithMessages>,
	relayers_state: UnrewardedRelayersState,
) -> (
	HeaderOf<BridgedChain>,
	GrandpaJustification<HeaderOf<BridgedChain>>,
	FromBridgedChainMessagesDeliveryProof<HashOf<BridgedChain>, LaneId>,
)
where
	BridgedChain: ChainWithGrandpa,
	ThisChainWithMessages: ChainWithMessages,
	LaneId: Copy + Encode,
{
	// prepare storage proof containing message delivery proof
	let (state_root, storage_proof) =
		prepare_message_delivery_storage_proof::<BridgedChain, ThisChainWithMessages, LaneId>(
			lane_id,
			InboundLaneData {
				state: LaneState::Opened,
				relayers: vec![
					UnrewardedRelayer {
						relayer: relayer_id_at_this_chain,
						messages: DeliveredMessages::new(1)
					};
					relayers_state.unrewarded_relayer_entries as usize
				]
				.into(),
				last_confirmed_nonce: 1,
			},
			UnverifiedStorageProofParams::default(),
		);

	let (header, justification) =
		make_complex_bridged_grandpa_header_proof::<BridgedChain>(state_root, header_number, false);

	let message_delivery_proof = FromBridgedChainMessagesDeliveryProof {
		bridged_header_hash: header.hash(),
		storage_proof,
		lane: lane_id,
	};

	(header, justification, message_delivery_proof)
}

/// Make bridged GRANDPA chain header with given state root.
pub fn make_complex_bridged_grandpa_header_proof<BridgedChain>(
	state_root: HashOf<BridgedChain>,
	header_number: BlockNumberOf<BridgedChain>,
	is_minimal_call: bool,
) -> (HeaderOf<BridgedChain>, GrandpaJustification<HeaderOf<BridgedChain>>)
where
	BridgedChain: ChainWithGrandpa,
{
	let mut header = bp_test_utils::test_header_with_root::<HeaderOf<BridgedChain>>(
		header_number.into(),
		state_root.into(),
	);

	// to compute proper cost of GRANDPA call, let's add some dummy bytes to header, so that the
	// `submit_finality_proof` call size would be close to maximal expected (and refundable)
	let extra_bytes_required = maximal_expected_submit_finality_proof_call_size::<BridgedChain>()
		.saturating_sub(header.encoded_size());
	if !is_minimal_call {
		header.digest_mut().push(DigestItem::Other(vec![42; extra_bytes_required]));
	}

	let justification = make_default_justification(&header);
	(header, justification)
}

/// Maximal expected `submit_finality_proof` call size.
pub fn maximal_expected_submit_finality_proof_call_size<BridgedChain: ChainWithGrandpa>() -> usize {
	bp_header_chain::max_expected_submit_finality_proof_arguments_size::<BridgedChain>(
		false,
		BridgedChain::MAX_AUTHORITIES_COUNT * 2 / 3 + 1,
	) as usize
}
