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

//! # Bridge Hub Rococo Runtime
//!
//! This runtime currently supports bridging between:
//! - Rococo <> Westend
//! - Rococo <> Rococo Bulletin

#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit = "256"]

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

pub mod bridge_common_config;
pub mod bridge_to_bulletin_config;
pub mod bridge_to_ethereum_config;
pub mod bridge_to_westend_config;
mod weights;
pub mod xcm_config;

use cumulus_pallet_parachain_system::RelayNumberMonotonicallyIncreases;
use snowbridge_beacon_primitives::{Fork, ForkVersions};
use snowbridge_core::{
	gwei, meth, outbound::Message, AgentId, AllowSiblingsOnly, PricingParameters, Rewards,
};
use snowbridge_router_primitives::inbound::MessageToXcm;
use sp_api::impl_runtime_apis;
use sp_core::{crypto::KeyTypeId, OpaqueMetadata, H160};
use sp_runtime::{
	create_runtime_str, generic, impl_opaque_keys,
	traits::{Block as BlockT, Keccak256},
	transaction_validity::{TransactionSource, TransactionValidity},
	ApplyExtrinsicResult, FixedU128,
};

use sp_std::prelude::*;
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

use cumulus_primitives_core::ParaId;
use frame_support::{
	construct_runtime, derive_impl,
	dispatch::DispatchClass,
	genesis_builder_helper::{build_config, create_default_config},
	parameter_types,
	traits::{ConstBool, ConstU32, ConstU64, ConstU8, TransformOrigin},
	weights::{ConstantMultiplier, Weight},
	PalletId,
};
use frame_system::{
	limits::{BlockLength, BlockWeights},
	EnsureRoot,
};
use testnet_parachains_constants::rococo::{
	consensus::*, currency::*, fee::WeightToFee, snowbridge::INBOUND_QUEUE_PALLET_INDEX, time::*,
};

#[cfg(feature = "runtime-benchmarks")]
use bp_runtime::Chain;
use bp_runtime::HeaderId;
use bridge_hub_common::{
	message_queue::{NarrowOriginToSibling, ParaIdToSibling},
	AggregateMessageOrigin,
};
use pallet_xcm::EnsureXcm;
pub use sp_consensus_aura::sr25519::AuthorityId as AuraId;
pub use sp_runtime::{MultiAddress, Perbill, Permill};
use xcm::VersionedLocation;
use xcm_config::{TreasuryAccount, XcmOriginToTransactDispatchOrigin, XcmRouter};

#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;

use polkadot_runtime_common::{BlockHashCount, SlowAdjustingFeeUpdate};
use rococo_runtime_constants::system_parachain::{ASSET_HUB_ID, BRIDGE_HUB_ID};
use xcm::latest::prelude::*;

use weights::{BlockExecutionWeight, ExtrinsicBaseWeight, RocksDbWeight};

use parachains_common::{
	impls::DealWithFees, AccountId, Balance, BlockNumber, Hash, Header, Nonce, Signature,
	AVERAGE_ON_INITIALIZE_RATIO, NORMAL_DISPATCH_RATIO,
};

use polkadot_runtime_common::prod_or_fast;

#[cfg(feature = "runtime-benchmarks")]
use benchmark_helpers::DoNothingRouter;
#[cfg(not(feature = "runtime-benchmarks"))]
use bridge_hub_common::BridgeHubMessageRouter;

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
	(
		bridge_to_westend_config::OnBridgeHubRococoRefundBridgeHubWestendMessages,
		bridge_to_bulletin_config::OnBridgeHubRococoRefundRococoBulletinMessages,
	),
);

/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic =
	generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, SignedExtra>;

/// Migrations to apply on runtime upgrade.
pub type Migrations = (
	pallet_collator_selection::migration::v1::MigrateToV1<Runtime>,
	pallet_multisig::migrations::v1::MigrateToV1<Runtime>,
	InitStorageVersions,
	cumulus_pallet_xcmp_queue::migration::v4::MigrationToV4<Runtime>,
	// unreleased
	snowbridge_pallet_system::migration::v0::InitializeOnUpgrade<
		Runtime,
		ConstU32<BRIDGE_HUB_ID>,
		ConstU32<ASSET_HUB_ID>,
	>,
	// permanent
	pallet_xcm::migration::MigrateToLatestXcmVersion<Runtime>,
);

/// Migration to initialize storage versions for pallets added after genesis.
///
/// Ideally this would be done automatically (see
/// <https://github.com/paritytech/polkadot-sdk/pull/1297>), but it probably won't be ready for some
/// time and it's beneficial to get try-runtime-cli on-runtime-upgrade checks into the CI, so we're
/// doing it manually.
pub struct InitStorageVersions;

impl frame_support::traits::OnRuntimeUpgrade for InitStorageVersions {
	fn on_runtime_upgrade() -> Weight {
		use frame_support::traits::{GetStorageVersion, StorageVersion};
		use sp_runtime::traits::Saturating;

		let mut writes = 0;

		if PolkadotXcm::on_chain_storage_version() == StorageVersion::new(0) {
			PolkadotXcm::current_storage_version().put::<PolkadotXcm>();
			writes.saturating_inc();
		}

		if Balances::on_chain_storage_version() == StorageVersion::new(0) {
			Balances::current_storage_version().put::<Balances>();
			writes.saturating_inc();
		}

		<Runtime as frame_system::Config>::DbWeight::get().reads_writes(2, writes)
	}
}

/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
	Runtime,
	Block,
	frame_system::ChainContext<Runtime>,
	Runtime,
	AllPalletsWithSystem,
	Migrations,
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
	spec_version: 1_006_001,
	impl_version: 0,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 4,
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

#[derive_impl(frame_system::config_preludes::ParaChainDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
	/// The identifier used to distinguish between accounts.
	type AccountId = AccountId;
	/// The index type for storing how many extrinsics an account has signed.
	type Nonce = Nonce;
	/// The type for hashing blocks and tries.
	type Hash = Hash;
	/// The block type.
	type Block = Block;
	/// Maximum number of block number to block hash mappings to keep (oldest pruned first).
	type BlockHashCount = BlockHashCount;
	/// Runtime version.
	type Version = Version;
	/// The data to be stored in an account.
	type AccountData = pallet_balances::AccountData<Balance>;
	/// The weight of database operations that the runtime can invoke.
	type DbWeight = RocksDbWeight;
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
	#[cfg(feature = "experimental")]
	type MinimumPeriod = ConstU64<0>;
	#[cfg(not(feature = "experimental"))]
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
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type FreezeIdentifier = ();
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
	type WeightInfo = weights::cumulus_pallet_parachain_system::WeightInfo<Runtime>;
	type RuntimeEvent = RuntimeEvent;
	type OnSystemEvent = ();
	type SelfParaId = parachain_info::Pallet<Runtime>;
	type OutboundXcmpMessageSource = XcmpQueue;
	type DmpQueue = frame_support::traits::EnqueueWithOrigin<MessageQueue, RelayOrigin>;
	type ReservedDmpWeight = ReservedDmpWeight;
	type XcmpMessageHandler = XcmpQueue;
	type ReservedXcmpWeight = ReservedXcmpWeight;
	type CheckAssociatedRelayNumber = RelayNumberMonotonicallyIncreases;
	type ConsensusHook = ConsensusHook;
}

type ConsensusHook = cumulus_pallet_aura_ext::FixedVelocityConsensusHook<
	Runtime,
	RELAY_CHAIN_SLOT_DURATION_MILLIS,
	BLOCK_PROCESSING_VELOCITY,
	UNINCLUDED_SEGMENT_CAPACITY,
>;

impl parachain_info::Config for Runtime {}

parameter_types! {
	/// Amount of weight that can be spent per block to service messages. This was increased
	/// from 35% to 60% of the max block weight to accommodate the Ethereum beacon light client
	/// extrinsics. The force_checkpoint and submit extrinsics (for submit, optionally) includes
	/// the sync committee's pubkeys (512 x 48 bytes)
	pub MessageQueueServiceWeight: Weight = Perbill::from_percent(60) * RuntimeBlockWeights::get().max_block;
}

impl pallet_message_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_message_queue::WeightInfo<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type MessageProcessor =
		pallet_message_queue::mock_helpers::NoopMessageProcessor<AggregateMessageOrigin>;
	#[cfg(not(feature = "runtime-benchmarks"))]
	type MessageProcessor = BridgeHubMessageRouter<
		xcm_builder::ProcessXcmMessage<
			AggregateMessageOrigin,
			xcm_executor::XcmExecutor<xcm_config::XcmConfig>,
			RuntimeCall,
		>,
		EthereumOutboundQueue,
	>;
	type Size = u32;
	// The XCMP queue pallet is only ever able to handle the `Sibling(ParaId)` origin:
	type QueueChangeHandler = NarrowOriginToSibling<XcmpQueue>;
	type QueuePausedQuery = NarrowOriginToSibling<XcmpQueue>;
	type HeapSize = sp_core::ConstU32<{ 64 * 1024 }>;
	type MaxStale = sp_core::ConstU32<8>;
	type ServiceWeight = MessageQueueServiceWeight;
}

impl cumulus_pallet_aura_ext::Config for Runtime {}

parameter_types! {
	/// The asset ID for the asset that we use to pay for message delivery fees.
	pub FeeAssetId: AssetId = AssetId(xcm_config::TokenLocation::get());
	/// The base fee for the message delivery fees.
	pub const BaseDeliveryFee: u128 = CENTS.saturating_mul(3);
}

pub type PriceForSiblingParachainDelivery = polkadot_runtime_common::xcm_sender::ExponentialPrice<
	FeeAssetId,
	BaseDeliveryFee,
	TransactionByteFee,
	XcmpQueue,
>;

impl cumulus_pallet_xcmp_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ChannelInfo = ParachainSystem;
	type VersionWrapper = PolkadotXcm;
	// Enqueue XCMP messages from siblings for later processing.
	type XcmpQueue = TransformOrigin<MessageQueue, AggregateMessageOrigin, ParaId, ParaIdToSibling>;
	type MaxInboundSuspended = sp_core::ConstU32<1_000>;
	type ControllerOrigin = EnsureRoot<AccountId>;
	type ControllerOriginConverter = XcmOriginToTransactDispatchOrigin;
	type WeightInfo = weights::cumulus_pallet_xcmp_queue::WeightInfo<Runtime>;
	type PriceForSiblingDelivery = PriceForSiblingParachainDelivery;
}

parameter_types! {
	pub const RelayOrigin: AggregateMessageOrigin = AggregateMessageOrigin::Parent;
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
	type AllowMultipleBlocksPerSlot = ConstBool<true>;
	#[cfg(feature = "experimental")]
	type SlotDuration = ConstU64<SLOT_DURATION>;
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

// Ethereum Bridge
parameter_types! {
	pub storage EthereumGatewayAddress: H160 = H160(hex_literal::hex!("EDa338E4dC46038493b885327842fD3E301CaB39"));
}

parameter_types! {
	pub const CreateAssetCall: [u8;2] = [53, 0];
	pub const CreateAssetDeposit: u128 = (UNITS / 10) + EXISTENTIAL_DEPOSIT;
	pub Parameters: PricingParameters<u128> = PricingParameters {
		exchange_rate: FixedU128::from_rational(1, 400),
		fee_per_gas: gwei(20),
		rewards: Rewards { local: 1 * UNITS, remote: meth(1) }
	};
}

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmark_helpers {
	use crate::{EthereumBeaconClient, Runtime, RuntimeOrigin};
	use codec::Encode;
	use snowbridge_beacon_primitives::CompactExecutionHeader;
	use snowbridge_pallet_inbound_queue::BenchmarkHelper;
	use sp_core::H256;
	use xcm::latest::{Assets, Location, SendError, SendResult, SendXcm, Xcm, XcmHash};

	impl<T: snowbridge_pallet_ethereum_client::Config> BenchmarkHelper<T> for Runtime {
		fn initialize_storage(block_hash: H256, header: CompactExecutionHeader) {
			EthereumBeaconClient::store_execution_header(block_hash, header, 0, H256::default())
		}
	}

	pub struct DoNothingRouter;
	impl SendXcm for DoNothingRouter {
		type Ticket = Xcm<()>;

		fn validate(
			_dest: &mut Option<Location>,
			xcm: &mut Option<Xcm<()>>,
		) -> SendResult<Self::Ticket> {
			Ok((xcm.clone().unwrap(), Assets::new()))
		}
		fn deliver(xcm: Xcm<()>) -> Result<XcmHash, SendError> {
			let hash = xcm.using_encoded(sp_io::hashing::blake2_256);
			Ok(hash)
		}
	}

	impl snowbridge_pallet_system::BenchmarkHelper<RuntimeOrigin> for () {
		fn make_xcm_origin(location: Location) -> RuntimeOrigin {
			RuntimeOrigin::from(pallet_xcm::Origin::Xcm(location))
		}
	}
}

impl snowbridge_pallet_inbound_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Verifier = snowbridge_pallet_ethereum_client::Pallet<Runtime>;
	type Token = Balances;
	#[cfg(not(feature = "runtime-benchmarks"))]
	type XcmSender = XcmRouter;
	#[cfg(feature = "runtime-benchmarks")]
	type XcmSender = DoNothingRouter;
	type ChannelLookup = EthereumSystem;
	type GatewayAddress = EthereumGatewayAddress;
	#[cfg(feature = "runtime-benchmarks")]
	type Helper = Runtime;
	type MessageConverter = MessageToXcm<
		CreateAssetCall,
		CreateAssetDeposit,
		ConstU8<INBOUND_QUEUE_PALLET_INDEX>,
		AccountId,
		Balance,
	>;
	type WeightToFee = WeightToFee;
	type LengthToFee = ConstantMultiplier<Balance, TransactionByteFee>;
	type MaxMessageSize = ConstU32<2048>;
	type WeightInfo = weights::snowbridge_pallet_inbound_queue::WeightInfo<Runtime>;
	type PricingParameters = EthereumSystem;
	type AssetTransactor = <xcm_config::XcmConfig as xcm_executor::Config>::AssetTransactor;
}

impl snowbridge_pallet_outbound_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Hashing = Keccak256;
	type MessageQueue = MessageQueue;
	type Decimals = ConstU8<12>;
	type MaxMessagePayloadSize = ConstU32<2048>;
	type MaxMessagesPerBlock = ConstU32<32>;
	type GasMeter = snowbridge_core::outbound::ConstantGasMeter;
	type Balance = Balance;
	type WeightToFee = WeightToFee;
	type WeightInfo = weights::snowbridge_pallet_outbound_queue::WeightInfo<Runtime>;
	type PricingParameters = EthereumSystem;
	type Channels = EthereumSystem;
}

#[cfg(any(feature = "std", feature = "fast-runtime", feature = "runtime-benchmarks", test))]
parameter_types! {
	pub const ChainForkVersions: ForkVersions = ForkVersions {
		genesis: Fork {
			version: [0, 0, 0, 0], // 0x00000000
			epoch: 0,
		},
		altair: Fork {
			version: [1, 0, 0, 0], // 0x01000000
			epoch: 0,
		},
		bellatrix: Fork {
			version: [2, 0, 0, 0], // 0x02000000
			epoch: 0,
		},
		capella: Fork {
			version: [3, 0, 0, 0], // 0x03000000
			epoch: 0,
		},
		deneb: Fork {
			version: [4, 0, 0, 0], // 0x04000000
			epoch: 0,
		}
	};
}

#[cfg(not(any(feature = "std", feature = "fast-runtime", feature = "runtime-benchmarks", test)))]
parameter_types! {
	pub const ChainForkVersions: ForkVersions = ForkVersions {
		genesis: Fork {
			version: [144, 0, 0, 111], // 0x90000069
			epoch: 0,
		},
		altair: Fork {
			version: [144, 0, 0, 112], // 0x90000070
			epoch: 50,
		},
		bellatrix: Fork {
			version: [144, 0, 0, 113], // 0x90000071
			epoch: 100,
		},
		capella: Fork {
			version: [144, 0, 0, 114], // 0x90000072
			epoch: 56832,
		},
		deneb: Fork {
			version: [144, 0, 0, 115], // 0x90000073
			epoch: 132608,
		},
	};
}

parameter_types! {
	pub const MaxExecutionHeadersToKeep: u32 = prod_or_fast!(8192 * 2, 1000);
}

impl snowbridge_pallet_ethereum_client::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ForkVersions = ChainForkVersions;
	type MaxExecutionHeadersToKeep = MaxExecutionHeadersToKeep;
	type WeightInfo = weights::snowbridge_pallet_ethereum_client::WeightInfo<Runtime>;
}

impl snowbridge_pallet_system::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OutboundQueue = EthereumOutboundQueue;
	type SiblingOrigin = EnsureXcm<AllowSiblingsOnly>;
	type AgentIdOf = snowbridge_core::AgentIdOf;
	type TreasuryAccount = TreasuryAccount;
	type Token = Balances;
	type WeightInfo = weights::snowbridge_pallet_system::WeightInfo<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type Helper = ();
	type DefaultPricingParameters = Parameters;
	type InboundDeliveryCost = EthereumInboundQueue;
}

// Create the runtime by composing the FRAME pallets that were previously configured.
construct_runtime!(
	pub enum Runtime
	{
		// System support stuff.
		System: frame_system = 0,
		ParachainSystem: cumulus_pallet_parachain_system = 1,
		Timestamp: pallet_timestamp = 2,
		ParachainInfo: parachain_info = 3,

		// Monetary stuff.
		Balances: pallet_balances = 10,
		TransactionPayment: pallet_transaction_payment = 11,

		// Collator support. The order of these 4 are important and shall not change.
		Authorship: pallet_authorship = 20,
		CollatorSelection: pallet_collator_selection = 21,
		Session: pallet_session = 22,
		Aura: pallet_aura = 23,
		AuraExt: cumulus_pallet_aura_ext = 24,

		// XCM helpers.
		XcmpQueue: cumulus_pallet_xcmp_queue = 30,
		PolkadotXcm: pallet_xcm = 31,
		CumulusXcm: cumulus_pallet_xcm = 32,

		// Handy utilities.
		Utility: pallet_utility = 40,
		Multisig: pallet_multisig = 36,

		// Bridge relayers pallet, used by several bridges here.
		BridgeRelayers: pallet_bridge_relayers = 47,

		// With-Westend GRANDPA bridge module.
		BridgeWestendGrandpa: pallet_bridge_grandpa::<Instance3> = 48,
		// With-Westend parachain bridge module.
		BridgeWestendParachains: pallet_bridge_parachains::<Instance3> = 49,
		// With-Westend messaging bridge module.
		BridgeWestendMessages: pallet_bridge_messages::<Instance3> = 51,
		// With-Westend bridge hub pallet.
		XcmOverBridgeHubWestend: pallet_xcm_bridge_hub::<Instance1> = 52,

		// With-Rococo Bulletin GRANDPA bridge module.
		//
		// we can't use `BridgeRococoBulletinGrandpa` name here, because the same Bulletin runtime
		// will be used for both Rococo and Polkadot Bulletin chains AND this name affects runtime
		// storage keys, used by the relayer process.
		BridgePolkadotBulletinGrandpa: pallet_bridge_grandpa::<Instance4> = 60,
		// With-Rococo Bulletin messaging bridge module.
		//
		// we can't use `BridgeRococoBulletinMessages` name here, because the same Bulletin runtime
		// will be used for both Rococo and Polkadot Bulletin chains AND this name affects runtime
		// storage keys, used by this runtime and the relayer process.
		BridgePolkadotBulletinMessages: pallet_bridge_messages::<Instance4> = 61,
		// With-Rococo Bulletin bridge hub pallet.
		XcmOverPolkadotBulletin: pallet_xcm_bridge_hub::<Instance2> = 62,

		EthereumInboundQueue: snowbridge_pallet_inbound_queue = 80,
		EthereumOutboundQueue: snowbridge_pallet_outbound_queue = 81,
		EthereumBeaconClient: snowbridge_pallet_ethereum_client = 82,
		EthereumSystem: snowbridge_pallet_system = 83,

		// Message Queue. Importantly, is registered last so that messages are processed after
		// the `on_initialize` hooks of bridging pallets.
		MessageQueue: pallet_message_queue = 250,
	}
);

/// Proper alias for bridge GRANDPA pallet used to bridge with the bulletin chain.
pub type BridgeRococoBulletinGrandpa = BridgePolkadotBulletinGrandpa;
/// Proper alias for bridge messages pallet used to bridge with the bulletin chain.
pub type BridgeRococoBulletinMessages = BridgePolkadotBulletinMessages;
/// Proper alias for bridge messages pallet used to bridge with the bulletin chain.
pub type XcmOverRococoBulletin = XcmOverPolkadotBulletin;

bridge_runtime_common::generate_bridge_reject_obsolete_headers_and_messages! {
	RuntimeCall, AccountId,
	// Grandpa
	BridgeWestendGrandpa,
	BridgeRococoBulletinGrandpa,
	// Parachains
	BridgeWestendParachains,
	// Messages
	BridgeWestendMessages,
	BridgeRococoBulletinMessages
}

#[cfg(feature = "runtime-benchmarks")]
mod benches {
	frame_benchmarking::define_benchmarks!(
		[frame_system, SystemBench::<Runtime>]
		[pallet_balances, Balances]
		[pallet_message_queue, MessageQueue]
		[pallet_multisig, Multisig]
		[pallet_session, SessionBench::<Runtime>]
		[pallet_utility, Utility]
		[pallet_timestamp, Timestamp]
		[pallet_collator_selection, CollatorSelection]
		[cumulus_pallet_parachain_system, ParachainSystem]
		[cumulus_pallet_xcmp_queue, XcmpQueue]
		// XCM
		[pallet_xcm, PalletXcmExtrinsicsBenchmark::<Runtime>]
		// NOTE: Make sure you point to the individual modules below.
		[pallet_xcm_benchmarks::fungible, XcmBalances]
		[pallet_xcm_benchmarks::generic, XcmGeneric]
		// Bridge pallets
		[pallet_bridge_grandpa, WestendFinality]
		[pallet_bridge_parachains, WithinWestend]
		[pallet_bridge_messages, RococoToWestend]
		[pallet_bridge_messages, RococoToRococoBulletin]
		[pallet_bridge_relayers, BridgeRelayersBench::<Runtime>]
		// Ethereum Bridge
		[snowbridge_pallet_inbound_queue, EthereumInboundQueue]
		[snowbridge_pallet_outbound_queue, EthereumOutboundQueue]
		[snowbridge_pallet_system, EthereumSystem]
		[snowbridge_pallet_ethereum_client, EthereumBeaconClient]
	);
}

impl_runtime_apis! {
	impl sp_consensus_aura::AuraApi<Block, AuraId> for Runtime {
		fn slot_duration() -> sp_consensus_aura::SlotDuration {
			sp_consensus_aura::SlotDuration::from_millis(SLOT_DURATION)
		}

		fn authorities() -> Vec<AuraId> {
			Aura::authorities().into_inner()
		}
	}

	impl cumulus_primitives_aura::AuraUnincludedSegmentApi<Block> for Runtime {
		fn can_build_upon(
			included_hash: <Block as BlockT>::Hash,
			slot: cumulus_primitives_aura::Slot,
		) -> bool {
			ConsensusHook::can_build_upon(included_hash, slot)
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

	impl cumulus_primitives_core::CollectCollationInfo<Block> for Runtime {
		fn collect_collation_info(header: &<Block as BlockT>::Header) -> cumulus_primitives_core::CollationInfo {
			ParachainSystem::collect_collation_info(header)
		}
	}

	impl bp_westend::WestendFinalityApi<Block> for Runtime {
		fn best_finalized() -> Option<HeaderId<bp_westend::Hash, bp_westend::BlockNumber>> {
			BridgeWestendGrandpa::best_finalized()
		}
		fn synced_headers_grandpa_info(
		) -> Vec<bp_header_chain::StoredHeaderGrandpaInfo<bp_westend::Header>> {
			BridgeWestendGrandpa::synced_headers_grandpa_info()
		}
	}

	impl bp_bridge_hub_westend::BridgeHubWestendFinalityApi<Block> for Runtime {
		fn best_finalized() -> Option<HeaderId<Hash, BlockNumber>> {
			BridgeWestendParachains::best_parachain_head_id::<
				bp_bridge_hub_westend::BridgeHubWestend
			>().unwrap_or(None)
		}
	}

	// This is exposed by BridgeHubRococo
	impl bp_bridge_hub_westend::FromBridgeHubWestendInboundLaneApi<Block> for Runtime {
		fn message_details(
			lane: bp_messages::LaneId,
			messages: Vec<(bp_messages::MessagePayload, bp_messages::OutboundMessageDetails)>,
		) -> Vec<bp_messages::InboundMessageDetails> {
			bridge_runtime_common::messages_api::inbound_message_details::<
				Runtime,
				bridge_to_westend_config::WithBridgeHubWestendMessagesInstance,
			>(lane, messages)
		}
	}

	// This is exposed by BridgeHubRococo
	impl bp_bridge_hub_westend::ToBridgeHubWestendOutboundLaneApi<Block> for Runtime {
		fn message_details(
			lane: bp_messages::LaneId,
			begin: bp_messages::MessageNonce,
			end: bp_messages::MessageNonce,
		) -> Vec<bp_messages::OutboundMessageDetails> {
			bridge_runtime_common::messages_api::outbound_message_details::<
				Runtime,
				bridge_to_westend_config::WithBridgeHubWestendMessagesInstance,
			>(lane, begin, end)
		}
	}

	impl bp_polkadot_bulletin::PolkadotBulletinFinalityApi<Block> for Runtime {
		fn best_finalized() -> Option<bp_runtime::HeaderId<bp_polkadot_bulletin::Hash, bp_polkadot_bulletin::BlockNumber>> {
			BridgePolkadotBulletinGrandpa::best_finalized()
		}

		fn synced_headers_grandpa_info(
		) -> Vec<bp_header_chain::StoredHeaderGrandpaInfo<bp_polkadot_bulletin::Header>> {
			BridgePolkadotBulletinGrandpa::synced_headers_grandpa_info()
		}
	}

	impl bp_polkadot_bulletin::FromPolkadotBulletinInboundLaneApi<Block> for Runtime {
		fn message_details(
			lane: bp_messages::LaneId,
			messages: Vec<(bp_messages::MessagePayload, bp_messages::OutboundMessageDetails)>,
		) -> Vec<bp_messages::InboundMessageDetails> {
			bridge_runtime_common::messages_api::inbound_message_details::<
				Runtime,
				bridge_to_bulletin_config::WithRococoBulletinMessagesInstance,
			>(lane, messages)
		}
	}

	impl bp_polkadot_bulletin::ToPolkadotBulletinOutboundLaneApi<Block> for Runtime {
		fn message_details(
			lane: bp_messages::LaneId,
			begin: bp_messages::MessageNonce,
			end: bp_messages::MessageNonce,
		) -> Vec<bp_messages::OutboundMessageDetails> {
			bridge_runtime_common::messages_api::outbound_message_details::<
				Runtime,
				bridge_to_bulletin_config::WithRococoBulletinMessagesInstance,
			>(lane, begin, end)
		}
	}

	impl snowbridge_outbound_queue_runtime_api::OutboundQueueApi<Block, Balance> for Runtime {
		fn prove_message(leaf_index: u64) -> Option<snowbridge_pallet_outbound_queue::MerkleProof> {
			snowbridge_pallet_outbound_queue::api::prove_message::<Runtime>(leaf_index)
		}

		fn calculate_fee(message: Message) -> Option<Balance> {
			snowbridge_pallet_outbound_queue::api::calculate_fee::<Runtime>(message)
		}
	}

	impl snowbridge_system_runtime_api::ControlApi<Block> for Runtime {
		fn agent_id(location: VersionedLocation) -> Option<AgentId> {
			snowbridge_pallet_system::api::agent_id::<Runtime>(location)
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
			use pallet_xcm::benchmarking::Pallet as PalletXcmExtrinsicsBenchmark;

			// This is defined once again in dispatch_benchmark, because list_benchmarks!
			// and add_benchmarks! are macros exported by define_benchmarks! macros and those types
			// are referenced in that call.
			type XcmBalances = pallet_xcm_benchmarks::fungible::Pallet::<Runtime>;
			type XcmGeneric = pallet_xcm_benchmarks::generic::Pallet::<Runtime>;

			use pallet_bridge_relayers::benchmarking::Pallet as BridgeRelayersBench;
			// Change weight file names.
			type WestendFinality = BridgeWestendGrandpa;
			type WithinWestend = pallet_bridge_parachains::benchmarking::Pallet::<Runtime, bridge_common_config::BridgeParachainWestendInstance>;
			type RococoToWestend = pallet_bridge_messages::benchmarking::Pallet ::<Runtime, bridge_to_westend_config::WithBridgeHubWestendMessagesInstance>;
			type RococoToRococoBulletin = pallet_bridge_messages::benchmarking::Pallet ::<Runtime, bridge_to_bulletin_config::WithRococoBulletinMessagesInstance>;

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

			use pallet_xcm::benchmarking::Pallet as PalletXcmExtrinsicsBenchmark;
			impl pallet_xcm::benchmarking::Config for Runtime {
				fn reachable_dest() -> Option<Location> {
					Some(Parent.into())
				}

				fn teleportable_asset_and_dest() -> Option<(Asset, Location)> {
					// Relay/native token can be teleported between BH and Relay.
					Some((
						Asset {
							fun: Fungible(EXISTENTIAL_DEPOSIT),
							id: AssetId(Parent.into())
						},
						Parent.into(),
					))
				}

				fn reserve_transferable_asset_and_dest() -> Option<(Asset, Location)> {
					// Reserve transfers are disabled on BH.
					None
				}

				fn set_up_complex_asset_transfer(
				) -> Option<(Assets, u32, Location, Box<dyn FnOnce()>)> {
					// BH only supports teleports to system parachain.
					// Relay/native token can be teleported between BH and Relay.
					let native_location = Parent.into();
					let dest = Parent.into();
					pallet_xcm::benchmarking::helpers::native_teleport_as_asset_transfer::<Runtime>(
						native_location,
						dest
					)
				}
			}

			use xcm::latest::prelude::*;
			use xcm_config::TokenLocation;

			parameter_types! {
				pub ExistentialDepositAsset: Option<Asset> = Some((
					TokenLocation::get(),
					ExistentialDeposit::get()
				).into());
			}

			impl pallet_xcm_benchmarks::Config for Runtime {
				type XcmConfig = xcm_config::XcmConfig;
				type AccountIdConverter = xcm_config::LocationToAccountId;
				type DeliveryHelper = cumulus_primitives_utility::ToParentDeliveryHelper<
					xcm_config::XcmConfig,
					ExistentialDepositAsset,
					xcm_config::PriceForParentDelivery,
				>;
				fn valid_destination() -> Result<Location, BenchmarkError> {
					Ok(TokenLocation::get())
				}
				fn worst_case_holding(_depositable_count: u32) -> Assets {
					// just concrete assets according to relay chain.
					let assets: Vec<Asset> = vec![
						Asset {
							id: AssetId(TokenLocation::get()),
							fun: Fungible(1_000_000 * UNITS),
						}
					];
					assets.into()
				}
			}

			parameter_types! {
				pub const TrustedTeleporter: Option<(Location, Asset)> = Some((
					TokenLocation::get(),
					Asset { fun: Fungible(UNITS), id: AssetId(TokenLocation::get()) },
				));
				pub const CheckedAccount: Option<(AccountId, xcm_builder::MintLocation)> = None;
				pub const TrustedReserve: Option<(Location, Asset)> = None;
			}

			impl pallet_xcm_benchmarks::fungible::Config for Runtime {
				type TransactAsset = Balances;

				type CheckedAccount = CheckedAccount;
				type TrustedTeleporter = TrustedTeleporter;
				type TrustedReserve = TrustedReserve;

				fn get_asset() -> Asset {
					Asset {
						id: AssetId(TokenLocation::get()),
						fun: Fungible(UNITS),
					}
				}
			}

			impl pallet_xcm_benchmarks::generic::Config for Runtime {
				type TransactAsset = Balances;
				type RuntimeCall = RuntimeCall;

				fn worst_case_response() -> (u64, Response) {
					(0u64, Response::Version(Default::default()))
				}

				fn worst_case_asset_exchange() -> Result<(Assets, Assets), BenchmarkError> {
					Err(BenchmarkError::Skip)
				}

				fn universal_alias() -> Result<(Location, Junction), BenchmarkError> {
					Err(BenchmarkError::Skip)
				}

				fn transact_origin_and_runtime_call() -> Result<(Location, RuntimeCall), BenchmarkError> {
					Ok((TokenLocation::get(), frame_system::Call::remark_with_event { remark: vec![] }.into()))
				}

				fn subscribe_origin() -> Result<Location, BenchmarkError> {
					Ok(TokenLocation::get())
				}

				fn claimable_asset() -> Result<(Location, Location, Assets), BenchmarkError> {
					let origin = TokenLocation::get();
					let assets: Assets = (AssetId(TokenLocation::get()), 1_000 * UNITS).into();
					let ticket = Location { parents: 0, interior: Here };
					Ok((origin, ticket, assets))
				}

				fn fee_asset() -> Result<Asset, BenchmarkError> {
					Ok(Asset {
						id: AssetId(TokenLocation::get()),
						fun: Fungible(1_000_000 * UNITS),
					})
				}

				fn unlockable_asset() -> Result<(Location, Location, Asset), BenchmarkError> {
					Err(BenchmarkError::Skip)
				}

				fn export_message_origin_and_destination(
				) -> Result<(Location, NetworkId, InteriorLocation), BenchmarkError> {
					// save XCM version for remote bridge hub
					let _ = PolkadotXcm::force_xcm_version(
						RuntimeOrigin::root(),
						Box::new(bridge_to_westend_config::BridgeHubWestendLocation::get()),
						XCM_VERSION,
					).map_err(|e| {
						log::error!(
							"Failed to dispatch `force_xcm_version({:?}, {:?}, {:?})`, error: {:?}",
							RuntimeOrigin::root(),
							bridge_to_westend_config::BridgeHubWestendLocation::get(),
							XCM_VERSION,
							e
						);
						BenchmarkError::Stop("XcmVersion was not stored!")
					})?;
					Ok(
						(
							bridge_to_westend_config::FromAssetHubRococoToAssetHubWestendRoute::get().location,
							NetworkId::Westend,
							[Parachain(bridge_to_westend_config::AssetHubWestendParaId::get().into())].into()
						)
					)
				}

				fn alias_origin() -> Result<(Location, Location), BenchmarkError> {
					Err(BenchmarkError::Skip)
				}
			}

			type XcmBalances = pallet_xcm_benchmarks::fungible::Pallet::<Runtime>;
			type XcmGeneric = pallet_xcm_benchmarks::generic::Pallet::<Runtime>;

			type WestendFinality = BridgeWestendGrandpa;
			type WithinWestend = pallet_bridge_parachains::benchmarking::Pallet::<Runtime, bridge_common_config::BridgeParachainWestendInstance>;
			type RococoToWestend = pallet_bridge_messages::benchmarking::Pallet ::<Runtime, bridge_to_westend_config::WithBridgeHubWestendMessagesInstance>;
			type RococoToRococoBulletin = pallet_bridge_messages::benchmarking::Pallet ::<Runtime, bridge_to_bulletin_config::WithRococoBulletinMessagesInstance>;

			use bridge_runtime_common::messages_benchmarking::{
				prepare_message_delivery_proof_from_grandpa_chain,
				prepare_message_delivery_proof_from_parachain,
				prepare_message_proof_from_grandpa_chain,
				prepare_message_proof_from_parachain,
				generate_xcm_builder_bridge_message_sample,
			};
			use pallet_bridge_messages::benchmarking::{
				Config as BridgeMessagesConfig,
				MessageDeliveryProofParams,
				MessageProofParams,
			};

			impl BridgeMessagesConfig<bridge_to_westend_config::WithBridgeHubWestendMessagesInstance> for Runtime {
				fn is_relayer_rewarded(relayer: &Self::AccountId) -> bool {
					let bench_lane_id = <Self as BridgeMessagesConfig<bridge_to_westend_config::WithBridgeHubWestendMessagesInstance>>::bench_lane_id();
					let bridged_chain_id = bp_bridge_hub_westend::BridgeHubWestend::ID;
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
				) -> (bridge_to_westend_config::FromWestendBridgeHubMessagesProof, Weight) {
					use cumulus_primitives_core::XcmpMessageSource;
					assert!(XcmpQueue::take_outbound_messages(usize::MAX).is_empty());
					ParachainSystem::open_outbound_hrmp_channel_for_benchmarks_or_tests(42.into());
					prepare_message_proof_from_parachain::<
						Runtime,
						bridge_common_config::BridgeGrandpaWestendInstance,
						bridge_to_westend_config::WithBridgeHubWestendMessageBridge,
					>(params, generate_xcm_builder_bridge_message_sample([GlobalConsensus(Rococo), Parachain(42)].into()))
				}

				fn prepare_message_delivery_proof(
					params: MessageDeliveryProofParams<AccountId>,
				) -> bridge_to_westend_config::ToWestendBridgeHubMessagesDeliveryProof {
					prepare_message_delivery_proof_from_parachain::<
						Runtime,
						bridge_common_config::BridgeGrandpaWestendInstance,
						bridge_to_westend_config::WithBridgeHubWestendMessageBridge,
					>(params)
				}

				fn is_message_successfully_dispatched(_nonce: bp_messages::MessageNonce) -> bool {
					use cumulus_primitives_core::XcmpMessageSource;
					!XcmpQueue::take_outbound_messages(usize::MAX).is_empty()
				}
			}

			impl BridgeMessagesConfig<bridge_to_bulletin_config::WithRococoBulletinMessagesInstance> for Runtime {
				fn is_relayer_rewarded(_relayer: &Self::AccountId) -> bool {
					// we do not pay any rewards in this bridge
					true
				}

				fn prepare_message_proof(
					params: MessageProofParams,
				) -> (bridge_to_bulletin_config::FromRococoBulletinMessagesProof, Weight) {
					use cumulus_primitives_core::XcmpMessageSource;
					assert!(XcmpQueue::take_outbound_messages(usize::MAX).is_empty());
					ParachainSystem::open_outbound_hrmp_channel_for_benchmarks_or_tests(42.into());
					prepare_message_proof_from_grandpa_chain::<
						Runtime,
						bridge_common_config::BridgeGrandpaRococoBulletinInstance,
						bridge_to_bulletin_config::WithRococoBulletinMessageBridge,
					>(params, generate_xcm_builder_bridge_message_sample([GlobalConsensus(Rococo), Parachain(42)].into()))
				}

				fn prepare_message_delivery_proof(
					params: MessageDeliveryProofParams<AccountId>,
				) -> bridge_to_bulletin_config::ToRococoBulletinMessagesDeliveryProof {
					prepare_message_delivery_proof_from_grandpa_chain::<
						Runtime,
						bridge_common_config::BridgeGrandpaRococoBulletinInstance,
						bridge_to_bulletin_config::WithRococoBulletinMessageBridge,
					>(params)
				}

				fn is_message_successfully_dispatched(_nonce: bp_messages::MessageNonce) -> bool {
					use cumulus_primitives_core::XcmpMessageSource;
					!XcmpQueue::take_outbound_messages(usize::MAX).is_empty()
				}
			}

			use bridge_runtime_common::parachains_benchmarking::prepare_parachain_heads_proof;
			use pallet_bridge_parachains::benchmarking::Config as BridgeParachainsConfig;
			use pallet_bridge_relayers::benchmarking::{
				Pallet as BridgeRelayersBench,
				Config as BridgeRelayersConfig,
			};

			impl BridgeParachainsConfig<bridge_common_config::BridgeParachainWestendInstance> for Runtime {
				fn parachains() -> Vec<bp_polkadot_core::parachains::ParaId> {
					use bp_runtime::Parachain;
					vec![bp_polkadot_core::parachains::ParaId(bp_bridge_hub_westend::BridgeHubWestend::PARACHAIN_ID)]
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
					prepare_parachain_heads_proof::<Runtime, bridge_common_config::BridgeParachainWestendInstance>(
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

	impl sp_genesis_builder::GenesisBuilder<Block> for Runtime {
		fn create_default_config() -> Vec<u8> {
			create_default_config::<RuntimeGenesisConfig>()
		}

		fn build_config(config: Vec<u8>) -> sp_genesis_builder::Result {
			build_config::<RuntimeGenesisConfig>(config)
		}
	}
}

cumulus_pallet_parachain_system::register_validate_block! {
	Runtime = Runtime,
	BlockExecutor = cumulus_pallet_aura_ext::BlockExecutor::<Runtime, Executive>,
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;
	use sp_runtime::{
		generic::Era,
		traits::{SignedExtension, Zero},
	};

	#[test]
	fn ensure_signed_extension_definition_is_compatible_with_relay() {
		use bp_polkadot_core::SuffixedCommonSignedExtensionExt;

		sp_io::TestExternalities::default().execute_with(|| {
			frame_system::BlockHash::<Runtime>::insert(BlockNumber::zero(), Hash::default());
			let payload: SignedExtra = (
				frame_system::CheckNonZeroSender::new(),
				frame_system::CheckSpecVersion::new(),
				frame_system::CheckTxVersion::new(),
				frame_system::CheckGenesis::new(),
				frame_system::CheckEra::from(Era::Immortal),
				frame_system::CheckNonce::from(10),
				frame_system::CheckWeight::new(),
				pallet_transaction_payment::ChargeTransactionPayment::from(10),
				BridgeRejectObsoleteHeadersAndMessages,
				(
					bridge_to_westend_config::OnBridgeHubRococoRefundBridgeHubWestendMessages::default(),
					bridge_to_bulletin_config::OnBridgeHubRococoRefundRococoBulletinMessages::default(),
				)
			);

			// for BridgeHubRococo
			{
				let bhr_indirect_payload = bp_bridge_hub_rococo::SignedExtension::from_params(
					VERSION.spec_version,
					VERSION.transaction_version,
					bp_runtime::TransactionEra::Immortal,
					System::block_hash(BlockNumber::zero()),
					10,
					10,
					(((), ()), ((), ())),
				);
				assert_eq!(payload.encode(), bhr_indirect_payload.encode());
				assert_eq!(
					payload.additional_signed().unwrap().encode(),
					bhr_indirect_payload.additional_signed().unwrap().encode()
				)
			}
		});
	}
}
