// This is free and unencumbered software released into the public domain.
//
// Anyone is free to copy, modify, publish, use, compile, sell, or
// distribute this software, either in source code form or as a compiled
// binary, for any purpose, commercial or non-commercial, and by any
// means.
//
// In jurisdictions that recognize copyright laws, the author or authors
// of this software dedicate any and all copyright interest in the
// software to the public domain. We make this dedication for the benefit
// of the public at large and to the detriment of our heirs and
// successors. We intend this dedication to be an overt act of
// relinquishment in perpetuity of all present and future rights to this
// software under copyright law.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
// OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
// ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
// OTHER DEALINGS IN THE SOFTWARE.
//
// For more information, please refer to <http://unlicense.org>

mod bags_thresholds;
mod staking_common;
mod xcm_config;

use crate::{
	ElectionProviderMultiBlock, ElectionSignedPallet, ElectionVerifierPallet, NominationPools,
	OriginCaller, Staking, Timestamp, TransactionPayment, UncheckedExtrinsic, VoterList, MINUTES,
};

// Substrate and Polkadot dependencies
use cumulus_pallet_parachain_system::RelayNumberMonotonicallyIncreases;
use cumulus_primitives_core::{AggregateMessageOrigin, ParaId};
use frame_support::{
	derive_impl,
	dispatch::DispatchClass,
	parameter_types,
	traits::{
		ConstBool, ConstU32, ConstU64, ConstU8, EitherOfDiverse, TransformOrigin, VariantCountOf,
	},
	weights::{ConstantMultiplier, Weight},
	PalletId,
};
use frame_system::{
	limits::{BlockLength, BlockWeights},
	EnsureRoot,
};
use pallet_xcm::{EnsureXcm, IsVoiceOfBody};
use parachains_common::message_queue::{NarrowOriginToSibling, ParaIdToSibling};
use polkadot_runtime_common::{
	xcm_sender::NoPriceForMessageDelivery, BlockHashCount, SlowAdjustingFeeUpdate,
};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_runtime::{curve::PiecewiseLinear, Perbill, Percent, SaturatedConversion};
use sp_std::vec;
use sp_version::RuntimeVersion;
use xcm::latest::prelude::BodyId;

// Local module imports
use super::{
	weights::{BlockExecutionWeight, ExtrinsicBaseWeight, RocksDbWeight},
	AccountId, Aura, Balance, Balances, Block, BlockNumber, CollatorSelection, ConsensusHook, Hash,
	MessageQueue, Nonce, PalletInfo, ParachainSystem, Runtime, RuntimeCall, RuntimeEvent,
	RuntimeFreezeReason, RuntimeHoldReason, RuntimeOrigin, RuntimeTask, Session, SessionKeys,
	System, WeightToFee, XcmpQueue, AVERAGE_ON_INITIALIZE_RATIO, EXISTENTIAL_DEPOSIT, HOURS,
	MAXIMUM_BLOCK_WEIGHT, MICRO_UNIT, NORMAL_DISPATCH_RATIO, SLOT_DURATION, VERSION,
};
use xcm_config::{RelayLocation, XcmOriginToTransactDispatchOrigin};

pub use pallet_election_provider_multi_block::{
	self as pallet_epm_core,
	signed::{self as pallet_epm_signed},
	unsigned::{self as pallet_epm_unsigned},
	verifier::{self as pallet_epm_verifier},
};

use frame_election_provider_support::{
	bounds::ElectionBoundsBuilder, onchain, PageIndex, SequentialPhragmen,
};
use staking_common::{TargetIndex, VoterIndex};

parameter_types! {
	pub const Version: RuntimeVersion = VERSION;

	pub EpochDuration: u64 = 2 * MINUTES as u64;

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

/// The default types are being injected by [`derive_impl`](`frame_support::derive_impl`) from
/// [`ParaChainDefaultConfig`](`struct@frame_system::config_preludes::ParaChainDefaultConfig`),
/// but overridden as needed.
#[derive_impl(frame_system::config_preludes::ParaChainDefaultConfig)]
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
	type MaxFreezes = VariantCountOf<RuntimeFreezeReason>;
}

parameter_types! {
	/// Relay Chain `TransactionByteFee` / 10
	pub const TransactionByteFee: Balance = 10 * MICRO_UNIT;
}

impl pallet_transaction_payment::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OnChargeTransaction = pallet_transaction_payment::FungibleAdapter<Balances, ()>;
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
	type CheckAssociatedRelayNumber = RelayNumberMonotonicallyIncreases;
	type ConsensusHook = ConsensusHook;
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
	type HeapSize = sp_core::ConstU32<{ 103 * 1024 }>;
	type MaxStale = sp_core::ConstU32<8>;
	type ServiceWeight = MessageQueueServiceWeight;
	type IdleMaxServiceWeight = ();
}

impl cumulus_pallet_aura_ext::Config for Runtime {}

impl cumulus_pallet_xcmp_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ChannelInfo = ParachainSystem;
	type VersionWrapper = ();
	// Enqueue XCMP messages from siblings for later processing.
	type XcmpQueue = TransformOrigin<MessageQueue, AggregateMessageOrigin, ParaId, ParaIdToSibling>;
	type MaxInboundSuspended = sp_core::ConstU32<1_000>;
	type MaxActiveOutboundChannels = ConstU32<128>;
	type MaxPageSize = ConstU32<{ 1 << 16 }>;
	type ControllerOrigin = EnsureRoot<AccountId>;
	type ControllerOriginConverter = XcmOriginToTransactDispatchOrigin;
	type WeightInfo = ();
	type PriceForSiblingDelivery = NoPriceForMessageDelivery<ParaId>;
}

parameter_types! {
	// pub const Period: u32 = 6 * HOURS;
	//pub const Period: u32 = 12 * MINUTES;
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

#[docify::export(aura_config)]
impl pallet_aura::Config for Runtime {
	type AuthorityId = AuraId;
	type DisabledValidators = ();
	type MaxAuthorities = ConstU32<100_000>;
	type AllowMultipleBlocksPerSlot = ConstBool<true>;
	type SlotDuration = ConstU64<SLOT_DURATION>;
}

parameter_types! {
	pub const PotId: PalletId = PalletId(*b"PotStake");
	pub const SessionLength: BlockNumber = 12 * HOURS;
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

impl pallet_timestamp::Config for Runtime {
	/// A timestamp: milliseconds since the unix epoch.
	type Moment = u64;
	type OnTimestampSet = Aura;
	type MinimumPeriod = ConstU64<{ SLOT_DURATION / 2 }>;
	type WeightInfo = ();
}

impl pallet_utility::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type PalletsOrigin = OriginCaller;
	type WeightInfo = ();
}

// ----- staking related configs from now on

pallet_staking_reward_curve::build! {
	const REWARD_CURVE: PiecewiseLinear<'static> = curve!(
		min_inflation: 0_025_000,
		max_inflation: 0_100_000,
		ideal_stake: 0_500_000,
		falloff: 0_050_000,
		max_piece_count: 40,
		test_precision: 0_005_000,
	);
}

parameter_types! {
	pub const SessionsPerEra: sp_staking::SessionIndex = 1;
	pub const BondingDuration: sp_staking::EraIndex = 2;
	pub const SlashDeferDuration: sp_staking::EraIndex = 1;
	pub const RewardCurve: &'static PiecewiseLinear<'static> = &REWARD_CURVE;
	//pub const MaxExposurePageSize: u32 = 64;
	pub const MaxNominations: u32 = <NposCompactSolution as frame_election_provider_support::NposSolution>::LIMIT as u32;
	pub const MaxControllersInDeprecationBatch: u32 = 751;
	//pub const MaxValidatorSet: u32 = 4_000;
}

// Disabling threshold for `UpToLimitDisablingStrategy`
pub(crate) const DISABLING_LIMIT_FACTOR: usize = 3;

impl pallet_staking::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type CurrencyBalance = Balance;
	type CurrencyToVote = staking_common::CurrencyToVote;
	type UnixTime = Timestamp;
	type RewardRemainder = ();
	type Slash = ();
	type Reward = ();
	type SessionsPerEra = SessionsPerEra;
	type BondingDuration = BondingDuration;
	type SlashDeferDuration = SlashDeferDuration;
	type AdminOrigin = EnsureRoot<AccountId>;
	type SessionInterface = ();
	type EraPayout = pallet_staking::ConvertCurve<RewardCurve>;
	type MaxExposurePageSize = MaxExposurePageSize;
	type MaxValidatorSet = MaxValidatorSet;
	type NextNewSession = Session;
	type ElectionProvider = ElectionProviderMultiBlock;
	type GenesisElectionProvider = onchain::OnChainExecution<OnChainSeqPhragmen>;
	type VoterList = VoterList;
	type TargetList = pallet_staking::UseValidatorsMap<Self>;
	type NominationsQuota = pallet_staking::FixedNominationsQuota<{ MaxNominations::get() }>;
	type MaxUnlockingChunks = frame_support::traits::ConstU32<32>;
	type HistoryDepth = frame_support::traits::ConstU32<84>;
	type MaxControllersInDeprecationBatch = MaxControllersInDeprecationBatch;
	type BenchmarkingConfig = staking_common::StakingBenchmarkingConfig;
	type EventListeners = NominationPools;
	type DisablingStrategy = pallet_staking::UpToLimitDisablingStrategy<DISABLING_LIMIT_FACTOR>;
	type WeightInfo = (); // weights
}

impl pallet_fast_unstake::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type BatchSize = frame_support::traits::ConstU32<64>;
	type Deposit = frame_support::traits::ConstU128<10>;
	type ControlOrigin = EnsureRoot<AccountId>;
	type Staking = Staking;
	type MaxErasToCheckPerBlock = ConstU32<1>;
	type WeightInfo = (); // weights::pallet_fast_unstake::WeightInfo<Runtime>;
}

parameter_types! {
	pub const PoolsPalletId: PalletId = PalletId(*b"py/nopls");
	pub const MaxPointsToBalance: u8 = 10;
}

impl pallet_nomination_pools::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type Currency = Balances;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type RewardCounter = sp_runtime::FixedU128;
	type BalanceToU256 = staking_common::BalanceToU256;
	type U256ToBalance = staking_common::U256ToBalance;
	type StakeAdapter = pallet_nomination_pools::adapter::TransferStake<Self, Staking>;
	type PostUnbondingPoolsWindow = ConstU32<4>;
	type MaxMetadataLen = ConstU32<256>;
	// we use the same number of allowed unlocking chunks as with staking.
	type MaxUnbonding = <Self as pallet_staking::Config>::MaxUnlockingChunks;
	type PalletId = PoolsPalletId;
	type MaxPointsToBalance = MaxPointsToBalance;
	type AdminOrigin = EnsureRoot<AccountId>;
}

parameter_types! {
	// npos_solution
	pub MaxVoters: u32 = VoterSnapshotPerBlock::get() * Pages::get();

	// SETUPS
	// see results at https://hackmd.io/KpU6KVL-QOiwRxWPY9FDdQ/view

	// AAA. (modified for staking-miner tests)
	// current numbers.
	// let validators_count = 1_500; in chainspec
	pub const Period: u32 = 5 * MINUTES;
	pub const MaxExposurePageSize: u32 = 512;
	pub const MaxValidatorSet: u32 = 1_000;
	pub SignedPhase: u32 = 25;
	pub UnsignedPhase: u32 = 0;
	pub Pages: PageIndex = 3;
	pub MaxWinnersPerPage: u32 = MaxValidatorSet::get();
	pub MaxBackersPerWinner: u32 = 5_000;
	pub VoterSnapshotPerBlock: VoterIndex = 500;
	pub TargetSnapshotPerBlock: TargetIndex = MaxWinnersPerPage::get().try_into().unwrap();

	/*
	// A1.
	// at voter snapshot creation:
	// ⚠️  ⚠️   PoV STORAGE PROOF OVER LIMIT (15473.44921875kb > 5120.0kb, ie. 302% overflow)
	pub const Period: u32 = 3 * MINUTES;
	pub const MaxExposurePageSize: u32 = 64;
	pub const MaxValidatorSet: u32 = 4_000;
	pub Pages: PageIndex = 1;
	pub MaxWinnersPerPage: u32 = MaxValidatorSet::get();
	pub MaxBackersPerWinner: u32 = 5_000;
	pub VoterSnapshotPerBlock: VoterIndex = 10_000;
	pub TargetSnapshotPerBlock: TargetIndex = MaxWinnersPerPage::get().try_into().unwrap();
	*/

	/*
	// B1.
	// at voter snapshot creation:
	// ⚠️  ⚠️   PoV STORAGE PROOF OVER LIMIT (9165.3466796875kb > 5120.0kb, ie. 179% overflow)
	pub const Period: u32 = 3 * MINUTES;
	pub const MaxExposurePageSize: u32 = 64;
	pub const MaxValidatorSet: u32 = 4_000;
	pub UnsignedPhase: u32 = 5;
	pub Pages: PageIndex = 1;
	pub MaxWinnersPerPage: u32 = MaxValidatorSet::get();
	pub MaxBackersPerWinner: u32 = 5_000;
	pub VoterSnapshotPerBlock: VoterIndex = 5_000;
	pub TargetSnapshotPerBlock: TargetIndex = MaxWinnersPerPage::get().try_into().unwrap();
	*/

	/*
	// C1.
	// OK E2E
	pub const Period: u32 = 3 * MINUTES;
	pub const MaxExposurePageSize: u32 = 64;
	pub const MaxValidatorSet: u32 = 4_000;
	pub UnsignedPhase: u32 = 5;
	pub Pages: PageIndex = 1;
	pub MaxWinnersPerPage: u32 = MaxValidatorSet::get();
	pub MaxBackersPerWinner: u32 = 5_000;
	pub VoterSnapshotPerBlock: VoterIndex = 2_000;
	pub TargetSnapshotPerBlock: TargetIndex = MaxWinnersPerPage::get().try_into().unwrap();
	*/

	/*
	// D1.
	// OK E2E
	pub const Period: u32 = 3 * MINUTES;
	pub const MaxExposurePageSize: u32 = 64;
	pub const MaxValidatorSet: u32 = 4_000;
	pub UnsignedPhase: u32 = 5;
	pub Pages: PageIndex = 1;
	pub MaxWinnersPerPage: u32 = 30_000;
	pub MaxBackersPerWinner: u32 = 30_000;
	pub VoterSnapshotPerBlock: VoterIndex = 2_000;
	pub TargetSnapshotPerBlock: TargetIndex = MaxWinnersPerPage::get().try_into().unwrap();
	*/


	/*
	// A2. OK E2E
	pub const Period: u32 = 12 * MINUTES;
	pub const MaxExposurePageSize: u32 = 64;
	pub const MaxValidatorSet: u32 = 4_000;
	pub UnsignedPhase: u32 = 60;
	pub Pages: PageIndex = 20;
	pub MaxWinnersPerPage: u32 = MaxValidatorSet::get();
	pub MaxBackersPerWinner: u32 = 30_000;
	pub VoterSnapshotPerBlock: VoterIndex = 2_000;
	pub TargetSnapshotPerBlock: TargetIndex = MaxWinnersPerPage::get().try_into().unwrap();
	*/

	/*
	// B2. OK E2E
	pub const Period: u32 = 8 * MINUTES;
	pub const MaxExposurePageSize: u32 = 64;
	pub const MaxValidatorSet: u32 = 4_000;
	pub UnsignedPhase: u32 = 30;
	pub Pages: PageIndex = 10;
	pub MaxWinnersPerPage: u32 = MaxValidatorSet::get();
	pub MaxBackersPerWinner: u32 = 30_000;
	pub VoterSnapshotPerBlock: VoterIndex = 2_000;
	pub TargetSnapshotPerBlock: TargetIndex = MaxWinnersPerPage::get().try_into().unwrap();
	*/

	/*
	// A3. at full page verification:
	//  ⚠️  ⚠️   PoV STORAGE PROOF OVER LIMIT (5571.548828125kb > 5120.0kb, ie. 108% overflow)
	// let validators_count = 4_000; in chainspec
	pub const Period: u32 = 20 * MINUTES;
	pub const MaxExposurePageSize: u32 = 64;
	pub const MaxValidatorSet: u32 = 4_000;
	pub UnsignedPhase: u32 = 80;
	pub Pages: PageIndex = 20;
	pub MaxWinnersPerPage: u32 = MaxValidatorSet::get();
	pub MaxBackersPerWinner: u32 = 30_000;
	pub VoterSnapshotPerBlock: VoterIndex = 2_000;
	pub TargetSnapshotPerBlock: TargetIndex = MaxWinnersPerPage::get().try_into().unwrap();
	*/

	/*
	// B3. OK
	// let validators_count = 3_000; in chainspec
	pub const Period: u32 = 20 * MINUTES;
	pub const MaxExposurePageSize: u32 = 64;
	pub const MaxValidatorSet: u32 = 4_000;
	pub UnsignedPhase: u32 = 80;
	pub Pages: PageIndex = 20;
	pub MaxWinnersPerPage: u32 = MaxValidatorSet::get();
	pub MaxBackersPerWinner: u32 = 30_000;
	pub VoterSnapshotPerBlock: VoterIndex = 2_000;
	pub TargetSnapshotPerBlock: TargetIndex = MaxWinnersPerPage::get().try_into().unwrap();
	*/

	//pub SignedPhase: u32 = 0; // (1 * MINUTES / 2).min(EpochDuration::get().saturated_into::<u32>() / 2);
	//pub UnsignedPhase: u32 = 60; // (5 * MINUTES / 2).min(EpochDuration::get().saturated_into::<u32>() / 2);
	pub SignedValidationPhase: BlockNumber = Pages::get() * SignedMaxSubmissions::get();
	pub Lookhaead: BlockNumber = 5;
	pub ExportPhaseLimit: BlockNumber = Pages::get().into();

	pub const SignedMaxSubmissions: u32 = 2;
	pub const SignedMaxRefunds: u32 = 128 / 4;
	pub const SignedFixedDeposit: Balance = 10;
	pub const SignedDepositByte: Balance = 10;
	pub SignedRewardBase: Balance = 10;
	pub const SignedDepositIncreaseFactor: Percent = Percent::from_percent(10);
	pub BetterUnsignedThreshold: Perbill = Perbill::from_rational(5u32, 10_000);

	// off-chain worker.
	pub OffchainRepeat: BlockNumber = UnsignedPhase::get() / 4;

	// on-chain election.
	pub const MaxOnchainElectingVoters: u32 = 22_500;
	pub OnChainElectionBounds: frame_election_provider_support::bounds::ElectionBounds =
		ElectionBoundsBuilder::default().voters_count(MaxOnchainElectingVoters::get().into()).build();


	// sub-pallets.
	pub SolutionImprovementThreshold: Perbill = Perbill::from_percent(10);

	pub ElectionSubmissionDepositBase: Balance = 10;
	pub DepositPerPage: Balance = 1;
	pub Reward: Balance = 10;
	pub MaxSubmissions: u32 = 5;

	pub OffchainRepeatInterval: BlockNumber = 10;
	pub MinerTxPriority: u64 = 0;
	pub MinerSolutionMaxLength: u32 = u32::MAX;
	pub MinerSolutionMaxWeight: Weight = Weight::MAX;
}

// solution type.
frame_election_provider_support::generate_solution_type!(
	#[compact]
	pub struct NposCompactSolution::<
		VoterIndex = VoterIndex,
		TargetIndex = TargetIndex,
		Accuracy = sp_runtime::PerU16,
		MaxVoters = MaxVoters,
	>(16)
);

impl<C> frame_system::offchain::SendTransactionTypes<C> for Runtime
where
	RuntimeCall: From<C>,
{
	type OverarchingCall = RuntimeCall;
	type Extrinsic = UncheckedExtrinsic;
}

impl pallet_epm_core::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SignedPhase = SignedPhase;
	type UnsignedPhase = UnsignedPhase;
	type SignedValidationPhase = SignedValidationPhase;
	type Lookhaead = Lookhaead;
	type VoterSnapshotPerBlock = VoterSnapshotPerBlock;
	type TargetSnapshotPerBlock = TargetSnapshotPerBlock;
	type MaxBackersPerWinner = MaxBackersPerWinner;
	type MaxWinnersPerPage = MaxWinnersPerPage;
	type Pages = Pages;
	type ExportPhaseLimit = ExportPhaseLimit;
	type MinerConfig = Self;
	type Fallback = frame_election_provider_support::NoElection<(
		AccountId,
		BlockNumber,
		Staking,
		MaxWinnersPerPage,
		MaxBackersPerWinner,
	)>;
	type Verifier = ElectionVerifierPallet;
	type DataProvider = Staking;
	type BenchmarkingConfig = staking_common::EPMBenchmarkingConfig;
	type WeightInfo = pallet_epm_core::weights::SubstrateWeight<Runtime>;
}

impl pallet_epm_verifier::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ForceOrigin = frame_system::EnsureRoot<AccountId>;
	type SolutionImprovementThreshold = SolutionImprovementThreshold;
	type SolutionDataProvider = ElectionSignedPallet;
	type WeightInfo = pallet_epm_verifier::weights::SubstrateWeight<Runtime>;
}

impl pallet_epm_signed::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type EstimateCallFee = TransactionPayment;
	type OnSlash = (); // burn
	type DepositBase = ConstDepositBase;
	type DepositPerPage = DepositPerPage;
	type Reward = Reward;
	type MaxSubmissions = MaxSubmissions;
	type RuntimeHoldReason = RuntimeHoldReason;
	type WeightInfo = ();
}

pub struct ConstDepositBase;
impl sp_runtime::traits::Convert<usize, Balance> for ConstDepositBase {
	fn convert(_a: usize) -> Balance {
		ElectionSubmissionDepositBase::get()
	}
}

impl pallet_epm_unsigned::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OffchainRepeatInterval = OffchainRepeatInterval;
	type MinerTxPriority = MinerTxPriority;
	type MaxLength = MinerSolutionMaxLength;
	type MaxWeight = MinerSolutionMaxWeight;
	type WeightInfo = pallet_epm_unsigned::weights::SubstrateWeight<Runtime>;
}

impl pallet_election_provider_multi_block::unsigned::miner::Config for Runtime {
	type AccountId = AccountId;
	type Solution = NposCompactSolution;
	type Solver = SequentialPhragmen<AccountId, sp_runtime::PerU16>;
	type Pages = Pages;
	type MaxVotesPerVoter = ConstU32<16>;
	type MaxWinnersPerPage = MaxWinnersPerPage;
	type MaxBackersPerWinner = MaxBackersPerWinner;
	type VoterSnapshotPerBlock = VoterSnapshotPerBlock;
	type TargetSnapshotPerBlock = TargetSnapshotPerBlock;
	type MaxWeight = MinerSolutionMaxWeight;
	type MaxLength = MinerSolutionMaxLength;
}

pub struct OnChainSeqPhragmen;
impl onchain::Config for OnChainSeqPhragmen {
	type System = Runtime;
	type Solver = SequentialPhragmen<AccountId, sp_runtime::PerU16>;
	type DataProvider = Staking;
	type Bounds = OnChainElectionBounds;
	type MaxBackersPerWinner = MaxBackersPerWinner;
	type MaxWinnersPerPage = MaxWinnersPerPage;
	type WeightInfo = ();
}

parameter_types! {
	pub const BagThresholds: &'static [u64] = &bags_thresholds::THRESHOLDS;
}

type VoterBagsListInstance = pallet_bags_list::Instance1;
impl pallet_bags_list::Config<VoterBagsListInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ScoreProvider = Staking;
	type WeightInfo = ();
	type BagThresholds = BagThresholds;
	type Score = sp_npos_elections::VoteWeight;
}
