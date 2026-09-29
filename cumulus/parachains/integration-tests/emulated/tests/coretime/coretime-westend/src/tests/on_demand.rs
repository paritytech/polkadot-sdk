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
use pallet_broker::{ConfigRecord, CoreAssignment, CoreMask, ScheduleItem, CORE_MASK_BITS};
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

/// The number of cores in the Instantaneous Coretime Pool, as seen by the broker.
fn pool_cores() -> u32 {
	CoretimeWestend::ext_wrapper(|| {
		pallet_broker::Status::<CoretimeRuntime>::get().map_or(0, |status| {
			(status.private_pool_size + status.system_pool_size) / CORE_MASK_BITS as u32
		})
	})
}

#[test]
fn on_demand_orders_reach_relay() {
	// RuntimeEvent aliases to avoid warning from usage of qualified paths in assertions due to
	// <https://github.com/rust-lang/rust/issues/86935>
	type CoretimeEvent = <CoretimeWestend as Chain>::RuntimeEvent;
	type RelayEvent = <Westend as Chain>::RuntimeEvent;

	let para_a = 2000;
	let para_b = 2001;
	let base_fee = CORETIME_WESTEND_ED * 10;
	let price_config = PriceParameters {
		order_cap: 10,
		drain_rate_per_block: 1,
		price_step: Perbill::from_percent(10),
		base_fee,
	};

	let sender = CoretimeWestendSender::get();
	let pot = CoretimeWestend::ext_wrapper(OnDemand::account_id);

	Westend::execute_with(|| {
		Dmp::make_parachain_reachable(CoretimeWestend::para_id());
	});

	// Put a whole core into the Instantaneous Coretime Pool, start sales, and configure the
	// on-demand pallet.
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

		// The pool only gets its core once the reserved schedule becomes active.
		assert_noop!(
			OnDemand::place_order(
				<CoretimeWestend as Chain>::RuntimeOrigin::signed(sender.clone()),
				para_a,
				base_fee
			),
			pallet_on_demand_para::Error::<CoretimeRuntime>::EmptyPool
		);
	});

	// The relay chain processes the core count request.
	Westend::execute_with(|| {
		Westend::assert_ump_queue_processed(true, Some(CoretimeWestend::para_id()), None);
	});

	// Run until the broker has committed the timeslice which puts the core into the pool.
	let mut blocks = 0;
	while pool_cores() == 0 {
		assert!(blocks < TIMESLICE_PERIOD * 10, "the pool never got a core");
		CoretimeWestend::execute_with(|| {});
		Westend::execute_with(|| {});
		blocks += 1;
	}

	// Place two orders. They are charged the spot price and batched up.
	let revenue = base_fee + base_fee + base_fee / 10;
	let ordered_at = CoretimeWestend::execute_with(|| {
		let origin = <CoretimeWestend as Chain>::RuntimeOrigin::signed(sender.clone());
		let second_price = base_fee + base_fee / 10;
		let before = Balances::free_balance(&sender);

		assert_ok!(OnDemand::place_order(origin.clone(), para_a, base_fee));
		assert_ok!(OnDemand::place_order(origin, para_b, second_price));

		assert_eq!(Balances::free_balance(&sender), before - base_fee - second_price);
		assert_eq!(PendingBatch::<CoretimeRuntime>::get().len(), 2);

		assert_expected_events!(
			CoretimeWestend,
			vec![
				CoretimeEvent::OnDemand(pallet_on_demand_para::Event::OrderPlaced {
					para_id,
					spot_price,
					..
				}) => { para_id: *para_id == para_a, spot_price: *spot_price == base_fee, },
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
	let mut claimed = None;
	let mut blocks = 0;
	while claimed.is_none() {
		assert!(blocks < TIMESLICE_PERIOD * 2, "the broker never claimed the on-demand revenue");
		CoretimeWestend::execute_with(|| {
			for record in <CoretimeWestend as Chain>::System::events() {
				if let CoretimeEvent::OnDemand(pallet_on_demand_para::Event::RevenueClaimed {
					until,
					amount,
					beneficiary,
				}) = record.event
				{
					assert!(until > ordered_at, "the orders must be covered by the claim");
					assert_eq!(beneficiary, broker_account);
					claimed = Some(amount);
				}
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
