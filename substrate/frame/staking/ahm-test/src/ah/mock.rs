use crate::shared;
use frame::testing_prelude::*;
use frame_election_provider_support::{
	bounds::{ElectionBounds, ElectionBoundsBuilder},
	SequentialPhragmen,
};
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

// alias Runtime with T.
pub type T = Runtime;

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
}

pub fn roll_many(blocks: BlockNumber) {
	let current = System::block_number();
	while System::block_number() < current + blocks {
		roll_next();
	}
}

pub fn roll_until_matches(criteria: impl Fn() -> bool, with_rc: bool) {
	while !criteria() {
		roll_next();
		if with_rc {
			if LocalQueue::get().is_some() {
				panic!("when local queue is set, you cannot roll ah forward as well!")
			}
			shared::in_rc(|| {
				crate::rc::roll_next();
			});
		}
	}
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

parameter_types! {
	pub Bounds: ElectionBounds = ElectionBoundsBuilder::default().build();
}

pub struct OnChainConfig;
impl frame_election_provider_support::onchain::Config for OnChainConfig {
	// unbounded
	type Bounds = Bounds;
	// We should not need sorting, as our bounds are large enough for the number of
	// nominators/validators in this test setup.
	type Sort = ConstBool<false>;
	type DataProvider = Staking;
	type MaxBackersPerWinner = MaxBackersPerWinner;
	type MaxWinnersPerPage = MaxWinnersPerPage;
	type Solver = SequentialPhragmen<AccountId, Perbill>;
	type System = Runtime;
	type WeightInfo = ();
}

impl multi_block::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;

	type AdminOrigin = EnsureRoot<AccountId>;
	type DataProvider = Staking;
	type Fallback = frame_election_provider_support::onchain::OnChainExecution<OnChainConfig>;
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
	pub static MaxSubmissions: u32 = 2;
	pub static RewardBase: Balance = 5;
}

impl multi_block::signed::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeHoldReason = RuntimeHoldReason;

	type Currency = Balances;

	type EjectGraceRatio = ();
	type BailoutGraceRatio = ();
	type DepositBase = DepositBase;
	type DepositPerPage = DepositPerPage;
	type EstimateCallFee = ConstU32<1>;
	type MaxSubmissions = MaxSubmissions;
	type RewardBase = RewardBase;
	type WeightInfo = multi_block::weights::AllZeroWeights;
}

parameter_types! {
	pub static BondingDuration: u32 = 3;
	pub static SlashDeferredDuration: u32 = 2;
	pub static SessionsPerEra: u32 = 6;
	pub static ElectionOffset: u32 = 1;
}

impl pallet_staking::Config for Runtime {
	type Filter = ();
	type RuntimeEvent = RuntimeEvent;
	type RuntimeHoldReason = RuntimeHoldReason;

	type AdminOrigin = EnsureRoot<AccountId>;
	type BenchmarkingConfig = pallet_staking::TestBenchmarkingConfig;
	type BondingDuration = BondingDuration;
	type SessionsPerEra = SessionsPerEra;
	type ElectionOffset = ElectionOffset;

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

	type RcClientInterface = RcClient;

	// TODO
	type NextNewSession = ();

	type WeightInfo = ();
}

impl pallet_staking_rc_client::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type AHStakingInterface = Staking;
	type SendToRelayChain = DeliverToRelay;
	type RelayChainOrigin = EnsureRoot<AccountId>;
}

pub struct DeliverToRelay;
impl pallet_staking_rc_client::SendToRelayChain for DeliverToRelay {
	type AccountId = AccountId;

	fn validator_set(report: pallet_staking_rc_client::ValidatorSetReport<Self::AccountId>) {
		if let Some(mut local_queue) = LocalQueue::get() {
			local_queue.push((System::block_number(), OutgoingMessages::ValidatorSet(report)));
			LocalQueue::set(Some(local_queue));
		} else {
			shared::in_rc(|| {
				let origin = crate::rc::RuntimeOrigin::root();
				pallet_staking_ah_client::Pallet::<crate::rc::Runtime>::validator_set(
					origin,
					report.clone(),
				)
				.unwrap();
			});
		}
	}
}

const INITIAL_BALANCE: Balance = 1000;
const INITIAL_STAKE: Balance = 100;

#[derive(Clone, Debug, PartialEq)]
pub enum OutgoingMessages {
	ValidatorSet(pallet_staking_rc_client::ValidatorSetReport<AccountId>),
}

parameter_types! {
	pub static LocalQueue: Option<Vec<(BlockNumber, OutgoingMessages)>> = None;
}

pub struct ExtBuilder {}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self {}
	}
}

impl ExtBuilder {
	/// Set this if you want to test the ah-runtime locally. This will push outgoing messages to
	/// `LocalQueue` instead of enacting them on RC.
	pub fn local_queue(self) -> Self {
		LocalQueue::set(Some(Default::default()));
		self
	}

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
			active_era: (0, 0, 0),
			..Default::default()
		}
		.assimilate_storage(&mut t)
		.unwrap();

		let mut state: TestState = t.into();

		state.execute_with(|| {
			// initialises events
			roll_next();
		});

		state
	}
}

parameter_types! {
	static StakingEventsIndex: usize = 0;
	static ElectionEventsIndex: usize = 0;
}
pub(crate) fn staking_events_since_last_call() -> Vec<pallet_staking::Event<T>> {
	let all: Vec<_> = System::events()
		.into_iter()
		.filter_map(|r| if let RuntimeEvent::Staking(inner) = r.event { Some(inner) } else { None })
		.collect();
	let seen = StakingEventsIndex::get();
	StakingEventsIndex::set(all.len());
	all.into_iter().skip(seen).collect()
}

pub(crate) fn election_events_since_last_call() -> Vec<multi_block::Event<T>> {
	let all: Vec<_> = System::events()
		.into_iter()
		.filter_map(
			|r| if let RuntimeEvent::MultiBlock(inner) = r.event { Some(inner) } else { None },
		)
		.collect();
	let seen = ElectionEventsIndex::get();
	ElectionEventsIndex::set(all.len());
	all.into_iter().skip(seen).collect()
}
