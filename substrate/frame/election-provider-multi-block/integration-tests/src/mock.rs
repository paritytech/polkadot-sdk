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

#![allow(dead_code)]

use frame_support::{
	assert_ok, parameter_types, traits,
	traits::{Hooks, VariantCountOf},
	weights::constants,
};
use frame_system::EnsureRoot;
use sp_core::{ConstU32, Get};
use sp_npos_elections::VoteWeight;
use sp_runtime::{
	offchain::{
		testing::{OffchainState, PoolState, TestOffchainExt, TestTransactionPoolExt},
		OffchainDbExt, OffchainWorkerExt, TransactionPoolExt,
	},
	testing,
	traits::Zero,
	transaction_validity, BuildStorage, PerU16, Perbill, Percent,
};
use sp_staking::{
	offence::{OffenceDetails, OnOffenceHandler},
	EraIndex, SessionIndex,
};
use std::collections::BTreeMap;

use codec::Decode;
use frame_election_provider_support::{
	bounds::ElectionBoundsBuilder, onchain, ElectionDataProvider, ExtendedBalance, PageIndex,
	SequentialPhragmen, Weight,
};
use sp_npos_elections::ElectionScore;

use pallet_election_provider_multi_block::{
	self as epm_core_pallet,
	signed::{self as epm_signed_pallet},
	unsigned::{self as epm_unsigned_pallet, miner},
	verifier::{self as epm_verifier_pallet},
	Config, Phase,
};

use pallet_staking::StakerStatus;
use parking_lot::RwLock;
use std::sync::Arc;

use frame_support::derive_impl;

use crate::{log, log_current_time};

pub const INIT_TIMESTAMP: BlockNumber = 30_000;
pub const BLOCK_TIME: BlockNumber = 1000;

type Block = frame_system::mocking::MockBlockU32<Runtime>;
type Extrinsic = testing::TestXt<RuntimeCall, ()>;
pub(crate) type T = Runtime;

frame_support::construct_runtime!(
	pub enum Runtime {
		System: frame_system,

		// EPM core and sub-pallets
		ElectionProvider: epm_core_pallet,
		VerifierPallet: epm_verifier_pallet,
		SignedPallet: epm_signed_pallet,
		UnsignedPallet: epm_unsigned_pallet,

		Pools: pallet_nomination_pools,
		Staking: pallet_staking,
		Balances: pallet_balances,
		BagsList: pallet_bags_list,
		Session: pallet_session,
		Historical: pallet_session::historical,
		Timestamp: pallet_timestamp,
	}
);

pub(crate) type AccountId = u64;
pub(crate) type AccountIndex = u32;
pub(crate) type BlockNumber = u32;
pub(crate) type Balance = u64;
pub(crate) type VoterIndex = u16;
pub(crate) type TargetIndex = u16;
pub(crate) type Moment = u32;

pub type Solver = SequentialPhragmen<AccountId, sp_runtime::PerU16>;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
}

const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);
parameter_types! {
	pub static ExistentialDeposit: Balance = 1;
	pub BlockWeights: frame_system::limits::BlockWeights = frame_system::limits::BlockWeights
		::with_sensible_defaults(
			Weight::from_parts(2u64 * constants::WEIGHT_REF_TIME_PER_SECOND, u64::MAX),
			NORMAL_DISPATCH_RATIO,
		);
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type MaxFreezes = VariantCountOf<RuntimeFreezeReason>;
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type FreezeIdentifier = RuntimeFreezeReason;
}

impl pallet_timestamp::Config for Runtime {
	type Moment = Moment;
	type OnTimestampSet = ();
	type MinimumPeriod = traits::ConstU32<5>;
	type WeightInfo = ();
}

parameter_types! {
	pub static Period: u32 = 30;
	pub static Offset: u32 = 0;
}

sp_runtime::impl_opaque_keys! {
	pub struct SessionKeys {
		pub other: OtherSessionHandler,
	}
}

impl pallet_session::Config for Runtime {
	type SessionManager = pallet_session::historical::NoteHistoricalRoot<Runtime, Staking>;
	type Keys = SessionKeys;
	type ShouldEndSession = pallet_session::PeriodicSessions<Period, Offset>;
	type NextSessionRotation = pallet_session::PeriodicSessions<Period, Offset>;
	type SessionHandler = (OtherSessionHandler,);
	type RuntimeEvent = RuntimeEvent;
	type ValidatorId = AccountId;
	type ValidatorIdOf = pallet_staking::StashOf<Runtime>;
	type WeightInfo = ();
}
impl pallet_session::historical::Config for Runtime {
	type FullIdentification = pallet_staking::Exposure<AccountId, Balance>;
	type FullIdentificationOf = pallet_staking::ExposureOf<Runtime>;
}

frame_election_provider_support::generate_solution_type!(
	#[compact]
	pub struct MockNposSolution::<
		VoterIndex = VoterIndex,
		TargetIndex = TargetIndex,
		Accuracy = PerU16,
		MaxVoters = ConstU32::<2_000>
	>(6)
);

parameter_types! {
	pub static SignedPhase: BlockNumber = 10;
	pub static UnsignedPhase: BlockNumber = 10;
	pub static SignedValidationPhase: BlockNumber = Pages::get().into();
	pub static Lookhaead: BlockNumber = Pages::get();
	pub static VoterSnapshotPerBlock: VoterIndex = 4;
	pub static TargetSnapshotPerBlock: TargetIndex = 8;
	pub static Pages: PageIndex = 3;
	pub static ExportPhaseLimit: BlockNumber = (Pages::get() * 2u32).into();

	// TODO: remove what's not needed from here down:

	// we expect a minimum of 3 blocks in signed phase and unsigned phases before trying
	// entering in emergency phase after the election failed.
	pub static MinBlocksBeforeEmergency: BlockNumber = 3;
	#[derive(Debug)]
	pub static MaxVotesPerVoter: u32 = 16;
	pub static SignedFixedDeposit: Balance = 1;
	pub static SignedDepositIncreaseFactor: Percent = Percent::from_percent(10);
	pub static ElectionBounds: frame_election_provider_support::bounds::ElectionBounds = ElectionBoundsBuilder::default()
		.voters_count(1_000.into()).targets_count(1_000.into()).build();
}

pub struct EPMBenchmarkingConfigs;
impl pallet_election_provider_multi_block::BenchmarkingConfig for EPMBenchmarkingConfigs {
	const VOTERS: u32 = 100;
	const TARGETS: u32 = 50;
	const VOTERS_PER_PAGE: [u32; 2] = [1, 5];
	const TARGETS_PER_PAGE: [u32; 2] = [1, 8];
}

impl epm_core_pallet::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SignedPhase = SignedPhase;
	type UnsignedPhase = UnsignedPhase;
	type SignedValidationPhase = SignedValidationPhase;
	type Lookhaead = Lookhaead;
	type VoterSnapshotPerBlock = VoterSnapshotPerBlock;
	type TargetSnapshotPerBlock = TargetSnapshotPerBlock;
	type Pages = Pages;
	type ExportPhaseLimit = ExportPhaseLimit;
	type MaxWinnersPerPage = MaxWinnersPerPage;
	type MaxBackersPerWinner = MaxBackersPerWinner;
	type MinerConfig = Self;
	type Fallback = frame_election_provider_support::NoElection<(
		AccountId,
		BlockNumber,
		Staking,
		MaxWinnersPerPage,
		MaxBackersPerWinner,
	)>;
	type Verifier = VerifierPallet;
	type DataProvider = Staking;
	type BenchmarkingConfig = EPMBenchmarkingConfigs;
	type WeightInfo = ();
}

parameter_types! {
	pub static SolutionImprovementThreshold: Perbill = Perbill::zero();
	pub static MaxWinnersPerPage: u32 = 4;
	pub static MaxBackersPerWinner: u32 = 16;
}

impl epm_verifier_pallet::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ForceOrigin = frame_system::EnsureRoot<AccountId>;
	type SolutionImprovementThreshold = SolutionImprovementThreshold;
	type SolutionDataProvider = SignedPallet;
	type WeightInfo = ();
}

parameter_types! {
	pub static DepositBase: Balance = 10;
	pub static DepositPerPage: Balance = 1;
	pub static Reward: Balance = 10;
	pub static MaxSubmissions: u32 = 5;
}

impl epm_signed_pallet::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type EstimateCallFee = ConstU32<8>;
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
		DepositBase::get()
	}
}

parameter_types! {
	pub static OffchainRepeatInterval: BlockNumber = 0;
	pub static TransactionPriority: transaction_validity::TransactionPriority = 1;
	pub static MinerMaxLength: u32 = 256;
	pub static MinerMaxWeight: Weight = BlockWeights::get().max_block;
}

impl epm_unsigned_pallet::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OffchainRepeatInterval = OffchainRepeatInterval;
	type MinerTxPriority = TransactionPriority;
	type MaxLength = MinerMaxLength;
	type MaxWeight = MinerMaxWeight;
	type WeightInfo = ();
}

impl miner::Config for Runtime {
	type AccountId = AccountId;
	type Solution = MockNposSolution;
	type Solver = Solver;
	type Pages = Pages;
	type MaxVotesPerVoter = ConstU32<16>;
	type MaxWinnersPerPage = MaxWinnersPerPage;
	type MaxBackersPerWinner = MaxBackersPerWinner;
	type VoterSnapshotPerBlock = VoterSnapshotPerBlock;
	type TargetSnapshotPerBlock = TargetSnapshotPerBlock;
	type MaxWeight = MinerMaxWeight;
	type MaxLength = MinerMaxLength;
}

const THRESHOLDS: [VoteWeight; 9] = [10, 20, 30, 40, 50, 60, 1_000, 2_000, 10_000];

parameter_types! {
	pub static BagThresholds: &'static [sp_npos_elections::VoteWeight] = &THRESHOLDS;
	pub const SessionsPerEra: sp_staking::SessionIndex = 2;
	pub const BondingDuration: sp_staking::EraIndex = 28;
	pub const SlashDeferDuration: sp_staking::EraIndex = 7; // 1/4 the bonding duration.
}

impl pallet_bags_list::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type ScoreProvider = Staking;
	type BagThresholds = BagThresholds;
	type Score = VoteWeight;
}

pub struct BalanceToU256;
impl sp_runtime::traits::Convert<Balance, sp_core::U256> for BalanceToU256 {
	fn convert(n: Balance) -> sp_core::U256 {
		n.into()
	}
}

pub struct U256ToBalance;
impl sp_runtime::traits::Convert<sp_core::U256, Balance> for U256ToBalance {
	fn convert(n: sp_core::U256) -> Balance {
		n.try_into().unwrap()
	}
}

parameter_types! {
	pub const PoolsPalletId: frame_support::PalletId = frame_support::PalletId(*b"py/nopls");
	pub static MaxUnbonding: u32 = 8;
}

impl pallet_nomination_pools::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type Currency = Balances;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type RewardCounter = sp_runtime::FixedU128;
	type BalanceToU256 = BalanceToU256;
	type U256ToBalance = U256ToBalance;
	type StakeAdapter = pallet_nomination_pools::adapter::TransferStake<Self, Staking>;
	type PostUnbondingPoolsWindow = ConstU32<2>;
	type PalletId = PoolsPalletId;
	type MaxMetadataLen = ConstU32<256>;
	type MaxUnbonding = MaxUnbonding;
	type MaxPointsToBalance = frame_support::traits::ConstU8<10>;
	type AdminOrigin = frame_system::EnsureRoot<Self::AccountId>;
}

parameter_types! {
	pub static MaxUnlockingChunks: u32 = 32;
	pub MaxControllersInDeprecationBatch: u32 = 5900;
	pub static MaxValidatorSet: u32 = 500;
}

/// Upper limit on the number of NPOS nominations.
const MAX_QUOTA_NOMINATIONS: u32 = 16;
/// Disabling factor set explicitly to byzantine threshold
pub(crate) const SLASHING_DISABLING_FACTOR: usize = 3;

#[derive_impl(pallet_staking::config_preludes::TestDefaultConfig)]
impl pallet_staking::Config for Runtime {
	type Currency = Balances;
	type CurrencyBalance = Balance;
	type UnixTime = Timestamp;
	type SessionsPerEra = SessionsPerEra;
	type BondingDuration = BondingDuration;
	type SlashDeferDuration = SlashDeferDuration;
	type AdminOrigin = EnsureRoot<AccountId>; // root can cancel slashes
	type SessionInterface = Self;
	type EraPayout = ();
	type NextNewSession = Session;
	type MaxExposurePageSize = ConstU32<256>;
	type MaxValidatorSet = MaxValidatorSet;
	type ElectionProvider = ElectionProvider;
	type GenesisElectionProvider = onchain::OnChainExecution<OnChainSeqPhragmen>;
	type VoterList = BagsList;
	type NominationsQuota = pallet_staking::FixedNominationsQuota<MAX_QUOTA_NOMINATIONS>;
	type TargetList = pallet_staking::UseValidatorsMap<Self>;
	type MaxUnlockingChunks = MaxUnlockingChunks;
	type EventListeners = Pools;
	type WeightInfo = pallet_staking::weights::SubstrateWeight<Runtime>;
	type DisablingStrategy = pallet_staking::UpToLimitDisablingStrategy<SLASHING_DISABLING_FACTOR>;
	type BenchmarkingConfig = pallet_staking::TestBenchmarkingConfig;
}

impl<LocalCall> frame_system::offchain::SendTransactionTypes<LocalCall> for Runtime
where
	RuntimeCall: From<LocalCall>,
{
	type OverarchingCall = RuntimeCall;
	type Extrinsic = Extrinsic;
}

pub struct OnChainSeqPhragmen;

parameter_types! {
	pub static VotersBound: u32 = 600;
	pub static TargetsBound: u32 = 400;
}

impl onchain::Config for OnChainSeqPhragmen {
	type System = Runtime;
	type Solver = Solver;
	type DataProvider = Staking;
	type WeightInfo = ();
	type Bounds = ElectionBounds;
	type MaxBackersPerWinner = MaxBackersPerWinner;
	type MaxWinnersPerPage = MaxWinnersPerPage;
}

pub struct OtherSessionHandler;
impl traits::OneSessionHandler<AccountId> for OtherSessionHandler {
	type Key = testing::UintAuthorityId;

	fn on_genesis_session<'a, I: 'a>(_: I)
	where
		I: Iterator<Item = (&'a AccountId, Self::Key)>,
		AccountId: 'a,
	{
	}

	fn on_new_session<'a, I: 'a>(_: bool, _: I, _: I)
	where
		I: Iterator<Item = (&'a AccountId, Self::Key)>,
		AccountId: 'a,
	{
	}

	fn on_disabled(_validator_index: u32) {}
}

impl sp_runtime::BoundToRuntimeAppPublic for OtherSessionHandler {
	type Public = testing::UintAuthorityId;
}

pub struct StakingExtBuilder {
	validator_count: u32,
	minimum_validator_count: u32,
	min_nominator_bond: Balance,
	min_validator_bond: Balance,
	status: BTreeMap<AccountId, StakerStatus<AccountId>>,
	stakes: BTreeMap<AccountId, Balance>,
	stakers: Vec<(AccountId, AccountId, Balance, StakerStatus<AccountId>)>,
}

impl Default for StakingExtBuilder {
	fn default() -> Self {
		let stakers = vec![
			// (stash, ctrl, stake, status)
			(11, 11, 1000, StakerStatus::<AccountId>::Validator),
			(21, 21, 1000, StakerStatus::<AccountId>::Validator),
			(31, 31, 500, StakerStatus::<AccountId>::Validator),
			(41, 41, 1500, StakerStatus::<AccountId>::Validator),
			(51, 51, 1500, StakerStatus::<AccountId>::Validator),
			(61, 61, 1500, StakerStatus::<AccountId>::Validator),
			(71, 71, 1500, StakerStatus::<AccountId>::Validator),
			(81, 81, 1500, StakerStatus::<AccountId>::Validator),
			(91, 91, 1500, StakerStatus::<AccountId>::Validator),
			(101, 101, 500, StakerStatus::<AccountId>::Validator),
			// idle validators
			(201, 201, 1000, StakerStatus::<AccountId>::Idle),
			(301, 301, 1000, StakerStatus::<AccountId>::Idle),
			// nominators
			(10, 10, 2000, StakerStatus::<AccountId>::Nominator(vec![11, 21])),
			(20, 20, 2000, StakerStatus::<AccountId>::Nominator(vec![31])),
			(30, 30, 2000, StakerStatus::<AccountId>::Nominator(vec![91, 101])),
			(40, 40, 2000, StakerStatus::<AccountId>::Nominator(vec![11, 101])),
		];

		Self {
			validator_count: 6,
			minimum_validator_count: 0,
			min_nominator_bond: ExistentialDeposit::get(),
			min_validator_bond: ExistentialDeposit::get(),
			status: Default::default(),
			stakes: Default::default(),
			stakers,
		}
	}
}

impl StakingExtBuilder {
	pub fn validator_count(mut self, n: u32) -> Self {
		self.validator_count = n;
		self
	}
}

pub struct EpmExtBuilder {}

impl Default for EpmExtBuilder {
	fn default() -> Self {
		EpmExtBuilder {}
	}
}

impl EpmExtBuilder {
	pub fn disable_emergency_throttling(self) -> Self {
		<MinBlocksBeforeEmergency>::set(0);
		self
	}

	pub fn phases(self, signed: BlockNumber, unsigned: BlockNumber) -> Self {
		<SignedPhase>::set(signed);
		<UnsignedPhase>::set(unsigned);
		self
	}
}

pub struct BalancesExtBuilder {
	balances: Vec<(AccountId, Balance)>,
}

impl Default for BalancesExtBuilder {
	fn default() -> Self {
		let balances = vec![
			// (account_id, balance)
			(1, 10),
			(2, 20),
			(3, 300),
			(4, 400),
			// nominators
			(10, 10_000),
			(20, 10_000),
			(30, 10_000),
			(40, 10_000),
			(50, 10_000),
			(60, 10_000),
			(70, 10_000),
			(80, 10_000),
			(90, 10_000),
			(100, 10_000),
			(200, 10_000),
			// validators
			(11, 1000),
			(21, 2000),
			(31, 3000),
			(41, 4000),
			(51, 5000),
			(61, 6000),
			(71, 7000),
			(81, 8000),
			(91, 9000),
			(101, 10000),
			(201, 20000),
			(301, 20000),
			// This allows us to have a total_payout different from 0.
			(999, 1_000_000_000_000),
		];
		Self { balances }
	}
}

pub struct ExtBuilder {
	staking_builder: StakingExtBuilder,
	epm_builder: EpmExtBuilder,
	balances_builder: BalancesExtBuilder,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self {
			staking_builder: StakingExtBuilder::default(),
			epm_builder: EpmExtBuilder::default(),
			balances_builder: BalancesExtBuilder::default(),
		}
	}
}

impl ExtBuilder {
	pub fn build(&self) -> sp_io::TestExternalities {
		sp_tracing::try_init_simple();
		let mut storage = frame_system::GenesisConfig::<T>::default().build_storage().unwrap();

		let _ = pallet_balances::GenesisConfig::<T> {
			balances: self.balances_builder.balances.clone(),
		}
		.assimilate_storage(&mut storage);

		let mut stakers = self.staking_builder.stakers.clone();
		self.staking_builder.status.clone().into_iter().for_each(|(stash, status)| {
			let (_, _, _, ref mut prev_status) = stakers
				.iter_mut()
				.find(|s| s.0 == stash)
				.expect("set_status staker should exist; qed");
			*prev_status = status;
		});
		// replaced any of the stakes if needed.
		self.staking_builder.stakes.clone().into_iter().for_each(|(stash, stake)| {
			let (_, _, ref mut prev_stake, _) = stakers
				.iter_mut()
				.find(|s| s.0 == stash)
				.expect("set_stake staker should exits; qed.");
			*prev_stake = stake;
		});

		let _ = pallet_staking::GenesisConfig::<T> {
			stakers: stakers.clone(),
			validator_count: self.staking_builder.validator_count,
			minimum_validator_count: self.staking_builder.minimum_validator_count,
			slash_reward_fraction: Perbill::from_percent(10),
			min_nominator_bond: self.staking_builder.min_nominator_bond,
			min_validator_bond: self.staking_builder.min_validator_bond,
			..Default::default()
		}
		.assimilate_storage(&mut storage);

		let _ = pallet_session::GenesisConfig::<T> {
			// set the keys for the first session.
			keys: stakers
				.into_iter()
				.map(|(id, ..)| (id, id, SessionKeys { other: (id as u64).into() }))
				.collect(),
			..Default::default()
		}
		.assimilate_storage(&mut storage);

		let mut ext = sp_io::TestExternalities::from(storage);

		// We consider all test to start after timestamp is initialized This must be ensured by
		// having `timestamp::on_initialize` called before `staking::on_initialize`.
		ext.execute_with(|| {
			System::set_block_number(1);
			Session::on_initialize(1);
			<Staking as Hooks<u32>>::on_initialize(1);
			Timestamp::set_timestamp(INIT_TIMESTAMP);
		});

		ext
	}

	pub fn staking(mut self, builder: StakingExtBuilder) -> Self {
		self.staking_builder = builder;
		self
	}

	pub fn epm(mut self, builder: EpmExtBuilder) -> Self {
		self.epm_builder = builder;
		self
	}

	pub fn balances(mut self, builder: BalancesExtBuilder) -> Self {
		self.balances_builder = builder;
		self
	}

	pub fn build_offchainify(
		self,
	) -> (sp_io::TestExternalities, Arc<RwLock<PoolState>>, Arc<RwLock<OffchainState>>) {
		// add offchain and pool externality extensions.
		let mut ext = self.build();
		let (offchain, offchain_state) = TestOffchainExt::new();
		let (pool, pool_state) = TestTransactionPoolExt::new();

		ext.register_extension(OffchainDbExt::new(offchain.clone()));
		ext.register_extension(OffchainWorkerExt::new(offchain));
		ext.register_extension(TransactionPoolExt::new(pool));

		(ext, pool_state, offchain_state)
	}

	pub fn build_and_execute(self, test: impl FnOnce() -> ()) {
		let mut ext = self.build();
		ext.execute_with(test);

		#[cfg(feature = "try-runtime")]
		ext.execute_with(|| {
			let bn = System::block_number();

			assert_ok!(<MultiPhase as Hooks<u64>>::try_state(bn));
			assert_ok!(<Staking as Hooks<u64>>::try_state(bn));
			assert_ok!(<Session as Hooks<u64>>::try_state(bn));
		});
	}
}

// Progress to given block, triggering session and era changes as we progress and ensuring that
// there is a solution queued when expected.
pub fn roll_to(n: BlockNumber, delay_solution: bool) {
	for b in (System::block_number()) + 1..=n {
		System::set_block_number(b);
		Session::on_initialize(b);
		Timestamp::set_timestamp(System::block_number() * BLOCK_TIME + INIT_TIMESTAMP);

		if ElectionProvider::current_phase() == Phase::Signed && !delay_solution {
			let _ = try_submit_paged_solution().map_err(|e| {
				log!(info, "failed to mine/queue solution: {:?}", e);
			});
		};

		ElectionProvider::on_initialize(b);
		VerifierPallet::on_initialize(b);
		SignedPallet::on_initialize(b);
		UnsignedPallet::on_initialize(b);

		Staking::on_initialize(b);
		if b != n {
			Staking::on_finalize(System::block_number());
		}

		log_current_time();
	}
}

// Progress to given block, triggering session and era changes as we progress and ensuring that
// there is a solution queued when expected.
pub fn roll_to_with_ocw(n: BlockNumber, pool: Arc<RwLock<PoolState>>, delay_solution: bool) {
	for b in (System::block_number()) + 1..=n {
		System::set_block_number(b);
		Session::on_initialize(b);
		Timestamp::set_timestamp(System::block_number() * BLOCK_TIME + INIT_TIMESTAMP);

		ElectionProvider::on_initialize(b);
		VerifierPallet::on_initialize(b);
		SignedPallet::on_initialize(b);
		UnsignedPallet::on_initialize(b);

		ElectionProvider::offchain_worker(b);

		if !delay_solution && pool.read().transactions.len() > 0 {
			// decode submit_unsigned callable that may be queued in the pool by ocw. skip all
			// other extrinsics in the pool.
			for encoded in &pool.read().transactions {
				let _extrinsic = Extrinsic::decode(&mut &encoded[..]).unwrap();

				// TODO(gpestana): fix when EPM sub-pallets are ready
				//let _ = match extrinsic.call {
				//	RuntimeCall::ElectionProvider(
				//		call @ Call::submit_unsigned { .. },
				//	) => {
				//		// call submit_unsigned callable in OCW pool.
				//		assert_ok!(call.dispatch_bypass_filter(RuntimeOrigin::none()));
				//	},
				//	_ => (),
				//};
			}

			pool.try_write().unwrap().transactions.clear();
		}

		Staking::on_initialize(b);
		if b != n {
			Staking::on_finalize(System::block_number());
		}

		log_current_time();
	}
}
// helper to progress one block ahead.
pub fn roll_one(pool: Option<Arc<RwLock<PoolState>>>, delay_solution: bool) {
	let bn = System::block_number().saturating_add(1);
	match pool {
		None => roll_to(bn, delay_solution),
		Some(pool) => roll_to_with_ocw(bn, pool, delay_solution),
	}
}

/// Progresses from the current block number (whatever that may be) to the block where the session
/// `session_index` starts.
pub(crate) fn start_session(
	session_index: SessionIndex,
	pool: Arc<RwLock<PoolState>>,
	delay_solution: bool,
) {
	let end = if Offset::get().is_zero() {
		Period::get() * session_index
	} else {
		Offset::get() * session_index + Period::get() * session_index
	};

	assert!(end >= System::block_number());

	roll_to_with_ocw(end, pool, delay_solution);

	// session must have progressed properly.
	assert_eq!(
		Session::current_index(),
		session_index,
		"current session index = {}, expected = {}",
		Session::current_index(),
		session_index,
	);
}

/// Go one session forward.
pub(crate) fn advance_session(pool: Arc<RwLock<PoolState>>) {
	let current_index = Session::current_index();
	start_session(current_index + 1, pool, false);
}

pub(crate) fn advance_session_delayed_solution(pool: Arc<RwLock<PoolState>>) {
	let current_index = Session::current_index();
	start_session(current_index + 1, pool, true);
}

pub(crate) fn start_next_active_era(pool: Arc<RwLock<PoolState>>) -> Result<(), ()> {
	start_active_era(active_era() + 1, pool, false)
}

pub(crate) fn start_next_active_era_delayed_solution(
	pool: Arc<RwLock<PoolState>>,
) -> Result<(), ()> {
	start_active_era(active_era() + 1, pool, true)
}

pub(crate) fn advance_eras(n: usize, pool: Arc<RwLock<PoolState>>) {
	for _ in 0..n {
		assert_ok!(start_next_active_era(pool.clone()));
	}
}

/// Progress until the given era.
pub(crate) fn start_active_era(
	era_index: EraIndex,
	pool: Arc<RwLock<PoolState>>,
	delay_solution: bool,
) -> Result<(), ()> {
	let era_before = current_era();

	start_session((era_index * <SessionsPerEra as Get<u32>>::get()).into(), pool, delay_solution);

	log!(
		info,
		"start_active_era - era_before: {}, current era: {} -> progress to: {} -> after era: {}",
		era_before,
		active_era(),
		era_index,
		current_era(),
	);

	// if the solution was not delayed, era should have progressed.
	if !delay_solution && (active_era() != era_index || current_era() != active_era()) {
		Err(())
	} else {
		Ok(())
	}
}

pub(crate) fn active_era() -> EraIndex {
	Staking::active_era().unwrap().index
}

pub(crate) fn current_era() -> EraIndex {
	Staking::current_era().unwrap()
}

// Fast forward until a given election phase.
pub fn roll_to_phase(phase: Phase<BlockNumber>, delay: bool) {
	while ElectionProvider::current_phase() != phase {
		roll_to(System::block_number() + 1, delay);
	}
}

pub fn election_prediction() -> BlockNumber {
	<<T as epm_core_pallet::Config>::DataProvider as ElectionDataProvider>::next_election_prediction(
		System::block_number(),
	)
}

parameter_types! {
	pub static LastSolutionSubmittedFor: Option<u32> = None;
}

pub(crate) fn try_submit_paged_solution() -> Result<(), ()> {
	let submit = || {
		// TODO: to finish.
		let (paged_solution, _) =
			miner::Miner::<<T as Config>::MinerConfig>::mine_paged_solution_with_snapshot(
				voters_snapshot,
				targets_snapshot,
				Pages::get(),
				round,
				desired_targets,
				false,
			)
			.unwrap();

		let _ = SignedPallet::register(RuntimeOrigin::signed(10), paged_solution.score).unwrap();

		for (idx, page) in paged_solution.solution_pages.into_iter().enumerate() {}
		log!(
			info,
			"submitter: successfully submitted {} pages with {:?} score in round {}.",
			Pages::get(),
			paged_solution.score,
			ElectionProvider::current_round(),
		);
	};

	match LastSolutionSubmittedFor::get() {
		Some(submitted_at) => {
			if submitted_at == ElectionProvider::current_round() {
				// solution already submitted in this round, do nothing.
			} else {
				// haven't submit in this round, submit it.
				submit()
			}
		},
		// never submitted, do it.
		None => submit(),
	};
	LastSolutionSubmittedFor::set(Some(ElectionProvider::current_round()));

	Ok(())
}

pub(crate) fn on_offence_now(
	offenders: &[OffenceDetails<AccountId, pallet_session::historical::IdentificationTuple<T>>],
	slash_fraction: &[Perbill],
) {
	let now = Staking::active_era().unwrap().index;
	let _ = Staking::on_offence(
		offenders,
		slash_fraction,
		Staking::eras_start_session_index(now).unwrap(),
	);
}

// Add offence to validator, slash it.
pub(crate) fn add_slash(who: &AccountId) {
	on_offence_now(
		&[OffenceDetails {
			offender: (*who, Staking::eras_stakers(active_era(), who)),
			reporters: vec![],
		}],
		&[Perbill::from_percent(10)],
	);
}

// Slashes 1/2 of the active set. Returns the `AccountId`s of the slashed validators.
pub(crate) fn slash_half_the_active_set() -> Vec<AccountId> {
	let mut slashed = Session::validators();
	slashed.truncate(slashed.len() / 2);

	for v in slashed.iter() {
		add_slash(v);
	}

	slashed
}

// Slashes a percentage of the active nominators that haven't been slashed yet, with
// a minimum of 1 validator slash.
pub(crate) fn slash_percentage(percentage: Perbill) -> Vec<AccountId> {
	let validators = Session::validators();
	let mut remaining_slashes = (percentage * validators.len() as u32).max(1);
	let mut slashed = vec![];

	for v in validators.into_iter() {
		if remaining_slashes != 0 {
			add_slash(&v);
			slashed.push(v);
			remaining_slashes -= 1;
		}
	}
	slashed
}

pub(crate) fn set_minimum_election_score(
	_minimal_stake: ExtendedBalance,
	_sum_stake: ExtendedBalance,
	_sum_stake_squared: ExtendedBalance,
) -> Result<(), ()> {
	todo!()
}

pub(crate) fn staking_events() -> Vec<pallet_staking::Event<T>> {
	System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(|e| if let RuntimeEvent::Staking(inner) = e { Some(inner) } else { None })
		.collect::<Vec<_>>()
}

pub(crate) fn epm_events() -> Vec<pallet_election_provider_multi_block::Event<T>> {
	System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(
			|e| {
				if let RuntimeEvent::ElectionProvider(inner) = e {
					Some(inner)
				} else {
					None
				}
			},
		)
		.collect::<Vec<_>>()
}
