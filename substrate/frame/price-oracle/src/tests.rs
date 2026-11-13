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

//! Tests of how reports and votes are anchored and expired, and of how votes become prices.

use crate::{self as pallet_price_oracle, *};
use core::{
	cell::{Cell, RefCell},
	time::Duration,
};
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
	static PRICE_UPDATES: RefCell<Vec<(PairId, Price)>> = const { RefCell::new(Vec::new()) };
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

/// Records the calls of [`OnPriceUpdate`], read with [`take_price_updates`].
pub struct RecordPriceUpdates;
impl OnPriceUpdate for RecordPriceUpdates {
	fn on_price_update(pair: PairId, price: Price) {
		PRICE_UPDATES.with(|u| u.borrow_mut().push((pair, price)));
	}
}

/// The `(pair, price)` that [`OnPriceUpdate`] was called with since the last call.
fn take_price_updates() -> Vec<(PairId, Price)> {
	PRICE_UPDATES.with(|u| u.take())
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
	type OnPriceUpdate = RecordPriceUpdates;
	type WeightInfo = ();
}

const PAIR: PairId = PairId(1);
/// A second pair, registered by the tests that use it.
const PAIR_B: PairId = PairId(2);
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
		// Tests start without the events and price updates of the setup.
		System::reset_events();
		take_price_updates();
	});
	ext
}

/// A report of `signer` at `anchor`, quoting a price for each of the given pairs.
fn report_quotes(
	signer: u64,
	anchor: Anchor,
	quotes: &[(PairId, u32)],
) -> SignedPriceReportOf<Test> {
	let report = PriceReport {
		anchor,
		quotes: quotes
			.iter()
			.map(|(pair, price)| Quote { pair: *pair, price: Price::from_u32(*price) })
			.collect(),
	};
	let signer = UintAuthorityId(signer);
	let signature = signer.sign(&report.signing_payload()).unwrap();
	SignedPriceReport { report, signer, signature }
}

fn report(signer: u64, block: u32, timestamp_ms: u64, price: u128) -> SignedPriceReportOf<Test> {
	report_quotes(signer, Anchor::new(block, timestamp_ms), &[(PAIR, price as u32)])
}

/// Process `reports` in a block of its own.
fn process(reports: Vec<SignedPriceReportOf<Test>>) {
	assert_ok!(PriceOracle::process_reports(RuntimeOrigin::none(), reports));
	PriceOracle::on_finalize(System::block_number());
}

/// The events the pallet deposited since the last call.
fn take_events() -> Vec<Event<Test>> {
	let events = System::events()
		.into_iter()
		.filter_map(|record| match record.event {
			RuntimeEvent::PriceOracle(event) => Some(event),
			_ => None,
		})
		.collect();
	System::reset_events();
	events
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

#[test]
fn price_is_the_median_of_the_votes() {
	new_test_ext().execute_with(|| {
		// The median is neither the mean (6) nor the price of the last report.
		process(vec![
			report(1, 100, START_MS, 5),
			report(2, 100, START_MS, 9),
			report(3, 100, START_MS, 4),
		]);
		assert_eq!(
			Prices::<Test>::get(PAIR),
			Some(PricePoint { price: Price::from_u32(5), updated_at: 100, signers: 3 })
		);
	});
}

#[test]
fn no_price_below_quorum() {
	new_test_ext().execute_with(|| {
		assert_ok!(PriceOracle::set_parameters(
			RuntimeOrigin::root(),
			Parameters { quorum: 2, ..PARAMS }
		));
		// One vote of the two needed: the vote is kept, the pair is not priced.
		process(vec![report(1, 100, START_MS, 5)]);
		assert_eq!(votes(), vec![(1, Anchor::new(100, START_MS))]);
		assert_eq!(Prices::<Test>::get(PAIR), None);
		// The vote of a second signer completes the quorum.
		System::set_block_number(101);
		process(vec![report(2, 101, START_MS, 7)]);
		assert_eq!(
			Prices::<Test>::get(PAIR),
			Some(PricePoint { price: Price::from_u32(6), updated_at: 101, signers: 2 })
		);
	});
}

#[test]
fn signer_counts_once_towards_quorum() {
	new_test_ext().execute_with(|| {
		assert_ok!(PriceOracle::set_parameters(
			RuntimeOrigin::root(),
			Parameters { quorum: 2, ..PARAMS }
		));
		process(vec![report(1, 100, START_MS, 5)]);
		// A second report of the same signer replaces its vote instead of adding one.
		System::set_block_number(101);
		process(vec![report(1, 101, START_MS, 7)]);
		assert_eq!(votes(), vec![(1, Anchor::new(101, START_MS))]);
		assert_eq!(Prices::<Test>::get(PAIR), None);
	});
}

#[test]
fn newer_vote_moves_the_median() {
	new_test_ext().execute_with(|| {
		process(vec![
			report(1, 100, START_MS, 5),
			report(2, 100, START_MS, 9),
			report(3, 100, START_MS, 4),
		]);
		assert_eq!(Prices::<Test>::get(PAIR).map(|p| p.price), Some(Price::from_u32(5)));
		// Signer 3 moves from below the median to above it: the votes are now 5, 9 and 10.
		System::set_block_number(101);
		process(vec![report(3, 101, START_MS, 10)]);
		assert_eq!(
			Prices::<Test>::get(PAIR),
			Some(PricePoint { price: Price::from_u32(9), updated_at: 101, signers: 3 })
		);
	});
}

#[test]
fn on_price_update_is_called_only_when_price_changes() {
	new_test_ext().execute_with(|| {
		let processed = Event::ReportsProcessed { accepted: 1, rejected: 0 };
		let updated =
			|price| Event::PriceUpdated { pair: PAIR, price: Price::from_u32(price), signers: 1 };

		// Signer 1 reports the first price. `PriceUpdated` is emitted and `OnPriceUpdate` is
		// called.
		process(vec![report(1, 100, START_MS, 5)]);
		assert_eq!(take_events(), vec![processed.clone(), updated(5)]);
		assert_eq!(take_price_updates(), vec![(PAIR, Price::from_u32(5))]);

		// Signer 1 reports the same price in the next block. `PriceUpdated` is not emitted and
		// `OnPriceUpdate` is not called, but `updated_at` is set to the current block.
		System::set_block_number(101);
		process(vec![report(1, 101, START_MS, 5)]);
		assert_eq!(take_events(), vec![processed.clone()]);
		assert_eq!(take_price_updates(), vec![]);
		assert_eq!(
			Prices::<Test>::get(PAIR),
			Some(PricePoint { price: Price::from_u32(5), updated_at: 101, signers: 1 })
		);

		// Signer 1 reports a different price. `PriceUpdated` is emitted and `OnPriceUpdate` is
		// called again.
		System::set_block_number(102);
		process(vec![report(1, 102, START_MS, 7)]);
		assert_eq!(take_events(), vec![processed, updated(7)]);
		assert_eq!(take_price_updates(), vec![(PAIR, Price::from_u32(7))]);
		assert_eq!(
			Prices::<Test>::get(PAIR),
			Some(PricePoint { price: Price::from_u32(7), updated_at: 102, signers: 1 })
		);
	});
}

#[test]
fn pairs_are_priced_independently() {
	new_test_ext().execute_with(|| {
		assert_ok!(PriceOracle::set_parameters(
			RuntimeOrigin::root(),
			Parameters { quorum: 2, ..PARAMS }
		));
		assert_ok!(PriceOracle::set_pair(
			RuntimeOrigin::root(),
			PAIR_B,
			PairConfig { market_quorum: 1 }
		));

		// Signer 1 quotes both pairs, signer 2 only `PAIR`. `PAIR` has two votes and a price,
		// `PAIR_B` has one vote and no price.
		process(vec![
			report_quotes(1, Anchor::new(100, START_MS), &[(PAIR, 5), (PAIR_B, 20)]),
			report_quotes(2, Anchor::new(100, START_MS), &[(PAIR, 7)]),
		]);
		assert_eq!(
			Prices::<Test>::get(PAIR),
			Some(PricePoint { price: Price::from_u32(6), updated_at: 100, signers: 2 })
		);
		assert_eq!(Votes::<Test>::get(PAIR_B).len(), 1);
		assert_eq!(Prices::<Test>::get(PAIR_B), None);

		// Signer 2 quotes only `PAIR_B` in the next block. `PAIR_B` gets a price, and the price
		// of `PAIR` still comes from the two votes of block 100.
		System::set_block_number(101);
		process(vec![report_quotes(2, Anchor::new(101, START_MS), &[(PAIR_B, 30)])]);
		assert_eq!(
			Prices::<Test>::get(PAIR_B),
			Some(PricePoint { price: Price::from_u32(25), updated_at: 101, signers: 2 })
		);
		assert_eq!(
			Prices::<Test>::get(PAIR),
			Some(PricePoint { price: Price::from_u32(6), updated_at: 101, signers: 2 })
		);
	});
}

#[test]
fn expired_votes_drop_out_of_the_median() {
	new_test_ext().execute_with(|| {
		// The vote of signer 1 is anchored five blocks before the other two.
		process(vec![
			report(1, 95, START_MS, 4),
			report(2, 100, START_MS, 6),
			report(3, 100, START_MS, 10),
		]);
		assert_eq!(
			Prices::<Test>::get(PAIR),
			Some(PricePoint { price: Price::from_u32(6), updated_at: 100, signers: 3 })
		);

		// At block 106 the vote of signer 1 is out of the block window. The price is the median
		// of the two votes left.
		System::set_block_number(106);
		process(vec![]);
		assert_eq!(votes(), vec![(2, Anchor::new(100, START_MS)), (3, Anchor::new(100, START_MS))]);
		assert_eq!(
			Prices::<Test>::get(PAIR),
			Some(PricePoint { price: Price::from_u32(8), updated_at: 106, signers: 2 })
		);

		// At block 111 the other two are out of the window as well, and the pair has no entry in
		// `Votes`.
		System::set_block_number(111);
		process(vec![]);
		assert!(!Votes::<Test>::contains_key(PAIR));
	});
}

#[test]
fn price_stays_after_quorum_is_lost() {
	new_test_ext().execute_with(|| {
		assert_ok!(PriceOracle::set_parameters(
			RuntimeOrigin::root(),
			Parameters { quorum: 2, ..PARAMS }
		));
		// The vote of signer 1 is anchored five blocks before the vote of signer 2.
		process(vec![report(1, 95, START_MS, 4), report(2, 100, START_MS, 6)]);
		let price = PricePoint { price: Price::from_u32(5), updated_at: 100, signers: 2 };
		assert_eq!(Prices::<Test>::get(PAIR), Some(price));

		// At block 106 the vote of signer 1 is out of the block window, leaving one vote of the
		// two needed. The price is not computed again and stays as it was at block 100.
		System::set_block_number(106);
		process(vec![]);
		assert_eq!(votes(), vec![(2, Anchor::new(100, START_MS))]);
		assert_eq!(Prices::<Test>::get(PAIR), Some(price));

		// At block 111 no votes are left. The price still stays, and consumers still read it.
		System::set_block_number(111);
		process(vec![]);
		assert!(votes().is_empty());
		assert_eq!(Prices::<Test>::get(PAIR), Some(price));
		assert_eq!(<PriceOracle as PriceProvider>::price(PAIR), Some(price));
	});
}
