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

//! # Asset Hub Westend Runtime
//!
//! Testnet for Asset Hub Polkadot.

#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "256"]

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

mod weights;
pub mod xcm_config;

use crate::xcm_config::{
	LocalAndForeignAssetsMultiLocationMatcher, TrustBackedAssetsPalletLocation,
};
use assets_common::{
	local_and_foreign_assets::{LocalAndForeignAssets, MultiLocationConverter},
	AssetIdForTrustBackedAssetsConvert,
};
use codec::{Decode, Encode, MaxEncodedLen};
use cumulus_pallet_parachain_system::RelayNumberStrictlyIncreases;
use frame_support::{
	construct_runtime,
	dispatch::DispatchClass,
	ord_parameter_types, parameter_types,
	traits::{
		tokens::nonfungibles_v2::Inspect, AsEnsureOriginWithArg, ConstBool, ConstU128, ConstU32,
		ConstU64, ConstU8, InstanceFilter,
	},
	weights::{ConstantMultiplier, Weight},
	BoundedVec, PalletId,
};
use frame_system::{
	limits::{BlockLength, BlockWeights},
	EnsureRoot, EnsureSigned, EnsureSignedBy,
};
use pallet_asset_conversion_tx_payment::AssetConversionAdapter;
use pallet_nfts::PalletFeatures;
pub use parachains_common as common;
use parachains_common::{
	impls::DealWithFees,
	westend::{consensus::*, currency::*, fee::WeightToFee},
	AccountId, AssetIdForTrustBackedAssets, AuraId, Balance, BlockNumber, Hash, Header, Nonce,
	Signature, AVERAGE_ON_INITIALIZE_RATIO, DAYS, HOURS, MAXIMUM_BLOCK_WEIGHT,
	NORMAL_DISPATCH_RATIO, SLOT_DURATION,
};
use sp_api::impl_runtime_apis;
use sp_core::{crypto::KeyTypeId, OpaqueMetadata};
use sp_runtime::{
	create_runtime_str, generic, impl_opaque_keys,
	traits::{AccountIdConversion, AccountIdLookup, BlakeTwo256, Block as BlockT, Verify},
	transaction_validity::{TransactionSource, TransactionValidity},
	ApplyExtrinsicResult, Permill, RuntimeDebug,
};
use sp_std::prelude::*;
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;
use xcm::opaque::v3::MultiLocation;
use xcm_config::{
	ForeignAssetsConvertedConcreteId, PoolAssetsConvertedConcreteId,
	TrustBackedAssetsConvertedConcreteId, WestendLocation, XcmConfig,
	XcmOriginToTransactDispatchOrigin,
};

#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;

use assets_common::{
	foreign_creators::ForeignCreators, matching::FromSiblingParachain, MultiLocationForAssetId,
};
use polkadot_runtime_common::{BlockHashCount, SlowAdjustingFeeUpdate};
use xcm_executor::XcmExecutor;

use crate::xcm_config::ForeignCreatorsSovereignAccountOf;
use weights::{BlockExecutionWeight, ExtrinsicBaseWeight, RocksDbWeight};

impl_opaque_keys! {
	pub struct SessionKeys {
		pub aura: Aura,
	}
}

#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
	// Note: "westmint" is the legacy name for this chain. It has been renamed to
	// "asset-hub-westend". Many wallets/tools depend on the `spec_name`, so it remains "westmint"
	// for the time being. Wallets/tools should update to treat "asset-hub-westend" equally.
	spec_name: create_runtime_str!("westmint"),
	impl_name: create_runtime_str!("westmint"),
	authoring_version: 1,
	spec_version: 10000,
	impl_version: 0,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 13,
	state_version: 0,
};

/// The version information used to identify this runtime when compiled natively.
#[cfg(feature = "std")]
pub fn native_version() -> NativeVersion {
	NativeVersion { runtime_version: VERSION, can_author_with: Default::default() }
}

parameter_types! {
	pub const Version: RuntimeVersion = VERSION;
	pub RuntimeBlockLength: BlockLength =
		BlockLength::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
	pub RuntimeBlockWeights: BlockWeights = BlockWeights::builder()
		.base_block(BlockExecutionWeight::get())
		.for_class(DispatchClass::all(), |weights| {
			weights.base_extrinsic = ExtrinsicBaseWeight::get();
		})
		.for_class(DispatchClass::Normal, |weights| {
			weights.max_total = Some(NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT);
		})
		.for_class(DispatchClass::Operational, |weights| {
			weights.max_total = Some(MAXIMUM_BLOCK_WEIGHT);
			// Operational transactions have some extra reserved space, so that they
			// are included even if block reached `MAXIMUM_BLOCK_WEIGHT`.
			weights.reserved = Some(
				MAXIMUM_BLOCK_WEIGHT - NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT
			);
		})
		.avg_block_initialization(AVERAGE_ON_INITIALIZE_RATIO)
		.build_or_panic();
	pub const SS58Prefix: u8 = 42;
}

// Configure FRAME pallets to include in runtime.
impl frame_system::Config for Runtime {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = RuntimeBlockWeights;
	type BlockLength = RuntimeBlockLength;
	type AccountId = AccountId;
	type RuntimeCall = RuntimeCall;
	type Lookup = AccountIdLookup<AccountId, ()>;
	type Nonce = Nonce;
	type Hash = Hash;
	type Hashing = BlakeTwo256;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type RuntimeOrigin = RuntimeOrigin;
	type BlockHashCount = BlockHashCount;
	type DbWeight = RocksDbWeight;
	type Version = Version;
	type PalletInfo = PalletInfo;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type AccountData = pallet_balances::AccountData<Balance>;
	type SystemWeightInfo = weights::frame_system::WeightInfo<Runtime>;
	type SS58Prefix = SS58Prefix;
	type OnSetCode = cumulus_pallet_parachain_system::ParachainSetCode<Self>;
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

impl pallet_timestamp::Config for Runtime {
	/// A timestamp: milliseconds since the unix epoch.
	type Moment = u64;
	type OnTimestampSet = Aura;
	type MinimumPeriod = ConstU64<{ SLOT_DURATION / 2 }>;
	type WeightInfo = weights::pallet_timestamp::WeightInfo<Runtime>;
}

impl pallet_authorship::Config for Runtime {
	type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Aura>;
	type EventHandler = (CollatorSelection,);
}

parameter_types! {
	pub const ExistentialDeposit: Balance = EXISTENTIAL_DEPOSIT;
}

impl pallet_balances::Config for Runtime {
	type MaxLocks = ConstU32<50>;
	/// The type for recording an account's balance.
	type Balance = Balance;
	/// The ubiquitous event type.
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type WeightInfo = weights::pallet_balances::WeightInfo<Runtime>;
	type MaxReserves = ConstU32<50>;
	type ReserveIdentifier = [u8; 8];
	type RuntimeHoldReason = RuntimeHoldReason;
	type FreezeIdentifier = ();
	// We allow each account to have holds on it from:
	//   - `NftFractionalization`: 1
	type MaxHolds = ConstU32<1>;
	type MaxFreezes = ConstU32<0>;
}

parameter_types! {
	/// Relay Chain `TransactionByteFee` / 10
	pub const TransactionByteFee: Balance = MILLICENTS;
}

impl pallet_transaction_payment::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OnChargeTransaction =
		pallet_transaction_payment::CurrencyAdapter<Balances, DealWithFees<Runtime>>;
	type WeightToFee = WeightToFee;
	type LengthToFee = ConstantMultiplier<Balance, TransactionByteFee>;
	type FeeMultiplierUpdate = SlowAdjustingFeeUpdate<Self>;
	type OperationalFeeMultiplier = ConstU8<5>;
}

parameter_types! {
	pub const AssetDeposit: Balance = UNITS / 10; // 1 / 10 WND deposit to create asset
	pub const AssetAccountDeposit: Balance = deposit(1, 16);
	pub const ApprovalDeposit: Balance = EXISTENTIAL_DEPOSIT;
	pub const AssetsStringLimit: u32 = 50;
	/// Key = 32 bytes, Value = 36 bytes (32+1+1+1+1)
	// https://github.com/paritytech/substrate/blob/069917b/frame/assets/src/lib.rs#L257L271
	pub const MetadataDepositBase: Balance = deposit(1, 68);
	pub const MetadataDepositPerByte: Balance = deposit(0, 1);
}

pub type AssetsForceOrigin = EnsureRoot<AccountId>;

// Called "Trust Backed" assets because these are generally registered by some account, and users of
// the asset assume it has some claimed backing. The pallet is called `Assets` in
// `construct_runtime` to avoid breaking changes on storage reads.
pub type TrustBackedAssetsInstance = pallet_assets::Instance1;
type TrustBackedAssetsCall = pallet_assets::Call<Runtime, TrustBackedAssetsInstance>;
impl pallet_assets::Config<TrustBackedAssetsInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type AssetId = AssetIdForTrustBackedAssets;
	type AssetIdParameter = codec::Compact<AssetIdForTrustBackedAssets>;
	type Currency = Balances;
	type CreateOrigin = AsEnsureOriginWithArg<EnsureSigned<AccountId>>;
	type ForceOrigin = AssetsForceOrigin;
	type AssetDeposit = AssetDeposit;
	type MetadataDepositBase = MetadataDepositBase;
	type MetadataDepositPerByte = MetadataDepositPerByte;
	type ApprovalDeposit = ApprovalDeposit;
	type StringLimit = AssetsStringLimit;
	type Freezer = ();
	type Extra = ();
	type WeightInfo = weights::pallet_assets_local::WeightInfo<Runtime>;
	type CallbackHandle = ();
	type AssetAccountDeposit = AssetAccountDeposit;
	type RemoveItemsLimit = ConstU32<1000>;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

parameter_types! {
	pub const AssetConversionPalletId: PalletId = PalletId(*b"py/ascon");
	pub const AllowMultiAssetPools: bool = false;
	// should be non-zero if AllowMultiAssetPools is true, otherwise can be zero
	pub const LiquidityWithdrawalFee: Permill = Permill::from_percent(0);
}

ord_parameter_types! {
	pub const AssetConversionOrigin: sp_runtime::AccountId32 =
		AccountIdConversion::<sp_runtime::AccountId32>::into_account_truncating(&AssetConversionPalletId::get());
}

pub type PoolAssetsInstance = pallet_assets::Instance3;
impl pallet_assets::Config<PoolAssetsInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type RemoveItemsLimit = ConstU32<1000>;
	type AssetId = u32;
	type AssetIdParameter = u32;
	type Currency = Balances;
	type CreateOrigin =
		AsEnsureOriginWithArg<EnsureSignedBy<AssetConversionOrigin, sp_runtime::AccountId32>>;
	type ForceOrigin = AssetsForceOrigin;
	type AssetDeposit = ConstU128<0>;
	type AssetAccountDeposit = ConstU128<0>;
	type MetadataDepositBase = ConstU128<0>;
	type MetadataDepositPerByte = ConstU128<0>;
	type ApprovalDeposit = ConstU128<0>;
	type StringLimit = ConstU32<50>;
	type Freezer = ();
	type Extra = ();
	type WeightInfo = weights::pallet_assets_pool::WeightInfo<Runtime>;
	type CallbackHandle = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

impl pallet_asset_conversion::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type HigherPrecisionBalance = sp_core::U256;
	type Currency = Balances;
	type AssetBalance = Balance;
	type AssetId = MultiLocation;
	type Assets = LocalAndForeignAssets<
		Assets,
		AssetIdForTrustBackedAssetsConvert<TrustBackedAssetsPalletLocation>,
		ForeignAssets,
	>;
	type PoolAssets = PoolAssets;
	type PoolAssetId = u32;
	type PoolSetupFee = ConstU128<0>; // Asset class deposit fees are sufficient to prevent spam
	type PoolSetupFeeReceiver = AssetConversionOrigin;
	type LiquidityWithdrawalFee = LiquidityWithdrawalFee; // should be non-zero if AllowMultiAssetPools is true, otherwise can be zero.
	type LPFee = ConstU32<3>;
	type PalletId = AssetConversionPalletId;
	type AllowMultiAssetPools = AllowMultiAssetPools;
	type MaxSwapPathLength = ConstU32<4>;
	type MultiAssetId = Box<MultiLocation>;
	type MultiAssetIdConverter =
		MultiLocationConverter<WestendLocation, LocalAndForeignAssetsMultiLocationMatcher>;
	type MintMinLiquidity = ConstU128<100>;
	type WeightInfo = weights::pallet_asset_conversion::WeightInfo<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper =
		crate::xcm_config::BenchmarkMultiLocationConverter<parachain_info::Pallet<Runtime>>;
}

parameter_types! {
	// we just reuse the same deposits
	pub const ForeignAssetsAssetDeposit: Balance = AssetDeposit::get();
	pub const ForeignAssetsAssetAccountDeposit: Balance = AssetAccountDeposit::get();
	pub const ForeignAssetsApprovalDeposit: Balance = ApprovalDeposit::get();
	pub const ForeignAssetsAssetsStringLimit: u32 = AssetsStringLimit::get();
	pub const ForeignAssetsMetadataDepositBase: Balance = MetadataDepositBase::get();
	pub const ForeignAssetsMetadataDepositPerByte: Balance = MetadataDepositPerByte::get();
}

/// Assets managed by some foreign location. Note: we do not declare a `ForeignAssetsCall` type, as
/// this type is used in proxy definitions. We assume that a foreign location would not want to set
/// an individual, local account as a proxy for the issuance of their assets. This issuance should
/// be managed by the foreign location's governance.
pub type ForeignAssetsInstance = pallet_assets::Instance2;
impl pallet_assets::Config<ForeignAssetsInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type AssetId = MultiLocationForAssetId;
	type AssetIdParameter = MultiLocationForAssetId;
	type Currency = Balances;
	type CreateOrigin = ForeignCreators<
		(FromSiblingParachain<parachain_info::Pallet<Runtime>>,),
		ForeignCreatorsSovereignAccountOf,
		AccountId,
	>;
	type ForceOrigin = AssetsForceOrigin;
	type AssetDeposit = ForeignAssetsAssetDeposit;
	type MetadataDepositBase = ForeignAssetsMetadataDepositBase;
	type MetadataDepositPerByte = ForeignAssetsMetadataDepositPerByte;
	type ApprovalDeposit = ForeignAssetsApprovalDeposit;
	type StringLimit = ForeignAssetsAssetsStringLimit;
	type Freezer = ();
	type Extra = ();
	type WeightInfo = weights::pallet_assets_foreign::WeightInfo<Runtime>;
	type CallbackHandle = ();
	type AssetAccountDeposit = ForeignAssetsAssetAccountDeposit;
	type RemoveItemsLimit = frame_support::traits::ConstU32<1000>;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = xcm_config::XcmBenchmarkHelper;
}

parameter_types! {
	// One storage item; key size is 32; value is size 4+4+16+32 bytes = 56 bytes.
	pub const DepositBase: Balance = deposit(1, 88);
	// Additional storage item size of 32 bytes.
	pub const DepositFactor: Balance = deposit(0, 32);
	pub const MaxSignatories: u32 = 100;
}

impl pallet_multisig::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type Currency = Balances;
	type DepositBase = DepositBase;
	type DepositFactor = DepositFactor;
	type MaxSignatories = MaxSignatories;
	type WeightInfo = weights::pallet_multisig::WeightInfo<Runtime>;
}

impl pallet_utility::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type PalletsOrigin = OriginCaller;
	type WeightInfo = weights::pallet_utility::WeightInfo<Runtime>;
}

parameter_types! {
	// One storage item; key size 32, value size 8; .
	pub const ProxyDepositBase: Balance = deposit(1, 40);
	// Additional storage item size of 33 bytes.
	pub const ProxyDepositFactor: Balance = deposit(0, 33);
	pub const MaxProxies: u16 = 32;
	// One storage item; key size 32, value size 16
	pub const AnnouncementDepositBase: Balance = deposit(1, 48);
	pub const AnnouncementDepositFactor: Balance = deposit(0, 66);
	pub const MaxPending: u16 = 32;
}

/// The type used to represent the kinds of proxying allowed.
#[derive(
	Copy,
	Clone,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Encode,
	Decode,
	RuntimeDebug,
	MaxEncodedLen,
	scale_info::TypeInfo,
)]
pub enum ProxyType {
	/// Fully permissioned proxy. Can execute any call on behalf of _proxied_.
	Any,
	/// Can execute any call that does not transfer funds or assets.
	NonTransfer,
	/// Proxy with the ability to reject time-delay proxy announcements.
	CancelProxy,
	/// Assets proxy. Can execute any call from `assets`, **including asset transfers**.
	Assets,
	/// Owner proxy. Can execute calls related to asset ownership.
	AssetOwner,
	/// Asset manager. Can execute calls related to asset management.
	AssetManager,
	/// Collator selection proxy. Can execute calls related to collator selection mechanism.
	Collator,
}
impl Default for ProxyType {
	fn default() -> Self {
		Self::Any
	}
}

impl InstanceFilter<RuntimeCall> for ProxyType {
	fn filter(&self, c: &RuntimeCall) -> bool {
		match self {
			ProxyType::Any => true,
			ProxyType::NonTransfer => !matches!(
				c,
				RuntimeCall::Balances { .. } |
					RuntimeCall::Assets { .. } |
					RuntimeCall::NftFractionalization { .. } |
					RuntimeCall::Nfts { .. } |
					RuntimeCall::Uniques { .. }
			),
			ProxyType::CancelProxy => matches!(
				c,
				RuntimeCall::Proxy(pallet_proxy::Call::reject_announcement { .. }) |
					RuntimeCall::Utility { .. } |
					RuntimeCall::Multisig { .. }
			),
			ProxyType::Assets => {
				matches!(
					c,
					RuntimeCall::Assets { .. } |
						RuntimeCall::Utility { .. } |
						RuntimeCall::Multisig { .. } |
						RuntimeCall::NftFractionalization { .. } |
						RuntimeCall::Nfts { .. } | RuntimeCall::Uniques { .. }
				)
			},
			ProxyType::AssetOwner => matches!(
				c,
				RuntimeCall::Assets(TrustBackedAssetsCall::create { .. }) |
					RuntimeCall::Assets(TrustBackedAssetsCall::start_destroy { .. }) |
					RuntimeCall::Assets(TrustBackedAssetsCall::destroy_accounts { .. }) |
					RuntimeCall::Assets(TrustBackedAssetsCall::destroy_approvals { .. }) |
					RuntimeCall::Assets(TrustBackedAssetsCall::finish_destroy { .. }) |
					RuntimeCall::Assets(TrustBackedAssetsCall::transfer_ownership { .. }) |
					RuntimeCall::Assets(TrustBackedAssetsCall::set_team { .. }) |
					RuntimeCall::Assets(TrustBackedAssetsCall::set_metadata { .. }) |
					RuntimeCall::Assets(TrustBackedAssetsCall::clear_metadata { .. }) |
					RuntimeCall::Assets(TrustBackedAssetsCall::set_min_balance { .. }) |
					RuntimeCall::Nfts(pallet_nfts::Call::create { .. }) |
					RuntimeCall::Nfts(pallet_nfts::Call::destroy { .. }) |
					RuntimeCall::Nfts(pallet_nfts::Call::redeposit { .. }) |
					RuntimeCall::Nfts(pallet_nfts::Call::transfer_ownership { .. }) |
					RuntimeCall::Nfts(pallet_nfts::Call::set_team { .. }) |
					RuntimeCall::Nfts(pallet_nfts::Call::set_collection_max_supply { .. }) |
					RuntimeCall::Nfts(pallet_nfts::Call::lock_collection { .. }) |
					RuntimeCall::Uniques(pallet_uniques::Call::create { .. }) |
					RuntimeCall::Uniques(pallet_uniques::Call::destroy { .. }) |
					RuntimeCall::Uniques(pallet_uniques::Call::transfer_ownership { .. }) |
					RuntimeCall::Uniques(pallet_uniques::Call::set_team { .. }) |
					RuntimeCall::Uniques(pallet_uniques::Call::set_metadata { .. }) |
					RuntimeCall::Uniques(pallet_uniques::Call::set_attribute { .. }) |
					RuntimeCall::Uniques(pallet_uniques::Call::set_collection_metadata { .. }) |
					RuntimeCall::Uniques(pallet_uniques::Call::clear_metadata { .. }) |
					RuntimeCall::Uniques(pallet_uniques::Call::clear_attribute { .. }) |
					RuntimeCall::Uniques(pallet_uniques::Call::clear_collection_metadata { .. }) |
					RuntimeCall::Uniques(pallet_uniques::Call::set_collection_max_supply { .. }) |
					RuntimeCall::Utility { .. } |
					RuntimeCall::Multisig { .. }
			),
			ProxyType::AssetManager => matches!(
				c,
				RuntimeCall::Assets(TrustBackedAssetsCall::mint { .. }) |
					RuntimeCall::Assets(TrustBackedAssetsCall::burn { .. }) |
					RuntimeCall::Assets(TrustBackedAssetsCall::freeze { .. }) |
					RuntimeCall::Assets(TrustBackedAssetsCall::block { .. }) |
					RuntimeCall::Assets(TrustBackedAssetsCall::thaw { .. }) |
					RuntimeCall::Assets(TrustBackedAssetsCall::freeze_asset { .. }) |
					RuntimeCall::Assets(TrustBackedAssetsCall::thaw_asset { .. }) |
					RuntimeCall::Assets(TrustBackedAssetsCall::touch_other { .. }) |
					RuntimeCall::Assets(TrustBackedAssetsCall::refund_other { .. }) |
					RuntimeCall::Nfts(pallet_nfts::Call::force_mint { .. }) |
					RuntimeCall::Nfts(pallet_nfts::Call::update_mint_settings { .. }) |
					RuntimeCall::Nfts(pallet_nfts::Call::mint_pre_signed { .. }) |
					RuntimeCall::Nfts(pallet_nfts::Call::set_attributes_pre_signed { .. }) |
					RuntimeCall::Nfts(pallet_nfts::Call::lock_item_transfer { .. }) |
					RuntimeCall::Nfts(pallet_nfts::Call::unlock_item_transfer { .. }) |
					RuntimeCall::Nfts(pallet_nfts::Call::lock_item_properties { .. }) |
					RuntimeCall::Nfts(pallet_nfts::Call::set_metadata { .. }) |
					RuntimeCall::Nfts(pallet_nfts::Call::clear_metadata { .. }) |
					RuntimeCall::Nfts(pallet_nfts::Call::set_collection_metadata { .. }) |
					RuntimeCall::Nfts(pallet_nfts::Call::clear_collection_metadata { .. }) |
					RuntimeCall::Uniques(pallet_uniques::Call::mint { .. }) |
					RuntimeCall::Uniques(pallet_uniques::Call::burn { .. }) |
					RuntimeCall::Uniques(pallet_uniques::Call::freeze { .. }) |
					RuntimeCall::Uniques(pallet_uniques::Call::thaw { .. }) |
					RuntimeCall::Uniques(pallet_uniques::Call::freeze_collection { .. }) |
					RuntimeCall::Uniques(pallet_uniques::Call::thaw_collection { .. }) |
					RuntimeCall::Utility { .. } |
					RuntimeCall::Multisig { .. }
			),
			ProxyType::Collator => matches!(
				c,
				RuntimeCall::CollatorSelection { .. } |
					RuntimeCall::Utility { .. } |
					RuntimeCall::Multisig { .. }
			),
		}
	}

	fn is_superset(&self, o: &Self) -> bool {
		match (self, o) {
			(x, y) if x == y => true,
			(ProxyType::Any, _) => true,
			(_, ProxyType::Any) => false,
			(ProxyType::Assets, ProxyType::AssetOwner) => true,
			(ProxyType::Assets, ProxyType::AssetManager) => true,
			(ProxyType::NonTransfer, ProxyType::Collator) => true,
			_ => false,
		}
	}
}

impl pallet_proxy::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type Currency = Balances;
	type ProxyType = ProxyType;
	type ProxyDepositBase = ProxyDepositBase;
	type ProxyDepositFactor = ProxyDepositFactor;
	type MaxProxies = MaxProxies;
	type WeightInfo = weights::pallet_proxy::WeightInfo<Runtime>;
	type MaxPending = MaxPending;
	type CallHasher = BlakeTwo256;
	type AnnouncementDepositBase = AnnouncementDepositBase;
	type AnnouncementDepositFactor = AnnouncementDepositFactor;
}

parameter_types! {
	pub const ReservedXcmpWeight: Weight = MAXIMUM_BLOCK_WEIGHT.saturating_div(4);
	pub const ReservedDmpWeight: Weight = MAXIMUM_BLOCK_WEIGHT.saturating_div(4);
}

impl cumulus_pallet_parachain_system::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OnSystemEvent = ();
	type SelfParaId = parachain_info::Pallet<Runtime>;
	type DmpMessageHandler = DmpQueue;
	type ReservedDmpWeight = ReservedDmpWeight;
	type OutboundXcmpMessageSource = XcmpQueue;
	type XcmpMessageHandler = XcmpQueue;
	type ReservedXcmpWeight = ReservedXcmpWeight;
	type CheckAssociatedRelayNumber = RelayNumberStrictlyIncreases;
	type ConsensusHook = cumulus_pallet_aura_ext::FixedVelocityConsensusHook<
		Runtime,
		RELAY_CHAIN_SLOT_DURATION_MILLIS,
		BLOCK_PROCESSING_VELOCITY,
		UNINCLUDED_SEGMENT_CAPACITY,
	>;
}

impl parachain_info::Config for Runtime {}

impl cumulus_pallet_aura_ext::Config for Runtime {}

impl cumulus_pallet_xcmp_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type ChannelInfo = ParachainSystem;
	type VersionWrapper = PolkadotXcm;
	type ExecuteOverweightOrigin = EnsureRoot<AccountId>;
	type ControllerOrigin = EnsureRoot<AccountId>;
	type ControllerOriginConverter = XcmOriginToTransactDispatchOrigin;
	type WeightInfo = weights::cumulus_pallet_xcmp_queue::WeightInfo<Runtime>;
	type PriceForSiblingDelivery = ();
}

impl cumulus_pallet_dmp_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type ExecuteOverweightOrigin = EnsureRoot<AccountId>;
}

parameter_types! {
	pub const Period: u32 = 6 * HOURS;
	pub const Offset: u32 = 0;
}

impl pallet_session::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	// we don't have stash and controller, thus we don't need the convert as well.
	type ValidatorIdOf = pallet_collator_selection::IdentityCollator;
	type ShouldEndSession = pallet_session::PeriodicSessions<Period, Offset>;
	type NextSessionRotation = pallet_session::PeriodicSessions<Period, Offset>;
	type SessionManager = CollatorSelection;
	// Essentially just Aura, but let's be pedantic.
	type SessionHandler = <SessionKeys as sp_runtime::traits::OpaqueKeys>::KeyTypeIdProviders;
	type Keys = SessionKeys;
	type WeightInfo = weights::pallet_session::WeightInfo<Runtime>;
}

impl pallet_aura::Config for Runtime {
	type AuthorityId = AuraId;
	type DisabledValidators = ();
	type MaxAuthorities = ConstU32<100_000>;
	type AllowMultipleBlocksPerSlot = ConstBool<false>;
	#[cfg(feature = "experimental")]
	type SlotDuration = pallet_aura::MinimumPeriodTimesTwo<Self>;
}

parameter_types! {
	pub const PotId: PalletId = PalletId(*b"PotStake");
	pub const SessionLength: BlockNumber = 6 * HOURS;
}

pub type CollatorSelectionUpdateOrigin = EnsureRoot<AccountId>;

impl pallet_collator_selection::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type UpdateOrigin = CollatorSelectionUpdateOrigin;
	type PotId = PotId;
	type MaxCandidates = ConstU32<100>;
	type MinEligibleCollators = ConstU32<4>;
	type MaxInvulnerables = ConstU32<20>;
	// should be a multiple of session or things will get inconsistent
	type KickThreshold = Period;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type ValidatorIdOf = pallet_collator_selection::IdentityCollator;
	type ValidatorRegistration = Session;
	type WeightInfo = weights::pallet_collator_selection::WeightInfo<Runtime>;
}

impl pallet_asset_conversion_tx_payment::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Fungibles = LocalAndForeignAssets<
		Assets,
		AssetIdForTrustBackedAssetsConvert<TrustBackedAssetsPalletLocation>,
		ForeignAssets,
	>;
	type OnChargeAssetTransaction = AssetConversionAdapter<Balances, AssetConversion>;
}

parameter_types! {
	pub const UniquesCollectionDeposit: Balance = UNITS / 10; // 1 / 10 UNIT deposit to create a collection
	pub const UniquesItemDeposit: Balance = UNITS / 1_000; // 1 / 1000 UNIT deposit to mint an item
	pub const UniquesMetadataDepositBase: Balance = deposit(1, 129);
	pub const UniquesAttributeDepositBase: Balance = deposit(1, 0);
	pub const UniquesDepositPerByte: Balance = deposit(0, 1);
}

impl pallet_uniques::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type CollectionId = u32;
	type ItemId = u32;
	type Currency = Balances;
	type ForceOrigin = AssetsForceOrigin;
	type CollectionDeposit = UniquesCollectionDeposit;
	type ItemDeposit = UniquesItemDeposit;
	type MetadataDepositBase = UniquesMetadataDepositBase;
	type AttributeDepositBase = UniquesAttributeDepositBase;
	type DepositPerByte = UniquesDepositPerByte;
	type StringLimit = ConstU32<128>;
	type KeyLimit = ConstU32<32>;
	type ValueLimit = ConstU32<64>;
	type WeightInfo = weights::pallet_uniques::WeightInfo<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type Helper = ();
	type CreateOrigin = AsEnsureOriginWithArg<EnsureSigned<AccountId>>;
	type Locker = ();
}

parameter_types! {
	pub const NftFractionalizationPalletId: PalletId = PalletId(*b"fraction");
	pub NewAssetSymbol: BoundedVec<u8, AssetsStringLimit> = (*b"FRAC").to_vec().try_into().unwrap();
	pub NewAssetName: BoundedVec<u8, AssetsStringLimit> = (*b"Frac").to_vec().try_into().unwrap();
}

impl pallet_nft_fractionalization::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Deposit = AssetDeposit;
	type Currency = Balances;
	type NewAssetSymbol = NewAssetSymbol;
	type NewAssetName = NewAssetName;
	type StringLimit = AssetsStringLimit;
	type NftCollectionId = <Self as pallet_nfts::Config>::CollectionId;
	type NftId = <Self as pallet_nfts::Config>::ItemId;
	type AssetBalance = <Self as pallet_balances::Config>::Balance;
	type AssetId = <Self as pallet_assets::Config<TrustBackedAssetsInstance>>::AssetId;
	type Assets = Assets;
	type Nfts = Nfts;
	type PalletId = NftFractionalizationPalletId;
	type WeightInfo = pallet_nft_fractionalization::weights::SubstrateWeight<Runtime>;
	type RuntimeHoldReason = RuntimeHoldReason;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

parameter_types! {
	pub NftsPalletFeatures: PalletFeatures = PalletFeatures::all_enabled();
	pub const NftsMaxDeadlineDuration: BlockNumber = 12 * 30 * DAYS;
	// re-use the Uniques deposits
	pub const NftsCollectionDeposit: Balance = UniquesCollectionDeposit::get();
	pub const NftsItemDeposit: Balance = UniquesItemDeposit::get();
	pub const NftsMetadataDepositBase: Balance = UniquesMetadataDepositBase::get();
	pub const NftsAttributeDepositBase: Balance = UniquesAttributeDepositBase::get();
	pub const NftsDepositPerByte: Balance = UniquesDepositPerByte::get();
}

impl pallet_nfts::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type CollectionId = u32;
	type ItemId = u32;
	type Currency = Balances;
	type CreateOrigin = AsEnsureOriginWithArg<EnsureSigned<AccountId>>;
	type ForceOrigin = AssetsForceOrigin;
	type Locker = ();
	type CollectionDeposit = NftsCollectionDeposit;
	type ItemDeposit = NftsItemDeposit;
	type MetadataDepositBase = NftsMetadataDepositBase;
	type AttributeDepositBase = NftsAttributeDepositBase;
	type DepositPerByte = NftsDepositPerByte;
	type StringLimit = ConstU32<256>;
	type KeyLimit = ConstU32<64>;
	type ValueLimit = ConstU32<256>;
	type ApprovalsLimit = ConstU32<20>;
	type ItemAttributesApprovalsLimit = ConstU32<30>;
	type MaxTips = ConstU32<10>;
	type MaxDeadlineDuration = NftsMaxDeadlineDuration;
	type MaxAttributesPerCall = ConstU32<10>;
	type Features = NftsPalletFeatures;
	type OffchainSignature = Signature;
	type OffchainPublic = <Signature as Verify>::Signer;
	type WeightInfo = weights::pallet_nfts::WeightInfo<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type Helper = ();
}

// Create the runtime by composing the FRAME pallets that were previously configured.
construct_runtime!(
	pub enum Runtime
	{
		// System support stuff.
		System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>} = 0,
		ParachainSystem: cumulus_pallet_parachain_system::{
			Pallet, Call, Config<T>, Storage, Inherent, Event<T>, ValidateUnsigned,
		} = 1,
		// RandomnessCollectiveFlip = 2 removed
		Timestamp: pallet_timestamp::{Pallet, Call, Storage, Inherent} = 3,
		ParachainInfo: parachain_info::{Pallet, Storage, Config<T>} = 4,

		// Monetary stuff.
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>} = 10,
		TransactionPayment: pallet_transaction_payment::{Pallet, Storage, Event<T>} = 11,
		// AssetTxPayment: pallet_asset_tx_payment::{Pallet, Event<T>} = 12,
		AssetTxPayment: pallet_asset_conversion_tx_payment::{Pallet, Event<T>} = 13,

		// Collator support. the order of these 5 are important and shall not change.
		Authorship: pallet_authorship::{Pallet, Storage} = 20,
		CollatorSelection: pallet_collator_selection::{Pallet, Call, Storage, Event<T>, Config<T>} = 21,
		Session: pallet_session::{Pallet, Call, Storage, Event, Config<T>} = 22,
		Aura: pallet_aura::{Pallet, Storage, Config<T>} = 23,
		AuraExt: cumulus_pallet_aura_ext::{Pallet, Storage, Config<T>} = 24,

		// XCM helpers.
		XcmpQueue: cumulus_pallet_xcmp_queue::{Pallet, Call, Storage, Event<T>} = 30,
		PolkadotXcm: pallet_xcm::{Pallet, Call, Storage, Event<T>, Origin, Config<T>} = 31,
		CumulusXcm: cumulus_pallet_xcm::{Pallet, Event<T>, Origin} = 32,
		DmpQueue: cumulus_pallet_dmp_queue::{Pallet, Call, Storage, Event<T>} = 33,

		// Handy utilities.
		Utility: pallet_utility::{Pallet, Call, Event} = 40,
		Multisig: pallet_multisig::{Pallet, Call, Storage, Event<T>} = 41,
		Proxy: pallet_proxy::{Pallet, Call, Storage, Event<T>} = 42,

		// The main stage.
		Assets: pallet_assets::<Instance1>::{Pallet, Call, Storage, Event<T>} = 50,
		Uniques: pallet_uniques::{Pallet, Call, Storage, Event<T>} = 51,
		Nfts: pallet_nfts::{Pallet, Call, Storage, Event<T>} = 52,
		ForeignAssets: pallet_assets::<Instance2>::{Pallet, Call, Storage, Event<T>} = 53,
		NftFractionalization: pallet_nft_fractionalization::{Pallet, Call, Storage, Event<T>, HoldReason} = 54,
		PoolAssets: pallet_assets::<Instance3>::{Pallet, Call, Storage, Event<T>} = 55,
		AssetConversion: pallet_asset_conversion::{Pallet, Call, Storage, Event<T>} = 56,
	}
);

/// The address format for describing accounts.
pub type Address = sp_runtime::MultiAddress<AccountId, ()>;
/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
/// A Block signed with a Justification
pub type SignedBlock = generic::SignedBlock<Block>;
/// BlockId type as expected by this runtime.
pub type BlockId = generic::BlockId<Block>;
/// The SignedExtension to the basic transaction logic.
pub type SignedExtra = (
	frame_system::CheckNonZeroSender<Runtime>,
	frame_system::CheckSpecVersion<Runtime>,
	frame_system::CheckTxVersion<Runtime>,
	frame_system::CheckGenesis<Runtime>,
	frame_system::CheckEra<Runtime>,
	frame_system::CheckNonce<Runtime>,
	frame_system::CheckWeight<Runtime>,
	pallet_asset_conversion_tx_payment::ChargeAssetTxPayment<Runtime>,
);
/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic =
	generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, SignedExtra>;

/// Migrations to apply on runtime upgrade.
pub type Migrations = (
	// v9420
	pallet_nfts::migration::v1::MigrateToV1<Runtime>,
	// unreleased
	pallet_collator_selection::migration::v1::MigrateToV1<Runtime>,
	// unreleased
	migrations::NativeAssetParents0ToParents1Migration<Runtime>,
);

/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
	Runtime,
	Block,
	frame_system::ChainContext<Runtime>,
	Runtime,
	AllPalletsWithSystem,
	Migrations,
>;

#[cfg(feature = "runtime-benchmarks")]
#[macro_use]
extern crate frame_benchmarking;

#[cfg(feature = "runtime-benchmarks")]
mod benches {
	define_benchmarks!(
		[frame_system, SystemBench::<Runtime>]
		[pallet_assets, Local]
		[pallet_assets, Foreign]
		[pallet_assets, Pool]
		[pallet_asset_conversion, AssetConversion]
		[pallet_balances, Balances]
		[pallet_multisig, Multisig]
		[pallet_nft_fractionalization, NftFractionalization]
		[pallet_nfts, Nfts]
		[pallet_proxy, Proxy]
		[pallet_session, SessionBench::<Runtime>]
		[pallet_uniques, Uniques]
		[pallet_utility, Utility]
		[pallet_timestamp, Timestamp]
		[pallet_collator_selection, CollatorSelection]
		[cumulus_pallet_xcmp_queue, XcmpQueue]
		// XCM
		[pallet_xcm, PolkadotXcm]
		// NOTE: Make sure you point to the individual modules below.
		[pallet_xcm_benchmarks::fungible, XcmBalances]
		[pallet_xcm_benchmarks::generic, XcmGeneric]
	);
}

impl_runtime_apis! {
	impl sp_consensus_aura::AuraApi<Block, AuraId> for Runtime {
		fn slot_duration() -> sp_consensus_aura::SlotDuration {
			sp_consensus_aura::SlotDuration::from_millis(Aura::slot_duration())
		}

		fn authorities() -> Vec<AuraId> {
			Aura::authorities().into_inner()
		}
	}

	impl sp_api::Core<Block> for Runtime {
		fn version() -> RuntimeVersion {
			VERSION
		}

		fn execute_block(block: Block) {
			Executive::execute_block(block)
		}

		fn initialize_block(header: &<Block as BlockT>::Header) {
			Executive::initialize_block(header)
		}
	}

	impl sp_api::Metadata<Block> for Runtime {
		fn metadata() -> OpaqueMetadata {
			OpaqueMetadata::new(Runtime::metadata().into())
		}

		fn metadata_at_version(version: u32) -> Option<OpaqueMetadata> {
			Runtime::metadata_at_version(version)
		}

		fn metadata_versions() -> sp_std::vec::Vec<u32> {
			Runtime::metadata_versions()
		}
	}

	impl sp_block_builder::BlockBuilder<Block> for Runtime {
		fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
			Executive::apply_extrinsic(extrinsic)
		}

		fn finalize_block() -> <Block as BlockT>::Header {
			Executive::finalize_block()
		}

		fn inherent_extrinsics(data: sp_inherents::InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
			data.create_extrinsics()
		}

		fn check_inherents(
			block: Block,
			data: sp_inherents::InherentData,
		) -> sp_inherents::CheckInherentsResult {
			data.check_extrinsics(&block)
		}
	}

	impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
		fn validate_transaction(
			source: TransactionSource,
			tx: <Block as BlockT>::Extrinsic,
			block_hash: <Block as BlockT>::Hash,
		) -> TransactionValidity {
			Executive::validate_transaction(source, tx, block_hash)
		}
	}

	impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
		fn offchain_worker(header: &<Block as BlockT>::Header) {
			Executive::offchain_worker(header)
		}
	}

	impl sp_session::SessionKeys<Block> for Runtime {
		fn generate_session_keys(seed: Option<Vec<u8>>) -> Vec<u8> {
			SessionKeys::generate(seed)
		}

		fn decode_session_keys(
			encoded: Vec<u8>,
		) -> Option<Vec<(Vec<u8>, KeyTypeId)>> {
			SessionKeys::decode_into_raw_public_keys(&encoded)
		}
	}

	impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Nonce> for Runtime {
		fn account_nonce(account: AccountId) -> Nonce {
			System::account_nonce(account)
		}
	}

	impl pallet_nfts_runtime_api::NftsApi<Block, AccountId, u32, u32> for Runtime {
		fn owner(collection: u32, item: u32) -> Option<AccountId> {
			<Nfts as Inspect<AccountId>>::owner(&collection, &item)
		}

		fn collection_owner(collection: u32) -> Option<AccountId> {
			<Nfts as Inspect<AccountId>>::collection_owner(&collection)
		}

		fn attribute(
			collection: u32,
			item: u32,
			key: Vec<u8>,
		) -> Option<Vec<u8>> {
			<Nfts as Inspect<AccountId>>::attribute(&collection, &item, &key)
		}

		fn custom_attribute(
			account: AccountId,
			collection: u32,
			item: u32,
			key: Vec<u8>,
		) -> Option<Vec<u8>> {
			<Nfts as Inspect<AccountId>>::custom_attribute(
				&account,
				&collection,
				&item,
				&key,
			)
		}

		fn system_attribute(
			collection: u32,
			item: u32,
			key: Vec<u8>,
		) -> Option<Vec<u8>> {
			<Nfts as Inspect<AccountId>>::system_attribute(&collection, &item, &key)
		}

		fn collection_attribute(collection: u32, key: Vec<u8>) -> Option<Vec<u8>> {
			<Nfts as Inspect<AccountId>>::collection_attribute(&collection, &key)
		}
	}

	impl pallet_asset_conversion::AssetConversionApi<
		Block,
		Balance,
		u128,
		Box<MultiLocation>,
	> for Runtime
	{
		fn quote_price_exact_tokens_for_tokens(asset1: Box<MultiLocation>, asset2: Box<MultiLocation>, amount: u128, include_fee: bool) -> Option<Balance> {
			AssetConversion::quote_price_exact_tokens_for_tokens(asset1, asset2, amount, include_fee)
		}

		fn quote_price_tokens_for_exact_tokens(asset1: Box<MultiLocation>, asset2: Box<MultiLocation>, amount: u128, include_fee: bool) -> Option<Balance> {
			AssetConversion::quote_price_tokens_for_exact_tokens(asset1, asset2, amount, include_fee)
		}

		fn get_reserves(asset1: Box<MultiLocation>, asset2: Box<MultiLocation>) -> Option<(Balance, Balance)> {
			AssetConversion::get_reserves(&asset1, &asset2).ok()
		}
	}

	impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<Block, Balance> for Runtime {
		fn query_info(
			uxt: <Block as BlockT>::Extrinsic,
			len: u32,
		) -> pallet_transaction_payment_rpc_runtime_api::RuntimeDispatchInfo<Balance> {
			TransactionPayment::query_info(uxt, len)
		}
		fn query_fee_details(
			uxt: <Block as BlockT>::Extrinsic,
			len: u32,
		) -> pallet_transaction_payment::FeeDetails<Balance> {
			TransactionPayment::query_fee_details(uxt, len)
		}
		fn query_weight_to_fee(weight: Weight) -> Balance {
			TransactionPayment::weight_to_fee(weight)
		}
		fn query_length_to_fee(length: u32) -> Balance {
			TransactionPayment::length_to_fee(length)
		}
	}

	impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentCallApi<Block, Balance, RuntimeCall>
		for Runtime
	{
		fn query_call_info(
			call: RuntimeCall,
			len: u32,
		) -> pallet_transaction_payment::RuntimeDispatchInfo<Balance> {
			TransactionPayment::query_call_info(call, len)
		}
		fn query_call_fee_details(
			call: RuntimeCall,
			len: u32,
		) -> pallet_transaction_payment::FeeDetails<Balance> {
			TransactionPayment::query_call_fee_details(call, len)
		}
		fn query_weight_to_fee(weight: Weight) -> Balance {
			TransactionPayment::weight_to_fee(weight)
		}
		fn query_length_to_fee(length: u32) -> Balance {
			TransactionPayment::length_to_fee(length)
		}
	}

	impl assets_common::runtime_api::FungiblesApi<
		Block,
		AccountId,
	> for Runtime
	{
		fn query_account_balances(account: AccountId) -> Result<xcm::VersionedMultiAssets, assets_common::runtime_api::FungiblesAccessError> {
			use assets_common::fungible_conversion::{convert, convert_balance};
			Ok([
				// collect pallet_balance
				{
					let balance = Balances::free_balance(account.clone());
					if balance > 0 {
						vec![convert_balance::<WestendLocation, Balance>(balance)?]
					} else {
						vec![]
					}
				},
				// collect pallet_assets (TrustBackedAssets)
				convert::<_, _, _, _, TrustBackedAssetsConvertedConcreteId>(
					Assets::account_balances(account.clone())
						.iter()
						.filter(|(_, balance)| balance > &0)
				)?,
				// collect pallet_assets (ForeignAssets)
				convert::<_, _, _, _, ForeignAssetsConvertedConcreteId>(
					ForeignAssets::account_balances(account.clone())
						.iter()
						.filter(|(_, balance)| balance > &0)
				)?,
				// collect pallet_assets (PoolAssets)
				convert::<_, _, _, _, PoolAssetsConvertedConcreteId>(
					PoolAssets::account_balances(account)
						.iter()
						.filter(|(_, balance)| balance > &0)
				)?,
				// collect ... e.g. other tokens
			].concat().into())
		}
	}

	impl cumulus_primitives_core::CollectCollationInfo<Block> for Runtime {
		fn collect_collation_info(header: &<Block as BlockT>::Header) -> cumulus_primitives_core::CollationInfo {
			ParachainSystem::collect_collation_info(header)
		}
	}

	#[cfg(feature = "try-runtime")]
	impl frame_try_runtime::TryRuntime<Block> for Runtime {
		fn on_runtime_upgrade(checks: frame_try_runtime::UpgradeCheckSelect) -> (Weight, Weight) {
			let weight = Executive::try_runtime_upgrade(checks).unwrap();
			(weight, RuntimeBlockWeights::get().max_block)
		}

		fn execute_block(
			block: Block,
			state_root_check: bool,
			signature_check: bool,
			select: frame_try_runtime::TryStateSelect,
		) -> Weight {
			// NOTE: intentional unwrap: we don't want to propagate the error backwards, and want to
			// have a backtrace here.
			Executive::try_execute_block(block, state_root_check, signature_check, select).unwrap()
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	impl frame_benchmarking::Benchmark<Block> for Runtime {
		fn benchmark_metadata(extra: bool) -> (
			Vec<frame_benchmarking::BenchmarkList>,
			Vec<frame_support::traits::StorageInfo>,
		) {
			use frame_benchmarking::{Benchmarking, BenchmarkList};
			use frame_support::traits::StorageInfoTrait;
			use frame_system_benchmarking::Pallet as SystemBench;
			use cumulus_pallet_session_benchmarking::Pallet as SessionBench;

			// This is defined once again in dispatch_benchmark, because list_benchmarks!
			// and add_benchmarks! are macros exported by define_benchmarks! macros and those types
			// are referenced in that call.
			type XcmBalances = pallet_xcm_benchmarks::fungible::Pallet::<Runtime>;
			type XcmGeneric = pallet_xcm_benchmarks::generic::Pallet::<Runtime>;

			// Benchmark files generated for `Assets/ForeignAssets` instances are by default
			// `pallet_assets_assets.rs / pallet_assets_foreign_assets`, which is not really nice,
			// so with this redefinition we can change names to nicer:
			// `pallet_assets_local.rs / pallet_assets_foreign.rs`.
			type Local = pallet_assets::Pallet::<Runtime, TrustBackedAssetsInstance>;
			type Foreign = pallet_assets::Pallet::<Runtime, ForeignAssetsInstance>;
			type Pool = pallet_assets::Pallet::<Runtime, PoolAssetsInstance>;

			let mut list = Vec::<BenchmarkList>::new();
			list_benchmarks!(list, extra);

			let storage_info = AllPalletsWithSystem::storage_info();
			(list, storage_info)
		}

		fn dispatch_benchmark(
			config: frame_benchmarking::BenchmarkConfig
		) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, sp_runtime::RuntimeString> {
			use frame_benchmarking::{Benchmarking, BenchmarkBatch, BenchmarkError};
			use sp_storage::TrackedStorageKey;

			use frame_system_benchmarking::Pallet as SystemBench;
			impl frame_system_benchmarking::Config for Runtime {
				fn setup_set_code_requirements(code: &sp_std::vec::Vec<u8>) -> Result<(), BenchmarkError> {
					ParachainSystem::initialize_for_set_code_benchmark(code.len() as u32);
					Ok(())
				}

				fn verify_set_code() {
					System::assert_last_event(cumulus_pallet_parachain_system::Event::<Runtime>::ValidationFunctionStored.into());
				}
			}

			use cumulus_pallet_session_benchmarking::Pallet as SessionBench;
			impl cumulus_pallet_session_benchmarking::Config for Runtime {}

			use xcm::latest::prelude::*;
			use xcm_config::{MaxAssetsIntoHolding, WestendLocation};
			use pallet_xcm_benchmarks::asset_instance_from;

			impl pallet_xcm_benchmarks::Config for Runtime {
				type XcmConfig = xcm_config::XcmConfig;
				type AccountIdConverter = xcm_config::LocationToAccountId;
				fn valid_destination() -> Result<MultiLocation, BenchmarkError> {
					Ok(WestendLocation::get())
				}
				fn worst_case_holding(depositable_count: u32) -> MultiAssets {
					// A mix of fungible, non-fungible, and concrete assets.
					let holding_non_fungibles = MaxAssetsIntoHolding::get() / 2 - depositable_count;
					let holding_fungibles = holding_non_fungibles - 1;
					let fungibles_amount: u128 = 100;
					let mut assets = (0..holding_fungibles)
						.map(|i| {
							MultiAsset {
								id: Concrete(GeneralIndex(i as u128).into()),
								fun: Fungible(fungibles_amount * i as u128),
							}
						})
						.chain(core::iter::once(MultiAsset { id: Concrete(Here.into()), fun: Fungible(u128::MAX) }))
						.chain((0..holding_non_fungibles).map(|i| MultiAsset {
							id: Concrete(GeneralIndex(i as u128).into()),
							fun: NonFungible(asset_instance_from(i)),
						}))
						.collect::<Vec<_>>();

					assets.push(MultiAsset {
						id: Concrete(WestendLocation::get()),
						fun: Fungible(1_000_000 * UNITS),
					});
					assets.into()
				}
			}

			parameter_types! {
				pub const TrustedTeleporter: Option<(MultiLocation, MultiAsset)> = Some((
					WestendLocation::get(),
					MultiAsset { fun: Fungible(UNITS), id: Concrete(WestendLocation::get()) },
				));
				pub const CheckedAccount: Option<(AccountId, xcm_builder::MintLocation)> = None;
				pub const TrustedReserve: Option<(MultiLocation, MultiAsset)> = None;
			}

			impl pallet_xcm_benchmarks::fungible::Config for Runtime {
				type TransactAsset = Balances;

				type CheckedAccount = CheckedAccount;
				type TrustedTeleporter = TrustedTeleporter;
				type TrustedReserve = TrustedReserve;

				fn get_multi_asset() -> MultiAsset {
					MultiAsset {
						id: Concrete(WestendLocation::get()),
						fun: Fungible(UNITS),
					}
				}
			}

			impl pallet_xcm_benchmarks::generic::Config for Runtime {
				type RuntimeCall = RuntimeCall;

				fn worst_case_response() -> (u64, Response) {
					(0u64, Response::Version(Default::default()))
				}

				fn worst_case_asset_exchange() -> Result<(MultiAssets, MultiAssets), BenchmarkError> {
					Err(BenchmarkError::Skip)
				}

				fn universal_alias() -> Result<(MultiLocation, Junction), BenchmarkError> {
					Err(BenchmarkError::Skip)
				}

				fn transact_origin_and_runtime_call() -> Result<(MultiLocation, RuntimeCall), BenchmarkError> {
					Ok((WestendLocation::get(), frame_system::Call::remark_with_event { remark: vec![] }.into()))
				}

				fn subscribe_origin() -> Result<MultiLocation, BenchmarkError> {
					Ok(WestendLocation::get())
				}

				fn claimable_asset() -> Result<(MultiLocation, MultiLocation, MultiAssets), BenchmarkError> {
					let origin = WestendLocation::get();
					let assets: MultiAssets = (Concrete(WestendLocation::get()), 1_000 * UNITS).into();
					let ticket = MultiLocation { parents: 0, interior: Here };
					Ok((origin, ticket, assets))
				}

				fn unlockable_asset() -> Result<(MultiLocation, MultiLocation, MultiAsset), BenchmarkError> {
					Err(BenchmarkError::Skip)
				}

				fn export_message_origin_and_destination(
				) -> Result<(MultiLocation, NetworkId, InteriorMultiLocation), BenchmarkError> {
					Err(BenchmarkError::Skip)
				}

				fn alias_origin() -> Result<(MultiLocation, MultiLocation), BenchmarkError> {
					Err(BenchmarkError::Skip)
				}
			}

			type XcmBalances = pallet_xcm_benchmarks::fungible::Pallet::<Runtime>;
			type XcmGeneric = pallet_xcm_benchmarks::generic::Pallet::<Runtime>;

			type Local = pallet_assets::Pallet::<Runtime, TrustBackedAssetsInstance>;
			type Foreign = pallet_assets::Pallet::<Runtime, ForeignAssetsInstance>;
			type Pool = pallet_assets::Pallet::<Runtime, PoolAssetsInstance>;

			let whitelist: Vec<TrackedStorageKey> = vec![
				// Block Number
				hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef702a5c1b19ab7a04f536c519aca4983ac").to_vec().into(),
				// Total Issuance
				hex_literal::hex!("c2261276cc9d1f8598ea4b6a74b15c2f57c875e4cff74148e4628f264b974c80").to_vec().into(),
				// Execution Phase
				hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef7ff553b5a9862a516939d82b3d3d8661a").to_vec().into(),
				// Event Count
				hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef70a98fdbe9ce6c55837576c60c7af3850").to_vec().into(),
				// System Events
				hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef780d41e5e16056765bc8461851072c9d7").to_vec().into(),
				//TODO: use from relay_well_known_keys::ACTIVE_CONFIG
				hex_literal::hex!("06de3d8a54d27e44a9d5ce189618f22db4b49d95320d9021994c850f25b8e385").to_vec().into(),
			];

			let mut batches = Vec::<BenchmarkBatch>::new();
			let params = (&config, &whitelist);
			add_benchmarks!(params, batches);

			Ok(batches)
		}
	}
}

cumulus_pallet_parachain_system::register_validate_block! {
	Runtime = Runtime,
	BlockExecutor = cumulus_pallet_aura_ext::BlockExecutor::<Runtime, Executive>,
}

pub mod migrations {
	use super::*;
	use frame_support::{
		pallet_prelude::Get,
		traits::{
			fungibles::{Inspect, Mutate},
			tokens::Preservation,
			OnRuntimeUpgrade, OriginTrait,
		},
	};
	use parachains_common::impls::AccountIdOf;
	use sp_runtime::{traits::StaticLookup, Saturating};
	use xcm::latest::prelude::*;

	/// Temporary migration because of bug with native asset, it can be removed once applied on
	/// `AssetHubWestend`. Migrates pools with `MultiLocation { parents: 0, interior: Here }` to
	/// `MultiLocation { parents: 1, interior: Here }`
	pub struct NativeAssetParents0ToParents1Migration<T>(sp_std::marker::PhantomData<T>);
	impl<
			T: pallet_asset_conversion::Config<
				MultiAssetId = Box<MultiLocation>,
				AssetId = MultiLocation,
			>,
		> OnRuntimeUpgrade for NativeAssetParents0ToParents1Migration<T>
	where
		<T as pallet_asset_conversion::Config>::PoolAssetId: Into<u32>,
		AccountIdOf<Runtime>: Into<[u8; 32]>,
		<T as frame_system::Config>::AccountId:
			Into<<<T as frame_system::Config>::RuntimeOrigin as OriginTrait>::AccountId>,
		<<T as frame_system::Config>::Lookup as StaticLookup>::Source:
			From<<T as frame_system::Config>::AccountId>,
		sp_runtime::AccountId32: From<<T as frame_system::Config>::AccountId>,
	{
		fn on_runtime_upgrade() -> Weight {
			let invalid_native_asset = MultiLocation { parents: 0, interior: Here };
			let valid_native_asset = WestendLocation::get();

			let mut reads: u64 = 1;
			let mut writes: u64 = 0;

			// migrate pools with invalid native asset
			let pools = pallet_asset_conversion::Pools::<T>::iter().collect::<Vec<_>>();
			reads.saturating_accrue(1);
			for (old_pool_id, pool_info) in pools {
				let old_pool_account =
					pallet_asset_conversion::Pallet::<T>::get_pool_account(&old_pool_id);
				reads.saturating_accrue(1);
				let pool_asset_id = pool_info.lp_token.clone();
				if old_pool_id.0.as_ref() != &invalid_native_asset {
					// skip, if ok
					continue
				}

				// fix new account
				let new_pool_id = pallet_asset_conversion::Pallet::<T>::get_pool_id(
					Box::new(valid_native_asset),
					old_pool_id.1.clone(),
				);
				let new_pool_account =
					pallet_asset_conversion::Pallet::<T>::get_pool_account(&new_pool_id);
				frame_system::Pallet::<T>::inc_providers(&new_pool_account);
				reads.saturating_accrue(2);
				writes.saturating_accrue(1);

				// move currency
				let _ = Balances::transfer_all(
					RuntimeOrigin::signed(sp_runtime::AccountId32::from(old_pool_account.clone())),
					sp_runtime::AccountId32::from(new_pool_account.clone()).into(),
					false,
				);
				reads.saturating_accrue(2);
				writes.saturating_accrue(2);

				// move LP token
				let _ = T::PoolAssets::transfer(
					pool_asset_id.clone(),
					&old_pool_account,
					&new_pool_account,
					T::PoolAssets::balance(pool_asset_id.clone(), &old_pool_account),
					Preservation::Expendable,
				);
				reads.saturating_accrue(1);
				writes.saturating_accrue(2);

				// change the ownership of LP token
				let _ = pallet_assets::Pallet::<Runtime, PoolAssetsInstance>::transfer_ownership(
					RuntimeOrigin::signed(sp_runtime::AccountId32::from(old_pool_account.clone())),
					pool_asset_id.into(),
					sp_runtime::AccountId32::from(new_pool_account.clone()).into(),
				);
				reads.saturating_accrue(1);
				writes.saturating_accrue(2);

				// move LocalOrForeignAssets
				let _ = T::Assets::transfer(
					*old_pool_id.1.as_ref(),
					&old_pool_account,
					&new_pool_account,
					T::Assets::balance(*old_pool_id.1.as_ref(), &old_pool_account),
					Preservation::Expendable,
				);
				reads.saturating_accrue(1);
				writes.saturating_accrue(2);

				// dec providers for old account
				let _ = frame_system::Pallet::<T>::dec_providers(&old_pool_account);
				writes.saturating_accrue(1);

				// change pool key
				pallet_asset_conversion::Pools::<T>::insert(new_pool_id, pool_info);
				pallet_asset_conversion::Pools::<T>::remove(old_pool_id);
			}

			T::DbWeight::get().reads_writes(reads, writes)
		}
	}
}
