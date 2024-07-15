// Copyright Parity Technologies (UK) Ltd.
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

//! # XCM Configuration

use frame::{
	deps::frame_system,
	runtime::prelude::*,
	traits::{Everything, Nothing},
};
use xcm::v4::prelude::*;
use xcm_builder::{
	AccountId32Aliases, DescribeAllTerminal, DescribeFamily, EnsureXcmOrigin,
	FrameTransactionalProcessor, HashedDescription, IsConcrete,
	SignedToAccountId32,
};
use xcm_executor::XcmExecutor;

use super::{AccountId, Balances, MessageQueue, Runtime, RuntimeCall, RuntimeEvent, RuntimeOrigin};

parameter_types! {
	pub RelayLocation: Location = Location::parent();
	pub ThisNetwork: NetworkId = NetworkId::Polkadot;
}

pub type LocationToAccountId = (
	HashedDescription<AccountId, DescribeFamily<DescribeAllTerminal>>,
	AccountId32Aliases<ThisNetwork, AccountId>,
);

/// Configuration related to asset transactors
#[docify::export]
mod asset_transactor {
	use super::*;

	use frame::traits::EverythingBut;
	use xcm_builder::{
		FungibleAdapter, FungiblesAdapter, IsConcrete, MatchedConvertedConcreteId, MintLocation,
		NoChecking, StartsWith,
	};
	use xcm_executor::traits::Identity;

	use super::super::{
		AccountId, Balance, Balances, ForeignAssets, PolkadotXcm,
	};

	parameter_types! {
		pub LocalPrefix: Location = Location::here();
		pub CheckingAccount: AccountId = PolkadotXcm::check_account();
	}

	/// AssetTransactor for handling the chain's native token.
	pub type FungibleTransactor = FungibleAdapter<
		// What implementation of the `fungible::*` traits do we want to use?
		Balances,
		// What tokens should be handled by this transactor?
		IsConcrete<LocalPrefix>,
		// How do we convert an XCM Location into a local account id?
		LocationToAccountId,
		// The type for account ids, only needed because `fungible` is generic over it.
		AccountId,
		// Tracking teleports.
		(),
	>;

	/// Type that matches foreign assets.
	/// We do this by matching on all possible Locations and excluding the ones
	/// inside our local chain.
	pub type ForeignAssetsMatcher = MatchedConvertedConcreteId<
		Location, // Asset id.
		Balance, // Balance type.
		EverythingBut<StartsWith<LocalPrefix>>, // Location matcher.
		Identity, // How to convert from Location to AssetId.
		Identity, // How to convert from u128 to Balance.
	>;

	/// AssetTransactor for handling other parachains' native tokens.
	pub type ForeignFungiblesTransactor = FungiblesAdapter<
		// What implementation of the `fungible::*` traits do we want to use?
		ForeignAssets,
		// What tokens should be handled by this transactor?
		ForeignAssetsMatcher,
		// How we convert from a Location to an account id.
		LocationToAccountId,
		// The `AccountId` type.
		AccountId,
		// Not tracking teleports since we only use reserve asset transfers for these types
		// of assets.
		NoChecking,
		// The account for checking.
		CheckingAccount,
	>;

    /// Actual configuration item that'll be used in the XCM config.
    /// It's a tuple, which means the transactors will be tried one by one until
    /// one of them matches the asset being handled.
	pub type AssetTransactor = (FungibleTransactor, ForeignFungiblesTransactor);
}

/// Configuration related to token reserves
#[docify::export]
mod is_reserve {
    use xcm_builder::NativeAsset;

    /// [`NativeAsset`] here means we trust any location as a reserve for their own native asset.
	pub type IsReserve = NativeAsset;
}

mod weigher {
	use super::*;
	use xcm_builder::FixedWeightBounds;

	parameter_types! {
		pub const WeightPerInstruction: Weight = Weight::from_parts(1, 1);
		pub const MaxInstructions: u32 = 100;
	}

	pub type Weigher = FixedWeightBounds<WeightPerInstruction, RuntimeCall, MaxInstructions>;
}

parameter_types! {
	pub UniversalLocation: InteriorLocation = [GlobalConsensus(NetworkId::Polkadot), Parachain(MessageQueue::parachain_id().into())].into();
}

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = ();
	type AssetTransactor = asset_transactor::AssetTransactor;
	type OriginConverter = ();
	// The declaration of which Locations are reserves for which Assets.
	type IsReserve = is_reserve::IsReserve;
	type IsTeleporter = ();
	type UniversalLocation = UniversalLocation;
	// This is not safe, you should use `xcm_builder::AllowTopLevelPaidExecutionFrom<T>` in a
	// production chain
	type Barrier = xcm_builder::AllowUnpaidExecutionFrom<Everything>;
	type Weigher = weigher::Weigher;
	type Trader = ();
	type ResponseHandler = ();
	type AssetTrap = ();
	type AssetLocker = ();
	type AssetExchanger = ();
	type AssetClaims = ();
	type SubscriptionService = ();
	type PalletInstancesInfo = ();
	type FeeManager = ();
	type MaxAssetsIntoHolding = frame::traits::ConstU32<1>;
	type MessageExporter = ();
	type UniversalAliases = Nothing;
	type CallDispatcher = RuntimeCall;
	type SafeCallFilter = Everything;
	type Aliasers = Nothing;
	type TransactionalProcessor = FrameTransactionalProcessor;
	type HrmpNewChannelOpenRequestHandler = ();
	type HrmpChannelAcceptedHandler = ();
	type HrmpChannelClosingHandler = ();
	type XcmRecorder = ();
}

pub type LocalOriginToLocation = SignedToAccountId32<RuntimeOrigin, AccountId, ThisNetwork>;

impl pallet_xcm::Config for Runtime {
	// We turn off sending for these tests
	type SendXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, ()>;
	type XcmRouter = super::super::network::ParachainXcmRouter<MessageQueue>; // Provided by xcm-simulator
																		  // Anyone can execute XCM programs
	type ExecuteXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	// We execute any type of program
	type XcmExecuteFilter = Everything;
	// How we execute programs
	type XcmExecutor = XcmExecutor<XcmConfig>;
	// We don't allow teleports
	type XcmTeleportFilter = Nothing;
	// We allow all reserve transfers
	type XcmReserveTransferFilter = Everything;
	// Same weigher executor uses to weigh XCM programs
	type Weigher = weigher::Weigher;
	// Same universal location
	type UniversalLocation = UniversalLocation;
	// No version discovery needed
	const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 0;
	type AdvertisedXcmVersion = frame::traits::ConstU32<3>;
	type AdminOrigin = frame_system::EnsureRoot<AccountId>;
	// No locking
	type TrustedLockers = ();
	type MaxLockers = frame::traits::ConstU32<0>;
	type MaxRemoteLockConsumers = frame::traits::ConstU32<0>;
	type RemoteLockConsumerIdentifier = ();
	// How to turn locations into accounts
	type SovereignAccountOf = LocationToAccountId;
	// A currency to pay for things and its matcher, we are using the relay token
	type Currency = Balances;
	type CurrencyMatcher = IsConcrete<RelayLocation>;
	// Pallet benchmarks, no need for this recipe
	type WeightInfo = pallet_xcm::TestWeightInfo;
	// Runtime types
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
}
