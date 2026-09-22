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

//! Tests for the on-demand pallet.

use crate::{
	mock::*, Config, Error, Event, PendingBatch, PriceConfig, PriceParameters, QueueState, Revenue,
	RevenueRecord, DEFAULT_BASE_FEE, DEFAULT_PRICE_STEP,
};
use fp_coretime::revenue::OnDemandRevenue;
use frame_support::{
	assert_noop, assert_ok, dispatch::Pays,
	traits::{
		fungible::{Inspect, Mutate},
		Get,
	},
};
use sp_runtime::{traits::BadOrigin, Perbill};

const ALICE: u64 = 1;
const BOB: u64 = 2;

/// Place an order at Relay-chain block `relay_block_number`, returning the spot price paid.
fn place_order_at(relay_block_number: u32) -> u64 {
	set_relay_block_number(relay_block_number);
	let before = Balances::balance(&ALICE);
	let max_amount = DEFAULT_BASE_FEE as u64 * 10;
	assert_ok!(OnDemand::place_order(RuntimeOrigin::signed(ALICE), 2000, max_amount));
	before - Balances::balance(&ALICE)
}

fn set_order_cap(order_cap: u32) {
	let mut config = PriceConfig::<Test>::get();
	config.order_cap = order_cap;
	PriceConfig::<Test>::put(config);
}

fn on_demand_events() -> Vec<Event<Test>> {
	System::events()
		.into_iter()
		.filter_map(|record| match record.event {
			RuntimeEvent::OnDemand(event) => Some(event),
			_ => None,
		})
		.collect()
}

#[test]
fn place_order_charges_spot_price_and_batches_the_order() {
	new_test_ext().execute_with(|| {
		let before = Balances::balance(&ALICE);

		assert_ok!(OnDemand::place_order(
			RuntimeOrigin::signed(ALICE),
			2000,
			DEFAULT_BASE_FEE as u64
		));

		// The spot price of the first order is the base fee, and it went to the pallet's pot.
		assert_eq!(Balances::balance(&ALICE), before - DEFAULT_BASE_FEE as u64);
		assert_eq!(
			Balances::balance(&OnDemand::account_id()),
			EXISTENTIAL_DEPOSIT + DEFAULT_BASE_FEE as u64
		);

		// The order is pending, waiting to be forwarded to the Relay chain.
		let batch = PendingBatch::<Test>::get();
		assert_eq!(batch.len(), 1);
		assert_eq!(batch[0].para_id, 2000);
		assert_eq!(batch[0].ordered_at, 0);

		// The local queue estimate grew by one.
		assert_eq!(QueueState::<Test>::get().unwrap().outstanding_orders, 1);

		assert_eq!(
			on_demand_events(),
			vec![Event::OrderPlaced {
				para_id: 2000,
				spot_price: DEFAULT_BASE_FEE as u64,
				ordered_by: ALICE
			}]
		);
	});
}

#[test]
fn spot_price_grows_with_the_queue_depth() {
	new_test_ext().execute_with(|| {
		// Every order already outstanding raises the price by 3%.
		let price_0 = DEFAULT_BASE_FEE as u64;
		let price_1 = price_0 / 100 * (100 + DEFAULT_PRICE_STEP as u64);
		let price_2 = price_1 / 100 * (100 + DEFAULT_PRICE_STEP as u64);
		for expected_price in [price_0, price_1, price_2] {
			let before = Balances::balance(&ALICE);
			assert_ok!(OnDemand::place_order(RuntimeOrigin::signed(ALICE), 2000, expected_price));
			assert_eq!(Balances::balance(&ALICE), before - expected_price);
		}

		assert_eq!(QueueState::<Test>::get().unwrap().outstanding_orders, 3);
	});
}

#[test]
fn place_order_respects_max_amount() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			OnDemand::place_order(RuntimeOrigin::signed(ALICE), 2000, DEFAULT_BASE_FEE as u64 - 1),
			Error::<Test>::SpotPriceHigherThanMaxAmount
		);
		assert!(PendingBatch::<Test>::get().is_empty());
	});
}

#[test]
fn place_order_fails_once_the_order_cap_is_reached() {
	new_test_ext().execute_with(|| {
		set_order_cap(1);
		assert_ok!(OnDemand::place_order(
			RuntimeOrigin::signed(ALICE),
			2000,
			DEFAULT_BASE_FEE as u64
		));
		assert_noop!(
			OnDemand::place_order(RuntimeOrigin::signed(ALICE), 2000, DEFAULT_ACCOUNT_BALANCE / 2),
			Error::<Test>::QueueFull
		);
	});
}

#[test]
fn place_order_fails_when_no_cores_in_pool() {
	new_test_ext().execute_with(|| {
		MockCorePool::set_pool_cores(0);
		assert_noop!(
			OnDemand::place_order(RuntimeOrigin::signed(ALICE), 2000, DEFAULT_ACCOUNT_BALANCE / 2),
			Error::<Test>::EmptyPool
		);
	});
}

#[test]
fn the_queue_estimate_drains_over_relay_chain_blocks() {
	new_test_ext().execute_with(|| {
		for _ in 0..3 {
			assert_ok!(OnDemand::place_order(
				RuntimeOrigin::signed(ALICE),
				2000,
				DEFAULT_ACCOUNT_BALANCE / 10
			));
		}
		assert_eq!(QueueState::<Test>::get().unwrap().outstanding_orders, 3);

		// One order per Relay-chain block is assumed to be drained, so after five blocks the queue
		// is considered empty again and the next order costs the base fee.
		set_relay_block_number(5);
		let before = Balances::balance(&ALICE);
		assert_ok!(OnDemand::place_order(
			RuntimeOrigin::signed(ALICE),
			2000,
			DEFAULT_BASE_FEE as u64
		));
		assert_eq!(Balances::balance(&ALICE), before - DEFAULT_BASE_FEE as u64);

		let queue_state = QueueState::<Test>::get().unwrap();
		assert_eq!(queue_state.outstanding_orders, 1);
		assert_eq!(queue_state.last_updated, 5);
	});
}

#[test]
fn the_pending_batch_is_forwarded_to_the_relay_chain_on_finalize() {
	new_test_ext().execute_with(|| {
		assert_ok!(OnDemand::place_order(
			RuntimeOrigin::signed(ALICE),
			2000,
			DEFAULT_ACCOUNT_BALANCE / 2
		));
		assert_ok!(OnDemand::place_order(
			RuntimeOrigin::signed(ALICE),
			2001,
			DEFAULT_ACCOUNT_BALANCE / 2
		));

		advance_block();

		assert_eq!(queued_batches(), vec![vec![(2000, 0), (2001, 0)]]);
		assert!(PendingBatch::<Test>::get().is_empty());

		// Nothing is sent when no orders were placed.
		advance_block();
		assert_eq!(queued_batches().len(), 1);
	});
}

#[test]
fn configure_works() {
	new_test_ext().execute_with(|| {
		let config = PriceParameters {
			order_cap: 10,
			drain_rate_per_block: 2,
			price_step: Perbill::from_percent(10),
			base_fee: 1_000,
		};

		// Only root and the admin can configure the pallet.
		assert_noop!(OnDemand::configure(RuntimeOrigin::signed(2), config.clone()), BadOrigin);
		assert_noop!(OnDemand::configure(RuntimeOrigin::none(), config.clone()), BadOrigin);

		// Doubling the price with every order overflows long before reaching the order cap.
		let overflowing_config =
			PriceParameters { order_cap: 100, price_step: Perbill::from_percent(100), ..config };
		assert_noop!(
			OnDemand::configure(RuntimeOrigin::root(), overflowing_config),
			Error::<Test>::OrderPriceCanOverflow
		);

		assert_eq!(PriceConfig::<Test>::get(), PriceParameters::default());

		// A valid configuration is stored, and setting it is free.
		let post_info =
			OnDemand::configure(RuntimeOrigin::root(), config.clone()).expect("config is valid");
		assert_eq!(post_info.pays_fee, Pays::No);
		assert_eq!(PriceConfig::<Test>::get(), config);

		// The admin can configure the pallet too.
		let admin_config = PriceParameters { base_fee: 2_000, ..config };
		assert_ok!(OnDemand::configure(RuntimeOrigin::signed(ALICE), admin_config.clone()));
		assert_eq!(PriceConfig::<Test>::get(), admin_config);

		// The new configuration is used for pricing.
		let before = Balances::balance(&ALICE);
		assert_ok!(OnDemand::place_order(RuntimeOrigin::signed(ALICE), 2000, 2_000));
		assert_eq!(Balances::balance(&ALICE), before - 2_000);
		assert_ok!(OnDemand::place_order(RuntimeOrigin::signed(ALICE), 2000, 2_200));
		assert_eq!(Balances::balance(&ALICE), before - 2_000 - 2_200);
	});
}

#[test]
fn revenue_is_bucketed_by_the_relay_chain_block_of_the_order() {
	new_test_ext().execute_with(|| {
		let first = place_order_at(0);
		let second = place_order_at(0);
		let third = place_order_at(7);

		// Orders placed at the same Relay-chain block share a bucket.
		assert_eq!(
			Revenue::<Test>::get().into_inner(),
			vec![
				RevenueRecord { ordered_at: 0, amount: first + second },
				RevenueRecord { ordered_at: 7, amount: third },
			]
		);
	});
}

#[test]
fn claim_revenue_until_pays_out_only_what_was_earned_before_the_cutoff() {
	new_test_ext().execute_with(|| {
		let early = place_order_at(0) + place_order_at(3);
		let late = place_order_at(4);

		let beneficiary_before = Balances::balance(&BOB);

		// Everything ordered before Relay-chain block 4 is claimable, the rest is not.
		assert_eq!(<OnDemand as OnDemandRevenue<_, _, _>>::claim_revenue_until(4, &BOB), early);
		assert_eq!(Balances::balance(&BOB), beneficiary_before + early);
		assert_eq!(
			Revenue::<Test>::get().into_inner(),
			vec![RevenueRecord { ordered_at: 4, amount: late }]
		);
		assert!(on_demand_events().contains(&Event::RevenueClaimed {
			until: 4,
			amount: early,
			beneficiary: BOB,
		}));

		// The remainder is claimed once its block is in the past as well.
		assert_eq!(<OnDemand as OnDemandRevenue<_, _, _>>::claim_revenue_until(5, &BOB), late);
		assert_eq!(Balances::balance(&BOB), beneficiary_before + early + late);
		assert!(Revenue::<Test>::get().is_empty());
	});
}

#[test]
fn claiming_revenue_keeps_the_pot_alive() {
	new_test_ext().execute_with(|| {
		let pot = OnDemand::account_id();
		let revenue = place_order_at(0);
		assert_eq!(Balances::balance(&pot), EXISTENTIAL_DEPOSIT + revenue);

		// The whole revenue is paid out, but the pot is left with its existential deposit.
		assert_eq!(<OnDemand as OnDemandRevenue<_, _, _>>::claim_revenue_until(1, &BOB), revenue);
		assert_eq!(Balances::balance(&pot), EXISTENTIAL_DEPOSIT);
	});
}

#[test]
fn claiming_revenue_from_an_unendowed_pot_holds_back_one_existential_deposit() {
	new_test_ext().execute_with(|| {
		let pot = OnDemand::account_id();
		// Take away the endowment, so that the pot holds nothing but the revenue of the orders.
		Balances::set_balance(&pot, 0);

		let revenue = place_order_at(0);
		assert_eq!(Balances::balance(&pot), revenue);

		// The existential deposit is held back, once, to keep the pot from being reaped.
		assert_eq!(
			<OnDemand as OnDemandRevenue<_, _, _>>::claim_revenue_until(1, &BOB),
			revenue - EXISTENTIAL_DEPOSIT
		);
		assert_eq!(Balances::balance(&pot), EXISTENTIAL_DEPOSIT);

		// From then on the full revenue of every order is paid out.
		let revenue = place_order_at(1);
		assert_eq!(<OnDemand as OnDemandRevenue<_, _, _>>::claim_revenue_until(2, &BOB), revenue);
		assert_eq!(Balances::balance(&pot), EXISTENTIAL_DEPOSIT);
	});
}

#[test]
fn claiming_revenue_with_nothing_to_claim_is_a_no_op() {
	new_test_ext().execute_with(|| {
		let revenue = place_order_at(5);
		let before = Balances::balance(&BOB);

		// Nothing was ordered before Relay-chain block 5.
		assert_eq!(<OnDemand as OnDemandRevenue<_, _, _>>::claim_revenue_until(5, &BOB), 0);
		assert_eq!(Balances::balance(&BOB), before);
		assert_eq!(
			Revenue::<Test>::get().into_inner(),
			vec![RevenueRecord { ordered_at: 5, amount: revenue }]
		);
	});
}

#[test]
fn a_full_revenue_history_merges_into_the_most_recent_bucket() {
	new_test_ext().execute_with(|| {
		let capacity = <<Test as Config>::MaxRevenueHistory as Get<u32>>::get();

		let mut total = 0;
		for block in 0..capacity {
			total += place_order_at(block);
		}
		assert_eq!(Revenue::<Test>::get().len() as u32, capacity);

		// The history is full, so this order's revenue joins the last bucket instead of opening a
		// new one - no revenue is lost.
		let overflowing = place_order_at(capacity);
		total += overflowing;
		let revenue = Revenue::<Test>::get();
		assert_eq!(revenue.len() as u32, capacity);
		assert_eq!(revenue.last().unwrap().ordered_at, capacity - 1);
		assert_eq!(revenue.iter().map(|record| record.amount).sum::<u64>(), total);

		// It is therefore claimable one Relay-chain block earlier than it was ordered at.
		assert_eq!(
			<OnDemand as OnDemandRevenue<_, _, _>>::claim_revenue_until(capacity, &BOB),
			total
		);
	});
}
