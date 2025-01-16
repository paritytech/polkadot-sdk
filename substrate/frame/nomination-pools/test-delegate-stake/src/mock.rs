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

use frame_election_provider_support::VoteWeight;
use frame_support::{
	assert_ok, derive_impl,
	pallet_prelude::*,
	parameter_types,
	traits::{ConstU64, ConstU8, VariantCountOf},
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
}

#[derive_impl(pallet_staking::config_preludes::TestDefaultConfig)]
impl pallet_staking::Config for Runtime {
	type Currency = Balances;
	type UnixTime = pallet_timestamp::Pallet<Self>;
	type AdminOrigin = frame_system::EnsureRoot<Self::AccountId>;
	type BondingDuration = BondingDuration;
	type EraPayout = pallet_staking::ConvertCurve<RewardCurve>;
	type ElectionProvider =
		frame_election_provider_support::NoElection<(AccountId, BlockNumber, Staking, (), ())>;
	type GenesisElectionProvider = Self::ElectionProvider;
	type VoterList = VoterList;
	type TargetList = pallet_staking::UseValidatorsMap<Self>;
	type EventListeners = (Pools, DelegatedStaking);
	type BenchmarkingConfig = pallet_staking::TestBenchmarkingConfig;
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
			return TransferStake::strategy_type()
		}
		DelegateStake::strategy_type()
	}
	fn transferable_balance(
		pool_account: Pool<Self::AccountId>,
		member_account: Member<Self::AccountId>,
	) -> Self::Balance {
		if LegacyAdapter::get() {
			return TransferStake::transferable_balance(pool_account, member_account)
		}
		DelegateStake::transferable_balance(pool_account, member_account)
	}

	fn total_balance(pool_account: Pool<Self::AccountId>) -> Option<Self::Balance> {
		if LegacyAdapter::get() {
			return TransferStake::total_balance(pool_account)
		}
		DelegateStake::total_balance(pool_account)
	}

	fn member_delegation_balance(member_account: Member<Self::AccountId>) -> Option<Self::Balance> {
		if LegacyAdapter::get() {
			return TransferStake::member_delegation_balance(member_account)
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
			return TransferStake::pledge_bond(who, pool_account, reward_account, amount, bond_type)
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
			return TransferStake::member_withdraw(who, pool_account, amount, num_slashing_spans)
		}
		DelegateStake::member_withdraw(who, pool_account, amount, num_slashing_spans)
	}

	fn dissolve(pool_account: Pool<Self::AccountId>) -> DispatchResult {
		if LegacyAdapter::get() {
			return TransferStake::dissolve(pool_account)
		}
		DelegateStake::dissolve(pool_account)
	}

	fn pending_slash(pool_account: Pool<Self::AccountId>) -> Self::Balance {
		if LegacyAdapter::get() {
			return TransferStake::pending_slash(pool_account)
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
			return TransferStake::member_slash(who, pool_account, amount, maybe_reporter)
		}
		DelegateStake::member_slash(who, pool_account, amount, maybe_reporter)
	}

	fn migrate_nominator_to_agent(
		agent: Pool<Self::AccountId>,
		reward_account: &Self::AccountId,
	) -> DispatchResult {
		if LegacyAdapter::get() {
			return TransferStake::migrate_nominator_to_agent(agent, reward_account)
		}
		DelegateStake::migrate_nominator_to_agent(agent, reward_account)
	}

	fn migrate_delegation(
		agent: Pool<Self::AccountId>,
		delegator: Member<Self::AccountId>,
		value: Self::Balance,
	) -> DispatchResult {
		if LegacyAdapter::get() {
			return TransferStake::migrate_delegation(agent, delegator, value)
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
type Block = frame_system::mocking::MockBlock<Runtime>;

frame_support::construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		Timestamp: pallet_timestamp,
		Balances: pallet_balances,
		Staking: pallet_staking,
		VoterList: pallet_bags_list::<Instance1>,
		Pools: pallet_nomination_pools,
		DelegatedStaking: pallet_delegated_staking,
	}
);

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

	let _ = pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![(10, 100), (20, 100), (21, 100), (22, 100)],
	}
	.assimilate_storage(&mut storage)
	.unwrap();

	let mut ext = sp_io::TestExternalities::from(storage);

	ext.execute_with(|| {
		// for events to be deposited.
		frame_system::Pallet::<Runtime>::set_block_number(1);

		// set some limit for nominations.
		assert_ok!(Staking::set_staking_configs(
			RuntimeOrigin::root(),
			pallet_staking::ConfigOp::Set(10), // minimum nominator bond
			pallet_staking::ConfigOp::Noop,
			pallet_staking::ConfigOp::Noop,
			pallet_staking::ConfigOp::Noop,
			pallet_staking::ConfigOp::Noop,
			pallet_staking::ConfigOp::Noop,
			pallet_staking::ConfigOp::Noop,
		));
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

pub(crate) fn staking_events_since_last_call() -> Vec<pallet_staking::Event<Runtime>> {
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
