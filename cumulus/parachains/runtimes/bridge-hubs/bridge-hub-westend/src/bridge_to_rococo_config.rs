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

//! Bridge definitions used on BridgeHub with the Westend flavor.

use crate::{
	bridge_common_config::{AssetHubLocation, BridgeRelayersInstance},
	weights,
	xcm_config::UniversalLocation,
	AccountId, Balance, Balances, BridgeRococoMessages, PolkadotXcm, Runtime, RuntimeEvent,
	RuntimeHoldReason, XcmOverBridgeHubRococo, XcmRouter,
};
use alloc::{vec, vec::Vec};
use bp_messages::{
	source_chain::FromBridgedChainMessagesDeliveryProof,
	target_chain::FromBridgedChainMessagesProof, LegacyLaneId,
};
use bp_polkadot_core::parachains::{ParaHead, ParaId};
use bp_runtime::HeaderOf;
use bridge_hub_common::xcm_version::XcmVersionOfDestAndRemoteBridge;
use codec::{Decode, Encode};
use pallet_xcm_bridge::XcmAsPlainPayload;

use frame_support::{
	parameter_types,
	traits::{ConstU32, PalletInfoAccess},
};
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
use sp_runtime::traits::{Convert, Header};
use testnet_parachains_constants::westend::currency::UNITS as WND;
use xcm::{
	latest::{prelude::*, ROCOCO_GENESIS_HASH},
	prelude::{InteriorLocation, NetworkId},
};
use xcm_builder::{BridgeBlobDispatcher, ParentIsPreset, SiblingParachainConvertsVia};

parameter_types! {
	pub const RelayChainHeadersToKeep: u32 = 1024;
	pub const ParachainHeadsToKeep: u32 = 64;

	pub const RococoBridgeParachainPalletName: &'static str = "Paras";
	pub const MaxRococoParaHeadDataSize: u32 = bp_rococo::MAX_NESTED_PARACHAIN_HEAD_DATA_SIZE;

	pub BridgeWestendToRococoMessagesPalletInstance: InteriorLocation = [PalletInstance(<BridgeRococoMessages as PalletInfoAccess>::index() as u8)].into();
	pub RococoGlobalConsensusNetwork: NetworkId = NetworkId::ByGenesis(ROCOCO_GENESIS_HASH);
	pub RococoGlobalConsensusNetworkLocation: Location = Location::new(
		2,
		[GlobalConsensus(RococoGlobalConsensusNetwork::get())]
	);
	// see the `FEE_BOOST_PER_RELAY_HEADER` constant get the meaning of this value
	pub PriorityBoostPerRelayHeader: u64 = 32_007_814_407_814;
	// see the `FEE_BOOST_PER_PARACHAIN_HEADER` constant get the meaning of this value
	pub PriorityBoostPerParachainHeader: u64 = 1_396_340_903_540_903;
	// see the `FEE_BOOST_PER_MESSAGE` constant to get the meaning of this value
	pub PriorityBoostPerMessage: u64 = 364_088_888_888_888;

	pub BridgeHubRococoLocation: Location = Location::new(
		2,
		[
			GlobalConsensus(RococoGlobalConsensusNetwork::get()),
			Parachain(<bp_bridge_hub_rococo::BridgeHubRococo as bp_runtime::Parachain>::PARACHAIN_ID)
		]
	);

	pub storage BridgeDeposit: Balance = 10 * WND;
	pub storage DeliveryRewardInBalance: u64 = 1_000_000;
}

/// Proof of messages, coming from Rococo.
pub type FromRococoBridgeHubMessagesProof<MI> =
	FromBridgedChainMessagesProof<bp_bridge_hub_rococo::Hash, LaneIdOf<Runtime, MI>>;
/// Messages delivery proof for Rococo Bridge Hub -> Westend Bridge Hub messages.
pub type ToRococoBridgeHubMessagesDeliveryProof<MI> =
	FromBridgedChainMessagesDeliveryProof<bp_bridge_hub_rococo::Hash, LaneIdOf<Runtime, MI>>;

/// Transaction extension that refunds relayers that are delivering messages from the Rococo
/// parachain.
pub type OnBridgeHubWestendRefundBridgeHubRococoMessages = BridgeRelayersTransactionExtension<
	Runtime,
	WithMessagesExtensionConfig<
		StrOnBridgeHubWestendRefundBridgeHubRococoMessages,
		Runtime,
		WithBridgeHubRococoMessagesInstance,
		BridgeRelayersInstance,
		PriorityBoostPerMessage,
	>,
>;
bp_runtime::generate_static_str_provider!(OnBridgeHubWestendRefundBridgeHubRococoMessages);

/// Add GRANDPA bridge pallet to track Rococo relay chain.
pub type BridgeGrandpaRococoInstance = pallet_bridge_grandpa::Instance1;
impl pallet_bridge_grandpa::Config<BridgeGrandpaRococoInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type BridgedChain = bp_rococo::Rococo;
	type MaxFreeHeadersPerBlock = ConstU32<4>;
	type FreeHeadersInterval = ConstU32<5>;
	type HeadersToKeep = RelayChainHeadersToKeep;
	type WeightInfo = weights::pallet_bridge_grandpa::WeightInfo<Runtime>;
}

/// Add parachain bridge pallet to track Rococo BridgeHub parachain
pub type BridgeParachainRococoInstance = pallet_bridge_parachains::Instance1;
impl pallet_bridge_parachains::Config<BridgeParachainRococoInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_bridge_parachains::WeightInfo<Runtime>;
	type BridgesGrandpaPalletInstance = BridgeGrandpaRococoInstance;
	type ParasPalletName = RococoBridgeParachainPalletName;
	type ParaStoredHeaderDataBuilder =
		(bp_bridge_hub_rococo::BridgeHubRococo, bp_asset_hub_rococo::AssetHubRococo);
	type HeadsToKeep = ParachainHeadsToKeep;
	type MaxParaHeadDataSize = MaxRococoParaHeadDataSize;
	type OnNewHead = (
		// Sync AHR headers with state roots.
		pallet_bridge_proof_root_sync::impls::SyncParaHeadersFor<
			Runtime,
			AssetHubRococoStateRootSyncInstance,
			bp_asset_hub_rococo::AssetHubRococo,
		>,
	);
}

/// `OnSend` implementation that sends validated AHR headers to AHW.
pub struct ToAssetHubWestendProofRootSender;
impl pallet_bridge_proof_root_sync::OnSend<ParaId, ParaHead> for ToAssetHubWestendProofRootSender {
	fn on_send(roots: &Vec<(ParaId, ParaHead)>) {
		// For smaller messages, we just send minimal data.
		let roots = roots
			.iter()
			.filter_map(|(id, head)| {
				let header: HeaderOf<bp_asset_hub_rococo::AssetHubRococo> =
					match Decode::decode(&mut &head.0[..]) {
						Ok(header) => header,
						Err(error) => {
							tracing::warn!(
								target: "runtime::bridge-xcm::on-send",
								?head,
								para_id = ?id,
								?error,
								"Failed to decode parachain header - skipping it!"
							);
							return None;
						},
					};
				// We just need block_hash and state_root.
				Some((header.hash(), *header.state_root()))
			})
			.collect::<Vec<_>>();

		// Send dedicated `Transact` to AHW.
		let xcm = Xcm(vec![
			UnpaidExecution { weight_limit: Unlimited, check_origin: None },
			Transact {
				origin_kind: OriginKind::Xcm,
				fallback_max_weight: None,
				call: bp_asset_hub_westend::Call::AssetHubRococoProofRootStore(
					bp_asset_hub_westend::ProofRootStoreCall::note_new_roots {
						roots: roots.clone(),
					},
				)
				.encode()
				.into(),
			},
			ExpectTransactStatus(MaybeErrorCode::Success),
		]);
		if let Err(error) = PolkadotXcm::send_xcm(Here, AssetHubLocation::get(), xcm) {
			tracing::warn!(
				target: "runtime::bridge-xcm::on-send",
				?error,
				"Failed to send XCM"
			);
		}
	}

	fn on_send_weight() -> Weight {
		<<Runtime as pallet_xcm::Config>::WeightInfo as pallet_xcm::WeightInfo>::send()
	}
}

/// Simple mechanism that syncs/sends validated Asset Hub Rococo headers to other local chains.
/// For example,
///  1. We need AHR headers for direct bridge messaging on AHW (ToAssetHubWestendProofRootSender).
///  2. We may need AHR headers for D-Day detection on Collectives (ToCollectivesProofRootSender).
pub type AssetHubRococoStateRootSyncInstance = pallet_bridge_proof_root_sync::Instance1;
impl pallet_bridge_proof_root_sync::Config<AssetHubRococoStateRootSyncInstance> for Runtime {
	type Key = ParaId;
	type Value = ParaHead;
	type RootsToKeep = ParachainHeadsToKeep;
	type MaxRootsToSend = ParachainHeadsToKeep;
	type OnSend = (ToAssetHubWestendProofRootSender,);
}

/// Add XCM messages support for BridgeHubWestend to support Westend->Rococo XCM messages
pub type WithBridgeHubRococoMessagesInstance = pallet_bridge_messages::Instance1;
impl pallet_bridge_messages::Config<WithBridgeHubRococoMessagesInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_bridge_messages::WeightInfo<Runtime>;

	type ThisChain = bp_bridge_hub_westend::BridgeHubWestend;
	type BridgedChain = bp_bridge_hub_rococo::BridgeHubRococo;
	type BridgedHeaderChain = pallet_bridge_parachains::ParachainHeaders<
		Runtime,
		BridgeParachainRococoInstance,
		bp_bridge_hub_rococo::BridgeHubRococo,
	>;

	type OutboundPayload = XcmAsPlainPayload;
	type InboundPayload = XcmAsPlainPayload;
	type LaneId = LegacyLaneId;

	type DeliveryPayments = ();
	type DeliveryConfirmationPayments = pallet_bridge_relayers::DeliveryConfirmationPaymentsAdapter<
		Runtime,
		WithBridgeHubRococoMessagesInstance,
		BridgeRelayersInstance,
		DeliveryRewardInBalance,
	>;

	type MessageDispatch = XcmOverBridgeHubRococo;
	type OnMessagesDelivered = XcmOverBridgeHubRococo;
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
					bp_asset_hub_westend::XcmBridgeHubRouterTransactCallMaxWeight::get(),
				),
				call: encoded_call.into(),
			},
			ExpectTransactStatus(MaybeErrorCode::Success),
		])
	}
}

/// Add support for the export and dispatch of XCM programs.
pub type XcmOverBridgeHubRococoInstance = pallet_xcm_bridge::Instance1;
impl pallet_xcm_bridge::Config<XcmOverBridgeHubRococoInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_xcm_bridge::WeightInfo<Runtime>;

	type UniversalLocation = UniversalLocation;
	type BridgedNetwork = RococoGlobalConsensusNetworkLocation;
	type BridgeMessagesPalletInstance = WithBridgeHubRococoMessagesInstance;

	type MessageExportPrice = ();
	type DestinationVersion = XcmVersionOfDestAndRemoteBridge<PolkadotXcm, BridgeHubRococoLocation>;

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
		XcmOverBridgeHubRococoInstance,
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
			BridgeWestendToRococoMessagesPalletInstance,
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
	const FEE_BOOST_PER_MESSAGE: Balance = 2 * WND;

	// see `FEE_BOOST_PER_MESSAGE` comment
	const FEE_BOOST_PER_RELAY_HEADER: Balance = 2 * WND;
	// see `FEE_BOOST_PER_MESSAGE` comment
	const FEE_BOOST_PER_PARACHAIN_HEADER: Balance = 2 * WND;

	#[test]
	fn ensure_bridge_hub_westend_message_lane_weights_are_correct() {
		check_message_lane_weights::<
			bp_bridge_hub_westend::BridgeHubWestend,
			Runtime,
			WithBridgeHubRococoMessagesInstance,
		>(
			bp_bridge_hub_rococo::EXTRA_STORAGE_PROOF_SIZE,
			bp_bridge_hub_westend::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
			bp_bridge_hub_westend::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
			true,
		);
	}

	#[test]
	fn ensure_bridge_integrity() {
		assert_complete_bridge_types!(
			runtime: Runtime,
			with_bridged_chain_messages_instance: WithBridgeHubRococoMessagesInstance,
			this_chain: bp_bridge_hub_westend::BridgeHubWestend,
			bridged_chain: bp_bridge_hub_rococo::BridgeHubRococo,
			expected_payload_type: XcmAsPlainPayload,
		);

		assert_complete_with_parachain_bridge_constants::<
			Runtime,
			BridgeGrandpaRococoInstance,
			WithBridgeHubRococoMessagesInstance,
		>(AssertCompleteBridgeConstants {
			this_chain_constants: AssertChainConstants {
				block_length: bp_bridge_hub_westend::BlockLength::get(),
				block_weights: bp_bridge_hub_westend::BlockWeightsForAsyncBacking::get(),
			},
		});

		pallet_bridge_relayers::extension::per_relay_header::ensure_priority_boost_is_sane::<
			Runtime,
			BridgeGrandpaRococoInstance,
			PriorityBoostPerRelayHeader,
		>(FEE_BOOST_PER_RELAY_HEADER);

		pallet_bridge_relayers::extension::per_parachain_header::ensure_priority_boost_is_sane::<
			Runtime,
			WithBridgeHubRococoMessagesInstance,
			bp_bridge_hub_rococo::BridgeHubRococo,
			PriorityBoostPerParachainHeader,
		>(FEE_BOOST_PER_PARACHAIN_HEADER);

		pallet_bridge_relayers::extension::per_message::ensure_priority_boost_is_sane::<
			Runtime,
			WithBridgeHubRococoMessagesInstance,
			PriorityBoostPerMessage,
		>(FEE_BOOST_PER_MESSAGE);

		assert_eq!(
			BridgeWestendToRococoMessagesPalletInstance::get(),
			[PalletInstance(
				bp_bridge_hub_westend::WITH_BRIDGE_WESTEND_TO_ROCOCO_MESSAGES_PALLET_INDEX
			)]
		);
	}
}

/// Contains the migration for the AssetHubWestend<>AssetHubRococo bridge.
pub mod migration {
	use super::*;
	use bp_messages::LegacyLaneId;

	parameter_types! {
		pub AssetHubWestendToAssetHubRococoMessagesLane: LegacyLaneId = LegacyLaneId([0, 0, 0, 2]);
		pub AssetHubWestendLocation: Location = Location::new(1, [Parachain(bp_asset_hub_westend::ASSET_HUB_WESTEND_PARACHAIN_ID)]);
		pub AssetHubRococoUniversalLocation: InteriorLocation = [GlobalConsensus(RococoGlobalConsensusNetwork::get()), Parachain(bp_asset_hub_rococo::ASSET_HUB_ROCOCO_PARACHAIN_ID)].into();
	}

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
