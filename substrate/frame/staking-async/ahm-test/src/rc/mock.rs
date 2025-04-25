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

use ah_client::OperatingMode;
use frame::{
	deps::sp_runtime::testing::UintAuthorityId, testing_prelude::*, traits::fungible::Mutate,
};
use frame_election_provider_support::{
	bounds::{ElectionBounds, ElectionBoundsBuilder},
	onchain, SequentialPhragmen,
};
use frame_support::traits::FindAuthor;
use pallet_staking_async_ah_client as ah_client;
use sp_staking::SessionIndex;

use crate::shared;

pub type T = Runtime;

construct_runtime! {
	pub enum Runtime {
		System: frame_system,
		Authorship: pallet_authorship,
		Balances: pallet_balances,
		Timestamp: pallet_timestamp,

		Session: pallet_session,
		SessionHistorical: pallet_session::historical,
		Staking: pallet_staking,
		StakingAhClient: pallet_staking_async_ah_client,
		RootOffences: pallet_root_offences,
	}
}

pub fn roll_next() {
	let now = System::block_number();
	let next = now + 1;

	System::set_block_number(next);
	// Timestamp is always the RC block number * 1000
	Timestamp::set_timestamp(next * 1000);
	Authorship::on_initialize(next);

	Session::on_initialize(next);
	StakingAhClient::on_initialize(next);
	Staking::on_initialize(next);
	Staking::on_finalize(next);
}

parameter_types! {
	/// The maximum number of blocks to roll before we stop rolling.
	///
	/// Avoids infinite loops in tests.
	pub static MaxRollsUntilCriteria: u16 = 1000;
}

pub fn roll_until_matches(criteria: impl Fn() -> bool, with_ah: bool) {
	let mut rolls = 0;
	while !criteria() {
		roll_next();
		rolls += 1;
		if with_ah {
			if LocalQueue::get().is_some() {
				panic!("when local queue is set, you cannot roll ah forward as well!")
			}
			shared::in_ah(|| {
				crate::ah::roll_next();
			});
		}

		if rolls > MaxRollsUntilCriteria::get() {
			panic!("rolled too many times");
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
	type AccountData = pallet_balances::AccountData<u128>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
	type Balance = u128;
	type AccountStore = System;
}

impl pallet_timestamp::Config for Runtime {
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = ConstU64<3>;
	type WeightInfo = ();
}

pub struct ValidatorIdOf;
impl Convert<AccountId, Option<AccountId>> for ValidatorIdOf {
	fn convert(a: AccountId) -> Option<AccountId> {
		Some(a)
	}
}

pub struct OtherSessionHandler;
impl OneSessionHandler<AccountId> for OtherSessionHandler {
	type Key = UintAuthorityId;

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

impl BoundToRuntimeAppPublic for OtherSessionHandler {
	type Public = UintAuthorityId;
}

frame::deps::sp_runtime::impl_opaque_keys! {
	pub struct SessionKeys {
		pub other: OtherSessionHandler,
	}
}

parameter_types! {
	pub static Period: BlockNumber = 30;
	pub static Offset: BlockNumber = 0;
}

impl pallet_session::historical::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type FullIdentification = sp_staking::Exposure<AccountId, Balance>;
	type FullIdentificationOf = ah_client::DefaultExposureOf<Self>;
}

impl pallet_session::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;

	type ValidatorIdOf = ValidatorIdOf;
	type ValidatorId = AccountId;

	type DisablingStrategy = pallet_session::disabling::UpToLimitDisablingStrategy<1>;

	type Keys = SessionKeys;
	type SessionHandler = <SessionKeys as frame::traits::OpaqueKeys>::KeyTypeIdProviders;

	type NextSessionRotation = Self::ShouldEndSession;
	type ShouldEndSession = pallet_session::PeriodicSessions<Period, Offset>;

	// Should be AH-client
	type SessionManager = pallet_session::historical::NoteHistoricalRoot<Self, StakingAhClient>;

	type WeightInfo = ();
}

parameter_types! {
	pub static DefaultAuthor: Option<AccountId> = Some(11);
}

pub struct GetAuthor;
impl FindAuthor<AccountId> for GetAuthor {
	fn find_author<'a, I>(_digests: I) -> Option<AccountId>
	where
		I: 'a + IntoIterator<Item = (frame_support::ConsensusEngineId, &'a [u8])>,
	{
		DefaultAuthor::get()
	}
}

impl pallet_authorship::Config for Runtime {
	type FindAuthor = GetAuthor;
	type EventHandler = StakingAhClient;
}

parameter_types! {
	pub static MaxBackersPerWinner: u32 = 256;
	pub static MaxWinnersPerPage: u32 = 100;
	pub static ElectionsBounds: ElectionBounds = ElectionBoundsBuilder::default().build();
}
pub struct OnChainSeqPhragmen;
impl onchain::Config for OnChainSeqPhragmen {
	type System = Runtime;
	type Solver = SequentialPhragmen<AccountId, Perbill>;
	type DataProvider = Staking;
	type WeightInfo = ();
	type MaxBackersPerWinner = MaxBackersPerWinner;
	type MaxWinnersPerPage = MaxWinnersPerPage;
	type Bounds = ElectionsBounds;
	type Sort = ConstBool<true>;
}

#[derive_impl(pallet_staking::config_preludes::TestDefaultConfig)]
impl pallet_staking::Config for Runtime {
	type OldCurrency = Balances;
	type Currency = Balances;
	type UnixTime = pallet_timestamp::Pallet<Self>;
	type AdminOrigin = frame_system::EnsureRoot<Self::AccountId>;
	type EraPayout = ();
	type ElectionProvider = onchain::OnChainExecution<OnChainSeqPhragmen>;
	type GenesisElectionProvider = Self::ElectionProvider;
	type VoterList = pallet_staking::UseNominatorsAndValidatorsMap<Self>;
	type TargetList = pallet_staking::UseValidatorsMap<Self>;
	type BenchmarkingConfig = pallet_staking::TestBenchmarkingConfig;
	type SlashDeferDuration = ConstU32<2>;
	type SessionInterface = Self;
	type BondingDuration = ConstU32<3>;
}

impl pallet_root_offences::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OffenceHandler = StakingAhClient;
}

#[derive(Clone, Debug, PartialEq)]
pub enum OutgoingMessages {
	SessionReport(rc_client::SessionReport<AccountId>),
	OffenceReport(SessionIndex, Vec<rc_client::Offence<AccountId>>),
}

parameter_types! {
	pub static MinimumValidatorSetSize: u32 = 4;
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

impl ah_client::Config for Runtime {
	type CurrencyBalance = Balance;
	type AdminOrigin = EnsureRoot<AccountId>;
	type SendToAssetHub = DeliverToAH;
	type AssetHubOrigin = EnsureSigned<AccountId>;
	type UnixTime = Timestamp;
	type MinimumValidatorSetSize = MinimumValidatorSetSize;
	type PointsPerBlock = ConstU32<20>;
	type SessionInterface = Self;
	type Fallback = Staking;
}

use pallet_staking_async_rc_client::{self as rc_client, ValidatorSetReport};
pub struct DeliverToAH;
impl ah_client::SendToAssetHub for DeliverToAH {
	type AccountId = AccountId;
	fn relay_new_offence(
		session_index: SessionIndex,
		offences: Vec<rc_client::Offence<Self::AccountId>>,
	) {
		if let Some(mut local_queue) = LocalQueue::get() {
			local_queue.push((
				System::block_number(),
				OutgoingMessages::OffenceReport(session_index, offences),
			));
			LocalQueue::set(Some(local_queue));
		} else {
			shared::CounterRCAHNewOffence::mutate(|x| *x += 1);
			shared::in_ah(|| {
				let origin = crate::ah::RuntimeOrigin::root();
				rc_client::Pallet::<crate::ah::Runtime>::relay_new_offence(
					origin,
					session_index,
					offences.clone(),
				)
				.unwrap();
			});
		}
	}

	fn relay_session_report(session_report: rc_client::SessionReport<Self::AccountId>) {
		if let Some(mut local_queue) = LocalQueue::get() {
			local_queue
				.push((System::block_number(), OutgoingMessages::SessionReport(session_report)));
			LocalQueue::set(Some(local_queue));
		} else {
			shared::CounterRCAHSessionReport::mutate(|x| *x += 1);
			shared::in_ah(|| {
				let origin = crate::ah::RuntimeOrigin::root();
				rc_client::Pallet::<crate::ah::Runtime>::relay_session_report(
					origin,
					session_report.clone(),
				)
				.unwrap();
			});
		}
	}
}

parameter_types! {
	pub static SessionEventsIndex: usize = 0;
	pub static HistoricalEventsIndex: usize = 0;
	pub static AhClientEventsIndex: usize = 0;
}

pub fn historical_events_since_last_call() -> Vec<pallet_session::historical::Event<Runtime>> {
	let all = frame_system::Pallet::<Runtime>::read_events_for_pallet::<
		pallet_session::historical::Event<Runtime>,
	>();
	let seen = HistoricalEventsIndex::get();
	HistoricalEventsIndex::set(all.len());
	all.into_iter().skip(seen).collect()
}

pub fn session_events_since_last_call() -> Vec<pallet_session::Event<Runtime>> {
	let all =
		frame_system::Pallet::<Runtime>::read_events_for_pallet::<pallet_session::Event<Runtime>>();
	let seen = SessionEventsIndex::get();
	SessionEventsIndex::set(all.len());
	all.into_iter().skip(seen).collect()
}

pub fn ah_client_events_since_last_call() -> Vec<ah_client::Event<Runtime>> {
	let all =
		frame_system::Pallet::<Runtime>::read_events_for_pallet::<ah_client::Event<Runtime>>();
	let seen = AhClientEventsIndex::get();
	AhClientEventsIndex::set(all.len());
	all.into_iter().skip(seen).collect()
}

const INITIAL_STAKE: Balance = 100;
const INITIAL_BALANCE: Balance = 1000;

pub struct ExtBuilder {
	session_keys: Vec<AccountId>,
	pre_migration: bool,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self { session_keys: vec![], pre_migration: false }
	}
}

impl ExtBuilder {
	/// Set this if you want to test the rc-runtime locally. This will push outgoing messages to
	/// `LocalQueue` instead of enacting them on AH.
	pub fn local_queue(self) -> Self {
		LocalQueue::set(Some(Default::default()));
		self
	}

	/// Set the session keys for the given accounts.
	pub fn session_keys(mut self, session_keys: Vec<AccountId>) -> Self {
		self.session_keys = session_keys;
		self
	}

	/// Don't set 11 as the automatic block author of every block
	pub fn no_default_author(self) -> Self {
		DefaultAuthor::set(None);
		self
	}

	/// Set the staking-classic state to be pre-AHM-migration state.
	pub fn pre_migration(mut self) -> Self {
		self.pre_migration = true;
		self
	}

	/// Set the smallest number of validators to be received by ah-client
	pub fn minimum_validator_set_size(self, size: u32) -> Self {
		MinimumValidatorSetSize::set(size);
		self
	}

	pub fn build(self) -> TestState {
		let _ = sp_tracing::try_init_simple();
		let mut t = frame_system::GenesisConfig::<T>::default().build_storage().unwrap();

		// add pre-migration state to staking-classic.
		let operating_mode = if self.pre_migration {
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
			.map(|(x, y)| (x, x, INITIAL_STAKE, pallet_staking_async::StakerStatus::Nominator(y)));

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

			// Set ah client in passive mode -> implies it is inactive and staking-classic is
			// active.
			OperatingMode::Passive
		} else {
			OperatingMode::Active
		};

		let mut state: TestState = t.into();

		state.execute_with(|| {
			// so events can be deposited.
			roll_next();

			for v in self.session_keys {
				// min some funds, create account and ref counts
				pallet_balances::Pallet::<T>::mint_into(&v, INITIAL_BALANCE).unwrap();
				pallet_session::Pallet::<T>::set_keys(
					RuntimeOrigin::signed(v),
					SessionKeys { other: UintAuthorityId(v) },
					vec![],
				)
				.unwrap();
			}

			ah_client::Mode::<T>::put(operating_mode);
		});

		state
	}
}

/// Progress until `sessions`, receive a `new_validator_set` with `id`, and go forward to `sessions
/// + 1` such that it is queued in pallet-session. If `active`, then progress until `sessions + 2`
/// such that it is in the active session validators.
pub(crate) fn receive_validator_set_at(
	sessions: SessionIndex,
	id: u32,
	new_validator_set: Vec<AccountId>,
	activate: bool,
) {
	roll_until_matches(|| pallet_session::CurrentIndex::<Runtime>::get() == sessions, false);
	assert_eq!(pallet_session::CurrentIndex::<Runtime>::get(), sessions);

	let report = ValidatorSetReport {
		id,
		prune_up_to: None,
		leftover: false,
		new_validator_set: new_validator_set.clone(),
	};

	assert_ok!(ah_client::Pallet::<Runtime>::validator_set(RuntimeOrigin::root(), report));

	// go forward till one more session such that these validators are in the session queue now
	roll_until_matches(|| pallet_session::CurrentIndex::<Runtime>::get() == sessions + 1, false);
	assert_eq!(pallet_session::CurrentIndex::<Runtime>::get(), sessions + 1);

	assert_eq!(
		pallet_session::QueuedKeys::<Runtime>::get()
			.into_iter()
			.map(|(x, _)| x)
			.collect::<Vec<_>>(),
		new_validator_set.clone(),
	);

	if activate {
		// if need be go one more session to activate them
		roll_until_matches(
			|| pallet_session::CurrentIndex::<Runtime>::get() == sessions + 2,
			false,
		);
		assert_eq!(pallet_session::Validators::<Runtime>::get(), new_validator_set);
	}
}
