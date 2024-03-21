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

use crate::*;
use coretime_rococo_runtime::RuntimeEvent;
use frame_support::traits::{Get, OnInitialize};
use pallet_broker::{ConfigRecord, CoreMask, Finality, RegionId};
use parachains_common::BlockNumber;
use sp_runtime::Perbill;

fn advance_to(b: BlockNumber) {
	type CoretimeSystem = <CoretimeRococo as Chain>::System;

	while CoretimeSystem::block_number() < b {
		CoretimeSystem::set_block_number(CoretimeSystem::block_number() + 1);
		<CoretimeRococo as CoretimeRococoPallet>::Broker::on_initialize(
			CoretimeSystem::block_number(),
		);
	}
}

#[test]
fn assign_core_transact_has_correct_weight() {
	type BrokerPallet = <CoretimeRococo as CoretimeRococoPallet>::Broker;

	let coretime_root_origin = <CoretimeRococo as Chain>::RuntimeOrigin::root();
	let sender_origin =
		<CoretimeRococo as Chain>::RuntimeOrigin::signed(CoretimeRococoSender::get().into());

	let timeslice_period: u32 =
		<<CoretimeRococo as Chain>::Runtime as pallet_broker::Config>::TimeslicePeriod::get();

	CoretimeRococo::execute_with(|| {
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
		assert_ok!(BrokerPallet::configure(coretime_root_origin.clone(), config.clone()));
		assert_ok!(BrokerPallet::start_sales(coretime_root_origin, 100, 1));

		// Advance past interlude of first sale.
		advance_to(config.interlude_length * timeslice_period);

		// Purchase region.
		let region_id = RegionId { begin: 1, core: 0, mask: CoreMask::complete() };
		assert_ok!(BrokerPallet::purchase(sender_origin.clone(), 200));

		// Assign the region, this sends the XCM with a `Transact`
		assert_ok!(BrokerPallet::assign(
			sender_origin,
			region_id,
			PenpalA::para_id().into(),
			Finality::Final
		));

		assert_expected_events!(
			CoretimeRococo,
			vec![
				RuntimeEvent::Broker(pallet_broker::Event::Assigned { region_id, duration, task }) => {
					region_id: region_id == region_id,
					duration: *duration == 1,
					task: *task == u32::from(PenpalA::para_id()),
				},
			]
		);
		CoretimeRococo::assert_xcm_pallet_sent();
	});

	Rococo::execute_with(|| {
		Rococo::assert_ump_queue_processed(
			true,
			Some(CoretimeRococo::para_id()),
			None, // for now
		)
	});
}
