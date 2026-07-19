// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Benchmarks for `pallet-footprint`.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{v2::*, BenchmarkError};
use frame_support::traits::tokens::fungible::Mutate;
use frame_system::RawOrigin;
use sp_runtime::traits::Bounded;

fn fund_account<T: Config>(who: &T::AccountId) {
	let balance = BalanceOf::<T>::max_value() / 100u32.into();
	let _ = T::Currency::set_balance(who, balance);
}

#[benchmarks]
mod benchmarks {
	use super::*;

	/// Adjust an existing purchased-allowance hold through the lowering path.
	#[benchmark]
	fn set_purchased() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let existing = T::MaxPurchased::get();
		if existing == 0 {
			return Err(BenchmarkError::Skip);
		}

		fund_account::<T>(&caller);
		Pallet::<T>::set_purchased(RawOrigin::Signed(caller.clone()).into(), existing)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), existing - 1);

		Ok(())
	}

	/// Claim a benchmark-created base allowance using a successful claim origin.
	#[benchmark]
	fn claim_base() -> Result<(), BenchmarkError> {
		let token = T::BaseAllowance::create_token().ok_or(BenchmarkError::Skip)?;
		if T::BaseAllowance::base_allowance(&token).is_none() {
			return Err(BenchmarkError::Skip);
		}
		let origin =
			T::ClaimOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let target: T::AccountId = account("target", 0, 0);

		#[extrinsic_call]
		_(origin, target);

		Ok(())
	}

	/// Revalidate an existing base claim.
	///
	/// Providers expose only a way to create a valid benchmark token, so generic runtimes cannot
	/// force revocation through this abstraction. Provider benchmark implementations may make the
	/// token invalid before this call to exercise the revocation branch.
	#[benchmark]
	fn revalidate_base() -> Result<(), BenchmarkError> {
		let token = T::BaseAllowance::create_token().ok_or(BenchmarkError::Skip)?;
		if T::BaseAllowance::base_allowance(&token).is_none() {
			return Err(BenchmarkError::Skip);
		}
		let claim_origin =
			T::ClaimOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let target: T::AccountId = account("target", 0, 0);
		Pallet::<T>::claim_base(claim_origin, target.clone())?;
		let caller: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), target);

		Ok(())
	}

	impl_benchmark_test_suite! {
		Pallet,
		crate::mock::ExtBuilder::default().build(),
		crate::mock::Test
	}
}
