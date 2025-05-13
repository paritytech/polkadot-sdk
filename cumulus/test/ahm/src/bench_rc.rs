// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Test AH migrator pallet benchmark functions.

#![cfg(feature = "runtime-benchmarks")]

use pallet_rc_migrator::benchmarking::*;
use polkadot_runtime::{Runtime as RelayChain, System as RcSystem};
use sp_runtime::BuildStorage;

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<RelayChain>::default().build_storage().unwrap();

	pallet_xcm::GenesisConfig::<RelayChain> {
		safe_xcm_version: Some(xcm::latest::VERSION),
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| RcSystem::set_block_number(1));
	ext
}

#[test]
fn test_bench_withdraw_account() {
	new_test_ext().execute_with(|| {
		test_withdraw_account::<RelayChain>();
	});
}

#[test]
fn test_bench_force_set_stage() {
	new_test_ext().execute_with(|| {
		test_force_set_stage::<RelayChain>();
	});
}

#[test]
fn test_bench_schedule_migration() {
	new_test_ext().execute_with(|| {
		test_schedule_migration::<RelayChain>();
	});
}

#[test]
fn test_bench_start_data_migration() {
	new_test_ext().execute_with(|| {
		test_start_data_migration::<RelayChain>();
	});
}

#[test]
fn test_bench_update_ah_msg_processed_count() {
	new_test_ext().execute_with(|| {
		test_update_ah_msg_processed_count::<RelayChain>();
	});
}

#[test]
fn test_bench_send_chunked_xcm_and_track() {
	new_test_ext().execute_with(|| {
		test_send_chunked_xcm_and_track::<RelayChain>();
	});
}
