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

///! Staking, and election related pallet configurations.
use super::*;
use cumulus_primitives_core::relay_chain::SessionIndex;
use frame_election_provider_support::{
	bounds::{CountBound, ElectionBounds, ElectionBoundsBuilder},
	onchain, ElectionDataProvider, SequentialPhragmen,
};
use frame_support::traits::{ConstU128, EitherOf};
use pallet_election_provider_multi_block::{self as multi_block, SolutionAccuracyOf};
use pallet_staking::UseValidatorsMap;
use polkadot_runtime_common::{prod_or_fast, BalanceToU256, CurrencyToVote, U256ToBalance};
use sp_runtime::{
	transaction_validity::TransactionPriority, FixedPointNumber, FixedU128, SaturatedConversion,
};

frame_election_provider_support::generate_solution_type!(
	#[compact]
	pub struct NposCompactSolution16::<
		VoterIndex = u32,
		TargetIndex = u16,
		Accuracy = sp_runtime::PerU16,
		MaxVoters = ConstU32<22_500>, // TODO: this should be made more accurate, as it improves the `MaxEncodedLen` estimate of this type.
	>(16)
);

impl multi_block::unsigned::miner::MinerConfig for Runtime {
	type AccountId = AccountId;
	type Hash = Hash;
	type MaxBackersPerWinner = <Self as multi_block::verifier::Config>::MaxBackersPerWinner;
	type MaxBackersPerWinnerFinal =
		<Self as multi_block::verifier::Config>::MaxBackersPerWinnerFinal;
	type MaxWinnersPerPage = <Self as multi_block::verifier::Config>::MaxWinnersPerPage;
	type MaxVotesPerVoter =
		<<Self as multi_block::Config>::DataProvider as ElectionDataProvider>::MaxVotesPerVoter;
	type MaxLength = MinerMaxLength;
	type Solver = <Runtime as multi_block::unsigned::Config>::OffchainSolver;
	type Pages = Pages;
	type Solution = NposCompactSolution16;
	type VoterSnapshotPerBlock = <Runtime as multi_block::Config>::VoterSnapshotPerBlock;
	type TargetSnapshotPerBlock = <Runtime as multi_block::Config>::TargetSnapshotPerBlock;
}

parameter_types! {
	pub MaxWinnersPerPage: u32 = 64;
	pub MaxBackersPerWinnerPerPage: u32 = 64;
	pub MaxBackersPerWinnerFinal: u32 = 512;
	pub MaxExposurePageSize: u32 = MaxBackersPerWinnerFinal::get() / 4;
	pub MaxValidatorSet: u32 = 1000;
	pub Pages: u32 = 8;
	pub VoterSnapshotPerBlock: u32 = 22_500 / Pages::get();
	pub TargetSnapshotPerBlock: u32 = MaxValidatorSet::get();
	// TODO: the duration of phases, plus our estimation of the next election should be fixed up based on the rc-client pallet.
	pub SignedPhase: u32 = TempFixedSessionLength::get() / 2;
	pub UnsignedPhase: u32 = TempFixedSessionLength::get() / 2;
	// validate up to 4 signed solution.
	pub SignedValidationPhase: u32 = Pages::get() * 4;
	pub SolutionImprovementThreshold: Perbill = Perbill::from_rational(1u32, 10_000);
}

impl multi_block::Config for Runtime {
	type AdminOrigin = EnsureRoot<AccountId>;
	type RuntimeEvent = RuntimeEvent;
	type DataProvider = Staking;
	type Fallback = multi_block::Continue<Self>;
	type Lookahead = ConstU32<5>;
	type Pages = Pages;
	type SignedPhase = SignedPhase;
	type UnsignedPhase = UnsignedPhase;
	type SignedValidationPhase = SignedValidationPhase;
	type MinerConfig = Self;
	type VoterSnapshotPerBlock = VoterSnapshotPerBlock;
	type TargetSnapshotPerBlock = TargetSnapshotPerBlock;
	type WeightInfo = ();
	type Verifier = MultiBlockVerifier;
}

impl multi_block::verifier::Config for Runtime {
	type MaxWinnersPerPage = MaxWinnersPerPage;
	type MaxBackersPerWinner = MaxBackersPerWinnerPerPage;
	type MaxBackersPerWinnerFinal = MaxBackersPerWinnerFinal;
	type SolutionDataProvider = MultiBlockSigned;
	type WeightInfo = ();
	type RuntimeEvent = RuntimeEvent;
	type SolutionImprovementThreshold = SolutionImprovementThreshold;
}

parameter_types! {
	pub BailoutGraceRatio: Perbill = Perbill::from_percent(50);
	pub DepositBase: Balance = 5 * UNITS;
	pub DepositPerPage: Balance = 1 * UNITS;
	pub RewardBase: Balance = 10 * UNITS;
}

impl multi_block::signed::Config for Runtime {
	type BailoutGraceRatio = BailoutGraceRatio;
	type Currency = Balances;
	type DepositBase = DepositBase;
	type DepositPerPage = DepositPerPage;
	type EstimateCallFee = TransactionPayment;
	type MaxSubmissions = ConstU32<32>;
	type RewardBase = RewardBase;
	type RuntimeEvent = RuntimeEvent;
	type RuntimeHoldReason = RuntimeHoldReason;
	type WeightInfo = ();
}

parameter_types! {
	pub MinerTxPriority: TransactionPriority = TransactionPriority::max_value() / 2;
	pub MinerMaxLength: u32 = Perbill::from_rational(9u32, 10) *
		*RuntimeBlockLength::get()
		.max
		.get(DispatchClass::Normal);
	pub MinerMaxWeight: Weight = RuntimeBlockWeights::get()
		.get(DispatchClass::Normal)
		.max_extrinsic.expect("Normal extrinsics have a weight limit configured; qed")
		.saturating_sub(BlockExecutionWeight::get());
}

impl multi_block::unsigned::Config for Runtime {
	type WeightInfo = ();
	type OffchainSolver = SequentialPhragmen<AccountId, SolutionAccuracyOf<Runtime>>;
	type MinerTxPriority = MinerTxPriority;
	type OffchainRepeat = ConstU32<5>;
}

parameter_types! {
	pub ElectionBoundsOnChain: ElectionBounds = ElectionBoundsBuilder::default()
		.voters_count(CountBound(500))
		.targets_count(CountBound(100))
		.build();
}
pub struct OnChainSeqPhragmen;
impl onchain::Config for OnChainSeqPhragmen {
	type Sort = ConstBool<true>;
	type System = Runtime;
	type Solver = SequentialPhragmen<AccountId, SolutionAccuracyOf<Runtime>>;
	type DataProvider = Staking;
	type WeightInfo = frame_election_provider_support::weights::SubstrateWeight<Runtime>;
	type Bounds = ElectionBoundsOnChain;
	type MaxBackersPerWinner = <Runtime as multi_block::verifier::Config>::MaxBackersPerWinner;
	type MaxWinnersPerPage = <Runtime as multi_block::verifier::Config>::MaxWinnersPerPage;
}

parameter_types! {
	/// A limit for off-chain phragmen unsigned solution submission.
	///
	/// We want to keep it as high as possible, but can't risk having it reject,
	/// so we always subtract the base block execution weight.
	pub OffchainSolutionWeightLimit: Weight = RuntimeBlockWeights::get()
		.get(DispatchClass::Normal)
		.max_extrinsic
		.expect("Normal extrinsics have weight limit configured by default; qed")
		.saturating_sub(weights::BlockExecutionWeight::get());

	/// A limit for off-chain phragmen unsigned solution length.
	///
	/// We allow up to 90% of the block's size to be consumed by the solution.
	pub OffchainSolutionLengthLimit: u32 = Perbill::from_rational(90_u32, 100) *
		*RuntimeBlockLength::get()
		.max
		.get(DispatchClass::Normal);

		pub const NposSolutionPriority: TransactionPriority = TransactionPriority::max_value() / 2;
}

parameter_types! {
	pub const BagThresholds: &'static [u64] = &bag_thresholds::THRESHOLDS;
}

type VoterBagsListInstance = pallet_bags_list::Instance1;
impl pallet_bags_list::Config<VoterBagsListInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ScoreProvider = Staking;
	type WeightInfo = weights::pallet_bags_list::WeightInfo<Runtime>;
	type BagThresholds = BagThresholds;
	type Score = sp_npos_elections::VoteWeight;
}

pub struct EraPayout;
impl pallet_staking::EraPayout<Balance> for EraPayout {
	fn era_payout(
		_total_staked: Balance,
		_total_issuance: Balance,
		era_duration_millis: u64,
	) -> (Balance, Balance) {
		const MILLISECONDS_PER_YEAR: u64 = (1000 * 3600 * 24 * 36525) / 100;
		// A normal-sized era will have 1 / 365.25 here:
		let relative_era_len =
			FixedU128::from_rational(era_duration_millis.into(), MILLISECONDS_PER_YEAR.into());

		// Fixed total TI that we use as baseline for the issuance.
		let fixed_total_issuance: i128 = 5_216_342_402_773_185_773;
		let fixed_inflation_rate = FixedU128::from_rational(8, 100);
		let yearly_emission = fixed_inflation_rate.saturating_mul_int(fixed_total_issuance);

		let era_emission = relative_era_len.saturating_mul_int(yearly_emission);
		// 15% to treasury, as per Polkadot ref 1139.
		let to_treasury = FixedU128::from_rational(15, 100).saturating_mul_int(era_emission);
		let to_stakers = era_emission.saturating_sub(to_treasury);

		(to_stakers.saturated_into(), to_treasury.saturated_into())
	}
}

parameter_types! {
	// TODO: (rc-client) Very short sessions. This needs to be reworked based on rc-client.
	pub const SessionsPerEra: SessionIndex = 1;
	// 2 eras for unbonding (12 hours).
	pub const BondingDuration: sp_staking::EraIndex = 2;
	// 1 era in which slashes can be cancelled (6 hours).
	pub const SlashDeferDuration: sp_staking::EraIndex = 1;
	// Note: this is not really correct as Max Nominators is (MaxExposurePageSize * page_count) but
	// this is an unbounded number. We just set it to a reasonably high value, 1 full page
	// of nominators.
	pub const MaxControllersInDeprecationBatch: u32 = 751;
	pub const MaxNominations: u32 = <NposCompactSolution16 as frame_election_provider_support::NposSolution>::LIMIT as u32;
}

parameter_types! {
	pub TempFixedSessionLength: u32 = 120;
}
type TempNextNewSession = pallet_session::PeriodicSessions<TempFixedSessionLength, ()>;

impl pallet_staking::Config for Runtime {
	type MaxValidatorSet = MaxValidatorSet;
	type OldCurrency = Balances;
	type Currency = Balances;
	type RuntimeHoldReason = RuntimeHoldReason;
	type CurrencyBalance = Balance;
	type UnixTime = Timestamp;
	type CurrencyToVote = CurrencyToVote;
	type RewardRemainder = ();
	type RuntimeEvent = RuntimeEvent;
	type Slash = ();
	type Reward = ();
	type SessionsPerEra = SessionsPerEra;
	type BondingDuration = BondingDuration;
	type SlashDeferDuration = SlashDeferDuration;
	type AdminOrigin = EitherOf<EnsureRoot<AccountId>, StakingAdmin>;
	type SessionInterface = Self;
	type NextNewSession = TempNextNewSession;
	type EraPayout = EraPayout;
	type MaxExposurePageSize = MaxExposurePageSize;
	type ElectionProvider = MultiBlock; // Kaboom!
	type GenesisElectionProvider = onchain::OnChainExecution<OnChainSeqPhragmen>;
	type VoterList = VoterList;
	type TargetList = UseValidatorsMap<Self>;
	type NominationsQuota = pallet_staking::FixedNominationsQuota<{ MaxNominations::get() }>;
	type MaxUnlockingChunks = ConstU32<32>;
	type HistoryDepth = ConstU32<84>;
	type MaxControllersInDeprecationBatch = MaxControllersInDeprecationBatch;
	type BenchmarkingConfig = polkadot_runtime_common::StakingBenchmarkingConfig;
	type EventListeners = (NominationPools, DelegatedStaking);
	type WeightInfo = weights::pallet_staking::WeightInfo<Runtime>;
	type DisablingStrategy = pallet_staking::UpToLimitDisablingStrategy;
}

impl pallet_fast_unstake::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type BatchSize = ConstU32<64>;
	type Deposit = ConstU128<{ UNITS }>;
	type ControlOrigin = EnsureRoot<AccountId>;
	type Staking = Staking;
	type MaxErasToCheckPerBlock = ConstU32<1>;
	type WeightInfo = weights::pallet_fast_unstake::WeightInfo<Runtime>;
}

parameter_types! {
	pub const PoolsPalletId: PalletId = PalletId(*b"py/nopls");
	pub const MaxPointsToBalance: u8 = 10;
}

impl pallet_nomination_pools::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_nomination_pools::WeightInfo<Self>;
	type Currency = Balances;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type RewardCounter = FixedU128;
	type BalanceToU256 = BalanceToU256;
	type U256ToBalance = U256ToBalance;
	type StakeAdapter =
		pallet_nomination_pools::adapter::DelegateStake<Self, Staking, DelegatedStaking>;
	type PostUnbondingPoolsWindow = ConstU32<4>;
	type MaxMetadataLen = ConstU32<256>;
	// we use the same number of allowed unlocking chunks as with staking.
	type MaxUnbonding = <Self as pallet_staking::Config>::MaxUnlockingChunks;
	type PalletId = PoolsPalletId;
	type MaxPointsToBalance = MaxPointsToBalance;
	type AdminOrigin = EitherOf<EnsureRoot<AccountId>, StakingAdmin>;
}

parameter_types! {
	pub const DelegatedStakingPalletId: PalletId = PalletId(*b"py/dlstk");
	pub const SlashRewardFraction: Perbill = Perbill::from_percent(1);
}

impl pallet_delegated_staking::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type PalletId = DelegatedStakingPalletId;
	type Currency = Balances;
	type OnSlash = ();
	type SlashRewardFraction = SlashRewardFraction;
	type RuntimeHoldReason = RuntimeHoldReason;
	type CoreStaking = Staking;
}

impl frame_system::offchain::SigningTypes for Runtime {
	type Public = <Signature as Verify>::Signer;
	type Signature = Signature;
}

impl<C> frame_system::offchain::CreateTransactionBase<C> for Runtime
where
	RuntimeCall: From<C>,
{
	type RuntimeCall = RuntimeCall;
	type Extrinsic = UncheckedExtrinsic;
}

impl<LocalCall> frame_system::offchain::CreateTransaction<LocalCall> for Runtime
where
	RuntimeCall: From<LocalCall>,
{
	type Extension = TxExtension;

	fn create_transaction(call: RuntimeCall, extension: TxExtension) -> UncheckedExtrinsic {
		UncheckedExtrinsic::new_transaction(call, extension)
	}
}

pub type UncheckedExtrinsic =
	generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, TxExtension>;

impl<LocalCall> frame_system::offchain::CreateInherent<LocalCall> for Runtime
where
	RuntimeCall: From<LocalCall>,
{
	fn create_inherent(call: RuntimeCall) -> UncheckedExtrinsic {
		UncheckedExtrinsic::new_bare(call)
	}
}

impl pallet_session::historical::Config for Runtime {
	type FullIdentification = pallet_staking::Exposure<AccountId, Balance>;
	type FullIdentificationOf = pallet_staking::ExposureOf<Runtime>;
}
