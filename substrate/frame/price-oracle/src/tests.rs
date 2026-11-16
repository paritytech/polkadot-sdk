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

//! Tests of when reports are processed, which of them are accepted, how reports and votes are
//! anchored and expired, how votes become prices, and what a pause stops.

use crate::{self as pallet_price_oracle, *};
use core::{
	cell::{Cell, RefCell},
	time::Duration,
};
use frame_support::{
	assert_noop, assert_ok, derive_impl,
	dispatch::GetDispatchInfo,
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
	static PRICES_AT_UPDATE: RefCell<Vec<Vec<(PairId, Price)>>> =
		const { RefCell::new(Vec::new()) };
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

/// Records the calls of [`OnPriceUpdate`], read with [`take_price_updates`], and what [`Prices`]
/// holds at each call, read with [`take_prices_at_update`].
pub struct RecordPriceUpdates;
impl OnPriceUpdate for RecordPriceUpdates {
	fn on_price_update(pair: PairId, price: Price) {
		PRICE_UPDATES.with(|u| u.borrow_mut().push((pair, price)));
		let mut prices: Vec<_> = Prices::<Test>::iter().map(|(pair, p)| (pair, p.price)).collect();
		prices.sort();
		PRICES_AT_UPDATE.with(|p| p.borrow_mut().push(prices));
	}
}

/// The `(pair, price)` that [`OnPriceUpdate`] was called with since the last call.
fn take_price_updates() -> Vec<(PairId, Price)> {
	PRICE_UPDATES.with(|u| u.take())
}

/// What [`Prices`] held at each call of [`OnPriceUpdate`] since the last call, sorted by pair.
fn take_prices_at_update() -> Vec<Vec<(PairId, Price)>> {
	PRICES_AT_UPDATE.with(|p| p.take())
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
		take_prices_at_update();
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

#[test]
fn on_price_update_is_called_after_all_prices_are_written() {
	new_test_ext().execute_with(|| {
		assert_ok!(PriceOracle::set_pair(
			RuntimeOrigin::root(),
			PAIR_B,
			PairConfig { market_quorum: 1 }
		));
		// Signer 1 quotes both pairs in one report. Both get a price in block 100, so
		// `OnPriceUpdate` is called twice, first for `PAIR` and then for `PAIR_B`.
		process(vec![report_quotes(1, Anchor::new(100, START_MS), &[(PAIR, 5), (PAIR_B, 20)])]);
		let both_prices = vec![(PAIR, Price::from_u32(5)), (PAIR_B, Price::from_u32(20))];
		assert_eq!(take_price_updates(), both_prices);

		// What `Prices` held at each of the two calls. The price of `PAIR_B` is already written
		// at the first call, the one for `PAIR`.
		let prices_at_update = take_prices_at_update();
		assert_eq!(prices_at_update[0], both_prices);
		assert_eq!(prices_at_update[1], both_prices);
	});
}

#[test]
fn vote_with_timestamp_ahead_does_not_survive_a_stall() {
	new_test_ext().execute_with(|| {
		let hour = 3_600_000;
		// Signer 1 reports with a timestamp one hour ahead of the chain time.
		process(vec![report(1, 100, START_MS + hour, 4)]);
		// The chain stalls for 59 minutes: one block later, the chain time moved on by far more
		// than `report_window_ms`. No vote made before the stall is left.
		System::set_block_number(101);
		set_now(START_MS + hour - 60_000);
		process(vec![]);
		assert!(votes().is_empty());
	});
}

#[test]
fn report_with_more_quotes_than_pairs_is_rejected() {
	new_test_ext().execute_with(|| {
		// Two quotes, while only `PAIR` is registered.
		process(vec![report_quotes(1, Anchor::new(100, START_MS), &[(PAIR, 5), (PAIR, 7)])]);
		assert_eq!(take_events(), vec![Event::ReportsProcessed { accepted: 0, rejected: 1 }]);
		assert!(votes().is_empty());
	});
}

#[test]
fn quote_of_unregistered_pair_is_ignored() {
	new_test_ext().execute_with(|| {
		assert_ok!(PriceOracle::set_pair(
			RuntimeOrigin::root(),
			PAIR_B,
			PairConfig { market_quorum: 1 }
		));
		// `PAIR` and `PAIR_B` are registered, this pair is not.
		let unregistered = PairId(3);

		// Signer 1 quotes `PAIR` and the unregistered pair in one report. The quote of `PAIR`
		// becomes a vote, the other one is ignored.
		process(vec![report_quotes(
			1,
			Anchor::new(100, START_MS),
			&[(PAIR, 5), (unregistered, 20)],
		)]);
		assert_eq!(
			Prices::<Test>::get(PAIR),
			Some(PricePoint { price: Price::from_u32(5), updated_at: 100, signers: 1 })
		);
		assert!(!Votes::<Test>::contains_key(unregistered));
		assert_eq!(Prices::<Test>::get(unregistered), None);
	});
}

#[test]
fn report_of_unknown_signer_is_rejected() {
	new_test_ext().execute_with(|| {
		// Signer 4 signs correctly, but is not in `Signers::signers()`.
		process(vec![report(4, 100, START_MS, 5)]);
		assert_eq!(take_events(), vec![Event::ReportsProcessed { accepted: 0, rejected: 1 }]);
		assert!(votes().is_empty());
	});
}

#[test]
fn report_with_invalid_signature_is_rejected() {
	new_test_ext().execute_with(|| {
		// The price is changed after signer 1 signed the report.
		let mut changed = report(1, 100, START_MS, 5);
		changed.report.quotes[0].price = Price::from_u32(9);
		process(vec![changed]);
		assert_eq!(take_events(), vec![Event::ReportsProcessed { accepted: 0, rejected: 1 }]);
		assert!(votes().is_empty());
	});
}

#[test]
fn highest_anchor_of_a_signer_in_a_block_is_kept() {
	new_test_ext().execute_with(|| {
		// Three reports of signer 1 in one block. The one anchored at block 100 is kept, whether
		// the others come before or after it.
		process(vec![
			report(1, 99, START_MS, 4),
			report(1, 100, START_MS, 6),
			report(1, 98, START_MS, 9),
		]);
		assert!(take_events().contains(&Event::ReportsProcessed { accepted: 1, rejected: 2 }));
		assert_eq!(votes(), vec![(1, Anchor::new(100, START_MS))]);
		assert_eq!(Prices::<Test>::get(PAIR).map(|p| p.price), Some(Price::from_u32(6)));
	});
}

#[test]
fn later_report_wins_at_equal_anchors() {
	new_test_ext().execute_with(|| {
		process(vec![report(1, 100, START_MS, 4), report(1, 100, START_MS, 6)]);
		assert_eq!(Prices::<Test>::get(PAIR).map(|p| p.price), Some(Price::from_u32(6)));
	});
}

#[test]
fn report_with_invalid_signature_does_not_replace_a_valid_one() {
	new_test_ext().execute_with(|| {
		// Two reports of signer 1 in one block. The one with the higher anchor has an invalid
		// signature, so the other one is kept.
		let mut changed = report(1, 100, START_MS, 6);
		changed.report.quotes[0].price = Price::from_u32(9);
		process(vec![report(1, 99, START_MS, 4), changed]);
		assert_eq!(votes(), vec![(1, Anchor::new(99, START_MS))]);
		assert_eq!(Prices::<Test>::get(PAIR).map(|p| p.price), Some(Price::from_u32(4)));
	});
}

#[test]
fn only_none_origin_can_process_reports() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			PriceOracle::process_reports(RuntimeOrigin::signed(1), vec![]),
			DispatchError::BadOrigin
		);
		assert_noop!(
			PriceOracle::process_reports(RuntimeOrigin::root(), vec![]),
			DispatchError::BadOrigin
		);
	});
}

#[test]
fn reports_are_processed_once_per_block() {
	new_test_ext().execute_with(|| {
		assert_ok!(PriceOracle::process_reports(
			RuntimeOrigin::none(),
			vec![report(1, 100, START_MS, 5)]
		));
		// A second call in the same block fails.
		assert_noop!(
			PriceOracle::process_reports(RuntimeOrigin::none(), vec![report(2, 100, START_MS, 7)]),
			Error::<Test>::AlreadyProcessed
		);
		// The next block processes reports again.
		PriceOracle::on_finalize(100);
		System::set_block_number(101);
		assert_ok!(PriceOracle::process_reports(
			RuntimeOrigin::none(),
			vec![report(2, 101, START_MS, 7)]
		));
		assert_eq!(votes(), vec![(1, Anchor::new(100, START_MS)), (2, Anchor::new(101, START_MS))]);
	});
}

#[test]
fn more_reports_than_max_signers_fail() {
	new_test_ext().execute_with(|| {
		// Reports of signer 1, each with a later timestamp than the one before.
		let reports = |count: u64| -> Vec<_> {
			(0..count).map(|i| report(1, 100, START_MS + i, 5)).collect()
		};
		// `MaxSigners` is 10.
		assert_noop!(
			PriceOracle::process_reports(RuntimeOrigin::none(), reports(11)),
			Error::<Test>::TooManyReports
		);
		// The failed call left nothing behind, so the block can still process reports. As many
		// as `MaxSigners` are fine.
		assert_ok!(PriceOracle::process_reports(RuntimeOrigin::none(), reports(10)));
		assert_eq!(votes(), vec![(1, Anchor::new(100, START_MS + 9))]);
	});
}

#[test]
fn nothing_is_processed_while_parameters_are_unset() {
	new_test_ext().execute_with(|| {
		Params::<Test>::kill();
		// The call succeeds, but the report is ignored.
		process(vec![report(1, 100, START_MS, 5)]);
		assert!(take_events().is_empty());
		assert!(votes().is_empty());
	});
}

#[test]
fn create_inherent_takes_the_reports_of_the_inherent_data() {
	new_test_ext().execute_with(|| {
		let reports = vec![report(1, 100, START_MS, 5), report(2, 100, START_MS, 7)];
		let mut data = InherentData::new();
		data.put_data(INHERENT_IDENTIFIER, &reports).unwrap();
		assert_eq!(PriceOracle::create_inherent(&data), Some(Call::process_reports { reports }));

		// Without inherent data, the call has no reports.
		let no_reports = Some(Call::process_reports { reports: vec![] });
		assert_eq!(PriceOracle::create_inherent(&InherentData::new()), no_reports);

		// The same with inherent data that does not decode.
		let mut malformed = InherentData::new();
		malformed.put_data(INHERENT_IDENTIFIER, &vec![1u8, 2, 3]).unwrap();
		assert_eq!(PriceOracle::create_inherent(&malformed), no_reports);
	});
}

#[test]
fn process_reports_is_operational() {
	// Not `Mandatory`, so a call that fails does not make the block invalid.
	let call = Call::<Test>::process_reports { reports: vec![] };
	assert_eq!(call.get_dispatch_info().class, DispatchClass::Operational);
}

#[test]
fn processing_pause_ignores_reports() {
	new_test_ext().execute_with(|| {
		process(vec![report(1, 100, START_MS, 5)]);
		assert_ok!(PriceOracle::set_pause(RuntimeOrigin::root(), Some(Pause::Processing)));
		take_events();

		// The report of signer 2 is ignored: no event, and the votes are as they were.
		System::set_block_number(101);
		process(vec![report(2, 101, START_MS, 9)]);
		assert!(take_events().is_empty());
		assert_eq!(votes(), vec![(1, Anchor::new(100, START_MS))]);
		// Consumers keep reading the last price.
		assert_eq!(
			<PriceOracle as PriceProvider>::price(PAIR),
			Some(PricePoint { price: Price::from_u32(5), updated_at: 100, signers: 1 })
		);
	});
}

#[test]
fn publishing_pause_withholds_prices() {
	new_test_ext().execute_with(|| {
		assert_ok!(PriceOracle::set_pause(RuntimeOrigin::root(), Some(Pause::Publishing)));
		process(vec![report(1, 100, START_MS, 5)]);
		// The price is computed.
		assert_eq!(
			Prices::<Test>::get(PAIR),
			Some(PricePoint { price: Price::from_u32(5), updated_at: 100, signers: 1 })
		);
		// Consumers do not get it, and `OnPriceUpdate` is not called.
		assert_eq!(<PriceOracle as PriceProvider>::price(PAIR), None);
		assert!(take_price_updates().is_empty());
	});
}

#[test]
fn full_pause_stops_processing_and_publishing() {
	new_test_ext().execute_with(|| {
		process(vec![report(1, 100, START_MS, 5)]);
		assert_ok!(PriceOracle::set_pause(RuntimeOrigin::root(), Some(Pause::Full)));

		System::set_block_number(101);
		process(vec![report(2, 101, START_MS, 9)]);
		// The report of signer 2 is ignored.
		assert_eq!(votes(), vec![(1, Anchor::new(100, START_MS))]);
		// Consumers do not get a price.
		assert_eq!(<PriceOracle as PriceProvider>::price(PAIR), None);
	});
}

#[test]
fn lifting_the_pause_resumes() {
	new_test_ext().execute_with(|| {
		assert_ok!(PriceOracle::set_pause(RuntimeOrigin::root(), Some(Pause::Full)));
		process(vec![report(1, 100, START_MS, 5)]);
		assert!(votes().is_empty());
		assert_eq!(<PriceOracle as PriceProvider>::price(PAIR), None);

		// With the pause lifted, reports are processed and consumers get the price.
		assert_ok!(PriceOracle::set_pause(RuntimeOrigin::root(), None));
		System::set_block_number(101);
		process(vec![report(1, 101, START_MS, 5)]);
		assert_eq!(
			<PriceOracle as PriceProvider>::price(PAIR),
			Some(PricePoint { price: Price::from_u32(5), updated_at: 101, signers: 1 })
		);
	});
}

#[test]
fn only_admin_can_set_pause() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			PriceOracle::set_pause(RuntimeOrigin::signed(1), Some(Pause::Full)),
			DispatchError::BadOrigin
		);

		assert_ok!(PriceOracle::set_pause(RuntimeOrigin::root(), Some(Pause::Full)));
		assert_eq!(Paused::<Test>::get(), Some(Pause::Full));
		// `None` lifts the pause.
		assert_ok!(PriceOracle::set_pause(RuntimeOrigin::root(), None));
		assert_eq!(Paused::<Test>::get(), None);
		assert_eq!(
			take_events(),
			vec![Event::PauseSet { pause: Some(Pause::Full) }, Event::PauseSet { pause: None }]
		);
	});
}

#[test]
fn lifting_the_pause_does_not_bring_back_the_old_price() {
	new_test_ext().execute_with(|| {
		process(vec![report(1, 100, START_MS, 5)]);
		assert_ok!(PriceOracle::set_pause(RuntimeOrigin::root(), Some(Pause::Full)));

		// The pause is lifted 100 blocks later. Consumers do not get the price from before it.
		System::set_block_number(200);
		assert_ok!(PriceOracle::set_pause(RuntimeOrigin::root(), None));
		assert_eq!(<PriceOracle as PriceProvider>::price(PAIR), None);
		// Also not in the next block, as long as no new votes come in.
		System::set_block_number(201);
		process(vec![]);
		assert_eq!(<PriceOracle as PriceProvider>::price(PAIR), None);
	});
}

#[test]
fn on_price_update_is_called_after_publishing_pause_is_lifted() {
	new_test_ext().execute_with(|| {
		process(vec![report(1, 100, START_MS, 5)]);
		assert_eq!(take_price_updates(), vec![(PAIR, Price::from_u32(5))]);

		// The price changes while publishing is paused. `OnPriceUpdate` is not called.
		assert_ok!(PriceOracle::set_pause(RuntimeOrigin::root(), Some(Pause::Publishing)));
		System::set_block_number(101);
		process(vec![report(1, 101, START_MS, 7)]);
		assert!(take_price_updates().is_empty());

		// The pause is lifted and the price stays the same. `OnPriceUpdate` is called with it.
		assert_ok!(PriceOracle::set_pause(RuntimeOrigin::root(), None));
		System::set_block_number(102);
		process(vec![report(1, 102, START_MS, 7)]);
		assert_eq!(take_price_updates(), vec![(PAIR, Price::from_u32(7))]);
	});
}
