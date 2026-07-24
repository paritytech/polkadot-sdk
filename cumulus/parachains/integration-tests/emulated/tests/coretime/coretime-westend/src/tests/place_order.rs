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
use frame_support::traits::OnInitialize;
use pallet_broker::{ConfigRecord, TaskId};
use sp_runtime::Perbill;

#[test]
fn on_demand_order_placed_via_coretime_chain() {
	// An on-demand order placed on the Coretime chain is paid for locally and forwarded to the
	// Relay-chain via UMP, where it is enqueued into the on-demand order queue without any
	// further payment.

	// RuntimeEvent aliases to avoid warning from usage of qualified paths in assertions due to
	// <https://github.com/rust-lang/rust/issues/86935>
	type CoretimeEvent = <CoretimeWestend as Chain>::RuntimeEvent;
	type RelayEvent = <Westend as Chain>::RuntimeEvent;

	let para_id: TaskId = 2000;
	let sender = CoretimeWestendSender::get();

	// Configure the broker with a non-zero on-demand base fee, start sales and place the order.
	CoretimeWestend::execute_with(|| {
		// Hooks don't run in emulated tests - workaround as we need `on_initialize` to tick
		// things along and have no concept of time passing otherwise.
		<CoretimeWestend as CoretimeWestendPallet>::Broker::on_initialize(
			<CoretimeWestend as Chain>::System::block_number(),
		);

		let coretime_root_origin = <CoretimeWestend as Chain>::RuntimeOrigin::root();

		let config = ConfigRecord {
			advance_notice: 1,
			interlude_length: 1,
			leadin_length: 2,
			region_length: 1,
			ideal_bulk_proportion: Perbill::from_percent(40),
			limit_cores_offered: None,
			renewal_bump: Perbill::from_percent(2),
			contribution_timeout: 1,
			// Above the existential deposit so that the payment can create the pot account.
			on_demand_base_fee: CORETIME_WESTEND_ED * 100,
			on_demand_queue_max_size: 100,
			on_demand_target_queue_utilization: Perbill::from_percent(25),
			on_demand_fee_variability: Perbill::from_percent(3),
		};
		assert_ok!(<CoretimeWestend as CoretimeWestendPallet>::Broker::configure(
			coretime_root_origin.clone(),
			config
		));
		assert_ok!(<CoretimeWestend as CoretimeWestendPallet>::Broker::start_sales(
			coretime_root_origin,
			100,
			0
		));

		// Place the order: the caller is charged locally and the order goes out via UMP.
		assert_ok!(<CoretimeWestend as CoretimeWestendPallet>::Broker::place_order(
			<CoretimeWestend as Chain>::RuntimeOrigin::signed(sender.clone()),
			CORETIME_WESTEND_ED * 1_000,
			para_id,
			sender.clone(),
		));

		assert_expected_events!(
			CoretimeWestend,
			vec![
				CoretimeEvent::Broker(
					pallet_broker::Event::OnDemandOrderPlaced { .. }
				) => {},
				CoretimeEvent::ParachainSystem(
					cumulus_pallet_parachain_system::Event::UpwardMessageSent { .. }
				) => {},
			]
		);
	});

	// The Relay-chain processes the order and enqueues it, without charging anything there.
	Westend::execute_with(|| {
		Westend::assert_ump_queue_processed(true, Some(CoretimeWestend::para_id()), None);

		assert_expected_events!(
			Westend,
			vec![
				RelayEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
				RelayEvent::OnDemandAssignmentProvider(
					polkadot_runtime_parachains::on_demand::Event::OnDemandOrderPlaced { .. }
				) => {},
			]
		);
	});
}
