// Copyright 2022 Parity Technologies (UK) Ltd.
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

use super::{
	AccountId, Balances, ParachainInfo, ParachainSystem, PolkadotXcm, Runtime, RuntimeCall,
	RuntimeEvent, RuntimeOrigin, WeightToFee, XcmpQueue,
};
use frame_support::{
	match_types, parameter_types,
	traits::{Everything, Nothing},
};
use pallet_xcm::XcmPassthrough;
use parachains_common::xcm_config::{
	ConcreteNativeAssetFrom, DenyReserveTransferToRelayChain, DenyThenTry,
};
use polkadot_parachain::primitives::Sibling;
use polkadot_runtime_common::impls::ToAuthor;
use xcm::latest::prelude::*;
use xcm_builder::{
	AccountId32Aliases, AllowKnownQueryResponses, AllowSubscriptionsFrom,
	AllowTopLevelPaidExecutionFrom, AllowUnpaidExecutionFrom, CurrencyAdapter, EnsureXcmOrigin,
	FixedWeightBounds, IsConcrete, LocationInverter, ParentIsPreset, RelayChainAsNative,
	SiblingParachainAsNative, SiblingParachainConvertsVia, SignedAccountId32AsNative,
	SignedToAccountId32, SovereignSignedViaLocation, TakeWeightCredit, UsingComponents,
};
use xcm_executor::XcmExecutor;

parameter_types! {
	pub const KsmRelayLocation: MultiLocation = MultiLocation::parent();
	pub const RelayNetwork: NetworkId = NetworkId::Kusama;
	pub RelayChainOrigin: RuntimeOrigin = cumulus_pallet_xcm::Origin::Relay.into();
	pub Ancestry: MultiLocation = Parachain(ParachainInfo::parachain_id().into()).into();
}

/// Type for specifying how a `MultiLocation` can be converted into an `AccountId`. This is used
/// when determining ownership of accounts for asset transacting and when attempting to use XCM
/// `Transact` in order to determine the dispatch Origin.
pub type LocationToAccountId = (
	// The parent (Relay-chain) origin converts to the parent `AccountId`.
	ParentIsPreset<AccountId>,
	// Sibling parachain origins convert to AccountId via the `ParaId::into`.
	SiblingParachainConvertsVia<Sibling, AccountId>,
	// Straight up local `AccountId32` origins just alias directly to `AccountId`.
	AccountId32Aliases<RelayNetwork, AccountId>,
);

/// Means for transacting the native currency on this chain.
pub type CurrencyTransactor = CurrencyAdapter<
	// Use this currency:
	Balances,
	// Use this currency when it is a fungible asset matching the given location or name:
	IsConcrete<KsmRelayLocation>,
	// Do a simple punn to convert an AccountId32 MultiLocation into a native chain account ID:
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId,
	// We don't track any teleports.
	(),
>;

/// This is the type we use to convert an (incoming) XCM origin into a local `Origin` instance,
/// ready for dispatching a transaction with Xcm's `Transact`. There is an `OriginKind` which can
/// biases the kind of local `Origin` it will become.
pub type XcmOriginToTransactDispatchOrigin = (
	// Sovereign account converter; this attempts to derive an `AccountId` from the origin location
	// using `LocationToAccountId` and then turn that into the usual `Signed` origin. Useful for
	// foreign chains who want to have a local sovereign account on this chain which they control.
	SovereignSignedViaLocation<LocationToAccountId, RuntimeOrigin>,
	// Native converter for Relay-chain (Parent) location; will converts to a `Relay` origin when
	// recognized.
	RelayChainAsNative<RelayChainOrigin, RuntimeOrigin>,
	// Native converter for sibling Parachains; will convert to a `SiblingPara` origin when
	// recognized.
	SiblingParachainAsNative<cumulus_pallet_xcm::Origin, RuntimeOrigin>,
	// Native signed account converter; this just converts an `AccountId32` origin into a normal
	// `Origin::Signed` origin of the same 32-byte value.
	SignedAccountId32AsNative<RelayNetwork, RuntimeOrigin>,
	// Xcm origins can be represented natively under the Xcm pallet's Xcm origin.
	XcmPassthrough<RuntimeOrigin>,
);

parameter_types! {
	// TODO: change to meassured weights - https://github.com/paritytech/parity-bridges-common/issues/391
	// One XCM operation is 1_000_000_000 weight - almost certainly a conservative estimate.
	pub UnitWeightCost: u64 = 1_000_000_000;
	pub const MaxInstructions: u32 = 100;
}

match_types! {
	// TODO: map gov2 origins here - after merge https://github.com/paritytech/cumulus/pull/1895
	pub type ParentOrParentsExecutivePlurality: impl Contains<MultiLocation> = {
		MultiLocation { parents: 1, interior: Here } |
		MultiLocation { parents: 1, interior: X1(Plurality { id: BodyId::Executive, .. }) }
	};
	pub type ParentOrSiblings: impl Contains<MultiLocation> = {
		MultiLocation { parents: 1, interior: Here } |
		MultiLocation { parents: 1, interior: X1(_) }
	};
}

pub type Barrier = DenyThenTry<
	DenyReserveTransferToRelayChain,
	(
		// Allow local users to buy weight credit.
		TakeWeightCredit,
		// Parent and its exec plurality get free execution.
		AllowUnpaidExecutionFrom<ParentOrParentsExecutivePlurality>,
		// Expected responses are OK.
		AllowKnownQueryResponses<PolkadotXcm>,
		// Subscriptions for version tracking are OK.
		AllowSubscriptionsFrom<ParentOrSiblings>,
		// Allow anything to pay for execution.
		AllowTopLevelPaidExecutionFrom<Everything>,
	),
>;

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = XcmRouter;
	type AssetTransactor = CurrencyTransactor;
	type OriginConverter = XcmOriginToTransactDispatchOrigin;
	// BridgeHub does not recognize a reserve location for any asset. Users must teleport KSM
	// where allowed (e.g. with the Relay Chain).
	type IsReserve = ();
	/// Only allow teleportation of KSM.
	type IsTeleporter = ConcreteNativeAssetFrom<KsmRelayLocation>;
	type LocationInverter = LocationInverter<Ancestry>;
	type Barrier = Barrier;
	// TODO: change to meassured weights - https://github.com/paritytech/parity-bridges-common/issues/391
	type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
	type Trader =
		UsingComponents<WeightToFee, KsmRelayLocation, AccountId, Balances, ToAuthor<Runtime>>;
	type ResponseHandler = PolkadotXcm;
	type AssetTrap = PolkadotXcm;
	type AssetClaims = PolkadotXcm;
	type SubscriptionService = PolkadotXcm;
}

/// Converts a local signed origin into an XCM multilocation.
/// Forms the basis for local origins sending/executing XCMs.
pub type LocalOriginToLocation = SignedToAccountId32<RuntimeOrigin, AccountId, RelayNetwork>;

/// The means for routing XCM messages which are not for local execution into the right message
/// queues.
pub type XcmRouter = (
	// Two routers - use UMP to communicate with the relay chain:
	cumulus_primitives_utility::ParentAsUmp<ParachainSystem, PolkadotXcm>,
	// ..and XCMP to communicate with the sibling chains.
	XcmpQueue,
);

impl pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	// We want to disallow users sending (arbitrary) XCMs from this chain.
	type SendXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, ()>;
	type XcmRouter = XcmRouter;
	// We support local origins dispatching XCM executions in principle...
	type ExecuteXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	// ... but disallow generic XCM execution. As a result only teleports are allowed.
	type XcmExecuteFilter = Nothing;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type XcmTeleportFilter = Everything;
	type XcmReserveTransferFilter = Nothing; // This parachain is not meant as a reserve location.

	// TODO: change to meassured weights - https://github.com/paritytech/parity-bridges-common/issues/391
	type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
	type LocationInverter = LocationInverter<Ancestry>;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 100;
	type AdvertisedXcmVersion = pallet_xcm::CurrentXcmVersion;
}

impl cumulus_pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmExecutor = XcmExecutor<XcmConfig>;
}
