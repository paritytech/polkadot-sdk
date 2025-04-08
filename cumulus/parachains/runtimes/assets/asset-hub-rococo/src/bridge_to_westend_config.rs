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

//! Bridge definitions used on BridgeHubRococo for bridging to BridgeHubWestend.

use crate::{
	bridge_common_config::{BridgeRelayersInstance, DeliveryRewardInBalance},
	weights, xcm_config,
	xcm_config::UniversalLocation,
	AccountId, Balance, Balances, PolkadotXcm, Runtime, RuntimeEvent, RuntimeHoldReason,
	ToWestendOverAssetHubWestendXcmRouter, XcmOverAssetHubWestend,
};
use alloc::{vec, vec::Vec};
use bp_messages::HashedLaneId;
use bp_runtime::HashOf;
use bridge_hub_common::xcm_version::XcmVersionOfDestAndRemoteBridge;
use pallet_xcm_bridge::XcmAsPlainPayload;

use frame_support::{
	parameter_types,
	traits::{EitherOf, Equals},
};
use frame_system::{EnsureRoot, EnsureRootWithSuccess};
use pallet_bridge_relayers::extension::{
	BridgeRelayersTransactionExtension, WithMessagesExtensionConfig,
};
use pallet_xcm::EnsureXcm;
use pallet_xcm_bridge::congestion::{
	BlobDispatcherWithChannelStatus, HereOrLocalConsensusXcmChannelManager,
	UpdateBridgeStatusXcmChannelManager,
};
use parachains_common::xcm_config::{
	AllSiblingSystemParachains, ParentRelayOrSiblingParachains, RelayOrOtherSystemParachains,
};
use polkadot_parachain_primitives::primitives::Sibling;
use sp_runtime::traits::Convert;
use testnet_parachains_constants::rococo::currency::UNITS as ROC;
use xcm::{
	latest::{prelude::*, WESTEND_GENESIS_HASH},
	prelude::NetworkId,
};
use xcm_builder::{
	BridgeBlobDispatcher, LocalExporter, ParentIsPreset, SiblingParachainConvertsVia,
};

parameter_types! {
	pub const HereLocation: Location = Location::here();
	pub WestendGlobalConsensusNetwork: NetworkId = NetworkId::ByGenesis(WESTEND_GENESIS_HASH);
	pub WestendGlobalConsensusNetworkLocation: Location = Location::new(
		2,
		[GlobalConsensus(WestendGlobalConsensusNetwork::get())]
	);
	// see the `FEE_BOOST_PER_MESSAGE` constant to get the meaning of this value
	pub PriorityBoostPerMessage: u64 = 364_088_888_888_888;

	// The other side of the bridge
	pub AssetHubWestendLocation: Location = Location::new(
		2,
		[
			GlobalConsensus(WestendGlobalConsensusNetwork::get()),
			Parachain(<bp_asset_hub_westend::AssetHubWestend as bp_runtime::Parachain>::PARACHAIN_ID)
		]
	);

	pub storage BridgeDeposit: Balance = 5 * ROC;
}

/// Transaction extension that refunds relayers that are delivering messages from the Westend
/// parachain.
pub type OnAssetHubRococoRefundAssetHubWestendMessages = BridgeRelayersTransactionExtension<
	Runtime,
	WithMessagesExtensionConfig<
		StrOnAssetHubRococoRefundAssetHubWestendMessages,
		Runtime,
		WithAssetHubWestendMessagesInstance,
		BridgeRelayersInstance,
		PriorityBoostPerMessage,
	>,
>;
bp_runtime::generate_static_str_provider!(OnAssetHubRococoRefundAssetHubWestendMessages);

/// Add XCM messages support for AssetHubRococo to support Rococo->Westend XCM messages
pub type WithAssetHubWestendMessagesInstance = pallet_bridge_messages::Instance1;
impl pallet_bridge_messages::Config<WithAssetHubWestendMessagesInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_bridge_messages::WeightInfo<Runtime>;

	type ThisChain = bp_asset_hub_rococo::AssetHubRococo;
	type BridgedChain = bp_asset_hub_westend::AssetHubWestend;
	type BridgedHeaderChain = ParachainHeaderProofs<Self::BridgedChain>;

	type OutboundPayload = XcmAsPlainPayload;
	type InboundPayload = XcmAsPlainPayload;
	type LaneId = HashedLaneId;

	type DeliveryPayments = ();
	type DeliveryConfirmationPayments = pallet_bridge_relayers::DeliveryConfirmationPaymentsAdapter<
		Runtime,
		WithAssetHubWestendMessagesInstance,
		BridgeRelayersInstance,
		DeliveryRewardInBalance,
	>;

	type MessageDispatch = XcmOverAssetHubWestend;
	type OnMessagesDelivered = XcmOverAssetHubWestend;
}

/// TODO: doc + FAIL-CI - implement storage for synced proofs from BridgeHub
pub struct ParachainHeaderProofs<C>(core::marker::PhantomData<C>);
impl<C: bp_runtime::Chain> bp_header_chain::HeaderChain<C> for ParachainHeaderProofs<C> {
	fn finalized_header_state_root(_header_hash: HashOf<C>) -> Option<HashOf<C>> {
		todo!("TODO: FAIL-CI - implement storage for synced proofs from BridgeHub")
	}
}

/// Converts encoded call to the unpaid XCM `Transact`.
pub struct UpdateBridgeStatusXcmProvider;
impl Convert<Vec<u8>, Xcm<()>> for UpdateBridgeStatusXcmProvider {
	fn convert(encoded_call: Vec<u8>) -> Xcm<()> {
		Xcm(vec![
			UnpaidExecution { weight_limit: Unlimited, check_origin: None },
			Transact {
				origin_kind: OriginKind::Xcm,
				call: encoded_call.into(),
				// TODO: FAIL-CI - add some test for this or remove TODO
				fallback_max_weight: Some(Weight::from_parts(200_000_000, 6144)),
			},
			ExpectTransactStatus(MaybeErrorCode::Success),
		])
	}
}

/// Add support for the export and dispatch of XCM programs withing
/// `WithAssetHubWestendMessagesInstance`.
pub type XcmOverAssetHubWestendInstance = pallet_xcm_bridge::Instance1;
impl pallet_xcm_bridge::Config<XcmOverAssetHubWestendInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_xcm_bridge::WeightInfo<Runtime>;

	type UniversalLocation = UniversalLocation;
	type BridgedNetwork = WestendGlobalConsensusNetworkLocation;
	type BridgeMessagesPalletInstance = WithAssetHubWestendMessagesInstance;

	// TODO: FAIL-CI: we need to setup some price or configure per location?
	type MessageExportPrice = ();
	type DestinationVersion = XcmVersionOfDestAndRemoteBridge<PolkadotXcm, AssetHubWestendLocation>;

	type ForceOrigin = EnsureRoot<AccountId>;
	// We allow creating bridges for the runtime itself and for other local consensus chains (relay,
	// paras).
	type OpenBridgeOrigin = EitherOf<
		// We want to translate `RuntimeOrigin::root()` to the `Location::here()`, e.g. for
		// governance calls.
		EnsureRootWithSuccess<AccountId, HereLocation>,
		// For relay or sibling chains
		EnsureXcm<ParentRelayOrSiblingParachains>,
	>;
	// Converter aligned with `OpenBridgeOrigin`.
	type BridgeOriginAccountIdConverter =
		(ParentIsPreset<AccountId>, SiblingParachainConvertsVia<Sibling, AccountId>);

	type BridgeDeposit = BridgeDeposit;
	type Currency = Balances;
	type RuntimeHoldReason = RuntimeHoldReason;
	// Do not require deposit from system parachains (including itself) or relay chain
	type AllowWithoutBridgeDeposit =
		(RelayOrOtherSystemParachains<AllSiblingSystemParachains, Runtime>, Equals<HereLocation>);

	// This pallet is deployed on AH, so we expect a remote router with `ExportMessage`. We handle
	// congestion with XCM using `udpate_bridge_status` sent to the sending chain. (congestion with
	// local sending chain)
	type LocalXcmChannelManager = HereOrLocalConsensusXcmChannelManager<
		pallet_xcm_bridge::BridgeId,
		// handles congestion for local chain router for local AH's bridges
		ToWestendOverAssetHubWestendXcmRouter,
		// handles congestion for other local chains with XCM using `update_bridge_status` sent to
		// the sending chain.
		UpdateBridgeStatusXcmChannelManager<
			Runtime,
			XcmOverAssetHubWestendInstance,
			UpdateBridgeStatusXcmProvider,
			xcm_config::LocalXcmRouter,
		>,
	>;
	// Dispatching inbound messages from the bridge and managing congestion with the local
	// receiving/destination chain
	type BlobDispatcher = BlobDispatcherWithChannelStatus<
		// Dispatches received XCM messages from other bridge
		BridgeBlobDispatcher<
			// TODO: FAIL-CI wait for https://github.com/paritytech/polkadot-sdk/pull/6002#issuecomment-2469892343
			xcm_config::LocalXcmRouter,
			UniversalLocation,
			(),
		>,
		// Provides the status of the XCMP queue's outbound queue, indicating whether messages can
		// be dispatched to the sibling.
		cumulus_pallet_xcmp_queue::bridging::OutXcmpChannelStatusProvider<Runtime>,
	>;
	type CongestionLimits = ();
}

/// XCM router instance to the local `pallet_xcm_bridge::<XcmOverAssetHubWestendInstance>` with
/// direct bridging capabilities for `Westend` global consensus with dynamic fees and back-pressure.
pub type ToWestendOverAssetHubWestendXcmRouterInstance = pallet_xcm_bridge_router::Instance4;
impl pallet_xcm_bridge_router::Config<ToWestendOverAssetHubWestendXcmRouterInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo =
		weights::pallet_xcm_bridge_router_to_westend_over_asset_hub_westend::WeightInfo<Runtime>;

	type DestinationVersion = PolkadotXcm;

	// We use `LocalExporter` with `ViaLocalBridgeHubExporter` ensures that
	// `pallet_xcm_bridge_router` can trigger directly `pallet_xcm_bridge` as exporter.
	type MessageExporter = pallet_xcm_bridge_router::impls::ViaLocalBridgeExporter<
		Runtime,
		ToWestendOverAssetHubWestendXcmRouterInstance,
		LocalExporter<XcmOverAssetHubWestend, UniversalLocation>,
	>;

	// For congestion - resolves `BridgeId` using the same algorithm as `pallet_xcm_bridge` on
	// the BH.
	type BridgeIdResolver =
		pallet_xcm_bridge_router::impls::EnsureIsRemoteBridgeIdResolver<UniversalLocation>;
	// We don't expect here `update_bridge_status` calls, but let's allow just for root (governance,
	// ...).
	type UpdateBridgeStatusOrigin = EnsureRoot<AccountId>;

	// TODO: FAIL-CI - fix/add new constants
	// For adding message size fees
	type ByteFee = xcm_config::bridging::XcmBridgeHubRouterByteFee;
	// For adding message size fees
	type FeeAsset = xcm_config::bridging::XcmBridgeHubRouterFeeAssetId;
}

#[cfg(test)]
mod tests {
	use super::*;
	use bridge_runtime_common::{
		assert_complete_bridge_types,
		integrity::{
			assert_standalone_messages_bridge_constants, check_message_lane_weights,
			AssertChainConstants, AssertCompleteBridgeConstants,
		},
	};

	/// Every additional message in the message delivery transaction boosts its priority.
	/// So the priority of transaction with `N+1` messages is larger than priority of
	/// transaction with `N` messages by the `PriorityBoostPerMessage`.
	///
	/// Economically, it is an equivalent of adding tip to the transaction with `N` messages.
	/// The `FEE_BOOST_PER_MESSAGE` constant is the value of this tip.
	///
	/// We want this tip to be large enough (delivery transactions with more messages = less
	/// operational costs and a faster bridge), so this value should be significant.
	const FEE_BOOST_PER_MESSAGE: Balance = 2 * ROC;

	#[test]
	fn ensure_bridge_hub_rococo_message_lane_weights_are_correct() {
		check_message_lane_weights::<
			bp_asset_hub_rococo::AssetHubRococo,
			Runtime,
			WithAssetHubWestendMessagesInstance,
		>(
			bp_asset_hub_westend::EXTRA_STORAGE_PROOF_SIZE,
			bp_asset_hub_rococo::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
			bp_asset_hub_rococo::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
			true,
		);
	}

	#[test]
	fn ensure_bridge_integrity() {
		assert_complete_bridge_types!(
			runtime: Runtime,
			with_bridged_chain_messages_instance: WithAssetHubWestendMessagesInstance,
			this_chain: bp_asset_hub_rococo::AssetHubRococo,
			bridged_chain: bp_asset_hub_westend::AssetHubWestend,
			expected_payload_type: XcmAsPlainPayload,
		);

		assert_standalone_messages_bridge_constants::<Runtime, WithAssetHubWestendMessagesInstance>(
			AssertCompleteBridgeConstants {
				this_chain_constants: AssertChainConstants {
					block_length: bp_bridge_hub_rococo::BlockLength::get(),
					block_weights: bp_bridge_hub_rococo::BlockWeightsForAsyncBacking::get(),
				},
			},
		);

		pallet_bridge_relayers::extension::per_message::ensure_priority_boost_is_sane::<
			Runtime,
			WithAssetHubWestendMessagesInstance,
			PriorityBoostPerMessage,
		>(FEE_BOOST_PER_MESSAGE);
	}
}
