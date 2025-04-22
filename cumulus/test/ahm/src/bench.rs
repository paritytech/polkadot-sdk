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

use asset_hub_polkadot_runtime::{Runtime as AssetHub, System as AssetHubSystem};
use pallet_ah_migrator::benchmarking::*;
use sp_runtime::BuildStorage;

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<AssetHub>::default().build_storage().unwrap();

	pallet_xcm::GenesisConfig::<AssetHub> {
		safe_xcm_version: Some(xcm::latest::VERSION),
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| AssetHubSystem::set_block_number(1));
	ext
}

const BENCHMARK_N: u32 = 10;

#[test]
fn test_bench_receive_preimage_chunk() {
	use pallet_rc_migrator::preimage::{alias::MAX_SIZE, chunks::CHUNK_SIZE};

	assert_eq!(
		MAX_SIZE / CHUNK_SIZE,
		84,
		"upper bound of `m` for `receive_preimage_chunk` benchmark should be updated"
	);

	new_test_ext().execute_with(|| {
		test_receive_preimage_chunk::<AssetHub>(1);
		test_receive_preimage_chunk::<AssetHub>(3);
		test_receive_preimage_chunk::<AssetHub>(80);
	});
}

#[test]
fn test_bench_receive_multisigs() {
	new_test_ext().execute_with(|| {
		test_receive_multisigs::<AssetHub>(BENCHMARK_N);
	});
}

#[test]
fn test_bench_on_finalize() {
	new_test_ext().execute_with(|| {
		test_on_finalize::<AssetHub>();
	});
}

#[test]
fn test_bench_receive_proxy_proxies() {
	new_test_ext().execute_with(|| {
		test_receive_proxy_proxies::<AssetHub>(BENCHMARK_N);
	});
}

#[test]
fn test_bench_receive_proxy_announcements() {
	new_test_ext().execute_with(|| {
		test_receive_proxy_announcements::<AssetHub>(BENCHMARK_N);
	});
}

#[test]
fn test_bench_receive_claims() {
	new_test_ext().execute_with(|| {
		test_receive_claims::<AssetHub>(BENCHMARK_N);
	});
}

#[test]
fn test_bench_receive_nom_pools_messages() {
	new_test_ext().execute_with(|| {
		test_receive_nom_pools_messages::<AssetHub>(BENCHMARK_N);
	});
}

#[test]
fn test_bench_receive_vesting_schedules() {
	new_test_ext().execute_with(|| {
		test_receive_vesting_schedules::<AssetHub>(BENCHMARK_N);
	});
}

#[test]
fn test_bench_receive_fast_unstake_messages() {
	new_test_ext().execute_with(|| {
		test_receive_fast_unstake_messages::<AssetHub>(BENCHMARK_N);
	});
}

#[test]
fn test_bench_receive_referenda_values() {
	new_test_ext().execute_with(|| {
		test_receive_referenda_values::<AssetHub>();
	});
}

#[test]
fn test_bench_receive_single_active_referendums() {
	new_test_ext().execute_with(|| {
		test_receive_single_active_referendums::<AssetHub>(BENCHMARK_N);
	});
}

#[test]
fn test_bench_receive_complete_referendums() {
	new_test_ext().execute_with(|| {
		test_receive_complete_referendums::<AssetHub>(BENCHMARK_N);
	});
}

#[test]
fn test_bench_receive_accounts() {
	new_test_ext().execute_with(|| {
		test_receive_accounts::<AssetHub>(BENCHMARK_N);
	});
}

#[test]
fn test_bench_receive_liquid_accounts() {
	new_test_ext().execute_with(|| {
		test_receive_liquid_accounts::<AssetHub>(BENCHMARK_N);
	});
}

#[test]
fn test_bench_receive_single_scheduler_agenda() {
	new_test_ext().execute_with(|| {
		test_receive_single_scheduler_agenda::<AssetHub>(BENCHMARK_N);
	});
}

#[test]
fn test_bench_receive_scheduler_lookup() {
	new_test_ext().execute_with(|| {
		test_receive_scheduler_lookup::<AssetHub>(BENCHMARK_N);
	});
}

#[test]
fn test_bench_receive_bags_list_messages() {
	new_test_ext().execute_with(|| {
		test_receive_bags_list_messages::<AssetHub>(BENCHMARK_N);
	});
}

#[test]
fn test_bench_receive_indices() {
	new_test_ext().execute_with(|| {
		test_receive_indices::<AssetHub>(BENCHMARK_N);
	});
}

#[test]
fn test_bench_receive_conviction_voting_messages() {
	new_test_ext().execute_with(|| {
		test_receive_conviction_voting_messages::<AssetHub>(BENCHMARK_N);
	});
}

#[test]
fn test_bench_receive_bounties_messages() {
	new_test_ext().execute_with(|| {
		test_receive_bounties_messages::<AssetHub>(BENCHMARK_N);
	});
}

#[test]
fn test_bench_receive_asset_rates() {
	new_test_ext().execute_with(|| {
		test_receive_asset_rates::<AssetHub>(BENCHMARK_N);
	});
}

#[test]
fn test_bench_receive_crowdloan_messages() {
	new_test_ext().execute_with(|| {
		test_receive_crowdloan_messages::<AssetHub>(BENCHMARK_N);
	});
}

#[test]
fn test_bench_receive_referenda_metadata() {
	new_test_ext().execute_with(|| {
		test_receive_referenda_metadata::<AssetHub>(BENCHMARK_N);
	});
}

#[test]
fn test_bench_receive_treasury_messages() {
	new_test_ext().execute_with(|| {
		test_receive_treasury_messages::<AssetHub>(BENCHMARK_N);
	});
}

#[test]
fn test_bench_force_set_stage() {
	new_test_ext().execute_with(|| {
		test_force_set_stage::<AssetHub>();
	});
}

#[test]
fn test_bench_start_migration() {
	new_test_ext().execute_with(|| {
		test_start_migration::<AssetHub>();
	});
}

#[test]
fn test_bench_finish_migration() {
	new_test_ext().execute_with(|| {
		test_finish_migration::<AssetHub>();
	});
}

#[test]
fn test_bench_receive_preimage_legacy_status() {
	new_test_ext().execute_with(|| {
		test_receive_preimage_legacy_status::<AssetHub>(BENCHMARK_N);
	});
}

#[test]
fn test_bench_receive_preimage_request_status() {
	new_test_ext().execute_with(|| {
		test_receive_preimage_request_status::<AssetHub>(BENCHMARK_N);
	});
}
