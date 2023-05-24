// Copyright 2022 Parity Technologies (UK) Ltd.
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

#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit = "256"]

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

pub mod bridge_hub_rococo_config;
pub mod bridge_hub_wococo_config;
pub mod constants;
mod weights;
pub mod xcm_config;

use constants::currency::*;
use cumulus_pallet_parachain_system::RelayNumberStrictlyIncreases;
use sp_api::impl_runtime_apis;
use sp_core::{crypto::KeyTypeId, OpaqueMetadata};
use sp_runtime::{
	create_runtime_str, generic, impl_opaque_keys,
	traits::{AccountIdLookup, BlakeTwo256, Block as BlockT},
	transaction_validity::{TransactionSource, TransactionValidity},
	ApplyExtrinsicResult,
};

use sp_std::prelude::*;
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

use frame_support::{
	construct_runtime,
	dispatch::DispatchClass,
	parameter_types,
	traits::{ConstU32, ConstU64, ConstU8, Everything},
	weights::{ConstantMultiplier, Weight},
	PalletId,
};
use frame_system::{
	limits::{BlockLength, BlockWeights},
	EnsureRoot,
};
pub use sp_consensus_aura::sr25519::AuthorityId as AuraId;
pub use sp_runtime::{MultiAddress, Perbill, Permill};
use xcm_config::{XcmConfig, XcmOriginToTransactDispatchOrigin};

use bp_parachains::SingleParaStoredHeaderDataBuilder;
use bp_runtime::HeaderId;

#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;

use polkadot_runtime_common::{BlockHashCount, SlowAdjustingFeeUpdate};

use weights::{BlockExecutionWeight, ExtrinsicBaseWeight, RocksDbWeight};

use crate::{
	bridge_hub_rococo_config::{
		BridgeRefundBridgeHubWococoMessages, OnBridgeHubRococoBlobDispatcher,
		WithBridgeHubWococoMessageBridge,
	},
	bridge_hub_wococo_config::{
		BridgeRefundBridgeHubRococoMessages, OnBridgeHubWococoBlobDispatcher,
		WithBridgeHubRococoMessageBridge,
	},
	constants::fee::WeightToFee,
	xcm_config::XcmRouter,
};
use bridge_runtime_common::{
	messages::{source::TargetHeaderChainAdapter, target::SourceHeaderChainAdapter},
	messages_xcm_extension::{XcmAsPlainPayload, XcmBlobMessageDispatch},
};
use parachains_common::{
	impls::DealWithFees, opaque, AccountId, Balance, BlockNumber, Hash, Header, Index, Signature,
	AVERAGE_ON_INITIALIZE_RATIO, HOURS, MAXIMUM_BLOCK_WEIGHT, NORMAL_DISPATCH_RATIO, SLOT_DURATION,
};
use xcm_executor::XcmExecutor;

/// The address format for describing accounts.
pub type Address = MultiAddress<AccountId, ()>;

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
	pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
	BridgeRejectObsoleteHeadersAndMessages,
	(BridgeRefundBridgeHubRococoMessages, BridgeRefundBridgeHubWococoMessages),
);

/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic =
	generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, SignedExtra>;

/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
	Runtime,
	Block,
	frame_system::ChainContext<Runtime>,
	Runtime,
	AllPalletsWithSystem,
>;

impl_opaque_keys! {
	pub struct SessionKeys {
		pub aura: Aura,
	}
}

#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: create_runtime_str!("bridge-hub-rococo"),
	impl_name: create_runtime_str!("bridge-hub-rococo"),
	authoring_version: 1,
	spec_version: 9420,
	impl_version: 0,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 3,
	state_version: 1,
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
	pub const SS58Prefix: u16 = 42;
}

// Configure FRAME pallets to include in runtime.

impl frame_system::Config for Runtime {
	/// The identifier used to distinguish between accounts.
	type AccountId = AccountId;
	/// The aggregated dispatch type that is available for extrinsics.
	type RuntimeCall = RuntimeCall;
	/// The lookup mechanism to get account ID from whatever is passed in dispatchers.
	type Lookup = AccountIdLookup<AccountId, ()>;
	/// The index type for storing how many extrinsics an account has signed.
	type Index = Index;
	/// The index type for blocks.
	type BlockNumber = BlockNumber;
	/// The type for hashing blocks and tries.
	type Hash = Hash;
	/// The hashing algorithm used.
	type Hashing = BlakeTwo256;
	/// The header type.
	type Header = generic::Header<BlockNumber, BlakeTwo256>;
	/// The ubiquitous event type.
	type RuntimeEvent = RuntimeEvent;
	/// The ubiquitous origin type.
	type RuntimeOrigin = RuntimeOrigin;
	/// Maximum number of block number to block hash mappings to keep (oldest pruned first).
	type BlockHashCount = BlockHashCount;
	/// Runtime version.
	type Version = Version;
	/// Converts a module to an index of this module in the runtime.
	type PalletInfo = PalletInfo;
	/// The data to be stored in an account.
	type AccountData = pallet_balances::AccountData<Balance>;
	/// What to do if a new account is created.
	type OnNewAccount = ();
	/// What to do if an account is fully reaped from the system.
	type OnKilledAccount = ();
	/// The weight of database operations that the runtime can invoke.
	type DbWeight = RocksDbWeight;
	/// The basic call filter to use in dispatchable.
	type BaseCallFilter = Everything;
	/// Weight information for the extrinsics of this pallet.
	type SystemWeightInfo = weights::frame_system::WeightInfo<Runtime>;
	/// Block & extrinsics weights: base values and limits.
	type BlockWeights = RuntimeBlockWeights;
	/// The maximum length of a block (in bytes).
	type BlockLength = RuntimeBlockLength;
	/// This is used as an identifier of the chain. 42 is the generic substrate prefix.
	type SS58Prefix = SS58Prefix;
	/// The action to take on a Runtime Upgrade
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
	/// The type for recording an account's balance.
	type Balance = Balance;
	type DustRemoval = ();
	/// The ubiquitous event type.
	type RuntimeEvent = RuntimeEvent;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type WeightInfo = weights::pallet_balances::WeightInfo<Runtime>;
	type MaxLocks = ConstU32<50>;
	type MaxReserves = ConstU32<50>;
	type ReserveIdentifier = [u8; 8];
	type RuntimeHoldReason = RuntimeHoldReason;
	type FreezeIdentifier = ();
	type MaxHolds = ConstU32<0>;
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
	type OperationalFeeMultiplier = ConstU8<5>;
	type WeightToFee = WeightToFee;
	type LengthToFee = ConstantMultiplier<Balance, TransactionByteFee>;
	type FeeMultiplierUpdate = SlowAdjustingFeeUpdate<Self>;
}

parameter_types! {
	pub const ReservedXcmpWeight: Weight = MAXIMUM_BLOCK_WEIGHT.saturating_div(4);
	pub const ReservedDmpWeight: Weight = MAXIMUM_BLOCK_WEIGHT.saturating_div(4);
}

impl cumulus_pallet_parachain_system::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OnSystemEvent = ();
	type SelfParaId = parachain_info::Pallet<Runtime>;
	type OutboundXcmpMessageSource = XcmpQueue;
	type DmpMessageHandler = DmpQueue;
	type ReservedDmpWeight = ReservedDmpWeight;
	type XcmpMessageHandler = XcmpQueue;
	type ReservedXcmpWeight = ReservedXcmpWeight;
	type CheckAssociatedRelayNumber = RelayNumberStrictlyIncreases;
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

pub const PERIOD: u32 = 6 * HOURS;
pub const OFFSET: u32 = 0;

impl pallet_session::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	// we don't have stash and controller, thus we don't need the convert as well.
	type ValidatorIdOf = pallet_collator_selection::IdentityCollator;
	type ShouldEndSession = pallet_session::PeriodicSessions<ConstU32<PERIOD>, ConstU32<OFFSET>>;
	type NextSessionRotation = pallet_session::PeriodicSessions<ConstU32<PERIOD>, ConstU32<OFFSET>>;
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
	type MaxCandidates = ConstU32<1000>;
	type MinCandidates = ConstU32<5>;
	type MaxInvulnerables = ConstU32<100>;
	// should be a multiple of session or things will get inconsistent
	type KickThreshold = ConstU32<PERIOD>;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type ValidatorIdOf = pallet_collator_selection::IdentityCollator;
	type ValidatorRegistration = Session;
	type WeightInfo = weights::pallet_collator_selection::WeightInfo<Runtime>;
}

parameter_types! {
	// One storage item; key size is 32; value is size 4+4+16+32 bytes = 56 bytes.
	pub const DepositBase: Balance = deposit(1, 88);
	// Additional storage item size of 32 bytes.
	pub const DepositFactor: Balance = deposit(0, 32);
}

impl pallet_multisig::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type Currency = Balances;
	type DepositBase = DepositBase;
	type DepositFactor = DepositFactor;
	type MaxSignatories = ConstU32<100>;
	type WeightInfo = weights::pallet_multisig::WeightInfo<Runtime>;
}

impl pallet_utility::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type PalletsOrigin = OriginCaller;
	type WeightInfo = weights::pallet_utility::WeightInfo<Runtime>;
}

// Add bridge pallets (GPA)

/// Add GRANDPA bridge pallet to track Wococo relay chain on Rococo BridgeHub
pub type BridgeGrandpaWococoInstance = pallet_bridge_grandpa::Instance1;
impl pallet_bridge_grandpa::Config<BridgeGrandpaWococoInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type BridgedChain = bp_wococo::Wococo;
	type MaxFreeMandatoryHeadersPerBlock = ConstU32<4>;
	type HeadersToKeep = RelayChainHeadersToKeep;
	type WeightInfo = weights::pallet_bridge_grandpa_bridge_wococo_grandpa::WeightInfo<Runtime>;
}

/// Add GRANDPA bridge pallet to track Rococo relay chain on Wococo BridgeHub
pub type BridgeGrandpaRococoInstance = pallet_bridge_grandpa::Instance2;
impl pallet_bridge_grandpa::Config<BridgeGrandpaRococoInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type BridgedChain = bp_rococo::Rococo;
	type MaxFreeMandatoryHeadersPerBlock = ConstU32<4>;
	type HeadersToKeep = RelayChainHeadersToKeep;
	type WeightInfo = weights::pallet_bridge_grandpa_bridge_rococo_grandpa::WeightInfo<Runtime>;
}

parameter_types! {
	pub const RelayChainHeadersToKeep: u32 = 1024;
	pub const ParachainHeadsToKeep: u32 = 64;
	pub const RelayerStakeLease: u32 = 8;

	pub const RococoBridgeParachainPalletName: &'static str = "Paras";
	pub const WococoBridgeParachainPalletName: &'static str = "Paras";
	pub const MaxRococoParaHeadDataSize: u32 = bp_rococo::MAX_NESTED_PARACHAIN_HEAD_DATA_SIZE;
	pub const MaxWococoParaHeadDataSize: u32 = bp_wococo::MAX_NESTED_PARACHAIN_HEAD_DATA_SIZE;

	pub storage DeliveryRewardInBalance: u64 = 1_000_000;
	pub storage RequiredStakeForStakeAndSlash: Balance = 1_000_000;

	pub const RelayerStakeReserveId: [u8; 8] = *b"brdgrlrs";
}

/// Add parachain bridge pallet to track Wococo bridge hub parachain
pub type BridgeParachainWococoInstance = pallet_bridge_parachains::Instance1;
impl pallet_bridge_parachains::Config<BridgeParachainWococoInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_bridge_parachains_bridge_parachains_bench_runtime_bridge_parachain_wococo_instance::WeightInfo<Runtime>;
	type BridgesGrandpaPalletInstance = BridgeGrandpaWococoInstance;
	type ParasPalletName = WococoBridgeParachainPalletName;
	type ParaStoredHeaderDataBuilder =
		SingleParaStoredHeaderDataBuilder<bp_bridge_hub_wococo::BridgeHubWococo>;
	type HeadsToKeep = ParachainHeadsToKeep;
	type MaxParaHeadDataSize = MaxWococoParaHeadDataSize;
}

/// Add parachain bridge pallet to track Rococo bridge hub parachain
pub type BridgeParachainRococoInstance = pallet_bridge_parachains::Instance2;
impl pallet_bridge_parachains::Config<BridgeParachainRococoInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_bridge_parachains_bridge_parachains_bench_runtime_bridge_parachain_rococo_instance::WeightInfo<Runtime>;
	type BridgesGrandpaPalletInstance = BridgeGrandpaRococoInstance;
	type ParasPalletName = RococoBridgeParachainPalletName;
	type ParaStoredHeaderDataBuilder =
		SingleParaStoredHeaderDataBuilder<bp_bridge_hub_rococo::BridgeHubRococo>;
	type HeadsToKeep = ParachainHeadsToKeep;
	type MaxParaHeadDataSize = MaxRococoParaHeadDataSize;
}

/// Add XCM messages support for BridgeHubRococo to support Rococo->Wococo XCM messages
pub type WithBridgeHubWococoMessagesInstance = pallet_bridge_messages::Instance1;
impl pallet_bridge_messages::Config<WithBridgeHubWococoMessagesInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_bridge_messages_bridge_messages_bench_runtime_with_bridge_hub_wococo_messages_instance::WeightInfo<Runtime>;
	type BridgedChainId = bridge_hub_rococo_config::BridgeHubWococoChainId;
	type ActiveOutboundLanes = bridge_hub_rococo_config::ActiveOutboundLanesToBridgeHubWococo;
	type MaxUnrewardedRelayerEntriesAtInboundLane =
		bridge_hub_rococo_config::MaxUnrewardedRelayerEntriesAtInboundLane;
	type MaxUnconfirmedMessagesAtInboundLane =
		bridge_hub_rococo_config::MaxUnconfirmedMessagesAtInboundLane;

	type MaximalOutboundPayloadSize =
		bridge_hub_rococo_config::ToBridgeHubWococoMaximalOutboundPayloadSize;
	type OutboundPayload = XcmAsPlainPayload;

	type InboundPayload = XcmAsPlainPayload;
	type InboundRelayer = AccountId;
	type DeliveryPayments = ();

	type TargetHeaderChain = TargetHeaderChainAdapter<WithBridgeHubWococoMessageBridge>;
	type LaneMessageVerifier = bridge_hub_rococo_config::ToBridgeHubWococoMessageVerifier;
	type DeliveryConfirmationPayments = pallet_bridge_relayers::DeliveryConfirmationPaymentsAdapter<
		Runtime,
		WithBridgeHubWococoMessagesInstance,
		DeliveryRewardInBalance,
	>;

	type SourceHeaderChain = SourceHeaderChainAdapter<WithBridgeHubWococoMessageBridge>;
	type MessageDispatch =
		XcmBlobMessageDispatch<OnBridgeHubRococoBlobDispatcher, Self::WeightInfo>;
}

/// Add XCM messages support for BridgeHubWococo to support Wococo->Rococo XCM messages
pub type WithBridgeHubRococoMessagesInstance = pallet_bridge_messages::Instance2;
impl pallet_bridge_messages::Config<WithBridgeHubRococoMessagesInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_bridge_messages_bridge_messages_bench_runtime_with_bridge_hub_rococo_messages_instance::WeightInfo<Runtime>;
	type BridgedChainId = bridge_hub_wococo_config::BridgeHubRococoChainId;
	type ActiveOutboundLanes = bridge_hub_wococo_config::ActiveOutboundLanesToBridgeHubRococo;
	type MaxUnrewardedRelayerEntriesAtInboundLane =
		bridge_hub_wococo_config::MaxUnrewardedRelayerEntriesAtInboundLane;
	type MaxUnconfirmedMessagesAtInboundLane =
		bridge_hub_wococo_config::MaxUnconfirmedMessagesAtInboundLane;

	type MaximalOutboundPayloadSize =
		bridge_hub_wococo_config::ToBridgeHubRococoMaximalOutboundPayloadSize;
	type OutboundPayload = XcmAsPlainPayload;

	type InboundPayload = XcmAsPlainPayload;
	type InboundRelayer = AccountId;
	type DeliveryPayments = ();

	type TargetHeaderChain = TargetHeaderChainAdapter<WithBridgeHubRococoMessageBridge>;
	type LaneMessageVerifier = bridge_hub_wococo_config::ToBridgeHubRococoMessageVerifier;
	type DeliveryConfirmationPayments = pallet_bridge_relayers::DeliveryConfirmationPaymentsAdapter<
		Runtime,
		WithBridgeHubRococoMessagesInstance,
		DeliveryRewardInBalance,
	>;

	type SourceHeaderChain = SourceHeaderChainAdapter<WithBridgeHubRococoMessageBridge>;
	type MessageDispatch =
		XcmBlobMessageDispatch<OnBridgeHubWococoBlobDispatcher, Self::WeightInfo>;
}

/// Allows collect and claim rewards for relayers
impl pallet_bridge_relayers::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Reward = Balance;
	type PaymentProcedure =
		bp_relayers::PayRewardFromAccount<pallet_balances::Pallet<Runtime>, AccountId>;
	type StakeAndSlash = pallet_bridge_relayers::StakeAndSlashNamed<
		AccountId,
		BlockNumber,
		Balances,
		RelayerStakeReserveId,
		RequiredStakeForStakeAndSlash,
		RelayerStakeLease,
	>;
	type WeightInfo = weights::pallet_bridge_relayers::WeightInfo<Runtime>;
}

// Create the runtime by composing the FRAME pallets that were previously configured.
construct_runtime!(
	pub enum Runtime where
		Block = Block,
		NodeBlock = opaque::Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		// System support stuff.
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>} = 0,
		ParachainSystem: cumulus_pallet_parachain_system::{
			Pallet, Call, Config, Storage, Inherent, Event<T>, ValidateUnsigned,
		} = 1,
		Timestamp: pallet_timestamp::{Pallet, Call, Storage, Inherent} = 2,
		ParachainInfo: parachain_info::{Pallet, Storage, Config} = 3,

		// Monetary stuff.
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>} = 10,
		TransactionPayment: pallet_transaction_payment::{Pallet, Storage, Event<T>} = 11,

		// Collator support. The order of these 4 are important and shall not change.
		Authorship: pallet_authorship::{Pallet, Storage} = 20,
		CollatorSelection: pallet_collator_selection::{Pallet, Call, Storage, Event<T>, Config<T>} = 21,
		Session: pallet_session::{Pallet, Call, Storage, Event, Config<T>} = 22,
		Aura: pallet_aura::{Pallet, Storage, Config<T>} = 23,
		AuraExt: cumulus_pallet_aura_ext::{Pallet, Storage, Config} = 24,

		// XCM helpers.
		XcmpQueue: cumulus_pallet_xcmp_queue::{Pallet, Call, Storage, Event<T>} = 30,
		PolkadotXcm: pallet_xcm::{Pallet, Call, Event<T>, Origin, Config} = 31,
		CumulusXcm: cumulus_pallet_xcm::{Pallet, Event<T>, Origin} = 32,
		DmpQueue: cumulus_pallet_dmp_queue::{Pallet, Call, Storage, Event<T>} = 33,

		// Handy utilities.
		Utility: pallet_utility::{Pallet, Call, Event} = 40,
		Multisig: pallet_multisig::{Pallet, Call, Storage, Event<T>} = 36,

		// Rococo and Wococo Bridge Hubs are sharing the runtime, so this runtime has two sets of
		// bridge pallets. Both are deployed at both runtimes, but only one set is actually used
		// at particular runtime.

		// With-Wococo bridge modules that are active (used) at Rococo Bridge Hub runtime.
		BridgeWococoGrandpa: pallet_bridge_grandpa::<Instance1>::{Pallet, Call, Storage, Event<T>, Config<T>} = 41,
		BridgeWococoParachain: pallet_bridge_parachains::<Instance1>::{Pallet, Call, Storage, Event<T>} = 42,
		BridgeWococoMessages: pallet_bridge_messages::<Instance1>::{Pallet, Call, Storage, Event<T>, Config<T>} = 46,

		// With-Rococo bridge modules that are active (used) at Wococo Bridge Hub runtime.
		BridgeRococoGrandpa: pallet_bridge_grandpa::<Instance2>::{Pallet, Call, Storage, Event<T>, Config<T>} = 43,
		BridgeRococoParachain: pallet_bridge_parachains::<Instance2>::{Pallet, Call, Storage, Event<T>} = 44,
		BridgeRococoMessages: pallet_bridge_messages::<Instance2>::{Pallet, Call, Storage, Event<T>, Config<T>} = 45,

		BridgeRelayers: pallet_bridge_relayers::{Pallet, Call, Storage, Event<T>} = 47,
	}
);

bridge_runtime_common::generate_bridge_reject_obsolete_headers_and_messages! {
	RuntimeCall, AccountId,
	// Grandpa
	BridgeRococoGrandpa, BridgeWococoGrandpa,
	// Parachains
	BridgeRococoParachain, BridgeWococoParachain,
	// Messages
	BridgeRococoMessages, BridgeWococoMessages
}

#[cfg(feature = "runtime-benchmarks")]
#[macro_use]
extern crate frame_benchmarking;

#[cfg(feature = "runtime-benchmarks")]
mod benches {
	define_benchmarks!(
		[frame_system, SystemBench::<Runtime>]
		[pallet_balances, Balances]
		[pallet_multisig, Multisig]
		[pallet_session, SessionBench::<Runtime>]
		[pallet_utility, Utility]
		[pallet_timestamp, Timestamp]
		[pallet_collator_selection, CollatorSelection]
		[cumulus_pallet_xcmp_queue, XcmpQueue]
		// XCM
		[pallet_xcm, PolkadotXcm]
		// NOTE: Make sure you point to the individual modules below.
		[pallet_xcm_benchmarks::fungible, XcmBalances]
		[pallet_xcm_benchmarks::generic, XcmGeneric]
		// Bridge pallets at Rococo
		[pallet_bridge_grandpa, BridgeWococoGrandpa]
		[pallet_bridge_parachains, BridgeParachainsBench::<Runtime, BridgeParachainWococoInstance>]
		[pallet_bridge_messages, BridgeMessagesBench::<Runtime, WithBridgeHubWococoMessagesInstance>]
		// Bridge pallets at Wococo
		[pallet_bridge_grandpa, BridgeRococoGrandpa]
		[pallet_bridge_parachains, BridgeParachainsBench::<Runtime, BridgeParachainRococoInstance>]
		[pallet_bridge_messages, BridgeMessagesBench::<Runtime, WithBridgeHubRococoMessagesInstance>]
		// Bridge relayer pallets
		[pallet_bridge_relayers, BridgeRelayersBench::<Runtime>]
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

	impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Index> for Runtime {
		fn account_nonce(account: AccountId) -> Index {
			System::account_nonce(account)
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

	impl cumulus_primitives_core::CollectCollationInfo<Block> for Runtime {
		fn collect_collation_info(header: &<Block as BlockT>::Header) -> cumulus_primitives_core::CollationInfo {
			ParachainSystem::collect_collation_info(header)
		}
	}

	impl bp_rococo::RococoFinalityApi<Block> for Runtime {
		fn best_finalized() -> Option<HeaderId<bp_rococo::Hash, bp_rococo::BlockNumber>> {
			BridgeRococoGrandpa::best_finalized()
		}
	}

	impl bp_wococo::WococoFinalityApi<Block> for Runtime {
		fn best_finalized() -> Option<HeaderId<bp_wococo::Hash, bp_wococo::BlockNumber>> {
			BridgeWococoGrandpa::best_finalized()
		}
	}


	impl bp_bridge_hub_rococo::BridgeHubRococoFinalityApi<Block> for Runtime {
		fn best_finalized() -> Option<HeaderId<Hash, BlockNumber>> {
			BridgeRococoParachain::best_parachain_head_id::<
				bp_bridge_hub_rococo::BridgeHubRococo
			>().unwrap_or(None)
		}
	}

	impl bp_bridge_hub_wococo::BridgeHubWococoFinalityApi<Block> for Runtime {
		fn best_finalized() -> Option<HeaderId<Hash, BlockNumber>> {
			BridgeWococoParachain::best_parachain_head_id::<
				bp_bridge_hub_wococo::BridgeHubWococo
			>().unwrap_or(None)
		}
	}

	// This exposed by BridgeHubRococo
	impl bp_bridge_hub_wococo::FromBridgeHubWococoInboundLaneApi<Block> for Runtime {
		fn message_details(
			lane: bp_messages::LaneId,
			messages: Vec<(bp_messages::MessagePayload, bp_messages::OutboundMessageDetails)>,
		) -> Vec<bp_messages::InboundMessageDetails> {
			bridge_runtime_common::messages_api::inbound_message_details::<
				Runtime,
				WithBridgeHubWococoMessagesInstance,
			>(lane, messages)
		}
	}

	// This exposed by BridgeHubRococo
	impl bp_bridge_hub_wococo::ToBridgeHubWococoOutboundLaneApi<Block> for Runtime {
		fn message_details(
			lane: bp_messages::LaneId,
			begin: bp_messages::MessageNonce,
			end: bp_messages::MessageNonce,
		) -> Vec<bp_messages::OutboundMessageDetails> {
			bridge_runtime_common::messages_api::outbound_message_details::<
				Runtime,
				WithBridgeHubWococoMessagesInstance,
			>(lane, begin, end)
		}
	}

	// This is exposed by BridgeHubWococo
	impl bp_bridge_hub_rococo::FromBridgeHubRococoInboundLaneApi<Block> for Runtime {
		fn message_details(
			lane: bp_messages::LaneId,
			messages: Vec<(bp_messages::MessagePayload, bp_messages::OutboundMessageDetails)>,
		) -> Vec<bp_messages::InboundMessageDetails> {
			bridge_runtime_common::messages_api::inbound_message_details::<
				Runtime,
				WithBridgeHubRococoMessagesInstance,
			>(lane, messages)
		}
	}

	// This is exposed by BridgeHubWococo
	impl bp_bridge_hub_rococo::ToBridgeHubRococoOutboundLaneApi<Block> for Runtime {
		fn message_details(
			lane: bp_messages::LaneId,
			begin: bp_messages::MessageNonce,
			end: bp_messages::MessageNonce,
		) -> Vec<bp_messages::OutboundMessageDetails> {
			bridge_runtime_common::messages_api::outbound_message_details::<
				Runtime,
				WithBridgeHubRococoMessagesInstance,
			>(lane, begin, end)
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

			use pallet_bridge_parachains::benchmarking::Pallet as BridgeParachainsBench;
			use pallet_bridge_messages::benchmarking::Pallet as BridgeMessagesBench;
			use pallet_bridge_relayers::benchmarking::Pallet as BridgeRelayersBench;

			let mut list = Vec::<BenchmarkList>::new();
			list_benchmarks!(list, extra);

			let storage_info = AllPalletsWithSystem::storage_info();
			(list, storage_info)
		}

		fn dispatch_benchmark(
			config: frame_benchmarking::BenchmarkConfig
		) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, sp_runtime::RuntimeString> {
			use frame_benchmarking::{Benchmarking, BenchmarkBatch, BenchmarkError, TrackedStorageKey};

			use frame_system_benchmarking::Pallet as SystemBench;
			impl frame_system_benchmarking::Config for Runtime {}

			use cumulus_pallet_session_benchmarking::Pallet as SessionBench;
			impl cumulus_pallet_session_benchmarking::Config for Runtime {}

			use xcm::latest::prelude::*;
			use xcm_config::RelayLocation;

			impl pallet_xcm_benchmarks::Config for Runtime {
				type XcmConfig = xcm_config::XcmConfig;
				type AccountIdConverter = xcm_config::LocationToAccountId;
				fn valid_destination() -> Result<MultiLocation, BenchmarkError> {
					Ok(RelayLocation::get())
				}
				fn worst_case_holding(_depositable_count: u32) -> MultiAssets {
					// just concrete assets according to relay chain.
					let assets: Vec<MultiAsset> = vec![
						MultiAsset {
							id: Concrete(RelayLocation::get()),
							fun: Fungible(1_000_000 * UNITS),
						}
					];
					assets.into()
				}
			}

			parameter_types! {
				pub const TrustedTeleporter: Option<(MultiLocation, MultiAsset)> = Some((
					RelayLocation::get(),
					MultiAsset { fun: Fungible(UNITS), id: Concrete(RelayLocation::get()) },
				));
				pub const CheckedAccount: Option<(AccountId, xcm_builder::MintLocation)> = None;
			}

			impl pallet_xcm_benchmarks::fungible::Config for Runtime {
				type TransactAsset = Balances;

				type CheckedAccount = CheckedAccount;
				type TrustedTeleporter = TrustedTeleporter;

				fn get_multi_asset() -> MultiAsset {
					MultiAsset {
						id: Concrete(RelayLocation::get()),
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
					Ok((RelayLocation::get(), frame_system::Call::remark_with_event { remark: vec![] }.into()))
				}

				fn subscribe_origin() -> Result<MultiLocation, BenchmarkError> {
					Ok(RelayLocation::get())
				}

				fn claimable_asset() -> Result<(MultiLocation, MultiLocation, MultiAssets), BenchmarkError> {
					let origin = RelayLocation::get();
					let assets: MultiAssets = (Concrete(RelayLocation::get()), 1_000 * UNITS).into();
					let ticket = MultiLocation { parents: 0, interior: Here };
					Ok((origin, ticket, assets))
				}

				fn unlockable_asset() -> Result<(MultiLocation, MultiLocation, MultiAsset), BenchmarkError> {
					Err(BenchmarkError::Skip)
				}

				fn export_message_origin_and_destination(
				) -> Result<(MultiLocation, NetworkId, InteriorMultiLocation), BenchmarkError> {
					Ok((RelayLocation::get(), NetworkId::Wococo, X1(Parachain(100))))
				}
			}

			type XcmBalances = pallet_xcm_benchmarks::fungible::Pallet::<Runtime>;
			type XcmGeneric = pallet_xcm_benchmarks::generic::Pallet::<Runtime>;

			use bridge_runtime_common::messages_benchmarking::{prepare_message_delivery_proof_from_parachain, prepare_message_proof_from_parachain};
			use pallet_bridge_messages::benchmarking::{
				Config as BridgeMessagesConfig,
				Pallet as BridgeMessagesBench,
				MessageDeliveryProofParams,
				MessageProofParams,
			};

			impl BridgeMessagesConfig<WithBridgeHubWococoMessagesInstance> for Runtime {
				fn is_relayer_rewarded(relayer: &Self::AccountId) -> bool {
					let bench_lane_id = <Self as BridgeMessagesConfig<WithBridgeHubWococoMessagesInstance>>::bench_lane_id();
					let bridged_chain_id = bp_runtime::BRIDGE_HUB_WOCOCO_CHAIN_ID;
					pallet_bridge_relayers::Pallet::<Runtime>::relayer_reward(
						relayer,
						bp_relayers::RewardsAccountParams::new(
							bench_lane_id,
							bridged_chain_id,
							bp_relayers::RewardsAccountOwner::BridgedChain
						)
					).is_some()
				}

				fn prepare_message_proof(
					params: MessageProofParams,
				) -> (bridge_hub_rococo_config::FromWococoBridgeHubMessagesProof, Weight) {
					use cumulus_primitives_core::XcmpMessageSource;
					assert!(XcmpQueue::take_outbound_messages(usize::MAX).is_empty());
					ParachainSystem::open_outbound_hrmp_channel_for_benchmarks(42.into());
					prepare_message_proof_from_parachain::<
						Runtime,
						BridgeGrandpaWococoInstance,
						bridge_hub_rococo_config::WithBridgeHubWococoMessageBridge,
					>(params, X2(GlobalConsensus(Rococo), Parachain(42)))
				}

				fn prepare_message_delivery_proof(
					params: MessageDeliveryProofParams<AccountId>,
				) -> bridge_hub_rococo_config::ToWococoBridgeHubMessagesDeliveryProof {
					prepare_message_delivery_proof_from_parachain::<
						Runtime,
						BridgeGrandpaWococoInstance,
						bridge_hub_rococo_config::WithBridgeHubWococoMessageBridge,
					>(params)
				}

				fn is_message_successfully_dispatched(_nonce: bp_messages::MessageNonce) -> bool {
					use cumulus_primitives_core::XcmpMessageSource;
					!XcmpQueue::take_outbound_messages(usize::MAX).is_empty()
				}
			}

			impl BridgeMessagesConfig<WithBridgeHubRococoMessagesInstance> for Runtime {
				fn is_relayer_rewarded(relayer: &Self::AccountId) -> bool {
					let bench_lane_id = <Self as BridgeMessagesConfig<WithBridgeHubRococoMessagesInstance>>::bench_lane_id();
					let bridged_chain_id = bp_runtime::BRIDGE_HUB_ROCOCO_CHAIN_ID;
					pallet_bridge_relayers::Pallet::<Runtime>::relayer_reward(
						relayer,
						bp_relayers::RewardsAccountParams::new(
							bench_lane_id,
							bridged_chain_id,
							bp_relayers::RewardsAccountOwner::BridgedChain
						)
					).is_some()
				}

				fn prepare_message_proof(
					params: MessageProofParams,
				) -> (bridge_hub_wococo_config::FromRococoBridgeHubMessagesProof, Weight) {
					use cumulus_primitives_core::XcmpMessageSource;
					assert!(XcmpQueue::take_outbound_messages(usize::MAX).is_empty());
					ParachainSystem::open_outbound_hrmp_channel_for_benchmarks(42.into());
					prepare_message_proof_from_parachain::<
						Runtime,
						BridgeGrandpaRococoInstance,
						bridge_hub_wococo_config::WithBridgeHubRococoMessageBridge,
					>(params, X2(GlobalConsensus(Wococo), Parachain(42)))
				}

				fn prepare_message_delivery_proof(
					params: MessageDeliveryProofParams<AccountId>,
				) -> bridge_hub_wococo_config::ToRococoBridgeHubMessagesDeliveryProof {
					prepare_message_delivery_proof_from_parachain::<
						Runtime,
						BridgeGrandpaRococoInstance,
						bridge_hub_wococo_config::WithBridgeHubRococoMessageBridge,
					>(params)
				}

				fn is_message_successfully_dispatched(_nonce: bp_messages::MessageNonce) -> bool {
					use cumulus_primitives_core::XcmpMessageSource;
					!XcmpQueue::take_outbound_messages(usize::MAX).is_empty()
				}
			}

			use bridge_runtime_common::parachains_benchmarking::prepare_parachain_heads_proof;
			use pallet_bridge_parachains::benchmarking::{
				Config as BridgeParachainsConfig,
				Pallet as BridgeParachainsBench,
			};
			use pallet_bridge_relayers::benchmarking::{
				Pallet as BridgeRelayersBench,
				Config as BridgeRelayersConfig,
			};

			impl BridgeParachainsConfig<BridgeParachainWococoInstance> for Runtime {
				fn parachains() -> Vec<bp_polkadot_core::parachains::ParaId> {
					use bp_runtime::Parachain;
					vec![bp_polkadot_core::parachains::ParaId(bp_bridge_hub_wococo::BridgeHubWococo::PARACHAIN_ID)]
				}

				fn prepare_parachain_heads_proof(
					parachains: &[bp_polkadot_core::parachains::ParaId],
					parachain_head_size: u32,
					proof_size: bp_runtime::StorageProofSize,
				) -> (
					pallet_bridge_parachains::RelayBlockNumber,
					pallet_bridge_parachains::RelayBlockHash,
					bp_polkadot_core::parachains::ParaHeadsProof,
					Vec<(bp_polkadot_core::parachains::ParaId, bp_polkadot_core::parachains::ParaHash)>,
				) {
					prepare_parachain_heads_proof::<Runtime, BridgeParachainWococoInstance>(
						parachains,
						parachain_head_size,
						proof_size,
					)
				}
			}

			impl BridgeParachainsConfig<BridgeParachainRococoInstance> for Runtime {
				fn parachains() -> Vec<bp_polkadot_core::parachains::ParaId> {
					use bp_runtime::Parachain;
					vec![bp_polkadot_core::parachains::ParaId(bp_bridge_hub_rococo::BridgeHubRococo::PARACHAIN_ID)]
				}

				fn prepare_parachain_heads_proof(
					parachains: &[bp_polkadot_core::parachains::ParaId],
					parachain_head_size: u32,
					proof_size: bp_runtime::StorageProofSize,
				) -> (
					pallet_bridge_parachains::RelayBlockNumber,
					pallet_bridge_parachains::RelayBlockHash,
					bp_polkadot_core::parachains::ParaHeadsProof,
					Vec<(bp_polkadot_core::parachains::ParaId, bp_polkadot_core::parachains::ParaHash)>,
				) {
					prepare_parachain_heads_proof::<Runtime, BridgeParachainRococoInstance>(
						parachains,
						parachain_head_size,
						proof_size,
					)
				}
			}

			impl BridgeRelayersConfig for Runtime {
				fn prepare_rewards_account(
					account_params: bp_relayers::RewardsAccountParams,
					reward: Balance,
				) {
					let rewards_account = bp_relayers::PayRewardFromAccount::<
						Balances,
						AccountId
					>::rewards_account(account_params);
					Self::deposit_account(rewards_account, reward);
				}

				fn deposit_account(account: AccountId, balance: Balance) {
					use frame_support::traits::fungible::Mutate;
					Balances::mint_into(&account, balance.saturating_add(ExistentialDeposit::get())).unwrap();
				}
			}

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
			];

			let mut batches = Vec::<BenchmarkBatch>::new();
			let params = (&config, &whitelist);
			add_benchmarks!(params, batches);

			Ok(batches)
		}
	}
}

struct CheckInherents;

impl cumulus_pallet_parachain_system::CheckInherents<Block> for CheckInherents {
	fn check_inherents(
		block: &Block,
		relay_state_proof: &cumulus_pallet_parachain_system::RelayChainStateProof,
	) -> sp_inherents::CheckInherentsResult {
		let relay_chain_slot = relay_state_proof
			.read_slot()
			.expect("Could not read the relay chain slot from the proof");

		let inherent_data =
			cumulus_primitives_timestamp::InherentDataProvider::from_relay_chain_slot_and_duration(
				relay_chain_slot,
				sp_std::time::Duration::from_secs(6),
			)
			.create_inherent_data()
			.expect("Could not create the timestamp inherent data");

		inherent_data.check_extrinsics(block)
	}
}

cumulus_pallet_parachain_system::register_validate_block! {
	Runtime = Runtime,
	BlockExecutor = cumulus_pallet_aura_ext::BlockExecutor::<Runtime, Executive>,
	CheckInherents = CheckInherents,
}

#[cfg(test)]
mod tests {
	use super::*;
	use bp_runtime::TransactionEra;
	use bridge_hub_test_utils::test_header;
	use codec::Encode;

	pub type TestBlockHeader =
		sp_runtime::generic::Header<bp_polkadot_core::BlockNumber, bp_polkadot_core::Hasher>;

	#[test]
	fn ensure_signed_extension_definition_is_compatible_with_relay() {
		let payload: SignedExtra = (
			frame_system::CheckNonZeroSender::new(),
			frame_system::CheckSpecVersion::new(),
			frame_system::CheckTxVersion::new(),
			frame_system::CheckGenesis::new(),
			frame_system::CheckEra::from(sp_runtime::generic::Era::Immortal),
			frame_system::CheckNonce::from(10),
			frame_system::CheckWeight::new(),
			pallet_transaction_payment::ChargeTransactionPayment::from(10),
			BridgeRejectObsoleteHeadersAndMessages {},
			(
				BridgeRefundBridgeHubRococoMessages::default(),
				BridgeRefundBridgeHubWococoMessages::default(),
			),
		);

		{
			use bp_bridge_hub_rococo::BridgeHubSignedExtension;
			let bhr_indirect_payload = bp_bridge_hub_rococo::SignedExtension::from_params(
				10,
				10,
				TransactionEra::Immortal,
				test_header::<TestBlockHeader>(1).hash(),
				10,
				10,
			);
			assert_eq!(payload.encode(), bhr_indirect_payload.encode());
		}

		{
			use bp_bridge_hub_wococo::BridgeHubSignedExtension;
			let bhw_indirect_payload = bp_bridge_hub_wococo::SignedExtension::from_params(
				10,
				10,
				TransactionEra::Immortal,
				test_header::<TestBlockHeader>(1).hash(),
				10,
				10,
			);
			assert_eq!(payload.encode(), bhw_indirect_payload.encode());
		}
	}
}
