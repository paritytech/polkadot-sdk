// Copyright Parity Technologies (UK) Ltd.
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

//! Polkadot BridgeHub definitions.

use crate::{
	BridgeKusamaMessages, BridgeParachainKusamaInstance, Runtime,
	WithBridgeHubKusamaMessagesInstance, XcmRouter,
};
use bp_messages::LaneId;
use bridge_runtime_common::{
	messages,
	messages::{
		source::FromBridgedChainMessagesDeliveryProof, target::FromBridgedChainMessagesProof,
		MessageBridge, ThisChainWithMessages, UnderlyingChainProvider,
	},
	messages_xcm_extension::{SenderAndLane, XcmBlobHauler, XcmBlobHaulerAdapter},
	refund_relayer_extension::{
		ActualFeeRefund, RefundBridgedParachainMessages, RefundableMessagesLane,
		RefundableParachain,
	},
};
use codec::Encode;
use frame_support::{parameter_types, traits::PalletInfoAccess};
use sp_runtime::RuntimeDebug;
use xcm::{latest::prelude::*, prelude::NetworkId};
use xcm_builder::{BridgeBlobDispatcher, HaulBlobExporter};

parameter_types! {
	pub const MaxUnrewardedRelayerEntriesAtInboundLane: bp_messages::MessageNonce =
		bp_bridge_hub_polkadot::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX;
	pub const MaxUnconfirmedMessagesAtInboundLane: bp_messages::MessageNonce =
		bp_bridge_hub_polkadot::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
	pub const BridgeHubKusamaChainId: bp_runtime::ChainId = bp_runtime::BRIDGE_HUB_KUSAMA_CHAIN_ID;
	pub BridgeKusamaMessagesPalletInstance: InteriorMultiLocation = X1(PalletInstance(<BridgeKusamaMessages as PalletInfoAccess>::index() as u8));
	pub KusamaGlobalConsensusNetwork: NetworkId = NetworkId::Kusama;
	// see the `FEE_BOOST_PER_MESSAGE` constant to get the meaning of this value
	pub PriorityBoostPerMessage: u64 = 4_551_111_111_111;

	pub AssetHubPolkadotParaId: cumulus_primitives_core::ParaId = 1000.into();

	pub FromAssetHubPolkadotToAssetHubKusamaRoute: SenderAndLane = SenderAndLane::new(
		ParentThen(X1(Parachain(AssetHubPolkadotParaId::get().into()))).into(),
		ASSET_HUB_POLKADOT_TO_ASSET_HUB_KUSAMA_LANE_ID,
	);

	pub CongestedMessage: Xcm<()> = sp_std::vec![Transact {
		origin_kind: OriginKind::Xcm,
		require_weight_at_most: bp_asset_hub_polkadot::XcmBridgeHubRouterTransactCallMaxWeight::get(),
		call: bp_asset_hub_polkadot::Call::ToKusamaXcmRouter(
			bp_asset_hub_polkadot::XcmBridgeHubRouterCall::report_bridge_status {
				bridge_id: Default::default(),
				is_congested: true,
			}
		).encode().into(),
	}].into();

	pub UncongestedMessage: Xcm<()> = sp_std::vec![Transact {
		origin_kind: OriginKind::Xcm,
		require_weight_at_most: bp_asset_hub_polkadot::XcmBridgeHubRouterTransactCallMaxWeight::get(),
		call: bp_asset_hub_polkadot::Call::ToKusamaXcmRouter(
			bp_asset_hub_polkadot::XcmBridgeHubRouterCall::report_bridge_status {
				bridge_id: Default::default(),
				is_congested: false,
			}
		).encode().into(),
	}].into();
}

/// Proof of messages, coming from BridgeHubKusama.
pub type FromBridgeHubKusamaMessagesProof =
	FromBridgedChainMessagesProof<bp_bridge_hub_kusama::Hash>;
/// Message delivery proof for `BridgeHubKusama` messages.
pub type ToBridgeHubKusamaMessagesDeliveryProof =
	FromBridgedChainMessagesDeliveryProof<bp_bridge_hub_kusama::Hash>;

/// Dispatches received XCM messages from other bridge
pub type OnThisChainBlobDispatcher<UniversalLocation> =
	BridgeBlobDispatcher<XcmRouter, UniversalLocation, BridgeKusamaMessagesPalletInstance>;

/// Export XCM messages to be relayed to the other side.
pub type ToBridgeHubKusamaHaulBlobExporter = HaulBlobExporter<
	XcmBlobHaulerAdapter<ToBridgeHubKusamaXcmBlobHauler>,
	KusamaGlobalConsensusNetwork,
	(),
>;
pub struct ToBridgeHubKusamaXcmBlobHauler;
impl XcmBlobHauler for ToBridgeHubKusamaXcmBlobHauler {
	type Runtime = Runtime;
	type MessagesInstance = WithBridgeHubKusamaMessagesInstance;
	type SenderAndLane = FromAssetHubPolkadotToAssetHubKusamaRoute;

	type ToSourceChainSender = crate::XcmRouter;
	type CongestedMessage = CongestedMessage;
	type UncongestedMessage = UncongestedMessage;
}

/// On messages delivered callback.
pub type OnMessagesDelivered = XcmBlobHaulerAdapter<ToBridgeHubKusamaXcmBlobHauler>;

/// Messaging Bridge configuration for ThisChain -> BridgeHubKusama
pub struct WithBridgeHubKusamaMessageBridge;
impl MessageBridge for WithBridgeHubKusamaMessageBridge {
	const BRIDGED_MESSAGES_PALLET_NAME: &'static str =
		bp_bridge_hub_polkadot::WITH_BRIDGE_HUB_POLKADOT_MESSAGES_PALLET_NAME;
	type ThisChain = ThisChain;
	type BridgedChain = BridgeHubKusama;
	type BridgedHeaderChain = pallet_bridge_parachains::ParachainHeaders<
		Runtime,
		BridgeParachainKusamaInstance,
		bp_bridge_hub_kusama::BridgeHubKusama,
	>;
}

/// Message verifier for BridgeHubKusama messages sent from ThisChain
pub type ToBridgeHubKusamaMessageVerifier =
	messages::source::FromThisChainMessageVerifier<WithBridgeHubKusamaMessageBridge>;

/// Maximal outbound payload size of ThisChain -> BridgeHubKusama messages.
pub type ToBridgeHubKusamaMaximalOutboundPayloadSize =
	messages::source::FromThisChainMaximalOutboundPayloadSize<WithBridgeHubKusamaMessageBridge>;

/// BridgeHubKusama chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct BridgeHubKusama;

impl UnderlyingChainProvider for BridgeHubKusama {
	type Chain = bp_bridge_hub_kusama::BridgeHubKusama;
}

impl messages::BridgedChainWithMessages for BridgeHubKusama {}

/// ThisChain chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct ThisChain;

impl UnderlyingChainProvider for ThisChain {
	type Chain = bp_bridge_hub_polkadot::BridgeHubPolkadot;
}

impl ThisChainWithMessages for ThisChain {
	type RuntimeOrigin = crate::RuntimeOrigin;
}

// TODO: rework once dynamic lanes are supported (https://github.com/paritytech/parity-bridges-common/issues/1760)
/// Signed extension that refunds relayers that are delivering messages from the kusama BridgeHub.
pub type BridgeRefundBridgeHubKusamaMessages = RefundBridgedParachainMessages<
	Runtime,
	RefundableParachain<BridgeParachainKusamaInstance, BridgeHubKusama>,
	RefundableMessagesLane<WithBridgeHubKusamaMessagesInstance, StatemintToStatemineMessageLane>,
	ActualFeeRefund<Runtime>,
	PriorityBoostPerMessage,
	StrBridgeRefundBridgeHubKusamaMessages,
>;
bp_runtime::generate_static_str_provider!(BridgeRefundBridgeHubKusamaMessages);

// TODO: rework once dynamic lanes are supported (https://github.com/paritytech/parity-bridges-common/issues/1760)
//       now we support only AHP<>AHK Lanes setup
pub const ASSET_HUB_POLKADOT_TO_ASSET_HUB_KUSAMA_LANE_ID: LaneId = LaneId([0, 0, 0, 0]);
parameter_types! {
	pub ActiveOutboundLanesToBridgeHubKusama: &'static [bp_messages::LaneId] = &[ASSET_HUB_POLKADOT_TO_ASSET_HUB_KUSAMA_LANE_ID];
	pub const StatemintToStatemineMessageLane: bp_messages::LaneId = ASSET_HUB_POLKADOT_TO_ASSET_HUB_KUSAMA_LANE_ID;
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::BridgeGrandpaKusamaInstance;
	use bridge_runtime_common::{
		assert_complete_bridge_types,
		integrity::{
			assert_complete_bridge_constants, check_message_lane_weights,
			AssertBridgeMessagesPalletConstants, AssertBridgePalletNames, AssertChainConstants,
			AssertCompleteBridgeConstants,
		},
	};
	use parachains_common::{polkadot, Balance};

	/// Every additional message in the message delivery transaction boosts its priority.
	/// So the priority of transaction with `N+1` messages is larger than priority of
	/// transaction with `N` messages by the `PriorityBoostPerMessage`.
	///
	/// Economically, it is an equivalent of adding tip to the transaction with `N` messages.
	/// The `FEE_BOOST_PER_MESSAGE` constant is the value of this tip.
	///
	/// We want this tip to be large enough (delivery transactions with more messages = less
	/// operational costs and a faster bridge), so this value should be significant.
	const FEE_BOOST_PER_MESSAGE: Balance = 5 * polkadot::currency::UNITS;

	#[test]
	fn ensure_lane_weights_are_correct() {
		check_message_lane_weights::<
			bp_bridge_hub_polkadot::BridgeHubPolkadot,
			Runtime,
			WithBridgeHubKusamaMessagesInstance,
		>(
			bp_bridge_hub_kusama::EXTRA_STORAGE_PROOF_SIZE,
			bp_bridge_hub_polkadot::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
			bp_bridge_hub_polkadot::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
			true,
		);
	}

	#[test]
	fn ensure_bridge_integrity() {
		assert_complete_bridge_types!(
			runtime: Runtime,
			with_bridged_chain_grandpa_instance: BridgeGrandpaKusamaInstance,
			with_bridged_chain_messages_instance: WithBridgeHubKusamaMessagesInstance,
			bridge: WithBridgeHubKusamaMessageBridge,
			this_chain: bp_polkadot::Polkadot,
			bridged_chain: bp_kusama::Kusama,
		);

		assert_complete_bridge_constants::<
			Runtime,
			BridgeGrandpaKusamaInstance,
			WithBridgeHubKusamaMessagesInstance,
			WithBridgeHubKusamaMessageBridge,
		>(AssertCompleteBridgeConstants {
			this_chain_constants: AssertChainConstants {
				block_length: bp_bridge_hub_polkadot::BlockLength::get(),
				block_weights: bp_bridge_hub_polkadot::BlockWeights::get(),
			},
			messages_pallet_constants: AssertBridgeMessagesPalletConstants {
				max_unrewarded_relayers_in_bridged_confirmation_tx:
					bp_bridge_hub_kusama::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
				max_unconfirmed_messages_in_bridged_confirmation_tx:
					bp_bridge_hub_kusama::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
				bridged_chain_id: bp_runtime::BRIDGE_HUB_KUSAMA_CHAIN_ID,
			},
			pallet_names: AssertBridgePalletNames {
				with_this_chain_messages_pallet_name:
					bp_bridge_hub_polkadot::WITH_BRIDGE_HUB_POLKADOT_MESSAGES_PALLET_NAME,
				with_bridged_chain_grandpa_pallet_name: bp_kusama::WITH_KUSAMA_GRANDPA_PALLET_NAME,
				with_bridged_chain_messages_pallet_name:
					bp_bridge_hub_kusama::WITH_BRIDGE_HUB_KUSAMA_MESSAGES_PALLET_NAME,
			},
		});

		bridge_runtime_common::priority_calculator::ensure_priority_boost_is_sane::<
			Runtime,
			WithBridgeHubKusamaMessagesInstance,
			PriorityBoostPerMessage,
		>(FEE_BOOST_PER_MESSAGE);
	}
}
