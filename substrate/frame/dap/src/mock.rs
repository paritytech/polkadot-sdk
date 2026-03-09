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

//! Test mock for the DAP pallet.

use crate::{self as pallet_dap, Config};
use frame_support::{
	derive_impl, parameter_types, sp_runtime::traits::AccountIdConversion, PalletId,
};
use sp_runtime::{traits::IdentityLookup, BuildStorage};

type Block = frame_system::mocking::MockBlock<Test>;
pub type AccountId = u128;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		Dap: pallet_dap,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type AccountData = pallet_balances::AccountData<u64>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
	type ExistentialDeposit = ExistentialDeposit;
}

parameter_types! {
	pub const DapPalletId: PalletId = PalletId(*b"dap/buff");
	pub const ExistentialDeposit: u64 = 10;
	pub const InflationCadence: u64 = 60_000; // 60 seconds
	pub const MaxElapsedPerDrip: u64 = 600_000; // 10 minutes
}

/// Returns 100 per 60_000ms elapsed (proportional).
pub struct TestInflationCurve;
impl sp_staking::InflationCurve<u64> for TestInflationCurve {
	fn inflation(_total_issuance: u64, elapsed_millis: u64) -> u64 {
		// 100 per minute (60_000ms)
		(100u128 * elapsed_millis as u128 / 60_000u128) as u64
	}
}

/// Mock time provider backed by storage.
pub struct MockTime;
impl MockTime {
	pub fn set(millis: u64) {
		MOCK_TIME.with(|v| *v.borrow_mut() = millis);
	}

	pub fn get() -> u64 {
		MOCK_TIME.with(|v| *v.borrow())
	}
}

std::thread_local! {
	static MOCK_TIME: core::cell::RefCell<u64> = core::cell::RefCell::new(0);
}

impl frame_support::traits::Time for MockTime {
	type Moment = u64;
	fn now() -> u64 {
		Self::get()
	}
}

/// Test budget recipient: staker rewards pot (account 500).
pub struct TestStakerRecipient;
impl sp_staking::BudgetRecipient<AccountId> for TestStakerRecipient {
	fn budget_key() -> sp_staking::BudgetKey {
		sp_staking::BudgetKey::truncate_from(b"staker_rewards".to_vec())
	}
	fn pot_account() -> AccountId {
		500
	}
}

/// Test budget recipient: validator incentive pot (account 501).
pub struct TestValidatorIncentiveRecipient;
impl sp_staking::BudgetRecipient<AccountId> for TestValidatorIncentiveRecipient {
	fn budget_key() -> sp_staking::BudgetKey {
		sp_staking::BudgetKey::truncate_from(b"validator_incentive".to_vec())
	}
	fn pot_account() -> AccountId {
		501
	}
}

impl Config for Test {
	type Currency = Balances;
	type PalletId = DapPalletId;
	type InflationCurve = TestInflationCurve;
	type BudgetRecipients = (Dap, TestStakerRecipient, TestValidatorIncentiveRecipient);
	type Time = MockTime;
	type InflationCadence = InflationCadence;
	type MaxElapsedPerDrip = MaxElapsedPerDrip;
	type BudgetOrigin = frame_system::EnsureRoot<AccountId>;
}

pub fn new_test_ext(fund_buffer: bool) -> sp_io::TestExternalities {
	let mut balances = vec![(1, 100), (2, 200), (3, 300)];

	if fund_buffer {
		let buffer: AccountId = DapPalletId::get().into_account_truncating();
		balances.push((buffer, ExistentialDeposit::get()));
	}

	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> { balances, ..Default::default() }
		.assimilate_storage(&mut t)
		.unwrap();
	let mut ext: sp_io::TestExternalities = t.into();

	ext.execute_with(|| {
		// Initialize time to simulate "genesis already happened".
		MockTime::set(1_000_000);
		// Initialize LastInflationTimestamp so drip doesn't skip first call.
		crate::LastInflationTimestamp::<Test>::put(1_000_000);
	});

	ext
}
