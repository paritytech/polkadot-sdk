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

//! Everything required to serve Millau <-> RialtoParachain messages.

use crate::{Runtime, RuntimeCall, RuntimeOrigin, WithRialtoParachainsInstance};

use bp_messages::{LaneId, MessageNonce};
use bp_runtime::{ChainId, MILLAU_CHAIN_ID, RIALTO_PARACHAIN_CHAIN_ID};
use bridge_runtime_common::messages::{
	self, source::TargetHeaderChainAdapter, target::SourceHeaderChainAdapter, MessageBridge,
};
use frame_support::{parameter_types, weights::Weight, RuntimeDebug};

/// Default lane that is used to send messages to Rialto parachain.
pub const XCM_LANE: LaneId = LaneId([0, 0, 0, 0]);
/// Weight of 2 XCM instructions is for simple `Trap(42)` program, coming through bridge
/// (it is prepended with `UniversalOrigin` instruction). It is used just for simplest manual
/// tests, confirming that we don't break encoding somewhere between.
pub const BASE_XCM_WEIGHT_TWICE: Weight = crate::xcm_config::BaseXcmWeight::get().saturating_mul(2);

parameter_types! {
	/// Weight credit for our test messages.
	///
	/// 2 XCM instructions is for simple `Trap(42)` program, coming through bridge
	/// (it is prepended with `UniversalOrigin` instruction).
	pub const WeightCredit: Weight = BASE_XCM_WEIGHT_TWICE;
}

/// Message payload for Millau -> RialtoParachain messages.
pub type ToRialtoParachainMessagePayload = messages::source::FromThisChainMessagePayload;

/// Message verifier for Millau -> RialtoParachain messages.
pub type ToRialtoParachainMessageVerifier =
	messages::source::FromThisChainMessageVerifier<WithRialtoParachainMessageBridge>;

/// Message payload for RialtoParachain -> Millau messages.
pub type FromRialtoParachainMessagePayload =
	messages::target::FromBridgedChainMessagePayload<RuntimeCall>;

/// Call-dispatch based message dispatch for RialtoParachain -> Millau messages.
pub type FromRialtoParachainMessageDispatch = messages::target::FromBridgedChainMessageDispatch<
	WithRialtoParachainMessageBridge,
	xcm_executor::XcmExecutor<crate::xcm_config::XcmConfig>,
	crate::xcm_config::XcmWeigher,
	WeightCredit,
>;

/// Maximal outbound payload size of Millau -> RialtoParachain messages.
pub type ToRialtoParachainMaximalOutboundPayloadSize =
	messages::source::FromThisChainMaximalOutboundPayloadSize<WithRialtoParachainMessageBridge>;

/// Millau <-> RialtoParachain message bridge.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct WithRialtoParachainMessageBridge;

impl MessageBridge for WithRialtoParachainMessageBridge {
	const THIS_CHAIN_ID: ChainId = MILLAU_CHAIN_ID;
	const BRIDGED_CHAIN_ID: ChainId = RIALTO_PARACHAIN_CHAIN_ID;
	const BRIDGED_MESSAGES_PALLET_NAME: &'static str = bp_millau::WITH_MILLAU_MESSAGES_PALLET_NAME;

	type ThisChain = Millau;
	type BridgedChain = RialtoParachain;
	type BridgedHeaderChain = pallet_bridge_parachains::ParachainHeaders<
		Runtime,
		WithRialtoParachainsInstance,
		bp_rialto_parachain::RialtoParachain,
	>;
}

/// Millau chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct Millau;

impl messages::UnderlyingChainProvider for Millau {
	type Chain = bp_millau::Millau;
}

impl messages::ThisChainWithMessages for Millau {
	type RuntimeCall = RuntimeCall;
	type RuntimeOrigin = RuntimeOrigin;

	fn is_message_accepted(_send_origin: &Self::RuntimeOrigin, _lane: &LaneId) -> bool {
		true
	}

	fn maximal_pending_messages_at_outbound_lane() -> MessageNonce {
		MessageNonce::MAX
	}
}

/// RialtoParachain chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct RialtoParachain;
/// RialtoParachain as source header chain.
pub type RialtoParachainAsSourceHeaderChain =
	SourceHeaderChainAdapter<WithRialtoParachainMessageBridge>;
/// RialtoParachain as target header chain.
pub type RialtoParachainAsTargetHeaderChain =
	TargetHeaderChainAdapter<WithRialtoParachainMessageBridge>;

impl messages::UnderlyingChainProvider for RialtoParachain {
	type Chain = bp_rialto_parachain::RialtoParachain;
}

impl messages::BridgedChainWithMessages for RialtoParachain {
	fn verify_dispatch_weight(_message_payload: &[u8]) -> bool {
		true
	}
}
