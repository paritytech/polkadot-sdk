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

//! Bridge definitions used on BridgeHubRococo for bridging to Rococo Bulletin.
//!
//! Rococo Bulletin chain will be the 1:1 copy of the Polkadot Bulletin, so we
//! are reusing Polkadot Bulletin chain primitives everywhere here.

use crate::{
	bridge_common_config::RelayersForPermissionlessLanesInstance, weights,
	xcm_config::UniversalLocation, AccountId, Balance, Balances, BridgeRococoBulletinGrandpa,
	BridgeRococoBulletinMessages, PolkadotXcm, Runtime, RuntimeEvent, RuntimeHoldReason,
	XcmOverRococoBulletin, XcmRouter,
};
use bp_messages::{
	source_chain::FromBridgedChainMessagesDeliveryProof,
	target_chain::FromBridgedChainMessagesProof, HashedLaneId,
};
use bridge_hub_common::xcm_version::XcmVersionOfDestAndRemoteBridge;

use frame_support::{
	parameter_types,
	traits::{Equals, PalletInfoAccess},
};
use frame_system::{EnsureNever, EnsureRoot};
use pallet_bridge_messages::LaneIdOf;
use pallet_bridge_relayers::extension::{
	BridgeRelayersTransactionExtension, WithMessagesExtensionConfig,
};
use pallet_xcm_bridge_hub::XcmAsPlainPayload;
use polkadot_parachain_primitives::primitives::Sibling;
use testnet_parachains_constants::rococo::currency::UNITS as ROC;
use xcm::{
	latest::prelude::*,
	prelude::{InteriorLocation, NetworkId},
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
	pub PriorityBoostPerMessage: u64 = 182_044_444_444_444;

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

/// Dispatches received XCM messages from other bridge.
type FromRococoBulletinMessageBlobDispatcher = BridgeBlobDispatcher<
	XcmRouter,
	UniversalLocation,
	BridgeRococoToRococoBulletinMessagesPalletInstance,
>;

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
	LaneIdOf<Runtime, WithRococoBulletinMessagesInstance>,
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
	type LaneId = HashedLaneId;

	type DeliveryPayments = ();
	type DeliveryConfirmationPayments = ();

	type MessageDispatch = XcmOverRococoBulletin;
	type OnMessagesDelivered = XcmOverRococoBulletin;
}

/// Add support for the export and dispatch of XCM programs.
pub type XcmOverPolkadotBulletinInstance = pallet_xcm_bridge_hub::Instance2;
impl pallet_xcm_bridge_hub::Config<XcmOverPolkadotBulletinInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;

	type UniversalLocation = UniversalLocation;
	type BridgedNetwork = RococoBulletinGlobalConsensusNetworkLocation;
	type BridgeMessagesPalletInstance = WithRococoBulletinMessagesInstance;

	type MessageExportPrice = ();
	type DestinationVersion =
		XcmVersionOfDestAndRemoteBridge<PolkadotXcm, RococoBulletinGlobalConsensusNetworkLocation>;

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
	type BlobDispatcher = FromRococoBulletinMessageBlobDispatcher;
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
			with_bridged_chain_grandpa_instance: BridgeGrandpaRococoBulletinInstance,
			with_bridged_chain_messages_instance: WithRococoBulletinMessagesInstance,
			this_chain: bp_bridge_hub_rococo::BridgeHubRococo,
			bridged_chain: bp_polkadot_bulletin::PolkadotBulletin,
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
	let universal_source = [GlobalConsensus(Rococo), Parachain(sibling_para_id)].into();
	let universal_destination =
		[GlobalConsensus(RococoBulletinGlobalConsensusNetwork::get()), Parachain(2075)].into();
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

/// Contains the migration for the PeopleRococo<>RococoBulletin bridge.
pub mod migration {
	use super::*;
	use frame_support::traits::ConstBool;

	parameter_types! {
		pub BulletinRococoLocation: InteriorLocation = [GlobalConsensus(RococoBulletinGlobalConsensusNetwork::get())].into();
		pub RococoPeopleToRococoBulletinMessagesLane: HashedLaneId = pallet_xcm_bridge_hub::Pallet::< Runtime, XcmOverPolkadotBulletinInstance >::bridge_locations(
				PeopleRococoLocation::get(),
				BulletinRococoLocation::get()
			)
			.unwrap()
			.calculate_lane_id(xcm::latest::VERSION).expect("Valid locations");
	}

	/// Ensure that the existing lanes for the People<>Bulletin bridge are correctly configured.
	pub type StaticToDynamicLanes = pallet_xcm_bridge_hub::migration::OpenBridgeForLane<
		Runtime,
		XcmOverPolkadotBulletinInstance,
		RococoPeopleToRococoBulletinMessagesLane,
		ConstBool<true>,
		PeopleRococoLocation,
		BulletinRococoLocation,
	>;
}
