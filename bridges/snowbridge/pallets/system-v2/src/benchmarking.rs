// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Benchmarking setup for pallet-template
use super::*;

#[allow(unused)]
use crate::Pallet as SnowbridgeControl;
use frame_benchmarking::v2::*;
use xcm::prelude::*;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn create_agent() -> Result<(), BenchmarkError> {
		let origin_location = Location::new(1, [Parachain(1000)]);
		let origin = T::Helper::make_xcm_origin(origin_location);

		let agent_origin = Box::new(VersionedLocation::from(Location::parent()));

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, agent_origin, 100);

		Ok(())
	}

	#[benchmark]
	fn register_token() -> Result<(), BenchmarkError> {
		let origin_location = Location::new(1, [Parachain(1000)]);
		let origin = T::Helper::make_xcm_origin(origin_location);

		let relay_token_asset_id: Location = Location::parent();
		let asset = Box::new(VersionedLocation::from(relay_token_asset_id));
		let asset_metadata = AssetMetadata {
			name: "wnd".as_bytes().to_vec().try_into().unwrap(),
			symbol: "wnd".as_bytes().to_vec().try_into().unwrap(),
			decimals: 12,
		};

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, asset, asset_metadata, 100);

		Ok(())
	}

	impl_benchmark_test_suite!(
		SnowbridgeControl,
		crate::mock::new_test_ext(true),
		crate::mock::Test
	);
}
