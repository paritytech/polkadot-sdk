// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Benchmarking setup for pallet-template
use super::*;
#[allow(unused)]
use crate::Pallet as SnowbridgeControlFrontend;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use xcm::prelude::{Location, *};
use sp_runtime::AccountId32;

impl From<AccountId32> for Location {
	fn from(_account_id: AccountId32) -> Self {
		// Use a dummy parachain + account key, suitable for benchmarking
		Location::new(1, [Parachain(1000), AccountKey20 { network: None, key: [0u8; 20] }])
	}
}

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

	#[benchmark]
	fn add_tip() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();

		let message_id = MessageId::Inbound(1);
		let dot = Location::new(1, Here);
		let asset = Asset::from((dot, 1_000_000_000u128));

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), message_id, asset);

		Ok(())
	}

	impl_benchmark_test_suite!(
		SnowbridgeControlFrontend,
		crate::mock::new_test_ext(),
		crate::mock::Test
	);
}
