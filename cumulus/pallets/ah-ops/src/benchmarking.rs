// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::*;
use frame_benchmarking::{account, v2::*};
use frame_support::{dispatch::RawOrigin, traits::Currency};

#[benchmarks]
pub mod benchmarks {
	use super::*;

	#[benchmark]
	fn unreserve_lease_deposit() {
		let sender = account("sender", 0, 0);
		let ed = <T::Currency as Currency<_>>::minimum_balance();
		let _ = T::Currency::deposit_creating(&sender, ed + ed);
		let _ = T::Currency::reserve(&sender, ed);
		let block = T::RcBlockNumberProvider::current_block_number();
		let para_id = ParaId::from(1u16);
		RcLeaseReserve::<T>::insert((block, para_id, &sender), ed);

		assert_eq!(T::Currency::reserved_balance(&sender), ed);

		#[extrinsic_call]
		_(RawOrigin::Signed(sender.clone()), block, None, para_id);

		assert_eq!(T::Currency::reserved_balance(&sender), 0);
		assert_eq!(RcLeaseReserve::<T>::get((block, para_id, &sender)), None);
	}

	#[benchmark]
	fn withdraw_crowdloan_contribution() {
		let pot = account("pot", 0, 0);
		let ed = <T::Currency as Currency<_>>::minimum_balance();
		let _ = T::Currency::deposit_creating(&pot, ed + ed);
		let _ = T::Currency::reserve(&pot, ed);
		let block = T::RcBlockNumberProvider::current_block_number();
		let para_id = ParaId::from(1u16);
		RcLeaseReserve::<T>::insert((block, para_id, &pot), ed);

		let sender = account("sender", 0, 0);
		RcCrowdloanContribution::<T>::insert((block, para_id, &sender), (pot.clone(), ed));

		assert_eq!(T::Currency::free_balance(&sender), 0);

		#[extrinsic_call]
		_(RawOrigin::Signed(sender.clone()), block, None, para_id);

		assert_eq!(RcCrowdloanContribution::<T>::get((block, para_id, &sender)), None);
		assert_eq!(RcLeaseReserve::<T>::get((block, para_id, &pot)), None);
		assert_eq!(T::Currency::free_balance(&pot), ed);
	}

	#[benchmark]
	fn unreserve_crowdloan_reserve() {
		let sender = account("sender", 0, 0);
		let ed = <T::Currency as Currency<_>>::minimum_balance();
		let _ = T::Currency::deposit_creating(&sender, ed + ed);
		let _ = T::Currency::reserve(&sender, ed);
		let block = T::RcBlockNumberProvider::current_block_number();
		let para_id = ParaId::from(1u16);
		RcCrowdloanReserve::<T>::insert((block, para_id, &sender), ed);

		assert_eq!(T::Currency::reserved_balance(&sender), ed);

		#[extrinsic_call]
		_(RawOrigin::Signed(sender.clone()), block, None, para_id);

		assert_eq!(T::Currency::reserved_balance(&sender), 0);
		assert_eq!(RcCrowdloanReserve::<T>::get((block, para_id, &sender)), None);
	}

	#[cfg(feature = "std")]
	pub fn test_unreserve_lease_deposit<T: Config>() {
		_unreserve_lease_deposit::<T>(true)
	}

	#[cfg(feature = "std")]
	pub fn test_withdraw_crowdloan_contribution<T: Config>() {
		_withdraw_crowdloan_contribution::<T>(true)
	}

	#[cfg(feature = "std")]
	pub fn test_unreserve_crowdloan_reserve<T: Config>() {
		_unreserve_crowdloan_reserve::<T>(true)
	}
}
