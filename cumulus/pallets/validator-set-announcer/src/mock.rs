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

use super::*;
use crate as pallet_validator_set_announcer;
use frame_support::{
	derive_impl, parameter_types,
	traits::{ConstU32, ConstU64},
};
use frame_system::EnsureRoot;
use sp_runtime::BuildStorage;

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,
		Session: pallet_session,
		ValidatorCollators: pallet_validator_collators,
		ValidatorSetAnnouncer: pallet_validator_set_announcer,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u64>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
}

impl pallet_session::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type ValidatorId = u64;
	type ValidatorIdOf = sp_runtime::traits::ConvertInto;
	type ShouldEndSession = ValidatorCollators;
	type NextSessionRotation = pallet_session::PeriodicSessions<Period, Offset>;
	type SessionManager = ValidatorCollators;
	type SessionHandler = pallet_session::TestSessionHandler;
	type Keys = sp_runtime::testing::UintAuthorityId;
	type DisablingStrategy = ();
	type WeightInfo = ();
	type Currency = Balances;
	type KeyDeposit = ();
}

/// Every account counts as having registered session keys.
pub struct AllRegistered;
impl frame_support::traits::ValidatorRegistration<u64> for AllRegistered {
	fn is_registered(_: &u64) -> bool {
		true
	}
}

parameter_types! {
	pub const Period: u64 = 10;
	pub const Offset: u64 = 0;
}

impl pallet_validator_collators::Config for Test {
	type SetOrigin = EnsureRoot<u64>;
	type UpdateOrigin = EnsureRoot<u64>;
	type ValidatorRegistration = AllRegistered;
	type MaxValidators = ConstU32<50>;
	type PeriodicSession = pallet_session::PeriodicSessions<Period, Offset>;
	type BlockReporter = ();
	type DefaultReportPeriod = Period;
	type MaxEntriesPerReport = ConstU32<4>;
	type UnixTime = ZeroTime;
	type DefaultExpectedEraDuration = ConstU64<1000>;
	type DefaultReportMargin = ConstU64<100>;
	type MaxReportMargin = ConstU64<500>;
	type MaxTrackedAuthorities = ConstU32<1>;
	type WeightInfo = ();
}

/// A clock that always reads zero.
pub struct ZeroTime;
impl frame_support::traits::UnixTime for ZeroTime {
	fn now() -> core::time::Duration {
		core::time::Duration::ZERO
	}
}

impl Config for Test {
	type Sender = MockSender;
	type Destinations = Destinations;
	type WeightInfo = ();
}

parameter_types! {
	pub static Destinations: Vec<u32> = vec![1, 2];
	pub static FailingDestinations: Vec<u32> = Vec::new();
	pub static Sent: Vec<(u32, EraIndex, Vec<u64>)> = Vec::new();
}

pub struct MockSender;
impl SendValidatorSet<u64> for MockSender {
	type Destination = u32;

	fn send(destination: &u32, era: EraIndex, validators: &[u64]) -> Result<(), ()> {
		if FailingDestinations::get().contains(destination) {
			return Err(());
		}
		Sent::mutate(|sent| sent.push((*destination, era, validators.to_vec())));
		Ok(())
	}
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext: sp_io::TestExternalities =
		frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into();
	ext.execute_with(|| System::set_block_number(1));
	ext
}
