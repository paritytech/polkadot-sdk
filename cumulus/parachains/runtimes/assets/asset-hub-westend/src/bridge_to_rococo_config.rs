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

//! Bridge definitions used on BridgeHubWestend for bridging to BridgeHubRococo.

use crate::{
	bridge_common_config::{BridgeRelayersInstance, DeliveryRewardInBalance},
	weights, xcm_config,
	xcm_config::UniversalLocation,
	AccountId, AssetHubRococoProofRootStore, Balance, Balances, BridgeRococoMessages, PolkadotXcm,
	Runtime, RuntimeEvent, RuntimeHoldReason, ToRococoOverAssetHubRococoXcmRouter,
	XcmOverAssetHubRococo,
};
use alloc::{vec, vec::Vec};
use bp_messages::HashedLaneId;
use bp_runtime::HashOf;
use bridge_hub_common::xcm_version::XcmVersionOfDestAndRemoteBridge;
use pallet_xcm_bridge::XcmAsPlainPayload;

use frame_support::{
	parameter_types,
	traits::{EitherOf, EitherOfDiverse, Equals, PalletInfoAccess},
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
use sp_runtime::traits::{ConstU32, Convert};
use testnet_parachains_constants::westend::currency::UNITS as WND;
use xcm::{
	latest::{prelude::*, ROCOCO_GENESIS_HASH},
	prelude::NetworkId,
};
use xcm_builder::{
	BridgeBlobDispatcher, LocalExporter, ParentIsPreset, SiblingParachainConvertsVia,
};

parameter_types! {
	pub BridgeWestendToRococoMessagesPalletInstance: InteriorLocation = [PalletInstance(<BridgeRococoMessages as PalletInfoAccess>::index() as u8)].into();
	pub const HereLocation: Location = Location::here();
	pub RococoGlobalConsensusNetwork: NetworkId = NetworkId::ByGenesis(ROCOCO_GENESIS_HASH);
	pub RococoGlobalConsensusNetworkLocation: Location = Location::new(
		2,
		[GlobalConsensus(RococoGlobalConsensusNetwork::get())]
	);
	// see the `FEE_BOOST_PER_MESSAGE` constant to get the meaning of this value
	pub PriorityBoostPerMessage: u64 = 364_088_888_888_888;

	// The other side of the bridge
	pub AssetHubRococoLocation: Location = Location::new(
		2,
		[
			GlobalConsensus(RococoGlobalConsensusNetwork::get()),
			Parachain(<bp_asset_hub_rococo::AssetHubRococo as bp_runtime::Parachain>::PARACHAIN_ID)
		]
	);

	pub storage BridgeDeposit: Balance = 5 * WND;
}

/// Transaction extension that refunds relayers that are delivering messages from the Rococo
/// parachain.
pub type OnAssetHubWestendRefundAssetHubRococoMessages = BridgeRelayersTransactionExtension<
	Runtime,
	WithMessagesExtensionConfig<
		StrOnAssetHubWestendRefundAssetHubRococoMessages,
		Runtime,
		WithAssetHubRococoMessagesInstance,
		BridgeRelayersInstance,
		PriorityBoostPerMessage,
	>,
>;
bp_runtime::generate_static_str_provider!(OnAssetHubWestendRefundAssetHubRococoMessages);

/// Add XCM messages support for AssetHubWestend to support Westend->Rococo XCM messages
pub type WithAssetHubRococoMessagesInstance = pallet_bridge_messages::Instance1;
impl pallet_bridge_messages::Config<WithAssetHubRococoMessagesInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_bridge_messages::WeightInfo<Runtime>;

	type ThisChain = bp_asset_hub_westend::AssetHubWestend;
	type BridgedChain = bp_asset_hub_rococo::AssetHubRococo;
	type BridgedHeaderChain = AssetHubRococoHeaders;

	type OutboundPayload = XcmAsPlainPayload;
	type InboundPayload = XcmAsPlainPayload;
	type LaneId = HashedLaneId;

	type DeliveryPayments = ();
	type DeliveryConfirmationPayments = pallet_bridge_relayers::DeliveryConfirmationPaymentsAdapter<
		Runtime,
		WithAssetHubRococoMessagesInstance,
		BridgeRelayersInstance,
		DeliveryRewardInBalance,
	>;

	type MessageDispatch = XcmOverAssetHubRococo;
	type OnMessagesDelivered = XcmOverAssetHubRococo;
}

/// Add support for storing bridged AssetHubRococo state roots.
pub type AssetHubRococoProofRootStoreInstance = pallet_bridge_proof_root_store::Instance1;
impl pallet_bridge_proof_root_store::Config<AssetHubRococoProofRootStoreInstance> for Runtime {
	// TOOD: FAIL-CI weights
	type WeightInfo = ();
	type SubmitOrigin = EitherOfDiverse<
		// `Root` can do whatever
		EnsureRoot<AccountId>,
		// and only the local BridgeHub can send updates.
		EnsureXcm<Equals<xcm_config::bridging::SiblingBridgeHub>>,
	>;
	// Means `block_hash` of AHR.
	type Key =
		HashOf<pallet_bridge_messages::BridgedChainOf<Runtime, WithAssetHubRococoMessagesInstance>>;
	// Means `state_root` of AHR.
	type Value =
		HashOf<pallet_bridge_messages::BridgedChainOf<Runtime, WithAssetHubRococoMessagesInstance>>;
	// Configured according to the BHW's `ParachainHeadsToKeep`
	type RootsToKeep = ConstU32<64>;
}

/// Adapter `bp_header_chain::HeaderChain` implementation which resolves AssetHubRococo `state_root`
/// for `block_hash`.
pub struct AssetHubRococoHeaders;
impl
	bp_header_chain::HeaderChain<
		pallet_bridge_messages::BridgedChainOf<Runtime, WithAssetHubRococoMessagesInstance>,
	> for AssetHubRococoHeaders
{
	fn finalized_header_state_root(
		header_hash: HashOf<
			pallet_bridge_messages::BridgedChainOf<Runtime, WithAssetHubRococoMessagesInstance>,
		>,
	) -> Option<
		HashOf<pallet_bridge_messages::BridgedChainOf<Runtime, WithAssetHubRococoMessagesInstance>>,
	> {
		AssetHubRococoProofRootStore::get_root(&header_hash)
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
/// `WithAssetHubRococoMessagesInstance`.
pub type XcmOverAssetHubRococoInstance = pallet_xcm_bridge::Instance1;
impl pallet_xcm_bridge::Config<XcmOverAssetHubRococoInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_xcm_bridge::WeightInfo<Runtime>;

	type UniversalLocation = UniversalLocation;
	type BridgedNetwork = RococoGlobalConsensusNetworkLocation;
	type BridgeMessagesPalletInstance = WithAssetHubRococoMessagesInstance;

	// TODO: FAIL-CI: we need to setup some price or configure per location?
	type MessageExportPrice = ();
	type DestinationVersion = XcmVersionOfDestAndRemoteBridge<PolkadotXcm, AssetHubRococoLocation>;

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
		ToRococoOverAssetHubRococoXcmRouter,
		// handles congestion for other local chains with XCM using `update_bridge_status` sent to
		// the sending chain.
		UpdateBridgeStatusXcmChannelManager<
			Runtime,
			XcmOverAssetHubRococoInstance,
			UpdateBridgeStatusXcmProvider,
			xcm_config::LocalXcmRouter,
		>,
	>;
	// Dispatching inbound messages from the bridge and managing congestion with the local
	// receiving/destination chain
	type BlobDispatcher = BlobDispatcherWithChannelStatus<
		// Dispatches received XCM messages from other bridge
		BridgeBlobDispatcher<
			xcm_config::LocalXcmRouter,
			UniversalLocation,
			// TODO: FAIL-CI wait for https://github.com/paritytech/polkadot-sdk/pull/6002#issuecomment-2469892343
			BridgeWestendToRococoMessagesPalletInstance,
		>,
		// Provides the status of the XCMP queue's outbound queue, indicating whether messages can
		// be dispatched to the sibling.
		cumulus_pallet_xcmp_queue::bridging::OutXcmpChannelStatusProvider<Runtime>,
	>;
	type CongestionLimits = ();
}

/// XCM router instance to the local `pallet_xcm_bridge::<XcmOverAssetHubRococoInstance>` with
/// direct bridging capabilities for `Rococo` global consensus with dynamic fees and back-pressure.
pub type ToRococoOverAssetHubRococoXcmRouterInstance = pallet_xcm_bridge_router::Instance2;
impl pallet_xcm_bridge_router::Config<ToRococoOverAssetHubRococoXcmRouterInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo =
		weights::pallet_xcm_bridge_router_to_rococo_over_asset_hub_rococo::WeightInfo<Runtime>;

	type DestinationVersion = PolkadotXcm;

	// We use `LocalExporter` with `ViaLocalBridgeHubExporter` ensures that
	// `pallet_xcm_bridge_router` can trigger directly `pallet_xcm_bridge` as exporter.
	type MessageExporter = pallet_xcm_bridge_router::impls::ViaLocalBridgeExporter<
		Runtime,
		ToRococoOverAssetHubRococoXcmRouterInstance,
		LocalExporter<XcmOverAssetHubRococo, UniversalLocation>,
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
	use crate::RuntimeCall;
	use bridge_runtime_common::{
		assert_complete_bridge_types,
		integrity::{
			assert_standalone_messages_bridge_constants, check_message_lane_weights,
			AssertChainConstants, AssertCompleteBridgeConstants,
		},
	};
	use codec::Encode;
	use frame_support::BoundedVec;

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

	#[test]
	fn ensure_bridge_hub_westend_message_lane_weights_are_correct() {
		check_message_lane_weights::<
			bp_asset_hub_westend::AssetHubWestend,
			Runtime,
			WithAssetHubRococoMessagesInstance,
		>(
			bp_asset_hub_rococo::EXTRA_STORAGE_PROOF_SIZE,
			bp_asset_hub_westend::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
			bp_asset_hub_westend::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
			true,
		);
	}

	#[test]
	fn ensure_bridge_integrity() {
		assert_complete_bridge_types!(
			runtime: Runtime,
			with_bridged_chain_messages_instance: WithAssetHubRococoMessagesInstance,
			this_chain: bp_asset_hub_westend::AssetHubWestend,
			bridged_chain: bp_asset_hub_rococo::AssetHubRococo,
			expected_payload_type: XcmAsPlainPayload,
		);

		assert_standalone_messages_bridge_constants::<Runtime, WithAssetHubRococoMessagesInstance>(
			AssertCompleteBridgeConstants {
				this_chain_constants: AssertChainConstants {
					block_length: bp_bridge_hub_westend::BlockLength::get(),
					block_weights: bp_bridge_hub_westend::BlockWeightsForAsyncBacking::get(),
				},
			},
		);

		pallet_bridge_relayers::extension::per_message::ensure_priority_boost_is_sane::<
			Runtime,
			WithAssetHubRococoMessagesInstance,
			PriorityBoostPerMessage,
		>(FEE_BOOST_PER_MESSAGE);

		let expected: InteriorLocation = [PalletInstance(
			bp_asset_hub_westend::WITH_BRIDGE_WESTEND_TO_ROCOCO_MESSAGES_PALLET_INDEX,
		)]
		.into();
		assert_eq!(BridgeWestendToRococoMessagesPalletInstance::get(), expected);
	}

	#[test]
	fn ensure_encoding_compatibility() {
		let hash = HashOf::<
			pallet_bridge_messages::BridgedChainOf<Runtime, WithAssetHubRococoMessagesInstance>,
		>::from([1; 32]);
		let roots = vec![(hash, hash), (hash, hash)];

		assert_eq!(
			bp_asset_hub_westend::Call::AssetHubRococoProofRootStore(
				bp_asset_hub_westend::ProofRootStoreCall::note_new_roots { roots: roots.clone() }
			)
			.encode(),
			RuntimeCall::AssetHubRococoProofRootStore(
				pallet_bridge_proof_root_store::Call::note_new_roots {
					roots: BoundedVec::truncate_from(roots)
				}
			)
			.encode()
		);
	}
}
