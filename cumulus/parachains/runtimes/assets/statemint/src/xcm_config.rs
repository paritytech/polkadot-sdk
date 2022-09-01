// Copyright (C) 2022 Parity Technologies (UK) Ltd.
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

use super::{
	AccountId, AssetId, Assets, Authorship, Balance, Balances, Call, Event, Origin, ParachainInfo,
	ParachainSystem, PolkadotXcm, Runtime, WeightToFee, XcmpQueue,
};
use frame_support::{
	match_types, parameter_types,
	traits::{Everything, Nothing, PalletInfoAccess},
};
use pallet_xcm::XcmPassthrough;
use parachains_common::{
	impls::ToStakingPot,
	xcm_config::{DenyReserveTransferToRelayChain, DenyThenTry},
};
use polkadot_parachain::primitives::Sibling;
use xcm::latest::prelude::*;
use xcm_builder::{
	AccountId32Aliases, AllowKnownQueryResponses, AllowSubscriptionsFrom,
	AllowTopLevelPaidExecutionFrom, AllowUnpaidExecutionFrom, AsPrefixedGeneralIndex,
	ConvertedConcreteAssetId, CurrencyAdapter, EnsureXcmOrigin, FungiblesAdapter, IsConcrete,
	LocationInverter, NativeAsset, ParentAsSuperuser, ParentIsPreset, RelayChainAsNative,
	SiblingParachainAsNative, SiblingParachainConvertsVia, SignedAccountId32AsNative,
	SignedToAccountId32, SovereignSignedViaLocation, TakeWeightCredit, UsingComponents,
	WeightInfoBounds,
};
use xcm_executor::{traits::JustTry, XcmExecutor};

parameter_types! {
	pub const DotLocation: MultiLocation = MultiLocation::parent();
	pub const RelayNetwork: NetworkId = NetworkId::Polkadot;
	pub RelayChainOrigin: Origin = cumulus_pallet_xcm::Origin::Relay.into();
	pub Ancestry: MultiLocation = Parachain(ParachainInfo::parachain_id().into()).into();
	pub const Local: MultiLocation = Here.into();
	pub AssetsPalletLocation: MultiLocation =
		PalletInstance(<Assets as PalletInfoAccess>::index() as u8).into();
	pub CheckingAccount: AccountId = PolkadotXcm::check_account();
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
	IsConcrete<DotLocation>,
	// Convert an XCM MultiLocation into a local account id:
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId,
	// We don't track any teleports of `Balances`.
	(),
>;

/// Means for transacting assets besides the native currency on this chain.
pub type FungiblesTransactor = FungiblesAdapter<
	// Use this fungibles implementation:
	Assets,
	// Use this currency when it is a fungible asset matching the given location or name:
	ConvertedConcreteAssetId<
		AssetId,
		Balance,
		AsPrefixedGeneralIndex<AssetsPalletLocation, AssetId, JustTry>,
		JustTry,
	>,
	// Convert an XCM MultiLocation into a local account id:
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId,
	// We only want to allow teleports of known assets. We use non-zero issuance as an indication
	// that this asset is known.
	parachains_common::impls::NonZeroIssuance<AccountId, Assets>,
	// The account to use for tracking teleports.
	CheckingAccount,
>;
/// Means for transacting assets on this chain.
pub type AssetTransactors = (CurrencyTransactor, FungiblesTransactor);

/// This is the type we use to convert an (incoming) XCM origin into a local `Origin` instance,
/// ready for dispatching a transaction with Xcm's `Transact`. There is an `OriginKind` which can
/// biases the kind of local `Origin` it will become.
pub type XcmOriginToTransactDispatchOrigin = (
	// Sovereign account converter; this attempts to derive an `AccountId` from the origin location
	// using `LocationToAccountId` and then turn that into the usual `Signed` origin. Useful for
	// foreign chains who want to have a local sovereign account on this chain which they control.
	SovereignSignedViaLocation<LocationToAccountId, Origin>,
	// Native converter for Relay-chain (Parent) location; will convert to a `Relay` origin when
	// recognised.
	RelayChainAsNative<RelayChainOrigin, Origin>,
	// Native converter for sibling Parachains; will convert to a `SiblingPara` origin when
	// recognised.
	SiblingParachainAsNative<cumulus_pallet_xcm::Origin, Origin>,
	// Superuser converter for the Relay-chain (Parent) location. This will allow it to issue a
	// transaction from the Root origin.
	ParentAsSuperuser<Origin>,
	// Native signed account converter; this just converts an `AccountId32` origin into a normal
	// `Origin::Signed` origin of the same 32-byte value.
	SignedAccountId32AsNative<RelayNetwork, Origin>,
	// Xcm origins can be represented natively under the Xcm pallet's Xcm origin.
	XcmPassthrough<Origin>,
);

parameter_types! {
	pub const MaxInstructions: u32 = 100;
	pub XcmAssetFeesReceiver: Option<AccountId> = Authorship::author();
}

match_types! {
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
		TakeWeightCredit,
		AllowTopLevelPaidExecutionFrom<Everything>,
		// Parent and its exec plurality get free execution
		AllowUnpaidExecutionFrom<ParentOrParentsExecutivePlurality>,
		// Expected responses are OK.
		AllowKnownQueryResponses<PolkadotXcm>,
		// Subscriptions for version tracking are OK.
		AllowSubscriptionsFrom<ParentOrSiblings>,
	),
>;

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type Call = Call;
	type XcmSender = XcmRouter;
	type AssetTransactor = AssetTransactors;
	type OriginConverter = XcmOriginToTransactDispatchOrigin;
	// Statemint does not recognize a reserve location for any asset. This does not prevent
	// Statemint acting _as_ a reserve location for DOT and assets created under `pallet-assets`.
	// For DOT, users must use teleport where allowed (e.g. with the Relay Chain).
	type IsReserve = ();
	type IsTeleporter = NativeAsset; // <- should be enough to allow teleportation of DOT
	type LocationInverter = LocationInverter<Ancestry>;
	type Barrier = Barrier;
	type Weigher =
		WeightInfoBounds<crate::weights::xcm::StatemintXcmWeight<Call>, Call, MaxInstructions>;
	type Trader =
		UsingComponents<WeightToFee, DotLocation, AccountId, Balances, ToStakingPot<Runtime>>;
	type ResponseHandler = PolkadotXcm;
	type AssetTrap = PolkadotXcm;
	type AssetClaims = PolkadotXcm;
	type SubscriptionService = PolkadotXcm;
}

/// Converts a local signed origin into an XCM multilocation.
/// Forms the basis for local origins sending/executing XCMs.
pub type LocalOriginToLocation = SignedToAccountId32<Origin, AccountId, RelayNetwork>;

/// The means for routing XCM messages which are not for local execution into the right message
/// queues.
pub type XcmRouter = (
	// Two routers - use UMP to communicate with the relay chain:
	cumulus_primitives_utility::ParentAsUmp<ParachainSystem, PolkadotXcm>,
	// ..and XCMP to communicate with the sibling chains.
	XcmpQueue,
);

impl pallet_xcm::Config for Runtime {
	type Event = Event;
	// We want to disallow users sending (arbitrary) XCMs from this chain.
	type SendXcmOrigin = EnsureXcmOrigin<Origin, ()>;
	type XcmRouter = XcmRouter;
	// We support local origins dispatching XCM executions in principle...
	type ExecuteXcmOrigin = EnsureXcmOrigin<Origin, LocalOriginToLocation>;
	// ... but disallow generic XCM execution. As a result only teleports and reserve transfers are allowed.
	type XcmExecuteFilter = Nothing;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type XcmTeleportFilter = Everything;
	type XcmReserveTransferFilter = Everything;
	type Weigher =
		WeightInfoBounds<crate::weights::xcm::StatemintXcmWeight<Call>, Call, MaxInstructions>;
	type LocationInverter = LocationInverter<Ancestry>;
	type Origin = Origin;
	type Call = Call;
	const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 100;
	type AdvertisedXcmVersion = pallet_xcm::CurrentXcmVersion;
}

impl cumulus_pallet_xcm::Config for Runtime {
	type Event = Event;
	type XcmExecutor = XcmExecutor<XcmConfig>;
}
