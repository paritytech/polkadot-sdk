//! # Cumulus Enabled Parachain
//!
//! By the end of this guide, you will run a Cumulus-based runtime parachain locally using
//! [Zombienet](https://github.com/paritytech/zombienet) and deploy it on Rococo. You will convert
//! the Currency FRAME pallet covered in [your_first_pallet] into a Cumulus-based runtime with XCM
//! support and learn about the most important Cumulus and Substrate pallets required to set up a
//! parachain.
//!
//! ## Topics Covered
//!
//! The following topics are covered in this guide:
//!
//! > TODO(gpestana)
//!
//! ## Convert a FRAME runtime into a Cumulus-based runtime
//!
//! Our goal is to convert the Currency pallet's runtime built in the [`your_first_pallet`] guide
//! into a Cumulus runtime so that we can deploy the pallet's logic in a parachain. The bulk of this
//! exercise consists of configuring a Cumulus runtime that has the main parachain system pallets
//! required. The final Cumulus runtime can then be easily deployed and registered on a relay-chain,
//! and a few other helpful pallets that enable XCM messaging between the parachain and the
//! relay-chain.
//!
//! The parachain runtime must use a set of parachain system pallets provided by Cumulus and
//! substrate, namely:
//!
//! - []()
//!
//! In addition, we will cover how to define the weight limits for the parachain blocks, parachain
//! runtime APIs and other useful and/or necessary configurations.
//!
//! First, we will define the types and constants that will be used by the runtime.
//!
//! TODO(gpestana): types
#![doc = docify::embed!("./src/guides/cumulus_enabled_parachain/mod.rs", opaque_types)]
//!
//! TODO(gpestana): consts
#![doc = docify::embed!("./src/guides/cumulus_enabled_parachain/mod.rs", consts)]
//!
//! TODO(gpestana): runtime version
#![doc = docify::embed!("./src/guides/cumulus_enabled_parachain/mod.rs", runtime_version)]
//!
//! **Opaque types**: These are used by the CLI to instantiate machinery that don't need to know the
//! specifics of the runtime. They can then be made to be agnostic over specific formats
//! of data like extrinsics, allowing for them to continue syncing the network through upgrades to
//! even the core data structures.
#![doc = docify::embed!("./src/guides/cumulus_enabled_parachain/mod.rs", opaque_types)]
//!
//! ## Pallets
#![doc = docify::embed!("./src/guides/cumulus_enabled_parachain/mod.rs", pallet_system)]
//!
//! ## Runtime APIs
//!
//! We need to configure the runtime APIs.. TODO(gpestana)
#![doc = docify::embed!("./src/guides/cumulus_enabled_parachain/mod.rs", runtime_apis)]
//!
//! ## Runtime Weights
//!
//! TODO(gpestana): WeightToFee
#![doc = docify::embed!("./src/guides/cumulus_enabled_parachain/mod.rs", weight_to_fee)]
//!
//! TODO(gpestana): weights
#![doc = docify::embed!("./src/guides/cumulus_enabled_parachain/mod.rs", weights)]
//!
//! ## XCM Config
//!
//! The XCM barrier ... TODO(gpestana)
#![doc = docify::embed!("./src/guides/cumulus_enabled_parachain/mod.rs", xcm_barrier)]
//!
//! ## CLI chain-spec generator
//!
//! ## Local deployment with Zombienet
//!
//! ## Deployment on Rococo

#[cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#[recursion_limit = "256"]
// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

use cumulus_pallet_parachain_system::RelayNumberStrictlyIncreases;
use polkadot_runtime_common::xcm_sender::NoPriceForMessageDelivery;
use sp_api::impl_runtime_apis;
use sp_core::{crypto::KeyTypeId, OpaqueMetadata};
use sp_runtime::{
	create_runtime_str,
	curve::PiecewiseLinear,
	generic, impl_opaque_keys,
	traits::{AccountIdLookup, BlakeTwo256, Block as BlockT, IdentifyAccount, Verify},
	transaction_validity::{TransactionPriority, TransactionSource, TransactionValidity},
	ApplyExtrinsicResult, MultiSignature, Percent,
};

use sp_std::prelude::*;
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

use cumulus_primitives_core::{AggregateMessageOrigin, ParaId};
use frame_support::{
	construct_runtime,
	dispatch::DispatchClass,
	genesis_builder_helper::{build_config, create_default_config},
	parameter_types,
	traits::{
		ConstBool, ConstU32, ConstU64, ConstU8, EitherOfDiverse, Everything, TransformOrigin,
	},
	weights::{
		constants::WEIGHT_REF_TIME_PER_SECOND, ConstantMultiplier, Weight, WeightToFeeCoefficient,
		WeightToFeeCoefficients, WeightToFeePolynomial,
	},
	PalletId,
};

use frame::{
	deps::{frame_executive, frame_support},
	prelude::{
		frame_system::{
			limits::{BlockLength, BlockWeights},
			EnsureRoot,
		},
		*,
	},
};
use pallet_xcm::{EnsureXcm, IsVoiceOfBody};
use parachains_common::{
	message_queue::{NarrowOriginToSibling, ParaIdToSibling},
	rococo::currency::deposit,
	wococo::currency::UNITS,
};
pub use sp_consensus_aura::sr25519::AuthorityId as AuraId;
pub use sp_runtime::{MultiAddress, Perbill, Permill};
use xcm_config::{RelayLocation, XcmOriginToTransactDispatchOrigin};

#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;

// Polkadot imports
use polkadot_runtime_common::{BlockHashCount, SlowAdjustingFeeUpdate};

use weights::{BlockExecutionWeight, ExtrinsicBaseWeight, RocksDbWeight};

// XCM Imports
use xcm::latest::prelude::BodyId;

use consts::*;
use types::*;

#[docify::export(types)]
pub mod types {
	use super::*;

	/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
	pub type Signature = MultiSignature;

	/// Some way of identifying an account on the chain. We intentionally make it equivalent
	/// to the public key of our transaction signing scheme.
	pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

	/// Balance of an account.
	pub type Balance = u128;

	/// Index of a transaction in the chain.
	pub type Nonce = u32;

	/// A hash of some data used by the chain.
	pub type Hash = sp_core::H256;

	/// An index to a block.
	pub type BlockNumber = u32;

	/// The address format for describing accounts.
	pub type Address = MultiAddress<AccountId, ()>;

	/// Block header type as expected by this runtime.
	pub type Header = generic::Header<BlockNumber, BlakeTwo256>;

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
}

#[docify::export(opaque_types)]
pub mod opaque {
	use super::*;
	use sp_runtime::{
		generic,
		traits::{BlakeTwo256, Hash as HashT},
	};

	pub use sp_runtime::OpaqueExtrinsic as UncheckedExtrinsic;
	/// Opaque block header type.
	pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
	/// Opaque block type.
	pub type Block = generic::Block<Header, UncheckedExtrinsic>;
	/// Opaque block identifier type.
	pub type BlockId = generic::BlockId<Block>;
	/// Opaque block hash type.
	pub type Hash = <BlakeTwo256 as HashT>::Output;
}

impl_opaque_keys! {
	pub struct SessionKeys {
		pub aura: Aura,
	}
}

/// Handles converting a weight scalar to a fee value, based on the scale and granularity of the
/// node's balance type.
///
/// This should typically create a mapping between the following ranges:
///   - `[0, MAXIMUM_BLOCK_WEIGHT]`
///   - `[Balance::min, Balance::max]`
///
/// Yet, it can be used for any other sort of change to weight-fee. Some examples being:
///   - Setting it to `0` will essentially disable the weight fee.
///   - Setting it to `1` will cause the literal `#[weight = x]` values to be charged.
#[docify::export(weight_to_fee)]
pub struct WeightToFee;
impl WeightToFeePolynomial for WeightToFee {
	type Balance = Balance;
	fn polynomial() -> WeightToFeeCoefficients<Self::Balance> {
		// in Rococo, extrinsic base weight (smallest non-zero weight) is mapped to 1 MILLIUNIT:
		// in our template, we map to 1/10 of that, or 1/10 MILLIUNIT
		let p = MILLIUNIT / 10;
		let q = 100 * Balance::from(ExtrinsicBaseWeight::get().ref_time());
		vec![WeightToFeeCoefficient {
			degree: 1,
			negative: false,
			coeff_frac: Perbill::from_rational(p % q, q),
			coeff_integer: p / q,
		}]
	}
}

#[docify::export(runtime_version)]
#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: create_runtime_str!("staking-parachain"),
	impl_name: create_runtime_str!("staking-parachain"),
	authoring_version: 1,
	spec_version: 1,
	impl_version: 0,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 1,
	state_version: 1,
};

#[docify::export(consts)]
pub mod consts {
	use super::*;

	/// This determines the average expected block time that we are targeting.
	/// Blocks will be produced at a minimum duration defined by `SLOT_DURATION`.
	/// `SLOT_DURATION` is picked up by `pallet_timestamp` which is in turn picked
	/// up by `pallet_aura` to implement `fn slot_duration()`.
	///
	/// Change this to adjust the block time.
	pub const MILLISECS_PER_BLOCK: u64 = 12000;

	// NOTE: Currently it is not possible to change the slot duration after the chain has started.
	//       Attempting to do so will brick block production.
	pub const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;

	pub const EPOCH_DURATION_IN_SLOTS: BlockNumber = 1 * HOURS;

	// Time is measured by number of blocks.
	pub const MINUTES: BlockNumber = 60_000 / (MILLISECS_PER_BLOCK as BlockNumber);
	pub const HOURS: BlockNumber = MINUTES * 60;
	pub const DAYS: BlockNumber = HOURS * 24;

	// Unit = the base number of indivisible units for balances
	pub const UNIT: Balance = 1_000_000_000_000;
	pub const MILLIUNIT: Balance = 1_000_000_000;
	pub const MICROUNIT: Balance = 1_000_000;

	/// The existential deposit. Set to 1/10 of the Connected Relay Chain.
	pub const EXISTENTIAL_DEPOSIT: Balance = MILLIUNIT;

	/// We assume that ~5% of the block weight is consumed by `on_initialize` handlers. This is
	/// used to limit the maximal weight of a single extrinsic.
	pub(crate) const AVERAGE_ON_INITIALIZE_RATIO: Perbill = Perbill::from_percent(5);

	/// We allow `Normal` extrinsics to fill up the block up to 75%, the rest can be used by
	/// `Operational` extrinsics.
	pub(crate) const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

	/// We allow for 0.5 of a second of compute with a 12 second average block time.
	pub(crate) const MAXIMUM_BLOCK_WEIGHT: Weight = Weight::from_parts(
		WEIGHT_REF_TIME_PER_SECOND.saturating_div(2),
		cumulus_primitives_core::relay_chain::MAX_POV_SIZE as u64,
	);

	/// Maximum number of blocks simultaneously accepted by the Runtime, not yet included
	/// into the relay chain.
	pub(crate) const UNINCLUDED_SEGMENT_CAPACITY: u32 = 1;
	/// How many parachain blocks are processed by the relay chain per parent. Limits the
	/// number of blocks authored per slot.
	pub(crate) const BLOCK_PROCESSING_VELOCITY: u32 = 1;
	/// Relay chain slot duration, in milliseconds.
	pub(crate) const RELAY_CHAIN_SLOT_DURATION_MILLIS: u32 = 6000;
}

/// The version information used to identify this runtime when compiled natively.
#[cfg(feature = "std")]
pub fn native_version() -> NativeVersion {
	NativeVersion { runtime_version: VERSION, can_author_with: Default::default() }
}

parameter_types! {
	pub const Version: RuntimeVersion = VERSION;
	// This part is copied from Substrate's `bin/node/runtime/src/lib.rs`.
	//  The `RuntimeBlockLength` and `RuntimeBlockWeights` exist here because the
	// `DeletionWeightLimit` and `DeletionQueueDepth` depend on those to parameterize
	// the lazy contract deletion.
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

#[docify::export(pallet_system)]
impl frame_system::Config for Runtime {
	type AccountId = AccountId;
	type RuntimeCall = RuntimeCall;
	type Lookup = AccountIdLookup<AccountId, ()>;
	type Nonce = Nonce;
	type Hash = Hash;
	type Hashing = BlakeTwo256;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeTask = ();
	type BlockHashCount = BlockHashCount;
	type Version = Version;
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type DbWeight = RocksDbWeight;
	type BaseCallFilter = Everything;
	type SystemWeightInfo = ();
	type BlockWeights = RuntimeBlockWeights;
	type BlockLength = RuntimeBlockLength;
	type SS58Prefix = SS58Prefix;
	type OnSetCode = cumulus_pallet_parachain_system::ParachainSetCode<Self>;
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

impl pallet_timestamp::Config for Runtime {
	/// A timestamp: milliseconds since the unix epoch.
	type Moment = u64;
	type OnTimestampSet = Aura;
	type MinimumPeriod = ConstU64<{ SLOT_DURATION / 2 }>;
	type WeightInfo = ();
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
	type WeightInfo = pallet_balances::weights::SubstrateWeight<Runtime>;
	type MaxReserves = ConstU32<50>;
	type ReserveIdentifier = [u8; 8];
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type FreezeIdentifier = RuntimeFreezeReason;
	type MaxHolds = ConstU32<0>;
	type MaxFreezes = ConstU32<1>;
}

parameter_types! {
	/// Relay Chain `TransactionByteFee` / 10
	pub const TransactionByteFee: Balance = 10 * MICROUNIT;
}

impl pallet_transaction_payment::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OnChargeTransaction = pallet_transaction_payment::CurrencyAdapter<Balances, ()>;
	type WeightToFee = WeightToFee;
	type LengthToFee = ConstantMultiplier<Balance, TransactionByteFee>;
	type FeeMultiplierUpdate = SlowAdjustingFeeUpdate<Self>;
	type OperationalFeeMultiplier = ConstU8<5>;
}

impl pallet_sudo::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type WeightInfo = ();
}

parameter_types! {
	pub const ReservedXcmpWeight: Weight = MAXIMUM_BLOCK_WEIGHT.saturating_div(4);
	pub const ReservedDmpWeight: Weight = MAXIMUM_BLOCK_WEIGHT.saturating_div(4);
	pub const RelayOrigin: AggregateMessageOrigin = AggregateMessageOrigin::Parent;
}

impl cumulus_pallet_parachain_system::Config for Runtime {
	type WeightInfo = ();
	type RuntimeEvent = RuntimeEvent;
	type OnSystemEvent = ();
	type SelfParaId = parachain_info::Pallet<Runtime>;
	type OutboundXcmpMessageSource = XcmpQueue;
	type DmpQueue = frame_support::traits::EnqueueWithOrigin<MessageQueue, RelayOrigin>;
	type ReservedDmpWeight = ReservedDmpWeight;
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

parameter_types! {
	pub MessageQueueServiceWeight: Weight = Perbill::from_percent(35) * RuntimeBlockWeights::get().max_block;
}

impl pallet_message_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type MessageProcessor = pallet_message_queue::mock_helpers::NoopMessageProcessor<
		cumulus_primitives_core::AggregateMessageOrigin,
	>;
	#[cfg(not(feature = "runtime-benchmarks"))]
	type MessageProcessor = xcm_builder::ProcessXcmMessage<
		AggregateMessageOrigin,
		xcm_executor::XcmExecutor<xcm_config::XcmConfig>,
		RuntimeCall,
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

impl cumulus_pallet_xcmp_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ChannelInfo = ParachainSystem;
	type VersionWrapper = ();
	// Enqueue XCMP messages from siblings for later processing.
	type XcmpQueue = TransformOrigin<MessageQueue, AggregateMessageOrigin, ParaId, ParaIdToSibling>;
	type MaxInboundSuspended = sp_core::ConstU32<1_000>;
	type ControllerOrigin = EnsureRoot<AccountId>;
	type ControllerOriginConverter = XcmOriginToTransactDispatchOrigin;
	type WeightInfo = ();
	type PriceForSiblingDelivery = NoPriceForMessageDelivery<ParaId>;
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
	type WeightInfo = ();
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
	// StakingAdmin pluralistic body.
	pub const StakingAdminBodyId: BodyId = BodyId::Defense;
}

/// We allow root and the StakingAdmin to execute privileged collator selection operations.
pub type CollatorSelectionUpdateOrigin = EitherOfDiverse<
	EnsureRoot<AccountId>,
	EnsureXcm<IsVoiceOfBody<RelayLocation, StakingAdminBodyId>>,
>;

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
	type WeightInfo = ();
}

parameter_types! {
	// TODO(gpestana)
}

// TODO(gpestana): your_pallet

// Create the runtime by composing the FRAME pallets that were previously configured.
#[docify::export(construct_runtime)]
construct_runtime!(
	pub struct Runtime {
		// System support stuff.
		System: frame_system = 0,
		ParachainSystem: cumulus_pallet_parachain_system = 1,
		Timestamp: pallet_timestamp = 2,
		ParachainInfo: parachain_info = 3,

		// Monetary stuff.
		Balances: pallet_balances = 10,
		TransactionPayment: pallet_transaction_payment = 11,

		// Governance
		Sudo: pallet_sudo = 15,

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
		MessageQueue: pallet_message_queue = 33,

		// Your Pallet.
		// TODO(gpestana)
	}
);

#[docify::export(runtime_apis)]
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

			let mut list = Vec::<BenchmarkList>::new();
			list_benchmarks!(list, extra);

			let storage_info = AllPalletsWithSystem::storage_info();
			(list, storage_info)
		}

		fn dispatch_benchmark(
			config: frame_benchmarking::BenchmarkConfig
		) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, sp_runtime::RuntimeString> {
			use frame_benchmarking::{BenchmarkError, Benchmarking, BenchmarkBatch};

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

			use frame_support::traits::WhitelistedStorageKeys;
			let whitelist = AllPalletsWithSystem::whitelisted_storage_keys();

			let mut batches = Vec::<BenchmarkBatch>::new();
			let params = (&config, &whitelist);
			add_benchmarks!(params, batches);

			if batches.is_empty() { return Err("Benchmark not found for this pallet.".into()) }
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

pub mod xcm_config {
	use super::{
		frame_system::EnsureRoot, AccountId, AllPalletsWithSystem, Balances, ParachainInfo,
		ParachainSystem, PolkadotXcm, Runtime, RuntimeCall, RuntimeEvent, RuntimeOrigin,
		WeightToFee, XcmpQueue,
	};
	use frame_support::{
		match_types, parameter_types,
		traits::{ConstU32, Everything, Nothing},
		weights::Weight,
	};
	use pallet_xcm::XcmPassthrough;
	use polkadot_parachain_primitives::primitives::Sibling;
	use polkadot_runtime_common::impls::ToAuthor;
	use xcm::latest::prelude::*;
	use xcm_builder::{
		AccountId32Aliases, AllowExplicitUnpaidExecutionFrom, AllowTopLevelPaidExecutionFrom,
		CurrencyAdapter, DenyReserveTransferToRelayChain, DenyThenTry, EnsureXcmOrigin,
		FixedWeightBounds, IsConcrete, NativeAsset, ParentIsPreset, RelayChainAsNative,
		SiblingParachainAsNative, SiblingParachainConvertsVia, SignedAccountId32AsNative,
		SignedToAccountId32, SovereignSignedViaLocation, TakeWeightCredit, TrailingSetTopicAsId,
		UsingComponents, WithComputedOrigin, WithUniqueTopic,
	};
	use xcm_executor::XcmExecutor;

	parameter_types! {
		pub const RelayLocation: MultiLocation = MultiLocation::parent();
		pub const RelayNetwork: Option<NetworkId> = None;
		pub RelayChainOrigin: RuntimeOrigin = cumulus_pallet_xcm::Origin::Relay.into();
		pub UniversalLocation: InteriorMultiLocation = Parachain(ParachainInfo::parachain_id().into()).into();
	}

	/// Type for specifying how a `MultiLocation` can be converted into an `AccountId`. This is used
	/// when determining ownership of accounts for asset transacting and when attempting to use XCM
	/// `Transact` in order to determine the dispatch Origin.
	pub type LocationToAccountId = (
		// The parent (Relay-chain) origin converts to the parent `AccountId`.
		ParentIsPreset<AccountId>,
		// Sibling parachain origins convert to AccountId via the `ParaId::into`.
		SiblingParachainConvertsVia<Sibling, AccountId>,
		// Straight up local `AccountId32` origins just alias directly to `AccountId`.
		AccountId32Aliases<RelayNetwork, AccountId>,
	);

	/// Means for transacting assets on this chain.
	pub type LocalAssetTransactor = CurrencyAdapter<
		// Use this currency:
		Balances,
		// Use this currency when it is a fungible asset matching the given location or name:
		IsConcrete<RelayLocation>,
		// Do a simple punn to convert an AccountId32 MultiLocation into a native chain account ID:
		LocationToAccountId,
		// Our chain's account ID type (we can't get away without mentioning it explicitly):
		AccountId,
		// We don't track any teleports.
		(),
	>;

	/// This is the type we use to convert an (incoming) XCM origin into a local `Origin` instance,
	/// ready for dispatching a transaction with Xcm's `Transact`. There is an `OriginKind` which
	/// can biases the kind of local `Origin` it will become.
	pub type XcmOriginToTransactDispatchOrigin = (
		// Sovereign account converter; this attempts to derive an `AccountId` from the origin
		// location using `LocationToAccountId` and then turn that into the usual `Signed` origin.
		// Useful for foreign chains who want to have a local sovereign account on this chain which
		// they control.
		SovereignSignedViaLocation<LocationToAccountId, RuntimeOrigin>,
		// Native converter for Relay-chain (Parent) location; will convert to a `Relay` origin
		// when recognized.
		RelayChainAsNative<RelayChainOrigin, RuntimeOrigin>,
		// Native converter for sibling Parachains; will convert to a `SiblingPara` origin when
		// recognized.
		SiblingParachainAsNative<cumulus_pallet_xcm::Origin, RuntimeOrigin>,
		// Native signed account converter; this just converts an `AccountId32` origin into a
		// normal `RuntimeOrigin::Signed` origin of the same 32-byte value.
		SignedAccountId32AsNative<RelayNetwork, RuntimeOrigin>,
		// Xcm origins can be represented natively under the Xcm pallet's Xcm origin.
		XcmPassthrough<RuntimeOrigin>,
	);

	parameter_types! {
		// One XCM operation is 1_000_000_000 weight - almost certainly a conservative estimate.
		pub UnitWeightCost: Weight = Weight::from_parts(1_000_000_000, 64 * 1024);
		pub const MaxInstructions: u32 = 100;
		pub const MaxAssetsIntoHolding: u32 = 64;
	}

	match_types! {
		pub type ParentOrParentsExecutivePlurality: impl Contains<MultiLocation> = {
			MultiLocation { parents: 1, interior: Here } |
			MultiLocation { parents: 1, interior: X1(Plurality { id: BodyId::Executive, .. }) }
		};
	}

	#[docify::export(xcm_barrier)]
	pub type Barrier = TrailingSetTopicAsId<
		DenyThenTry<
			DenyReserveTransferToRelayChain,
			(
				TakeWeightCredit,
				WithComputedOrigin<
					(
						AllowTopLevelPaidExecutionFrom<Everything>,
						AllowExplicitUnpaidExecutionFrom<ParentOrParentsExecutivePlurality>,
						// ^^^ Parent and its exec plurality get free execution
					),
					UniversalLocation,
					ConstU32<8>,
				>,
			),
		>,
	>;

	pub struct XcmConfig;
	impl xcm_executor::Config for XcmConfig {
		type RuntimeCall = RuntimeCall;
		type XcmSender = XcmRouter;
		// How to withdraw and deposit an asset.
		type AssetTransactor = LocalAssetTransactor;
		type OriginConverter = XcmOriginToTransactDispatchOrigin;
		type IsReserve = NativeAsset;
		type IsTeleporter = (); // Teleporting is disabled.
		type UniversalLocation = UniversalLocation;
		type Barrier = Barrier;
		type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
		type Trader =
			UsingComponents<WeightToFee, RelayLocation, AccountId, Balances, ToAuthor<Runtime>>;
		type ResponseHandler = PolkadotXcm;
		type AssetTrap = PolkadotXcm;
		type AssetClaims = PolkadotXcm;
		type SubscriptionService = PolkadotXcm;
		type PalletInstancesInfo = AllPalletsWithSystem;
		type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
		type AssetLocker = ();
		type AssetExchanger = ();
		type FeeManager = ();
		type MessageExporter = ();
		type UniversalAliases = Nothing;
		type CallDispatcher = RuntimeCall;
		type SafeCallFilter = Everything;
		type Aliasers = Nothing;
	}

	/// No local origins on this chain are allowed to dispatch XCM sends/executions.
	pub type LocalOriginToLocation = SignedToAccountId32<RuntimeOrigin, AccountId, RelayNetwork>;

	/// The means for routing XCM messages which are not for local execution into the right message
	/// queues.
	pub type XcmRouter = WithUniqueTopic<(
		// Two routers - use UMP to communicate with the relay chain:
		cumulus_primitives_utility::ParentAsUmp<ParachainSystem, (), ()>,
		// ..and XCMP to communicate with the sibling chains.
		XcmpQueue,
	)>;

	impl pallet_xcm::Config for Runtime {
		type RuntimeEvent = RuntimeEvent;
		type SendXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
		type XcmRouter = XcmRouter;
		type ExecuteXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
		type XcmExecuteFilter = Nothing;
		// ^ Disable dispatchable execute on the XCM pallet.
		// Needs to be `Everything` for local testing.
		type XcmExecutor = XcmExecutor<XcmConfig>;
		type XcmTeleportFilter = Everything;
		type XcmReserveTransferFilter = Nothing;
		type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
		type UniversalLocation = UniversalLocation;
		type RuntimeOrigin = RuntimeOrigin;
		type RuntimeCall = RuntimeCall;

		const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 100;
		// ^ Override for AdvertisedXcmVersion default
		type AdvertisedXcmVersion = pallet_xcm::CurrentXcmVersion;
		type Currency = Balances;
		type CurrencyMatcher = ();
		type TrustedLockers = ();
		type SovereignAccountOf = LocationToAccountId;
		type MaxLockers = ConstU32<8>;
		type WeightInfo = pallet_xcm::TestWeightInfo;
		type AdminOrigin = EnsureRoot<AccountId>;
		type MaxRemoteLockConsumers = ConstU32<0>;
		type RemoteLockConsumerIdentifier = ();
	}

	impl cumulus_pallet_xcm::Config for Runtime {
		type RuntimeEvent = RuntimeEvent;
		type XcmExecutor = XcmExecutor<XcmConfig>;
	}
}

#[docify::export(weights)]
pub mod weights {
	use frame_support::{
		parameter_types,
		weights::{constants, RuntimeDbWeight, Weight},
	};

	// Block weights
	parameter_types! {
		/// Importing a block with 0 Extrinsics.
		pub const BlockExecutionWeight: Weight =
			Weight::from_parts(constants::WEIGHT_REF_TIME_PER_NANOS.saturating_mul(5_000_000), 0);
	}

	// Extrinsic weights
	parameter_types! {
		/// Executing a NO-OP `System::remarks` Extrinsic.
		pub const ExtrinsicBaseWeight: Weight =
			Weight::from_parts(constants::WEIGHT_REF_TIME_PER_NANOS.saturating_mul(125_000), 0);
	}

	// ParityDb weights
	parameter_types! {
		/// `ParityDB` can be enabled with a feature flag, but is still experimental. These weights
		/// are available for brave runtime engineers who may want to try this out as default.
		pub const ParityDbWeight: RuntimeDbWeight = RuntimeDbWeight {
			read: 8_000 * constants::WEIGHT_REF_TIME_PER_NANOS,
			write: 50_000 * constants::WEIGHT_REF_TIME_PER_NANOS,
		};
	}

	// RocksDb weights
	parameter_types! {
		/// By default, Substrate uses `RocksDB`, so this will be the weight used throughout
		/// the runtime.
		pub const RocksDbWeight: RuntimeDbWeight = RuntimeDbWeight {
			read: 25_000 * constants::WEIGHT_REF_TIME_PER_NANOS,
			write: 100_000 * constants::WEIGHT_REF_TIME_PER_NANOS,
		};
	}
}
