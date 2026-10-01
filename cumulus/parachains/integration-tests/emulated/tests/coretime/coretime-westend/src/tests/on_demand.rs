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

use crate::imports::*;
use cumulus_pallet_parachain_system::RelaychainDataProvider;
use frame_support::assert_noop;
use pallet_broker::{
	ConfigRecord, CoreAssignment, CoreMask, ScheduleItem, Timeslice, CORE_MASK_BITS,
};
use pallet_on_demand_para::{PendingBatch, PriceConfig, PriceParameters};
use polkadot_runtime_parachains::on_demand;
use sp_runtime::{traits::BlockNumberProvider, Perbill};
use westend_runtime_constants::system_parachain::coretime::TIMESLICE_PERIOD;
use westend_system_emulated_network::westend_emulated_chain::westend_runtime::{
	Dmp, OnDemandAssignmentProvider, Runtime as WestendRuntime,
};

type CoretimeRuntime = <CoretimeWestend as Chain>::Runtime;
type OnDemand = <CoretimeWestend as CoretimeWestendPallet>::OnDemand;
type Broker = <CoretimeWestend as CoretimeWestendPallet>::Broker;
type Balances = <CoretimeWestend as CoretimeWestendPallet>::Balances;
type Balance = pallet_on_demand_para::BalanceOf<CoretimeRuntime>;

/// The number of cores in the Instantaneous Coretime Pool, as seen by the broker.
fn pool_cores() -> u32 {
	CoretimeWestend::ext_wrapper(|| {
		pallet_broker::Status::<CoretimeRuntime>::get().map_or(0, |status| {
			(status.private_pool_size + status.system_pool_size) / CORE_MASK_BITS as u32
		})
	})
}

/// The last timeslice the broker has ended.
fn broker_last_timeslice() -> Timeslice {
	CoretimeWestend::ext_wrapper(|| {
		pallet_broker::Status::<CoretimeRuntime>::get()
			.expect("sales have started")
			.last_timeslice
	})
}

/// The base fee of on-demand orders in the revenue attribution tests.
const BASE_FEE: Balance = CORETIME_WESTEND_ED * 10;

/// The price parameters used by the tests, with the given base fee.
fn price_config() -> PriceParameters<Balance> {
	PriceParameters {
		order_cap: 10,
		drain_rate_per_block: 1,
		price_step: Perbill::from_percent(10),
		base_fee: BASE_FEE,
	}
}

/// Reserve a whole core for the Instantaneous Coretime Pool, start sales, and configure the
/// on-demand pallet.
fn configure_pool_and_on_demand(price_config: PriceParameters<Balance>) {
	let sender = CoretimeWestendSender::get();
	let pot = CoretimeWestend::ext_wrapper(OnDemand::account_id);

	Westend::execute_with(|| {
		Dmp::make_parachain_reachable(CoretimeWestend::para_id());
	});

	CoretimeWestend::execute_with(|| {
		let root = <CoretimeWestend as Chain>::RuntimeOrigin::root();

		// The pot is expected to be pre-funded with the existential deposit, which must never be
		// paid out.
		assert_ok!(Balances::transfer_keep_alive(
			<CoretimeWestend as Chain>::RuntimeOrigin::signed(sender.clone()),
			pot.clone().into(),
			CORETIME_WESTEND_ED
		));
		assert_eq!(Balances::free_balance(&pot), CORETIME_WESTEND_ED);

		let schedule =
			vec![ScheduleItem { mask: CoreMask::complete(), assignment: CoreAssignment::Pool }];
		assert_ok!(Broker::reserve(
			root.clone(),
			schedule.try_into().expect("Vector is within bounds.")
		));

		let config = ConfigRecord {
			advance_notice: 1,
			interlude_length: 1,
			leadin_length: 2,
			region_length: 1,
			ideal_bulk_proportion: Perbill::from_percent(40),
			limit_cores_offered: None,
			renewal_bump: Perbill::from_percent(2),
			contribution_timeout: 1,
		};
		assert_ok!(Broker::configure(root.clone(), config));
		assert_ok!(Broker::start_sales(root.clone(), 100, 0));

		assert_ok!(OnDemand::configure(root, price_config.clone()));
		assert_eq!(PriceConfig::<CoretimeRuntime>::get(), price_config);
	});
}

/// Run until the broker has committed the timeslice which puts the reserved core into the pool.
fn wait_for_pool() {
	// The relay chain processes the core count request.
	Westend::execute_with(|| {
		Westend::assert_ump_queue_processed(true, Some(CoretimeWestend::para_id()), None);
	});

	let mut blocks = 0;
	while pool_cores() == 0 {
		assert!(blocks < TIMESLICE_PERIOD * 10, "the pool never got a core");
		CoretimeWestend::execute_with(|| {});
		Westend::execute_with(|| {});
		blocks += 1;
	}
}

/// Make the next Coretime chain block be built on top of Relay-chain block `relay_parent`,
/// skipping the Relay-chain blocks in between, as if the chain had stalled.
fn jump_relay_chain_to(relay_parent: u32) {
	Westend::ext_wrapper(|| {
		let now = <Westend as Chain>::System::block_number();
		assert!(now < relay_parent, "can only jump forward");
		// Building the next Coretime chain block advances the Relay chain by one block.
		<Westend as Chain>::System::set_block_number(relay_parent - 1);
	});
}

/// The on-demand revenue claimed from `pallet-on-demand-para` so far, as `(timeslice, amount)`.
type Claims = Vec<(Timeslice, Balance)>;

/// Build a Coretime chain block, running `f` in it, and record the on-demand revenue claimed in
/// it into `claims`.
///
/// Checks that the broker attributes every claim to the timeslice it was claimed for.
fn build_block<R>(claims: &mut Claims, f: impl FnOnce() -> R) -> R {
	type CoretimeEvent = <CoretimeWestend as Chain>::RuntimeEvent;

	CoretimeWestend::execute_with(|| {
		let r = f();

		let broker_account = Broker::account_id();
		let mut claimed = None;
		let mut attributed_to = None;
		for record in <CoretimeWestend as Chain>::System::events() {
			match record.event {
				CoretimeEvent::OnDemand(pallet_on_demand_para::Event::RevenueClaimed {
					when,
					amount,
					beneficiary,
				}) => {
					assert_eq!(beneficiary, broker_account);
					assert!(claimed.is_none(), "at most one timeslice is claimed per block");
					claimed = Some((when, amount));
				},
				CoretimeEvent::Broker(
					pallet_broker::Event::ClaimsReady { when, .. } |
					pallet_broker::Event::HistoryDropped { when, .. },
				) => attributed_to = Some(when),
				_ => {},
			}
		}
		if let Some((when, amount)) = claimed {
			assert_eq!(attributed_to, Some(when), "the broker attributes the revenue to {when}");
			claims.push((when, amount));
		}

		r
	})
}

/// Place an order for `para_id` in a new Coretime chain block, returning the spot price paid and
/// the timeslice the order was placed in.
fn place_order_in_new_block(claims: &mut Claims, para_id: u32) -> (Balance, Timeslice) {
	build_block(claims, || {
		let sender = CoretimeWestendSender::get();
		let before = Balances::free_balance(&sender);
		assert_ok!(OnDemand::place_order(
			<CoretimeWestend as Chain>::RuntimeOrigin::signed(sender.clone()),
			para_id,
			// The queue is never deep enough for the price to double.
			BASE_FEE * 2
		));
		let ordered_at = RelaychainDataProvider::<CoretimeRuntime>::current_block_number();
		(before - Balances::free_balance(&sender), ordered_at / TIMESLICE_PERIOD)
	})
}

/// Build blocks until the broker has ended timeslice `timeslice`.
fn run_until_broker_ends(claims: &mut Claims, timeslice: Timeslice) {
	let mut blocks = 0;
	while broker_last_timeslice() <= timeslice {
		assert!(blocks < TIMESLICE_PERIOD * 2, "the broker never ended timeslice {timeslice}");
		build_block(claims, || {});
		blocks += 1;
	}
}

#[test]
fn on_demand_orders_reach_relay() {
	// RuntimeEvent aliases to avoid warning from usage of qualified paths in assertions due to
	// <https://github.com/rust-lang/rust/issues/86935>
	type CoretimeEvent = <CoretimeWestend as Chain>::RuntimeEvent;
	type RelayEvent = <Westend as Chain>::RuntimeEvent;

	let para_a = 2000;
	let para_b = 2001;
	let price_config = price_config();

	let sender = CoretimeWestendSender::get();
	let pot = CoretimeWestend::ext_wrapper(OnDemand::account_id);

	configure_pool_and_on_demand(price_config);

	// The pool only gets its core once the reserved schedule becomes active.
	CoretimeWestend::execute_with(|| {
		assert_noop!(
			OnDemand::place_order(
				<CoretimeWestend as Chain>::RuntimeOrigin::signed(sender.clone()),
				para_a,
				BASE_FEE
			),
			pallet_on_demand_para::Error::<CoretimeRuntime>::EmptyPool
		);
	});

	wait_for_pool();

	// Place two orders. They are charged the spot price and batched up.
	let revenue = BASE_FEE + BASE_FEE + BASE_FEE / 10;
	let ordered_at = CoretimeWestend::execute_with(|| {
		let origin = <CoretimeWestend as Chain>::RuntimeOrigin::signed(sender.clone());
		let second_price = BASE_FEE + BASE_FEE / 10;
		let before = Balances::free_balance(&sender);

		assert_ok!(OnDemand::place_order(origin.clone(), para_a, BASE_FEE));
		assert_ok!(OnDemand::place_order(origin, para_b, second_price));

		assert_eq!(Balances::free_balance(&sender), before - BASE_FEE - second_price);
		assert_eq!(PendingBatch::<CoretimeRuntime>::get().len(), 2);

		assert_expected_events!(
			CoretimeWestend,
			vec![
				CoretimeEvent::OnDemand(pallet_on_demand_para::Event::OrderPlaced {
					para_id,
					spot_price,
					..
				}) => { para_id: *para_id == para_a, spot_price: *spot_price == BASE_FEE, },
				CoretimeEvent::OnDemand(pallet_on_demand_para::Event::OrderPlaced {
					para_id,
					spot_price,
					..
				}) => { para_id: *para_id == para_b, spot_price: *spot_price == second_price, },
			]
		);

		RelaychainDataProvider::<CoretimeRuntime>::current_block_number()
	});

	// The batch was sent out when the block was finalized, and the revenue is in the pot.
	CoretimeWestend::ext_wrapper(|| {
		assert!(PendingBatch::<CoretimeRuntime>::get().is_empty());
		assert_eq!(Balances::free_balance(&pot), CORETIME_WESTEND_ED + revenue);
	});

	// The relay chain receives the batch and queues both orders.
	Westend::execute_with(|| {
		Westend::assert_ump_queue_processed(true, Some(CoretimeWestend::para_id()), None);

		let expected_batch = vec![(para_a.into(), ordered_at), (para_b.into(), ordered_at)];
		assert_expected_events!(
			Westend,
			vec![
				RelayEvent::OnDemandAssignmentProvider(on_demand::Event::BatchQueued { batch }) => {
					batch: *batch == expected_batch,
				},
			]
		);

		let mut queue = OnDemandAssignmentProvider::peek_order_queue();
		let queued: Vec<_> =
			queue.pop_assignment_for_cores::<WestendRuntime>(ordered_at + 10, 2).collect();
		assert_eq!(queued, vec![para_a.into(), para_b.into()]);
	});

	// Run until the broker claims the revenue of the orders at the start of the next timeslice.
	let broker_account = CoretimeWestend::ext_wrapper(Broker::account_id);
	let ordered_ts = ordered_at / TIMESLICE_PERIOD;
	let mut claimed = None;
	let mut blocks = 0;
	while claimed.is_none() {
		assert!(blocks < TIMESLICE_PERIOD * 2, "the broker never claimed the on-demand revenue");
		CoretimeWestend::execute_with(|| {
			let mut attributed_to = None;
			for record in <CoretimeWestend as Chain>::System::events() {
				match record.event {
					CoretimeEvent::OnDemand(pallet_on_demand_para::Event::RevenueClaimed {
						when,
						amount,
						beneficiary,
					}) => {
						assert_eq!(when, ordered_ts, "the orders' timeslice must be claimed");
						assert_eq!(beneficiary, broker_account);
						claimed = Some(amount);
					},
					CoretimeEvent::Broker(
						pallet_broker::Event::ClaimsReady { when, .. } |
						pallet_broker::Event::HistoryDropped { when, .. },
					) => attributed_to = Some(when),
					_ => {},
				}
			}
			if claimed.is_some() {
				// The revenue is claimed once the timeslice the orders were placed in has ended,
				// and the broker attributes it to that timeslice.
				assert_eq!(attributed_to, Some(ordered_ts));
			}
		});
		Westend::execute_with(|| {});
		blocks += 1;
	}

	// The whole revenue was paid out, and the existential deposit was left untouched.
	assert_eq!(claimed, Some(revenue));
	CoretimeWestend::ext_wrapper(|| {
		assert_eq!(Balances::free_balance(&pot), CORETIME_WESTEND_ED);
	});
}

#[test]
fn on_demand_revenue_is_attributed_to_its_timeslice_when_the_chain_skips_one() {
	let para_id = 2000;
	configure_pool_and_on_demand(price_config());
	wait_for_pool();

	let mut claims = Claims::new();
	let (first, first_ts) = place_order_in_new_block(&mut claims, para_id);

	// The next block is built two timeslices later, so no blocks are built in the timeslice in
	// between. The broker only ends one timeslice per block, so it ends the skipped timeslice
	// while orders are already placed in the one after it.
	jump_relay_chain_to((first_ts + 2) * TIMESLICE_PERIOD + 1);
	let (second, second_ts) = place_order_in_new_block(&mut claims, para_id);
	assert_eq!(second_ts, first_ts + 2);

	run_until_broker_ends(&mut claims, second_ts);

	// Each order's revenue is attributed to the timeslice it was placed in, and nothing to the
	// skipped one.
	assert_eq!(claims, vec![(first_ts, first), (second_ts, second)]);
}

#[test]
fn on_demand_revenue_is_attributed_to_its_timeslice_when_catching_up_crosses_a_boundary() {
	let para_id = 2000;
	configure_pool_and_on_demand(price_config());
	wait_for_pool();

	let mut claims = Claims::new();
	let (first, first_ts) = place_order_in_new_block(&mut claims, para_id);

	// The chain resumes at the very end of a timeslice several timeslices later, so the Relay
	// chain moves on to the next timeslice while the broker is still catching up.
	let resumed_ts = first_ts + 5;
	jump_relay_chain_to((resumed_ts + 1) * TIMESLICE_PERIOD - 1);
	let (second, second_ts) = place_order_in_new_block(&mut claims, para_id);
	assert_eq!(second_ts, resumed_ts);
	let (third, third_ts) = place_order_in_new_block(&mut claims, para_id);
	assert_eq!(third_ts, resumed_ts + 1);
	assert!(broker_last_timeslice() < second_ts, "the broker is still catching up");

	run_until_broker_ends(&mut claims, third_ts);

	// Each order's revenue is attributed to the timeslice it was placed in.
	assert_eq!(claims, vec![(first_ts, first), (second_ts, second), (third_ts, third)]);
}
