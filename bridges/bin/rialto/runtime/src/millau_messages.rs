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

//! Everything required to serve Millau <-> Rialto messages.

use crate::{MillauGrandpaInstance, Runtime, RuntimeCall, RuntimeOrigin};

use bp_messages::{LaneId, MessageNonce};
use bp_runtime::{ChainId, MILLAU_CHAIN_ID, RIALTO_CHAIN_ID};
use bridge_runtime_common::messages::{
	self, source::TargetHeaderChainAdapter, target::SourceHeaderChainAdapter, MessageBridge,
};
use frame_support::{parameter_types, weights::Weight, RuntimeDebug};

/// Lane that is used for XCM messages exchange.
pub const XCM_LANE: LaneId = LaneId([0, 0, 0, 0]);
/// Weight of 2 XCM instructions is for simple `Trap(42)` program, coming through bridge
/// (it is prepended with `UniversalOrigin` instruction). It is used just for simplest manual
/// tests, confirming that we don't break encoding somewhere between.
pub const BASE_XCM_WEIGHT_TWICE: u64 = 2 * crate::xcm_config::BASE_XCM_WEIGHT;

parameter_types! {
	/// Weight credit for our test messages.
	///
	/// 2 XCM instructions is for simple `Trap(42)` program, coming through bridge
	/// (it is prepended with `UniversalOrigin` instruction).
	pub const WeightCredit: Weight = Weight::from_ref_time(BASE_XCM_WEIGHT_TWICE);
}

/// Message payload for Rialto -> Millau messages.
pub type ToMillauMessagePayload = messages::source::FromThisChainMessagePayload;

/// Message verifier for Rialto -> Millau messages.
pub type ToMillauMessageVerifier =
	messages::source::FromThisChainMessageVerifier<WithMillauMessageBridge>;

/// Message payload for Millau -> Rialto messages.
pub type FromMillauMessagePayload = messages::target::FromBridgedChainMessagePayload<RuntimeCall>;

/// Call-dispatch based message dispatch for Millau -> Rialto messages.
pub type FromMillauMessageDispatch = messages::target::FromBridgedChainMessageDispatch<
	WithMillauMessageBridge,
	xcm_executor::XcmExecutor<crate::xcm_config::XcmConfig>,
	crate::xcm_config::XcmWeigher,
	WeightCredit,
>;

/// Messages proof for Millau -> Rialto messages.
pub type FromMillauMessagesProof = messages::target::FromBridgedChainMessagesProof<bp_millau::Hash>;

/// Messages delivery proof for Rialto -> Millau messages.
pub type ToMillauMessagesDeliveryProof =
	messages::source::FromBridgedChainMessagesDeliveryProof<bp_millau::Hash>;

/// Maximal outbound payload size of Rialto -> Millau messages.
pub type ToMillauMaximalOutboundPayloadSize =
	messages::source::FromThisChainMaximalOutboundPayloadSize<WithMillauMessageBridge>;

/// Millau <-> Rialto message bridge.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct WithMillauMessageBridge;

impl MessageBridge for WithMillauMessageBridge {
	const THIS_CHAIN_ID: ChainId = RIALTO_CHAIN_ID;
	const BRIDGED_CHAIN_ID: ChainId = MILLAU_CHAIN_ID;
	const BRIDGED_MESSAGES_PALLET_NAME: &'static str = bp_rialto::WITH_RIALTO_MESSAGES_PALLET_NAME;

	type ThisChain = Rialto;
	type BridgedChain = Millau;
	type BridgedHeaderChain =
		pallet_bridge_grandpa::GrandpaChainHeaders<Runtime, MillauGrandpaInstance>;
}

/// Rialto chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct Rialto;

impl messages::UnderlyingChainProvider for Rialto {
	type Chain = bp_rialto::Rialto;
}

impl messages::ThisChainWithMessages for Rialto {
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;

	fn is_message_accepted(_send_origin: &Self::RuntimeOrigin, _lane: &LaneId) -> bool {
		true
	}

	fn maximal_pending_messages_at_outbound_lane() -> MessageNonce {
		MessageNonce::MAX
	}
}

/// Millau chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct Millau;
/// Millau as source header chain.
pub type MillauAsSourceHeaderChain = SourceHeaderChainAdapter<WithMillauMessageBridge>;
/// Millau as target header chain.
pub type MillauAsTargetHeaderChain = TargetHeaderChainAdapter<WithMillauMessageBridge>;

impl messages::UnderlyingChainProvider for Millau {
	type Chain = bp_millau::Millau;
}

impl messages::BridgedChainWithMessages for Millau {
	fn verify_dispatch_weight(_message_payload: &[u8]) -> bool {
		true
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{MillauGrandpaInstance, Runtime, WithMillauMessagesInstance};
	use bridge_runtime_common::{
		assert_complete_bridge_types,
		integrity::{
			assert_complete_bridge_constants, check_message_lane_weights,
			AssertBridgeMessagesPalletConstants, AssertBridgePalletNames, AssertChainConstants,
			AssertCompleteBridgeConstants,
		},
	};

	#[test]
	fn ensure_rialto_message_lane_weights_are_correct() {
		check_message_lane_weights::<bp_rialto::Rialto, Runtime>(
			bp_millau::EXTRA_STORAGE_PROOF_SIZE,
			bp_rialto::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
			bp_rialto::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
		);
	}

	#[test]
	fn ensure_bridge_integrity() {
		assert_complete_bridge_types!(
			runtime: Runtime,
			with_bridged_chain_grandpa_instance: MillauGrandpaInstance,
			with_bridged_chain_messages_instance: WithMillauMessagesInstance,
			bridge: WithMillauMessageBridge,
			this_chain: bp_rialto::Rialto,
			bridged_chain: bp_millau::Millau,
		);

		assert_complete_bridge_constants::<
			Runtime,
			MillauGrandpaInstance,
			WithMillauMessagesInstance,
			WithMillauMessageBridge,
			bp_rialto::Rialto,
		>(AssertCompleteBridgeConstants {
			this_chain_constants: AssertChainConstants {
				block_length: bp_rialto::BlockLength::get(),
				block_weights: bp_rialto::BlockWeights::get(),
			},
			messages_pallet_constants: AssertBridgeMessagesPalletConstants {
				max_unrewarded_relayers_in_bridged_confirmation_tx:
					bp_millau::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
				max_unconfirmed_messages_in_bridged_confirmation_tx:
					bp_millau::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
				bridged_chain_id: bp_runtime::MILLAU_CHAIN_ID,
			},
			pallet_names: AssertBridgePalletNames {
				with_this_chain_messages_pallet_name: bp_rialto::WITH_RIALTO_MESSAGES_PALLET_NAME,
				with_bridged_chain_grandpa_pallet_name: bp_millau::WITH_MILLAU_GRANDPA_PALLET_NAME,
				with_bridged_chain_messages_pallet_name:
					bp_millau::WITH_MILLAU_MESSAGES_PALLET_NAME,
			},
		});
	}
}
