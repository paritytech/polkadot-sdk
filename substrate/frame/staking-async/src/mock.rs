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

//! Test utilities

use crate::{
	self as pallet_staking_async,
	session_rotation::{Eras, Rotator},
	*,
};

use frame_election_provider_support::{
	bounds::{ElectionBounds, ElectionBoundsBuilder},
	onchain, BoundedSupports, BoundedSupportsOf, ElectionProvider, PageIndex, SequentialPhragmen,
	Support, VoteWeight,
};
use frame_support::{
	assert_ok, derive_impl, ord_parameter_types, parameter_types,
	traits::{EitherOfDiverse, Get, Imbalance, OnUnbalanced},
	weights::constants::RocksDbWeight,
};
use frame_system::{pallet_prelude::BlockNumberFor, EnsureRoot, EnsureSignedBy};
use pallet_staking_async_rc_client as rc_client;
use sp_core::ConstBool;
use sp_io;
use sp_npos_elections::BalancingConfig;
use sp_runtime::{traits::Zero, BuildStorage};
use sp_staking::{
	currency_to_vote::SaturatingCurrencyToVote, OnStakingUpdate, SessionIndex, StakingAccount,
};

pub(crate) const INIT_TIMESTAMP: u64 = 30_000;
pub(crate) const BLOCK_TIME: u64 = 1000;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		Staking: pallet_staking_async,
		VoterBagsList: pallet_bags_list::<Instance1>,
	}
);

pub(crate) type T = Test;
pub(crate) type Runtime = Test;
pub(crate) type AccountId = <Runtime as frame_system::Config>::AccountId;
pub(crate) type BlockNumber = BlockNumberFor<Runtime>;
pub(crate) type Balance = <Runtime as pallet_balances::Config>::Balance;

parameter_types! {
	pub static ExistentialDeposit: Balance = 1;
	pub static SlashDeferDuration: EraIndex = 0;
	pub static MaxControllersInDeprecationBatch: u32 = 5900;
	pub static BondingDuration: EraIndex = 3;
	pub static HistoryDepth: u32 = 80;
	pub static MaxExposurePageSize: u32 = 64;
	pub static MaxUnlockingChunks: u32 = 32;
	pub static RewardOnUnbalanceWasCalled: bool = false;
	pub static MaxValidatorSet: u32 = 100;
	pub static ElectionsBounds: ElectionBounds = ElectionBoundsBuilder::default().build();
	pub static AbsoluteMaxNominations: u32 = 16;
	pub static PlanningEraOffset: u32 = 1;
	// Session configs
	pub static SessionsPerEra: SessionIndex = 3;
	pub static Period: BlockNumber = 5;
	pub static Offset: BlockNumber = 0;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type DbWeight = RocksDbWeight;
	type Block = frame_system::mocking::MockBlock<Test>;
	type AccountData = pallet_balances::AccountData<Balance>;
}
#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type MaxLocks = frame_support::traits::ConstU32<1024>;
	type Balance = u128;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
}

parameter_types! {
	pub static RewardRemainderUnbalanced: u128 = 0;
}
pub struct RewardRemainderMock;
impl OnUnbalanced<NegativeImbalanceOf<Test>> for RewardRemainderMock {
	fn on_nonzero_unbalanced(amount: NegativeImbalanceOf<Test>) {
		RewardRemainderUnbalanced::mutate(|v| {
			*v += amount.peek();
		});
		drop(amount);
	}
}

pub(crate) const THRESHOLDS: [sp_npos_elections::VoteWeight; 9] =
	[10, 20, 30, 40, 50, 60, 1_000, 2_000, 10_000];

parameter_types! {
	pub static BagThresholds: &'static [sp_npos_elections::VoteWeight] = &THRESHOLDS;
}

pub type VoterBagsListInstance = pallet_bags_list::Instance1;
impl pallet_bags_list::Config<VoterBagsListInstance> for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	// Staking is the source of truth for voter bags list, since they are not kept up to date.
	type ScoreProvider = Staking;
	type BagThresholds = BagThresholds;
	type Score = VoteWeight;
}

// multi-page types and controller.
parameter_types! {
	pub static Pages: PageIndex = 1;
	pub static MaxBackersPerWinner: u32 = 256;
	pub static MaxWinnersPerPage: u32 = MaxValidatorSet::get();
	pub static StartReceived: bool = false;
}

pub type InnerElection = onchain::OnChainExecution<OnChainSeqPhragmen>;
pub struct Balancing;
impl Get<Option<BalancingConfig>> for Balancing {
	fn get() -> Option<BalancingConfig> {
		Some(BalancingConfig { iterations: 5, tolerance: 0 })
	}
}

pub struct OnChainSeqPhragmen;
impl onchain::Config for OnChainSeqPhragmen {
	type System = Test;
	type Solver = SequentialPhragmen<AccountId, Perbill, Balancing>;
	type DataProvider = Staking;
	type WeightInfo = ();
	type Bounds = ElectionsBounds;
	type Sort = ConstBool<true>;
	type MaxBackersPerWinner = MaxBackersPerWinner;
	type MaxWinnersPerPage = MaxWinnersPerPage;
}

pub struct TestElectionProvider;
impl ElectionProvider for TestElectionProvider {
	type AccountId = AccountId;
	type BlockNumber = BlockNumber;
	type MaxWinnersPerPage = MaxWinnersPerPage;
	type MaxBackersPerWinner = MaxBackersPerWinner;
	type Pages = Pages;
	type DataProvider = Staking;
	type Error = onchain::Error;

	fn elect(page: PageIndex) -> Result<BoundedSupportsOf<Self>, Self::Error> {
		if page == 0 {
			StartReceived::set(false);
		}
		InnerElection::elect(page)
	}
	fn start() -> Result<(), Self::Error> {
		StartReceived::set(true);
		Ok(())
	}
	fn duration() -> Self::BlockNumber {
		InnerElection::duration()
	}
	fn status() -> Result<bool, ()> {
		if StartReceived::get() {
			Ok(true)
		} else {
			Err(())
		}
	}
}
pub struct MockReward {}
impl OnUnbalanced<PositiveImbalanceOf<Test>> for MockReward {
	fn on_unbalanced(_: PositiveImbalanceOf<Test>) {
		RewardOnUnbalanceWasCalled::set(true);
	}
}

parameter_types! {
	pub static LedgerSlashPerEra:
		(BalanceOf<Test>, BTreeMap<EraIndex, BalanceOf<Test>>) =
		(Zero::zero(), BTreeMap::new());
	pub static SlashObserver: BTreeMap<AccountId, BalanceOf<Test>> = BTreeMap::new();
	pub static RestrictedAccounts: Vec<AccountId> = Vec::new();
}

pub struct EventListenerMock;
impl OnStakingUpdate<AccountId, Balance> for EventListenerMock {
	fn on_slash(
		pool_account: &AccountId,
		slashed_bonded: Balance,
		slashed_chunks: &BTreeMap<EraIndex, Balance>,
		total_slashed: Balance,
	) {
		LedgerSlashPerEra::set((slashed_bonded, slashed_chunks.clone()));
		SlashObserver::mutate(|map| {
			map.insert(*pool_account, map.get(pool_account).unwrap_or(&0) + total_slashed)
		});
	}
}

pub struct MockedRestrictList;
impl Contains<AccountId> for MockedRestrictList {
	fn contains(who: &AccountId) -> bool {
		RestrictedAccounts::get().contains(who)
	}
}

/// A representation of the session pallet that lives on the relay chain.
pub mod session_mock {
	use super::*;
	use pallet_staking_async_rc_client::ValidatorSetReport;

	pub struct Session;

	impl Session {
		pub fn queued_validators() -> Option<Vec<AccountId>> {
			Queued::get()
		}

		pub fn validators() -> Vec<AccountId> {
			Active::get()
		}

		pub fn current_index() -> SessionIndex {
			CurrentIndex::get()
		}

		pub fn roll_until(block: BlockNumber) {
			while System::block_number() < block {
				Self::roll_next();
			}
		}

		pub fn roll_next() {
			let now = System::block_number();
			Timestamp::mutate(|ts| *ts += BLOCK_TIME);
			System::run_to_block::<AllPalletsWithSystem>(now + 1);
			Self::maybe_rotate_session_now();
		}

		pub fn roll_to_next_session() {
			let current = Self::current_index();
			while Self::current_index() != (current + 1) {
				Self::roll_next();
			}
		}

		pub fn roll_until_session(session: SessionIndex) {
			while Self::current_index() != session {
				Self::roll_next();
			}
		}

		pub fn roll_until_active_era(era: EraIndex) {
			while active_era() != era {
				Self::roll_next();
			}
		}

		fn maybe_rotate_session_now() {
			let now = System::block_number();
			let period = Period::get();
			if now % period == 0 {
				Self::advance_session();
			}
		}

		fn advance_session() {
			let ending = Self::current_index();
			if let Some((q, id)) = Queued::get().zip(QueuedId::get()) {
				// we have something queued
				if QueuedBufferSessions::get() == 0 {
					// buffer time has passed
					Active::set(q);
					Rotator::<Test>::end_session(ending, Some((Timestamp::get(), id)));
					Queued::reset();
					QueuedId::reset();
				} else {
					QueuedBufferSessions::mutate(|s| *s -= 1);
					Rotator::<Test>::end_session(ending, None);
				}
			} else {
				// just end the session.
				Rotator::<Test>::end_session(ending, None);
			}
			CurrentIndex::set(ending + 1);
		}
	}

	parameter_types! {
		pub static ReceivedValidatorSets
			: BTreeMap<BlockNumber, ValidatorSetReport<AccountId>>
			= BTreeMap::new();
		pub static Queued: Option<Vec<AccountId>> = None;
		pub static QueuedId: Option<u32> = None;
		pub static QueuedBufferSessions: BlockNumber = 1;
		pub static Active: Vec<AccountId> = Vec::new();
		pub static CurrentIndex: u32 = 0;
		pub static Timestamp: u64 = INIT_TIMESTAMP;
	}

	impl ReceivedValidatorSets {
		pub fn get_last() -> ValidatorSetReport<AccountId> {
			let mut data = Self::get();
			data.pop_last().unwrap().1
		}
	}

	impl pallet_staking_async_rc_client::RcClientInterface for Session {
		type AccountId = AccountId;

		fn validator_set(
			new_validator_set: Vec<Self::AccountId>,
			id: u32,
			prune_up_to: Option<u32>,
		) {
			log::debug!(target: "runtime::session_mock", "Received validator set: {:?}", new_validator_set);
			let now = System::block_number();
			// store the report for further inspection.
			ReceivedValidatorSets::mutate(|reports| {
				reports.insert(
					now,
					ValidatorSetReport {
						id,
						new_validator_set: new_validator_set.clone(),
						prune_up_to,
						leftover: false,
					},
				);
			});

			// queue the validator set.
			Queued::set(Some(new_validator_set));
			QueuedId::set(Some(id));
			QueuedBufferSessions::set(1);
		}
	}
}

pub use session_mock::Session;

ord_parameter_types! {
	pub const One: u64 = 1;
}

parameter_types! {
	pub static RemainderRatio: Perbill = Perbill::from_percent(50);
}
pub struct OneTokenPerMillisecond;
impl EraPayout<Balance> for OneTokenPerMillisecond {
	fn era_payout(
		_total_staked: Balance,
		_total_issuance: Balance,
		era_duration_millis: u64,
	) -> (Balance, Balance) {
		let total = era_duration_millis as Balance;
		let remainder = RemainderRatio::get() * total;
		let stakers = total - remainder;
		(stakers, remainder)
	}
}

impl crate::pallet::pallet::Config for Test {
	type RuntimeHoldReason = RuntimeHoldReason;
	type OldCurrency = Balances;
	type Currency = Balances;
	type RewardRemainder = RewardRemainderMock;
	type Reward = MockReward;
	type SessionsPerEra = SessionsPerEra;
	type SlashDeferDuration = SlashDeferDuration;
	type AdminOrigin = EitherOfDiverse<EnsureRoot<AccountId>, EnsureSignedBy<One, AccountId>>;
	type EraPayout = OneTokenPerMillisecond;
	type MaxExposurePageSize = MaxExposurePageSize;
	type MaxValidatorSet = MaxValidatorSet;
	type ElectionProvider = TestElectionProvider;
	type VoterList = VoterBagsList;
	type TargetList = UseValidatorsMap<Self>;
	type NominationsQuota = WeightedNominationsQuota<16>;
	type MaxUnlockingChunks = MaxUnlockingChunks;
	type HistoryDepth = HistoryDepth;
	type BondingDuration = BondingDuration;
	type MaxControllersInDeprecationBatch = MaxControllersInDeprecationBatch;
	type EventListeners = EventListenerMock;
	type MaxInvulnerables = ConstU32<20>;
	type MaxDisabledValidators = ConstU32<100>;
	type PlanningEraOffset = PlanningEraOffset;
	type Filter = MockedRestrictList;
	type RcClientInterface = session_mock::Session;
	type CurrencyBalance = Balance;
	type CurrencyToVote = SaturatingCurrencyToVote;
	type Slash = ();
	type WeightInfo = ();
}

pub struct WeightedNominationsQuota<const MAX: u32>;
impl<Balance, const MAX: u32> NominationsQuota<Balance> for WeightedNominationsQuota<MAX>
where
	u128: From<Balance>,
{
	type MaxNominations = AbsoluteMaxNominations;

	fn curve(balance: Balance) -> u32 {
		match balance.into() {
			// random curve for testing.
			0..=110 => MAX,
			111 => 0,
			222 => 2,
			333 => MAX + 10,
			_ => MAX,
		}
	}
}

parameter_types! {
	// if true, skips the try-state for the test running.
	pub static SkipTryStateCheck: bool = false;
}

pub struct ExtBuilder {
	nominate: bool,
	validator_count: u32,
	invulnerables: BoundedVec<AccountId, <Test as Config>::MaxInvulnerables>,
	has_stakers: bool,
	pub min_nominator_bond: Balance,
	min_validator_bond: Balance,
	balance_factor: Balance,
	status: BTreeMap<AccountId, StakerStatus<AccountId>>,
	stakes: BTreeMap<AccountId, Balance>,
	stakers: Vec<(AccountId, Balance, StakerStatus<AccountId>)>,
	flush_events: bool,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self {
			nominate: true,
			validator_count: 2,
			balance_factor: 1,
			invulnerables: BoundedVec::new(),
			has_stakers: true,
			min_nominator_bond: ExistentialDeposit::get(),
			min_validator_bond: ExistentialDeposit::get(),
			status: Default::default(),
			stakes: Default::default(),
			stakers: Default::default(),
			flush_events: true,
		}
	}
}

#[allow(unused)]
impl ExtBuilder {
	pub(crate) fn existential_deposit(self, existential_deposit: Balance) -> Self {
		EXISTENTIAL_DEPOSIT.with(|v| *v.borrow_mut() = existential_deposit);
		self
	}
	pub(crate) fn max_unlock_chunks(self, max: u32) -> Self {
		MaxUnlockingChunks::set(max);
		self
	}
	pub(crate) fn bonding_duration(self, bonding_duration: EraIndex) -> Self {
		BondingDuration::set(bonding_duration);
		self
	}
	pub(crate) fn planning_era_offset(self, offset: SessionIndex) -> Self {
		PlanningEraOffset::set(offset);
		self
	}
	pub(crate) fn nominate(mut self, nominate: bool) -> Self {
		self.nominate = nominate;
		self
	}
	pub(crate) fn no_flush_events(mut self) -> Self {
		self.flush_events = false;
		self
	}
	pub(crate) fn validator_count(mut self, count: u32) -> Self {
		self.validator_count = count;
		self
	}
	pub(crate) fn slash_defer_duration(self, eras: EraIndex) -> Self {
		SlashDeferDuration::set(eras);
		self
	}
	pub(crate) fn invulnerables(mut self, invulnerables: Vec<AccountId>) -> Self {
		self.invulnerables = BoundedVec::try_from(invulnerables)
			.expect("Too many invulnerable validators: upper limit is MaxInvulnerables");
		self
	}
	pub(crate) fn session_per_era(self, length: SessionIndex) -> Self {
		SessionsPerEra::set(length);
		self
	}
	pub(crate) fn period(self, length: BlockNumber) -> Self {
		Period::set(length);
		self
	}
	pub(crate) fn has_stakers(mut self, has: bool) -> Self {
		self.has_stakers = has;
		self
	}
	pub(crate) fn offset(self, offset: BlockNumber) -> Self {
		OFFSET.with(|v| *v.borrow_mut() = offset);
		self
	}
	pub(crate) fn min_nominator_bond(mut self, amount: Balance) -> Self {
		self.min_nominator_bond = amount;
		self
	}
	pub(crate) fn min_validator_bond(mut self, amount: Balance) -> Self {
		self.min_validator_bond = amount;
		self
	}
	pub(crate) fn set_status(mut self, who: AccountId, status: StakerStatus<AccountId>) -> Self {
		self.status.insert(who, status);
		self
	}
	pub(crate) fn set_stake(mut self, who: AccountId, stake: Balance) -> Self {
		self.stakes.insert(who, stake);
		self
	}
	pub(crate) fn add_staker(
		mut self,
		stash: AccountId,
		stake: Balance,
		status: StakerStatus<AccountId>,
	) -> Self {
		self.stakers.push((stash, stake, status));
		self
	}
	pub(crate) fn exposures_page_size(self, max: u32) -> Self {
		MaxExposurePageSize::set(max);
		self
	}
	pub(crate) fn balance_factor(mut self, factor: Balance) -> Self {
		self.balance_factor = factor;
		self
	}
	pub(crate) fn multi_page_election_provider(self, pages: PageIndex) -> Self {
		Pages::set(pages);
		self
	}
	pub(crate) fn election_bounds(self, voter_count: u32, target_count: u32) -> Self {
		let bounds = ElectionBoundsBuilder::default()
			.voters_count(voter_count.into())
			.targets_count(target_count.into())
			.build();
		ElectionsBounds::set(bounds);
		self
	}
	pub(crate) fn max_winners_per_page(self, max: u32) -> Self {
		MaxWinnersPerPage::set(max);
		self
	}
	pub(crate) fn try_state(self, enable: bool) -> Self {
		SkipTryStateCheck::set(!enable);
		self
	}
	fn build(self) -> sp_io::TestExternalities {
		sp_tracing::try_init_simple();
		let mut storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
		let ed = ExistentialDeposit::get();

		let mut maybe_stakers = vec![];
		if self.has_stakers {
			maybe_stakers = vec![
				// (stash, stake, status)
				// these two will be elected in the default test where we elect 2.
				(11, self.balance_factor * 1000, StakerStatus::<AccountId>::Validator),
				(21, self.balance_factor * 1000, StakerStatus::<AccountId>::Validator),
				// a loser validator
				(31, self.balance_factor * 500, StakerStatus::<AccountId>::Validator),
				// idle stakers
				(41, self.balance_factor * 4000, StakerStatus::<AccountId>::Idle),
				(51, self.balance_factor * 5000, StakerStatus::<AccountId>::Idle),
			]; // optionally add a nominator
			if self.nominate {
				maybe_stakers.push((
					101,
					self.balance_factor * 500,
					StakerStatus::<AccountId>::Nominator(vec![11, 21]),
				))
			}
			// replace any of the status if needed.
			self.status.into_iter().for_each(|(stash, status)| {
				let (_, _, ref mut prev_status) = maybe_stakers
					.iter_mut()
					.find(|s| s.0 == stash)
					.expect("set_status staker should exist; qed");
				*prev_status = status;
			});
			// replaced any of the stakes if needed.
			self.stakes.into_iter().for_each(|(stash, stake)| {
				let (_, ref mut prev_stake, _) = maybe_stakers
					.iter_mut()
					.find(|s| s.0 == stash)
					.expect("set_stake staker should exits; qed.");
				*prev_stake = stake;
			});
			// extend stakers if needed.
			maybe_stakers.extend(self.stakers)
		}

		let aux_balances = vec![
			// aux accounts
			(1, ed + 10 * self.balance_factor),
			(2, ed + 20 * self.balance_factor),
			(3, ed + 300 * self.balance_factor),
			(4, ed + 400 * self.balance_factor),
			// This allows us to have a total_payout different from 0.
			(999, 1_000_000_000_000),
		];
		// given each stakers their stake + ed as balance.
		let stakers_balances =
			maybe_stakers.clone().into_iter().map(|(who, stake, _)| (who, stake + ed));
		let balances = aux_balances.into_iter().chain(stakers_balances).collect::<Vec<_>>();

		let _ = pallet_balances::GenesisConfig::<Test> { balances, ..Default::default() }
			.assimilate_storage(&mut storage);

		let _ = pallet_staking_async::GenesisConfig::<Test> {
			stakers: maybe_stakers,
			validator_count: self.validator_count,
			invulnerables: self.invulnerables,
			active_era: (0, 0, INIT_TIMESTAMP),
			slash_reward_fraction: Perbill::from_percent(10),
			min_nominator_bond: self.min_nominator_bond,
			min_validator_bond: self.min_validator_bond,
			..Default::default()
		}
		.assimilate_storage(&mut storage);

		let mut ext = sp_io::TestExternalities::from(storage);

		ext.execute_with(|| {
			session_mock::Session::roll_until_active_era(1);
			RewardRemainderUnbalanced::set(0);
			if self.flush_events {
				let _ = staking_events_since_last_call();
			}
		});

		ext
	}
	pub fn build_and_execute(self, test: impl FnOnce() -> ()) {
		sp_tracing::try_init_simple();
		let mut ext = self.build();
		ext.execute_with(test);
		ext.execute_with(|| {
			if !SkipTryStateCheck::get() {
				Staking::do_try_state(System::block_number()).unwrap();
			}
		});
	}
}

pub(crate) fn active_era() -> EraIndex {
	pallet_staking_async::ActiveEra::<Test>::get().unwrap().index
}

pub(crate) fn current_era() -> EraIndex {
	pallet_staking_async::CurrentEra::<Test>::get().unwrap()
}

pub(crate) fn bond(who: AccountId, val: Balance) {
	let _ = asset::set_stakeable_balance::<Test>(&who, val);
	assert_ok!(Staking::bond(RuntimeOrigin::signed(who), val, RewardDestination::Stash));
}

pub(crate) fn bond_validator(who: AccountId, val: Balance) {
	bond(who, val);
	assert_ok!(Staking::validate(RuntimeOrigin::signed(who), ValidatorPrefs::default()));
}

pub(crate) fn bond_nominator(who: AccountId, val: Balance, target: Vec<AccountId>) {
	bond(who, val);
	assert_ok!(Staking::nominate(RuntimeOrigin::signed(who), target));
}

pub(crate) fn bond_virtual_nominator(
	who: AccountId,
	payee: AccountId,
	val: Balance,
	target: Vec<AccountId>,
) {
	// Bond who virtually.
	assert_ok!(<Staking as sp_staking::StakingUnchecked>::virtual_bond(&who, val, &payee));
	assert_ok!(Staking::nominate(RuntimeOrigin::signed(who), target));
}

pub(crate) fn validator_payout_for(duration: u64) -> Balance {
	let (payout, _rest) = <Test as Config>::EraPayout::era_payout(
		pallet_staking_async::ErasTotalStake::<Test>::get(active_era()),
		pallet_balances::TotalIssuance::<Test>::get(),
		duration,
	);
	assert!(payout > 0);
	payout
}

pub(crate) fn total_payout_for(duration: u64) -> Balance {
	let (payout, rest) = <Test as Config>::EraPayout::era_payout(
		pallet_staking_async::ErasTotalStake::<Test>::get(active_era()),
		pallet_balances::TotalIssuance::<Test>::get(),
		duration,
	);
	payout + rest
}

/// Time it takes to finish a session.
///
/// Note, if you see `time_per_session() - BLOCK_TIME`, it is fine. This is because we set the
/// timestamp after on_initialize, so the timestamp is always one block old.
pub(crate) fn time_per_session() -> u64 {
	Period::get() * BLOCK_TIME
}

/// Time it takes to finish an era.
pub(crate) fn time_per_era() -> u64 {
	time_per_session() * SessionsPerEra::get() as u64
}

pub(crate) fn reward_all_elected() {
	let rewards = session_mock::Session::validators().into_iter().map(|v| (v, 1));
	<Pallet<Test>>::reward_by_ids(rewards)
}

pub(crate) fn era_exposures(era: u32) -> Vec<(AccountId, Exposure<AccountId, Balance>)> {
	ErasStakersOverview::<T>::iter_prefix(era)
		.map(|(v, _overview)| (v, Staking::eras_stakers(era, &v)))
		.collect::<Vec<_>>()
}

pub(crate) fn session_validators() -> Vec<AccountId> {
	Session::validators()
}

/// Add a slash for who
pub(crate) fn add_slash(who: AccountId) {
	let _ = <Staking as rc_client::AHStakingInterface>::on_new_offences(
		session_mock::Session::current_index(),
		vec![rc_client::Offence {
			offender: who,
			reporters: vec![],
			slash_fraction: Perbill::from_percent(10),
		}],
	);
}

pub(crate) fn add_slash_in_era(who: AccountId, era: EraIndex) {
	let _ = <Staking as rc_client::AHStakingInterface>::on_new_offences(
		ErasStartSessionIndex::<T>::get(era).unwrap(),
		vec![rc_client::Offence {
			offender: who,
			reporters: vec![],
			slash_fraction: Perbill::from_percent(10),
		}],
	);
}

pub(crate) fn add_slash_in_era_with_value(who: AccountId, era: EraIndex, p: Perbill) {
	let _ = <Staking as rc_client::AHStakingInterface>::on_new_offences(
		ErasStartSessionIndex::<T>::get(era).unwrap(),
		vec![rc_client::Offence { offender: who, reporters: vec![], slash_fraction: p }],
	);
}

pub(crate) fn add_slash_with_percent(who: AccountId, percent: u32) {
	let _ = <Staking as rc_client::AHStakingInterface>::on_new_offences(
		session_mock::Session::current_index(),
		vec![rc_client::Offence {
			offender: who,
			reporters: vec![],
			slash_fraction: Perbill::from_percent(percent),
		}],
	);
}

/// Make all validator and nominator request their payment
pub(crate) fn make_all_reward_payment(era: EraIndex) {
	let validators_with_reward = ErasRewardPoints::<Test>::get(era)
		.individual
		.keys()
		.cloned()
		.collect::<Vec<_>>();

	// reward validators
	for validator_controller in validators_with_reward.iter().filter_map(Staking::bonded) {
		let ledger = <Ledger<Test>>::get(&validator_controller).unwrap();
		for page in 0..Eras::<Test>::exposure_page_count(era, &ledger.stash) {
			assert_ok!(Staking::payout_stakers_by_page(
				RuntimeOrigin::signed(1337),
				ledger.stash,
				era,
				page
			));
		}
	}
}

pub(crate) fn bond_controller_stash(controller: AccountId, stash: AccountId) -> Result<(), String> {
	<Bonded<Test>>::get(&stash).map_or(Ok(()), |_| Err("stash already bonded"))?;
	<Ledger<Test>>::get(&controller).map_or(Ok(()), |_| Err("controller already bonded"))?;

	<Bonded<Test>>::insert(stash, controller);
	<Ledger<Test>>::insert(controller, StakingLedger::<Test>::default_from(stash));
	<Payee<Test>>::insert(stash, RewardDestination::Staked);

	Ok(())
}

// simulates `set_controller` without corrupted ledger checks for testing purposes.
pub(crate) fn set_controller_no_checks(stash: &AccountId) {
	let controller = Bonded::<Test>::get(stash).expect("testing stash should be bonded");
	let ledger = Ledger::<Test>::get(&controller).expect("testing ledger should exist");

	Ledger::<Test>::remove(&controller);
	Ledger::<Test>::insert(stash, ledger);
	Bonded::<Test>::insert(stash, stash);
}

// simulates `bond_extra` without corrupted ledger checks for testing purposes.
pub(crate) fn bond_extra_no_checks(stash: &AccountId, amount: Balance) {
	let controller = Bonded::<Test>::get(stash).expect("bond must exist to bond_extra");
	let mut ledger = Ledger::<Test>::get(&controller).expect("ledger must exist to bond_extra");

	let new_total = ledger.total + amount;
	let _ = asset::update_stake::<Test>(stash, new_total);
	ledger.total = new_total;
	ledger.active = new_total;
	Ledger::<Test>::insert(controller, ledger);
}

pub(crate) fn setup_double_bonded_ledgers() {
	let init_ledgers = Ledger::<Test>::iter().count();

	let _ = asset::set_stakeable_balance::<Test>(&333, 2000);
	let _ = asset::set_stakeable_balance::<Test>(&444, 2000);
	let _ = asset::set_stakeable_balance::<Test>(&555, 2000);
	let _ = asset::set_stakeable_balance::<Test>(&777, 2000);

	assert_ok!(Staking::bond(RuntimeOrigin::signed(333), 10, RewardDestination::Staked));
	assert_ok!(Staking::bond(RuntimeOrigin::signed(444), 20, RewardDestination::Staked));
	assert_ok!(Staking::bond(RuntimeOrigin::signed(555), 20, RewardDestination::Staked));
	// not relevant to the test case, but ensures try-runtime checks pass.
	[333, 444, 555]
		.iter()
		.for_each(|s| Payee::<Test>::insert(s, RewardDestination::Staked));

	// we want to test the case where a controller can also be a stash of another ledger.
	// for that, we change the controller/stash bonding so that:
	// * 444 becomes controller of 333.
	// * 555 becomes controller of 444.
	// * 777 becomes controller of 555.
	let ledger_333 = Ledger::<Test>::get(333).unwrap();
	let ledger_444 = Ledger::<Test>::get(444).unwrap();
	let ledger_555 = Ledger::<Test>::get(555).unwrap();

	// 777 becomes controller of 555.
	Bonded::<Test>::mutate(555, |controller| *controller = Some(777));
	Ledger::<Test>::insert(777, ledger_555);

	// 555 becomes controller of 444.
	Bonded::<Test>::mutate(444, |controller| *controller = Some(555));
	Ledger::<Test>::insert(555, ledger_444);

	// 444 becomes controller of 333.
	Bonded::<Test>::mutate(333, |controller| *controller = Some(444));
	Ledger::<Test>::insert(444, ledger_333);

	// 333 is not controller anymore.
	Ledger::<Test>::remove(333);

	// checks. now we have:
	// * +3 ledgers
	assert_eq!(Ledger::<Test>::iter().count(), 3 + init_ledgers);

	// * stash 333 has controller 444.
	assert_eq!(Bonded::<Test>::get(333), Some(444));
	assert_eq!(StakingLedger::<Test>::paired_account(StakingAccount::Stash(333)), Some(444));
	assert_eq!(Ledger::<Test>::get(444).unwrap().stash, 333);

	// * stash 444 has controller 555.
	assert_eq!(Bonded::<Test>::get(444), Some(555));
	assert_eq!(StakingLedger::<Test>::paired_account(StakingAccount::Stash(444)), Some(555));
	assert_eq!(Ledger::<Test>::get(555).unwrap().stash, 444);

	// * stash 555 has controller 777.
	assert_eq!(Bonded::<Test>::get(555), Some(777));
	assert_eq!(StakingLedger::<Test>::paired_account(StakingAccount::Stash(555)), Some(777));
	assert_eq!(Ledger::<Test>::get(777).unwrap().stash, 555);
}

#[macro_export]
macro_rules! assert_session_era {
	($session:expr, $era:expr) => {
		assert_eq!(
			session_mock::Session::current_index(),
			$session,
			"wrong session {} != {}",
			session_mock::Session::current_index(),
			$session,
		);
		assert_eq!(
			CurrentEra::<T>::get().unwrap(),
			$era,
			"wrong current era {} != {}",
			CurrentEra::<T>::get().unwrap(),
			$era,
		);
	};
}

pub(crate) fn staking_events() -> Vec<crate::Event<Test>> {
	System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(|e| if let RuntimeEvent::Staking(inner) = e { Some(inner) } else { None })
		.collect()
}

parameter_types! {
	static StakingEventsIndex: usize = 0;
}

pub(crate) fn staking_events_since_last_call() -> Vec<crate::Event<Test>> {
	let all: Vec<_> = System::events()
		.into_iter()
		.filter_map(|r| if let RuntimeEvent::Staking(inner) = r.event { Some(inner) } else { None })
		.collect();
	let seen = StakingEventsIndex::get();
	StakingEventsIndex::set(all.len());
	all.into_iter().skip(seen).collect()
}

pub(crate) fn to_bounded_supports(
	supports: Vec<(AccountId, Support<AccountId>)>,
) -> BoundedSupports<
	AccountId,
	<<Test as Config>::ElectionProvider as ElectionProvider>::MaxWinnersPerPage,
	<<Test as Config>::ElectionProvider as ElectionProvider>::MaxBackersPerWinner,
> {
	supports.try_into().unwrap()
}

pub(crate) fn restrict(who: &AccountId) {
	if !RestrictedAccounts::get().contains(who) {
		RestrictedAccounts::mutate(|l| l.push(*who));
	}
}

pub(crate) fn remove_from_restrict_list(who: &AccountId) {
	RestrictedAccounts::mutate(|l| l.retain(|x| x != who));
}
