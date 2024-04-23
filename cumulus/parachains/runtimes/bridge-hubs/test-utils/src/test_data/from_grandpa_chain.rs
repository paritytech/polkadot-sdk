// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Generating test data for bridges with remote GRANDPA chains.

use crate::test_data::prepare_inbound_xcm;

use bp_messages::{
	source_chain::TargetHeaderChain, target_chain::SourceHeaderChain, LaneId, MessageNonce,
	UnrewardedRelayersState,
};
use bp_runtime::{AccountIdOf, BlockNumberOf, HeaderOf, StorageProofSize, UnderlyingChainOf};
use bp_test_utils::make_default_justification;
use bridge_runtime_common::{
	messages::{
		source::FromBridgedChainMessagesDeliveryProof, target::FromBridgedChainMessagesProof,
		BridgedChain as MessageBridgedChain, MessageBridge, ThisChain as MessageThisChain,
	},
	messages_generation::{
		encode_all_messages, encode_lane_data, prepare_message_delivery_storage_proof,
		prepare_messages_storage_proof,
	},
	messages_xcm_extension::XcmAsPlainPayload,
};
use codec::Encode;
use pallet_bridge_grandpa::{BridgedChain, BridgedHeader};
use sp_runtime::traits::Header as HeaderT;
use xcm::latest::prelude::*;

use bp_header_chain::{justification::GrandpaJustification, ChainWithGrandpa};
use bp_messages::{DeliveredMessages, InboundLaneData, UnrewardedRelayer};
use bp_runtime::HashOf;
use sp_runtime::DigestItem;

/// Prepare a batch call with bridged GRANDPA finality and message proof.
pub fn make_complex_relayer_delivery_batch<Runtime, GPI, MPI>(
	bridged_header: BridgedHeader<Runtime, GPI>,
	bridged_justification: GrandpaJustification<BridgedHeader<Runtime, GPI>>,
	message_proof: FromBridgedChainMessagesProof<HashOf<BridgedChain<Runtime, GPI>>>,
	relayer_id_at_bridged_chain: AccountIdOf<BridgedChain<Runtime, GPI>>,
) -> pallet_utility::Call<Runtime>
where
	Runtime: pallet_bridge_grandpa::Config<GPI>
		+ pallet_bridge_messages::Config<
			MPI,
			InboundPayload = XcmAsPlainPayload,
			InboundRelayer = AccountIdOf<BridgedChain<Runtime, GPI>>,
		> + pallet_utility::Config,
	GPI: 'static,
	MPI: 'static,
	<Runtime as pallet_bridge_messages::Config<MPI>>::SourceHeaderChain: SourceHeaderChain<
		MessagesProof = FromBridgedChainMessagesProof<HashOf<BridgedChain<Runtime, GPI>>>,
	>,
	<Runtime as pallet_utility::Config>::RuntimeCall: From<pallet_bridge_grandpa::Call<Runtime, GPI>>
		+ From<pallet_bridge_messages::Call<Runtime, MPI>>,
{
	let submit_grandpa = pallet_bridge_grandpa::Call::<Runtime, GPI>::submit_finality_proof {
		finality_target: Box::new(bridged_header),
		justification: bridged_justification,
	};
	let submit_message = pallet_bridge_messages::Call::<Runtime, MPI>::receive_messages_proof {
		relayer_id_at_bridged_chain,
		proof: message_proof,
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
	>,
	relayers_state: UnrewardedRelayersState,
) -> pallet_utility::Call<Runtime>
where
	Runtime: pallet_bridge_grandpa::Config<GPI>
		+ pallet_bridge_messages::Config<MPI, OutboundPayload = XcmAsPlainPayload>
		+ pallet_utility::Config,
	GPI: 'static,
	MPI: 'static,
	<Runtime as pallet_bridge_messages::Config<MPI>>::TargetHeaderChain: TargetHeaderChain<
		XcmAsPlainPayload,
		Runtime::AccountId,
		MessagesDeliveryProof = FromBridgedChainMessagesDeliveryProof<
			HashOf<BridgedChain<Runtime, GPI>>,
		>,
	>,
	<Runtime as pallet_utility::Config>::RuntimeCall: From<pallet_bridge_grandpa::Call<Runtime, GPI>>
		+ From<pallet_bridge_messages::Call<Runtime, MPI>>,
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

/// Prepare storage proofs of messages, stored at the (bridged) source GRANDPA chain.
pub fn make_complex_relayer_delivery_proofs<MB, InnerXcmRuntimeCall>(
	lane_id: LaneId,
	xcm_message: Xcm<InnerXcmRuntimeCall>,
	message_nonce: MessageNonce,
	message_destination: Junctions,
	header_number: BlockNumberOf<MessageBridgedChain<MB>>,
) -> (
	HeaderOf<MessageBridgedChain<MB>>,
	GrandpaJustification<HeaderOf<MessageBridgedChain<MB>>>,
	FromBridgedChainMessagesProof<HashOf<MessageBridgedChain<MB>>>,
)
where
	MB: MessageBridge,
	MessageBridgedChain<MB>: Send + Sync + 'static,
	UnderlyingChainOf<MessageBridgedChain<MB>>: ChainWithGrandpa,
{
	let message_payload = prepare_inbound_xcm(xcm_message, message_destination);
	let message_size = StorageProofSize::Minimal(message_payload.len() as u32);
	// prepare para storage proof containing message
	let (state_root, storage_proof) = prepare_messages_storage_proof::<MB>(
		lane_id,
		message_nonce..=message_nonce,
		None,
		message_size,
		message_payload,
		encode_all_messages,
		encode_lane_data,
	);

	let (header, justification) = make_complex_bridged_grandpa_header_proof::<
		MessageBridgedChain<MB>,
	>(state_root, header_number);

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
pub fn make_complex_relayer_confirmation_proofs<MB, InnerXcmRuntimeCall>(
	lane_id: LaneId,
	header_number: BlockNumberOf<MessageBridgedChain<MB>>,
	relayer_id_at_this_chain: AccountIdOf<MessageThisChain<MB>>,
	relayers_state: UnrewardedRelayersState,
) -> (
	HeaderOf<MessageBridgedChain<MB>>,
	GrandpaJustification<HeaderOf<MessageBridgedChain<MB>>>,
	FromBridgedChainMessagesDeliveryProof<HashOf<MessageBridgedChain<MB>>>,
)
where
	MB: MessageBridge,
	MessageBridgedChain<MB>: Send + Sync + 'static,
	MessageThisChain<MB>: Send + Sync + 'static,
	UnderlyingChainOf<MessageBridgedChain<MB>>: ChainWithGrandpa,
{
	// prepare storage proof containing message delivery proof
	let (state_root, storage_proof) = prepare_message_delivery_storage_proof::<MB>(
		lane_id,
		InboundLaneData {
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
		StorageProofSize::Minimal(0),
	);

	let (header, justification) =
		make_complex_bridged_grandpa_header_proof::<MB::BridgedChain>(state_root, header_number);

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
	header.digest_mut().push(DigestItem::Other(vec![42; extra_bytes_required]));

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
