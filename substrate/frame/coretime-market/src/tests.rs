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

use crate::{
	mock::*,
	pallet::{Configuration, SaleInfo},
	BidDisplacement, Event, InitData, SalePhase,
};
use fp_coretime::{
	market::{AdjustBidResult, Market, OrderResult, RenewalOrderResult, TickAction},
	PotentialRenewalId, Timeslice,
};
use frame_support::{assert_noop, assert_ok, weights::WeightMeter};

type CoretimeMarket = crate::Pallet<Test>;
type Error = crate::pallet::Error<Test>;

fn market_events() -> Vec<Event<Test>> {
	frame_system::Pallet::<Test>::read_events_for_pallet::<Event<Test>>()
}

fn last_market_event() -> Event<Test> {
	market_events().pop().expect("Expected at least one market event")
}

/// The region_begin of the first sale with default config.
/// Computed as: old_region_end = commit_ts + region_length = (0+2)/2 + 3 = 4, new_begin = 4.
const FIRST_REGION_BEGIN: Timeslice = 4;

fn start_sales(reserve_price: u64) {
	let init = InitData { reserve_price };
	<CoretimeMarket as Market<u64, u64, u64>>::start_sales(0, init)
		.expect("start_sales should succeed");
}

fn tick(block_number: u64) -> Vec<TickAction<u64, u64, u64>> {
	let mut meter = WeightMeter::new();
	<CoretimeMarket as Market<u64, u64, u64>>::tick(block_number, &mut meter)
}

fn tick_with_ts(block_number: u64, latest_ready_ts: Timeslice) -> Vec<TickAction<u64, u64, u64>> {
	TestTimesliceProvider::set_latest_ready(latest_ready_ts);
	tick(block_number)
}

fn place_bid(
	block_number: u64,
	who: u64,
	price_limit: u64,
) -> Result<OrderResult<u64, u32>, Error> {
	<CoretimeMarket as Market<u64, u64, u64>>::place_order(block_number, &who, price_limit)
}

fn place_renewal(
	block_number: u64,
	who: u64,
	core: u16,
	when: u32,
) -> Result<RenewalOrderResult<u64, u32>, Error> {
	let renewal_id = PotentialRenewalId { core, when };
	<CoretimeMarket as Market<u64, u64, u64>>::place_renewal_order(block_number, &who, renewal_id)
}

fn adjust_bid(
	block_number: u64,
	id: u32,
	who: u64,
	new_price: Option<u64>,
) -> Result<AdjustBidResult<u64>, Error> {
	<CoretimeMarket as Market<u64, u64, u64>>::adjust_bid(block_number, id, &who, new_price)
}

/// Helper: run a sale through Market→Renewal, returning the sale info at Renewal phase.
fn setup_renewal_phase(bids: &[(u64, u64)]) -> crate::SaleInfoRecord<u64, u64> {
	start_sales(100);
	for &(who, price) in bids {
		place_bid(0, who, price).unwrap();
	}
	tick(20); // settle auction → Renewal
	assert_eq!(SaleInfo::<Test>::get().map(|s| s.phase), Some(SalePhase::Renewal));
	SaleInfo::<Test>::get().unwrap()
}

// --- Configuration ---

#[test]
fn configure_works() {
	TestExt::new().execute_with(|| {
		let mut config = new_config();
		config.market_period = 50;
		<CoretimeMarket as Market<u64, u64, u64>>::configure(config.clone()).unwrap();

		assert_eq!(Configuration::<Test>::get(), Some(config));
	});
}

#[test]
fn configure_rejects_invalid() {
	TestExt::new().execute_with(|| {
		let mut config = new_config();
		config.market_period = 0; // Invalid.
		assert_noop!(
			<CoretimeMarket as Market<u64, u64, u64>>::configure(config),
			Error::InvalidConfig
		);
	});
}

// --- Sales initialization ---

#[test]
fn start_sales_works() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		assert!(SaleInfo::<Test>::get().is_some());
		assert_eq!(SaleInfo::<Test>::get().map(|s| s.phase), Some(SalePhase::Market));
	});
}

#[test]
fn start_sales_fails_without_config() {
	new_test_ext().execute_with(|| {
		// No configure() called — should fail.
		let init = InitData { reserve_price: 100 };
		assert_noop!(
			<CoretimeMarket as Market<u64, u64, u64>>::start_sales(0, init),
			Error::Uninitialized
		);
	});
}

#[test]
fn start_sales_fails_without_core_range() {
	new_test_ext().execute_with(|| {
		TestCoreRangeProvider::clear();
		<CoretimeMarket as Market<u64, u64, u64>>::configure(new_config()).unwrap();
		// CoreRangeProvider returns None — should fail.
		let init = InitData { reserve_price: 100 };
		assert_noop!(
			<CoretimeMarket as Market<u64, u64, u64>>::start_sales(0, init),
			Error::Uninitialized
		);
	});
}

// --- Bidding ---

#[test]
fn place_bid_works() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		let result = place_bid(0, 1, 500).expect("bid should succeed");
		match result {
			OrderResult::BidPlaced { id, bid_price } => {
				assert_eq!(id, 0);
				assert_eq!(bid_price, 200);
			},
			_ => panic!("Expected BidPlaced"),
		}
		assert_eq!(last_market_event(), Event::BidPlaced { who: 1, bid_id: 0, amount: 200 });
	});
}

#[test]
fn place_bid_wrong_phase() {
	TestExt::new().execute_with(|| {
		// No sales started.
		assert_noop!(place_bid(0, 1, 100), Error::NoSales);
	});
}

#[test]
fn place_bid_too_early() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		let sale = SaleInfo::<Test>::get().unwrap();

		if sale.sale_start > 0 {
			assert_noop!(place_bid(sale.sale_start - 1, 1, 200), Error::TooEarly);
		}
	});
}

#[test]
fn bid_capped_at_current_descending_price() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		// At block 0, current_price = opening = 200.
		// Bid with price_limit 500 → should be capped at 200.
		let result = place_bid(0, 1, 500).unwrap();
		match result {
			OrderResult::BidPlaced { bid_price, .. } => assert_eq!(bid_price, 200),
			_ => panic!("Expected BidPlaced"),
		}

		// At block 10 (midpoint), current_price = 150.
		let result = place_bid(10, 2, 500).unwrap();
		match result {
			OrderResult::BidPlaced { bid_price, .. } => assert_eq!(bid_price, 150),
			_ => panic!("Expected BidPlaced"),
		}
	});
}

#[test]
fn bid_below_reserve_fails_without_filling_bid_slot() {
	TestExt::new().execute_with(|| {
		start_sales(100);

		assert_noop!(place_bid(0, 1, 99), Error::BidTooLow);
		assert!(crate::pallet::Bids::<Test>::get().is_empty());

		// A bid at the reserve is still valid.
		assert_ok!(place_bid(20, 1, 100));
		assert_eq!(crate::pallet::Bids::<Test>::get().len(), 1);
	});
}

#[test]
fn max_bids_limit_enforced() {
	TestExt::new().execute_with(|| {
		// MaxBids = 100 in mock config.
		start_sales(100);
		for i in 0..100u64 {
			place_bid(0, i + 1, 200).unwrap();
		}

		// 101st bid should fail.
		assert_noop!(place_bid(0, 101, 200), Error::TooManyBids);
	});
}

#[test]
fn adjust_bid_raise_works() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		let OrderResult::BidPlaced { id, bid_price: _ } = place_bid(0, 1, 150).unwrap() else {
			panic!()
		};

		let result = adjust_bid(0, id, 1, Some(180)).unwrap();
		match result {
			AdjustBidResult::Lock { amount } => {
				assert_eq!(amount, 30);
			},
			_ => panic!("Expected Lock"),
		}
		assert_eq!(
			last_market_event(),
			Event::BidRaised { who: 1, bid_id: id, old_price: 150, new_price: 180 }
		);
	});
}

#[test]
fn adjust_bid_withdraw_not_allowed() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		let OrderResult::BidPlaced { id, .. } = place_bid(0, 1, 150).unwrap() else { panic!() };

		// Withdrawal should fail (RFC-17: binding bids).
		assert_noop!(adjust_bid(0, id, 1, None), Error::NotAllowed);
	});
}

#[test]
fn adjust_bid_lower_fails() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		let OrderResult::BidPlaced { id, .. } = place_bid(0, 1, 150).unwrap() else { panic!() };

		assert_noop!(adjust_bid(0, id, 1, Some(100)), Error::Overpriced);
	});
}

#[test]
fn adjust_bid_wrong_owner_fails() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		let OrderResult::BidPlaced { id, .. } = place_bid(0, 1, 150).unwrap() else { panic!() };

		// User 2 tries to adjust user 1's bid.
		assert_noop!(adjust_bid(0, id, 2, Some(180)), Error::NotAllowed);
	});
}

#[test]
fn adjust_bid_nonexistent_fails() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		assert_noop!(adjust_bid(0, 999, 1, Some(100)), Error::BidNotExist);
	});
}

#[test]
fn adjust_bid_wrong_phase_fails() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		let OrderResult::BidPlaced { id, .. } = place_bid(0, 1, 150).unwrap() else { panic!() };

		tick(20);
		assert_eq!(SaleInfo::<Test>::get().map(|s| s.phase), Some(SalePhase::Renewal));

		assert_noop!(adjust_bid(25, id, 1, Some(180)), Error::WrongPhase);
	});
}

#[test]
fn adjust_bid_above_current_price_fails() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		// At block 10, price = 150 (midpoint between 200 and 100).
		let OrderResult::BidPlaced { id, .. } = place_bid(10, 1, 140).unwrap() else { panic!() };

		assert_noop!(adjust_bid(10, id, 1, Some(160)), Error::BidTooHigh);
	});
}

// --- Auction settlement ---

#[test]
fn auction_settles_on_tick() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		place_bid(0, 1, 200).unwrap();
		place_bid(0, 2, 150).unwrap();

		let actions = tick(20);

		assert!(SaleInfo::<Test>::get().map(|s| s.phase) == Some(SalePhase::Renewal));
		assert!(SaleInfo::<Test>::get().unwrap().clearing_price.is_some());

		let has_process =
			actions.iter().any(|a| matches!(a, TickAction::ProcessAutoRenewals { .. }));
		assert!(has_process, "Should have ProcessAutoRenewals action");

		let events = market_events();
		assert!(events.iter().any(|e| matches!(e, Event::AuctionSettled { .. })));
		assert!(events.iter().any(|e| matches!(
			e,
			Event::PhaseTransitioned { from: SalePhase::Market, to: SalePhase::Renewal }
		)));
	});
}

#[test]
fn fewer_bids_than_cores_clearing_at_reserve() {
	TestExt::new().execute_with(|| {
		// 2 cores offered, 1 bid at 200. Clearing = reserve (100).
		start_sales(100);
		place_bid(0, 1, 200).unwrap();
		let actions = tick(20);

		let clearing = SaleInfo::<Test>::get().unwrap().clearing_price.unwrap();
		let sale = SaleInfo::<Test>::get().unwrap();
		assert_eq!(clearing, sale.reserve_price);

		// Excess refund: 200 - 100 = 100.
		let refund = actions
			.iter()
			.find(|a| matches!(a, TickAction::Refund { who, .. } if *who == 1));
		match refund {
			Some(TickAction::Refund { amount, .. }) => assert_eq!(*amount, 100),
			_ => panic!("Expected excess refund"),
		}
	});
}

#[test]
fn no_bids_settles_cleanly() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		// No bids placed.
		let actions = tick(20);
		assert_eq!(SaleInfo::<Test>::get().map(|s| s.phase), Some(SalePhase::Renewal));

		// Clearing price should be reserve.
		let clearing = SaleInfo::<Test>::get().unwrap().clearing_price.unwrap();
		let sale = SaleInfo::<Test>::get().unwrap();
		assert_eq!(clearing, sale.reserve_price);
		assert_eq!(sale.cores_sold, 0);

		// No refund actions (no bids to refund).
		let refund_count =
			actions.iter().filter(|a| matches!(a, TickAction::Refund { .. })).count();
		assert_eq!(refund_count, 0);
	});
}

#[test]
fn settlement_refunds_excess_to_winners() {
	TestExt::new().execute_with(|| {
		// Use higher reserve so opening price is high enough to accept large bids.
		// reserve=200, opening=200*2=400. 2 cores.
		start_sales(200);
		// At block 0, current_price = opening = 400.
		place_bid(0, 1, 300).unwrap(); // bid capped at 300
		place_bid(0, 2, 200).unwrap(); // bid capped at 200
		let actions = tick(20);

		let clearing = SaleInfo::<Test>::get().unwrap().clearing_price.unwrap();
		assert_eq!(clearing, 200);

		// User 1 bid 300, clearing 200 → should get 100 excess refund.
		let refund_1 = actions
			.iter()
			.find(|a| matches!(a, TickAction::Refund { who, .. } if *who == 1));
		match refund_1 {
			Some(TickAction::Refund { amount, .. }) => assert_eq!(*amount, 100),
			_ => panic!("Expected 100 excess refund for user 1"),
		}

		// User 2 bid 200 = clearing → no refund.
		let refund_2 = actions
			.iter()
			.find(|a| matches!(a, TickAction::Refund { who, .. } if *who == 2));
		assert!(refund_2.is_none(), "User 2 should have no excess refund");
	});
}

#[test]
fn losers_get_full_refund() {
	TestExt::new().execute_with(|| {
		// 2 cores. 3 bids: 300, 200, 150. User 3 loses.
		TestCoreRangeProvider::set(0, 2);
		start_sales(100);
		place_bid(0, 1, 300).unwrap();
		place_bid(0, 2, 200).unwrap();
		place_bid(0, 3, 150).unwrap();
		let actions = tick(20);

		let refund_3 = actions
			.iter()
			.find(|a| matches!(a, TickAction::Refund { who, .. } if *who == 3));
		match refund_3 {
			Some(TickAction::Refund { amount, .. }) => assert_eq!(*amount, 150),
			_ => panic!("Expected full refund for losing user 3"),
		}
	});
}

#[test]
fn highest_bidders_win_not_first_bidders() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		place_bid(0, 1, 100).unwrap(); // lowest
		place_bid(0, 2, 150).unwrap(); // mid
		place_bid(0, 3, 200).unwrap(); // highest

		let actions = tick(20);

		let refund_1 = actions.iter().find(
			|a| matches!(a, TickAction::Refund { who, amount } if *who == 1 && *amount == 100),
		);
		assert!(refund_1.is_some(), "User 1 (lowest) should lose");

		let sale = SaleInfo::<Test>::get().unwrap();
		assert_eq!(sale.cores_sold, 2);
	});
}

#[test]
fn same_account_multiple_bids() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		place_bid(0, 1, 200).unwrap();
		place_bid(0, 1, 180).unwrap();

		tick(20);
		let sale = SaleInfo::<Test>::get().unwrap();
		assert_eq!(sale.cores_sold, 2);

		let actions = tick(30);
		let user1_sells = actions
			.iter()
			.filter(|a| matches!(a, TickAction::SellRegion { owner, .. } if *owner == 1))
			.count();
		assert_eq!(user1_sells, 2);
	});
}

// --- Renewals ---

#[test]
fn renewal_during_renewal_phase() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		place_bid(0, 1, 200).unwrap();
		tick(20);
		assert_eq!(SaleInfo::<Test>::get().map(|s| s.phase), Some(SalePhase::Renewal));

		let sale = SaleInfo::<Test>::get().unwrap();
		TestRenewalRights::set(2, sale.region_begin, 1);

		let result = place_renewal(25, 2, 0, sale.region_begin).unwrap();
		match result {
			RenewalOrderResult::Renewed { price, region_id, effective_to } => {
				assert!(price > 0);
				assert_eq!(region_id.begin, sale.region_begin);
				assert_eq!(effective_to, sale.region_end);

				assert_eq!(
					last_market_event(),
					Event::RenewalExercised { who: 2, price, region_id }
				);
			},
			_ => panic!("Expected Renewed"),
		}
	});
}

#[test]
fn renewal_without_rights_fails() {
	TestExt::new().execute_with(|| {
		let sale = setup_renewal_phase(&[(1, 200)]);
		assert_noop!(place_renewal(25, 2, 0, sale.region_begin), Error::Unavailable);
	});
}

#[test]
fn renewal_wrong_phase_fails() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		assert_eq!(SaleInfo::<Test>::get().map(|s| s.phase), Some(SalePhase::Market));

		TestRenewalRights::set(1, 3, 1);
		assert_noop!(place_renewal(5, 1, 0, 3), Error::WrongPhase);
	});
}

#[test]
fn renewal_with_wrong_timeslice_fails() {
	TestExt::new().execute_with(|| {
		let sale = setup_renewal_phase(&[(1, 200)]);

		TestRenewalRights::set(2, sale.region_begin, 1);

		// Pass a PotentialRenewalId with wrong `when` — doesn't match sale.region_begin.
		assert_noop!(place_renewal(25, 2, 0, sale.region_begin + 1), Error::Unavailable);
	});
}

#[test]
fn double_renewal_prevented() {
	TestExt::new().execute_with(|| {
		let sale = setup_renewal_phase(&[]);

		TestRenewalRights::set(1, sale.region_begin, 1);
		assert!(place_renewal(25, 1, 0, sale.region_begin).is_ok());

		assert_noop!(place_renewal(25, 1, 0, sale.region_begin), Error::Unavailable);
	});
}

#[test]
fn displacement_fails_when_pending_actions_full() {
	TestExt::new().execute_with(|| {
		let sale = setup_renewal_phase(&[(1, 200), (2, 150)]);

		// Pre-fill PendingRenewalActions to capacity.
		crate::pallet::PendingDisplacements::<Test>::put(sp_runtime::BoundedVec::truncate_from(
			(0..100u64)
				.map(|i| BidDisplacement { who: i, refund: 50 })
				.collect::<alloc::vec::Vec<_>>(),
		));

		TestRenewalRights::set(3, sale.region_begin, 1);
		assert_noop!(place_renewal(25, 3, 0, sale.region_begin), Error::TooManyBids);
	});
}

#[test]
fn multiple_renewal_rights_respected() {
	TestExt::new().execute_with(|| {
		let sale = setup_renewal_phase(&[]);

		TestRenewalRights::set(1, sale.region_begin, 2);
		assert!(place_renewal(25, 1, 0, sale.region_begin).is_ok());
		assert!(place_renewal(25, 1, 1, sale.region_begin).is_ok());

		assert_noop!(place_renewal(25, 1, 2, sale.region_begin), Error::Unavailable);
	});
}

#[test]
fn penalty_applied_when_oversubscribed() {
	TestExt::new().execute_with(|| {
		let sale = setup_renewal_phase(&[(1, 200), (2, 180)]);

		let clearing = SaleInfo::<Test>::get().unwrap().clearing_price.unwrap();
		let config = Configuration::<Test>::get().unwrap();
		let expected_penalty = config.penalty * clearing;
		let expected_price = clearing + expected_penalty;

		TestRenewalRights::set(3, sale.region_begin, 1);
		let result = place_renewal(25, 3, 0, sale.region_begin).unwrap();
		match result {
			RenewalOrderResult::Renewed { price, .. } => {
				assert_eq!(price, expected_price);
				assert!(price > clearing, "Penalty should increase renewal price");
			},
			_ => panic!("Expected Renewed"),
		}
	});
}

#[test]
fn no_penalty_when_not_oversubscribed() {
	TestExt::new().execute_with(|| {
		let sale = setup_renewal_phase(&[(1, 200)]);

		let clearing = SaleInfo::<Test>::get().unwrap().clearing_price.unwrap();

		TestRenewalRights::set(2, sale.region_begin, 1);
		let result = place_renewal(25, 2, 0, sale.region_begin).unwrap();
		match result {
			RenewalOrderResult::Renewed { price, .. } => {
				assert_eq!(price, clearing, "No penalty when not oversubscribed");
			},
			_ => panic!("Expected Renewed"),
		}
	});
}

#[test]
fn no_displacement_when_not_oversubscribed() {
	TestExt::new().execute_with(|| {
		let sale = setup_renewal_phase(&[(1, 200)]);

		TestRenewalRights::set(2, sale.region_begin, 1);
		let result = place_renewal(25, 2, 0, sale.region_begin).unwrap();
		match result {
			RenewalOrderResult::Renewed { .. } => {},
			_ => panic!("Expected direct Renewed"),
		}

		let actions = tick(30);
		let refund_count =
			actions.iter().filter(|a| matches!(a, TickAction::Refund { .. })).count();
		assert_eq!(refund_count, 0, "No displacement refunds");
	});
}

#[test]
fn quotas_cleared_between_sales() {
	TestExt::new().execute_with(|| {
		TestRenewalRights::set(1, FIRST_REGION_BEGIN, 2);
		start_sales(100);
		place_bid(0, 1, 200).unwrap();
		tick(20);

		let sale = SaleInfo::<Test>::get().unwrap();
		let quota = crate::pallet::Quotas::<Test>::get(1);
		assert_eq!(quota.auction_wins, 1);

		tick(30);
		tick_with_ts(35, sale.region_begin);

		let quota = crate::pallet::Quotas::<Test>::get(1);
		assert_eq!(quota.auction_wins, 0);
		assert_eq!(quota.renewals_used, 0);
	});
}

#[test]
fn renewal_rights_reset_after_sale_cycle() {
	TestExt::new().execute_with(|| {
		let sale = setup_renewal_phase(&[]);

		TestRenewalRights::set(1, sale.region_begin, 1);
		place_renewal(25, 1, 0, sale.region_begin).unwrap();

		tick(30);
		assert_eq!(SaleInfo::<Test>::get().map(|s| s.phase), Some(SalePhase::Settlement));

		let sale = SaleInfo::<Test>::get().unwrap();
		tick_with_ts(35, sale.region_begin);
		assert_eq!(SaleInfo::<Test>::get().map(|s| s.phase), Some(SalePhase::Market));

		tick(55);
		assert_eq!(SaleInfo::<Test>::get().map(|s| s.phase), Some(SalePhase::Renewal));

		let new_sale = SaleInfo::<Test>::get().unwrap();
		TestRenewalRights::set(1, new_sale.region_begin, 1);

		assert!(place_renewal(60, 1, 0, new_sale.region_begin).is_ok());
	});
}

// --- Renewal quota (auction wins reduce rights) ---

#[test]
fn renewal_quota_reduced_by_auction_wins() {
	TestExt::new().execute_with(|| {
		TestCoreRangeProvider::set(0, 3);
		TestRenewalRights::set(1, FIRST_REGION_BEGIN, 3);

		start_sales(100);
		place_bid(0, 1, 200).unwrap();
		place_bid(0, 1, 180).unwrap();
		tick(20);
		assert_eq!(SaleInfo::<Test>::get().map(|s| s.phase), Some(SalePhase::Renewal));
		let sale = SaleInfo::<Test>::get().unwrap();

		// remaining = 3 total - 2 auction wins = 1 renewal allowed.
		assert_ok!(place_renewal(25, 1, 0, sale.region_begin));
		assert_noop!(place_renewal(25, 1, 1, sale.region_begin), Error::Unavailable);
	});
}

#[test]
fn auction_wins_plus_renewals_exhaust_quota() {
	TestExt::new().execute_with(|| {
		TestCoreRangeProvider::set(0, 4);
		TestRenewalRights::set(1, FIRST_REGION_BEGIN, 3);

		start_sales(100);
		place_bid(0, 1, 200).unwrap(); // 1 auction win
		tick(20);
		let sale = SaleInfo::<Test>::get().unwrap();

		// remaining = 3 total - 1 auction win = 2 renewals allowed.
		assert_ok!(place_renewal(25, 1, 0, sale.region_begin));
		assert_ok!(place_renewal(25, 1, 1, sale.region_begin));

		// Third renewal fails: 1 auction + 2 renewals = 3 = total rights.
		assert_noop!(place_renewal(25, 1, 2, sale.region_begin), Error::Unavailable);
	});
}

#[test]
fn renewer_who_also_won_auction() {
	TestExt::new().execute_with(|| {
		TestCoreRangeProvider::set(0, 3);
		TestRenewalRights::set(1, FIRST_REGION_BEGIN, 3);

		start_sales(100);
		place_bid(0, 1, 200).unwrap();
		tick(20);
		let sale = SaleInfo::<Test>::get().unwrap();

		assert_ok!(place_renewal(25, 1, 0, sale.region_begin));
		assert_ok!(place_renewal(25, 1, 1, sale.region_begin));

		// Finalize: 1 SellRegion (auction win) + 2 RenewRegion.
		let actions = tick(30);
		let sell_count =
			actions.iter().filter(|a| matches!(a, TickAction::SellRegion { .. })).count();
		assert_eq!(sell_count, 1);
	});
}

// --- Displacement ---

#[test]
fn displacement_works_when_oversubscribed() {
	TestExt::new().execute_with(|| {
		let sale = setup_renewal_phase(&[(1, 200), (2, 150)]);

		TestRenewalRights::set(3, sale.region_begin, 1);
		let result = place_renewal(25, 3, 0, sale.region_begin).unwrap();
		match result {
			RenewalOrderResult::Renewed { price, region_id, effective_to } => {
				assert!(price > 0);
				assert_eq!(region_id.begin, sale.region_begin);
				assert_eq!(effective_to, sale.region_end);
			},
			_ => panic!("Expected Renewed via displacement"),
		}

		let events = market_events();
		assert!(events.iter().any(|e| matches!(e, Event::BidDisplaced { who: 2, .. })));
		assert!(events.iter().any(|e| matches!(e, Event::RenewalExercised { who: 3, .. })));

		let actions = tick(30);
		let sell_count =
			actions.iter().filter(|a| matches!(a, TickAction::SellRegion { .. })).count();
		let refund_count =
			actions.iter().filter(|a| matches!(a, TickAction::Refund { .. })).count();

		assert_eq!(sell_count, 1, "1 remaining auction winner");
		assert_eq!(refund_count, 1, "1 displaced refund");
	});
}

#[test]
fn displacement_targets_lowest_non_tenant_bidder() {
	TestExt::new().execute_with(|| {
		let sale = setup_renewal_phase(&[(1, 200), (2, 150)]);

		TestRenewalRights::set(3, sale.region_begin, 1);
		place_renewal(25, 3, 0, sale.region_begin).unwrap();

		let actions = tick(30);
		let refund = actions.iter().find(|a| matches!(a, TickAction::Refund { .. }));
		match refund {
			Some(TickAction::Refund { who, .. }) => assert_eq!(*who, 2),
			_ => panic!("Expected refund for displaced user 2"),
		}
	});
}

#[test]
fn existing_tenant_protected_from_displacement() {
	TestExt::new().execute_with(|| {
		// User 1 (tenant) has the LOWER bid and 2 renewal rights.
		// After winning 1 core in auction, they have 1 remaining right → protected.
		TestRenewalRights::set(1, FIRST_REGION_BEGIN, 2);
		let sale = setup_renewal_phase(&[(1, 150), (2, 200)]);

		TestRenewalRights::set(3, sale.region_begin, 1);
		// Should displace user 2 (non-tenant), NOT user 1 (tenant with lower bid).
		place_renewal(25, 3, 0, sale.region_begin).unwrap();

		let actions = tick(30);
		let refund = actions.iter().find(|a| matches!(a, TickAction::Refund { .. }));
		match refund {
			Some(TickAction::Refund { who, .. }) => assert_eq!(*who, 2),
			_ => panic!("Expected refund for non-tenant user 2"),
		}

		let sell = actions
			.iter()
			.find(|a| matches!(a, TickAction::SellRegion { owner, .. } if *owner == 1));
		assert!(sell.is_some(), "Tenant user 1 should keep their allocation");
	});
}

#[test]
fn displacement_fails_when_all_winners_are_tenants() {
	TestExt::new().execute_with(|| {
		// Both bidders are tenants with 2 rights each.
		// After 1 auction win each, they still have remaining capacity → protected.
		TestRenewalRights::set(1, FIRST_REGION_BEGIN, 2);
		TestRenewalRights::set(2, FIRST_REGION_BEGIN, 2);
		let sale = setup_renewal_phase(&[(1, 200), (2, 150)]);

		TestRenewalRights::set(3, sale.region_begin, 1);
		assert_noop!(place_renewal(25, 3, 0, sale.region_begin), Error::Unavailable);
	});
}

#[test]
fn tenant_protection_limited_to_renewal_rights_count() {
	TestExt::new().execute_with(|| {
		// User 1 has 1 renewal right but wins 2 cores in auction.
		// Only 1 allocation should be protected.
		TestCoreRangeProvider::set(0, 3);
		TestRenewalRights::set(1, FIRST_REGION_BEGIN, 1);

		start_sales(100);
		place_bid(0, 1, 200).unwrap();
		place_bid(0, 1, 180).unwrap();
		place_bid(0, 2, 150).unwrap();
		tick(20);
		assert_eq!(SaleInfo::<Test>::get().map(|s| s.phase), Some(SalePhase::Renewal));
		let sale = SaleInfo::<Test>::get().unwrap();

		// User 2 (150) displaced first, then user 1's unprotected allocation (180).
		TestRenewalRights::set(3, sale.region_begin, 1);
		place_renewal(25, 3, 0, sale.region_begin).unwrap();

		TestRenewalRights::set(4, sale.region_begin, 1);
		place_renewal(25, 4, 0, sale.region_begin).unwrap();

		let actions = tick(30);

		let user1_sells = actions
			.iter()
			.filter(|a| matches!(a, TickAction::SellRegion { owner, .. } if *owner == 1))
			.count();
		assert_eq!(user1_sells, 1, "Only 1 of user 1's 2 allocations should survive");

		let refund_count =
			actions.iter().filter(|a| matches!(a, TickAction::Refund { .. })).count();
		assert_eq!(refund_count, 2);
	});
}

#[test]
fn displaced_auction_win_stops_counting_against_tenant_protection() {
	TestExt::new().execute_with(|| {
		// User 1 has 1 renewal right and wins 2 cores in auction. Only one of
		// those wins is unprotected, so after one displacement the other must stay.
		TestRenewalRights::set(1, FIRST_REGION_BEGIN, 1);

		start_sales(100);
		place_bid(0, 1, 200).unwrap();
		place_bid(0, 1, 180).unwrap();
		tick(20);
		assert_eq!(SaleInfo::<Test>::get().map(|s| s.phase), Some(SalePhase::Renewal));
		let sale = SaleInfo::<Test>::get().unwrap();

		assert_eq!(crate::pallet::Quotas::<Test>::get(1).auction_wins, 2);

		// User 2 renews and displaces one of user 1's unprotected auction wins.
		TestRenewalRights::set(2, sale.region_begin, 1);
		assert_ok!(place_renewal(25, 2, 0, sale.region_begin));

		// User 1 now has 1 active auction win left, matching their 1 renewal right.
		assert_eq!(crate::pallet::Quotas::<Test>::get(1).auction_wins, 1);

		// User 3 cannot displace user 1's remaining protected auction win.
		TestRenewalRights::set(3, sale.region_begin, 1);
		assert_noop!(place_renewal(25, 3, 0, sale.region_begin), Error::Unavailable);

		let actions = tick(30);
		let user1_sells = actions
			.iter()
			.filter(|a| matches!(a, TickAction::SellRegion { owner, .. } if *owner == 1))
			.count();
		assert_eq!(user1_sells, 1, "User 1's protected allocation should survive");
	});
}

#[test]
fn displaced_refund_equals_clearing_price() {
	TestExt::new().execute_with(|| {
		start_sales(200); // reserve=200, opening=400
		place_bid(0, 1, 300).unwrap();
		place_bid(0, 2, 250).unwrap();
		tick(20);

		let clearing = SaleInfo::<Test>::get().unwrap().clearing_price.unwrap();

		let sale = SaleInfo::<Test>::get().unwrap();
		TestRenewalRights::set(3, sale.region_begin, 1);
		place_renewal(25, 3, 0, sale.region_begin).unwrap();

		let actions = tick(30);
		let refund = actions.iter().find_map(|a| match a {
			TickAction::Refund { amount, .. } => Some(*amount),
			_ => None,
		});
		assert_eq!(refund, Some(clearing));
	});
}

#[test]
fn multiple_displacements_in_one_renewal_phase() {
	TestExt::new().execute_with(|| {
		TestCoreRangeProvider::set(0, 3);
		start_sales(100);
		place_bid(0, 1, 200).unwrap();
		place_bid(0, 2, 180).unwrap();
		place_bid(0, 3, 150).unwrap();
		tick(20);
		let sale = SaleInfo::<Test>::get().unwrap();

		TestRenewalRights::set(4, sale.region_begin, 1);
		TestRenewalRights::set(5, sale.region_begin, 1);
		place_renewal(25, 4, 0, sale.region_begin).unwrap();
		place_renewal(25, 5, 0, sale.region_begin).unwrap();

		let actions = tick(30);
		let sell_count =
			actions.iter().filter(|a| matches!(a, TickAction::SellRegion { .. })).count();
		let refund_count =
			actions.iter().filter(|a| matches!(a, TickAction::Refund { .. })).count();

		assert_eq!(sell_count, 1, "1 remaining auction winner");
		assert_eq!(refund_count, 2, "2 displaced refunds");

		let finalized_sale = SaleInfo::<Test>::get().unwrap();
		assert_eq!(finalized_sale.cores_offered, 3);
		assert_eq!(finalized_sale.cores_sold, 3);
	});
}

// --- Descending price ---

#[test]
fn descending_price_linear() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		let sale = SaleInfo::<Test>::get().unwrap();

		// opening = reserve(100) * multiplier(2) = 200, reserve = 100, market_period = 20.
		let price_at_start = CoretimeMarket::current_price(sale.sale_start).unwrap();
		assert_eq!(price_at_start, 200, "Price at start should be opening price");

		let price_at_mid = CoretimeMarket::current_price(sale.sale_start + 10).unwrap();
		assert_eq!(price_at_mid, 150, "Price at midpoint should be halfway");

		let price_at_end = CoretimeMarket::current_price(sale.sale_start + 20).unwrap();
		assert_eq!(price_at_end, 100, "Price at end should be reserve price");

		// Past the end should clamp at reserve.
		let price_past_end = CoretimeMarket::current_price(sale.sale_start + 30).unwrap();
		assert_eq!(price_past_end, 100, "Price past end should be reserve");
	});
}

// --- Price adaptation ---

#[test]
fn price_unchanged_at_target_consumption() {
	TestExt::new().execute_with(|| {
		// 10 cores, sell 9 = 90% = target consumption rate.
		TestCoreRangeProvider::set(0, 10);
		let mut config = new_config();
		config.ideal_bulk_proportion = sp_arithmetic::Perbill::from_percent(100);
		<CoretimeMarket as Market<u64, u64, u64>>::configure(config).unwrap();

		start_sales(1000);
		// Place 9 bids to fill 9 of 10 cores = 90%.
		for i in 1..=9u64 {
			place_bid(0, i, 10000).unwrap();
		}

		tick(20);
		tick(30);
		let old_sale = SaleInfo::<Test>::get().unwrap();
		let old_reserve = old_sale.reserve_price;

		tick_with_ts(35, old_sale.region_begin);
		let new_sale = SaleInfo::<Test>::get().unwrap();

		// At target: exp(K * 0) = exp(0) = 1 → reserve unchanged.
		assert_eq!(new_sale.reserve_price, old_reserve);
	});
}

#[test]
fn full_consumption_applies_min_increment() {
	TestExt::new().execute_with(|| {
		// 100% consumption: exp(K * 0.1) increase is < min_increment(100),
		// so min_increment applies → new reserve = 100 + 100 = 200.
		start_sales(100);
		place_bid(0, 1, 200).unwrap();
		place_bid(0, 2, 200).unwrap();

		tick(20);
		tick(30);
		let old_sale = SaleInfo::<Test>::get().unwrap();

		tick_with_ts(35, old_sale.region_begin);
		let new_sale = SaleInfo::<Test>::get().unwrap();

		// min_increment = 100, old_reserve = 100 → new_reserve = 200.
		assert_eq!(new_sale.reserve_price, 200);
	});
}

#[test]
fn zero_consumption_price_drops_significantly() {
	TestExt::new().execute_with(|| {
		// 0% consumption → reserve drops from 100 to 10.
		start_sales(100);

		tick(20);
		tick(30);
		let old_sale = SaleInfo::<Test>::get().unwrap();

		tick_with_ts(35, old_sale.region_begin);
		let new_sale = SaleInfo::<Test>::get().unwrap();

		assert_eq!(new_sale.reserve_price, 10);
	});
}

#[test]
fn reserve_price_floored_at_minimum() {
	TestExt::new().execute_with(|| {
		// Very low reserve (2), 0% consumption → candidate would be < 1.
		// min_reserve_price = 1 should floor it.
		start_sales(2);

		tick(20);
		tick(30);
		let old_sale = SaleInfo::<Test>::get().unwrap();

		tick_with_ts(35, old_sale.region_begin);
		let new_sale = SaleInfo::<Test>::get().unwrap();

		assert_eq!(new_sale.reserve_price, 1, "Reserve should be floored at min_reserve_price");
	});
}

#[test]
fn half_consumption_price_decreases_moderately() {
	TestExt::new().execute_with(|| {
		// 2 cores, 1 sold = 50% consumption.
		// deviation = 40% (negative), K = 2.5, exp(-1.0) ≈ 0.3679
		// candidate = floor(0.3679 * 100) = 36
		start_sales(100);
		place_bid(0, 1, 200).unwrap();

		tick(20);
		tick(30);
		let old_sale = SaleInfo::<Test>::get().unwrap();

		tick_with_ts(35, old_sale.region_begin);
		let new_sale = SaleInfo::<Test>::get().unwrap();

		assert_eq!(new_sale.reserve_price, 36);
	});
}

// --- Sale rotation ---

#[test]
fn sale_rotation_sets_correct_regions() {
	TestExt::new().execute_with(|| {
		start_sales(100);

		// Get first sale's region boundaries.
		let first_sale = SaleInfo::<Test>::get().unwrap();
		let first_region_begin = first_sale.region_begin;
		let first_region_end = first_sale.region_end;
		let config = Configuration::<Test>::get().unwrap();

		assert_eq!(first_region_end, first_region_begin + config.region_length);

		// Run through full cycle to rotate.
		tick(20);
		tick(30);
		tick_with_ts(35, first_sale.region_begin);

		let second_sale = SaleInfo::<Test>::get().unwrap();

		// New sale's region_begin should be old sale's region_end.
		assert_eq!(second_sale.region_begin, first_region_end);
		assert_eq!(second_sale.region_end, first_region_end + config.region_length);
		assert_eq!(second_sale.cores_sold, 0);
		assert!(second_sale.clearing_price.is_none());
	});
}

#[test]
fn sale_rotation_respects_core_range() {
	TestExt::new().execute_with(|| {
		// Set core range [2, 5) → 3 sellable cores, first_core = 2.
		TestCoreRangeProvider::set(2, 5);
		start_sales(100);

		let sale = SaleInfo::<Test>::get().unwrap();
		assert_eq!(sale.first_core, 2);
		assert_eq!(sale.cores_offered, 3);
	});
}

#[test]
fn sale_rotation_opening_price_from_reserve() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		let sale = SaleInfo::<Test>::get().unwrap();
		let config = Configuration::<Test>::get().unwrap();

		// opening = max(min_opening_price, reserve * multiplier)
		let expected = sale
			.reserve_price
			.saturating_mul(config.price_multiplier as u64)
			.max(config.min_opening_price);
		assert_eq!(sale.opening_price, expected);
	});
}

#[test]
fn settlement_does_not_rotate_until_timeslice_ready() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		place_bid(0, 1, 200).unwrap();

		tick(20); // → Renewal
		tick(30); // → Settlement
		assert_eq!(SaleInfo::<Test>::get().map(|s| s.phase), Some(SalePhase::Settlement));

		TestTimesliceProvider::set_latest_ready(0);
		let sale = SaleInfo::<Test>::get().unwrap();
		let actions = tick(35);
		assert!(actions.is_empty());
		assert_eq!(SaleInfo::<Test>::get().map(|s| s.phase), Some(SalePhase::Settlement));

		let actions = tick_with_ts(36, sale.region_begin);
		assert_eq!(SaleInfo::<Test>::get().map(|s| s.phase), Some(SalePhase::Market));
		assert!(actions.iter().any(|a| matches!(a, TickAction::SaleRotated { .. })));
	});
}

// --- Full sale cycle ---

#[test]
fn tick_before_sales_started_returns_empty() {
	TestExt::new().execute_with(|| {
		let actions = tick(10);
		assert!(actions.is_empty());
	});
}

#[test]
fn full_sale_cycle() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		place_bid(0, 1, 200).unwrap();
		place_bid(0, 2, 180).unwrap();

		let _actions = tick(20);
		assert_eq!(SaleInfo::<Test>::get().map(|s| s.phase), Some(SalePhase::Renewal));

		let actions = tick(30);
		assert_eq!(SaleInfo::<Test>::get().map(|s| s.phase), Some(SalePhase::Settlement));

		let sell_count =
			actions.iter().filter(|a| matches!(a, TickAction::SellRegion { .. })).count();
		assert_eq!(sell_count, 2);

		let events = market_events();
		assert!(events.iter().any(|e| matches!(e, Event::SaleFinalized { regions_issued: 2 })));
		assert!(events.iter().any(|e| matches!(
			e,
			Event::PhaseTransitioned { from: SalePhase::Renewal, to: SalePhase::Settlement }
		)));

		let sale = SaleInfo::<Test>::get().unwrap();
		let actions = tick_with_ts(35, sale.region_begin);
		assert_eq!(SaleInfo::<Test>::get().map(|s| s.phase), Some(SalePhase::Market));

		let has_rotated = actions.iter().any(|a| matches!(a, TickAction::SaleRotated { .. }));
		assert!(has_rotated, "Should have SaleRotated action");

		let events = market_events();
		assert!(events.iter().any(|e| matches!(e, Event::SaleInitialized { .. })));
	});
}

#[test]
fn multiple_sale_cycles_price_adapts() {
	TestExt::new().execute_with(|| {
		// Cycle 1: 100% consumption → price increases.
		start_sales(100);
		place_bid(0, 1, 200).unwrap();
		place_bid(0, 2, 200).unwrap();
		tick(20);
		tick(30);
		let sale1 = SaleInfo::<Test>::get().unwrap();
		tick_with_ts(35, sale1.region_begin);

		let sale2 = SaleInfo::<Test>::get().unwrap();
		let reserve_after_cycle1 = sale2.reserve_price;
		assert!(reserve_after_cycle1 > 100, "Reserve should increase after full consumption");

		// Cycle 2: 0% consumption → price decreases.
		tick(55);
		tick(65);
		let sale2_final = SaleInfo::<Test>::get().unwrap();
		tick_with_ts(70, sale2_final.region_begin);

		let sale3 = SaleInfo::<Test>::get().unwrap();
		assert!(
			sale3.reserve_price < reserve_after_cycle1,
			"Reserve should decrease after zero consumption"
		);
	});
}
