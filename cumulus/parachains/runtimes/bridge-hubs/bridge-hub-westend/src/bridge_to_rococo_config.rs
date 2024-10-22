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

//! Bridge definitions used on BridgeHub with the Westend flavor.

use crate::{
	bridge_common_config::{DeliveryRewardInBalance, RelayersForLegacyLaneIdsMessagesInstance},
	weights,
	xcm_config::UniversalLocation,
	AccountId, Balance, Balances, BridgeRococoMessages, PolkadotXcm, Runtime, RuntimeEvent,
	RuntimeHoldReason, XcmOverBridgeHubRococo, XcmRouter,
};
use bp_messages::{
	source_chain::FromBridgedChainMessagesDeliveryProof,
	target_chain::FromBridgedChainMessagesProof, LegacyLaneId,
};
use bp_parachains::SingleParaStoredHeaderDataBuilder;
use bridge_hub_common::xcm_version::XcmVersionOfDestAndRemoteBridge;
use pallet_xcm_bridge_hub::XcmAsPlainPayload;

use frame_support::{
	parameter_types,
	traits::{ConstU32, PalletInfoAccess},
};
use frame_system::{EnsureNever, EnsureRoot};
use pallet_bridge_messages::LaneIdOf;
use pallet_bridge_relayers::extension::{
	BridgeRelayersTransactionExtension, WithMessagesExtensionConfig,
};
use parachains_common::xcm_config::{AllSiblingSystemParachains, RelayOrOtherSystemParachains};
use polkadot_parachain_primitives::primitives::Sibling;
use testnet_parachains_constants::westend::currency::UNITS as WND;
use xcm::{
	latest::prelude::*,
	prelude::{InteriorLocation, NetworkId},
};
use xcm_builder::{BridgeBlobDispatcher, ParentIsPreset, SiblingParachainConvertsVia};

parameter_types! {
	pub const RelayChainHeadersToKeep: u32 = 1024;
	pub const ParachainHeadsToKeep: u32 = 64;

	pub const RococoBridgeParachainPalletName: &'static str = "Paras";
	pub const MaxRococoParaHeadDataSize: u32 = bp_rococo::MAX_NESTED_PARACHAIN_HEAD_DATA_SIZE;

	pub BridgeWestendToRococoMessagesPalletInstance: InteriorLocation = [PalletInstance(<BridgeRococoMessages as PalletInfoAccess>::index() as u8)].into();
	pub RococoGlobalConsensusNetwork: NetworkId = NetworkId::Rococo;
	pub RococoGlobalConsensusNetworkLocation: Location = Location::new(
		2,
		[GlobalConsensus(RococoGlobalConsensusNetwork::get())]
	);
	// see the `FEE_BOOST_PER_RELAY_HEADER` constant get the meaning of this value
	pub PriorityBoostPerRelayHeader: u64 = 32_007_814_407_814;
	// see the `FEE_BOOST_PER_PARACHAIN_HEADER` constant get the meaning of this value
	pub PriorityBoostPerParachainHeader: u64 = 1_396_340_903_540_903;
	// see the `FEE_BOOST_PER_MESSAGE` constant to get the meaning of this value
	pub PriorityBoostPerMessage: u64 = 182_044_444_444_444;

	pub BridgeHubRococoLocation: Location = Location::new(
		2,
		[
			GlobalConsensus(RococoGlobalConsensusNetwork::get()),
			Parachain(<bp_bridge_hub_rococo::BridgeHubRococo as bp_runtime::Parachain>::PARACHAIN_ID)
		]
	);

	pub storage BridgeDeposit: Balance = 10 * WND;
}

/// Proof of messages, coming from Rococo.
pub type FromRococoBridgeHubMessagesProof<MI> =
	FromBridgedChainMessagesProof<bp_bridge_hub_rococo::Hash, LaneIdOf<Runtime, MI>>;
/// Messages delivery proof for Rococo Bridge Hub -> Westend Bridge Hub messages.
pub type ToRococoBridgeHubMessagesDeliveryProof<MI> =
	FromBridgedChainMessagesDeliveryProof<bp_bridge_hub_rococo::Hash, LaneIdOf<Runtime, MI>>;

/// Dispatches received XCM messages from other bridge
type FromRococoMessageBlobDispatcher =
	BridgeBlobDispatcher<XcmRouter, UniversalLocation, BridgeWestendToRococoMessagesPalletInstance>;

/// Transaction extension that refunds relayers that are delivering messages from the Rococo
/// parachain.
pub type OnBridgeHubWestendRefundBridgeHubRococoMessages = BridgeRelayersTransactionExtension<
	Runtime,
	WithMessagesExtensionConfig<
		StrOnBridgeHubWestendRefundBridgeHubRococoMessages,
		Runtime,
		WithBridgeHubRococoMessagesInstance,
		RelayersForLegacyLaneIdsMessagesInstance,
		PriorityBoostPerMessage,
	>,
	LaneIdOf<Runtime, WithBridgeHubRococoMessagesInstance>,
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
		SingleParaStoredHeaderDataBuilder<bp_bridge_hub_rococo::BridgeHubRococo>;
	type HeadsToKeep = ParachainHeadsToKeep;
	type MaxParaHeadDataSize = MaxRococoParaHeadDataSize;
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
		DeliveryRewardInBalance,
	>;

	type MessageDispatch = XcmOverBridgeHubRococo;
	type OnMessagesDelivered = XcmOverBridgeHubRococo;
}

/// Add support for the export and dispatch of XCM programs.
pub type XcmOverBridgeHubRococoInstance = pallet_xcm_bridge_hub::Instance1;
impl pallet_xcm_bridge_hub::Config<XcmOverBridgeHubRococoInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;

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

	// TODO:(bridges-v2) - add `LocalXcmChannelManager` impl - https://github.com/paritytech/parity-bridges-common/issues/3047
	type LocalXcmChannelManager = ();
	type BlobDispatcher = FromRococoMessageBlobDispatcher;
}

#[cfg(feature = "runtime-benchmarks")]
pub(crate) fn open_bridge_for_benchmarks<R, XBHI, C>(
	with: pallet_xcm_bridge_hub::LaneIdOf<R, XBHI>,
	sibling_para_id: u32,
) -> InteriorLocation
where
	R: pallet_xcm_bridge_hub::Config<XBHI>,
	XBHI: 'static,
	C: xcm_executor::traits::ConvertLocation<
		bp_runtime::AccountIdOf<pallet_xcm_bridge_hub::ThisChainOf<R, XBHI>>,
	>,
{
	use pallet_xcm_bridge_hub::{Bridge, BridgeId, BridgeState};
	use sp_runtime::traits::Zero;
	use xcm::VersionedInteriorLocation;

	// insert bridge metadata
	let lane_id = with;
	let sibling_parachain = Location::new(1, [Parachain(sibling_para_id)]);
	let universal_source = [GlobalConsensus(Westend), Parachain(sibling_para_id)].into();
	let universal_destination = [GlobalConsensus(Rococo), Parachain(2075)].into();
	let bridge_id = BridgeId::new(&universal_source, &universal_destination);

	// insert only bridge metadata, because the benchmarks create lanes
	pallet_xcm_bridge_hub::Bridges::<R, XBHI>::insert(
		bridge_id,
		Bridge {
			bridge_origin_relative_location: alloc::boxed::Box::new(
				sibling_parachain.clone().into(),
			),
			bridge_origin_universal_location: alloc::boxed::Box::new(
				VersionedInteriorLocation::from(universal_source.clone()),
			),
			bridge_destination_universal_location: alloc::boxed::Box::new(
				VersionedInteriorLocation::from(universal_destination),
			),
			state: BridgeState::Opened,
			bridge_owner_account: C::convert_location(&sibling_parachain).expect("valid AccountId"),
			deposit: Zero::zero(),
			lane_id,
		},
	);
	pallet_xcm_bridge_hub::LaneToBridge::<R, XBHI>::insert(lane_id, bridge_id);

	universal_source
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
			with_bridged_chain_grandpa_instance: BridgeGrandpaRococoInstance,
			with_bridged_chain_messages_instance: WithBridgeHubRococoMessagesInstance,
			this_chain: bp_bridge_hub_westend::BridgeHubWestend,
			bridged_chain: bp_bridge_hub_rococo::BridgeHubRococo,
		);

		assert_complete_with_parachain_bridge_constants::<
			Runtime,
			BridgeGrandpaRococoInstance,
			WithBridgeHubRococoMessagesInstance,
			bp_rococo::Rococo,
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
	use frame_support::traits::ConstBool;

	parameter_types! {
		pub AssetHubWestendToAssetHubRococoMessagesLane: LegacyLaneId = LegacyLaneId([0, 0, 0, 2]);
		pub AssetHubWestendLocation: Location = Location::new(1, [Parachain(bp_asset_hub_westend::ASSET_HUB_WESTEND_PARACHAIN_ID)]);
		pub AssetHubRococoUniversalLocation: InteriorLocation = [GlobalConsensus(RococoGlobalConsensusNetwork::get()), Parachain(bp_asset_hub_rococo::ASSET_HUB_ROCOCO_PARACHAIN_ID)].into();
	}

	/// Ensure that the existing lanes for the AHW<>AHR bridge are correctly configured.
	pub type StaticToDynamicLanes = pallet_xcm_bridge_hub::migration::OpenBridgeForLane<
		Runtime,
		XcmOverBridgeHubRococoInstance,
		AssetHubWestendToAssetHubRococoMessagesLane,
		// the lanes are already created for AHR<>AHW, but we need to link them to the bridge
		// structs
		ConstBool<false>,
		AssetHubWestendLocation,
		AssetHubRococoUniversalLocation,
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
