// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Benchmarking setup for pallet-template
use super::*;

#[allow(unused)]
use crate::Pallet as SnowbridgeControl;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use snowbridge_core::{eth, outbound::OperatingMode};
use sp_runtime::SaturatedConversion;
use xcm::prelude::*;

#[allow(clippy::result_large_err)]
fn fund_sovereign_account<T: Config>(para_id: ParaId) -> Result<(), BenchmarkError> {
	let amount: BalanceOf<T> = (10_000_000_000_000_u64).saturated_into::<u128>().saturated_into();
	let sovereign_account = sibling_sovereign_account::<T>(para_id);
	T::Token::mint_into(&sovereign_account, amount)?;
	Ok(())
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn upgrade() -> Result<(), BenchmarkError> {
		let impl_address = H160::repeat_byte(1);
		let impl_code_hash = H256::repeat_byte(1);

		// Assume 256 bytes passed to initializer
		let params: Vec<u8> = (0..256).map(|_| 1u8).collect();

		#[extrinsic_call]
		_(
			RawOrigin::Root,
			impl_address,
			impl_code_hash,
			Some(Initializer { params, maximum_required_gas: 100000 }),
		);

		Ok(())
	}

	#[benchmark]
	fn set_operating_mode() -> Result<(), BenchmarkError> {
		#[extrinsic_call]
		_(RawOrigin::Root, OperatingMode::RejectingOutboundMessages);

		Ok(())
	}

	#[benchmark]
	fn set_pricing_parameters() -> Result<(), BenchmarkError> {
		let params = T::DefaultPricingParameters::get();

		#[extrinsic_call]
		_(RawOrigin::Root, params);

		Ok(())
	}

	#[benchmark]
	fn create_agent() -> Result<(), BenchmarkError> {
		let origin_para_id = 2000;
		let origin_location = Location::new(1, [Parachain(origin_para_id)]);
		let origin = T::Helper::make_xcm_origin(origin_location);
		fund_sovereign_account::<T>(origin_para_id.into())?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin);

		Ok(())
	}

	#[benchmark]
	fn create_channel() -> Result<(), BenchmarkError> {
		let origin_para_id = 2000;
		let origin_location = Location::new(1, [Parachain(origin_para_id)]);
		let origin = T::Helper::make_xcm_origin(origin_location);
		fund_sovereign_account::<T>(origin_para_id.into())?;

		SnowbridgeControl::<T>::create_agent(origin.clone())?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, OperatingMode::Normal);

		Ok(())
	}

	#[benchmark]
	fn update_channel() -> Result<(), BenchmarkError> {
		let origin_para_id = 2000;
		let origin_location = Location::new(1, [Parachain(origin_para_id)]);
		let origin = T::Helper::make_xcm_origin(origin_location);
		fund_sovereign_account::<T>(origin_para_id.into())?;
		SnowbridgeControl::<T>::create_agent(origin.clone())?;
		SnowbridgeControl::<T>::create_channel(origin.clone(), OperatingMode::Normal)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, OperatingMode::RejectingOutboundMessages);

		Ok(())
	}

	#[benchmark]
	fn force_update_channel() -> Result<(), BenchmarkError> {
		let origin_para_id = 2000;
		let origin_location = Location::new(1, [Parachain(origin_para_id)]);
		let origin = T::Helper::make_xcm_origin(origin_location);
		let channel_id: ChannelId = ParaId::from(origin_para_id).into();

		fund_sovereign_account::<T>(origin_para_id.into())?;
		SnowbridgeControl::<T>::create_agent(origin.clone())?;
		SnowbridgeControl::<T>::create_channel(origin.clone(), OperatingMode::Normal)?;

		#[extrinsic_call]
		_(RawOrigin::Root, channel_id, OperatingMode::RejectingOutboundMessages);

		Ok(())
	}

	#[benchmark]
	fn transfer_native_from_agent() -> Result<(), BenchmarkError> {
		let origin_para_id = 2000;
		let origin_location = Location::new(1, [Parachain(origin_para_id)]);
		let origin = T::Helper::make_xcm_origin(origin_location);
		fund_sovereign_account::<T>(origin_para_id.into())?;
		SnowbridgeControl::<T>::create_agent(origin.clone())?;
		SnowbridgeControl::<T>::create_channel(origin.clone(), OperatingMode::Normal)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, H160::default(), 1);

		Ok(())
	}

	#[benchmark]
	fn force_transfer_native_from_agent() -> Result<(), BenchmarkError> {
		let origin_para_id = 2000;
		let origin_location = Location::new(1, [Parachain(origin_para_id)]);
		let origin = T::Helper::make_xcm_origin(origin_location.clone());
		fund_sovereign_account::<T>(origin_para_id.into())?;
		SnowbridgeControl::<T>::create_agent(origin.clone())?;

		let versioned_location: VersionedLocation = origin_location.into();

		#[extrinsic_call]
		_(RawOrigin::Root, Box::new(versioned_location), H160::default(), 1);

		Ok(())
	}

	#[benchmark]
	fn set_token_transfer_fees() -> Result<(), BenchmarkError> {
		#[extrinsic_call]
		_(RawOrigin::Root, 1, 1, eth(1));

		Ok(())
	}

	#[benchmark]
	fn register_token() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();

		let amount: BalanceOf<T> =
			(10_000_000_000_000_u128).saturated_into::<u128>().saturated_into();

		T::Token::mint_into(&caller, amount)?;

		let relay_token_asset_id: Location = Location::parent();
		let asset = Box::new(VersionedLocation::from(relay_token_asset_id));
		let asset_metadata = AssetMetadata {
			name: "wnd".as_bytes().to_vec().try_into().unwrap(),
			symbol: "wnd".as_bytes().to_vec().try_into().unwrap(),
			decimals: 12,
		};

		#[extrinsic_call]
		_(RawOrigin::Root, asset, asset_metadata);

		Ok(())
	}

	impl_benchmark_test_suite!(
		SnowbridgeControl,
		crate::mock::new_test_ext(true),
		crate::mock::Test
	);
}
