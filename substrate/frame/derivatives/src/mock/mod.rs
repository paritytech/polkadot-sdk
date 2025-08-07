// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Test environment for `pallet-derivatives`.
//!
//! It contains a simple NFT-like `unique_items` pallet that emulate both NFT collections and their
//! tokens (depending on the pallet instance). This test pallet is instatiated three times in the
//! test environment to cover the usage scenarios of `pallet-derivatives` described in it's crate
//! doc comment.
//!
//! * The first instance, called `PredefinedIdCollections`, emulates NFT collections that are
//!   created with a predefined ID.
//! The ID is set to XCM `AssetId`, so a derivative collection can be created directly using the
//! foreign collection's ID. This pallet instance illustrates and tests the `pallet-derivatives`
//! usage scenario #1 (i.e., when no suitable way of directly creating a derivative collection is
//! provided by the hosting pallet). The configuration of this instance can be found in the
//! [predefined_id_collections] module. The corresponding `pallet-derivatives` instance is called
//! `PredefinedIdDerivativeCollections`.
//!
//! * The second instance, called `AutoIdCollections`, emulates NFT collections that are created
//!   with an automatically assigned ID (e.g., an incremental one).
//! The ID is set to `u64`, so a mapping between the foreign collection's ID and the derivative
//! collection ID is needed. This pallet instance illustrates and tests the `pallet-derivatives`
//! usage scenario #2 combined with scenario #1 (since we also test manual collection creation and
//! destruction). The configuration of this instance can be found in the [auto_id_collections]
//! module. The corresponding `pallet-derivatives` instance is called `AutoIdDerivativeCollections`.
//!
//! * The third instance, called `PredefinedIdNfts`, emulates non-fungible tokens within collections
//!   from the pallet's second instance.
//! The full NFT ID is a tuple consisting of the collection ID and a token ID, both of which are of
//! the `u64` type. Since a foreign NFT is identified by `(AssetId, AssetInstance)`, we need the
//! mapping between it and the derivative NFT ID. This pallet instance illustrates and tests the
//! `pallet-derivatives` usage scenario #2 without scenario #1 (the manual creation and destruction
//! of derivative NFTs is forbidden). The configuration of this instance can be found in the
//! [auto_id_nfts] module. The corresponding `pallet-derivatives` instance is called
//! `DerivativeNfts`. Additionally, there is an example of asset transactors dealing with derivative
//! NFTs implemented using `pallet-derivatives`, which includes original-to-derivative ID mapping.

use super::*;
use crate as pallet_derivatives;

use frame_support::{
	construct_runtime, derive_impl, parameter_types,
	traits::{
		tokens::asset_ops::{common_ops::*, common_strategies::*, *},
		ContainsPair, Everything, Nothing, PalletInfoAccess,
	},
};
use frame_system::{EnsureNever, EnsureRoot, EnsureSigned};
use sp_runtime::{traits::TryConvertInto, BuildStorage};
use xcm::prelude::*;
use xcm_builder::{
	unique_instances::{
		ExtractAssetId, NonFungibleAsset, UniqueInstancesAdapter, UniqueInstancesDepositAdapter,
	},
	AllowUnpaidExecutionFrom, AsPrefixedGeneralIndex, FixedWeightBounds, MatchInClassInstances,
	MatchedConvertedConcreteId, StartsWith,
};
use xcm_executor::traits::ConvertLocation;

mod auto_id_collections;
mod auto_id_nfts;
mod predefined_id_collections;

pub use auto_id_collections::*;
pub use auto_id_nfts::*;
pub use predefined_id_collections::*;

type AccountId = u64;
type Block = frame_system::mocking::MockBlock<Test>;
type Balance = u64;

#[frame_support::pallet]
pub mod unique_items {
	use frame_support::pallet_prelude::*;

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		type ItemId: Member + Parameter + MaxEncodedLen + TypeInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::error]
	pub enum Error<T, I = ()> {
		AlreadyExists,
		NoPermission,
		UnknownItem,
	}

	#[pallet::event]
	pub enum Event<T: Config<I>, I: 'static = ()> {}

	#[pallet::storage]
	pub type CurrentItemId<T: Config<I>, I: 'static = ()> = StorageValue<_, T::ItemId, OptionQuery>;

	#[pallet::storage]
	pub type ItemOwner<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Blake2_128Concat, T::ItemId, T::AccountId, OptionQuery>;
}

construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,

		PredefinedIdCollections: unique_items::<Instance1>,
		PredefinedIdDerivativeCollections: pallet_derivatives::<Instance1>,

		AutoIdCollections: unique_items::<Instance2>,
		AutoIdDerivativeCollections: pallet_derivatives::<Instance2>,

		PredefinedIdNfts: unique_items::<Instance3>,
		DerivativeNfts: pallet_derivatives::<Instance3>,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type AccountId = AccountId;
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
}

// XCM configuration parameters.
parameter_types! {
	pub const DeliveryFees: u128 = 20; // Random value.
	pub const ExistentialDeposit: u128 = 1; // Random value.
	pub const BaseXcmWeight: Weight = Weight::from_parts(100, 10); // Random value.
	pub UniversalLocation: InteriorLocation = [GlobalConsensus(NetworkId::ByGenesis([0; 32])), Parachain(2000)].into();
	pub const HereLocation: Location = Location::here();
	pub const RelayLocation: Location = Location::parent();
	pub const MaxAssetsIntoHolding: u32 = 64;
	pub const AnyNetwork: Option<NetworkId> = None;
	pub LocalNftsPalletLocation: Location = PalletInstance(<PredefinedIdNfts as PalletInfoAccess>::index() as u8).into();
	pub StashAccountId: AccountId = u64::MAX;
}

/// It is an `IsReserve` that returns true only if the given asset's location begins with a sibling
/// parachain junction which equals the given origin.
pub struct TrustAssetsFromSiblings;
impl ContainsPair<Asset, Location> for TrustAssetsFromSiblings {
	fn contains(asset: &Asset, origin: &Location) -> bool {
		let AssetId(asset_location) = &asset.id;

		match (asset_location.unpack(), origin.unpack()) {
			((1, [Parachain(asset_para_id), ..]), (1, [Parachain(origin_para_id)]))
				if asset_para_id == origin_para_id =>
				true,
			_ => false,
		}
	}
}

/// Converts locations that are only the `AccountIndex64` junction into local u64 accounts.
pub struct AccountIndex64Aliases<Network, AccountId>(PhantomData<(Network, AccountId)>);
impl<Network: Get<Option<NetworkId>>, AccountId: From<u64>> ConvertLocation<AccountId>
	for AccountIndex64Aliases<Network, AccountId>
{
	fn convert_location(location: &Location) -> Option<AccountId> {
		let index = match location.unpack() {
			(0, [AccountIndex64 { index, network: None }]) => index,
			(0, [AccountIndex64 { index, network }]) if *network == Network::get() => index,
			_ => return None,
		};
		Some((*index).into())
	}
}

/// Custom location converter to turn sibling chains into u64 accounts.
pub struct SiblingChainToIndex64;
impl ConvertLocation<AccountId> for SiblingChainToIndex64 {
	fn convert_location(location: &Location) -> Option<AccountId> {
		let index = match location.unpack() {
			(1, [Parachain(id)]) => id,
			_ => return None,
		};
		Some((*index).into())
	}
}

/// XCM executor's Location-to-AccountId converter.
pub type LocationToAccountId = (AccountIndex64Aliases<AnyNetwork, u64>, SiblingChainToIndex64);

/// Converts an asset's location to the corresponding sibling parachain sovereign account.
pub struct SiblingAssetToReserveLocationConvert;
impl ConvertLocation<AccountId> for SiblingAssetToReserveLocationConvert {
	fn convert_location(location: &Location) -> Option<AccountId> {
		match location.unpack() {
			(1, [Parachain(para_id), ..]) =>
				LocationToAccountId::convert_location(&Location::new(1, Parachain(*para_id))),
			_ => None,
		}
	}
}

/// Creates a new derivative using the provided `IdAssignment` (can be a predefined ID or a derived
/// one) and `CreateOp`. The `IdAssignment` must take XCM `AssetId` as a parameter.
///
/// The derivative will be created using the `WithConfig` strategy with the owner account set to the
/// original asset's reserve location.
///
/// The `InvalidAssetErr` is the error that must be returned if getting the reserve location's
/// sovereign account fails.
pub type CreateDerivativeOwnedBySovAcc<IdAssignment, CreateOp, InvalidAssetErr> =
	DeriveStrategyThenCreate<
		// Derived strategy: assign the derivative owner.
		WithConfig<ConfigValue<Owner<AccountId>>, IdAssignment>,
		// Converts the provided XCM `AssetId` (original ID) to the reserve location's sovereign
		// account, and returns the specified strategy above.
		OwnerConvertedLocation<SiblingAssetToReserveLocationConvert, IdAssignment, InvalidAssetErr>,
		CreateOp,
	>;

/// The `pallet-derivatives` instance corresponding to the `PredefinedIdCollections` instance of the
/// `unique_items` mock pallet.
pub type PredefinedIdDerivativeCollectionsInstance = pallet_derivatives::Instance1;
impl pallet_derivatives::Config<PredefinedIdDerivativeCollectionsInstance> for Test {
	type WeightInfo = pallet_derivatives::TestWeightInfo;

	type Original = AssetId;
	type Derivative = AssetId;

	type DerivativeExtra = ();

	type CreateOrigin = EnsureSigned<AccountId>;

	// `NoStoredMapping` tells the pallet not to store the mapping between the `Original` and the
	// `Derivative`
	type CreateOp = pallet_derivatives::NoStoredMapping<
		CreateDerivativeOwnedBySovAcc<
			PredefinedId<AssetId>,
			PredefinedIdCollections,
			pallet_derivatives::InvalidAssetError<PredefinedIdDerivativeCollections>,
		>,
	>;

	type DestroyOrigin = EnsureRoot<AccountId>;
	type DestroyOp = PredefinedIdCollections;
}

/// The `pallet-derivatives` instance corresponding to the `AutoIdCollections` instance of the
/// `unique_items` mock pallet.
pub type AutoIdDerivativeCollectionsInstance = pallet_derivatives::Instance2;
impl pallet_derivatives::Config<AutoIdDerivativeCollectionsInstance> for Test {
	type WeightInfo = pallet_derivatives::TestWeightInfo;

	type Original = AssetId;
	type Derivative = CollectionAutoId;

	// The current in-collection derivative token ID to use when creating a new derivative NFT.
	type DerivativeExtra = NftLocalId;

	type CreateOrigin = EnsureSigned<AccountId>;

	// `StoreMapping` tells the pallet to store the mapping between the `Original` and the
	// `Derivative`
	type CreateOp = pallet_derivatives::StoreMapping<
		CreateDerivativeOwnedBySovAcc<
			AutoId<Self::Derivative>,
			AutoIdCollections,
			pallet_derivatives::InvalidAssetError<AutoIdDerivativeCollections>,
		>,
	>;

	type DestroyOrigin = EnsureRoot<AccountId>;

	// The `AutoIdCollections` uses `Derivative` as an ID type.
	// But the `destroy_derivative` extrinsic takes the ID parameter of type `Original`.
	//
	// We use `MapId` here to map the `Original` value to the corresponding `Derivative` one
	// using the `OriginalToDerivativeConvert`, which uses the stored mapping between them.
	type DestroyOp = MapId<
		Self::Original,
		Self::Derivative,
		OriginalToDerivativeConvert<AutoIdDerivativeCollections>,
		AutoIdCollections,
	>;
}

/// The `pallet-derivatives` instance corresponding to the `PredefinedIdNfts` instance of the
/// `unique_items` mock pallet.
pub type DerivativeNftsInstance = pallet_derivatives::Instance3;
impl pallet_derivatives::Config<DerivativeNftsInstance> for Test {
	type WeightInfo = pallet_derivatives::TestWeightInfo;

	type Original = NonFungibleAsset;
	type Derivative = NftFullId;

	type DerivativeExtra = ();

	// The derivative NFTs can't be manually created.
	type CreateOrigin = EnsureNever<AccountId>;
	type CreateOp = DisabledOps<Self::Original>;

	// The derivative NFTs can't be manually destroyed.
	type DestroyOrigin = EnsureNever<AccountId>;
	type DestroyOp = DisabledOps<Self::Original>;
}

/// Matches NFTs within the `PredefinedIdNfts` pallet.
/// These NFTs are considered "local" since they are minted on this chain.
pub type LocalNftsMatcher = MatchInClassInstances<
	MatchedConvertedConcreteId<
		CollectionAutoId,
		NftLocalId,
		StartsWith<LocalNftsPalletLocation>,
		AsPrefixedGeneralIndex<LocalNftsPalletLocation, CollectionAutoId, TryConvertInto>,
		TryConvertInto,
	>,
>;

/// This asset transactor deals with local NFTs that aren't derivatives.
///
/// The derivative NFTs are excluded to avoid acting like a reserve chain for assets that came from
/// other chains.
pub type LocalNftsTransactor = UniqueInstancesAdapter<
	AccountId,
	LocationToAccountId,
	// The `EnsureNotDerivativeInstance` uses the `DerivativeNfts` stored mapping
	// to prevent derivative NFTs from being matched.
	EnsureNotDerivativeInstance<DerivativeNfts, LocalNftsMatcher>,
	// The `StashAccountAssetOps` adds the `Stash` and `Restore` operations
	// to the `PredefinedIdNfts` by utilizing the NFT transfer to and from the `StashAccountId`
	// correspondingly.
	StashAccountAssetOps<StashAccountId, PredefinedIdNfts>,
>;

/// This asset transactor deals with already registered derivative NFTs.
pub type RegisteredDerivativeNftsTransactor = UniqueInstancesAdapter<
	AccountId,
	LocationToAccountId,
	// Matches derivative NFTs using the `DerivativeNfts` stored mapping.
	MatchDerivativeInstances<DerivativeNfts>,
	// The `StashAccountAssetOps` adds the `Stash` and `Restore` operations
	// to the `PredefinedIdNfts` utilizing the NFT transfer to and from the `StashAccountId`
	// correspondingly.
	StashAccountAssetOps<StashAccountId, PredefinedIdNfts>,
>;

/// Takes `(AssetId, AssetInstance)` to create a new NFT while the underlying `CreateOp` accepts
/// only the `AssetId`.
///
/// Extracts `AssetId` from the tuple and passes it to `CreateOp`.
pub type CreateUsingXcmNft<CreateOp> = MapId<NonFungibleAsset, AssetId, ExtractAssetId, CreateOp>;

/// Takes `AssetId` to create a new NFT while the underlying `CreateOp` accepts `CollectionAutoId`.
///
/// Maps the provided `AssetId` to `CollectionAutoId` using the stored mapping in the
/// `AutoIdDerivativeCollections`. Passes the mapped ID to `CreateOp`.
pub type CreateUsingXcmAssetId<CreateOp> = MapId<
	AssetId,
	CollectionAutoId,
	OriginalToDerivativeConvert<AutoIdDerivativeCollections>,
	CreateOp,
>;

/// * Takes `CollectionAutoId` (the NFT derivative collection ID)
/// * Gets the associated extra data (current in-collection token ID) using the
///   `AutoIdDerivativeCollections`,
/// * Makes the `FullNftId` using the collection ID and current token ID,
/// * Creates a derivative NFT using the `FullNftId`
/// * Increments the token ID and sets it as the derivative collection's extra data.
pub type DeriveNft = ConcatIncrementalExtra<
	CollectionAutoId,
	NftLocalId,
	AutoIdDerivativeCollections,
	PredefinedIdNfts,
>;

/// Create a derivative NFT using `(AssetId, AssetInstance)`.
pub type CreateDerivativeNft = CreateUsingXcmNft<CreateUsingXcmAssetId<DeriveNft>>;

/// This asset transactor registers a new derivative NFT.
pub type DerivativeNftsRegistrar = UniqueInstancesDepositAdapter<
	AccountId,
	LocationToAccountId,
	NftFullId,
	// Creates a new NFT using the `CreateDerivativeNft`,
	// and stores the mapping between the XCM NFT ID and the derivative ID in the `DerivativeNfts`
	// registry.
	RegisterDerivative<DerivativeNfts, CreateDerivativeNft>,
>;

pub type AssetTransactors =
	(LocalNftsTransactor, RegisteredDerivativeNftsTransactor, DerivativeNftsRegistrar);

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = ();
	type XcmEventEmitter = ();
	type AssetTransactor = AssetTransactors;
	type OriginConverter = ();
	type IsReserve = TrustAssetsFromSiblings;
	type IsTeleporter = ();
	type UniversalLocation = UniversalLocation;
	type Barrier = AllowUnpaidExecutionFrom<Everything>;
	type Weigher = FixedWeightBounds<BaseXcmWeight, RuntimeCall, ConstU32<100>>;
	type Trader = ();
	type ResponseHandler = ();
	type AssetTrap = ();
	type AssetLocker = ();
	type AssetExchanger = ();
	type AssetClaims = ();
	type SubscriptionService = ();
	type PalletInstancesInfo = AllPalletsWithSystem;
	type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
	type FeeManager = ();
	type MessageExporter = ();
	type UniversalAliases = ();
	type CallDispatcher = RuntimeCall;
	type SafeCallFilter = Nothing;
	type Aliasers = Nothing;
	type TransactionalProcessor = ();
	type HrmpNewChannelOpenRequestHandler = ();
	type HrmpChannelAcceptedHandler = ();
	type HrmpChannelClosingHandler = ();
	type XcmRecorder = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	sp_io::TestExternalities::new(t)
}
