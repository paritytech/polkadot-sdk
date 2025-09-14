// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
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

use super::{AccountId, Balances, MessageQueue, Runtime, RuntimeCall, RuntimeEvent, RuntimeOrigin};
use crate::cookbook::foreign_assets_and_asset_conversion::network::{
	ASSET_PARA_ID, SIMPLE_PARA_ID,
};
use frame::{
	deps::frame_system,
	runtime::prelude::*,
	traits::{Disabled, Everything, Nothing},
};
use xcm::latest::prelude::*;
use xcm_builder::{
	AccountId32Aliases, Case, DescribeAllTerminal, DescribeFamily, EnsureXcmOrigin,
	FrameTransactionalProcessor, FungibleAdapter, HashedDescription, IsConcrete,
	SignedToAccountId32,
};
use xcm_executor::XcmExecutor;

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

	parameter_types! {
		pub HereLocation: Location = Location::here();
	}

	/// AssetTransactor for handling our chain's token
	pub type FungibleTransactor = FungibleAdapter<
		// Use this implementation of the `fungible::*` traits.
		// `Balances` is the name given to the balances pallet in this particular recipe.
		// Any implementation of the traits would suffice.
		Balances,
		// This transactor deals with our native token.
		IsConcrete<HereLocation>,
		// How to convert an XCM Location into a local account id.
		// This is also something that's configured in the XCM executor.
		LocationToAccountId,
		// The type for account ids, only needed because `fungible` is generic over it.
		AccountId,
		// Not tracking teleports.
		// This recipe only uses reserve asset transfers to handle the Relay Chain token.
		(),
	>;

	/// Actual configuration item that'll be set in the XCM config.
	/// A tuple could be used here to have multiple transactors, each (potentially) handling
	/// different assets.
	/// In this recipe, we only have one.
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
	pub UniversalLocation: InteriorLocation = [GlobalConsensus(NetworkId::Polkadot), Parachain(SIMPLE_PARA_ID)].into();

	pub AssetParaLocation: Location = Location::new(1, [Parachain(ASSET_PARA_ID)]);
	pub const SimpleParaNative: AssetFilter = Wild(AllOf { fun: WildFungible, id: AssetId(Location::here()) });
	pub AssetParaTrustedTeleporter: (AssetFilter, Location) = (SimpleParaNative::get(), AssetParaLocation::get());
}

/// Teleport config to send our asset to the Asset Para.
#[docify::export]
mod teleport_config {
	use super::*;

	parameter_types! {
		pub AssetParaLocation: Location = Location::new(1, [Parachain(ASSET_PARA_ID)]);
		pub const SimpleParaNative: AssetFilter = Wild(AllOf { fun: WildFungible, id: AssetId(Location::here()) });

		/// The Asset Para is a trusted teleporter for our native token.
		pub AssetParaTrustedTeleporter: (AssetFilter, Location) = (SimpleParaNative::get(), AssetParaLocation::get());
	}

	/// All our trusted teleporter Cases. In this example it is only one.
	pub type TrustedTeleporters = (Case<AssetParaTrustedTeleporter>,);
}

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = ();
	type XcmEventEmitter = ();
	type AssetTransactor = asset_transactor::AssetTransactor;
	type OriginConverter = ();
	// The declaration of which Locations are reserves for which Assets.
	type IsReserve = ();
	// Trusted teleport destinations
	type IsTeleporter = teleport_config::TrustedTeleporters;
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

/// Converts a local signed origin into an XCM location. Forms the basis for local origins
/// sending/executing XCMs.
pub type LocalOriginToLocation = SignedToAccountId32<RuntimeOrigin, AccountId, ThisNetwork>;

impl pallet_xcm::Config for Runtime {
	// We turn off sending for these tests
	type SendXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, ()>;
	// Provided by xcm-simulator
	type XcmRouter = super::super::network::ParachainXcmRouter<MessageQueue>;
	// Anyone can execute XCM programs
	type ExecuteXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	// We execute any type of program
	type XcmExecuteFilter = Everything;
	// How we execute programs
	type XcmExecutor = XcmExecutor<XcmConfig>;
	// We only allow teleporting our own asset.
	type XcmTeleportFilter = Everything;
	// We allow all reserve transfers
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
	type CurrencyMatcher = IsConcrete<RelayLocation>;
	// Pallet benchmarks, no need for this recipe
	type WeightInfo = pallet_xcm::TestWeightInfo;
	// Runtime types
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	// Aliasing is disabled: xcm_executor::Config::Aliasers is set to `Nothing`.
	type AuthorizedAliasConsideration = Disabled;
}
