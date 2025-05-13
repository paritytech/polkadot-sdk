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
use pallet_ah_ops::benchmarking::*;
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

#[test]
fn test_bench_unreserve_lease_deposit() {
	new_test_ext().execute_with(|| {
		test_unreserve_lease_deposit::<AssetHub>();
	});
}

#[test]
fn test_bench_withdraw_crowdloan_contribution() {
	new_test_ext().execute_with(|| {
		test_withdraw_crowdloan_contribution::<AssetHub>();
	});
}

#[test]
fn test_bench_unreserve_crowdloan_reserve() {
	new_test_ext().execute_with(|| {
		test_unreserve_crowdloan_reserve::<AssetHub>();
	});
}
