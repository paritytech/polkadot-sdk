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

//! Relay chain XCM configuration

use frame::{
	deps::frame_system,
	runtime::prelude::*,
	traits::{Everything, Nothing},
};
use xcm::v5::prelude::*;
use xcm_builder::{
	AccountId32Aliases, DescribeAllTerminal, DescribeFamily, EnsureXcmOrigin,
	FrameTransactionalProcessor, FungibleAdapter, HashedDescription, IsConcrete,
	SignedToAccountId32,
};
use xcm_executor::XcmExecutor;

use super::{AccountId, Balances, Runtime, RuntimeCall, RuntimeEvent, RuntimeOrigin};

parameter_types! {
	pub HereLocation: Location = Location::here();
	pub ThisNetwork: NetworkId = NetworkId::Polkadot;
}

/// Converter from XCM Locations to accounts.
/// This generates sovereign accounts for Locations and converts
/// local AccountId32 junctions to local accounts.
pub type LocationToAccountId = (
	HashedDescription<AccountId, DescribeFamily<DescribeAllTerminal>>,
	AccountId32Aliases<ThisNetwork, AccountId>,
);

mod asset_transactor {
	use super::*;

	/// AssetTransactor for handling the Relay Chain token.
	pub type FungibleTransactor = FungibleAdapter<
		// Use this `fungible` implementation.
		Balances,
		// This transactor handles the native token.
		IsConcrete<HereLocation>,
		// How to convert an XCM Location into a local account id.
		// Whenever assets are handled, the location is turned into an account.
		// This account is the one where balances are withdrawn/deposited.
		LocationToAccountId,
		// The account id type, needed because `fungible` is generic over it.
		AccountId,
		// Not tracking teleports.
		(),
	>;

	/// All asset transactors, in this case only one
	pub type AssetTransactor = FungibleTransactor;
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
	pub UniversalLocation: InteriorLocation = [GlobalConsensus(NetworkId::Polkadot)].into();
}

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = ();
	type AssetTransactor = asset_transactor::AssetTransactor;
	type OriginConverter = ();
	// We don't need to recognize anyone as a reserve
	type IsReserve = ();
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
	// No one can call `send`
	type SendXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, ()>;
	type XcmRouter = super::super::network::RelayChainXcmRouter; // Provided by xcm-simulator
															  // Anyone can execute XCM programs
	type ExecuteXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	// We execute any type of program
	type XcmExecuteFilter = Everything;
	// How we execute programs
	type XcmExecutor = XcmExecutor<XcmConfig>;
	// We don't allow teleports
	type XcmTeleportFilter = Nothing;
	// We allow all reserve transfers.
	// This is so it can act as a reserve for its native token.
	type XcmReserveTransferFilter = Everything;
	// Same weigher executor uses to weigh XCM programs
	type Weigher = weigher::Weigher;
	// Same universal location
	type UniversalLocation = UniversalLocation;
	// No version discovery needed
	const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 0;
	type AdvertisedXcmVersion = pallet_xcm::CurrentXcmVersion;
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
	type CurrencyMatcher = IsConcrete<HereLocation>;
	// Pallet benchmarks, no need for this example
	type WeightInfo = pallet_xcm::TestWeightInfo;
	// Runtime types
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
}
