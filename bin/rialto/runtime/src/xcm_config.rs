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

//! XCM configurations for the Rialto runtime.

use super::{
	millau_messages::ToMillauBlobExporter, AccountId, AllPalletsWithSystem, Balances, Runtime,
	RuntimeCall, RuntimeEvent, RuntimeOrigin, XcmPallet,
};
use bp_rialto::WeightToFee;
use bridge_runtime_common::CustomNetworkId;
use frame_support::{
	parameter_types,
	traits::{ConstU32, Everything, Nothing},
	weights::Weight,
};
use frame_system::EnsureRoot;
use xcm::latest::prelude::*;
use xcm_builder::{
	AccountId32Aliases, CurrencyAdapter as XcmCurrencyAdapter, IsConcrete, MintLocation,
	SignedAccountId32AsNative, SignedToAccountId32, SovereignSignedViaLocation, TakeWeightCredit,
	UsingComponents,
};

parameter_types! {
	/// The location of the `MLAU` token, from the context of this chain. Since this token is native to this
	/// chain, we make it synonymous with it and thus it is the `Here` location, which means "equivalent to
	/// the context".
	pub const TokenLocation: MultiLocation = Here.into_location();
	/// The Rialto network ID.
	pub const ThisNetwork: NetworkId = CustomNetworkId::Rialto.as_network_id();
	/// The Millau network ID.
	pub const MillauNetwork: NetworkId = CustomNetworkId::Millau.as_network_id();

	/// Our XCM location ancestry - i.e. our location within the Consensus Universe.
	///
	/// Since Polkadot is a top-level relay-chain with its own consensus, it's just our network ID.
	pub UniversalLocation: InteriorMultiLocation = ThisNetwork::get().into();
	/// The check account, which holds any native assets that have been teleported out and not back in (yet).
	pub CheckAccount: (AccountId, MintLocation) = (XcmPallet::check_account(), MintLocation::Local);
}

/// The canonical means of converting a `MultiLocation` into an `AccountId`, used when we want to
/// determine the sovereign account controlled by a location.
pub type SovereignAccountOf = (
	// We can directly alias an `AccountId32` into a local account.
	AccountId32Aliases<ThisNetwork, AccountId>,
);

/// Our asset transactor. This is what allows us to interest with the runtime facilities from the
/// point of view of XCM-only concepts like `MultiLocation` and `MultiAsset`.
///
/// Ours is only aware of the Balances pallet, which is mapped to `TokenLocation`.
pub type LocalAssetTransactor = XcmCurrencyAdapter<
	// Use this currency:
	Balances,
	// Use this currency when it is a fungible asset matching the given location or name:
	IsConcrete<TokenLocation>,
	// We can convert the MultiLocations with our converter above:
	SovereignAccountOf,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId,
	// We track our teleports in/out to keep total issuance correct.
	CheckAccount,
>;

/// The means that we convert the XCM message origin location into a local dispatch origin.
type LocalOriginConverter = (
	// A `Signed` origin of the sovereign account that the original location controls.
	SovereignSignedViaLocation<SovereignAccountOf, RuntimeOrigin>,
	// The AccountId32 location type can be expressed natively as a `Signed` origin.
	SignedAccountId32AsNative<ThisNetwork, RuntimeOrigin>,
);

parameter_types! {
	/// The amount of weight an XCM operation takes. This is a safe overestimate.
	pub const BaseXcmWeight: Weight = Weight::from_parts(1_000_000_000, 64 * 1024);
	/// Maximum number of instructions in a single XCM fragment. A sanity check against weight
	/// calculations getting too crazy.
	pub const MaxInstructions: u32 = 100;
}

/// The XCM router. We are not sending messages to sibling/parent/child chains here.
pub type XcmRouter = ();

/// The barriers one of which must be passed for an XCM message to be executed.
pub type Barrier = (
	// Weight that is paid for may be consumed.
	TakeWeightCredit,
);

/// Dispatches received XCM messages from other chain.
pub type OnRialtoBlobDispatcher = xcm_builder::BridgeBlobDispatcher<
	crate::xcm_config::XcmRouter,
	crate::xcm_config::UniversalLocation,
>;

/// Incoming XCM weigher type.
pub type XcmWeigher = xcm_builder::FixedWeightBounds<BaseXcmWeight, RuntimeCall, MaxInstructions>;

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = ();
	type AssetTransactor = LocalAssetTransactor;
	type OriginConverter = LocalOriginConverter;
	type IsReserve = ();
	type IsTeleporter = ();
	type UniversalLocation = UniversalLocation;
	type Barrier = Barrier;
	type Weigher = XcmWeigher;
	// The weight trader piggybacks on the existing transaction-fee conversion logic.
	type Trader = UsingComponents<WeightToFee, TokenLocation, AccountId, Balances, ()>;
	type ResponseHandler = XcmPallet;
	type AssetTrap = XcmPallet;
	type AssetLocker = ();
	type AssetExchanger = ();
	type AssetClaims = XcmPallet;
	type SubscriptionService = XcmPallet;
	type PalletInstancesInfo = AllPalletsWithSystem;
	type MaxAssetsIntoHolding = ConstU32<64>;
	type FeeManager = ();
	type MessageExporter = ToMillauBlobExporter;
	type UniversalAliases = Nothing;
	type CallDispatcher = RuntimeCall;
	type SafeCallFilter = Everything;
}

/// Type to convert an `Origin` type value into a `MultiLocation` value which represents an interior
/// location of this chain.
pub type LocalOriginToLocation = (
	// Usual Signed origin to be used in XCM as a corresponding AccountId32
	SignedToAccountId32<RuntimeOrigin, AccountId, ThisNetwork>,
);

#[cfg(feature = "runtime-benchmarks")]
parameter_types! {
	pub ReachableDest: Option<MultiLocation> = None;
}

impl pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	// We don't allow any messages to be sent via the transaction yet. This is basically safe to
	// enable, (safe the possibility of someone spamming the parachain if they're willing to pay
	// the DOT to send from the Relay-chain). But it's useless until we bring in XCM v3 which will
	// make `DescendOrigin` a bit more useful.
	type SendXcmOrigin = xcm_builder::EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	type XcmRouter = XcmRouter;
	// Anyone can execute XCM messages locally.
	type ExecuteXcmOrigin = xcm_builder::EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	type XcmExecuteFilter = Everything;
	type XcmExecutor = xcm_executor::XcmExecutor<XcmConfig>;
	// Anyone is able to use teleportation regardless of who they are and what they want to
	// teleport.
	type XcmTeleportFilter = Everything;
	// Anyone is able to use reserve transfers regardless of who they are and what they want to
	// transfer.
	type XcmReserveTransferFilter = Everything;
	type Weigher = XcmWeigher;
	type UniversalLocation = UniversalLocation;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 100;
	type AdvertisedXcmVersion = pallet_xcm::CurrentXcmVersion;
	type Currency = Balances;
	type CurrencyMatcher = ();
	type TrustedLockers = ();
	type SovereignAccountOf = SovereignAccountOf;
	type MaxLockers = frame_support::traits::ConstU32<8>;
	type WeightInfo = pallet_xcm::TestWeightInfo;
	#[cfg(feature = "runtime-benchmarks")]
	type ReachableDest = ReachableDest;
	type AdminOrigin = EnsureRoot<AccountId>;
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		millau_messages::{FromMillauMessageDispatch, XCM_LANE},
		WithMillauMessagesInstance,
	};
	use bp_messages::{
		target_chain::{DispatchMessage, DispatchMessageData, MessageDispatch},
		LaneId, MessageKey,
	};
	use bridge_runtime_common::messages_xcm_extension::XcmBlobMessageDispatchResult;
	use codec::Encode;
	use pallet_bridge_messages::OutboundLanes;
	use xcm_executor::XcmExecutor;

	fn new_test_ext() -> sp_io::TestExternalities {
		sp_io::TestExternalities::new(
			frame_system::GenesisConfig::default().build_storage::<Runtime>().unwrap(),
		)
	}

	fn prepare_outbound_xcm_message(destination: NetworkId) -> Xcm<RuntimeCall> {
		vec![ExportMessage {
			network: destination,
			destination: destination.into(),
			xcm: vec![Instruction::Trap(42)].into(),
		}]
		.into()
	}

	#[test]
	fn xcm_messages_to_millau_are_sent_using_bridge_exporter() {
		new_test_ext().execute_with(|| {
			// ensure that the there are no messages queued
			assert_eq!(
				OutboundLanes::<Runtime, WithMillauMessagesInstance>::get(XCM_LANE)
					.latest_generated_nonce,
				0,
			);

			// export message instruction "sends" message to Rialto
			XcmExecutor::<XcmConfig>::execute_xcm_in_credit(
				Here,
				prepare_outbound_xcm_message(MillauNetwork::get()),
				Default::default(),
				Weight::MAX,
				Weight::MAX,
			)
			.ensure_complete()
			.expect("runtime configuration must be correct");

			// ensure that the message has been queued
			assert_eq!(
				OutboundLanes::<Runtime, WithMillauMessagesInstance>::get(XCM_LANE)
					.latest_generated_nonce,
				1,
			);
		})
	}

	fn prepare_inbound_bridge_message() -> DispatchMessage<Vec<u8>> {
		let xcm = xcm::VersionedXcm::<RuntimeCall>::V3(vec![Instruction::Trap(42)].into());
		let location =
			xcm::VersionedInteriorMultiLocation::V3(X1(GlobalConsensus(ThisNetwork::get())));
		// this is the `BridgeMessage` from polkadot xcm builder, but it has no constructor
		// or public fields, so just tuple
		let bridge_message = (location, xcm).encode();
		DispatchMessage {
			key: MessageKey { lane_id: LaneId([0, 0, 0, 0]), nonce: 1 },
			data: DispatchMessageData { payload: Ok(bridge_message) },
		}
	}

	#[test]
	fn xcm_messages_from_millau_are_dispatched() {
		let incoming_message = prepare_inbound_bridge_message();

		// we care only about handing message to the XCM dispatcher, so we don't care about its
		// actual dispatch
		let dispatch_result = FromMillauMessageDispatch::dispatch(incoming_message);
		assert!(matches!(
			dispatch_result.dispatch_level_result,
			XcmBlobMessageDispatchResult::NotDispatched(_),
		));
	}
}
