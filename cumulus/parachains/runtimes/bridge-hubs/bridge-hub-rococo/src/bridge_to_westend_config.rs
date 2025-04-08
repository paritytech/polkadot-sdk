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

//! Bridge definitions used on BridgeHubRococo for bridging to BridgeHubWestend.

use crate::{
	bridge_common_config::{
		BridgeParachainWestendInstance, DeliveryRewardInBalance,
		RelayersForLegacyLaneIdsMessagesInstance,
	},
	weights,
	xcm_config::UniversalLocation,
	AccountId, Balance, Balances, BridgeWestendMessages, PolkadotXcm, Runtime, RuntimeEvent,
	RuntimeHoldReason, XcmOverBridgeHubWestend, XcmRouter,
};
use alloc::{vec, vec::Vec};
use bp_messages::{
	source_chain::FromBridgedChainMessagesDeliveryProof,
	target_chain::FromBridgedChainMessagesProof, LegacyLaneId,
};
use bridge_hub_common::xcm_version::XcmVersionOfDestAndRemoteBridge;
use pallet_xcm_bridge::XcmAsPlainPayload;

use frame_support::{parameter_types, traits::PalletInfoAccess};
use frame_system::{EnsureNever, EnsureRoot};
use pallet_bridge_messages::LaneIdOf;
use pallet_bridge_relayers::extension::{
	BridgeRelayersTransactionExtension, WithMessagesExtensionConfig,
};
use pallet_xcm_bridge::congestion::{
	BlobDispatcherWithChannelStatus, UpdateBridgeStatusXcmChannelManager,
};
use parachains_common::xcm_config::{AllSiblingSystemParachains, RelayOrOtherSystemParachains};
use polkadot_parachain_primitives::primitives::Sibling;
use sp_runtime::traits::Convert;
use testnet_parachains_constants::rococo::currency::UNITS as ROC;
use xcm::{
	latest::{prelude::*, WESTEND_GENESIS_HASH},
	prelude::{InteriorLocation, NetworkId},
};
use xcm_builder::{BridgeBlobDispatcher, ParentIsPreset, SiblingParachainConvertsVia};

parameter_types! {
	pub BridgeRococoToWestendMessagesPalletInstance: InteriorLocation = [PalletInstance(<BridgeWestendMessages as PalletInfoAccess>::index() as u8)].into();
	pub WestendGlobalConsensusNetwork: NetworkId = NetworkId::ByGenesis(WESTEND_GENESIS_HASH);
	pub WestendGlobalConsensusNetworkLocation: Location = Location::new(
		2,
		[GlobalConsensus(WestendGlobalConsensusNetwork::get())]
	);
	// see the `FEE_BOOST_PER_RELAY_HEADER` constant get the meaning of this value
	pub PriorityBoostPerRelayHeader: u64 = 32_007_814_407_814;
	// see the `FEE_BOOST_PER_PARACHAIN_HEADER` constant get the meaning of this value
	pub PriorityBoostPerParachainHeader: u64 = 1_396_340_903_540_903;
	// see the `FEE_BOOST_PER_MESSAGE` constant to get the meaning of this value
	pub PriorityBoostPerMessage: u64 = 364_088_888_888_888;

	pub BridgeHubWestendLocation: Location = Location::new(
		2,
		[
			GlobalConsensus(WestendGlobalConsensusNetwork::get()),
			Parachain(<bp_bridge_hub_westend::BridgeHubWestend as bp_runtime::Parachain>::PARACHAIN_ID)
		]
	);

	pub storage BridgeDeposit: Balance = 5 * ROC;
}

/// Proof of messages, coming from Westend.
pub type FromWestendBridgeHubMessagesProof<MI> =
	FromBridgedChainMessagesProof<bp_bridge_hub_westend::Hash, LaneIdOf<Runtime, MI>>;
/// Messages delivery proof for Rococo Bridge Hub -> Westend Bridge Hub messages.
pub type ToWestendBridgeHubMessagesDeliveryProof<MI> =
	FromBridgedChainMessagesDeliveryProof<bp_bridge_hub_westend::Hash, LaneIdOf<Runtime, MI>>;

/// Transaction extension that refunds relayers that are delivering messages from the Westend
/// parachain.
pub type OnBridgeHubRococoRefundBridgeHubWestendMessages = BridgeRelayersTransactionExtension<
	Runtime,
	WithMessagesExtensionConfig<
		StrOnBridgeHubRococoRefundBridgeHubWestendMessages,
		Runtime,
		WithBridgeHubWestendMessagesInstance,
		RelayersForLegacyLaneIdsMessagesInstance,
		PriorityBoostPerMessage,
	>,
>;
bp_runtime::generate_static_str_provider!(OnBridgeHubRococoRefundBridgeHubWestendMessages);

/// Add XCM messages support for BridgeHubRococo to support Rococo->Westend XCM messages
pub type WithBridgeHubWestendMessagesInstance = pallet_bridge_messages::Instance3;
impl pallet_bridge_messages::Config<WithBridgeHubWestendMessagesInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_bridge_messages_rococo_to_westend::WeightInfo<Runtime>;

	type ThisChain = bp_bridge_hub_rococo::BridgeHubRococo;
	type BridgedChain = bp_bridge_hub_westend::BridgeHubWestend;
	type BridgedHeaderChain = pallet_bridge_parachains::ParachainHeaders<
		Runtime,
		BridgeParachainWestendInstance,
		bp_bridge_hub_westend::BridgeHubWestend,
	>;

	type OutboundPayload = XcmAsPlainPayload;
	type InboundPayload = XcmAsPlainPayload;
	type LaneId = LegacyLaneId;

	type DeliveryPayments = ();
	type DeliveryConfirmationPayments = pallet_bridge_relayers::DeliveryConfirmationPaymentsAdapter<
		Runtime,
		WithBridgeHubWestendMessagesInstance,
		RelayersForLegacyLaneIdsMessagesInstance,
		DeliveryRewardInBalance,
	>;

	type MessageDispatch = XcmOverBridgeHubWestend;
	type OnMessagesDelivered = XcmOverBridgeHubWestend;
}

/// Converts encoded call to the unpaid XCM `Transact`.
pub struct UpdateBridgeStatusXcmProvider;
impl Convert<Vec<u8>, Xcm<()>> for UpdateBridgeStatusXcmProvider {
	fn convert(encoded_call: Vec<u8>) -> Xcm<()> {
		Xcm(vec![
			UnpaidExecution { weight_limit: Unlimited, check_origin: None },
			Transact {
				origin_kind: OriginKind::Xcm,
				fallback_max_weight: Some(
					bp_asset_hub_rococo::XcmBridgeHubRouterTransactCallMaxWeight::get(),
				),
				call: encoded_call.into(),
			},
			ExpectTransactStatus(MaybeErrorCode::Success),
		])
	}
}

/// Add support for the export and dispatch of XCM programs withing
/// `WithBridgeHubWestendMessagesInstance`.
pub type XcmOverBridgeHubWestendInstance = pallet_xcm_bridge::Instance1;
impl pallet_xcm_bridge::Config<XcmOverBridgeHubWestendInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_xcm_bridge_over_westend::WeightInfo<Runtime>;

	type UniversalLocation = UniversalLocation;
	type BridgedNetwork = WestendGlobalConsensusNetworkLocation;
	type BridgeMessagesPalletInstance = WithBridgeHubWestendMessagesInstance;

	type MessageExportPrice = ();
	type DestinationVersion =
		XcmVersionOfDestAndRemoteBridge<PolkadotXcm, BridgeHubWestendLocation>;

	type ForceOrigin = EnsureRoot<AccountId>;
	// We don't want to allow creating bridges for this instance with `LegacyLaneId`.
	type OpenBridgeOrigin = EnsureNever<Location>;
	// Converter aligned with `OpenBridgeOrigin`.
	type BridgeOriginAccountIdConverter =
		(ParentIsPreset<AccountId>, SiblingParachainConvertsVia<Sibling, AccountId>);

	type BridgeDeposit = BridgeDeposit;
	type Currency = Balances;
	type RuntimeHoldReason = RuntimeHoldReason;
	// Do not require deposit from system parachains or relay chain
	type AllowWithoutBridgeDeposit =
		RelayOrOtherSystemParachains<AllSiblingSystemParachains, Runtime>;

	// This pallet is deployed on BH, so we expect a remote router with `ExportMessage`. We handle
	// congestion with XCM using `update_bridge_status` sent to the sending chain. (congestion with
	// local sending chain)
	type LocalXcmChannelManager = UpdateBridgeStatusXcmChannelManager<
		Runtime,
		XcmOverBridgeHubWestendInstance,
		UpdateBridgeStatusXcmProvider,
		XcmRouter,
	>;
	// Dispatching inbound messages from the bridge and managing congestion with the local
	// receiving/destination chain
	type BlobDispatcher = BlobDispatcherWithChannelStatus<
		// Dispatches received XCM messages from other bridge
		BridgeBlobDispatcher<
			XcmRouter,
			UniversalLocation,
			BridgeRococoToWestendMessagesPalletInstance,
		>,
		// Provides the status of the XCMP queue's outbound queue, indicating whether messages can
		// be dispatched to the sibling.
		cumulus_pallet_xcmp_queue::bridging::OutXcmpChannelStatusProvider<Runtime>,
	>;
	type CongestionLimits = ();
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::bridge_common_config::BridgeGrandpaWestendInstance;
	use bridge_runtime_common::{
		assert_complete_bridge_types,
		integrity::{
			assert_complete_with_parachain_bridge_constants, check_message_lane_weights,
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

	// see `FEE_BOOST_PER_MESSAGE` comment
	const FEE_BOOST_PER_RELAY_HEADER: Balance = 2 * ROC;
	// see `FEE_BOOST_PER_MESSAGE` comment
	const FEE_BOOST_PER_PARACHAIN_HEADER: Balance = 2 * ROC;

	#[test]
	fn ensure_bridge_hub_rococo_message_lane_weights_are_correct() {
		check_message_lane_weights::<
			bp_bridge_hub_rococo::BridgeHubRococo,
			Runtime,
			WithBridgeHubWestendMessagesInstance,
		>(
			bp_bridge_hub_westend::EXTRA_STORAGE_PROOF_SIZE,
			bp_bridge_hub_rococo::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
			bp_bridge_hub_rococo::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
			true,
		);
	}

	#[test]
	fn ensure_bridge_integrity() {
		assert_complete_bridge_types!(
			runtime: Runtime,
			with_bridged_chain_messages_instance: WithBridgeHubWestendMessagesInstance,
			this_chain: bp_bridge_hub_rococo::BridgeHubRococo,
			bridged_chain: bp_bridge_hub_westend::BridgeHubWestend,
			expected_payload_type: XcmAsPlainPayload,
		);

		assert_complete_with_parachain_bridge_constants::<
			Runtime,
			BridgeGrandpaWestendInstance,
			WithBridgeHubWestendMessagesInstance,
		>(AssertCompleteBridgeConstants {
			this_chain_constants: AssertChainConstants {
				block_length: bp_bridge_hub_rococo::BlockLength::get(),
				block_weights: bp_bridge_hub_rococo::BlockWeightsForAsyncBacking::get(),
			},
		});

		pallet_bridge_relayers::extension::per_relay_header::ensure_priority_boost_is_sane::<
			Runtime,
			BridgeGrandpaWestendInstance,
			PriorityBoostPerRelayHeader,
		>(FEE_BOOST_PER_RELAY_HEADER);

		pallet_bridge_relayers::extension::per_parachain_header::ensure_priority_boost_is_sane::<
			Runtime,
			WithBridgeHubWestendMessagesInstance,
			bp_bridge_hub_westend::BridgeHubWestend,
			PriorityBoostPerParachainHeader,
		>(FEE_BOOST_PER_PARACHAIN_HEADER);

		pallet_bridge_relayers::extension::per_message::ensure_priority_boost_is_sane::<
			Runtime,
			WithBridgeHubWestendMessagesInstance,
			PriorityBoostPerMessage,
		>(FEE_BOOST_PER_MESSAGE);

		let expected: InteriorLocation = [PalletInstance(
			bp_bridge_hub_rococo::WITH_BRIDGE_ROCOCO_TO_WESTEND_MESSAGES_PALLET_INDEX,
		)]
		.into();

		assert_eq!(BridgeRococoToWestendMessagesPalletInstance::get(), expected,);
	}
}

/// Contains the migration for the AssetHubRococo<>AssetHubWestend bridge.
pub mod migration {
	use super::*;
	use frame_support::traits::ConstBool;

	parameter_types! {
		pub AssetHubRococoToAssetHubWestendMessagesLane: LegacyLaneId = LegacyLaneId([0, 0, 0, 2]);
		pub AssetHubRococoLocation: Location = Location::new(1, [Parachain(bp_asset_hub_rococo::ASSET_HUB_ROCOCO_PARACHAIN_ID)]);
		pub AssetHubWestendUniversalLocation: InteriorLocation = [GlobalConsensus(WestendGlobalConsensusNetwork::get()), Parachain(bp_asset_hub_westend::ASSET_HUB_WESTEND_PARACHAIN_ID)].into();
	}

	/// Ensure that the existing lanes for the AHR<>AHW bridge are correctly configured.
	pub type StaticToDynamicLanes = pallet_xcm_bridge::migration::OpenBridgeForLane<
		Runtime,
		XcmOverBridgeHubWestendInstance,
		AssetHubRococoToAssetHubWestendMessagesLane,
		// the lanes are already created for AHR<>AHW, but we need to link them to the bridge
		// structs
		ConstBool<false>,
		AssetHubRococoLocation,
		AssetHubWestendUniversalLocation,
		(),
	>;

	mod v1_wrong {
		use bp_messages::{LaneState, MessageNonce, UnrewardedRelayer};
		use bp_runtime::AccountIdOf;
		use codec::{Decode, Encode};
		use pallet_bridge_messages::BridgedChainOf;
		use sp_std::collections::vec_deque::VecDeque;

		#[derive(Encode, Decode, Clone, PartialEq, Eq)]
		pub(crate) struct StoredInboundLaneData<T: pallet_bridge_messages::Config<I>, I: 'static>(
			pub(crate) InboundLaneData<AccountIdOf<BridgedChainOf<T, I>>>,
		);
		#[derive(Encode, Decode, Clone, PartialEq, Eq)]
		pub(crate) struct InboundLaneData<RelayerId> {
			pub state: LaneState,
			pub(crate) relayers: VecDeque<UnrewardedRelayer<RelayerId>>,
			pub(crate) last_confirmed_nonce: MessageNonce,
		}
		#[derive(Encode, Decode, Clone, PartialEq, Eq)]
		pub(crate) struct OutboundLaneData {
			pub state: LaneState,
			pub(crate) oldest_unpruned_nonce: MessageNonce,
			pub(crate) latest_received_nonce: MessageNonce,
			pub(crate) latest_generated_nonce: MessageNonce,
		}
	}

	mod v1 {
		pub use bp_messages::{InboundLaneData, LaneState, OutboundLaneData};
		pub use pallet_bridge_messages::{InboundLanes, OutboundLanes, StoredInboundLaneData};
	}

	/// Fix for v1 migration - corrects data for OutboundLaneData/InboundLaneData (it is needed only
	/// for Rococo/Westend).
	pub struct FixMessagesV1Migration<T, I>(sp_std::marker::PhantomData<(T, I)>);

	impl<T: pallet_bridge_messages::Config<I>, I: 'static> frame_support::traits::OnRuntimeUpgrade
		for FixMessagesV1Migration<T, I>
	{
		fn on_runtime_upgrade() -> Weight {
			use sp_core::Get;
			let mut weight = T::DbWeight::get().reads(1);

			// `InboundLanes` - add state to the old structs
			let translate_inbound =
				|pre: v1_wrong::StoredInboundLaneData<T, I>| -> Option<v1::StoredInboundLaneData<T, I>> {
					weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
					Some(v1::StoredInboundLaneData(v1::InboundLaneData {
						state: v1::LaneState::Opened,
						relayers: pre.0.relayers,
						last_confirmed_nonce: pre.0.last_confirmed_nonce,
					}))
				};
			v1::InboundLanes::<T, I>::translate_values(translate_inbound);

			// `OutboundLanes` - add state to the old structs
			let translate_outbound =
				|pre: v1_wrong::OutboundLaneData| -> Option<v1::OutboundLaneData> {
					weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
					Some(v1::OutboundLaneData {
						state: v1::LaneState::Opened,
						oldest_unpruned_nonce: pre.oldest_unpruned_nonce,
						latest_received_nonce: pre.latest_received_nonce,
						latest_generated_nonce: pre.latest_generated_nonce,
					})
				};
			v1::OutboundLanes::<T, I>::translate_values(translate_outbound);

			weight
		}
	}
}
