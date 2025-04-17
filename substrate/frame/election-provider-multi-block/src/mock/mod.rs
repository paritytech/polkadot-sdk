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

//! The overarching mock crate for all EPMB pallets.

mod signed;
mod staking;

use super::*;
use crate::{
	self as multi_block,
	signed::{self as signed_pallet, HoldReason},
	unsigned::{
		self as unsigned_pallet,
		miner::{MinerConfig, OffchainMinerError, OffchainWorkerMiner},
	},
	verifier::{self as verifier_pallet, AsynchronousVerifier, Status},
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_election_provider_support::{
	bounds::{ElectionBounds, ElectionBoundsBuilder},
	InstantElectionProvider, NposSolution, SequentialPhragmen,
};
pub use frame_support::{assert_noop, assert_ok};
use frame_support::{
	derive_impl, parameter_types,
	traits::{fungible::InspectHold, Hooks},
	weights::{constants, Weight},
};
use frame_system::EnsureRoot;
use parking_lot::RwLock;
pub use signed::*;
use sp_core::{
	offchain::{
		testing::{PoolState, TestOffchainExt, TestTransactionPoolExt},
		OffchainDbExt, OffchainWorkerExt, TransactionPoolExt,
	},
	ConstBool,
};
use sp_npos_elections::EvaluateSupport;
use sp_runtime::{
	bounded_vec,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage, PerU16, Perbill,
};
pub use staking::*;
use std::{sync::Arc, vec};

pub type Extrinsic = sp_runtime::testing::TestXt<RuntimeCall, ()>;

pub type Balance = u64;
pub type AccountId = u64;
pub type BlockNumber = u64;
pub type VoterIndex = u32;
pub type TargetIndex = u16;

frame_support::construct_runtime!(
	pub enum Runtime  {
		System: frame_system,
		Balances: pallet_balances,
		MultiBlock: multi_block,
		SignedPallet: signed_pallet,
		VerifierPallet: verifier_pallet,
		UnsignedPallet: unsigned_pallet,
	}
);

frame_election_provider_support::generate_solution_type!(
	pub struct TestNposSolution::<
		VoterIndex = VoterIndex,
		TargetIndex = TargetIndex,
		Accuracy = PerU16,
		MaxVoters = ConstU32::<2_000>
	>(16)
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type BlockLength = ();
	type BlockWeights = BlockWeights;
	type AccountData = pallet_balances::AccountData<Balance>;
	type Block = frame_system::mocking::MockBlock<Self>;
}

const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);
parameter_types! {
	pub const ExistentialDeposit: Balance = 1;
	pub BlockWeights: frame_system::limits::BlockWeights = frame_system::limits::BlockWeights
		::with_sensible_defaults(
			Weight::from_parts(2u64 * constants::WEIGHT_REF_TIME_PER_SECOND, u64::MAX),
			NORMAL_DISPATCH_RATIO,
		);
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
	type Balance = Balance;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type MaxLocks = ();
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type WeightInfo = ();
}

#[allow(unused)]
#[derive(Clone, Debug)]
pub enum FallbackModes {
	Continue,
	Emergency,
	Onchain,
}

#[derive(Clone, Debug)]
pub enum AreWeDoneModes {
	Proceed,
	BackToSigned,
}

parameter_types! {
	// The block at which we emit the start signal. This is used in `roll_next`, which is used all
	// across tests. The number comes across as a bit weird, but this is mainly due to backwards
	// compatibility with olds tests, when we used to have pull based election prediction.
	pub static ElectionStart: BlockNumber = 11;


	pub static Pages: PageIndex = 3;
	pub static UnsignedPhase: BlockNumber = 5;
	pub static SignedPhase: BlockNumber = 5;
	pub static SignedValidationPhase: BlockNumber = 5;

	pub static FallbackMode: FallbackModes = FallbackModes::Emergency;
	pub static MinerTxPriority: u64 = 100;
	pub static SolutionImprovementThreshold: Perbill = Perbill::zero();
	pub static OffchainRepeat: BlockNumber = 5;
	pub static MinerMaxLength: u32 = 256;
	pub static MinerPages: u32 = 1;
	pub static MaxVotesPerVoter: u32 = <TestNposSolution as NposSolution>::LIMIT as u32;

	// by default we stick to 3 pages to host our 12 voters.
	pub static VoterSnapshotPerBlock: VoterIndex = 4;
	// and 4 targets, whom we fetch all.
	pub static TargetSnapshotPerBlock: TargetIndex = 4;

	// we have 12 voters in the default setting, this should be enough to make sure they are not
	// trimmed accidentally in any test.
	#[derive(Encode, Decode, PartialEq, Eq, Debug, scale_info::TypeInfo, MaxEncodedLen)]
	pub static MaxBackersPerWinner: u32 = 12;
	pub static MaxBackersPerWinnerFinal: u32 = 12;
	// we have 4 targets in total and we desire `Desired` thereof, no single page can represent more
	// than the min of these two.
	#[derive(Encode, Decode, PartialEq, Eq, Debug, scale_info::TypeInfo, MaxEncodedLen)]
	pub static MaxWinnersPerPage: u32 = (staking::Targets::get().len() as u32).min(staking::DesiredTargets::get());
	pub static AreWeDone: AreWeDoneModes = AreWeDoneModes::Proceed;
}

impl Get<Phase<Runtime>> for AreWeDone {
	fn get() -> Phase<Runtime> {
		match <Self as Get<AreWeDoneModes>>::get() {
			AreWeDoneModes::BackToSigned => RevertToSignedIfNotQueuedOf::<Runtime>::get(),
			AreWeDoneModes::Proceed => ProceedRegardlessOf::<Runtime>::get(),
		}
	}
}

impl crate::verifier::Config for Runtime {
	type SolutionImprovementThreshold = SolutionImprovementThreshold;
	type MaxBackersPerWinnerFinal = MaxBackersPerWinnerFinal;
	type MaxBackersPerWinner = MaxBackersPerWinner;
	type MaxWinnersPerPage = MaxWinnersPerPage;
	type SolutionDataProvider = signed::DualSignedPhase;
	type WeightInfo = ();
}

impl crate::unsigned::Config for Runtime {
	type MinerPages = MinerPages;
	type OffchainRepeat = OffchainRepeat;
	type MinerTxPriority = MinerTxPriority;
	type OffchainSolver = SequentialPhragmen<Self::AccountId, Perbill>;
	type WeightInfo = ();
}

impl MinerConfig for Runtime {
	type AccountId = AccountId;
	type Hash = <Runtime as frame_system::Config>::Hash;
	type MaxLength = MinerMaxLength;
	type Pages = Pages;
	type MaxVotesPerVoter = MaxVotesPerVoter;
	type Solution = TestNposSolution;
	type Solver = SequentialPhragmen<AccountId, Perbill>;
	type TargetSnapshotPerBlock = TargetSnapshotPerBlock;
	type VoterSnapshotPerBlock = VoterSnapshotPerBlock;
	type MaxBackersPerWinner = MaxBackersPerWinner;
	type MaxBackersPerWinnerFinal = MaxBackersPerWinnerFinal;
	type MaxWinnersPerPage = MaxWinnersPerPage;
}

impl crate::Config for Runtime {
	type SignedPhase = SignedPhase;
	type SignedValidationPhase = SignedValidationPhase;
	type UnsignedPhase = UnsignedPhase;
	type DataProvider = staking::MockStaking;
	type Fallback = MockFallback;
	type TargetSnapshotPerBlock = TargetSnapshotPerBlock;
	type VoterSnapshotPerBlock = VoterSnapshotPerBlock;
	type MinerConfig = Self;
	type WeightInfo = ();
	type Verifier = VerifierPallet;
	type AdminOrigin = EnsureRoot<AccountId>;
	type Pages = Pages;
	type AreWeDone = AreWeDone;
}

parameter_types! {
	pub static OnChainElectionBounds: ElectionBounds = ElectionBoundsBuilder::default().build();
}

impl onchain::Config for Runtime {
	type DataProvider = staking::MockStaking;
	type MaxBackersPerWinner = MaxBackersPerWinner;
	type MaxWinnersPerPage = MaxWinnersPerPage;
	type Sort = ConstBool<true>;
	type Solver = SequentialPhragmen<AccountId, sp_runtime::PerU16, ()>;
	type System = Runtime;
	type WeightInfo = ();
	type Bounds = OnChainElectionBounds;
}

pub struct MockFallback;
impl ElectionProvider for MockFallback {
	type AccountId = AccountId;
	type BlockNumber = u64;
	type Error = String;
	type DataProvider = staking::MockStaking;
	type Pages = ConstU32<1>;
	type MaxBackersPerWinner = MaxBackersPerWinner;
	type MaxWinnersPerPage = MaxWinnersPerPage;

	fn elect(_remaining: PageIndex) -> Result<BoundedSupportsOf<Self>, Self::Error> {
		unreachable!()
	}

	fn duration() -> Self::BlockNumber {
		0
	}

	fn start() -> Result<(), Self::Error> {
		Ok(())
	}

	fn status() -> Result<bool, ()> {
		Ok(true)
	}
}

impl InstantElectionProvider for MockFallback {
	fn instant_elect(
		voters: Vec<VoterOf<Runtime>>,
		targets: Vec<Self::AccountId>,
		desired_targets: u32,
	) -> Result<BoundedSupportsOf<Self>, Self::Error> {
		match FallbackMode::get() {
			FallbackModes::Continue =>
				crate::Continue::<Runtime>::instant_elect(voters, targets, desired_targets)
					.map_err(|x| x.to_string()),
			FallbackModes::Emergency => crate::InitiateEmergencyPhase::<Runtime>::instant_elect(
				voters,
				targets,
				desired_targets,
			)
			.map_err(|x| x.to_string()),
			FallbackModes::Onchain => onchain::OnChainExecution::<Runtime>::instant_elect(
				voters,
				targets,
				desired_targets,
			)
			.map_err(|e| format!("onchain fallback failed: {:?}", e)),
		}
	}
	fn bother() -> bool {
		matches!(FallbackMode::get(), FallbackModes::Onchain)
	}
}

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

pub struct ExtBuilder {}

impl ExtBuilder {
	pub fn full() -> Self {
		Self {}
	}

	pub fn verifier() -> Self {
		SignedPhase::set(0);
		SignedValidationPhase::set(0);
		signed::SignedPhaseSwitch::set(signed::SignedSwitch::Mock);
		Self {}
	}

	pub fn unsigned() -> Self {
		SignedPhase::set(0);
		SignedValidationPhase::set(0);
		signed::SignedPhaseSwitch::set(signed::SignedSwitch::Mock);
		Self {}
	}

	pub fn signed() -> Self {
		UnsignedPhase::set(0);
		Self {}
	}
}

impl ExtBuilder {
	pub(crate) fn max_backers_per_winner(self, c: u32) -> Self {
		MaxBackersPerWinner::set(c);
		self
	}
	pub(crate) fn max_backers_per_winner_final(self, c: u32) -> Self {
		MaxBackersPerWinnerFinal::set(c);
		self
	}
	pub(crate) fn miner_tx_priority(self, p: u64) -> Self {
		MinerTxPriority::set(p);
		self
	}
	pub(crate) fn solution_improvement_threshold(self, p: Perbill) -> Self {
		SolutionImprovementThreshold::set(p);
		self
	}
	pub(crate) fn election_start(self, at: BlockNumber) -> Self {
		ElectionStart::set(at);
		self
	}
	pub(crate) fn pages(self, pages: PageIndex) -> Self {
		Pages::set(pages);
		self
	}
	pub(crate) fn voter_per_page(self, count: u32) -> Self {
		VoterSnapshotPerBlock::set(count);
		self
	}
	pub(crate) fn miner_max_length(self, len: u32) -> Self {
		MinerMaxLength::set(len);
		self
	}
	pub(crate) fn desired_targets(self, t: u32) -> Self {
		staking::DesiredTargets::set(t);
		self
	}
	pub(crate) fn signed_phase(self, d: BlockNumber, v: BlockNumber) -> Self {
		SignedPhase::set(d);
		SignedValidationPhase::set(v);
		self
	}
	pub(crate) fn unsigned_phase(self, d: BlockNumber) -> Self {
		UnsignedPhase::set(d);
		self
	}
	pub(crate) fn signed_validation_phase(self, d: BlockNumber) -> Self {
		SignedValidationPhase::set(d);
		self
	}
	pub(crate) fn miner_pages(self, p: u32) -> Self {
		MinerPages::set(p);
		self
	}
	#[allow(unused)]
	pub(crate) fn add_voter(self, who: AccountId, stake: Balance, targets: Vec<AccountId>) -> Self {
		staking::VOTERS.with(|v| v.borrow_mut().push((who, stake, targets.try_into().unwrap())));
		self
	}
	pub(crate) fn fallback_mode(self, mode: FallbackModes) -> Self {
		FallbackMode::set(mode);
		self
	}
	pub(crate) fn are_we_done(self, mode: AreWeDoneModes) -> Self {
		AreWeDone::set(mode);
		self
	}
	pub(crate) fn build_unchecked(self) -> sp_io::TestExternalities {
		sp_tracing::try_init_simple();
		let mut storage =
			frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

		let _ = pallet_balances::GenesisConfig::<Runtime> {
			balances: vec![
				// bunch of account for submitting stuff only.
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
			..Default::default()
		}
		.assimilate_storage(&mut storage);

		sp_io::TestExternalities::from(storage)
	}

	/// Warning: this does not execute the post-sanity-checks.
	pub(crate) fn build_offchainify(self) -> (sp_io::TestExternalities, Arc<RwLock<PoolState>>) {
		let mut ext = self.build_unchecked();
		let (offchain, _offchain_state) = TestOffchainExt::new();
		let (pool, pool_state) = TestTransactionPoolExt::new();

		ext.register_extension(OffchainDbExt::new(offchain.clone()));
		ext.register_extension(OffchainWorkerExt::new(offchain));
		ext.register_extension(TransactionPoolExt::new(pool));

		(ext, pool_state)
	}

	/// Build the externalities, and execute the given  s`test` closure with it.
	pub(crate) fn build_and_execute(self, test: impl FnOnce() -> ()) {
		let mut ext = self.build_unchecked();
		ext.execute_with_sanity_checks(test);
	}
}

pub trait ExecuteWithSanityChecks {
	fn execute_with_sanity_checks(&mut self, test: impl FnOnce() -> ());
}

impl ExecuteWithSanityChecks for sp_io::TestExternalities {
	fn execute_with_sanity_checks(&mut self, test: impl FnOnce() -> ()) {
		self.execute_with(test);
		self.execute_with(all_pallets_sanity_checks)
	}
}

fn all_pallets_sanity_checks() {
	let now = System::block_number();
	let _ = VerifierPallet::do_try_state(now).unwrap();
	let _ = UnsignedPallet::do_try_state(now).unwrap();
	let _ = MultiBlock::do_try_state(now).unwrap();
	let _ = SignedPallet::do_try_state(now).unwrap();
}

/// Fully verify a solution.
///
/// This will progress the blocks until the verifier pallet is done verifying it.
///
/// The solution must have already been loaded via `load_and_start_verification`.
///
/// Return the final supports, which is the outcome. If this succeeds, then the valid variant of the
/// `QueuedSolution` form `verifier` is ready to be read.
pub fn roll_to_full_verification() -> Vec<BoundedSupportsOf<MultiBlock>> {
	// we must be ready to verify.
	assert_eq!(VerifierPallet::status(), Status::Ongoing(Pages::get() - 1));

	while matches!(VerifierPallet::status(), Status::Ongoing(_)) {
		roll_to(System::block_number() + 1);
	}

	(MultiBlock::lsp()..=MultiBlock::msp())
		.map(|p| VerifierPallet::get_queued_solution_page(p).unwrap_or_default())
		.collect::<Vec<_>>()
}

/// Generate a single page of `TestNposSolution` from the give supports.
///
/// All of the voters in this support must live in a single page of the snapshot, noted by
/// `snapshot_page`.
pub fn solution_from_supports(
	supports: sp_npos_elections::Supports<AccountId>,
	snapshot_page: PageIndex,
) -> TestNposSolution {
	let staked = sp_npos_elections::supports_to_staked_assignment(supports);
	let assignments = sp_npos_elections::assignment_staked_to_ratio_normalized(staked).unwrap();

	let voters = crate::Snapshot::<Runtime>::voters(snapshot_page).unwrap();
	let targets = crate::Snapshot::<Runtime>::targets().unwrap();
	let voter_index = helpers::voter_index_fn_linear::<Runtime>(&voters);
	let target_index = helpers::target_index_fn_linear::<Runtime>(&targets);

	TestNposSolution::from_assignment(&assignments, &voter_index, &target_index).unwrap()
}

/// Generate a raw paged solution from the given vector of supports.
///
/// Given vector must be aligned with the snapshot, at most need to be 'pagified' which we do
/// internally.
pub fn raw_paged_from_supports(
	paged_supports: Vec<sp_npos_elections::Supports<AccountId>>,
	round: u32,
) -> PagedRawSolution<Runtime> {
	let score = {
		let flattened = paged_supports.iter().cloned().flatten().collect::<Vec<_>>();
		flattened.evaluate()
	};

	let solution_pages = paged_supports
		.pagify(Pages::get())
		.map(|(page_index, page_support)| solution_from_supports(page_support.to_vec(), page_index))
		.collect::<Vec<_>>();

	let solution_pages = solution_pages.try_into().unwrap();
	PagedRawSolution { solution_pages, score, round }
}

/// ensure that the snapshot fully exists.
///
/// NOTE: this should not be used that often, because we check snapshot in sanity checks, which are
/// called ALL THE TIME.
pub fn assert_full_snapshot() {
	assert_ok!(Snapshot::<Runtime>::ensure_snapshot(true, Pages::get()));
}

/// ensure that the no snapshot exists.
///
/// NOTE: this should not be used that often, because we check snapshot in sanity checks, which are
/// called ALL THE TIME.
pub fn assert_none_snapshot() {
	assert_ok!(Snapshot::<Runtime>::ensure_snapshot(false, Pages::get()));
}

/// Simple wrapper for mining a new solution. Just more handy in case the interface of mine solution
/// changes.
///
/// For testing, we never want to do reduce.
pub fn mine_full_solution() -> Result<PagedRawSolution<Runtime>, OffchainMinerError<Runtime>> {
	OffchainWorkerMiner::<Runtime>::mine_solution(Pages::get(), false)
}

/// Same as [`mine_full_solution`] but with custom pages.
pub fn mine_solution(
	pages: PageIndex,
) -> Result<PagedRawSolution<Runtime>, OffchainMinerError<Runtime>> {
	OffchainWorkerMiner::<Runtime>::mine_solution(pages, false)
}

/// Assert that `count` voters exist across `pages` number of pages.
pub fn ensure_voters(pages: PageIndex, count: usize) {
	assert_eq!(crate::Snapshot::<Runtime>::voter_pages(), pages);
	assert_eq!(crate::Snapshot::<Runtime>::voters_iter_flattened().count(), count);
}

/// Assert that `count` targets exist across `pages` number of pages.
pub fn ensure_targets(pages: PageIndex, count: usize) {
	assert_eq!(crate::Snapshot::<Runtime>::target_pages(), pages);
	assert_eq!(crate::Snapshot::<Runtime>::targets().unwrap().len(), count);
}

/// get the events of the multi-block pallet.
pub fn multi_block_events() -> Vec<crate::Event<Runtime>> {
	System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(|e| if let RuntimeEvent::MultiBlock(inner) = e { Some(inner) } else { None })
		.collect::<Vec<_>>()
}

parameter_types! {
	static MultiBlockEvents: u32 = 0;
}

pub fn multi_block_events_since_last_call() -> Vec<crate::Event<Runtime>> {
	let events = multi_block_events();
	let already_seen = MultiBlockEvents::get();
	MultiBlockEvents::set(events.len() as u32);
	events.into_iter().skip(already_seen as usize).collect()
}

/// get the events of the verifier pallet.
pub fn verifier_events() -> Vec<crate::verifier::Event<Runtime>> {
	System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(
			|e| if let RuntimeEvent::VerifierPallet(inner) = e { Some(inner) } else { None },
		)
		.collect::<Vec<_>>()
}

/// proceed block number to `n`.
pub fn roll_to(n: BlockNumber) {
	crate::Pallet::<Runtime>::roll_to(
		n,
		matches!(SignedPhaseSwitch::get(), SignedSwitch::Real),
		true,
	);
}

/// proceed block number to whenever the snapshot is fully created (`Phase::Snapshot(0)`).
pub fn roll_to_snapshot_created() {
	while !matches!(MultiBlock::current_phase(), Phase::Snapshot(0)) {
		roll_next()
	}
	roll_next();
	assert_full_snapshot();
}

/// proceed block number to whenever the unsigned phase is open (`Phase::Unsigned(_)`).
pub fn roll_to_unsigned_open() {
	while !matches!(MultiBlock::current_phase(), Phase::Unsigned(_)) {
		roll_next()
	}
}

/// proceed block number to whenever the unsigned phase is about to close (`Phase::Unsigned(_)`).
pub fn roll_to_last_unsigned() {
	while !matches!(MultiBlock::current_phase(), Phase::Unsigned(0)) {
		roll_next()
	}
}

/// proceed block number to whenever the signed phase is open (`Phase::Signed(_)`).
pub fn roll_to_signed_open() {
	while !matches!(MultiBlock::current_phase(), Phase::Signed(_)) {
		roll_next();
	}
}

/// proceed block number to whenever the signed validation phase is open
/// (`Phase::SignedValidation(_)`).
pub fn roll_to_signed_validation_open() {
	while !matches!(MultiBlock::current_phase(), Phase::SignedValidation(_)) {
		roll_next()
	}
}

/// Proceed one block.
pub fn roll_next() {
	let now = System::block_number();
	roll_to(now + 1);
}

/// Proceed one block, and execute offchain workers as well.
pub fn roll_next_with_ocw(maybe_pool: Option<Arc<RwLock<PoolState>>>) {
	roll_to_with_ocw(System::block_number() + 1, maybe_pool)
}

pub fn roll_to_unsigned_open_with_ocw(maybe_pool: Option<Arc<RwLock<PoolState>>>) {
	while !matches!(MultiBlock::current_phase(), Phase::Unsigned(_)) {
		roll_next_with_ocw(maybe_pool.clone());
	}
}

/// proceed block number to `n`, while running all offchain workers as well.
pub fn roll_to_with_ocw(n: BlockNumber, maybe_pool: Option<Arc<RwLock<PoolState>>>) {
	use sp_runtime::traits::Dispatchable;
	let now = System::block_number();
	for i in now + 1..=n {
		// check the offchain transaction pool, and if anything's there, submit it.
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

		System::set_block_number(i);

		MultiBlock::on_initialize(i);
		VerifierPallet::on_initialize(i);
		UnsignedPallet::on_initialize(i);
		if matches!(SignedPhaseSwitch::get(), SignedSwitch::Real) {
			SignedPallet::on_initialize(i);
		}

		MultiBlock::offchain_worker(i);
		VerifierPallet::offchain_worker(i);
		UnsignedPallet::offchain_worker(i);
		if matches!(SignedPhaseSwitch::get(), SignedSwitch::Real) {
			SignedPallet::offchain_worker(i);
		}

		// invariants must hold at the end of each block.
		all_pallets_sanity_checks()
	}
}

/// An invalid solution with any score.
pub fn fake_solution(score: ElectionScore) -> PagedRawSolution<Runtime> {
	PagedRawSolution {
		score,
		solution_pages: bounded_vec![Default::default()],
		..Default::default()
	}
}

/// A real solution that's valid, but has a really bad score.
///
/// This is different from `solution_from_supports` in that it does not require the snapshot to
/// exist.
pub fn raw_paged_solution_low_score() -> PagedRawSolution<Runtime> {
	PagedRawSolution {
		solution_pages: vec![TestNposSolution {
			// 2 targets, both voting for themselves
			votes1: vec![(0, 0), (1, 2)],
			..Default::default()
		}]
		.try_into()
		.unwrap(),
		round: 0,
		score: ElectionScore { minimal_stake: 10, sum_stake: 20, sum_stake_squared: 200 },
	}
}

/// Get the free and held balance of `who`.
pub fn balances(who: AccountId) -> (Balance, Balance) {
	(
		Balances::free_balance(who),
		Balances::balance_on_hold(&HoldReason::SignedSubmission.into(), &who),
	)
}

/// Election bounds based on just the given count.
pub fn bound_by_count(count: Option<u32>) -> DataProviderBounds {
	DataProviderBounds { count: count.map(|x| x.into()), size: None }
}

pub fn emergency_solution() -> (BoundedSupportsOf<MultiBlock>, ElectionScore) {
	let supports = onchain::OnChainExecution::<Runtime>::elect(0).unwrap();
	let score = supports.evaluate();
	(supports, score)
}
