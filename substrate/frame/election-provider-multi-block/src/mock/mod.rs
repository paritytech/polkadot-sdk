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

mod staking;

use frame_election_provider_support::{bounds::ElectionBounds, onchain, SequentialPhragmen};
pub use staking::*;

use crate::{
	self as epm,
	signed::{self as signed_pallet},
	unsigned::{self as unsigned_pallet},
	verifier::{self as verifier_pallet},
	Config, *,
};
use frame_support::{derive_impl, pallet_prelude::*, parameter_types};
use parking_lot::RwLock;
use sp_runtime::{
	offchain::{
		testing::{PoolState, TestOffchainExt, TestTransactionPoolExt},
		OffchainDbExt, OffchainWorkerExt, TransactionPoolExt,
	},
	BuildStorage, Perbill,
};
use std::sync::Arc;

frame_support::construct_runtime!(
	pub struct Runtime {
		System: frame_system,
		Balances: pallet_balances,
		MultiPhase: epm,
		VerifierPallet: verifier_pallet,
		SignedPallet: signed_pallet,
		UnsignedPallet: unsigned_pallet,
	}
);

pub type AccountId = u64;
pub type Balance = u128;
pub type BlockNumber = u64;
pub type VoterIndex = u32;
pub type TargetIndex = u16;
pub type T = Runtime;
pub type Block = frame_system::mocking::MockBlock<Runtime>;

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
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = ();
}

parameter_types! {
	pub static SignedPhase: BlockNumber = 3;
	pub static UnsignedPhase: BlockNumber = 5;
	pub static SignedValidationPhase: BlockNumber = Pages::get().into();
	pub static Lookhaead: BlockNumber = 0;
	pub static VoterSnapshotPerBlock: VoterIndex = 4;
	pub static TargetSnapshotPerBlock: TargetIndex = 8;
	pub static Pages: PageIndex = 3;
	pub static ExportPhaseLimit: BlockNumber = (Pages::get() * 2u32).into();
}

impl Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SignedPhase = SignedPhase;
	type UnsignedPhase = UnsignedPhase;
	type SignedValidationPhase = SignedValidationPhase;
	type Lookhaead = Lookhaead;
	type VoterSnapshotPerBlock = VoterSnapshotPerBlock;
	type TargetSnapshotPerBlock = TargetSnapshotPerBlock;
	type Pages = Pages;
	type ExportPhaseLimit = ExportPhaseLimit;
	type DataProvider = MockStaking;
	type Solution = TestNposSolution;
	type Fallback = MockFallback;
	type Verifier = VerifierPallet;
}

parameter_types! {
	pub static SolutionImprovementThreshold: Perbill = Perbill::zero();
	pub static MaxWinnersPerPage: u32 = 3;
	pub static MaxBackersPerWinner: u32 = (staking::Targets::get().len() as u32).min(staking::DesiredTargets::get());
}

impl crate::verifier::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ForceOrigin = frame_system::EnsureRoot<AccountId>;
	type SolutionImprovementThreshold = SolutionImprovementThreshold;
	type MaxBackersPerWinner = MaxBackersPerWinner;
	type MaxWinnersPerPage = MaxWinnersPerPage;
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
	type OffchainSolver = SequentialPhragmen<AccountId, sp_runtime::PerU16>;
	type OffchainRepeatInterval = OffchainRepeatInterval;
	type MinerTxPriority = MinerTxPriority;
	type MaxLength = MinerSolutionMaxLength;
	type MaxWeight = MinerSolutionMaxWeight;
	type WeightInfo = ();
}

pub type Extrinsic = sp_runtime::testing::TestXt<RuntimeCall, ()>;

impl<LocalCall> frame_system::offchain::SendTransactionTypes<LocalCall> for Runtime
where
	RuntimeCall: From<LocalCall>,
{
	type OverarchingCall = RuntimeCall;
	type Extrinsic = Extrinsic;
}

pub struct ConstDepositBase;
impl sp_runtime::traits::Convert<usize, Balance> for ConstDepositBase {
	fn convert(_a: usize) -> Balance {
		DepositBase::get()
	}
}

parameter_types! {
	pub static OnChainElectionBounds: ElectionBounds = ElectionBoundsBuilder::default().build();
	pub static MaxVotesPerVoter: u32 = <TestNposSolution as NposSolution>::LIMIT as u32;
	pub static FallbackEnabled: bool = true;
}

impl onchain::Config for Runtime {
	type System = Runtime;
	type Solver = SequentialPhragmen<AccountId, sp_runtime::PerU16, ()>;
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
}

#[derive(Default)]
pub struct ExtBuilder {
	with_verifier: bool,
}

impl ExtBuilder {
	pub(crate) fn pages(self, pages: u32) -> Self {
		Pages::set(pages);
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

	pub(crate) fn verifier() -> Self {
		ExtBuilder { with_verifier: true }
	}

	pub(crate) fn build(self) -> sp_io::TestExternalities {
		sp_tracing::try_init_simple();

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

		if self.with_verifier {
			// nothing special for now
		}

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
		ext.execute_with(test);

		#[cfg(feature = "try-runtime")]
		ext.execute_with(|| {
			//MultiPhase::do_try_state().unwrap();
			// etc..

			let _ = VerifierPallet::do_try_state()
				.map_err(|err| println!(" 🕵️‍♂️  Verifier `try_state` failure: {:?}", err));
		});
	}
}

pub(crate) fn roll_to(n: BlockNumber) {
	for bn in (System::block_number()) + 1..=n {
		System::set_block_number(bn);

		MultiPhase::on_initialize(bn);
		VerifierPallet::on_initialize(bn);
		SignedPallet::on_initialize(bn);
		UnsignedPallet::on_initialize(bn);
		UnsignedPallet::offchain_worker(bn);

		// TODO: add try-checks for all pallets here too, as we progress the blocks.
		log!(
			info,
			"Block: {}, Phase: {:?}, Round: {:?}, Election at {:?}",
			bn,
			<CurrentPhase<T>>::get(),
			<Round<T>>::get(),
			election_prediction()
		);
	}
}

// Fast forward until a given election phase.
pub fn roll_to_phase(phase: Phase<BlockNumber>) {
	while MultiPhase::current_phase() != phase {
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
				xt.call.dispatch(frame_system::RawOrigin::None.into()).unwrap();
			});
		pool.try_write().unwrap().transactions.clear();
	}

	roll_to(bn);
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

pub fn current_phase() -> Phase<BlockNumber> {
	MultiPhase::current_phase()
}

pub(crate) fn unsigned_events() -> Vec<crate::unsigned::Event<T>> {
	System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(
			|e| if let RuntimeEvent::UnsignedPallet(inner) = e { Some(inner) } else { None },
		)
		.collect()
}
