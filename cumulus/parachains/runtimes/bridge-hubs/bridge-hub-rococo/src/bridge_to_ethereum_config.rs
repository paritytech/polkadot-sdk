use bp_messages::LaneId;
use bp_messages::source_chain::OnMessagesDelivered;
use bridge_runtime_common::messages::source::TargetHeaderChainAdapter;
use bridge_runtime_common::messages::target::SourceHeaderChainAdapter;
use bridge_runtime_common::messages_xcm_extension::{XcmAsPlainPayload, XcmBlobHauler, XcmBlobHaulerAdapter, XcmBlobMessageDispatch};
use frame_support::parameter_types;
use xcm_builder::HaulBlobExporter;
use crate::bridge_common_config::DeliveryRewardInBalance;
use crate::bridge_to_rococo_config::{ActiveOutboundLanesToBridgeHubRococo, AssetHubWococoParaId, BridgeHubRococoChainId, CongestedMessage, MaxUnconfirmedMessagesAtInboundLane, MaxUnrewardedRelayerEntriesAtInboundLane, RococoGlobalConsensusNetwork, ToBridgeHubRococoMaximalOutboundPayloadSize, ToBridgeHubRococoMessageVerifier, UncongestedMessage, WithBridgeHubRococoMessageBridge};
use crate::weights;
use crate::xcm_config::XcmRouter;

parameter_types! {
	pub EthereumGlobalConsensusNetwork: NetworkId = NetworkId::Ethereum { chain_id: 15 };
	pub FromAssetHubRococoToBridgehubRococoRoute: SenderAndLane = SenderAndLane::new(
		ParentThen(X1(Parachain(AssetHubRococoParaId::get().into()))).into(),
		XCM_LANE_FOR_ASSET_HUB_ROCOCO_TO_BRIDGE_HUB_ROCOCO,
	);
}
pub const XCM_LANE_FOR_ASSET_HUB_ROCOCO_TO_BRIDGE_HUB_ROCOCO: LaneId = LaneId([0, 0, 0, 3]);

/// Export XCM messages to be relayed to the other side
pub type ToBridgeHubEthereumBlobExporter = HaulBlobExporter<
	XcmBlobHaulerAdapter<ToBridgeHubEthereumXcmBlobHauler>,
	EthereumGlobalConsensusNetwork,
	(),
>;
pub struct ToBridgeHubEthereumXcmBlobHauler;
impl XcmBlobHauler for ToBridgeHubEthereumXcmBlobHauler {
	type Runtime = Runtime;
	type MessagesInstance = WithBridgeHubRococoMessagesInstance;
	type SenderAndLane = FromAssetHubRococoToBridgehubRococoRoute;

	type ToSourceChainSender = XcmRouter;
	type CongestedMessage = CongestedMessage;
	type UncongestedMessage = UncongestedMessage;
}

pub type WithBridgeHubRococoMessagesInstance = pallet_bridge_messages::Instance4;
impl pallet_bridge_messages::Config<WithBridgeHubRococoMessagesInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_bridge_messages_wococo_to_rococo::WeightInfo<Runtime>;
	type BridgedChainId = BridgeHubRococoChainId;
	type ActiveOutboundLanes = ActiveOutboundLanesToBridgeHubRococo;
	type MaxUnrewardedRelayerEntriesAtInboundLane = MaxUnrewardedRelayerEntriesAtInboundLane;
	type MaxUnconfirmedMessagesAtInboundLane = MaxUnconfirmedMessagesAtInboundLane;

	type MaximalOutboundPayloadSize = ToBridgeHubRococoMaximalOutboundPayloadSize;
	type OutboundPayload = XcmAsPlainPayload;

	type InboundPayload = XcmAsPlainPayload;
	type InboundRelayer = AccountId;
	type DeliveryPayments = ();

	type TargetHeaderChain = TargetHeaderChainAdapter<WithBridgeHubRococoMessageBridge>;
	type LaneMessageVerifier = ToBridgeHubRococoMessageVerifier;
	type DeliveryConfirmationPayments = pallet_bridge_relayers::DeliveryConfirmationPaymentsAdapter<
		Runtime,
		WithBridgeHubRococoMessagesInstance,
		DeliveryRewardInBalance,
	>;

	type SourceHeaderChain = SourceHeaderChainAdapter<WithBridgeHubRococoMessageBridge>;
	type MessageDispatch = XcmBlobMessageDispatch<
		FromRococoMessageBlobDispatcher,
		Self::WeightInfo,
		cumulus_pallet_xcmp_queue::bridging::OutXcmpChannelStatusProvider<
			AssetHubWococoParaId,
			Runtime,
		>,
	>;
	type OnMessagesDelivered = OnMessagesDelivered;
}
