// The Licensed Work is (c) 2022 Sygma
// SPDX-License-Identifier: LGPL-3.0-only

//! Sygma access-segreator pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]
use super::*;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin as SystemOrigin;

use sp_std::vec;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn grant_access() {
		let caller: <T as frame_system::Config>::AccountId = whitelisted_caller();

		#[extrinsic_call]
		grant_access(SystemOrigin::Root, 100, b"grant_access".to_vec(), caller.clone());

		assert_eq!(
			ExtrinsicAccess::<T>::get(&(100, b"grant_access".to_vec())),
			Some(caller).into(),
		);
	}
}
