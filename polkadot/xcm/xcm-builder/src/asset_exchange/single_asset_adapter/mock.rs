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

//! Mock to test [`SingleAssetExchangeAdapter`].

use core::marker::PhantomData;
use frame_support::{
	assert_ok, construct_runtime, derive_impl, ord_parameter_types, parameter_types,
	traits::{
		fungible::{self, NativeFromLeft, NativeOrWithId},
		fungibles::Mutate,
		tokens::imbalance::ResolveAssetTo,
		AsEnsureOriginWithArg, Equals, Everything, Nothing, OriginTrait, PalletInfoAccess,
	},
	PalletId,
};
use sp_core::{ConstU128, ConstU32, Get};
use sp_runtime::{
	traits::{AccountIdConversion, IdentityLookup, MaybeEquivalence, TryConvert, TryConvertInto},
	BuildStorage, Permill,
};
use xcm::prelude::*;
use xcm_executor::{traits::ConvertLocation, XcmExecutor};

use crate::{FungibleAdapter, IsConcrete, MatchedConvertedConcreteId, StartsWith};

pub type Block = frame_system::mocking::MockBlock<Runtime>;
pub type AccountId = u64;
pub type Balance = u128;

construct_runtime! {
	pub struct Runtime {
		System: frame_system,
		Balances: pallet_balances,
		AssetsPallet: pallet_assets::<Instance1>,
		PoolAssets: pallet_assets::<Instance2>,
		XcmPallet: pallet_xcm,
		AssetConversion: pallet_asset_conversion,
	}
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<AccountId>;
	type AccountData = pallet_balances::AccountData<Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
	type Balance = Balance;
	type AccountStore = System;
	type ExistentialDeposit = ConstU128<1>;
}

pub type TrustBackedAssetsInstance = pallet_assets::Instance1;
pub type PoolAssetsInstance = pallet_assets::Instance2;

#[derive_impl(pallet_assets::config_preludes::TestDefaultConfig)]
impl pallet_assets::Config<TrustBackedAssetsInstance> for Runtime {
	type Currency = Balances;
	type Balance = Balance;
	type AssetDeposit = ConstU128<1>;
	type AssetAccountDeposit = ConstU128<10>;
	type MetadataDepositBase = ConstU128<1>;
	type MetadataDepositPerByte = ConstU128<1>;
	type ApprovalDeposit = ConstU128<1>;
	type CreateOrigin = AsEnsureOriginWithArg<frame_system::EnsureSigned<AccountId>>;
	type ForceOrigin = frame_system::EnsureRoot<AccountId>;
	type Freezer = ();
	type CallbackHandle = ();
}

#[derive_impl(pallet_assets::config_preludes::TestDefaultConfig)]
impl pallet_assets::Config<PoolAssetsInstance> for Runtime {
	type Currency = Balances;
	type Balance = Balance;
	type AssetDeposit = ConstU128<1>;
	type AssetAccountDeposit = ConstU128<10>;
	type MetadataDepositBase = ConstU128<1>;
	type MetadataDepositPerByte = ConstU128<1>;
	type ApprovalDeposit = ConstU128<1>;
	type CreateOrigin = AsEnsureOriginWithArg<frame_system::EnsureSigned<AccountId>>;
	type ForceOrigin = frame_system::EnsureRoot<AccountId>;
	type Freezer = ();
	type CallbackHandle = ();
}

/// Union fungibles implementation for `Assets` and `Balances`.
pub type NativeAndAssets =
	fungible::UnionOf<Balances, AssetsPallet, NativeFromLeft, NativeOrWithId<u32>, AccountId>;

parameter_types! {
	pub const AssetConversionPalletId: PalletId = PalletId(*b"py/ascon");
	pub const Native: NativeOrWithId<u32> = NativeOrWithId::Native;
	pub const LiquidityWithdrawalFee: Permill = Permill::from_percent(0);
}

ord_parameter_types! {
	pub const AssetConversionOrigin: AccountId =
		AccountIdConversion::<AccountId>::into_account_truncating(&AssetConversionPalletId::get());
}

pub type PoolIdToAccountId = pallet_asset_conversion::AccountIdConverter<
	AssetConversionPalletId,
	(NativeOrWithId<u32>, NativeOrWithId<u32>),
>;

impl pallet_asset_conversion::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type HigherPrecisionBalance = sp_core::U256;
	type AssetKind = NativeOrWithId<u32>;
	type Assets = NativeAndAssets;
	type PoolId = (Self::AssetKind, Self::AssetKind);
	type PoolLocator = pallet_asset_conversion::WithFirstAsset<
		Native,
		AccountId,
		Self::AssetKind,
		PoolIdToAccountId,
	>;
	type PoolAssetId = u32;
	type PoolAssets = PoolAssets;
	type PoolSetupFee = ConstU128<100>; // Asset class deposit fees are sufficient to prevent spam
	type PoolSetupFeeAsset = Native;
	type PoolSetupFeeTarget = ResolveAssetTo<AssetConversionOrigin, Self::Assets>;
	type LiquidityWithdrawalFee = LiquidityWithdrawalFee;
	type LPFee = ConstU32<3>;
	type PalletId = AssetConversionPalletId;
	type MaxSwapPathLength = ConstU32<3>;
	type MintMinLiquidity = ConstU128<100>;
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

/// We only alias local accounts.
pub type LocationToAccountId = AccountIndex64Aliases;

parameter_types! {
	pub HereLocation: Location = Here.into_location();
	pub WeightPerInstruction: Weight = Weight::from_parts(1, 1);
	pub MaxInstructions: u32 = 100;
	pub UniversalLocation: InteriorLocation = [GlobalConsensus(Polkadot), Parachain(1000)].into();
	pub TrustBackedAssetsPalletIndex: u8 = <AssetsPallet as PalletInfoAccess>::index() as u8;
	pub TrustBackedAssetsPalletLocation: Location =	PalletInstance(TrustBackedAssetsPalletIndex::get()).into();
}

/// Adapter for the native token.
pub type FungibleTransactor = FungibleAdapter<
	// Use this implementation of the `fungible::*` traits.
	// `Balances` is the name given to the balances pallet
	Balances,
	// This transactor deals with the native token.
	IsConcrete<HereLocation>,
	// How to convert an XCM Location into a local account id.
	// This is also something that's configured in the XCM executor.
	LocationToAccountId,
	// The type for account ids, only needed because `fungible` is generic over it.
	AccountId,
	// Not tracking teleports.
	(),
>;

pub type Weigher = crate::FixedWeightBounds<WeightPerInstruction, RuntimeCall, MaxInstructions>;

pub struct LocationToAssetId;
impl MaybeEquivalence<Location, NativeOrWithId<u32>> for LocationToAssetId {
	fn convert(location: &Location) -> Option<NativeOrWithId<u32>> {
		let pallet_instance = TrustBackedAssetsPalletIndex::get();
		match location.unpack() {
			(0, [PalletInstance(instance), GeneralIndex(index)])
				if *instance == pallet_instance =>
				Some(NativeOrWithId::WithId(*index as u32)),
			(0, []) => Some(NativeOrWithId::Native),
			_ => None,
		}
	}

	fn convert_back(asset_id: &NativeOrWithId<u32>) -> Option<Location> {
		let pallet_instance = TrustBackedAssetsPalletIndex::get();
		Some(match asset_id {
			NativeOrWithId::WithId(id) =>
				Location::new(0, [PalletInstance(pallet_instance), GeneralIndex((*id).into())]),
			NativeOrWithId::Native => Location::new(0, []),
		})
	}
}

pub type PoolAssetsExchanger = crate::SingleAssetExchangeAdapter<
	AssetConversion,
	NativeAndAssets,
	MatchedConvertedConcreteId<
		NativeOrWithId<u32>,
		Balance,
		(StartsWith<TrustBackedAssetsPalletLocation>, Equals<HereLocation>),
		LocationToAssetId,
		TryConvertInto,
	>,
	AccountId,
>;

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = ();
	type AssetTransactor = FungibleTransactor;
	type OriginConverter = ();
	type IsReserve = ();
	type IsTeleporter = ();
	type UniversalLocation = UniversalLocation;
	// This is not safe, you should use `crate::AllowTopLevelPaidExecutionFrom<T>` in a
	// production chain
	type Barrier = crate::AllowUnpaidExecutionFrom<Everything>;
	type Weigher = Weigher;
	type Trader = ();
	type ResponseHandler = ();
	type AssetTrap = ();
	type AssetLocker = ();
	type AssetExchanger = PoolAssetsExchanger;
	type AssetClaims = ();
	type SubscriptionService = ();
	type PalletInstancesInfo = ();
	type FeeManager = ();
	type MaxAssetsIntoHolding = ConstU32<1>;
	type MessageExporter = ();
	type UniversalAliases = Nothing;
	type CallDispatcher = RuntimeCall;
	type SafeCallFilter = Everything;
	type Aliasers = Nothing;
	type TransactionalProcessor = crate::FrameTransactionalProcessor;
	type HrmpNewChannelOpenRequestHandler = ();
	type HrmpChannelAcceptedHandler = ();
	type HrmpChannelClosingHandler = ();
	type XcmRecorder = ();
}

/// Simple converter from a [`Location`] with an [`AccountIndex64`] junction and no parent to a
/// `u64`.
pub struct AccountIndex64Aliases;
impl ConvertLocation<AccountId> for AccountIndex64Aliases {
	fn convert_location(location: &Location) -> Option<AccountId> {
		let index = match location.unpack() {
			(0, [AccountIndex64 { index, network: None }]) => index,
			_ => return None,
		};
		Some((*index).into())
	}
}

/// `Convert` implementation to convert from some a `Signed` (system) `Origin` into an
/// `AccountIndex64`.
///
/// Typically used when configuring `pallet-xcm` in tests to allow `u64` accounts to dispatch an XCM
/// from an `AccountIndex64` origin.
pub struct SignedToAccountIndex64<RuntimeOrigin, AccountId, Network>(
	PhantomData<(RuntimeOrigin, AccountId, Network)>,
);
impl<RuntimeOrigin: OriginTrait + Clone, AccountId: Into<u64>, Network: Get<Option<NetworkId>>>
	TryConvert<RuntimeOrigin, Location> for SignedToAccountIndex64<RuntimeOrigin, AccountId, Network>
where
	RuntimeOrigin::PalletsOrigin: From<frame_system::RawOrigin<AccountId>>
		+ TryInto<frame_system::RawOrigin<AccountId>, Error = RuntimeOrigin::PalletsOrigin>,
{
	fn try_convert(o: RuntimeOrigin) -> Result<Location, RuntimeOrigin> {
		o.try_with_caller(|caller| match caller.try_into() {
			Ok(frame_system::RawOrigin::Signed(who)) =>
				Ok(Junction::AccountIndex64 { network: Network::get(), index: who.into() }.into()),
			Ok(other) => Err(other.into()),
			Err(other) => Err(other),
		})
	}
}

parameter_types! {
	pub const NoNetwork: Option<NetworkId> = None;
}

pub type LocalOriginToLocation = SignedToAccountIndex64<RuntimeOrigin, AccountId, NoNetwork>;

impl pallet_xcm::Config for Runtime {
	// We turn off sending for these tests
	type SendXcmOrigin = crate::EnsureXcmOrigin<RuntimeOrigin, ()>;
	type XcmRouter = ();
	// Anyone can execute XCM programs
	type ExecuteXcmOrigin = crate::EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	// We execute any type of program
	type XcmExecuteFilter = Everything;
	// How we execute programs
	type XcmExecutor = XcmExecutor<XcmConfig>;
	// We don't allow teleports
	type XcmTeleportFilter = Nothing;
	// We don't allow reserve transfers
	type XcmReserveTransferFilter = Nothing;
	// Same weigher executor uses to weigh XCM programs
	type Weigher = Weigher;
	// Same universal location
	type UniversalLocation = UniversalLocation;
	// No version discovery needed
	const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 0;
	type AdvertisedXcmVersion = pallet_xcm::CurrentXcmVersion;
	type AdminOrigin = frame_system::EnsureRoot<AccountId>;
	// No locking
	type TrustedLockers = ();
	type MaxLockers = frame_support::traits::ConstU32<0>;
	type MaxRemoteLockConsumers = frame_support::traits::ConstU32<0>;
	type RemoteLockConsumerIdentifier = ();
	// How to turn locations into accounts
	type SovereignAccountOf = LocationToAccountId;
	// A currency to pay for things and its matcher, we are using the relay token
	type Currency = Balances;
	type CurrencyMatcher = crate::IsConcrete<HereLocation>;
	// Pallet benchmarks, no need for this recipe
	type WeightInfo = pallet_xcm::TestWeightInfo;
	// Runtime types
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
}

pub const INITIAL_BALANCE: Balance = 1_000_000_000;

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

	pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![(0, INITIAL_BALANCE), (1, INITIAL_BALANCE), (2, INITIAL_BALANCE)],
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let owner = 0;

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| {
		System::set_block_number(1);
		assert_ok!(AssetsPallet::force_create(RuntimeOrigin::root(), 1, owner, false, 1,));
		assert_ok!(AssetsPallet::mint_into(1, &owner, INITIAL_BALANCE,));
		assert_ok!(AssetConversion::create_pool(
			RuntimeOrigin::signed(owner),
			Box::new(NativeOrWithId::Native),
			Box::new(NativeOrWithId::WithId(1)),
		));
		assert_ok!(AssetConversion::add_liquidity(
			RuntimeOrigin::signed(owner),
			Box::new(NativeOrWithId::Native),
			Box::new(NativeOrWithId::WithId(1)),
			50_000_000,
			100_000_000,
			0,
			0,
			owner,
		));
	});
	ext
}
