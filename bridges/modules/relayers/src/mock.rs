// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

#![cfg(test)]

use crate as pallet_bridge_relayers;

use bp_messages::LaneId;
use bp_relayers::{
	PayRewardFromAccount, PaymentProcedure, RewardsAccountOwner, RewardsAccountParams,
};
use frame_support::{
	derive_impl, parameter_types, traits::fungible::Mutate, weights::RuntimeDbWeight,
};
use sp_runtime::BuildStorage;

pub type AccountId = u64;
pub type Balance = u64;
pub type BlockNumber = u64;

pub type TestStakeAndSlash = pallet_bridge_relayers::StakeAndSlashNamed<
	AccountId,
	BlockNumber,
	Balances,
	ReserveId,
	Stake,
	Lease,
>;

type Block = frame_system::mocking::MockBlock<TestRuntime>;

frame_support::construct_runtime! {
	pub enum TestRuntime
	{
		System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
		Balances: pallet_balances::{Pallet, Event<T>},
		Relayers: pallet_bridge_relayers::{Pallet, Call, Event<T>},
	}
}

parameter_types! {
	pub const DbWeight: RuntimeDbWeight = RuntimeDbWeight { read: 1, write: 2 };
	pub const ExistentialDeposit: Balance = 1;
	pub const ReserveId: [u8; 8] = *b"brdgrlrs";
	pub const Stake: Balance = 1_000;
	pub const Lease: BlockNumber = 8;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for TestRuntime {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
	type DbWeight = DbWeight;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig as pallet_balances::DefaultConfig)]
impl pallet_balances::Config for TestRuntime {
	type ReserveIdentifier = [u8; 8];
	type AccountStore = System;
}

impl pallet_bridge_relayers::Config for TestRuntime {
	type RuntimeEvent = RuntimeEvent;
	type Reward = Balance;
	type PaymentProcedure = TestPaymentProcedure;
	type StakeAndSlash = TestStakeAndSlash;
	type WeightInfo = ();
}

#[cfg(feature = "runtime-benchmarks")]
impl pallet_bridge_relayers::benchmarking::Config for TestRuntime {
	fn prepare_rewards_account(account_params: RewardsAccountParams, reward: Balance) {
		let rewards_account =
			bp_relayers::PayRewardFromAccount::<Balances, AccountId>::rewards_account(
				account_params,
			);
		Self::deposit_account(rewards_account, reward);
	}

	fn deposit_account(account: Self::AccountId, balance: Self::Reward) {
		Balances::mint_into(&account, balance.saturating_add(ExistentialDeposit::get())).unwrap();
	}
}

/// Message lane that we're using in tests.
pub const TEST_REWARDS_ACCOUNT_PARAMS: RewardsAccountParams =
	RewardsAccountParams::new(LaneId([0, 0, 0, 0]), *b"test", RewardsAccountOwner::ThisChain);

/// Regular relayer that may receive rewards.
pub const REGULAR_RELAYER: AccountId = 1;

/// Relayer that can't receive rewards.
pub const FAILING_RELAYER: AccountId = 2;

/// Relayer that is able to register.
pub const REGISTER_RELAYER: AccountId = 42;

/// Payment procedure that rejects payments to the `FAILING_RELAYER`.
pub struct TestPaymentProcedure;

impl TestPaymentProcedure {
	pub fn rewards_account(params: RewardsAccountParams) -> AccountId {
		PayRewardFromAccount::<(), AccountId>::rewards_account(params)
	}
}

impl PaymentProcedure<AccountId, Balance> for TestPaymentProcedure {
	type Error = ();

	fn pay_reward(
		relayer: &AccountId,
		_lane_id: RewardsAccountParams,
		_reward: Balance,
	) -> Result<(), Self::Error> {
		match *relayer {
			FAILING_RELAYER => Err(()),
			_ => Ok(()),
		}
	}
}

/// Return test externalities to use in tests.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<TestRuntime>::default().build_storage().unwrap();
	sp_io::TestExternalities::new(t)
}

/// Run pallet test.
pub fn run_test<T>(test: impl FnOnce() -> T) -> T {
	new_test_ext().execute_with(|| {
		Balances::mint_into(&REGISTER_RELAYER, ExistentialDeposit::get() + 10 * Stake::get())
			.unwrap();

		test()
	})
}
