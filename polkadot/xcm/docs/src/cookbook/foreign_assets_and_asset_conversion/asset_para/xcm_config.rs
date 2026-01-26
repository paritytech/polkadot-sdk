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

use super::{
	assets::NativeAndAssets, AccountId, AssetConversion, Balance, Balances, ForeignAssets,
	MessageQueue, PoolAssets, Runtime, RuntimeCall, RuntimeEvent, RuntimeOrigin, XcmPallet,
};
use crate::cookbook::foreign_assets_and_asset_conversion::network::ASSET_PARA_ID;
use assets_common::matching::{FromSiblingParachain, IsForeignConcreteAsset};
use frame::{
	deps::frame_system,
	prelude::imbalance::{ResolveAssetTo, ResolveTo},
	runtime::prelude::*,
	testing_prelude::weights::IdentityFee,
	traits::{Disabled, Equals, Everything, Nothing},
};
use pallet_xcm::XcmPassthrough;
use polkadot_parachain_primitives::primitives::Sibling;
use sp_runtime::traits::TryConvertInto;
use xcm::latest::prelude::*;
use xcm_builder::{
	AccountId32Aliases, AllowTopLevelPaidExecutionFrom, DescribeAllTerminal, DescribeFamily,
	EnsureXcmOrigin, FrameTransactionalProcessor, FungibleAdapter, HashedDescription, IsConcrete,
	SiblingParachainAsNative, SiblingParachainConvertsVia, SignedAccountId32AsNative,
	SignedToAccountId32, SovereignSignedViaLocation, StartsWithExplicitGlobalConsensus,
	TakeWeightCredit, TrailingSetTopicAsId, UsingComponents, XcmFeeManagerFromComponents,
};
use xcm_executor::XcmExecutor;

parameter_types! {
	pub HereLocation: Location = Location::here();
	pub ThisNetwork: NetworkId = NetworkId::Polkadot;
	pub UniversalLocation: InteriorLocation = [GlobalConsensus(NetworkId::Polkadot), Parachain(ASSET_PARA_ID)].into();
	pub UniversalLocationNetworkId: NetworkId = UniversalLocation::get().global_consensus().unwrap();
	pub PoolAssetsPalletLocation: Location =
		PalletInstance(<PoolAssets as PalletInfoAccess>::index() as u8).into();

	pub CheckingAccount: AccountId = XcmPallet::check_account();
	pub TreasuryAccount: AccountId = AccountId::new([9u8; 32]);
}

pub type LocationToAccountId = (
	// Sibling parachain origins convert to AccountId via the `ParaId::into`.
	SiblingParachainConvertsVia<Sibling, AccountId>,
	// Straight up local `AccountId32` origins just alias directly to `AccountId`.
	AccountId32Aliases<ThisNetwork, AccountId>,
	// Foreign locations alias into accounts according to a hash of their standard description.
	HashedDescription<AccountId, DescribeFamily<DescribeAllTerminal>>,
);

/// Configuration related to asset transactors
#[docify::export]
mod asset_transactor {
	use super::*;
	use xcm_builder::{
		FungiblesAdapter, LocalMint, MatchedConvertedConcreteId, NoChecking,
		SingleAssetExchangeAdapter, WithLatestLocationConverter,
	};

	parameter_types! {
		pub HereLocation: Location = Location::here();
	}

	/// AssetTransactor for handling the relay chain token
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

	/// `AssetId`/`Balance` converter for `ForeignAssets`
	pub type ForeignAssetsConvertedConcreteId = assets_common::ForeignAssetsConvertedConcreteId<
		(
			// Ignore assets that start explicitly with our `GlobalConsensus(NetworkId)`, means:
			// - foreign assets from our consensus should be: `Location {parents: 1,
			//   X*(Parachain(xyz), ..)}`
			// - foreign assets outside our consensus with the same `GlobalConsensus(NetworkId)`
			//   won't be accepted here
			StartsWithExplicitGlobalConsensus<UniversalLocationNetworkId>,
		),
		Balance,
		Location,
	>;

	/// Means for transacting foreign assets from different global consensus.
	pub type ForeignFungiblesTransactor = FungiblesAdapter<
		// Use this fungibles' implementation:
		ForeignAssets,
		// Use this currency when it is a fungible asset matching the given location or name:
		ForeignAssetsConvertedConcreteId,
		// Convert an XCM `Location` into a local account ID:
		LocationToAccountId,
		// Our chain's account ID type (we can't get away without mentioning it explicitly):
		AccountId,
		// We don't need to check teleports here.
		NoChecking,
		// The account to use for tracking teleports.
		CheckingAccount,
	>;

	/// `AssetId`/`Balance` converter for `PoolAssets`.
	pub type PoolAssetsConvertedConcreteId =
		assets_common::PoolAssetsConvertedConcreteId<PoolAssetsPalletLocation, Balance>;

	/// Means for transacting asset conversion pool assets on this chain.
	pub type PoolFungiblesTransactor = FungiblesAdapter<
		// Use this fungibles' implementation:
		PoolAssets,
		// Use this currency when it is a fungible asset matching the given location or name:
		PoolAssetsConvertedConcreteId,
		// Convert an XCM `Location` into a local account ID:
		LocationToAccountId,
		// Our chain's account ID type (we can't get away without mentioning it explicitly):
		AccountId,
		// We only want to allow teleports of known assets. We use non-zero issuance as an
		// indication that this asset is known.
		LocalMint<parachains_common::impls::NonZeroIssuance<AccountId, PoolAssets>>,
		// The account to use for tracking teleports.
		CheckingAccount,
	>;

	/// Asset converter for pool assets.
	///
	/// Used to convert one asset to another, when there is a pool available between the two.
	///
	/// This type thus allows paying delivery fees with any asset as long as there is a pool between
	/// said asset and the asset required for fee payment.
	pub type PoolAssetsExchanger = SingleAssetExchangeAdapter<
		AssetConversion,
		NativeAndAssets,
		(
			ForeignAssetsConvertedConcreteId,
			// `ForeignAssetsConvertedConcreteId` doesn't include our native token, so we handle it
			// explicitly here.
			MatchedConvertedConcreteId<
				Location,
				Balance,
				Equals<HereLocation>,
				WithLatestLocationConverter<Location>,
				TryConvertInto,
			>,
		),
		AccountId,
	>;

	/// Actual configuration item that'll be set in the XCM config.
	///
	/// The XCM-Executor will iterate through the individual transactors until one works for
	/// a given XCM instruction.
	pub type AssetTransactors =
		(FungibleTransactor, ForeignFungiblesTransactor, PoolFungiblesTransactor);
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

/// Teleport config for this runtime.
#[docify::export]
mod teleport_config {
	use super::*;

	/// We generally accept teleports from other chains, when they send their native token, i.e.,
	/// when the following is true:
	/// - Sibling parachains' assets from where they originate (as `ForeignCreators`).
	pub type TrustedTeleporters =
		(IsForeignConcreteAsset<FromSiblingParachain<parachain_info::Pallet<Runtime>>>,);
}

/// The trader for this runtime, which can pay with any asset exchangeable to the local asset.
#[docify::export]
mod traders {
	use super::*;

	/// Our trader to charge XCM fees for XCM execution on our chain.
	pub type Traders = (
		// This trader simply allows to pay in the native token.
		UsingComponents<
			IdentityFee<Balance>,
			HereLocation,
			AccountId,
			Balances,
			ResolveTo<TreasuryAccount, Balances>,
		>,
		// This trader allows to pay with any assets exchangeable to this chain's native token with
		// [`AssetConversion`].
		cumulus_primitives_utility::SwapFirstAssetTrader<
			HereLocation,
			AssetConversion,
			// In this example we will just charge 1 for any XCM message.
			FixedFee<1u32, Balance>,
			NativeAndAssets,
			(asset_transactor::ForeignAssetsConvertedConcreteId,),
			ResolveAssetTo<TreasuryAccount, NativeAndAssets>,
			AccountId,
		>,
	);
}

/// This is the type we use to convert an (incoming) XCM origin into a local `Origin` instance,
/// ready for dispatching a transaction with Xcm's `Transact`. There is an `OriginKind` which can
/// biases the kind of local `Origin` it will become.
pub type XcmOriginToTransactDispatchOrigin = (
	// Sovereign account converter; this attempts to derive an `AccountId` from the origin location
	// using `LocationToAccountId` and then turn that into the usual `Signed` origin. Useful for
	// foreign chains who want to have a local sovereign account on this chain which they control.
	SovereignSignedViaLocation<LocationToAccountId, RuntimeOrigin>,
	// Native converter for sibling Parachains; will convert to a `SiblingPara` origin when
	// recognised.
	SiblingParachainAsNative<cumulus_pallet_xcm::Origin, RuntimeOrigin>,
	// Native signed account converter; this just converts an `AccountId32` origin into a normal
	// `Origin::Signed` origin of the same 32-byte value.
	SignedAccountId32AsNative<ThisNetwork, RuntimeOrigin>,
	// Xcm origins can be represented natively under the Xcm pallet's Xcm origin.
	XcmPassthrough<RuntimeOrigin>,
);

pub type Barrier = TrailingSetTopicAsId<(
	// Withdraws funds from a local account for local execution.
	TakeWeightCredit,
	// If the message is one that immediately attempts to pay for execution, then
	// allow it.
	AllowTopLevelPaidExecutionFrom<Everything>,
)>;

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = ();
	type XcmEventEmitter = ();
	type AssetTransactor = asset_transactor::AssetTransactors;
	type OriginConverter = XcmOriginToTransactDispatchOrigin;
	// The declaration of which Locations are reserves for which Assets.
	type IsReserve = ();
	type IsTeleporter = teleport_config::TrustedTeleporters;
	type UniversalLocation = UniversalLocation;
	type Barrier = Barrier;
	type Weigher = weigher::Weigher;
	type Trader = traders::Traders;
	type ResponseHandler = ();
	type AssetTrap = ();
	type AssetLocker = ();
	type AssetExchanger = asset_transactor::PoolAssetsExchanger;
	type AssetClaims = ();
	type SubscriptionService = ();
	type PalletInstancesInfo = ();
	// Fee manager that simply burns the fee.
	type FeeManager = XcmFeeManagerFromComponents<(), ()>;
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
	// We allow teleports in general. The filtering happens through the `TrusterTeleporters`.
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
	type CurrencyMatcher = IsConcrete<HereLocation>;
	// Pallet benchmarks, no need for this recipe
	type WeightInfo = pallet_xcm::TestWeightInfo;
	// Runtime types
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	// Aliasing is disabled: xcm_executor::Config::Aliasers is set to `Nothing`.
	type AuthorizedAliasConsideration = Disabled;
}

impl cumulus_pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmExecutor = XcmExecutor<XcmConfig>;
}
