// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Benchmarking setup for pallet-template
use super::*;
#[allow(unused)]
use crate::Pallet as SnowbridgeControlFrontend;
use frame_benchmarking::v2::*;
use xcm::prelude::{Location, *};

#[benchmarks]
mod benchmarks {
	use super::*;
	#[benchmark]
	fn register_token() -> Result<(), BenchmarkError> {
		let origin_location = Location::new(1, [Parachain(2000)]);
		let origin = T::Helper::make_xcm_origin(origin_location.clone());

		let asset_location: Location = Location::new(1, [Parachain(2000), GeneralIndex(1)]);
		let asset_id = Box::new(VersionedLocation::from(asset_location.clone()));
		T::Helper::initialize_storage(asset_location, origin_location);

		let asset_metadata = AssetMetadata {
			name: "pal".as_bytes().to_vec().try_into().unwrap(),
			symbol: "pal".as_bytes().to_vec().try_into().unwrap(),
			decimals: 12,
		};

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, asset_id, asset_metadata);

		Ok(())
	}

	impl_benchmark_test_suite!(
		SnowbridgeControlFrontend,
		crate::mock::new_test_ext(),
		crate::mock::Test
	);
}
