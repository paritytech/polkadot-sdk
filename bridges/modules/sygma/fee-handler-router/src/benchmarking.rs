// The Licensed Work is (c) 2022 Sygma
// SPDX-License-Identifier: LGPL-3.0-only

//! Sygma basic-fee-handler pallet benchmarking.

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
	fn set_fee_handler() {
		let dest_domain_id: DomainID = 1;
		let native_location: MultiLocation = MultiLocation::here();

		#[extrinsic_call]
		set_fee_handler(
			SystemOrigin::Root,
			dest_domain_id,
			Box::new(native_location.clone().into()),
			FeeHandlerType::BasicFeeHandler,
		);

		assert_eq!(
			HandlerType::<T>::get(&(dest_domain_id, native_location.into())),
			Some(FeeHandlerType::BasicFeeHandler),
		);
	}
}
