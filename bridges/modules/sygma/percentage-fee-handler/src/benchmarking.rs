// The Licensed Work is (c) 2022 Sygma
// SPDX-License-Identifier: LGPL-3.0-only

//! Sygma percentage-fee-handler pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]
use super::*;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin as SystemOrigin;

use sp_std::vec;
use sygma_traits::DomainID;
use xcm::latest::prelude::*;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn set_fee_rate() {
		let dest_domain_id: DomainID = 1;
		let native_location: MultiLocation = MultiLocation::here();
		let fee_rate = 500u32; // 5%

		#[extrinsic_call]
		set_fee_rate(
			SystemOrigin::Root,
			dest_domain_id,
			Box::new(native_location.clone().into()),
			fee_rate,
			0u128,
			100_000_000_000_000u128,
		);

		assert_eq!(
			AssetFeeRate::<T>::get(&(dest_domain_id, native_location.into())),
			Some((fee_rate, 0u128, 100_000_000_000_000u128)),
		);
	}
}
