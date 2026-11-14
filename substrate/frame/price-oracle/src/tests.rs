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

//! Tests of how reports and votes are anchored and expired.

use crate::{self as pallet_price_oracle, *};
use core::{cell::Cell, time::Duration};
use frame_support::{
	assert_noop, assert_ok, derive_impl,
	traits::{ConstU32, Hooks},
};
use sp_price_oracle::{PriceReport, Quote};
use sp_runtime::{testing::UintAuthorityId, BuildStorage};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		PriceOracle: pallet_price_oracle,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

std::thread_local! {
	static NOW_MS: Cell<u64> = const { Cell::new(0) };
}

/// The chain time of the tests, set with [`set_now`].
pub struct ChainTime;
impl UnixTime for ChainTime {
	fn now() -> Duration {
		Duration::from_millis(NOW_MS.with(|n| n.get()))
	}
}

fn set_now(ms: u64) {
	NOW_MS.with(|n| n.set(ms));
}

/// Signers 1 to 3.
pub struct TestSigners;
impl Signers<UintAuthorityId> for TestSigners {
	fn signers() -> Vec<UintAuthorityId> {
		(1..=3).map(UintAuthorityId).collect()
	}
}

impl Config for Test {
	type SignerId = UintAuthorityId;
	type SignerSignature = sp_runtime::testing::TestSignature;
	type Signers = TestSigners;
	type MaxSigners = ConstU32<10>;
	type MarketPricing = order_book::OrderBookPricing;
	type MaxCrossRates = ConstU32<4>;
	type AnchorProvider = (System, ChainTime);
	type BlockNumberProvider = System;
	type AdminOrigin = frame_system::EnsureRoot<u64>;
	type OnPriceUpdate = ();
	type WeightInfo = ();
}

const PAIR: PairId = PairId(1);
/// Chain time at the start of every test.
const START_MS: u64 = 1_800_000_000_000;
const PARAMS: Parameters =
	Parameters { report_window: 10, report_window_ms: 30_000, quorum: 1, tick_interval_ms: 2_000 };

fn new_test_ext() -> sp_io::TestExternalities {
	let storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext = sp_io::TestExternalities::new(storage);
	ext.execute_with(|| {
		set_now(START_MS);
		System::set_block_number(100);
		assert_ok!(PriceOracle::set_parameters(RuntimeOrigin::root(), PARAMS));
		assert_ok!(PriceOracle::set_pair(
			RuntimeOrigin::root(),
			PAIR,
			PairConfig { market_quorum: 1 }
		));
	});
	ext
}

fn report(signer: u64, block: u32, timestamp_ms: u64, price: u128) -> SignedPriceReportOf<Test> {
	let report = PriceReport {
		anchor: Anchor::new(block, timestamp_ms),
		quotes: vec![Quote { pair: PAIR, price: Price::from_u32(price as u32) }],
	};
	let signer = UintAuthorityId(signer);
	let signature = signer.sign(&report.signing_payload()).unwrap();
	SignedPriceReport { report, signer, signature }
}

/// Process `reports` in a block of its own.
fn process(reports: Vec<SignedPriceReportOf<Test>>) {
	assert_ok!(PriceOracle::process_reports(RuntimeOrigin::none(), reports));
	PriceOracle::on_finalize(System::block_number());
}

/// The anchors of the votes for [`PAIR`], by signer.
fn votes() -> Vec<(u64, Anchor)> {
	Votes::<Test>::get(PAIR).into_iter().map(|v| (v.signer.0, v.anchor)).collect()
}

#[test]
fn reports_within_both_windows_are_accepted() {
	new_test_ext().execute_with(|| {
		process(vec![
			// The current block and time.
			report(1, 100, START_MS, 4),
			// The oldest block and time still accepted.
			report(2, 90, START_MS - 30_000, 4),
		]);
		assert_eq!(
			votes(),
			vec![(1, Anchor::new(100, START_MS)), (2, Anchor::new(90, START_MS - 30_000))]
		);
	});
}

#[test]
fn reports_outside_a_window_are_rejected() {
	new_test_ext().execute_with(|| {
		process(vec![
			// One block too old.
			report(1, 89, START_MS, 4),
			// One millisecond too old.
			report(2, 100, START_MS - 30_001, 4),
		]);
		assert!(votes().is_empty());
	});
}

#[test]
fn reports_ahead_of_the_block_are_rejected_but_not_ahead_of_the_time() {
	new_test_ext().execute_with(|| {
		process(vec![
			// A block ahead of the chain: rejected.
			report(1, 101, START_MS, 4),
			// A timestamp ahead of the chain time: the chain time may lag behind the signer's
			// clock, so it is accepted.
			report(2, 100, START_MS + 60_000, 4),
		]);
		assert_eq!(votes(), vec![(2, Anchor::new(100, START_MS + 60_000))]);
	});
}

#[test]
fn a_timestamp_ahead_does_not_outlive_the_block_window() {
	new_test_ext().execute_with(|| {
		process(vec![report(1, 100, START_MS + 60_000, 4)]);
		assert_eq!(votes().len(), 1);
		// Eleven blocks later the vote is out of the block window, whatever its timestamp.
		System::set_block_number(111);
		process(vec![]);
		assert!(votes().is_empty());
	});
}

#[test]
fn votes_expire_by_time_while_the_chain_stalls() {
	new_test_ext().execute_with(|| {
		process(vec![report(1, 100, START_MS, 4)]);
		assert!(Prices::<Test>::get(PAIR).is_some());
		// The chain stalls for a minute: one block later, the chain time moved on.
		System::set_block_number(101);
		set_now(START_MS + 60_000);
		process(vec![]);
		assert!(votes().is_empty());
	});
}

#[test]
fn reports_are_ordered_by_block_then_timestamp() {
	new_test_ext().execute_with(|| {
		process(vec![report(1, 99, START_MS, 4)]);
		// Same block, later timestamp: replaces.
		System::set_block_number(101);
		process(vec![report(1, 99, START_MS + 1_000, 5)]);
		assert_eq!(votes(), vec![(1, Anchor::new(99, START_MS + 1_000))]);
		// Same block, earlier timestamp: ignored.
		System::set_block_number(102);
		process(vec![report(1, 99, START_MS + 500, 6)]);
		assert_eq!(votes(), vec![(1, Anchor::new(99, START_MS + 1_000))]);
		// Later block, earlier timestamp: replaces, the block comes first.
		System::set_block_number(103);
		process(vec![report(1, 100, START_MS + 200, 7)]);
		assert_eq!(votes(), vec![(1, Anchor::new(100, START_MS + 200))]);
	});
}

#[test]
fn the_time_window_must_be_set() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			PriceOracle::set_parameters(
				RuntimeOrigin::root(),
				Parameters { report_window_ms: 0, ..PARAMS }
			),
			Error::<Test>::InvalidParameters
		);
	});
}

#[test]
fn settings_carry_both_windows() {
	new_test_ext().execute_with(|| {
		let settings = PriceOracle::settings();
		assert_eq!((settings.report_window, settings.report_window_ms), (10, 30_000));
	});
}
