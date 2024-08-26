// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
#![cfg(all(test, not(feature = "runtime-benchmarks")))]

use crate::{
	migration::v0::{
		CompactExecutionHeader, ExecutionHeaderIndex, ExecutionHeaderMapping, ExecutionHeaderState,
		ExecutionHeaders, LatestExecutionState,
	},
	mock::{
		new_tester, run_to_block_with_migrator, AllPalletsWithSystem, ExecutionHeaderCount,
		MigratorServiceWeight, System, Test,
	},
	pallet,
	weights::WeightInfo as _,
};
use frame_support::traits::OnRuntimeUpgrade;
use pallet_migrations::WeightInfo as _;
use snowbridge_ethereum::H256;

#[test]
fn ethereum_execution_header_migration_works() {
	new_tester().execute_with(|| {
		frame_support::__private::sp_tracing::try_init_simple();
		// Insert some values into the old storage items.
		LatestExecutionState::<Test>::set(ExecutionHeaderState {
			beacon_block_root: H256::random(),
			beacon_slot: 5353,
			block_hash: H256::random(),
			block_number: 5454,
		});
		ExecutionHeaderIndex::<Test>::set(5500);

		let execution_header_count = 5500;

		let mut block_roots: Vec<H256> = vec![];
		for index in 0..execution_header_count {
			let block_root = H256::random();
			ExecutionHeaders::<Test>::insert(
				block_root,
				CompactExecutionHeader {
					parent_hash: H256::random(),
					block_number: index,
					state_root: H256::random(),
					receipts_root: H256::random(),
				},
			);
			ExecutionHeaderMapping::<Test>::insert(index as u32, block_root);
			block_roots.push(block_root);
		}

		// Give it enough weight to do 16 iterations:
		let limit = <Test as pallet_migrations::Config>::WeightInfo::progress_mbms_none() +
			pallet_migrations::Pallet::<Test>::exec_migration_max_weight() +
			<Test as pallet::Config>::WeightInfo::step() * 16;
		MigratorServiceWeight::set(&limit);
		ExecutionHeaderCount::set(&(execution_header_count as u32));

		System::set_block_number(1);
		AllPalletsWithSystem::on_runtime_upgrade(); // onboard MBMs

		// Check everything is empty
		for index in 0..execution_header_count {
			run_to_block_with_migrator(index + 2);
			let block_root_hash = block_roots.get(index as usize).unwrap();
			assert_eq!(ExecutionHeaderMapping::<Test>::get(index as u32), H256::zero());
			assert!(ExecutionHeaders::<Test>::get(block_root_hash).is_none());
		}
	});
}
