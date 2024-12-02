// This file is part of Substrate.

// Copyright (C) 2022 Parity Technologies (UK) Ltd.
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

#![allow(unused)]

mod staking;

use frame_election_provider_support::{bounds::ElectionBounds, onchain, SequentialPhragmen};
use sp_npos_elections::ElectionScore;
pub use staking::*;

use crate::{
	self as epm,
	signed::{self as signed_pallet, HoldReason},
	unsigned::{
		self as unsigned_pallet,
		miner::{self, Miner, MinerError, OffchainWorkerMiner},
	},
	verifier::{self as verifier_pallet},
	Config, *,
};
use frame_support::{
	derive_impl, pallet_prelude::*, parameter_types, traits::fungible::InspectHold,
	weights::RuntimeDbWeight,
};
use parking_lot::RwLock;
use sp_runtime::{
	offchain::{
		testing::{PoolState, TestOffchainExt, TestTransactionPoolExt},
		OffchainDbExt, OffchainWorkerExt, TransactionPoolExt,
	},
	BuildStorage, Perbill,
};
use std::{collections::HashMap, sync::Arc};

frame_support::construct_runtime!(
	pub struct Runtime {
		System: frame_system,
		Balances: pallet_balances,
		MultiPhase: epm,
		SignedPallet: signed_pallet,
		UnsignedPallet: unsigned_pallet,
		VerifierPallet: verifier_pallet,
	}
);

pub type AccountId = u64;
pub type Balance = u128;
pub type BlockNumber = u64;
pub type VoterIndex = u32;
pub type TargetIndex = u16;
pub type T = Runtime;
pub type Block = frame_system::mocking::MockBlock<Runtime>;
pub(crate) type Solver = SequentialPhragmen<AccountId, sp_runtime::PerU16, MaxBackersPerWinner, ()>;

pub struct Weighter;

impl Get<RuntimeDbWeight> for Weighter {
	fn get() -> RuntimeDbWeight {
		return RuntimeDbWeight { read: 12, write: 12 }
	}
}

frame_election_provider_support::generate_solution_type!(
	#[compact]
	pub struct TestNposSolution::<
		VoterIndex = VoterIndex,
		TargetIndex = TargetIndex,
		Accuracy = sp_runtime::PerU16,
		MaxVoters = frame_support::traits::ConstU32::<2_000>
	>(16)
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
	type DbWeight = Weighter;
}

parameter_types! {
	pub const ExistentialDeposit: Balance = 1;
}

impl pallet_balances::Config for Runtime {
	type Balance = Balance;
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type MaxLocks = ();
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type WeightInfo = ();
	type FreezeIdentifier = ();
	type MaxFreezes = ();
	type DoneSlashHandler = ();
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = ();
}

parameter_types! {
	// same as signed validation phase + 20 blocks for margin.
	pub static SignedPhase: BlockNumber = ((Pages::get() + 1) * MaxSubmissions::get() + 20).into();
	pub static SignedValidationPhase: BlockNumber = ((Pages::get() + 1) * MaxSubmissions::get()).into();
	pub static UnsignedPhase: BlockNumber = 5;
	pub static Lookhaead: BlockNumber = 0;
	pub static VoterSnapshotPerBlock: VoterIndex = 5;
	pub static TargetSnapshotPerBlock: TargetIndex = 8;
	pub static Pages: PageIndex = 3;
	pub static ExportPhaseLimit: BlockNumber = (Pages::get() * 2u32).into();
}

pub struct EPMBenchmarkingConfigs;
impl BenchmarkingConfig for EPMBenchmarkingConfigs {
	const VOTERS: u32 = 100;
	const TARGETS: u32 = 50;
	const VOTERS_PER_PAGE: [u32; 2] = [1, 5];
	const TARGETS_PER_PAGE: [u32; 2] = [1, 8];
}

impl Config for Runtime {
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
	type DataProvider = MockStaking;
	type MinerConfig = Self;
	type Fallback = MockFallback;
	type Verifier = VerifierPallet;
	type BenchmarkingConfig = EPMBenchmarkingConfigs;
	type WeightInfo = ();
}

parameter_types! {
	pub static SolutionImprovementThreshold: Perbill = Perbill::zero();
	pub static MaxWinnersPerPage: u32 = 100;
	pub static MaxBackersPerWinner: u32 = 1000;
}

impl crate::verifier::Config for Runtime {
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

impl crate::signed::Config for Runtime {
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

parameter_types! {
	pub OffchainRepeatInterval: BlockNumber = 10;
	pub MinerTxPriority: u64 = 0;
	pub MinerSolutionMaxLength: u32 = 10;
	pub MinerSolutionMaxWeight: Weight = Default::default();
}

impl crate::unsigned::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OffchainRepeatInterval = OffchainRepeatInterval;
	type MinerTxPriority = MinerTxPriority;
	type MaxLength = MinerSolutionMaxLength;
	type MaxWeight = MinerSolutionMaxWeight;
	type WeightInfo = ();
}

impl miner::Config for Runtime {
	type AccountId = AccountId;
	type Solution = TestNposSolution;
	type Solver = Solver;
	type Pages = Pages;
	type MaxVotesPerVoter = MaxVotesPerVoter;
	type MaxWinnersPerPage = MaxWinnersPerPage;
	type MaxBackersPerWinner = MaxBackersPerWinner;
	type VoterSnapshotPerBlock = VoterSnapshotPerBlock;
	type TargetSnapshotPerBlock = TargetSnapshotPerBlock;
	type MaxWeight = MinerSolutionMaxWeight;
	type MaxLength = MinerSolutionMaxLength;
}

pub type Extrinsic = sp_runtime::testing::TestXt<RuntimeCall, ()>;

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

pub struct ConstDepositBase;
impl sp_runtime::traits::Convert<usize, Balance> for ConstDepositBase {
	fn convert(_a: usize) -> Balance {
		DepositBase::get()
	}
}

parameter_types! {
	pub static OnChainElectionBounds: ElectionBounds = ElectionBoundsBuilder::default().build();
	pub static MaxVotesPerVoter: u32 = <TestNposSolution as frame_election_provider_support::NposSolution>::LIMIT as u32;
	pub static FallbackEnabled: bool = true;
}

impl onchain::Config for Runtime {
	type System = Runtime;
	type Solver = Solver;
	type MaxWinnersPerPage = MaxWinnersPerPage;
	type MaxBackersPerWinner = MaxBackersPerWinner;
	type Bounds = OnChainElectionBounds;
	type DataProvider = MockStaking;
	type WeightInfo = ();
}

pub struct MockFallback;
impl ElectionProvider for MockFallback {
	type AccountId = AccountId;
	type BlockNumber = BlockNumberFor<Runtime>;
	type Error = &'static str;
	type DataProvider = MockStaking;
	type Pages = ConstU32<1>;
	type MaxWinnersPerPage = MaxWinnersPerPage;
	type MaxBackersPerWinner = MaxBackersPerWinner;

	fn elect(remaining: PageIndex) -> Result<BoundedSupportsOf<Self>, Self::Error> {
		if FallbackEnabled::get() {
			onchain::OnChainExecution::<Runtime>::elect(remaining)
				.map_err(|_| "fallback election failed")
		} else {
			Err("fallback election failed (forced in mock)")
		}
	}

	fn ongoing() -> bool {
		false
	}
}

#[derive(Copy, Clone)]
pub struct ExtBuilder {
	minimum_score: Option<ElectionScore>,
	core_try_state: bool,
	signed_try_state: bool,
	verifier_try_state: bool,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self {
			core_try_state: true,
			signed_try_state: true,
			verifier_try_state: true,
			minimum_score: None,
		}
	}
}

// TODO(gpestana): separate ext builder into separate builders for each pallet.
impl ExtBuilder {
	pub(crate) fn pages(self, pages: u32) -> Self {
		Pages::set(pages);
		self
	}

	pub(crate) fn snapshot_voters_page(self, voters: VoterIndex) -> Self {
		VoterSnapshotPerBlock::set(voters);
		self
	}

	pub(crate) fn snapshot_targets_page(self, targets: TargetIndex) -> Self {
		TargetSnapshotPerBlock::set(targets);
		self
	}

	pub(crate) fn signed_phase(self, blocks: BlockNumber) -> Self {
		SignedPhase::set(blocks);
		self
	}

	pub(crate) fn validate_signed_phase(self, blocks: BlockNumber) -> Self {
		SignedValidationPhase::set(blocks);
		self
	}

	pub(crate) fn unsigned_phase(self, blocks: BlockNumber) -> Self {
		UnsignedPhase::set(blocks);
		self
	}

	pub(crate) fn lookahead(self, blocks: BlockNumber) -> Self {
		Lookhaead::set(blocks);
		self
	}

	pub(crate) fn export_limit(self, blocks: BlockNumber) -> Self {
		ExportPhaseLimit::set(blocks);
		self
	}

	pub(crate) fn max_winners_per_page(self, max: u32) -> Self {
		MaxWinnersPerPage::set(max);
		self
	}

	pub(crate) fn max_backers_per_winner(self, max: u32) -> Self {
		MaxBackersPerWinner::set(max);
		self
	}

	pub(crate) fn desired_targets(self, desired: u32) -> Self {
		DesiredTargets::set(Ok(desired));
		self
	}

	pub(crate) fn no_desired_targets(self) -> Self {
		DesiredTargets::set(Err("none"));
		self
	}

	pub(crate) fn signed_max_submissions(self, max: u32) -> Self {
		MaxSubmissions::set(max);
		self
	}

	pub(crate) fn solution_improvements_threshold(self, threshold: Perbill) -> Self {
		SolutionImprovementThreshold::set(threshold);
		self
	}

	pub(crate) fn minimum_score(mut self, score: ElectionScore) -> Self {
		self.minimum_score = Some(score);
		self
	}

	pub(crate) fn core_try_state(mut self, enable: bool) -> Self {
		self.core_try_state = enable;
		self
	}

	pub(crate) fn signed_try_state(mut self, enable: bool) -> Self {
		self.signed_try_state = enable;
		self
	}

	pub(crate) fn verifier_try_state(mut self, enable: bool) -> Self {
		self.verifier_try_state = enable;
		self
	}

	pub(crate) fn build(self) -> sp_io::TestExternalities {
		let mut storage = frame_system::GenesisConfig::<T>::default().build_storage().unwrap();
		let _ = pallet_balances::GenesisConfig::<T> {
			balances: vec![
				(10, 100_000),
				(20, 100_000),
				(30, 100_000),
				(40, 100_000),
				(50, 100_000),
				(60, 100_000),
				(70, 100_000),
				(80, 100_000),
				(90, 100_000),
				(91, 100),
				(92, 100),
				(93, 100),
				(94, 100),
				(95, 100),
				(96, 100),
				(97, 100),
				(99, 100),
				(999, 100),
				(9999, 100),
			],
		}
		.assimilate_storage(&mut storage);

		let _ = verifier_pallet::GenesisConfig::<T> {
			minimum_score: self.minimum_score,
			..Default::default()
		}
		.assimilate_storage(&mut storage);
		sp_io::TestExternalities::from(storage)
	}

	pub fn build_offchainify(
		self,
		iters: u32,
	) -> (sp_io::TestExternalities, Arc<RwLock<PoolState>>) {
		let mut ext = self.build();
		let (offchain, offchain_state) = TestOffchainExt::new();
		let (pool, pool_state) = TestTransactionPoolExt::new();

		let mut seed = [0_u8; 32];
		seed[0..4].copy_from_slice(&iters.to_le_bytes());
		offchain_state.write().seed = seed;

		ext.register_extension(OffchainDbExt::new(offchain.clone()));
		ext.register_extension(OffchainWorkerExt::new(offchain));
		ext.register_extension(TransactionPoolExt::new(pool));

		(ext, pool_state)
	}

	pub(crate) fn build_and_execute(self, test: impl FnOnce() -> ()) {
		let mut ext = self.build();
		ext.execute_with(|| roll_one());
		ext.execute_with(test);

		ext.execute_with(|| {
			if self.core_try_state {
				MultiPhase::do_try_state(System::block_number())
					.map_err(|err| println!(" 🕵️‍♂️  Core pallet `try_state` failure: {:?}", err))
					.unwrap();
			}

			if self.signed_try_state {
				SignedPallet::do_try_state(System::block_number())
					.map_err(|err| println!(" 🕵️‍♂️  Signed pallet `try_state` failure: {:?}", err))
					.unwrap();
			}

			if self.verifier_try_state {
				VerifierPallet::do_try_state(System::block_number())
					.map_err(|err| println!(" 🕵️‍♂️  Verifier pallet `try_state` failure: {:?}", err))
					.unwrap();
			}
		});
	}
}

pub(crate) fn force_phase(phase: Phase<BlockNumber>) {
	CurrentPhase::<T>::set(phase);
}

pub(crate) fn compute_snapshot_checked() {
	let msp = crate::Pallet::<T>::msp();

	crate::Pallet::<T>::try_start_snapshot();
	assert!(Snapshot::<T>::targets().is_some());

	for page in (1..=Pages::get()).rev() {
		force_phase(Phase::Snapshot(page - 1));
		crate::Pallet::<T>::try_progress_snapshot();

		if page <= msp {
			assert!(Snapshot::<T>::voters(page).is_some());
		}
	}

	Snapshot::<T>::ensure().unwrap();
}

pub(crate) fn mine_and_verify_all() -> Result<
	Vec<
		frame_election_provider_support::BoundedSupports<
			AccountId,
			MaxWinnersPerPage,
			MaxBackersPerWinner,
		>,
	>,
	&'static str,
> {
	let phase = current_phase();

	let msp = crate::Pallet::<T>::msp();
	let mut paged_supports = vec![];

	for page in (0..=msp).rev() {
		let (_, score, solution) =
			OffchainWorkerMiner::<T>::mine(page).map_err(|e| "error mining")?;

		set_phase_to(Phase::SignedValidation(0));
		<VerifierPallet as AsyncVerifier>::set_status(verifier::Status::Ongoing(0));

		let supports =
			<VerifierPallet as verifier::Verifier>::verify_synchronous(solution, score, page)
				.map_err(|_| "error verifying paged solution")?;

		paged_supports.push(supports);
	}

	set_phase_to(phase);
	Ok(paged_supports)
}

pub(crate) fn roll_to(n: BlockNumber) {
	for bn in (System::block_number()) + 1..=n {
		System::set_block_number(bn);

		MultiPhase::on_initialize(bn);
		VerifierPallet::on_initialize(bn);
		SignedPallet::on_initialize(bn);
		UnsignedPallet::on_initialize(bn);
		UnsignedPallet::offchain_worker(bn);

		log!(
			trace,
			"Block: {}, Phase: {:?}, Round: {:?}, Election at {:?}",
			bn,
			<CurrentPhase<T>>::get(),
			<Round<T>>::get(),
			election_prediction()
		);
	}
}

pub(crate) fn roll_one() {
	roll_to(System::block_number() + 1);
}

// Fast forward until a given election phase.
pub fn roll_to_phase(phase: Phase<BlockNumber>) {
	while MultiPhase::current_phase() != phase {
		roll_to(System::block_number() + 1);
	}
}

pub fn set_phase_to(phase: Phase<BlockNumber>) {
	CurrentPhase::<Runtime>::set(phase);
}

pub fn roll_to_export() {
	while !MultiPhase::current_phase().is_export() {
		roll_to(System::block_number() + 1);
	}
}

pub(crate) fn roll_to_snapshot() {
	while !MultiPhase::current_phase().is_snapshot() {
		roll_to(System::block_number() + 1);
	}
}

pub fn roll_one_with_ocw(maybe_pool: Option<Arc<RwLock<PoolState>>>) {
	use sp_runtime::traits::Dispatchable;
	let bn = System::block_number() + 1;
	// if there's anything in the submission pool, submit it.
	if let Some(ref pool) = maybe_pool {
		pool.read()
			.transactions
			.clone()
			.into_iter()
			.map(|uxt| <Extrinsic as codec::Decode>::decode(&mut &*uxt).unwrap())
			.for_each(|xt| {
				xt.function.dispatch(frame_system::RawOrigin::None.into()).unwrap();
			});
		pool.try_write().unwrap().transactions.clear();
	}

	roll_to(bn);
}

pub fn roll_to_phase_with_ocw(
	phase: Phase<BlockNumber>,
	maybe_pool: Option<Arc<RwLock<PoolState>>>,
) {
	while MultiPhase::current_phase() != phase {
		roll_one_with_ocw(maybe_pool.clone());
	}
}

pub fn roll_to_with_ocw(n: BlockNumber, maybe_pool: Option<Arc<RwLock<PoolState>>>) {
	let now = System::block_number();
	for _i in now + 1..=n {
		roll_one_with_ocw(maybe_pool.clone());
	}
}

pub fn election_prediction() -> BlockNumber {
	<<Runtime as Config>::DataProvider as ElectionDataProvider>::next_election_prediction(
		System::block_number(),
	)
}

pub fn calculate_phases() -> HashMap<&'static str, BlockNumber> {
	let mut map = HashMap::new();

	let next_election = election_prediction();
	let export_deadline = next_election - (ExportPhaseLimit::get() + Lookhaead::get());
	let expected_unsigned = export_deadline - UnsignedPhase::get();
	let expected_validate = expected_unsigned - SignedValidationPhase::get();
	let expected_signed = expected_validate - SignedPhase::get() + 1;
	let expected_snapshot = expected_signed - Pages::get() as u64 - 1;

	map.insert("export", export_deadline);
	map.insert("unsigned", expected_unsigned);
	map.insert("validate", expected_validate);
	map.insert("signed", expected_signed);
	map.insert("snapshot", expected_snapshot);

	map
}

pub fn current_phase() -> Phase<BlockNumber> {
	MultiPhase::current_phase()
}

pub fn current_round() -> u32 {
	Pallet::<T>::current_round()
}

pub fn call_elect() -> Result<(), crate::ElectionError<T>> {
	for p in (0..=Pallet::<T>::msp()).rev() {
		<MultiPhase as ElectionProvider>::elect(p)?;
	}
	Ok(())
}

pub fn assert_snapshots() -> Result<(), &'static str> {
	Snapshot::<T>::ensure()
}

pub fn clear_snapshot() {
	let _ = crate::PagedVoterSnapshot::<T>::clear(u32::MAX, None);
	let _ = crate::TargetSnapshot::<T>::kill();
}

/// Returns the free balance, and the total on-hold for the election submissions.
pub fn balances(who: AccountId) -> (Balance, Balance) {
	(
		Balances::free_balance(who),
		Balances::balance_on_hold(&HoldReason::ElectionSolutionSubmission.into(), &who),
	)
}

pub fn mine_full() -> Result<PagedRawSolution<T>, MinerError> {
	let (targets, voters) =
		OffchainWorkerMiner::<T>::fetch_snapshots().map_err(|_| MinerError::DataProvider)?;

	let reduce = false;
	let round = crate::Pallet::<T>::current_round();
	let desired_targets = <MockStaking as ElectionDataProvider>::desired_targets()
		.map_err(|_| MinerError::DataProvider)?;

	Miner::<Runtime>::mine_paged_solution_with_snapshot(
		&targets,
		&voters,
		Pages::get(),
		round,
		desired_targets,
		reduce,
	)
	.map(|(s, _)| s)
}

pub fn mine(
	page: PageIndex,
) -> Result<(ElectionScore, SolutionOf<<T as Config>::MinerConfig>), ()> {
	let (_, partial_score, partial_solution) =
		OffchainWorkerMiner::<T>::mine(page).map_err(|_| ())?;

	Ok((partial_score, partial_solution))
}

// Pallet events filters.
// TODO: may be simplified with a macro.
parameter_types! {
	static SignedEventsIdx: usize = 0;
	static UnsignedEventsIdx: usize = 0;
	static VerifierEventsIdx: usize = 0;
}

pub(crate) fn unsigned_events() -> Vec<crate::unsigned::Event<T>> {
	let all: Vec<_> = System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(
			|e| if let RuntimeEvent::UnsignedPallet(inner) = e { Some(inner) } else { None },
		)
		.collect();

	let seen = UnsignedEventsIdx::get();
	UnsignedEventsIdx::set(all.len());
	all.into_iter().skip(seen).collect()
}

pub(crate) fn signed_events() -> Vec<crate::signed::Event<T>> {
	let all: Vec<_> = System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(|e| if let RuntimeEvent::SignedPallet(inner) = e { Some(inner) } else { None })
		.collect();

	let seen = SignedEventsIdx::get();
	SignedEventsIdx::set(all.len());
	all.into_iter().skip(seen).collect()
}

pub(crate) fn verifier_events() -> Vec<crate::verifier::Event<T>> {
	let all: Vec<_> = System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(
			|e| if let RuntimeEvent::VerifierPallet(inner) = e { Some(inner) } else { None },
		)
		.collect();

	let seen = VerifierEventsIdx::get();
	VerifierEventsIdx::set(all.len());
	all.into_iter().skip(seen).collect()
}
