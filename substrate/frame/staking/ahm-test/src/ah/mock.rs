use frame::{testing_prelude::*, traits::Extrinsic as _};
use frame_election_provider_support::{ElectionProvider, SequentialPhragmen};
use frame_support::sp_runtime::testing::TestXt;
use pallet_election_provider_multi_block as multi_block;
use sp_staking::SessionIndex;

construct_runtime! {
	pub enum Runtime {
		System: frame_system,
		Balances: pallet_balances,

		Staking: pallet_staking,
		RcClient: pallet_staking_rc_client,

		MultiBlock: multi_block,
		MultiBlockVerifier: multi_block::verifier,
		MultiBlockSigned: multi_block::signed,
		MultiBlockUnsigned: multi_block::unsigned,
	}
}

pub fn roll_next() {
	let now = System::block_number();
	let next = now + 1;

	System::set_block_number(next);

	Staking::on_initialize(next);
	RcClient::on_initialize(next);
	MultiBlock::on_initialize(next);
	MultiBlockVerifier::on_initialize(next);
	MultiBlockSigned::on_initialize(next);
	MultiBlockUnsigned::on_initialize(next);

	// we assume this block is empty, no operations happen.

	// staking is the only pallet that has on-finalize.
	Staking::on_finalize(now);
}

pub type AccountId = <Runtime as frame_system::Config>::AccountId;
pub type Balance = <Runtime as pallet_balances::Config>::Balance;
pub type Hash = <Runtime as frame_system::Config>::Hash;
pub type BlockNumber = BlockNumberFor<Runtime>;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = MockBlock<Self>;
	type AccountData = pallet_balances::AccountData<Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
	type AccountStore = System;
}

frame_election_provider_support::generate_solution_type!(
	pub struct TestNposSolution::<
		VoterIndex = u16,
		TargetIndex = u16,
		Accuracy = PerU16,
		MaxVoters = ConstU32::<1000>
	>(16)
);

type Extrinsic = TestXt<RuntimeCall, ()>;
impl<LocalCall> frame_system::offchain::CreateTransactionBase<LocalCall> for Runtime
where
	RuntimeCall: From<LocalCall>,
{
	type RuntimeCall = RuntimeCall;
	type Extrinsic = Extrinsic;
}

impl<LocalCall> frame_system::offchain::CreateInherent<LocalCall> for Runtime
where
	RuntimeCall: From<LocalCall>,
{
	fn create_inherent(call: Self::RuntimeCall) -> Self::Extrinsic {
		Extrinsic::new_bare(call)
	}
}

type MaxVotesPerVoter = pallet_staking::MaxNominationsOf<Runtime>;
parameter_types! {
	pub static MaxValidators: u32 = 32;
	pub static MaxBackersPerWinner: u32 = 16;
	pub static MaxExposurePageSize: u32 = 8;
	pub static MaxBackersPerWinnerFinal: u32 = 16;
	pub static MaxWinnersPerPage: u32 = 16;
	pub static MaxLength: u32 = 4 * 1024 * 1024;
	pub static Pages: u32 = 3;
	pub static TargetSnapshotPerBlock: u32 = 4;
	pub static VoterSnapshotPerBlock: u32 = 4;

	pub static SignedPhase: BlockNumber = 4;
	pub static UnsignedPhase: BlockNumber = 4;
	pub static SignedValidationPhase: BlockNumber = (2 * Pages::get() as BlockNumber);
}

impl multi_block::unsigned::miner::MinerConfig for Runtime {
	type AccountId = AccountId;
	type Hash = Hash;
	type MaxBackersPerWinner = MaxBackersPerWinner;
	type MaxWinnersPerPage = MaxWinnersPerPage;
	type MaxBackersPerWinnerFinal = MaxBackersPerWinnerFinal;
	type MaxVotesPerVoter = MaxVotesPerVoter;
	type Solution = TestNposSolution;
	type MaxLength = MaxLength;
	type Pages = Pages;
	type Solver = SequentialPhragmen<AccountId, Perbill>;
	type TargetSnapshotPerBlock = TargetSnapshotPerBlock;
	type VoterSnapshotPerBlock = VoterSnapshotPerBlock;
}

impl multi_block::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;

	type AdminOrigin = EnsureRoot<AccountId>;
	type DataProvider = Staking;
	type Fallback = multi_block::Continue<Self>;
	type MinerConfig = Self;

	type Pages = Pages;
	type SignedPhase = SignedPhase;
	type UnsignedPhase = UnsignedPhase;
	type SignedValidationPhase = SignedValidationPhase;
	type TargetSnapshotPerBlock = TargetSnapshotPerBlock;
	type VoterSnapshotPerBlock = VoterSnapshotPerBlock;
	type Verifier = MultiBlockVerifier;
	type WeightInfo = multi_block::weights::AllZeroWeights;
}

impl multi_block::verifier::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;

	type MaxBackersPerWinner = MaxBackersPerWinner;
	type MaxBackersPerWinnerFinal = MaxBackersPerWinnerFinal;
	type MaxWinnersPerPage = MaxWinnersPerPage;

	type SolutionDataProvider = MultiBlockSigned;
	type SolutionImprovementThreshold = ();
	type WeightInfo = multi_block::weights::AllZeroWeights;
}

impl multi_block::unsigned::Config for Runtime {
	type WeightInfo = multi_block::weights::AllZeroWeights;
	type MinerTxPriority = ConstU64<{ u64::MAX }>;
	type OffchainRepeat = ();
	type OffchainSolver = SequentialPhragmen<AccountId, Perbill>;
}

parameter_types! {
	pub static DepositBase: Balance = 1;
	pub static DepositPerPage: Balance = 1;
	pub static EstimateCallFee: Balance = 1;
	pub static MaxSubmissions: u32 = 2;
	pub static RewardBase: Balance = 5;
}

impl multi_block::signed::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeHoldReason = RuntimeHoldReason;

	type Currency = Balances;

	type BailoutGraceRatio = ();
	type DepositBase = DepositBase;
	type DepositPerPage = DepositPerPage;
	type EstimateCallFee = EstimateCallFee;
	type MaxSubmissions = MaxSubmissions;
	type RewardBase = RewardBase;
	type WeightInfo = multi_block::weights::AllZeroWeights;
}

parameter_types! {
	pub static BondingDuration: u32 = 3;
	pub static SlashDeferredDuration: u32 = 2;
	pub static SessionsPerEra: u32 = 3;
}

impl pallet_staking::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeHoldReason = RuntimeHoldReason;

	type AdminOrigin = EnsureRoot<AccountId>;
	type BenchmarkingConfig = pallet_staking::TestBenchmarkingConfig;
	type BondingDuration = BondingDuration;
	type SessionsPerEra = SessionsPerEra;

	type Currency = Balances;
	type OldCurrency = Balances;
	type CurrencyBalance = Balance;
	type CurrencyToVote = ();

	type ElectionProvider = MultiBlock;
	type GenesisElectionProvider = Self::ElectionProvider;

	type EraPayout = ();
	type EventListeners = ();
	type Reward = ();
	type RewardRemainder = ();
	type Slash = ();
	type SlashDeferDuration = SlashDeferredDuration;

	type HistoryDepth = ConstU32<7>;
	type MaxControllersInDeprecationBatch = ();

	type MaxDisabledValidators = MaxValidators;
	type MaxValidatorSet = MaxValidators;
	type MaxExposurePageSize = MaxExposurePageSize;
	type MaxInvulnerables = MaxValidators;
	type MaxUnlockingChunks = ConstU32<16>;
	type NominationsQuota = pallet_staking::FixedNominationsQuota<16>;

	type VoterList = pallet_staking::UseNominatorsAndValidatorsMap<Self>;
	type TargetList = pallet_staking::UseValidatorsMap<Self>;

	fn maybe_start_election(
		current_planned_session: SessionIndex,
		era_start_session: SessionIndex,
	) -> bool {
		let session_progress = current_planned_session - era_start_session;
		// start the election 1 session before the intended time.
		session_progress == (SessionsPerEra::get() - 1)
	}

	// TODO
	type NextNewSession = ();
	// Staking no longer has this.
	type UnixTime = TempToRemoveTimestamp;
	// type SessionInterface = Self;

	type WeightInfo = ();
}

pub struct TempToRemoveTimestamp;
impl frame::traits::UnixTime for TempToRemoveTimestamp {
	fn now() -> core::time::Duration {
		unimplemented!()
	}
}

impl pallet_staking_rc_client::Config for Runtime {
	type AHStakingInterface = Staking;
	type SendToRelayChain = DeliverToRelay;
	type RelayChainOrigin = EnsureRoot<AccountId>;
}

pub struct DeliverToRelay;
impl pallet_staking_rc_client::SendToRelayChain for DeliverToRelay {
	type AccountId = AccountId;

	fn validator_set(report: pallet_staking_rc_client::ValidatorSetReport<Self::AccountId>) {
		todo!();
	}
}

const INITIAL_BALANCE: Balance = 1000;
const INITIAL_STAKE: Balance = 100;

pub struct ExtBuilder {}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self {}
	}
}

impl ExtBuilder {
	pub fn build(self) -> TestState {
		let _ = sp_tracing::try_init_simple();
		let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

		let validators = vec![1, 2, 3, 4, 5, 6, 7, 8]
			.into_iter()
			.map(|x| (x, x, INITIAL_STAKE, pallet_staking::StakerStatus::Validator));

		let nominators = vec![
			(100, vec![1, 2]),
			(101, vec![2, 5]),
			(102, vec![1, 1]),
			(103, vec![3, 3]),
			(104, vec![1, 5]),
			(105, vec![5, 4]),
			(106, vec![6, 2]),
			(107, vec![1, 6]),
			(108, vec![2, 7]),
			(109, vec![4, 8]),
			(110, vec![5, 2]),
			(111, vec![6, 6]),
			(112, vec![8, 1]),
		]
		.into_iter()
		.map(|(x, y)| {
			let y = y.into_iter().map(|x| x).collect::<Vec<_>>();
			(x, x, INITIAL_STAKE, pallet_staking::StakerStatus::Nominator(y))
		});

		let stakers = validators.chain(nominators).collect::<Vec<_>>();
		let balances = stakers
			.clone()
			.into_iter()
			.map(|(x, _, _, _)| (x, INITIAL_BALANCE))
			.collect::<Vec<_>>();

		pallet_balances::GenesisConfig::<Runtime> { balances, ..Default::default() }
			.assimilate_storage(&mut t)
			.unwrap();
		pallet_staking::GenesisConfig::<Runtime> {
			stakers,
			validator_count: 4,
			minimum_validator_count: 2,
			..Default::default()
		}
		.assimilate_storage(&mut t)
		.unwrap();

		let state = t.into();

		state
	}
}
