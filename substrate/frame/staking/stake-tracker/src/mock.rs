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

#![cfg(test)]

use crate::{self as pallet_stake_tracker, *};

use frame_election_provider_support::{ScoreProvider, VoteWeight};
use frame_support::{derive_impl, parameter_types, traits::ConstU32};
use sp_runtime::{BuildStorage, DispatchResult, Perbill};
use sp_staking::{Stake, StakingInterface};

pub(crate) type AccountId = u64;
pub(crate) type Balance = u64;

type Block = frame_system::mocking::MockBlockU32<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,
		VoterBagsList: pallet_bags_list::<Instance1>,
		TargetBagsList: pallet_bags_list::<Instance2>,
		StakeTracker: pallet_stake_tracker,
	}
);

parameter_types! {
	pub static ExistentialDeposit: Balance = 1;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type BlockHashCount = ConstU32<10>;

	type AccountData = pallet_balances::AccountData<Balance>;
}

impl pallet_balances::Config for Test {
	type Balance = Balance;
	type DustRemoval = ();
	type RuntimeEvent = RuntimeEvent;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type WeightInfo = ();
	type MaxLocks = frame_support::traits::ConstU32<1024>;
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type FreezeIdentifier = ();
	type RuntimeHoldReason = ();
	type RuntimeFreezeReason = ();
	type MaxFreezes = ();
}

const VOTER_THRESHOLDS: [sp_npos_elections::VoteWeight; 9] =
	[100, 200, 300, 400, 500, 600, 700, 800, 900];

const TARGET_THRESHOLDS: [u128; 9] = [100, 200, 300, 400, 500, 600, 700, 800, 900];

parameter_types! {
	pub static VoterBagThresholds: &'static [VoteWeight] = &VOTER_THRESHOLDS;
	pub static TargetBagThresholds: &'static [u128] = &TARGET_THRESHOLDS;

	pub static VoterUpdateMode: crate::VoterUpdateMode = crate::VoterUpdateMode::Strict;
}

type VoterBagsListInstance = pallet_bags_list::Instance1;
impl pallet_bags_list::Config<VoterBagsListInstance> for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type ScoreProvider = StakingMock;
	type BagThresholds = VoterBagThresholds;
	type Score = VoteWeight;
}

type TargetBagsListInstance = pallet_bags_list::Instance2;
impl pallet_bags_list::Config<TargetBagsListInstance> for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type ScoreProvider = pallet_bags_list::Pallet<Test, TargetBagsListInstance>;
	type BagThresholds = TargetBagThresholds;
	type Score = u128;
}

impl pallet_stake_tracker::Config for Test {
	type Currency = Balances;
	type Staking = StakingMock;
	type VoterList = VoterBagsList;
	type TargetList = TargetBagsList;
	type VoterUpdateMode = VoterUpdateMode;
}

pub struct StakingMock {}

impl ScoreProvider<AccountId> for StakingMock {
	type Score = VoteWeight;

	fn score(id: &AccountId) -> Self::Score {
		let nominators = TestNominators::get();
		nominators.get(id).unwrap().0.active
	}

	fn set_score_of(_: &AccountId, _: Self::Score) {
		unreachable!();
	}
}

impl StakingInterface for StakingMock {
	type Balance = Balance;
	type AccountId = AccountId;
	type CurrencyToVote = ();

	fn stake(who: &Self::AccountId) -> Result<Stake<Self::Balance>, sp_runtime::DispatchError> {
		let n = TestNominators::get();
		match n.get(who) {
			Some(nominator) => Some(nominator.0),
			None => {
				let v = TestValidators::get();
				v.get(who).copied()
			},
		}
		.ok_or("not a staker".into())
	}

	fn status(
		who: &Self::AccountId,
	) -> Result<sp_staking::StakerStatus<Self::AccountId>, sp_runtime::DispatchError> {
		let nominators = TestNominators::get();

		match (
			TestValidators::get().contains_key(who),
			nominators.contains_key(who),
			Bonded::get().contains(who),
		) {
			(true, true, true) => Ok(StakerStatus::Validator),
			(false, true, true) =>
				Ok(StakerStatus::Nominator(nominators.get(who).expect("exists").1.clone())),
			(false, false, true) => Ok(StakerStatus::Idle),
			(false, false, false) =>
				if TargetBagsList::contains(who) {
					Err("dangling".into())
				} else {
					Err("not a staker".into())
				},
			_ => Err("bad state".into()),
		}
	}

	fn nominations(who: &Self::AccountId) -> Option<Vec<Self::AccountId>> {
		let n = TestNominators::get();
		n.get(who).map(|nominator| nominator.1.clone())
	}

	fn minimum_nominator_bond() -> Self::Balance {
		unreachable!();
	}

	fn minimum_validator_bond() -> Self::Balance {
		unreachable!();
	}

	fn stash_by_ctrl(
		_controller: &Self::AccountId,
	) -> Result<Self::AccountId, sp_runtime::DispatchError> {
		unreachable!();
	}

	fn bonding_duration() -> sp_staking::EraIndex {
		unreachable!();
	}

	fn current_era() -> sp_staking::EraIndex {
		unreachable!();
	}

	fn bond(
		_who: &Self::AccountId,
		_value: Self::Balance,
		_payee: &Self::AccountId,
	) -> sp_runtime::DispatchResult {
		unreachable!();
	}

	fn nominate(
		who: &Self::AccountId,
		validators: Vec<Self::AccountId>,
	) -> sp_runtime::DispatchResult {
		update_nominations_of(*who, validators);

		Ok(())
	}

	fn chill(_who: &Self::AccountId) -> sp_runtime::DispatchResult {
		unreachable!();
	}

	fn bond_extra(_who: &Self::AccountId, _extra: Self::Balance) -> sp_runtime::DispatchResult {
		unreachable!();
	}

	fn withdraw_unbonded(
		_stash: Self::AccountId,
		_num_slashing_spans: u32,
	) -> Result<bool, sp_runtime::DispatchError> {
		unreachable!();
	}

	fn desired_validator_count() -> u32 {
		unreachable!();
	}

	fn election_ongoing() -> bool {
		unreachable!();
	}

	fn force_unstake(_who: Self::AccountId) -> sp_runtime::DispatchResult {
		unreachable!();
	}

	fn is_exposed_in_era(_who: &Self::AccountId, _era: &sp_staking::EraIndex) -> bool {
		unreachable!();
	}

	fn unbond(_stash: &Self::AccountId, _value: Self::Balance) -> sp_runtime::DispatchResult {
		unreachable!();
	}

	fn update_payee(_stash: &Self::AccountId, _reward_acc: &Self::AccountId) -> DispatchResult {
		unreachable!();
	}

	fn slash_reward_fraction() -> Perbill {
		unreachable!();
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn add_era_stakers(
		_current_era: &sp_staking::EraIndex,
		_stash: &Self::AccountId,
		_exposures: Vec<(Self::AccountId, Self::Balance)>,
	) {
		unimplemented!("method currently not used in testing")
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_current_era(_era: sp_staking::EraIndex) {
		unimplemented!("method currently not used in testing")
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn max_exposure_page_size() -> sp_staking::Page {
		unimplemented!("method currently not used in testing")
	}
}

type Nominations = Vec<AccountId>;

parameter_types! {
	pub static TestNominators: BTreeMap<AccountId, (Stake<Balance>, Nominations)> = Default::default();
	pub static TestValidators: BTreeMap<AccountId, Stake<Balance>> = Default::default();
	pub static Bonded: Vec<AccountId> = Default::default();
}

pub(crate) fn target_scores() -> Vec<(AccountId, u128)> {
	TargetBagsList::iter()
		.map(|e| (e, TargetBagsList::get_score(&e).unwrap()))
		.collect::<Vec<_>>()
}

pub(crate) fn voter_scores() -> Vec<(AccountId, Balance)> {
	VoterBagsList::iter()
		.map(|e| (e, VoterBagsList::get_score(&e).unwrap()))
		.collect::<Vec<_>>()
}

pub(crate) fn populate_lists() {
	add_validator(10, 100);
	add_validator(11, 100);

	add_nominator_with_nominations(1, 100, vec![10]);
	add_nominator_with_nominations(2, 100, vec![10, 11]);
}

pub(crate) fn add_nominator(who: AccountId, stake: Balance) {
	Bonded::mutate(|b| {
		b.push(who);
	});

	TestNominators::mutate(|n| {
		n.insert(who, (Stake::<Balance> { active: stake, total: stake }, vec![]));
	});

	// add new nominator (called at `fn bond` in staking)
	<StakeTracker as OnStakingUpdate<AccountId, Balance>>::on_nominator_add(&who, vec![]);
}

pub(crate) fn stake_of(who: AccountId) -> Option<Stake<Balance>> {
	StakingMock::stake(&who).ok()
}

pub(crate) fn score_of_target(who: AccountId) -> Balance {
	<pallet_bags_list::Pallet<Test, TargetBagsListInstance> as ScoreProvider<AccountId>>::score(
		&who,
	)
	.try_into()
	.unwrap()
}

pub(crate) fn add_nominator_with_nominations(
	who: AccountId,
	stake: Balance,
	nominations: Nominations,
) {
	// add new nominator (called at `fn bond` in staking)
	add_nominator(who, stake);

	TestNominators::mutate(|n| {
		n.insert(who, (Stake::<Balance> { active: stake, total: stake }, nominations.clone()));
	});

	<StakeTracker as OnStakingUpdate<AccountId, Balance>>::on_nominator_update(
		&who,
		vec![],
		nominations,
	);
}

pub(crate) fn update_nominations_of(who: AccountId, new_nominations: Nominations) {
	// add nominations (called at `fn nominate` in staking)
	let current_nom = TestNominators::get();
	let (current_stake, prev_nominations) = current_nom.get(&who).unwrap();

	TestNominators::mutate(|n| {
		n.insert(who, (*current_stake, new_nominations.clone()));
	});

	<StakeTracker as OnStakingUpdate<AccountId, Balance>>::on_nominator_update(
		&who,
		prev_nominations.clone(),
		new_nominations,
	);
}

pub(crate) fn add_validator(who: AccountId, self_stake: Balance) {
	Bonded::mutate(|b| {
		b.push(who);
	});

	let stake = Stake { active: self_stake, total: self_stake };

	TestValidators::mutate(|v| {
		v.insert(who, stake);
	});
	// validator is a nominator too.
	TestNominators::mutate(|v| {
		v.insert(who, (stake, vec![]));
	});

	<StakeTracker as OnStakingUpdate<AccountId, Balance>>::on_validator_add(&who, Some(stake));
}

pub(crate) fn update_stake(who: AccountId, new: Balance, prev_stake: Option<Stake<Balance>>) {
	match StakingMock::status(&who) {
		Ok(StakerStatus::Nominator(nominations)) => {
			TestNominators::mutate(|n| {
				n.insert(who, (Stake { active: new, total: new }, nominations));
			});
		},
		Ok(StakerStatus::Validator) => {
			TestValidators::mutate(|v| {
				v.insert(who, Stake { active: new, total: new });
			});
			TestNominators::mutate(|n| {
				let nominations = n.get(&who).expect("exists").1.clone();
				n.insert(who, (Stake { active: new, total: new }, nominations));
			})
		},
		Ok(StakerStatus::Idle) | Err(_) => panic!("not a staker"),
	}

	<StakeTracker as OnStakingUpdate<AccountId, Balance>>::on_stake_update(
		&who,
		prev_stake,
		Stake { total: new, active: new },
	);
}

pub(crate) fn chill_staker(who: AccountId) {
	if TestNominators::get().contains_key(&who) && !TestValidators::get().contains_key(&who) {
		let nominations = <StakingMock as StakingInterface>::nominations(&who).unwrap();

		<StakeTracker as OnStakingUpdate<AccountId, Balance>>::on_nominator_idle(&who, nominations);
		TestNominators::mutate(|n| n.remove(&who));
	} else if TestValidators::get().contains_key(&who) {
		<StakeTracker as OnStakingUpdate<AccountId, Balance>>::on_validator_idle(&who);
		TestValidators::mutate(|v| v.remove(&who));
		TestNominators::mutate(|v| v.remove(&who));
	};
}

pub(crate) fn remove_staker(who: AccountId) {
	match StakingMock::status(&who) {
		Ok(StakerStatus::Nominator(_)) => {
			let nominations = <StakingMock as StakingInterface>::nominations(&who).unwrap();
			<StakeTracker as OnStakingUpdate<AccountId, Balance>>::on_nominator_remove(
				&who,
				nominations,
			);
			TestNominators::mutate(|n| {
				n.remove(&who);
			});
		},
		Ok(StakerStatus::Validator) => {
			<StakeTracker as OnStakingUpdate<AccountId, Balance>>::on_validator_remove(&who);
			TestValidators::mutate(|v| v.remove(&who));
		},
		Ok(StakerStatus::Idle) =>
			if TargetBagsList::contains(&who) {
				<StakeTracker as OnStakingUpdate<AccountId, Balance>>::on_validator_remove(&who);
			},
		_ => {},
	}

	Bonded::mutate(|b| {
		b.retain(|s| s != &who);
	});
}

pub(crate) fn target_bags_events() -> Vec<pallet_bags_list::Event<Test, TargetBagsListInstance>> {
	System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(
			|e| if let RuntimeEvent::TargetBagsList(inner) = e { Some(inner) } else { None },
		)
		.collect::<Vec<_>>()
}

pub(crate) fn voter_bags_events() -> Vec<pallet_bags_list::Event<Test, VoterBagsListInstance>> {
	System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(|e| if let RuntimeEvent::VoterBagsList(inner) = e { Some(inner) } else { None })
		.collect::<Vec<_>>()
}

#[derive(Default, Copy, Clone)]
pub struct ExtBuilder {
	populate_lists: bool,
}

impl ExtBuilder {
	pub fn populate_lists(mut self) -> Self {
		self.populate_lists = true;
		self
	}

	pub fn voter_update_mode(self, mode: crate::VoterUpdateMode) -> Self {
		VoterUpdateMode::set(mode);
		self
	}

	pub fn build(self) -> sp_io::TestExternalities {
		sp_tracing::try_init_simple();
		let storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

		sp_io::TestExternalities::from(storage)
	}

	pub fn build_and_execute(self, test: impl FnOnce()) {
		sp_tracing::try_init_simple();

		let mut ext = self.build();
		ext.execute_with(|| {
			if self.populate_lists {
				populate_lists();
			}
			// move past genesis to register events.
			System::set_block_number(1);
		});
		ext.execute_with(test);
	}
}
