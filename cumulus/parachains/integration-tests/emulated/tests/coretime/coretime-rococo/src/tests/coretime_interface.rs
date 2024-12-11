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
use pallet_broker::{ConfigRecord, Configuration, CoreAssignment, CoreMask, ScheduleItem};
use rococo_runtime_constants::system_parachain::coretime::TIMESLICE_PERIOD;
use sp_runtime::Perbill;

#[test]
fn transact_hardcoded_weights_are_sane() {
	// There are three transacts with hardcoded weights sent from the Coretime Chain to the Relay
	// Chain across the CoretimeInterface which are triggered at various points in the sales cycle.
	// - Request core count - triggered directly by `start_sales` or `request_core_count`
	//   extrinsics.
	// - Request revenue info - triggered when each timeslice is committed.
	// - Assign core - triggered when an entry is encountered in the workplan for the next
	//   timeslice.

	// RuntimeEvent aliases to avoid warning from usage of qualified paths in assertions due to
	// <https://github.com/rust-lang/rust/issues/86935>
	type CoretimeEvent = <CoretimeRococo as Chain>::RuntimeEvent;
	type RelayEvent = <Rococo as Chain>::RuntimeEvent;

	// Reserve a workload, configure broker and start sales.
	CoretimeRococo::execute_with(|| {
		// Hooks don't run in emulated tests - workaround as we need `on_initialize` to tick things
		// along and have no concept of time passing otherwise.
		<CoretimeRococo as CoretimeRococoPallet>::Broker::on_initialize(
			<CoretimeRococo as Chain>::System::block_number(),
		);

		let coretime_root_origin = <CoretimeRococo as Chain>::RuntimeOrigin::root();

		// Create and populate schedule with the worst case assignment on this core.
		let mut schedule = Vec::new();
		for i in 0..80 {
			schedule.push(ScheduleItem {
				mask: CoreMask::void().set(i),
				assignment: CoreAssignment::Task(2000 + i),
			})
		}

		assert_ok!(<CoretimeRococo as CoretimeRococoPallet>::Broker::reserve(
			coretime_root_origin.clone(),
			schedule.try_into().expect("Vector is within bounds."),
		));

		// Configure broker and start sales.
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
		assert_ok!(<CoretimeRococo as CoretimeRococoPallet>::Broker::configure(
			coretime_root_origin.clone(),
			config
		));
		assert_ok!(<CoretimeRococo as CoretimeRococoPallet>::Broker::start_sales(
			coretime_root_origin,
			100,
			0
		));
		assert_eq!(
			pallet_broker::Status::<<CoretimeRococo as Chain>::Runtime>::get()
				.unwrap()
				.core_count,
			1
		);

		assert_expected_events!(
			CoretimeRococo,
			vec![
				CoretimeEvent::Broker(
					pallet_broker::Event::ReservationMade { .. }
				) => {},
				CoretimeEvent::Broker(
					pallet_broker::Event::CoreCountRequested { core_count: 1 }
				) => {},
				CoretimeEvent::ParachainSystem(
					cumulus_pallet_parachain_system::Event::UpwardMessageSent { .. }
				) => {},
			]
		);
	});

	// Check that the request_core_count message was processed successfully. This will fail if the
	// weights are misconfigured.
	Rococo::execute_with(|| {
		Rococo::assert_ump_queue_processed(true, Some(CoretimeRococo::para_id()), None);

		assert_expected_events!(
			Rococo,
			vec![
				RelayEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
			]
		);
	});

	// Keep track of the relay chain block number so we can fast forward while still checking the
	// right block.
	let mut block_number_cursor = Rococo::ext_wrapper(<Rococo as Chain>::System::block_number);

	let config = CoretimeRococo::ext_wrapper(|| {
		Configuration::<<CoretimeRococo as Chain>::Runtime>::get()
			.expect("Pallet was configured earlier.")
	});

	// Now run up to the block before the sale is rotated.
	while block_number_cursor < TIMESLICE_PERIOD - config.advance_notice - 1 {
		CoretimeRococo::execute_with(|| {
			// Hooks don't run in emulated tests - workaround.
			<CoretimeRococo as CoretimeRococoPallet>::Broker::on_initialize(
				<CoretimeRococo as Chain>::System::block_number(),
			);
		});

		Rococo::ext_wrapper(|| {
			block_number_cursor = <Rococo as Chain>::System::block_number();
		});
	}

	// In this block we trigger assign core.
	CoretimeRococo::execute_with(|| {
		// Hooks don't run in emulated tests - workaround.
		<CoretimeRococo as CoretimeRococoPallet>::Broker::on_initialize(
			<CoretimeRococo as Chain>::System::block_number(),
		);

		assert_expected_events!(
			CoretimeRococo,
			vec![
				CoretimeEvent::Broker(
					pallet_broker::Event::SaleInitialized { .. }
				) => {},
				CoretimeEvent::Broker(
					pallet_broker::Event::CoreAssigned { .. }
				) => {},
				CoretimeEvent::ParachainSystem(
					cumulus_pallet_parachain_system::Event::UpwardMessageSent { .. }
				) => {},
			]
		);
	});

	// Check that the assign_core message was processed successfully.
	// This will fail if the weights are misconfigured.
	Rococo::execute_with(|| {
		Rococo::assert_ump_queue_processed(true, Some(CoretimeRococo::para_id()), None);

		assert_expected_events!(
			Rococo,
			vec![
				RelayEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
				RelayEvent::Coretime(
					polkadot_runtime_parachains::coretime::Event::CoreAssigned { .. }
				) => {},
			]
		);
	});

	// In this block we trigger request revenue.
	CoretimeRococo::execute_with(|| {
		// Hooks don't run in emulated tests - workaround.
		<CoretimeRococo as CoretimeRococoPallet>::Broker::on_initialize(
			<CoretimeRococo as Chain>::System::block_number(),
		);

		assert_expected_events!(
			CoretimeRococo,
			vec![
				CoretimeEvent::ParachainSystem(
					cumulus_pallet_parachain_system::Event::UpwardMessageSent { .. }
				) => {},
			]
		);
	});

	// Check that the request_revenue_info_at message was processed successfully.
	// This will fail if the weights are misconfigured.
	Rococo::execute_with(|| {
		Rococo::assert_ump_queue_processed(true, Some(CoretimeRococo::para_id()), None);

		assert_expected_events!(
			Rococo,
			vec![
				RelayEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
			]
		);
	});

	// Here we receive and process the notify_revenue XCM with zero revenue.
	CoretimeRococo::execute_with(|| {
		// Hooks don't run in emulated tests - workaround.
		<CoretimeRococo as CoretimeRococoPallet>::Broker::on_initialize(
			<CoretimeRococo as Chain>::System::block_number(),
		);

		assert_expected_events!(
			CoretimeRococo,
			vec![
				CoretimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
				// Zero revenue in first timeslice so history is immediately dropped.
				CoretimeEvent::Broker(
					pallet_broker::Event::HistoryDropped { when: 0, revenue: 0 }
				) => {},
			]
		);
	});
}
