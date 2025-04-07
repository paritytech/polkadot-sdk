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

//! Bridge definitions used on BridgeHubRococo for bridging to Rococo Bulletin.
//!
//! Rococo Bulletin chain will be the 1:1 copy of the Polkadot Bulletin, so we
//! are reusing Polkadot Bulletin chain primitives everywhere here.

use crate::{
	bridge_common_config::RelayersForPermissionlessLanesInstance, weights,
	xcm_config::UniversalLocation, AccountId, Balance, Balances, BridgeRococoBulletinGrandpa,
	BridgeRococoBulletinMessages, Runtime, RuntimeEvent, RuntimeHoldReason, XcmOverRococoBulletin,
	XcmRouter,
};
use bp_messages::{
	source_chain::FromBridgedChainMessagesDeliveryProof,
	target_chain::FromBridgedChainMessagesProof, LegacyLaneId,
};

use frame_support::{
	parameter_types,
	traits::{Equals, PalletInfoAccess},
};
use frame_system::{EnsureNever, EnsureRoot};
use pallet_bridge_messages::LaneIdOf;
use pallet_bridge_relayers::extension::{
	BridgeRelayersTransactionExtension, WithMessagesExtensionConfig,
};
use pallet_xcm_bridge::{congestion::BlobDispatcherWithChannelStatus, XcmAsPlainPayload};
use polkadot_parachain_primitives::primitives::Sibling;
use testnet_parachains_constants::rococo::currency::UNITS as ROC;
use xcm::{
	latest::prelude::*,
	prelude::{InteriorLocation, NetworkId},
	AlwaysV5,
};
use xcm_builder::{BridgeBlobDispatcher, ParentIsPreset, SiblingParachainConvertsVia};

parameter_types! {
	/// Interior location (relative to this runtime) of the with-RococoBulletin messages pallet.
	pub BridgeRococoToRococoBulletinMessagesPalletInstance: InteriorLocation = [
		PalletInstance(<BridgeRococoBulletinMessages as PalletInfoAccess>::index() as u8)
	].into();
	/// Rococo Bulletin Network identifier.
	pub RococoBulletinGlobalConsensusNetwork: NetworkId = NetworkId::PolkadotBulletin;
	/// Relative location of the Rococo Bulletin chain.
	pub RococoBulletinGlobalConsensusNetworkLocation: Location = Location::new(
		2,
		[GlobalConsensus(RococoBulletinGlobalConsensusNetwork::get())]
	);

	// see the `FEE_BOOST_PER_RELAY_HEADER` constant get the meaning of this value
	pub PriorityBoostPerRelayHeader: u64 = 58_014_163_614_163;

	/// Priority boost that the registered relayer receives for every additional message in the message
	/// delivery transaction.
	///
	/// It is determined semi-automatically - see `FEE_BOOST_PER_MESSAGE` constant to get the
	/// meaning of this value.
	pub PriorityBoostPerMessage: u64 = 364_088_888_888_888;

	/// PeopleRococo location
	pub PeopleRococoLocation: Location = Location::new(1, [Parachain(rococo_runtime_constants::system_parachain::PEOPLE_ID)]);

	pub storage BridgeDeposit: Balance = 5 * ROC;
}

/// Proof of messages, coming from Rococo Bulletin chain.
pub type FromRococoBulletinMessagesProof<MI> =
	FromBridgedChainMessagesProof<bp_polkadot_bulletin::Hash, LaneIdOf<Runtime, MI>>;
/// Messages delivery proof for Rococo Bridge Hub -> Rococo Bulletin messages.
pub type ToRococoBulletinMessagesDeliveryProof<MI> =
	FromBridgedChainMessagesDeliveryProof<bp_polkadot_bulletin::Hash, LaneIdOf<Runtime, MI>>;

/// Transaction extension that refunds relayers that are delivering messages from the Rococo
/// Bulletin chain.
pub type OnBridgeHubRococoRefundRococoBulletinMessages = BridgeRelayersTransactionExtension<
	Runtime,
	WithMessagesExtensionConfig<
		StrOnBridgeHubRococoRefundRococoBulletinMessages,
		Runtime,
		WithRococoBulletinMessagesInstance,
		RelayersForPermissionlessLanesInstance,
		PriorityBoostPerMessage,
	>,
>;
bp_runtime::generate_static_str_provider!(OnBridgeHubRococoRefundRococoBulletinMessages);

/// Add XCM messages support for BridgeHubRococo to support Rococo->Rococo Bulletin XCM messages.
pub type WithRococoBulletinMessagesInstance = pallet_bridge_messages::Instance4;
impl pallet_bridge_messages::Config<WithRococoBulletinMessagesInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo =
		weights::pallet_bridge_messages_rococo_to_rococo_bulletin::WeightInfo<Runtime>;

	type ThisChain = bp_bridge_hub_rococo::BridgeHubRococo;
	type BridgedChain = bp_polkadot_bulletin::PolkadotBulletin;
	type BridgedHeaderChain = BridgeRococoBulletinGrandpa;

	type OutboundPayload = XcmAsPlainPayload;
	type InboundPayload = XcmAsPlainPayload;
	type LaneId = LegacyLaneId;

	type DeliveryPayments = ();
	type DeliveryConfirmationPayments = ();

	type MessageDispatch = XcmOverRococoBulletin;
	type OnMessagesDelivered = XcmOverRococoBulletin;
}

/// Add support for the export and dispatch of XCM programs.
pub type XcmOverPolkadotBulletinInstance = pallet_xcm_bridge::Instance2;
impl pallet_xcm_bridge::Config<XcmOverPolkadotBulletinInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_xcm_bridge_over_bulletin::WeightInfo<Runtime>;

	type UniversalLocation = UniversalLocation;
	type BridgedNetwork = RococoBulletinGlobalConsensusNetworkLocation;
	type BridgeMessagesPalletInstance = WithRococoBulletinMessagesInstance;

	type MessageExportPrice = ();
	type DestinationVersion = AlwaysV5;

	type ForceOrigin = EnsureRoot<AccountId>;
	// We don't want to allow creating bridges for this instance.
	type OpenBridgeOrigin = EnsureNever<Location>;
	// Converter aligned with `OpenBridgeOrigin`.
	type BridgeOriginAccountIdConverter =
		(ParentIsPreset<AccountId>, SiblingParachainConvertsVia<Sibling, AccountId>);

	type BridgeDeposit = BridgeDeposit;
	type Currency = Balances;
	type RuntimeHoldReason = RuntimeHoldReason;
	// Do not require deposit from People parachains.
	type AllowWithoutBridgeDeposit = Equals<PeopleRococoLocation>;

	type LocalXcmChannelManager = ();
	// Dispatching inbound messages from the bridge.
	type BlobDispatcher = BlobDispatcherWithChannelStatus<
		// Dispatches received XCM messages from other bridge
		BridgeBlobDispatcher<
			XcmRouter,
			UniversalLocation,
			BridgeRococoToRococoBulletinMessagesPalletInstance,
		>,
		// no congestion checking
		(),
	>;
	type CongestionLimits = ();
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::bridge_common_config::BridgeGrandpaRococoBulletinInstance;
	use bridge_runtime_common::{
		assert_complete_bridge_types, integrity::check_message_lane_weights,
	};
	use parachains_common::Balance;
	use testnet_parachains_constants::rococo;

	/// Every additional message in the message delivery transaction boosts its priority.
	/// So the priority of transaction with `N+1` messages is larger than priority of
	/// transaction with `N` messages by the `PriorityBoostPerMessage`.
	///
	/// Economically, it is an equivalent of adding tip to the transaction with `N` messages.
	/// The `FEE_BOOST_PER_MESSAGE` constant is the value of this tip.
	///
	/// We want this tip to be large enough (delivery transactions with more messages = less
	/// operational costs and a faster bridge), so this value should be significant.
	const FEE_BOOST_PER_MESSAGE: Balance = 2 * rococo::currency::UNITS;

	// see `FEE_BOOST_PER_MESSAGE` comment
	const FEE_BOOST_PER_RELAY_HEADER: Balance = 2 * rococo::currency::UNITS;

	#[test]
	fn ensure_bridge_hub_rococo_message_lane_weights_are_correct() {
		check_message_lane_weights::<
			bp_bridge_hub_rococo::BridgeHubRococo,
			Runtime,
			WithRococoBulletinMessagesInstance,
		>(
			bp_polkadot_bulletin::EXTRA_STORAGE_PROOF_SIZE,
			bp_bridge_hub_rococo::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
			bp_bridge_hub_rococo::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
			true,
		);
	}

	#[test]
	fn ensure_bridge_integrity() {
		assert_complete_bridge_types!(
			runtime: Runtime,
			with_bridged_chain_messages_instance: WithRococoBulletinMessagesInstance,
			this_chain: bp_bridge_hub_rococo::BridgeHubRococo,
			bridged_chain: bp_polkadot_bulletin::PolkadotBulletin,
			expected_payload_type: XcmAsPlainPayload,
		);

		// we can't use `assert_complete_bridge_constants` here, because there's a trick with
		// Bulletin chain - it has the same (almost) runtime for Polkadot Bulletin and Rococo
		// Bulletin, so we have to adhere Polkadot names here

		pallet_bridge_relayers::extension::per_relay_header::ensure_priority_boost_is_sane::<
			Runtime,
			BridgeGrandpaRococoBulletinInstance,
			PriorityBoostPerRelayHeader,
		>(FEE_BOOST_PER_RELAY_HEADER);

		pallet_bridge_relayers::extension::per_message::ensure_priority_boost_is_sane::<
			Runtime,
			WithRococoBulletinMessagesInstance,
			PriorityBoostPerMessage,
		>(FEE_BOOST_PER_MESSAGE);

		let expected: InteriorLocation = PalletInstance(
			bp_bridge_hub_rococo::WITH_BRIDGE_ROCOCO_TO_BULLETIN_MESSAGES_PALLET_INDEX,
		)
		.into();

		assert_eq!(BridgeRococoToRococoBulletinMessagesPalletInstance::get(), expected,);
	}
}

/// Contains the migration for the PeopleRococo<>RococoBulletin bridge.
pub mod migration {
	use super::*;
	use frame_support::traits::ConstBool;

	parameter_types! {
		pub BulletinRococoLocation: InteriorLocation = [GlobalConsensus(RococoBulletinGlobalConsensusNetwork::get())].into();
		pub RococoPeopleToRococoBulletinMessagesLane: bp_messages::HashedLaneId = pallet_xcm_bridge::Pallet::< Runtime, XcmOverPolkadotBulletinInstance >::bridge_locations(
				PeopleRococoLocation::get(),
				BulletinRococoLocation::get()
			)
			.unwrap()
			.calculate_lane_id(xcm::latest::VERSION).expect("Valid locations");
	}

	/// Ensure that the existing lanes for the People<>Bulletin bridge are correctly configured.
	pub type StaticToDynamicLanes = pallet_xcm_bridge::migration::OpenBridgeForLane<
		Runtime,
		XcmOverPolkadotBulletinInstance,
		RococoPeopleToRococoBulletinMessagesLane,
		ConstBool<true>,
		PeopleRococoLocation,
		BulletinRococoLocation,
		(),
	>;
}
