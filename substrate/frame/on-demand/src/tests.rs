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
	mock::*, weights::WeightInfo, Enabled, Error, Event, PendingBatch, PriceConfig,
	PriceParameters, QueueState, Revenue, DEFAULT_BASE_FEE, DEFAULT_PRICE_STEP,
};
use fp_coretime::revenue::OnDemandRevenue;
use frame_support::{
	assert_noop, assert_ok, assert_storage_noop,
	dispatch::Pays,
	traits::fungible::{Inspect, Mutate},
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
fn enable_works() {
	new_test_ext().execute_with(|| {
		// On-demand Coretime is not sold until it is enabled.
		Enabled::<Test>::kill();
		assert!(!<OnDemand as OnDemandRevenue<_, _>>::is_enabled());

		// Only the admin or root can enable it.
		assert_noop!(OnDemand::enable(RuntimeOrigin::signed(BOB)), BadOrigin);
		let post_info =
			OnDemand::enable(RuntimeOrigin::signed(ALICE)).expect("the admin can enable it");
		assert_eq!(post_info.pays_fee, Pays::No);
		assert!(<OnDemand as OnDemandRevenue<_, _>>::is_enabled());
		assert!(on_demand_events().contains(&Event::Enabled));

		// Enabling it again does nothing.
		System::reset_events();
		assert_storage_noop!(assert_ok!(OnDemand::enable(RuntimeOrigin::root())));
		assert!(on_demand_events().is_empty());
	});
}

#[test]
fn place_order_fails_until_enabled() {
	new_test_ext().execute_with(|| {
		Enabled::<Test>::kill();
		assert_noop!(
			OnDemand::place_order(RuntimeOrigin::signed(ALICE), 2000, DEFAULT_ACCOUNT_BALANCE / 2),
			Error::<Test>::Disabled
		);

		assert_ok!(OnDemand::enable(RuntimeOrigin::root()));
		assert_ok!(OnDemand::place_order(
			RuntimeOrigin::signed(ALICE),
			2000,
			DEFAULT_ACCOUNT_BALANCE / 2
		));
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

/// Claim the revenue of timeslice `up_to` and every earlier one for `BOB`.
fn claim(up_to: u32) -> Vec<(u32, u64)> {
	let result = <OnDemand as OnDemandRevenue<_, _>>::claim_revenue(up_to, &BOB);
	assert_eq!(result.paid.last().map(|&(when, _)| when), Some(up_to), "`up_to` is settled");
	result.paid
}

/// The unclaimed revenue, as `(timeslice, amount)`, newest first.
fn unclaimed() -> Vec<(u32, u64)> {
	Revenue::<Test>::get()
		.iter()
		.map(|record| (record.when, record.amount))
		.collect()
}

#[test]
fn revenue_is_booked_against_the_timeslice_of_the_order() {
	new_test_ext().execute_with(|| {
		// The first and last Relay-chain blocks of timeslice 0, and the first one of timeslice 1.
		let first = place_order_at(0);
		let second = place_order_at(TIMESLICE_PERIOD - 1);
		let third = place_order_at(TIMESLICE_PERIOD);

		assert_eq!(unclaimed(), vec![(1, third), (0, first + second)]);
	});
}

#[test]
fn claim_revenue_pays_out_the_given_and_all_earlier_timeslices() {
	new_test_ext().execute_with(|| {
		let in_0 = place_order_at(0);
		let in_2 = place_order_at(2 * TIMESLICE_PERIOD);
		let in_3 = place_order_at(3 * TIMESLICE_PERIOD);

		let beneficiary_before = Balances::balance(&BOB);

		// Timeslices 0 and 2 are claimed, oldest first, and timeslice 3 stays booked.
		assert_eq!(claim(2), vec![(0, in_0), (2, in_2)]);
		assert_eq!(Balances::balance(&BOB), beneficiary_before + in_0 + in_2);
		assert_eq!(unclaimed(), vec![(3, in_3)]);
		let events = on_demand_events();
		for (when, amount) in [(0, in_0), (2, in_2)] {
			assert!(events.contains(&Event::RevenueClaimed { when, amount, beneficiary: BOB }));
		}

		// A timeslice is only paid out once; it is still listed, as it is settled.
		assert_eq!(claim(2), vec![(2, 0)]);

		assert_eq!(claim(3), vec![(3, in_3)]);
		assert_eq!(Balances::balance(&BOB), beneficiary_before + in_0 + in_2 + in_3);
		assert!(unclaimed().is_empty());
	});
}

#[test]
fn the_oldest_unclaimed_revenue_is_merged_into_the_next_oldest_once_the_history_is_full() {
	new_test_ext().execute_with(|| {
		let revenue: Vec<_> = (0..=MAX_UNCLAIMED_TIMESLICES)
			.map(|when| place_order_at(when * TIMESLICE_PERIOD))
			.collect();

		// One timeslice too many has unclaimed revenue, so that of timeslice 0 is kept with
		// timeslice 1's. Nothing is lost.
		let mut expected: Vec<_> = (2..=MAX_UNCLAIMED_TIMESLICES)
			.rev()
			.map(|when| (when, revenue[when as usize]))
			.collect();
		expected.push((1, revenue[0] + revenue[1]));
		assert_eq!(unclaimed(), expected);

		expected.reverse();
		assert_eq!(claim(MAX_UNCLAIMED_TIMESLICES), expected);
	});
}

#[test]
fn revenue_for_an_earlier_timeslice_is_booked_against_that_timeslice() {
	new_test_ext().execute_with(|| {
		let in_0 = place_order_at(0);
		let in_2 = place_order_at(2 * TIMESLICE_PERIOD);
		let more_in_0 = place_order_at(0);
		let in_1 = place_order_at(TIMESLICE_PERIOD);

		assert_eq!(unclaimed(), vec![(2, in_2), (1, in_1), (0, in_0 + more_in_0)]);
	});
}

#[test]
fn revenue_for_an_earlier_timeslice_is_merged_correctly_into_a_full_history() {
	new_test_ext().execute_with(|| {
		// Fill the history with timeslices 2, 4, 6.
		let mut revenue: Vec<_> = (1..=MAX_UNCLAIMED_TIMESLICES)
			.map(|i| place_order_at(2 * i * TIMESLICE_PERIOD))
			.collect();
		revenue.reverse();

		// Timeslice 3 takes the place of timeslice 2, which is merged into it.
		let in_3 = place_order_at(3 * TIMESLICE_PERIOD);
		assert_eq!(unclaimed(), vec![(6, revenue[0]), (4, revenue[1]), (3, in_3 + revenue[2])]);

		// Timeslice 1 is older than any timeslice kept apart, so it is merged into the oldest.
		let in_1 = place_order_at(TIMESLICE_PERIOD);
		assert_eq!(
			unclaimed(),
			vec![(6, revenue[0]), (4, revenue[1]), (3, in_3 + revenue[2] + in_1)]
		);
	});
}

#[test]
fn the_weight_of_a_claim_depends_on_the_number_of_timeslices_claimed() {
	new_test_ext().execute_with(|| {
		place_order_at(0);
		place_order_at(TIMESLICE_PERIOD);
		place_order_at(2 * TIMESLICE_PERIOD);

		let result = <OnDemand as OnDemandRevenue<_, _>>::claim_revenue(1, &BOB);
		assert_eq!(result.weight, <() as WeightInfo>::claim_revenue(2));
		let result = <OnDemand as OnDemandRevenue<_, _>>::claim_revenue(1, &BOB);
		assert_eq!(result.weight, <() as WeightInfo>::claim_revenue(0));
		assert!(<() as WeightInfo>::claim_revenue(0).any_lt(<() as WeightInfo>::claim_revenue(2)));
	});
}

#[test]
fn claiming_revenue_keeps_the_pot_alive() {
	new_test_ext().execute_with(|| {
		let pot = OnDemand::account_id();
		let revenue = place_order_at(0);
		assert_eq!(Balances::balance(&pot), EXISTENTIAL_DEPOSIT + revenue);

		// The whole revenue is paid out, but the pot is left with its existential deposit.
		assert_eq!(claim(0), vec![(0, revenue)]);
		assert_eq!(Balances::balance(&pot), EXISTENTIAL_DEPOSIT);
	});
}

#[test]
fn only_revenue_which_can_be_paid_in_full_is_claimed_oldest_first() {
	new_test_ext().execute_with(|| {
		let pot = OnDemand::account_id();
		let in_0 = place_order_at(0);
		let in_1 = place_order_at(TIMESLICE_PERIOD);
		let in_2 = place_order_at(2 * TIMESLICE_PERIOD);

		// Something went wrong, and the pot is short of the revenue of timeslice 2 by one unit.
		let shortfall = 1;
		Balances::set_balance(&pot, EXISTENTIAL_DEPOSIT + in_0 + in_1 + in_2 - shortfall);

		// Timeslices 0 and 1 are paid out in full, timeslice 2 not at all.
		let result = <OnDemand as OnDemandRevenue<_, _>>::claim_revenue(2, &BOB);
		assert_eq!(result.paid, vec![(0, in_0), (1, in_1)]);
		assert_eq!(unclaimed(), vec![(2, in_2)]);
		assert_eq!(Balances::balance(&pot), EXISTENTIAL_DEPOSIT + in_2 - shortfall);
		assert!(on_demand_events().contains(&Event::UnexpectedLowFunds {
			attempted_to_claim: in_0 + in_1 + in_2,
			claimable: in_0 + in_1 + in_2 - shortfall,
			paid_out: in_0 + in_1,
		}));

		// Claiming again does not help until the pot has been topped up.
		let result = <OnDemand as OnDemandRevenue<_, _>>::claim_revenue(2, &BOB);
		assert_eq!(result.paid, vec![]);
		assert_eq!(unclaimed(), vec![(2, in_2)]);

		// Once it has, the revenue of timeslice 2 is claimed along with that of a later one.
		Balances::set_balance(&pot, Balances::balance(&pot) + shortfall);
		let in_3 = place_order_at(3 * TIMESLICE_PERIOD);
		assert_eq!(claim(3), vec![(2, in_2), (3, in_3)]);
		assert_eq!(Balances::balance(&pot), EXISTENTIAL_DEPOSIT);
	});
}

#[test]
fn an_unendowed_pot_holds_back_the_revenue_it_cannot_pay_out_while_staying_alive() {
	new_test_ext().execute_with(|| {
		let pot = OnDemand::account_id();
		// Take away the endowment, so that the pot holds nothing but the revenue of the orders.
		Balances::set_balance(&pot, 0);

		let revenue = place_order_at(0);
		assert_eq!(Balances::balance(&pot), revenue);

		// Paying out the revenue would reap the pot, so it is not paid out.
		let result = <OnDemand as OnDemandRevenue<_, _>>::claim_revenue(0, &BOB);
		assert_eq!(result.paid, vec![]);
		assert_eq!(unclaimed(), vec![(0, revenue)]);

		// Endowing the pot fixes it.
		Balances::set_balance(&pot, EXISTENTIAL_DEPOSIT + revenue);
		assert_eq!(claim(0), vec![(0, revenue)]);
		assert_eq!(Balances::balance(&pot), EXISTENTIAL_DEPOSIT);
	});
}

#[test]
fn claiming_revenue_with_nothing_to_claim_is_a_no_op() {
	new_test_ext().execute_with(|| {
		let revenue = place_order_at(TIMESLICE_PERIOD);
		let before = Balances::balance(&BOB);

		// Nothing was ordered in timeslice 0, but it is settled, so it is listed with nothing paid.
		assert_eq!(claim(0), vec![(0, 0)]);
		assert_eq!(Balances::balance(&BOB), before);
		assert_eq!(unclaimed(), vec![(1, revenue)]);
		assert!(!on_demand_events().iter().any(|e| matches!(e, Event::RevenueClaimed { .. })));
	});
}
