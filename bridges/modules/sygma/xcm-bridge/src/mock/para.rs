// The Licensed Work is (c) 2022 Sygma
// SPDX-License-Identifier: LGPL-3.0-only

use std::marker::PhantomData;
use std::result;

use cumulus_primitives_core::{ChannelStatus, GetChannelInfo, ParaId, Weight};

use frame_support::pallet_prelude::Get;

use crate as sygma_xcm_bridge;
use frame_support::traits::{ConstU16, ConstU64, Nothing};
use frame_support::{
	construct_runtime, parameter_types,
	traits::{AsEnsureOriginWithArg, ConstU128, ConstU32, Everything},
};
use frame_system as system;
use frame_system::EnsureRoot;
use polkadot_parachain_primitives::primitives::Sibling;
use sp_core::{crypto::AccountId32, H256};
use sp_runtime::traits::{IdentityLookup, Zero};
use sygma_traits::AssetTypeIdentifier;
use xcm::latest::{
	AssetId as XcmAssetId, InteriorMultiLocation, MultiAsset, MultiLocation, NetworkId,
	Weight as XCMWeight, XcmContext,
};
use xcm::prelude::{Concrete, Fungible, GlobalConsensus, Parachain, XcmError, X1, X2};
use xcm_builder::{
	AccountId32Aliases, AllowTopLevelPaidExecutionFrom, AllowUnpaidExecutionFrom, CurrencyAdapter,
	FixedWeightBounds, FungiblesAdapter, IsConcrete, NativeAsset, NoChecking, ParentIsPreset,
	RelayChainAsNative, SiblingParachainAsNative, SiblingParachainConvertsVia,
	SignedAccountId32AsNative, SovereignSignedViaLocation, TakeWeightCredit,
};
use xcm_executor::{
	traits::{Error as ExecutionError, MatchesFungibles, WeightTrader, WithOriginFilter},
	Assets as XcmAssets, Config, XcmExecutor,
};

use super::ParachainXcmRouter;

construct_runtime!(
	pub struct Runtime {
		System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
		Balances: pallet_balances::{Pallet, Call, Config<T>, Storage, Event<T>},
		Assets: pallet_assets::{Pallet, Call, Storage, Event<T>},

		ParachainInfo: pallet_parachain_info::{Pallet, Storage, Config<T>},

		XcmpQueue: cumulus_pallet_xcmp_queue::{Pallet, Call, Storage, Event<T>},
		CumulusXcm: cumulus_pallet_xcm::{Pallet, Event<T>, Origin},
		DmpQueue: cumulus_pallet_dmp_queue::{Pallet, Call, Storage, Event<T>},

		SygmaXcmBridge: sygma_xcm_bridge::{Pallet, Event<T>},
	}
);

type Block = frame_system::mocking::MockBlock<Runtime>;

pub(crate) type Balance = u128;

pub type AccountId = AccountId32;

pub type AssetId = u32;

impl frame_system::Config for Runtime {
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = ::sp_runtime::traits::BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = ConstU64<250>;
	type BlockWeights = ();
	type BlockLength = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type DbWeight = ();
	type BaseCallFilter = Everything;
	type SystemWeightInfo = ();
	type SS58Prefix = ConstU16<20>;
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
}

parameter_types! {
	pub const ExistentialDeposit: Balance = 1; // 1 Unit deposit to create asset
	pub const ApprovalDeposit: Balance = 1;
	pub const AssetsStringLimit: u32 = 50;
	pub const MetadataDepositBase: Balance = 1;
	pub const MetadataDepositPerByte: Balance = 1;
}

impl pallet_balances::Config for Runtime {
	type Balance = Balance;
	type DustRemoval = ();
	type RuntimeEvent = RuntimeEvent;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type WeightInfo = ();
	type MaxLocks = ();
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type FreezeIdentifier = ();
	type MaxHolds = ConstU32<1>;
	type MaxFreezes = ConstU32<1>;
	type RuntimeHoldReason = ();
}

impl pallet_assets::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type AssetId = u32;
	type AssetIdParameter = u32;
	type Currency = Balances;
	type CreateOrigin = AsEnsureOriginWithArg<frame_system::EnsureSigned<AccountId>>;
	type ForceOrigin = EnsureRoot<AccountId>;
	type AssetDeposit = ExistentialDeposit;
	type AssetAccountDeposit = ConstU128<10>;
	type MetadataDepositBase = MetadataDepositBase;
	type MetadataDepositPerByte = MetadataDepositPerByte;
	type ApprovalDeposit = ApprovalDeposit;
	type StringLimit = AssetsStringLimit;
	type Freezer = ();
	type Extra = ();
	type CallbackHandle = ();
	type WeightInfo = ();
	type RemoveItemsLimit = ConstU32<5>;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

impl sygma_xcm_bridge::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type AssetReservedChecker = NativeAssetTypeIdentifier<ParachainInfo>;
	type UniversalLocation = UniversalLocation;
	type SelfLocation = SelfLocation;
	type MinXcmFee = MinXcmFee;
}

impl pallet_parachain_info::Config for Runtime {}

pub struct XcmConfig;
impl Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = XcmRouter;
	type AssetTransactor = (CurrencyTransactor, FungiblesTransactor);
	type OriginConverter = XcmOriginToTransactDispatchOrigin;
	type IsReserve = NativeAsset;
	type IsTeleporter = ();
	type UniversalLocation = UniversalLocation;
	type Barrier = Barrier;
	type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
	type Trader = AllTokensAreCreatedEqualToWeight;
	type ResponseHandler = ();
	type AssetTrap = ();
	type AssetClaims = ();
	type SubscriptionService = ();
	type PalletInstancesInfo = AllPalletsWithSystem;
	type MaxAssetsIntoHolding = ConstU32<64>;
	type AssetLocker = ();
	type AssetExchanger = ();
	type FeeManager = ();
	type MessageExporter = ();
	type UniversalAliases = Nothing;
	type CallDispatcher = WithOriginFilter<Everything>;
	type SafeCallFilter = Everything;
	type Aliasers = ();
}

pub type XcmRouter = ParachainXcmRouter<ParachainInfo>;

pub type Barrier = (
	TakeWeightCredit,
	AllowTopLevelPaidExecutionFrom<Everything>,
	AllowUnpaidExecutionFrom<Everything>,
);

pub type LocationToAccountId = (
	ParentIsPreset<AccountId>,
	SiblingParachainConvertsVia<Sibling, AccountId>,
	AccountId32Aliases<RelayNetwork, AccountId>,
);

pub type XcmOriginToTransactDispatchOrigin = (
	SovereignSignedViaLocation<LocationToAccountId, RuntimeOrigin>,
	RelayChainAsNative<RelayChainOrigin, RuntimeOrigin>,
	SiblingParachainAsNative<cumulus_pallet_xcm::Origin, RuntimeOrigin>,
	SignedAccountId32AsNative<RelayNetwork, RuntimeOrigin>,
);

parameter_types! {
	pub const RelayNetwork: NetworkId = NetworkId::Rococo;
	pub RelayChainOrigin: RuntimeOrigin = cumulus_pallet_xcm::Origin::Relay.into();
	pub UnitWeightCost: XCMWeight = 1u64.into();
	pub const MaxInstructions: u32 = 100;
}

parameter_types! {
	pub NativeLocation: MultiLocation = MultiLocation::here();
	pub NativeAssetId: AssetId = 0; // native asset ID is used for token registration on other parachain as foreign asset
	pub PAALocation: MultiLocation = MultiLocation::new(1, X1(Parachain(1u32)));
	pub PBALocation: MultiLocation = MultiLocation::new(1, X1(Parachain(2u32)));
	pub UsdtAssetId: AssetId = 1;
	pub UsdtLocation: MultiLocation = MultiLocation::new(
		1,
		X1(
			Parachain(2005),
		),
	);
	// Parachain A and Parachain B native asset multilocation
	pub CheckingAccount: AccountId32 = AccountId32::new([102u8; 32]);
}

parameter_types! {
	pub SelfLocation: MultiLocation = MultiLocation::new(1, X1(Parachain(ParachainInfo::parachain_id().into())));
	pub UniversalLocation: InteriorMultiLocation = X2(GlobalConsensus(RelayNetwork::get()), Parachain(ParachainInfo::parachain_id().into()));

	// set 1 token as min fee
	pub MinXcmFee: Vec<(XcmAssetId, u128)> = vec![(NativeLocation::get().into(), 1_000_000_000_000u128), (PBALocation::get().into(), 1_000_000_000_000u128), (UsdtLocation::get().into(), 1_000_000u128)];
}

pub struct SimpleForeignAssetConverter(PhantomData<()>);
impl MatchesFungibles<AssetId, Balance> for SimpleForeignAssetConverter {
	fn matches_fungibles(a: &MultiAsset) -> result::Result<(AssetId, Balance), ExecutionError> {
		match (&a.fun, &a.id) {
			(Fungible(ref amount), Concrete(ref id)) => {
				if id == &UsdtLocation::get() {
					Ok((UsdtAssetId::get(), *amount))
				} else if id == &PBALocation::get() || id == &PAALocation::get() {
					Ok((NativeAssetId::get(), *amount))
				} else {
					Err(ExecutionError::AssetNotHandled)
				}
			},
			_ => Err(ExecutionError::AssetNotHandled),
		}
	}
}

pub type CurrencyTransactor = CurrencyAdapter<
	// Use this currency:
	Balances,
	// Use this currency when it is a fungible asset matching the given location or name:
	IsConcrete<NativeLocation>,
	// Convert an XCM MultiLocation into a local account id:
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId32,
	// We don't track any teleports of `Balances`.
	(),
>;

/// Means for transacting assets besides the native currency on this chain.
pub type FungiblesTransactor = FungiblesAdapter<
	// Use this fungibles implementation:
	Assets,
	// Use this currency when it is a fungible asset matching the given location or name:
	SimpleForeignAssetConverter,
	// Convert an XCM MultiLocation into a local account id:
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId32,
	// Disable teleport.
	NoChecking,
	// The account to use for tracking teleports.
	CheckingAccount,
>;

/// NativeAssetTypeIdentifier impl AssetTypeIdentifier for XCMAssetTransactor
/// This impl is only for local mock purpose, the integrated parachain might have their own version
pub struct NativeAssetTypeIdentifier<T>(PhantomData<T>);
impl<T: Get<ParaId>> AssetTypeIdentifier for NativeAssetTypeIdentifier<T> {
	/// check if the given MultiAsset is a native asset
	fn is_native_asset(asset: &MultiAsset) -> bool {
		// currently there are two multilocations are considered as native asset:
		// 1. integrated parachain native asset(MultiLocation::here())
		// 2. other parachain native asset(MultiLocation::new(1, X1(Parachain(T::get().into()))))
		let native_locations =
			[MultiLocation::here(), MultiLocation::new(1, X1(Parachain(T::get().into())))];

		match (&asset.id, &asset.fun) {
			(Concrete(ref id), Fungible(_)) => native_locations.contains(id),
			_ => false,
		}
	}
}

pub struct ChannelInfo;

impl GetChannelInfo for ChannelInfo {
	fn get_channel_status(_id: ParaId) -> ChannelStatus {
		ChannelStatus::Ready(10, 10)
	}
	fn get_channel_max(_id: ParaId) -> Option<usize> {
		Some(usize::max_value())
	}
}

impl cumulus_pallet_xcmp_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type ChannelInfo = ChannelInfo;
	type VersionWrapper = ();
	type ExecuteOverweightOrigin = EnsureRoot<AccountId>;
	type ControllerOrigin = EnsureRoot<AccountId>;
	type ControllerOriginConverter = XcmOriginToTransactDispatchOrigin;
	type PriceForSiblingDelivery = ();
	type WeightInfo = ();
}

impl cumulus_pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmExecutor = XcmExecutor<XcmConfig>;
}

impl cumulus_pallet_dmp_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type ExecuteOverweightOrigin = EnsureRoot<AccountId>;
}

pub struct AllTokensAreCreatedEqualToWeight(MultiLocation);
impl WeightTrader for AllTokensAreCreatedEqualToWeight {
	fn new() -> Self {
		Self(MultiLocation::parent())
	}

	fn buy_weight(
		&mut self,
		weight: Weight,
		payment: XcmAssets,
		_context: &XcmContext,
	) -> Result<XcmAssets, XcmError> {
		let asset_id = payment.fungible.iter().next().expect("Payment must be something; qed").0;
		let required = MultiAsset { id: *asset_id, fun: Fungible(weight.ref_time() as u128) };

		if let MultiAsset { fun: _, id: Concrete(ref id) } = &required {
			self.0 = *id;
		}

		let unused = payment.checked_sub(required).map_err(|_| XcmError::TooExpensive)?;
		Ok(unused)
	}

	fn refund_weight(&mut self, weight: Weight, _context: &XcmContext) -> Option<MultiAsset> {
		if weight.is_zero() {
			None
		} else {
			Some((self.0, weight.ref_time() as u128).into())
		}
	}
}

// Checks events against the latest. A contiguous set of events must be provided. They must
// include the most recent event, but do not have to include every past event.
pub fn assert_events(mut expected: Vec<RuntimeEvent>) {
	let mut actual: Vec<RuntimeEvent> =
		system::Pallet::<Runtime>::events().iter().map(|e| e.event.clone()).collect();

	expected.reverse();

	for evt in expected {
		let next = actual.pop().expect("event expected");
		assert_eq!(next, evt, "Events don't match");
	}
}
