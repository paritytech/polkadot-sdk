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

use crate::*;
use fp_coretime::market::{CoreRangeProvider, SoldCoresRange, TimesliceProvider};
use frame_support::{derive_impl, traits::Randomness};
use sp_core::ConstU32;
use sp_runtime::BuildStorage;

type Block = frame_system::mocking::MockBlock<Test>;

/// Test randomness derived from parent hash and subject.
pub struct TestRandomness;
impl Randomness<sp_core::H256, u64> for TestRandomness {
	fn random(subject: &[u8]) -> (sp_core::H256, u64) {
		let parent_hash = frame_system::Pallet::<Test>::parent_hash();
		let mut seed = parent_hash.as_ref().to_vec();
		seed.extend_from_slice(subject);
		(sp_core::H256::from(sp_io::hashing::blake2_256(&seed)), 0)
	}
}

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		CoretimeMarket: crate,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

/// Mock core range provider.
pub struct TestCoreRangeProvider;

thread_local! {
	static CORE_RANGE: core::cell::RefCell<Option<SoldCoresRange>> =
		core::cell::RefCell::new(None);
}

impl TestCoreRangeProvider {
	pub fn set(from: CoreIndex, to: CoreIndex) {
		CORE_RANGE.with(|r| {
			*r.borrow_mut() = Some(SoldCoresRange { from, to });
		});
	}

	pub fn clear() {
		CORE_RANGE.with(|r| *r.borrow_mut() = None);
	}
}

impl CoreRangeProvider for TestCoreRangeProvider {
	fn core_range() -> Option<SoldCoresRange> {
		CORE_RANGE.with(|r| {
			r.borrow()
				.as_ref()
				.map(|range| SoldCoresRange { from: range.from, to: range.to })
		})
	}
}

/// Mock timeslice provider.
pub struct TestTimesliceProvider;

thread_local! {
	static LATEST_TS_READY: core::cell::Cell<Option<Timeslice>> = const { core::cell::Cell::new(None) };
}

impl TestTimesliceProvider {
	pub fn set_latest_ready(ts: Timeslice) {
		LATEST_TS_READY.with(|c| c.set(Some(ts)));
	}
}

impl TimesliceProvider for TestTimesliceProvider {
	fn next_timeslice_to_commit() -> Option<Timeslice> {
		LATEST_TS_READY.with(|c| c.get().map(|ts| ts.saturating_add(1)))
	}

	fn latest_timeslice_ready_to_commit() -> Option<Timeslice> {
		Self::next_timeslice_to_commit()
	}
}

/// Mock renewal rights provider.
pub struct TestRenewalRights;

thread_local! {
	static RENEWAL_RIGHTS: core::cell::RefCell<alloc::collections::BTreeMap<(u64, Timeslice), u32>> =
		core::cell::RefCell::new(Default::default());
}

impl TestRenewalRights {
	pub fn set(who: u64, when: Timeslice, count: u32) {
		RENEWAL_RIGHTS.with(|r| {
			r.borrow_mut().insert((who, when), count);
		});
	}
}

impl RenewalRightsProvider<u64> for TestRenewalRights {
	fn renewal_rights_count(who: &u64, when: Timeslice) -> u32 {
		RENEWAL_RIGHTS.with(|r| r.borrow().get(&(*who, when)).copied().unwrap_or(0))
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_rights_count(who: &u64, when: Timeslice, count: u32) {
		Self::set(*who, when, count);
	}
}

impl crate::pallet::Config for Test {
	type Balance = u64;
	type RelayBlockNumber = u64;
	type WeightInfo = ();
	type CoreRangeProvider = TestCoreRangeProvider;
	type TimesliceProvider = TestTimesliceProvider;
	type RenewalRights = TestRenewalRights;
	type MaxBids = ConstU32<100>;
	type Randomness = TestRandomness;
}

pub fn new_config() -> ConfigRecord<u64, u64> {
	ConfigRecord {
		advance_notice: 2,
		market_period: 20,
		renewal_period: 10,
		ideal_bulk_proportion: sp_arithmetic::Perbill::from_percent(100),
		limit_cores_offered: None,
		region_length: 3,
		penalty: sp_arithmetic::Perbill::from_percent(30),
		contribution_timeout: 5,
		price_multiplier: 2,
		min_opening_price: 10,
		target_consumption_rate: sp_arithmetic::Perbill::from_percent(90),
		sensitivity_millis: 2500, // K = 2.5
		min_reserve_price: 1,
		min_increment: 100,
	}
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	TestCoreRangeProvider::set(DEFAULT_RESERVED, DEFAULT_CORE_COUNT);
	TestTimesliceProvider::set_latest_ready(0);
	let c = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	sp_io::TestExternalities::from(c)
}

/// Default core range: reserved=0, total=2 (so cores 0..2 sellable).
pub const DEFAULT_RESERVED: CoreIndex = 0;
pub const DEFAULT_CORE_COUNT: CoreIndex = 2;

pub struct TestExt(ConfigRecord<u64, u64>);
#[allow(dead_code)]
impl TestExt {
	pub fn new() -> Self {
		Self(new_config())
	}

	pub fn new_with_config(config: ConfigRecord<u64, u64>) -> Self {
		Self(config)
	}

	pub fn execute_with<R>(self, f: impl Fn() -> R) -> R {
		new_test_ext().execute_with(|| {
			frame_system::Pallet::<Test>::set_block_number(1);
			<CoretimeMarket as Market<u64, u64, u64>>::configure(self.0)
				.expect("configure should not fail");
			f()
		})
	}
}
