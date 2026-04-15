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

// Disable warnings for `TransferStake` being deprecated.
#![allow(deprecated)]

use frame_election_provider_support::VoteWeight;
use frame_support::{
	assert_ok, derive_impl,
	pallet_prelude::*,
	parameter_types,
	traits::{ConstBool, ConstU64, ConstU8, Nothing, VariantCountOf},
	PalletId,
};
use frame_system::EnsureRoot;
use pallet_nomination_pools::{
	adapter::{Member, Pool, StakeStrategyType},
	BondType,
};
use sp_runtime::{
	traits::{Convert, IdentityLookup},
	BuildStorage, FixedU128, Perbill,
};
use pallet_staking_async::PotAccountProvider;
use sp_staking::budget::BudgetRecipient;

type AccountId = u128;
type Nonce = u32;
type BlockNumber = u64;
type Balance = u128;

pub(crate) type T = Runtime;

pub(crate) const POOL1_BONDED: AccountId = 20318131474730217858575332831085u128;
pub(crate) const POOL1_REWARD: AccountId = 20397359637244482196168876781421u128;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type Nonce = Nonce;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
}

impl pallet_timestamp::Config for Runtime {
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = ConstU64<5>;
	type WeightInfo = ();
}

parameter_types! {
	pub static ExistentialDeposit: Balance = 5;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
	type Balance = Balance;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type FreezeIdentifier = RuntimeFreezeReason;
	type MaxFreezes = VariantCountOf<RuntimeFreezeReason>;
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
}

pallet_staking_reward_curve::build! {
	const I_NPOS: sp_runtime::curve::PiecewiseLinear<'static> = curve!(
		min_inflation: 0_025_000,
		max_inflation: 0_100_000,
		ideal_stake: 0_500_000,
		falloff: 0_050_000,
		max_piece_count: 40,
		test_precision: 0_005_000,
	);
}

parameter_types! {
	pub const RewardCurve: &'static sp_runtime::curve::PiecewiseLinear<'static> = &I_NPOS;
	pub static BondingDuration: u32 = 3;
	pub static EraPayout: (Balance, Balance) = (1000, 100);
}

/// A simple EraPayout implementation for testing that returns fixed values.
pub struct TestEraPayout;
impl pallet_staking_async::EraPayout<Balance> for TestEraPayout {
	fn era_payout(
		_total_staked: Balance,
		_total_issuance: Balance,
		_era_duration_millis: u64,
	) -> (Balance, Balance) {
		EraPayout::get()
	}
}

/// A mock RcClientInterface for tests that don't need actual session/validator set management.
pub struct MockRcClient;
impl pallet_staking_async_rc_client::RcClientInterface for MockRcClient {
	type AccountId = AccountId;

	fn validator_set(
		_new_validator_set: Vec<Self::AccountId>,
		_id: u32,
		_prune_up_to: Option<u32>,
	) {
		// No-op for tests
	}
}

#[derive_impl(pallet_staking_async::config_preludes::TestDefaultConfig)]
impl pallet_staking_async::Config for Runtime {
	type OldCurrency = Balances;
	type Currency = Balances;
	type AdminOrigin = frame_system::EnsureRoot<Self::AccountId>;
	type EraPayout = TestEraPayout;
	type DisableMinting = ConstBool<false>;
	type BondingDuration = BondingDuration;
	type RewardPots = pallet_staking_async::SequentialTest;
	type ElectionProvider =
		frame_election_provider_support::NoElection<(AccountId, BlockNumber, Staking, (), ())>;
	type VoterList = VoterList;
	type TargetList = pallet_staking_async::UseValidatorsMap<Self>;
	type EventListeners = (Pools, DelegatedStaking);
	type RcClientInterface = MockRcClient;
}

parameter_types! {
	pub static BagThresholds: &'static [VoteWeight] = &[10, 20, 30, 40, 50, 60, 1_000, 2_000, 10_000];
}

type VoterBagsListInstance = pallet_bags_list::Instance1;
impl pallet_bags_list::Config<VoterBagsListInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type BagThresholds = BagThresholds;
	type ScoreProvider = Staking;
	type Score = VoteWeight;
	type MaxAutoRebagPerBlock = ();
}

pub struct BalanceToU256;
impl Convert<Balance, sp_core::U256> for BalanceToU256 {
	fn convert(n: Balance) -> sp_core::U256 {
		n.into()
	}
}

pub struct U256ToBalance;
impl Convert<sp_core::U256, Balance> for U256ToBalance {
	fn convert(n: sp_core::U256) -> Balance {
		n.try_into().unwrap()
	}
}

parameter_types! {
	pub const PostUnbondingPoolsWindow: u32 = 10;
	pub const PoolsPalletId: PalletId = PalletId(*b"py/nopls");
	pub static LegacyAdapter: bool = false;
}

pub struct MockAdapter;
type DelegateStake =
	pallet_nomination_pools::adapter::DelegateStake<Runtime, Staking, DelegatedStaking>;
type TransferStake = pallet_nomination_pools::adapter::TransferStake<Runtime, Staking>;
impl pallet_nomination_pools::adapter::StakeStrategy for MockAdapter {
	type Balance = Balance;
	type AccountId = AccountId;
	type CoreStaking = Staking;

	fn strategy_type() -> StakeStrategyType {
		if LegacyAdapter::get() {
			return TransferStake::strategy_type();
		}
		DelegateStake::strategy_type()
	}
	fn transferable_balance(
		pool_account: Pool<Self::AccountId>,
		member_account: Member<Self::AccountId>,
	) -> Self::Balance {
		if LegacyAdapter::get() {
			return TransferStake::transferable_balance(pool_account, member_account);
		}
		DelegateStake::transferable_balance(pool_account, member_account)
	}

	fn total_balance(pool_account: Pool<Self::AccountId>) -> Option<Self::Balance> {
		if LegacyAdapter::get() {
			return TransferStake::total_balance(pool_account);
		}
		DelegateStake::total_balance(pool_account)
	}

	fn member_delegation_balance(member_account: Member<Self::AccountId>) -> Option<Self::Balance> {
		if LegacyAdapter::get() {
			return TransferStake::member_delegation_balance(member_account);
		}
		DelegateStake::member_delegation_balance(member_account)
	}

	fn pledge_bond(
		who: Member<Self::AccountId>,
		pool_account: Pool<Self::AccountId>,
		reward_account: &Self::AccountId,
		amount: Self::Balance,
		bond_type: BondType,
	) -> DispatchResult {
		if LegacyAdapter::get() {
			return TransferStake::pledge_bond(
				who,
				pool_account,
				reward_account,
				amount,
				bond_type,
			);
		}
		DelegateStake::pledge_bond(who, pool_account, reward_account, amount, bond_type)
	}

	fn member_withdraw(
		who: Member<Self::AccountId>,
		pool_account: Pool<Self::AccountId>,
		amount: Self::Balance,
		num_slashing_spans: u32,
	) -> DispatchResult {
		if LegacyAdapter::get() {
			return TransferStake::member_withdraw(who, pool_account, amount, num_slashing_spans);
		}
		DelegateStake::member_withdraw(who, pool_account, amount, num_slashing_spans)
	}

	fn dissolve(pool_account: Pool<Self::AccountId>) -> DispatchResult {
		if LegacyAdapter::get() {
			return TransferStake::dissolve(pool_account);
		}
		DelegateStake::dissolve(pool_account)
	}

	fn pending_slash(pool_account: Pool<Self::AccountId>) -> Self::Balance {
		if LegacyAdapter::get() {
			return TransferStake::pending_slash(pool_account);
		}
		DelegateStake::pending_slash(pool_account)
	}

	fn member_slash(
		who: Member<Self::AccountId>,
		pool_account: Pool<Self::AccountId>,
		amount: Self::Balance,
		maybe_reporter: Option<Self::AccountId>,
	) -> DispatchResult {
		if LegacyAdapter::get() {
			return TransferStake::member_slash(who, pool_account, amount, maybe_reporter);
		}
		DelegateStake::member_slash(who, pool_account, amount, maybe_reporter)
	}

	fn migrate_nominator_to_agent(
		agent: Pool<Self::AccountId>,
		reward_account: &Self::AccountId,
	) -> DispatchResult {
		if LegacyAdapter::get() {
			return TransferStake::migrate_nominator_to_agent(agent, reward_account);
		}
		DelegateStake::migrate_nominator_to_agent(agent, reward_account)
	}

	fn migrate_delegation(
		agent: Pool<Self::AccountId>,
		delegator: Member<Self::AccountId>,
		value: Self::Balance,
	) -> DispatchResult {
		if LegacyAdapter::get() {
			return TransferStake::migrate_delegation(agent, delegator, value);
		}
		DelegateStake::migrate_delegation(agent, delegator, value)
	}
}
impl pallet_nomination_pools::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type Currency = Balances;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type RewardCounter = FixedU128;
	type BalanceToU256 = BalanceToU256;
	type U256ToBalance = U256ToBalance;
	type StakeAdapter = MockAdapter;
	type PostUnbondingPoolsWindow = PostUnbondingPoolsWindow;
	type MaxMetadataLen = ConstU32<256>;
	type MaxUnbonding = ConstU32<8>;
	type MaxPointsToBalance = ConstU8<10>;
	type PalletId = PoolsPalletId;
	type AdminOrigin = EnsureRoot<AccountId>;
	type BlockNumberProvider = System;
	type Filter = Nothing;
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
parameter_types! {
	pub const DapPalletId: PalletId = PalletId(*b"dap/buff");
	pub const DapIssuanceCadence: u64 = 0;
	pub const DapMaxElapsedPerDrip: u64 = 600_000;
}

/// Simple issuance curve for tests: 1 token per millisecond.
pub struct OneTokenPerMillisecond;
impl sp_staking::budget::IssuanceCurve<Balance> for OneTokenPerMillisecond {
	fn issue(_total_issuance: Balance, elapsed_millis: u64) -> Balance {
		elapsed_millis as Balance
	}
}

impl pallet_dap::Config for Runtime {
	type Currency = Balances;
	type PalletId = DapPalletId;
	type IssuanceCurve = OneTokenPerMillisecond;
	type BudgetRecipients = (
		pallet_dap::Pallet<Runtime>,
		pallet_staking_async::StakerRewardRecipient<pallet_staking_async::SequentialTest>,
		pallet_staking_async::ValidatorIncentiveRecipient<pallet_staking_async::SequentialTest>,
	);
	type Time = Timestamp;
	type IssuanceCadence = DapIssuanceCadence;
	type MaxElapsedPerDrip = DapMaxElapsedPerDrip;
	type BudgetOrigin = frame_system::EnsureRoot<AccountId>;
	type WeightInfo = ();
}

type Block = frame_system::mocking::MockBlock<Runtime>;

frame_support::construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		Timestamp: pallet_timestamp,
		Balances: pallet_balances,
		Staking: pallet_staking_async,
		VoterList: pallet_bags_list::<Instance1>,
		Pools: pallet_nomination_pools,
		DelegatedStaking: pallet_delegated_staking,
		Dap: pallet_dap,
	}
);

// Test validators that pools can nominate
pub(crate) const TEST_VALIDATORS: [AccountId; 3] = [1, 2, 3];

pub fn new_test_ext() -> sp_io::TestExternalities {
	sp_tracing::try_init_simple();
	let mut storage = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
	let _ = pallet_nomination_pools::GenesisConfig::<Runtime> {
		min_join_bond: 2,
		min_create_bond: 2,
		max_pools: Some(3),
		max_members_per_pool: Some(5),
		max_members: Some(3 * 5),
		global_max_commission: Some(Perbill::from_percent(90)),
	}
	.assimilate_storage(&mut storage)
	.unwrap();

	// Fund the DAP buffer account with ED so it can receive slashes.
	use frame_support::sp_runtime::traits::AccountIdConversion;
	let dap_buffer: AccountId = DapPalletId::get().into_account_truncating();

	let _ = pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![(10, 100), (20, 100), (21, 100), (22, 100), (dap_buffer, 5)]
			.into_iter()
			.chain(TEST_VALIDATORS.iter().map(|&v| (v, 1000)))
			.collect::<Vec<_>>(),
		..Default::default()
	}
	.assimilate_storage(&mut storage)
	.unwrap();

	let mut ext = sp_io::TestExternalities::from(storage);

	ext.execute_with(|| {
		// for events to be deposited.
		frame_system::Pallet::<Runtime>::set_block_number(1);

		// Initialize era state for pallet-staking-async
		pallet_staking_async::CurrentEra::<Runtime>::put(0);
		pallet_staking_async::ActiveEra::<Runtime>::put(pallet_staking_async::ActiveEraInfo {
			index: 0,
			start: None,
		});

		// set some limit for nominations.
		assert_ok!(Staking::set_staking_configs(
			RuntimeOrigin::root(),
			pallet_staking_async::ConfigOp::Set(10), // minimum nominator bond
			pallet_staking_async::ConfigOp::Noop,
			pallet_staking_async::ConfigOp::Noop,
			pallet_staking_async::ConfigOp::Noop,
			pallet_staking_async::ConfigOp::Noop,
			pallet_staking_async::ConfigOp::Noop,
			pallet_staking_async::ConfigOp::Noop,
			pallet_staking_async::ConfigOp::Noop, // are_nominators_slashable
		));

		// Set up validators that tests can nominate
		for &validator in TEST_VALIDATORS.iter() {
			assert_ok!(Staking::bond(
				RuntimeOrigin::signed(validator),
				500,
				pallet_staking_async::RewardDestination::Staked
			));
			assert_ok!(Staking::validate(
				RuntimeOrigin::signed(validator),
				pallet_staking_async::ValidatorPrefs::default()
			));
		}

		// Clear events from setup to avoid test interference
		frame_system::Pallet::<Runtime>::reset_events();
	});

	ext
}

parameter_types! {
	static ObservedEventsPools: usize = 0;
	static ObservedEventsStaking: usize = 0;
	static ObservedEventsBalances: usize = 0;
	static ObservedEventsDelegatedStaking: usize = 0;
}

pub(crate) fn pool_events_since_last_call() -> Vec<pallet_nomination_pools::Event<Runtime>> {
	let events = System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(|e| if let RuntimeEvent::Pools(inner) = e { Some(inner) } else { None })
		.collect::<Vec<_>>();
	let already_seen = ObservedEventsPools::get();
	ObservedEventsPools::set(events.len());
	events.into_iter().skip(already_seen).collect()
}

pub(crate) fn staking_events_since_last_call() -> Vec<pallet_staking_async::Event<Runtime>> {
	let events = System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(|e| if let RuntimeEvent::Staking(inner) = e { Some(inner) } else { None })
		.collect::<Vec<_>>();
	let already_seen = ObservedEventsStaking::get();
	ObservedEventsStaking::set(events.len());
	events.into_iter().skip(already_seen).collect()
}

pub(crate) fn delegated_staking_events_since_last_call(
) -> Vec<pallet_delegated_staking::Event<Runtime>> {
	let events = System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(
			|e| if let RuntimeEvent::DelegatedStaking(inner) = e { Some(inner) } else { None },
		)
		.collect::<Vec<_>>();
	let already_seen = ObservedEventsDelegatedStaking::get();
	ObservedEventsDelegatedStaking::set(events.len());
	events.into_iter().skip(already_seen).collect()
}

/// Set up DAP in transfer-based (non-minting) mode with a given budget split.
/// This enables DAP-based era rewards instead of legacy minting.
pub(crate) fn setup_dap_with_budget(staker_pct: u32, incentive_pct: u32, buffer_pct: u32) {
	use pallet_staking_async::{
		RewardKind, RewardPot, SequentialTest, StakerRewardRecipient, ValidatorIncentiveRecipient,
	};
	use frame_support::traits::fungible::Mutate as FungibleMutate;

	assert_eq!(staker_pct + incentive_pct + buffer_pct, 100);

	let staker_key =
		<StakerRewardRecipient<SequentialTest> as BudgetRecipient<AccountId>>::budget_key();
	let incentive_key =
		<ValidatorIncentiveRecipient<SequentialTest> as BudgetRecipient<AccountId>>::budget_key();
	let buffer_key =
		<pallet_dap::Pallet<Runtime> as BudgetRecipient<AccountId>>::budget_key();

	let mut budget = pallet_dap::BudgetAllocationMap::new();
	budget.try_insert(staker_key, Perbill::from_percent(staker_pct)).unwrap();
	budget.try_insert(incentive_key, Perbill::from_percent(incentive_pct)).unwrap();
	budget.try_insert(buffer_key, Perbill::from_percent(buffer_pct)).unwrap();
	pallet_dap::BudgetAllocation::<Runtime>::put(budget);

	// Seed timestamp so DAP doesn't skip first drip.
	pallet_dap::LastIssuanceTimestamp::<Runtime>::put(
		<Timestamp as frame_support::traits::Time>::now()
	);

	// Fund general pots with ED.
	let general_staker =
		SequentialTest::pot_account(RewardPot::General(RewardKind::StakerRewards));
	let general_incentive =
		SequentialTest::pot_account(RewardPot::General(RewardKind::ValidatorSelfStake));
	Balances::mint_into(&general_staker, ExistentialDeposit::get()).unwrap();
	Balances::mint_into(&general_incentive, ExistentialDeposit::get()).unwrap();
}

/// Fund an era's staker reward pot with a specific amount, simulating DAP snapshot.
/// This is a manual shortcut for tests that need a funded era pot without running
/// the full DAP drip + era rotation cycle.
pub(crate) fn fund_era_staker_pot(era: u32, amount: Balance) {
	use pallet_staking_async::{RewardKind, RewardPot, SequentialTest, PotAccountProvider};
	use frame_support::traits::fungible::Mutate as FungibleMutate;

	let pot = SequentialTest::pot_account(RewardPot::Era(era, RewardKind::StakerRewards));
	// Create the pot (add provider reference).
	frame_system::Pallet::<Runtime>::inc_providers(&pot);
	// Fund it.
	Balances::mint_into(&pot, amount).unwrap();
	// Record the era reward.
	pallet_staking_async::ErasValidatorReward::<Runtime>::insert(era, amount);
}
