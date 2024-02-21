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

//! XCM configuration for Rococo.

use super::{
	parachains_origin, AccountId, AllPalletsWithSystem, Balances, Dmp, Fellows, ParaId, Runtime,
	RuntimeCall, RuntimeEvent, RuntimeOrigin, TransactionByteFee, Treasury, WeightToFee, XcmPallet,
};

use crate::governance::StakingAdmin;

use frame_support::{
	parameter_types,
	traits::{Contains, Equals, Everything, Nothing},
	weights::Weight,
};
use frame_system::EnsureRoot;
use rococo_runtime_constants::{currency::CENTS, system_parachain::*};
use runtime_common::{
	xcm_sender::{ChildParachainRouter, ExponentialPrice},
	ToAuthor,
};
use sp_core::ConstU32;
use xcm::latest::prelude::*;
use xcm_builder::{
	AccountId32Aliases, AllowExplicitUnpaidExecutionFrom, AllowKnownQueryResponses,
	AllowSubscriptionsFrom, AllowTopLevelPaidExecutionFrom, ChildParachainAsNative,
	ChildParachainConvertsVia, DescribeBodyTerminal, DescribeFamily, FixedWeightBounds,
	FrameTransactionalProcessor, FungibleAdapter, HashedDescription, IsChildSystemParachain,
	IsConcrete, MintLocation, OriginToPluralityVoice, SignedAccountId32AsNative,
	SignedToAccountId32, SovereignSignedViaLocation, TakeWeightCredit, TrailingSetTopicAsId,
	UsingComponents, WeightInfoBounds, WithComputedOrigin, WithUniqueTopic,
	XcmFeeManagerFromComponents, XcmFeeToAccount,
};
use xcm_executor::XcmExecutor;

parameter_types! {
	pub TokenLocation: Location = Here.into_location();
	pub RootLocation: Location = Location::here();
	pub const ThisNetwork: NetworkId = NetworkId::Rococo;
	pub UniversalLocation: InteriorLocation = ThisNetwork::get().into();
	pub CheckAccount: AccountId = XcmPallet::check_account();
	pub LocalCheckAccount: (AccountId, MintLocation) = (CheckAccount::get(), MintLocation::Local);
	pub TreasuryAccount: AccountId = Treasury::account_id();
}

pub type LocationConverter = (
	// We can convert a child parachain using the standard `AccountId` conversion.
	ChildParachainConvertsVia<ParaId, AccountId>,
	// We can directly alias an `AccountId32` into a local account.
	AccountId32Aliases<ThisNetwork, AccountId>,
	// Allow governance body to be used as a sovereign account.
	HashedDescription<AccountId, DescribeFamily<DescribeBodyTerminal>>,
);

/// Our asset transactor. This is what allows us to interest with the runtime facilities from the
/// point of view of XCM-only concepts like `Location` and `Asset`.
///
/// Ours is only aware of the Balances pallet, which is mapped to `RocLocation`.
pub type LocalAssetTransactor = FungibleAdapter<
	// Use this currency:
	Balances,
	// Use this currency when it is a fungible asset matching the given location or name:
	IsConcrete<TokenLocation>,
	// We can convert the Locations with our converter above:
	LocationConverter,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId,
	// We track our teleports in/out to keep total issuance correct.
	LocalCheckAccount,
>;

/// The means that we convert an the XCM message origin location into a local dispatch origin.
type LocalOriginConverter = (
	// A `Signed` origin of the sovereign account that the original location controls.
	SovereignSignedViaLocation<LocationConverter, RuntimeOrigin>,
	// A child parachain, natively expressed, has the `Parachain` origin.
	ChildParachainAsNative<parachains_origin::Origin, RuntimeOrigin>,
	// The AccountId32 location type can be expressed natively as a `Signed` origin.
	SignedAccountId32AsNative<ThisNetwork, RuntimeOrigin>,
);

parameter_types! {
	/// The amount of weight an XCM operation takes. This is a safe overestimate.
	pub const BaseXcmWeight: Weight = Weight::from_parts(1_000_000_000, 64 * 1024);
	/// The asset ID for the asset that we use to pay for message delivery fees.
	pub FeeAssetId: AssetId = AssetId(TokenLocation::get());
	/// The base fee for the message delivery fees.
	pub const BaseDeliveryFee: u128 = CENTS.saturating_mul(3);
}

pub type PriceForChildParachainDelivery =
	ExponentialPrice<FeeAssetId, BaseDeliveryFee, TransactionByteFee, Dmp>;

/// The XCM router. When we want to send an XCM message, we use this type. It amalgamates all of our
/// individual routers.
pub type XcmRouter = WithUniqueTopic<
	// Only one router so far - use DMP to communicate with child parachains.
	ChildParachainRouter<Runtime, XcmPallet, PriceForChildParachainDelivery>,
>;

parameter_types! {
	pub Roc: AssetFilter = Wild(AllOf { fun: WildFungible, id: AssetId(TokenLocation::get()) });
	pub AssetHub: Location = Parachain(ASSET_HUB_ID).into_location();
	pub Contracts: Location = Parachain(CONTRACTS_ID).into_location();
	pub Encointer: Location = Parachain(ENCOINTER_ID).into_location();
	pub BridgeHub: Location = Parachain(BRIDGE_HUB_ID).into_location();
	pub People: Location = Parachain(PEOPLE_ID).into_location();
	pub Broker: Location = Parachain(BROKER_ID).into_location();
	pub Tick: Location = Parachain(100).into_location();
	pub Trick: Location = Parachain(110).into_location();
	pub Track: Location = Parachain(120).into_location();
	pub RocForTick: (AssetFilter, Location) = (Roc::get(), Tick::get());
	pub RocForTrick: (AssetFilter, Location) = (Roc::get(), Trick::get());
	pub RocForTrack: (AssetFilter, Location) = (Roc::get(), Track::get());
	pub RocForAssetHub: (AssetFilter, Location) = (Roc::get(), AssetHub::get());
	pub RocForContracts: (AssetFilter, Location) = (Roc::get(), Contracts::get());
	pub RocForEncointer: (AssetFilter, Location) = (Roc::get(), Encointer::get());
	pub RocForBridgeHub: (AssetFilter, Location) = (Roc::get(), BridgeHub::get());
	pub RocForPeople: (AssetFilter, Location) = (Roc::get(), People::get());
	pub RocForBroker: (AssetFilter, Location) = (Roc::get(), Broker::get());
	pub const MaxInstructions: u32 = 100;
	pub const MaxAssetsIntoHolding: u32 = 64;
}
pub type TrustedTeleporters = (
	xcm_builder::Case<RocForTick>,
	xcm_builder::Case<RocForTrick>,
	xcm_builder::Case<RocForTrack>,
	xcm_builder::Case<RocForAssetHub>,
	xcm_builder::Case<RocForContracts>,
	xcm_builder::Case<RocForEncointer>,
	xcm_builder::Case<RocForBridgeHub>,
	xcm_builder::Case<RocForPeople>,
	xcm_builder::Case<RocForBroker>,
);

pub struct OnlyParachains;
impl Contains<Location> for OnlyParachains {
	fn contains(loc: &Location) -> bool {
		matches!(loc.unpack(), (0, [Parachain(_)]))
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
			// Messages coming from system parachains need not pay for execution.
			AllowExplicitUnpaidExecutionFrom<IsChildSystemParachain<ParaId>>,
			// Subscriptions for version tracking are OK.
			AllowSubscriptionsFrom<OnlyParachains>,
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
		crate::weights::xcm::RococoXcmWeight<RuntimeCall>,
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
	pub const CollectiveBodyId: BodyId = BodyId::Unit;
	// StakingAdmin pluralistic body.
	pub const StakingAdminBodyId: BodyId = BodyId::Defense;
	// Fellows pluralistic body.
	pub const FellowsBodyId: BodyId = BodyId::Technical;
}

/// Type to convert an `Origin` type value into a `Location` value which represents an interior
/// location of this chain.
pub type LocalOriginToLocation = (
	// And a usual Signed origin to be used in XCM as a corresponding AccountId32
	SignedToAccountId32<RuntimeOrigin, AccountId, ThisNetwork>,
);

/// Type to convert the `StakingAdmin` origin to a Plurality `Location` value.
pub type StakingAdminToPlurality =
	OriginToPluralityVoice<RuntimeOrigin, StakingAdmin, StakingAdminBodyId>;

/// Type to convert the Fellows origin to a Plurality `Location` value.
pub type FellowsToPlurality = OriginToPluralityVoice<RuntimeOrigin, Fellows, FellowsBodyId>;

/// Type to convert a pallet `Origin` type value into a `Location` value which represents an
/// interior location of this chain for a destination chain.
pub type LocalPalletOriginToLocation = (
	// StakingAdmin origin to be used in XCM as a corresponding Plurality `Location` value.
	StakingAdminToPlurality,
	// Fellows origin to be used in XCM as a corresponding Plurality `Location` value.
	FellowsToPlurality,
);

impl pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	// We only allow the root, fellows and the staking admin to send messages.
	// This is basically safe to enable for everyone (safe the possibility of someone spamming the
	// parachain if they're willing to pay the KSM to send from the Relay-chain), but it's useless
	// until we bring in XCM v3 which will make `DescendOrigin` a bit more useful.
	type SendXcmOrigin = xcm_builder::EnsureXcmOrigin<RuntimeOrigin, LocalPalletOriginToLocation>;
	type XcmRouter = XcmRouter;
	// Anyone can execute XCM messages locally.
	type ExecuteXcmOrigin = xcm_builder::EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	type XcmExecuteFilter = Everything;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type XcmTeleportFilter = Everything;
	// Anyone is able to use reserve transfers regardless of who they are and what they want to
	// transfer.
	type XcmReserveTransferFilter = Everything;
	type Weigher = FixedWeightBounds<BaseXcmWeight, RuntimeCall, MaxInstructions>;
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
