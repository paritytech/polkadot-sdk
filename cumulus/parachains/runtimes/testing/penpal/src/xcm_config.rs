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

//! Holds the XCM specific configuration that would otherwise be in lib.rs
//!
//! This configuration dictates how the Penpal chain will communicate with other chains.
//!
//! One of the main uses of the penpal chain will be to be a benefactor of reserve asset transfers
//! with Asset Hub as the reserve. At present no derivative tokens are minted on receipt of a
//! `ReserveAssetTransferDeposited` message but that will but the intension will be to support this
//! soon.
use super::{
	AccountId, AllPalletsWithSystem, AssetId as AssetIdPalletAssets, Assets, Authorship, Balance,
	Balances, CollatorSelection, ForeignAssets, ForeignAssetsInstance, NonZeroIssuance,
	ParachainInfo, ParachainSystem, PolkadotXcm, Runtime, RuntimeCall, RuntimeEvent, RuntimeOrigin,
	WeightToFee, XcmpQueue,
};
use crate::{BaseDeliveryFee, FeeAssetId, TransactionByteFee};
use assets_common::TrustBackedAssetsAsLocation;
use core::marker::PhantomData;
use frame_support::{
	parameter_types,
	traits::{
		tokens::imbalance::ResolveAssetTo, ConstU32, Contains, ContainsPair, Everything,
		EverythingBut, Get, Nothing, PalletInfoAccess,
	},
	weights::Weight,
};
use frame_system::EnsureRoot;
use pallet_xcm::XcmPassthrough;
use parachains_common::{xcm_config::AssetFeeAsExistentialDepositMultiplier, TREASURY_PALLET_ID};
use polkadot_parachain_primitives::primitives::Sibling;
use polkadot_runtime_common::{impls::ToAuthor, xcm_sender::ExponentialPrice};
use snowbridge_router_primitives::inbound::GlobalConsensusEthereumConvertsFor;
use sp_runtime::traits::{AccountIdConversion, ConvertInto, Identity, TryConvertInto};
use xcm::latest::prelude::*;
use xcm_builder::{
	AccountId32Aliases, AliasOriginRootUsingFilter, AllowHrmpNotificationsFromRelayChain,
	AllowKnownQueryResponses, AllowSubscriptionsFrom, AllowTopLevelPaidExecutionFrom,
	AsPrefixedGeneralIndex, ConvertedConcreteId, DescribeAllTerminal, DescribeFamily,
	EnsureXcmOrigin, FixedWeightBounds, FrameTransactionalProcessor, FungibleAdapter,
	FungiblesAdapter, GlobalConsensusParachainConvertsFor, HashedDescription, IsConcrete,
	LocalMint, NativeAsset, NoChecking, ParentAsSuperuser, ParentIsPreset, RelayChainAsNative,
	SendXcmFeeToAccount, SiblingParachainAsNative, SiblingParachainConvertsVia,
	SignedAccountId32AsNative, SignedToAccountId32, SingleAssetExchangeAdapter,
	SovereignSignedViaLocation, StartsWith, TakeWeightCredit, TrailingSetTopicAsId,
	UsingComponents, WithComputedOrigin, WithUniqueTopic, XcmFeeManagerFromComponents,
};
use xcm_executor::{traits::JustTry, XcmExecutor};

parameter_types! {
	pub const RelayLocation: Location = Location::parent();
	// Local native currency which is stored in `pallet_balances`
	pub const PenpalNativeCurrency: Location = Location::here();
	// The Penpal runtime is utilized for testing with various environment setups.
	// This storage item allows us to customize the `NetworkId` where Penpal is deployed.
	// By default, it is set to `NetworkId::Westend` and can be changed using `System::set_storage`.
	pub storage RelayNetworkId: NetworkId = NetworkId::Westend;
	pub RelayNetwork: Option<NetworkId> = Some(RelayNetworkId::get());
	pub RelayChainOrigin: RuntimeOrigin = cumulus_pallet_xcm::Origin::Relay.into();
	pub UniversalLocation: InteriorLocation = [
		GlobalConsensus(RelayNetworkId::get()),
		Parachain(ParachainInfo::parachain_id().into())
	].into();
	pub TreasuryAccount: AccountId = TREASURY_PALLET_ID.into_account_truncating();
	pub StakingPot: AccountId = CollatorSelection::account_id();
	pub TrustBackedAssetsPalletIndex: u8 = <Assets as PalletInfoAccess>::index() as u8;
	pub TrustBackedAssetsPalletLocation: Location =
		PalletInstance(TrustBackedAssetsPalletIndex::get()).into();
}

/// Type for specifying how a `Location` can be converted into an `AccountId`. This is used
/// when determining ownership of accounts for asset transacting and when attempting to use XCM
/// `Transact` in order to determine the dispatch Origin.
pub type LocationToAccountId = (
	// The parent (Relay-chain) origin converts to the parent `AccountId`.
	ParentIsPreset<AccountId>,
	// Sibling parachain origins convert to AccountId via the `ParaId::into`.
	SiblingParachainConvertsVia<Sibling, AccountId>,
	// Straight up local `AccountId32` origins just alias directly to `AccountId`.
	AccountId32Aliases<RelayNetwork, AccountId>,
	// Foreign locations alias into accounts according to a hash of their standard description.
	HashedDescription<AccountId, DescribeFamily<DescribeAllTerminal>>,
	// Different global consensus parachain sovereign account.
	// (Used for over-bridge transfers and reserve processing)
	GlobalConsensusParachainConvertsFor<UniversalLocation, AccountId>,
	// Ethereum contract sovereign account.
	// (Used to get convert ethereum contract locations to sovereign account)
	GlobalConsensusEthereumConvertsFor<AccountId>,
);

/// Means for transacting assets on this chain.
pub type FungibleTransactor = FungibleAdapter<
	// Use this currency:
	Balances,
	// Use this currency when it is a fungible asset matching the given location or name:
	IsConcrete<PenpalNativeCurrency>,
	// Do a simple punn to convert an AccountId32 Location into a native chain account ID:
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId,
	// We don't track any teleports.
	(),
>;

/// Means for transacting assets besides the native currency on this chain.
pub type FungiblesTransactor = FungiblesAdapter<
	// Use this fungibles implementation:
	Assets,
	// Use this currency when it is a fungible asset matching the given location or name:
	(
		ConvertedConcreteId<
			AssetIdPalletAssets,
			Balance,
			AsPrefixedGeneralIndex<AssetsPalletLocation, AssetIdPalletAssets, JustTry>,
			JustTry,
		>,
		ConvertedConcreteId<
			AssetIdPalletAssets,
			Balance,
			AsPrefixedGeneralIndex<
				SystemAssetHubAssetsPalletLocation,
				AssetIdPalletAssets,
				JustTry,
			>,
			JustTry,
		>,
	),
	// Convert an XCM Location into a local account id:
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId,
	// We only want to allow teleports of known assets. We use non-zero issuance as an indication
	// that this asset is known.
	LocalMint<NonZeroIssuance<AccountId, Assets>>,
	// The account to use for tracking teleports.
	CheckingAccount,
>;

// Using the latest `Location`, we don't need to worry about migrations for Penpal.
pub type ForeignAssetsAssetId = Location;
pub type ForeignAssetsConvertedConcreteId = xcm_builder::MatchedConvertedConcreteId<
	Location,
	Balance,
	EverythingBut<(
		// Here we rely on fact that something like this works:
		// assert!(Location::new(1,
		// [Parachain(100)]).starts_with(&Location::parent()));
		// assert!([Parachain(100)].into().starts_with(&Here));
		StartsWith<assets_common::matching::LocalLocationPattern>,
	)>,
	Identity,
	TryConvertInto,
>;

/// Means for transacting foreign assets from different global consensus.
pub type ForeignFungiblesTransactor = FungiblesAdapter<
	// Use this fungibles implementation:
	ForeignAssets,
	// Use this currency when it is a fungible asset matching the given location or name:
	ForeignAssetsConvertedConcreteId,
	// Convert an XCM Location into a local account id:
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId,
	// We don't need to check teleports here.
	NoChecking,
	// The account to use for tracking teleports.
	CheckingAccount,
>;

/// Means for transacting assets on this chain.
pub type AssetTransactors = (FungibleTransactor, ForeignFungiblesTransactor, FungiblesTransactor);

/// This is the type we use to convert an (incoming) XCM origin into a local `Origin` instance,
/// ready for dispatching a transaction with Xcm's `Transact`. There is an `OriginKind` which can
/// biases the kind of local `Origin` it will become.
pub type XcmOriginToTransactDispatchOrigin = (
	// Sovereign account converter; this attempts to derive an `AccountId` from the origin location
	// using `LocationToAccountId` and then turn that into the usual `Signed` origin. Useful for
	// foreign chains who want to have a local sovereign account on this chain which they control.
	SovereignSignedViaLocation<LocationToAccountId, RuntimeOrigin>,
	// Native converter for Relay-chain (Parent) location; will convert to a `Relay` origin when
	// recognized.
	RelayChainAsNative<RelayChainOrigin, RuntimeOrigin>,
	// Native converter for sibling Parachains; will convert to a `SiblingPara` origin when
	// recognized.
	SiblingParachainAsNative<cumulus_pallet_xcm::Origin, RuntimeOrigin>,
	// Superuser converter for the Relay-chain (Parent) location. This will allow it to issue a
	// transaction from the Root origin.
	ParentAsSuperuser<RuntimeOrigin>,
	// Native signed account converter; this just converts an `AccountId32` origin into a normal
	// `RuntimeOrigin::Signed` origin of the same 32-byte value.
	SignedAccountId32AsNative<RelayNetwork, RuntimeOrigin>,
	// Xcm origins can be represented natively under the Xcm pallet's Xcm origin.
	XcmPassthrough<RuntimeOrigin>,
);

parameter_types! {
	// One XCM operation is 1_000_000_000 weight - almost certainly a conservative estimate.
	pub UnitWeightCost: Weight = Weight::from_parts(1_000_000_000, 64 * 1024);
	pub const MaxInstructions: u32 = 100;
	pub const MaxAssetsIntoHolding: u32 = 64;
	pub XcmAssetFeesReceiver: Option<AccountId> = Authorship::author();
}

pub struct ParentOrParentsExecutivePlurality;
impl Contains<Location> for ParentOrParentsExecutivePlurality {
	fn contains(location: &Location) -> bool {
		matches!(location.unpack(), (1, []) | (1, [Plurality { id: BodyId::Executive, .. }]))
	}
}

pub type Barrier = TrailingSetTopicAsId<(
	TakeWeightCredit,
	// Expected responses are OK.
	AllowKnownQueryResponses<PolkadotXcm>,
	// Allow XCMs with some computed origins to pass through.
	WithComputedOrigin<
		(
			// If the message is one that immediately attempts to pay for execution, then
			// allow it.
			AllowTopLevelPaidExecutionFrom<Everything>,
			// Subscriptions for version tracking are OK.
			AllowSubscriptionsFrom<Everything>,
			// HRMP notifications from the relay chain are OK.
			AllowHrmpNotificationsFromRelayChain,
		),
		UniversalLocation,
		ConstU32<8>,
	>,
)>;

/// Type alias to conveniently refer to `frame_system`'s `Config::AccountId`.
pub type AccountIdOf<R> = <R as frame_system::Config>::AccountId;

/// Asset filter that allows all assets from a certain location matching asset id.
pub struct AssetPrefixFrom<Prefix, Origin>(PhantomData<(Prefix, Origin)>);
impl<Prefix, Origin> ContainsPair<Asset, Location> for AssetPrefixFrom<Prefix, Origin>
where
	Prefix: Get<Location>,
	Origin: Get<Location>,
{
	fn contains(asset: &Asset, origin: &Location) -> bool {
		let loc = Origin::get();
		&loc == origin &&
			matches!(asset, Asset { id: AssetId(asset_loc), fun: Fungible(_a) }
			if asset_loc.starts_with(&Prefix::get()))
	}
}

type AssetsFrom<T> = AssetPrefixFrom<T, T>;

/// Asset filter that allows native/relay asset if coming from a certain location.
pub struct NativeAssetFrom<T>(PhantomData<T>);
impl<T: Get<Location>> ContainsPair<Asset, Location> for NativeAssetFrom<T> {
	fn contains(asset: &Asset, origin: &Location) -> bool {
		let loc = T::get();
		&loc == origin &&
			matches!(asset, Asset { id: AssetId(asset_loc), fun: Fungible(_a) }
			if *asset_loc == Location::from(Parent))
	}
}

// This asset can be added to AH as Asset and reserved transfer between Penpal and AH
pub const RESERVABLE_ASSET_ID: u32 = 1;
// This asset can be added to AH as ForeignAsset and teleported between Penpal and AH
pub const TELEPORTABLE_ASSET_ID: u32 = 2;

pub const ASSETS_PALLET_ID: u8 = 50;
pub const ASSET_HUB_ID: u32 = 1000;

pub const USDT_ASSET_ID: u128 = 1984;

parameter_types! {
	/// The location that this chain recognizes as the Relay network's Asset Hub.
	pub SystemAssetHubLocation: Location = Location::new(1, [Parachain(ASSET_HUB_ID)]);
	// the Relay Chain's Asset Hub's Assets pallet index
	pub SystemAssetHubAssetsPalletLocation: Location =
		Location::new(1, [Parachain(ASSET_HUB_ID), PalletInstance(ASSETS_PALLET_ID)]);
	pub AssetsPalletLocation: Location =
		Location::new(0, [PalletInstance(ASSETS_PALLET_ID)]);
	pub CheckingAccount: AccountId = PolkadotXcm::check_account();
	pub LocalTeleportableToAssetHub: Location = Location::new(
		0,
		[PalletInstance(ASSETS_PALLET_ID), GeneralIndex(TELEPORTABLE_ASSET_ID.into())]
	);
	pub LocalReservableFromAssetHub: Location = Location::new(
		1,
		[Parachain(ASSET_HUB_ID), PalletInstance(ASSETS_PALLET_ID), GeneralIndex(RESERVABLE_ASSET_ID.into())]
	);
	pub UsdtFromAssetHub: Location = Location::new(
		1,
		[Parachain(ASSET_HUB_ID), PalletInstance(ASSETS_PALLET_ID), GeneralIndex(USDT_ASSET_ID)],
	);

	/// The Penpal runtime is utilized for testing with various environment setups.
	/// This storage item provides the opportunity to customize testing scenarios
	/// by configuring the trusted asset from the `SystemAssetHub`.
	///
	/// By default, it is configured as a `SystemAssetHubLocation` and can be modified using `System::set_storage`.
	pub storage CustomizableAssetFromSystemAssetHub: Location = SystemAssetHubLocation::get();
}

/// Accepts asset with ID `AssetLocation` and is coming from `Origin` chain.
pub struct AssetFromChain<AssetLocation, Origin>(PhantomData<(AssetLocation, Origin)>);
impl<AssetLocation: Get<Location>, Origin: Get<Location>> ContainsPair<Asset, Location>
	for AssetFromChain<AssetLocation, Origin>
{
	fn contains(asset: &Asset, origin: &Location) -> bool {
		log::trace!(target: "xcm::contains", "AssetFromChain asset: {:?}, origin: {:?}", asset, origin);
		*origin == Origin::get() &&
			matches!(asset.id.clone(), AssetId(id) if id == AssetLocation::get())
	}
}

pub type TrustedReserves = (
	NativeAsset,
	AssetsFrom<SystemAssetHubLocation>,
	NativeAssetFrom<SystemAssetHubLocation>,
	AssetPrefixFrom<CustomizableAssetFromSystemAssetHub, SystemAssetHubLocation>,
);
pub type TrustedTeleporters =
	(AssetFromChain<LocalTeleportableToAssetHub, SystemAssetHubLocation>,);

/// `AssetId`/`Balance` converter for `TrustBackedAssets`.
pub type TrustBackedAssetsConvertedConcreteId =
	assets_common::TrustBackedAssetsConvertedConcreteId<AssetsPalletLocation, Balance>;

/// Asset converter for pool assets.
/// Used to convert assets in pools to the asset required for fee payment.
/// The pool must be between the first asset and the one required for fee payment.
/// This type allows paying fees with any asset in a pool with the asset required for fee payment.
pub type PoolAssetsExchanger = SingleAssetExchangeAdapter<
	crate::AssetConversion,
	crate::NativeAndAssets,
	(
		TrustBackedAssetsAsLocation<
			TrustBackedAssetsPalletLocation,
			Balance,
			xcm::latest::Location,
		>,
		ForeignAssetsConvertedConcreteId,
	),
	AccountId,
>;

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = XcmRouter;
	// How to withdraw and deposit an asset.
	type AssetTransactor = AssetTransactors;
	type OriginConverter = XcmOriginToTransactDispatchOrigin;
	type IsReserve = TrustedReserves;
	// no teleport trust established with other chains
	type IsTeleporter = TrustedTeleporters;
	type UniversalLocation = UniversalLocation;
	type Barrier = Barrier;
	type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
	type Trader = (
		UsingComponents<WeightToFee, RelayLocation, AccountId, Balances, ToAuthor<Runtime>>,
		cumulus_primitives_utility::SwapFirstAssetTrader<
			RelayLocation,
			crate::AssetConversion,
			WeightToFee,
			crate::NativeAndAssets,
			(
				TrustBackedAssetsAsLocation<
					TrustBackedAssetsPalletLocation,
					Balance,
					xcm::latest::Location,
				>,
				ForeignAssetsConvertedConcreteId,
			),
			ResolveAssetTo<StakingPot, crate::NativeAndAssets>,
			AccountId,
		>,
	);
	type ResponseHandler = PolkadotXcm;
	type AssetTrap = PolkadotXcm;
	type AssetClaims = PolkadotXcm;
	type SubscriptionService = PolkadotXcm;
	type PalletInstancesInfo = AllPalletsWithSystem;
	type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
	type AssetLocker = ();
	type AssetExchanger = PoolAssetsExchanger;
	type FeeManager = XcmFeeManagerFromComponents<
		(),
		SendXcmFeeToAccount<Self::AssetTransactor, TreasuryAccount>,
	>;
	type MessageExporter = ();
	type UniversalAliases = Nothing;
	type CallDispatcher = RuntimeCall;
	type SafeCallFilter = Everything;
	// We allow trusted Asset Hub root to alias other locations.
	type Aliasers = AliasOriginRootUsingFilter<SystemAssetHubLocation, Everything>;
	type TransactionalProcessor = FrameTransactionalProcessor;
	type HrmpNewChannelOpenRequestHandler = ();
	type HrmpChannelAcceptedHandler = ();
	type HrmpChannelClosingHandler = ();
	type XcmRecorder = PolkadotXcm;
}

/// Multiplier used for dedicated `TakeFirstAssetTrader` with `ForeignAssets` instance.
pub type ForeignAssetFeeAsExistentialDepositMultiplierFeeCharger =
	AssetFeeAsExistentialDepositMultiplier<
		Runtime,
		WeightToFee,
		pallet_assets::BalanceToAssetBalance<Balances, Runtime, ConvertInto, ForeignAssetsInstance>,
		ForeignAssetsInstance,
	>;

/// No local origins on this chain are allowed to dispatch XCM sends/executions.
pub type LocalOriginToLocation = SignedToAccountId32<RuntimeOrigin, AccountId, RelayNetwork>;

pub type PriceForParentDelivery =
	ExponentialPrice<FeeAssetId, BaseDeliveryFee, TransactionByteFee, ParachainSystem>;

/// The means for routing XCM messages which are not for local execution into the right message
/// queues.
pub type XcmRouter = WithUniqueTopic<(
	// Two routers - use UMP to communicate with the relay chain:
	cumulus_primitives_utility::ParentAsUmp<ParachainSystem, PolkadotXcm, PriceForParentDelivery>,
	// ..and XCMP to communicate with the sibling chains.
	XcmpQueue,
)>;

impl pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SendXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	type XcmRouter = XcmRouter;
	type ExecuteXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	type XcmExecuteFilter = Everything;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type XcmTeleportFilter = Everything;
	type XcmReserveTransferFilter = Everything;
	type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
	type UniversalLocation = UniversalLocation;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;

	const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 100;
	// ^ Override for AdvertisedXcmVersion default
	type AdvertisedXcmVersion = pallet_xcm::CurrentXcmVersion;
	type Currency = Balances;
	type CurrencyMatcher = ();
	type TrustedLockers = ();
	type SovereignAccountOf = LocationToAccountId;
	type MaxLockers = ConstU32<8>;
	type WeightInfo = pallet_xcm::TestWeightInfo;
	type AdminOrigin = EnsureRoot<AccountId>;
	type MaxRemoteLockConsumers = ConstU32<0>;
	type RemoteLockConsumerIdentifier = ();
}

impl cumulus_pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmExecutor = XcmExecutor<XcmConfig>;
}

/// Simple conversion of `u32` into an `AssetId` for use in benchmarking.
pub struct XcmBenchmarkHelper;
#[cfg(feature = "runtime-benchmarks")]
impl pallet_assets::BenchmarkHelper<ForeignAssetsAssetId> for XcmBenchmarkHelper {
	fn create_asset_id_parameter(id: u32) -> ForeignAssetsAssetId {
		Location::new(1, [Parachain(id)])
	}
}
