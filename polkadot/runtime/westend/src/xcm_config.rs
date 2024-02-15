// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! XCM configurations for Westend.

use super::{
	parachains_origin, AccountId, AllPalletsWithSystem, Balances, Dmp, FellowshipAdmin,
	GeneralAdmin, ParaId, Runtime, RuntimeCall, RuntimeEvent, RuntimeOrigin, StakingAdmin,
	TransactionByteFee, Treasury, WeightToFee, XcmPallet,
};
use crate::governance::pallet_custom_origins::Treasurer;
use frame_support::{
	parameter_types,
	traits::{Contains, Equals, Everything, Nothing},
};
use frame_system::EnsureRoot;
use pallet_xcm::XcmPassthrough;
use runtime_common::{
	xcm_sender::{ChildParachainRouter, ExponentialPrice},
	ToAuthor,
};
use sp_core::ConstU32;
use westend_runtime_constants::{
	currency::CENTS,
	system_parachain::*,
	xcm::body::{FELLOWSHIP_ADMIN_INDEX, TREASURER_INDEX},
};
use xcm::latest::prelude::*;
use xcm_builder::{
	AccountId32Aliases, AllowExplicitUnpaidExecutionFrom, AllowKnownQueryResponses,
	AllowSubscriptionsFrom, AllowTopLevelPaidExecutionFrom, ChildParachainAsNative,
	ChildParachainConvertsVia, DescribeBodyTerminal, DescribeFamily, FrameTransactionalProcessor,
	FungibleAdapter, HashedDescription, IsConcrete, MintLocation, OriginToPluralityVoice,
	SignedAccountId32AsNative, SignedToAccountId32, SovereignSignedViaLocation, TakeWeightCredit,
	TrailingSetTopicAsId, UsingComponents, WeightInfoBounds, WithComputedOrigin, WithUniqueTopic,
	XcmFeeManagerFromComponents, XcmFeeToAccount,
};
use xcm_executor::XcmExecutor;

parameter_types! {
	pub const TokenLocation: Location = Here.into_location();
	pub const RootLocation: Location = Location::here();
	pub const ThisNetwork: NetworkId = Westend;
	pub UniversalLocation: InteriorLocation = [GlobalConsensus(ThisNetwork::get())].into();
	pub CheckAccount: AccountId = XcmPallet::check_account();
	pub LocalCheckAccount: (AccountId, MintLocation) = (CheckAccount::get(), MintLocation::Local);
	pub TreasuryAccount: AccountId = Treasury::account_id();
	/// The asset ID for the asset that we use to pay for message delivery fees.
	pub FeeAssetId: AssetId = AssetId(TokenLocation::get());
	/// The base fee for the message delivery fees.
	pub const BaseDeliveryFee: u128 = CENTS.saturating_mul(3);
}

pub type LocationConverter = (
	// We can convert a child parachain using the standard `AccountId` conversion.
	ChildParachainConvertsVia<ParaId, AccountId>,
	// We can directly alias an `AccountId32` into a local account.
	AccountId32Aliases<ThisNetwork, AccountId>,
	// Allow governance body to be used as a sovereign account.
	HashedDescription<AccountId, DescribeFamily<DescribeBodyTerminal>>,
);

pub type LocalAssetTransactor = FungibleAdapter<
	// Use this currency:
	Balances,
	// Use this currency when it is a fungible asset matching the given location or name:
	IsConcrete<TokenLocation>,
	// We can convert the Locations with our converter above:
	LocationConverter,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId,
	// It's a native asset so we keep track of the teleports to maintain total issuance.
	LocalCheckAccount,
>;

type LocalOriginConverter = (
	// If the origin kind is `Sovereign`, then return a `Signed` origin with the account determined
	// by the `LocationConverter` converter.
	SovereignSignedViaLocation<LocationConverter, RuntimeOrigin>,
	// If the origin kind is `Native` and the XCM origin is a child parachain, then we can express
	// it with the special `parachains_origin::Origin` origin variant.
	ChildParachainAsNative<parachains_origin::Origin, RuntimeOrigin>,
	// If the origin kind is `Native` and the XCM origin is the `AccountId32` location, then it can
	// be expressed using the `Signed` origin variant.
	SignedAccountId32AsNative<ThisNetwork, RuntimeOrigin>,
	// Xcm origins can be represented natively under the Xcm pallet's Xcm origin.
	XcmPassthrough<RuntimeOrigin>,
);

pub type PriceForChildParachainDelivery =
	ExponentialPrice<FeeAssetId, BaseDeliveryFee, TransactionByteFee, Dmp>;

/// The XCM router. When we want to send an XCM message, we use this type. It amalgamates all of our
/// individual routers.
pub type XcmRouter = WithUniqueTopic<
	// Only one router so far - use DMP to communicate with child parachains.
	ChildParachainRouter<Runtime, XcmPallet, PriceForChildParachainDelivery>,
>;

parameter_types! {
	pub AssetHub: Location = Parachain(ASSET_HUB_ID).into_location();
	pub Collectives: Location = Parachain(COLLECTIVES_ID).into_location();
	pub BridgeHub: Location = Parachain(BRIDGE_HUB_ID).into_location();
	pub People: Location = Parachain(PEOPLE_ID).into_location();
	pub Wnd: AssetFilter = Wild(AllOf { fun: WildFungible, id: AssetId(TokenLocation::get()) });
	pub WndForAssetHub: (AssetFilter, Location) = (Wnd::get(), AssetHub::get());
	pub WndForCollectives: (AssetFilter, Location) = (Wnd::get(), Collectives::get());
	pub WndForBridgeHub: (AssetFilter, Location) = (Wnd::get(), BridgeHub::get());
	pub WndForPeople: (AssetFilter, Location) = (Wnd::get(), People::get());
	pub MaxInstructions: u32 = 100;
	pub MaxAssetsIntoHolding: u32 = 64;
}

pub type TrustedTeleporters = (
	xcm_builder::Case<WndForAssetHub>,
	xcm_builder::Case<WndForCollectives>,
	xcm_builder::Case<WndForBridgeHub>,
	xcm_builder::Case<WndForPeople>,
);

pub struct OnlyParachains;
impl Contains<Location> for OnlyParachains {
	fn contains(location: &Location) -> bool {
		matches!(location.unpack(), (0, [Parachain(_)]))
	}
}

pub struct CollectivesOrFellows;
impl Contains<Location> for CollectivesOrFellows {
	fn contains(location: &Location) -> bool {
		matches!(
			location.unpack(),
			(0, [Parachain(COLLECTIVES_ID)]) |
				(0, [Parachain(COLLECTIVES_ID), Plurality { id: BodyId::Technical, .. }])
		)
	}
}

pub struct LocalPlurality;
impl Contains<Location> for LocalPlurality {
	fn contains(loc: &Location) -> bool {
		matches!(loc.unpack(), (0, [Plurality { .. }]))
	}
}

/// The barriers one of which must be passed for an XCM message to be executed.
pub type Barrier = TrailingSetTopicAsId<(
	// Weight that is paid for may be consumed.
	TakeWeightCredit,
	// Expected responses are OK.
	AllowKnownQueryResponses<XcmPallet>,
	WithComputedOrigin<
		(
			// If the message is one that immediately attempts to pay for execution, then allow it.
			AllowTopLevelPaidExecutionFrom<Everything>,
			// Subscriptions for version tracking are OK.
			AllowSubscriptionsFrom<OnlyParachains>,
			// Collectives and Fellows plurality get free execution.
			AllowExplicitUnpaidExecutionFrom<CollectivesOrFellows>,
		),
		UniversalLocation,
		ConstU32<8>,
	>,
)>;

/// Locations that will not be charged fees in the executor, neither for execution nor delivery.
/// We only waive fees for system functions, which these locations represent.
pub type WaivedLocations = (SystemParachains, Equals<RootLocation>, LocalPlurality);

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = XcmRouter;
	type AssetTransactor = LocalAssetTransactor;
	type OriginConverter = LocalOriginConverter;
	type IsReserve = ();
	type IsTeleporter = TrustedTeleporters;
	type UniversalLocation = UniversalLocation;
	type Barrier = Barrier;
	type Weigher = WeightInfoBounds<
		crate::weights::xcm::WestendXcmWeight<RuntimeCall>,
		RuntimeCall,
		MaxInstructions,
	>;
	type Trader =
		UsingComponents<WeightToFee, TokenLocation, AccountId, Balances, ToAuthor<Runtime>>;
	type ResponseHandler = XcmPallet;
	type AssetTrap = XcmPallet;
	type AssetLocker = ();
	type AssetExchanger = ();
	type AssetClaims = XcmPallet;
	type SubscriptionService = XcmPallet;
	type PalletInstancesInfo = AllPalletsWithSystem;
	type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
	type FeeManager = XcmFeeManagerFromComponents<
		WaivedLocations,
		XcmFeeToAccount<Self::AssetTransactor, AccountId, TreasuryAccount>,
	>;
	type MessageExporter = ();
	type UniversalAliases = Nothing;
	type CallDispatcher = RuntimeCall;
	type SafeCallFilter = Everything;
	type Aliasers = Nothing;
	type TransactionalProcessor = FrameTransactionalProcessor;
}

parameter_types! {
	// `GeneralAdmin` pluralistic body.
	pub const GeneralAdminBodyId: BodyId = BodyId::Administration;
	// StakingAdmin pluralistic body.
	pub const StakingAdminBodyId: BodyId = BodyId::Defense;
	// FellowshipAdmin pluralistic body.
	pub const FellowshipAdminBodyId: BodyId = BodyId::Index(FELLOWSHIP_ADMIN_INDEX);
	// `Treasurer` pluralistic body.
	pub const TreasurerBodyId: BodyId = BodyId::Index(TREASURER_INDEX);
}

/// Type to convert the `GeneralAdmin` origin to a Plurality `Location` value.
pub type GeneralAdminToPlurality =
	OriginToPluralityVoice<RuntimeOrigin, GeneralAdmin, GeneralAdminBodyId>;

/// location of this chain.
pub type LocalOriginToLocation = (
	GeneralAdminToPlurality,
	// And a usual Signed origin to be used in XCM as a corresponding AccountId32
	SignedToAccountId32<RuntimeOrigin, AccountId, ThisNetwork>,
);

/// Type to convert the `StakingAdmin` origin to a Plurality `Location` value.
pub type StakingAdminToPlurality =
	OriginToPluralityVoice<RuntimeOrigin, StakingAdmin, StakingAdminBodyId>;

/// Type to convert the `FellowshipAdmin` origin to a Plurality `Location` value.
pub type FellowshipAdminToPlurality =
	OriginToPluralityVoice<RuntimeOrigin, FellowshipAdmin, FellowshipAdminBodyId>;

/// Type to convert the `Treasurer` origin to a Plurality `Location` value.
pub type TreasurerToPlurality = OriginToPluralityVoice<RuntimeOrigin, Treasurer, TreasurerBodyId>;

/// Type to convert a pallet `Origin` type value into a `Location` value which represents an
/// interior location of this chain for a destination chain.
pub type LocalPalletOriginToLocation = (
	// GeneralAdmin origin to be used in XCM as a corresponding Plurality `Location` value.
	GeneralAdminToPlurality,
	// StakingAdmin origin to be used in XCM as a corresponding Plurality `Location` value.
	StakingAdminToPlurality,
	// FellowshipAdmin origin to be used in XCM as a corresponding Plurality `Location` value.
	FellowshipAdminToPlurality,
	// `Treasurer` origin to be used in XCM as a corresponding Plurality `MultiLocation` value.
	TreasurerToPlurality,
);

impl pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SendXcmOrigin = xcm_builder::EnsureXcmOrigin<RuntimeOrigin, LocalPalletOriginToLocation>;
	type XcmRouter = XcmRouter;
	// Anyone can execute XCM messages locally...
	type ExecuteXcmOrigin = xcm_builder::EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	// ...but they must match our filter, which rejects everything.
	type XcmExecuteFilter = Nothing;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type XcmTeleportFilter = Everything;
	type XcmReserveTransferFilter = Everything;
	type Weigher = WeightInfoBounds<
		crate::weights::xcm::WestendXcmWeight<RuntimeCall>,
		RuntimeCall,
		MaxInstructions,
	>;
	type UniversalLocation = UniversalLocation;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 100;
	type AdvertisedXcmVersion = pallet_xcm::CurrentXcmVersion;
	type Currency = Balances;
	type CurrencyMatcher = IsConcrete<TokenLocation>;
	type TrustedLockers = ();
	type SovereignAccountOf = LocationConverter;
	type MaxLockers = ConstU32<8>;
	type MaxRemoteLockConsumers = ConstU32<0>;
	type RemoteLockConsumerIdentifier = ();
	type WeightInfo = crate::weights::pallet_xcm::WeightInfo<Runtime>;
	type AdminOrigin = EnsureRoot<AccountId>;
}
