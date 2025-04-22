// This file is part of Substrate.

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

use crate::shared;
use frame::testing_prelude::*;
use frame_election_provider_support::{
	bounds::{ElectionBounds, ElectionBoundsBuilder},
	SequentialPhragmen,
};
use frame_support::sp_runtime::testing::TestXt;
use pallet_election_provider_multi_block as multi_block;
use pallet_staking_async::Forcing;
use pallet_staking_async_rc_client::{SessionReport, ValidatorSetReport};
use sp_staking::SessionIndex;

construct_runtime! {
	pub enum Runtime {
		System: frame_system,
		Balances: pallet_balances,

		Staking: pallet_staking_async,
		RcClient: pallet_staking_async_rc_client,

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

/// Use the given `end_index` as the first session report, and increment as per needed.
pub(crate) fn roll_until_next_active(mut end_index: SessionIndex) -> Vec<AccountId> {
	// receive enough session reports, such that we plan a new era
	let planned_era = pallet_staking_async::session_rotation::Rotator::<Runtime>::planning_era();
	let active_era = pallet_staking_async::session_rotation::Rotator::<Runtime>::active_era();

	while pallet_staking_async::session_rotation::Rotator::<Runtime>::planning_era() == planned_era
	{
		let report = SessionReport {
			end_index,
			activation_timestamp: None,
			leftover: false,
			validator_points: Default::default(),
		};
		assert_ok!(pallet_staking_async_rc_client::Pallet::<Runtime>::relay_session_report(
			RuntimeOrigin::root(),
			report
		));
		roll_next();
		end_index += 1;
	}

	// now we have planned a new session. Roll until we have an outgoing message ready, meaning the
	// election is done
	LocalQueue::flush();
	loop {
		let messages = LocalQueue::get_since_last_call();
		match messages.len() {
			0 => {
				roll_next();
				continue;
			},
			1 => {
				assert_eq!(
					messages[0],
					(
						System::block_number(),
						OutgoingMessages::ValidatorSet(ValidatorSetReport {
							id: planned_era + 1,
							leftover: false,
							// arbitrary, feel free to change if test setup updates
							new_validator_set: vec![3, 5, 6, 8],
							prune_up_to: None,
						})
					)
				);
				break
			},
			_ => panic!("Expected only one message in local queue, but got: {:?}", messages),
		}
	}

	// active era is still 0
	assert_eq!(
		pallet_staking_async::session_rotation::Rotator::<Runtime>::active_era(),
		active_era
	);

	// rc will not tell us that it has instantly activated a validator set.
	let report = SessionReport {
		end_index,
		activation_timestamp: Some((1000, planned_era + 1)),
		leftover: false,
		validator_points: Default::default(),
	};
	assert_ok!(pallet_staking_async_rc_client::Pallet::<Runtime>::relay_session_report(
		RuntimeOrigin::root(),
		report
	));

	// active era is now 1.
	assert_eq!(
		pallet_staking_async::session_rotation::Rotator::<Runtime>::active_era(),
		active_era + 1
	);

	// arbitrary, feel free to change if test setup updates
	vec![3, 5, 6, 8]
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
	type Balance = u128;
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

type MaxVotesPerVoter = pallet_staking_async::MaxNominationsOf<Runtime>;
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
	type AreWeDone = multi_block::ProceedRegardlessOf<Self>;
	type WeightInfo = multi_block::weights::AllZeroWeights;
}

impl multi_block::verifier::Config for Runtime {
	type MaxBackersPerWinner = MaxBackersPerWinner;
	type MaxBackersPerWinnerFinal = MaxBackersPerWinnerFinal;
	type MaxWinnersPerPage = MaxWinnersPerPage;

	type SolutionDataProvider = MultiBlockSigned;
	type SolutionImprovementThreshold = ();
	type WeightInfo = multi_block::weights::AllZeroWeights;
}

impl multi_block::unsigned::Config for Runtime {
	type MinerPages = ConstU32<1>;
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
	pub static PlanningEraOffset: u32 = 1;
}

impl pallet_staking_async::Config for Runtime {
	type Filter = ();
	type RuntimeHoldReason = RuntimeHoldReason;

	type AdminOrigin = EnsureRoot<AccountId>;
	type BondingDuration = BondingDuration;
	type SessionsPerEra = SessionsPerEra;
	type PlanningEraOffset = PlanningEraOffset;

	type Currency = Balances;
	type OldCurrency = Balances;
	type CurrencyBalance = Balance;
	type CurrencyToVote = ();

	type ElectionProvider = MultiBlock;

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
	type NominationsQuota = pallet_staking_async::FixedNominationsQuota<16>;

	type VoterList = pallet_staking_async::UseNominatorsAndValidatorsMap<Self>;
	type TargetList = pallet_staking_async::UseValidatorsMap<Self>;

	type RcClientInterface = RcClient;

	type WeightInfo = ();
}

impl pallet_staking_async_rc_client::Config for Runtime {
	type AHStakingInterface = Staking;
	type SendToRelayChain = DeliverToRelay;
	type RelayChainOrigin = EnsureRoot<AccountId>;
}

pub struct DeliverToRelay;
impl pallet_staking_async_rc_client::SendToRelayChain for DeliverToRelay {
	type AccountId = AccountId;

	fn validator_set(report: pallet_staking_async_rc_client::ValidatorSetReport<Self::AccountId>) {
		if let Some(mut local_queue) = LocalQueue::get() {
			local_queue.push((System::block_number(), OutgoingMessages::ValidatorSet(report)));
			LocalQueue::set(Some(local_queue));
		} else {
			shared::CounterAHRCValidatorSet::mutate(|x| *x += 1);
			shared::in_rc(|| {
				let origin = crate::rc::RuntimeOrigin::root();
				pallet_staking_async_ah_client::Pallet::<crate::rc::Runtime>::validator_set(
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
	ValidatorSet(pallet_staking_async_rc_client::ValidatorSetReport<AccountId>),
}

parameter_types! {
	pub static LocalQueue: Option<Vec<(BlockNumber, OutgoingMessages)>> = None;
	pub static LocalQueueLastIndex: usize = 0;
}

impl LocalQueue {
	pub fn get_since_last_call() -> Vec<(BlockNumber, OutgoingMessages)> {
		if let Some(all) = Self::get() {
			let last = LocalQueueLastIndex::get();
			LocalQueueLastIndex::set(all.len());
			all.into_iter().skip(last).collect()
		} else {
			panic!("Must set local_queue()!")
		}
	}

	pub fn flush() {
		let _ = Self::get_since_last_call();
	}
}

pub struct ExtBuilder {
	// if true, emulate pre-ahm-migration state
	pre_migration: bool,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self { pre_migration: false }
	}
}

impl ExtBuilder {
	/// Set this if you want to emulate pre-migration state of staking-async.
	pub fn pre_migration(self) -> Self {
		Self { pre_migration: true }
	}

	/// Set this if you want to test the ah-runtime locally. This will push outgoing messages to
	/// `LocalQueue` instead of enacting them on RC.
	pub fn local_queue(self) -> Self {
		LocalQueue::set(Some(Default::default()));
		self
	}

	pub fn slash_defer_duration(self, duration: u32) -> Self {
		SlashDeferredDuration::set(duration);
		self
	}

	pub fn build(self) -> TestState {
		let _ = sp_tracing::try_init_simple();
		let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

		// Note: The state in pallet-staking-async is retained even when pre-migration is set.
		// This does not impact the tests, but for strict accuracy, be aware that the state isn't
		// fully representative.
		let validators = vec![1, 2, 3, 4, 5, 6, 7, 8]
			.into_iter()
			.map(|x| (x, INITIAL_STAKE, pallet_staking_async::StakerStatus::Validator));

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
		.map(|(x, y)| (x, INITIAL_STAKE, pallet_staking_async::StakerStatus::Nominator(y)));

		let stakers = validators.chain(nominators).collect::<Vec<_>>();
		let balances = stakers
			.clone()
			.into_iter()
			.map(|(x, _, _)| (x, INITIAL_BALANCE))
			.collect::<Vec<_>>();

		pallet_balances::GenesisConfig::<Runtime> { balances, ..Default::default() }
			.assimilate_storage(&mut t)
			.unwrap();

		pallet_staking_async::GenesisConfig::<Runtime> {
			stakers,
			validator_count: 4,
			active_era: (0, 0, 0),
			force_era: if self.pre_migration { Forcing::ForceNone } else { Forcing::default() },
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
	static RcClientEventsIndex: usize = 0;
}

pub(crate) fn rc_client_events_since_last_call() -> Vec<pallet_staking_async_rc_client::Event<T>> {
	let all: Vec<_> = System::events()
		.into_iter()
		.filter_map(
			|r| if let RuntimeEvent::RcClient(inner) = r.event { Some(inner) } else { None },
		)
		.collect();
	let seen = RcClientEventsIndex::get();
	RcClientEventsIndex::set(all.len());
	all.into_iter().skip(seen).collect()
}

pub(crate) fn staking_events_since_last_call() -> Vec<pallet_staking_async::Event<T>> {
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
