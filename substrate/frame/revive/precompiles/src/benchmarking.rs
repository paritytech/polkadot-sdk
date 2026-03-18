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

//! Benchmarks for `pallet-revive-precompile-vesting`.

#![cfg(feature = "runtime-benchmarks")]

use crate::{
	Vesting, VestingBalance,
	pallet::{Config, Pallet},
};
use alloy_core::sol_types::SolValue;
use frame_benchmarking::v2::*;
use frame_support::traits::{Currency, VestingSchedule};
use pallet_revive::{
	AddressMapper,
	precompiles::{Precompile, U256},
};
use pallet_revive_uapi::precompiles::vesting::IVesting;
use sp_runtime::traits::Zero;

type CurrencyOf<T> = <T as pallet_vesting::Config>::Currency;

/// Add a single vesting schedule to `who`.
///
/// The schedule locks `locked` tokens over 20 blocks starting at block 0, so at block 0
/// everything is still locked — ideal for benchmarking `vest()`.
fn add_vesting_schedule<T: Config>(who: &T::AccountId, locked: VestingBalance<T>)
where
	VestingBalance<T>: Into<U256>,
	VestingBalance<T>: From<<T as pallet_revive::Config>::Balance>,
	<T as pallet_revive::Config>::Balance: From<VestingBalance<T>>,
{
	let per_block = locked / 20u32.into();
	let starting_block = Zero::zero();

	<pallet_vesting::Pallet<T> as VestingSchedule<T::AccountId>>::add_vesting_schedule(
		who,
		locked,
		per_block,
		starting_block,
	)
	.expect("adding vesting schedule should succeed");
}

#[benchmarks(
	where
		VestingBalance<T>: Into<U256>,
		VestingBalance<T>: From<<T as pallet_revive::Config>::Balance>,
		<T as pallet_revive::Config>::Balance: From<VestingBalance<T>>,
)]
mod benchmarks {
	use super::*;
	fn precompile_address<T: Config>() -> [u8; 20]
	where
		VestingBalance<T>: Into<U256>,
		VestingBalance<T>: From<<T as pallet_revive::Config>::Balance>,
		<T as pallet_revive::Config>::Balance: From<VestingBalance<T>>,
	{
		Vesting::<T>::MATCHER.base_address()
	}

	/// Benchmark `vest()`: unlock vested funds for the caller.
	#[benchmark]
	fn vest() {
		let mut call_setup = pallet_revive::call_builder::CallSetup::<T>::default();
		let caller_account = call_setup.contract().caller.clone();

		// Give the caller a vesting schedule. The caller already has funds from CallSetup.
		let locked: VestingBalance<T> = 10_000u32.into();
		CurrencyOf::<T>::make_free_balance_be(&caller_account, locked * 10u32.into());
		add_vesting_schedule::<T>(&caller_account, locked);

		let input = IVesting::IVestingCalls::vest(IVesting::vestCall {});
		let address = precompile_address::<T>();
		let (mut ext, _) = call_setup.ext();

		let result;
		#[block]
		{
			result = Vesting::<T>::call(&address, &input, &mut ext);
		}
		assert!(result.is_ok());
	}

	/// Benchmark `vestOther(target)`: unlock vested funds for another account.
	#[benchmark]
	fn vest_other() {
		let mut call_setup = pallet_revive::call_builder::CallSetup::<T>::default();

		// Use a distinct target account with a vesting schedule.
		let target_addr = pallet_revive::precompiles::H160::from_low_u64_be(0xBEEF);
		let target_account = T::AddressMapper::to_account_id(&target_addr);
		let locked: VestingBalance<T> = 10_000u32.into();
		CurrencyOf::<T>::make_free_balance_be(&target_account, locked * 10u32.into());
		add_vesting_schedule::<T>(&target_account, locked);

		let input = IVesting::IVestingCalls::vestOther(IVesting::vestOtherCall {
			target: alloy_core::primitives::Address::from_slice(target_addr.as_bytes()),
		});
		let address = precompile_address::<T>();
		let (mut ext, _) = call_setup.ext();

		let result;
		#[block]
		{
			result = Vesting::<T>::call(&address, &input, &mut ext);
		}
		assert!(result.is_ok());
	}

	/// Benchmark `vestingBalance()`: query locked balance for the caller (with schedule).
	#[benchmark]
	fn vesting_balance() {
		let mut call_setup = pallet_revive::call_builder::CallSetup::<T>::default();
		let caller_account = call_setup.contract().caller.clone();

		let locked: VestingBalance<T> = 10_000u32.into();
		CurrencyOf::<T>::make_free_balance_be(&caller_account, locked * 10u32.into());
		add_vesting_schedule::<T>(&caller_account, locked);

		let input = IVesting::IVestingCalls::vestingBalance(IVesting::vestingBalanceCall {});
		let address = precompile_address::<T>();
		let (mut ext, _) = call_setup.ext();

		let result;
		#[block]
		{
			result = Vesting::<T>::call(&address, &input, &mut ext);
		}
		let raw_data = result.unwrap();
		let balance = U256::from_big_endian(&<[u8; 32]>::abi_decode(&raw_data).unwrap());
		assert!(balance > U256::zero(), "locked balance should be non-zero");
	}

	/// Benchmark `vestingBalanceOf(target)`: query locked balance for another account.
	#[benchmark]
	fn vesting_balance_of() {
		let mut call_setup = pallet_revive::call_builder::CallSetup::<T>::default();

		let target_addr = pallet_revive::precompiles::H160::from_low_u64_be(0xBEEF);
		let target_account = T::AddressMapper::to_account_id(&target_addr);
		let locked: VestingBalance<T> = 10_000u32.into();
		CurrencyOf::<T>::make_free_balance_be(&target_account, locked * 10u32.into());
		add_vesting_schedule::<T>(&target_account, locked);

		let input = IVesting::IVestingCalls::vestingBalanceOf(IVesting::vestingBalanceOfCall {
			target: alloy_core::primitives::Address::from_slice(target_addr.as_bytes()),
		});
		let address = precompile_address::<T>();
		let (mut ext, _) = call_setup.ext();

		let result;
		#[block]
		{
			result = Vesting::<T>::call(&address, &input, &mut ext);
		}
		let raw_data = result.unwrap();
		let balance = U256::from_big_endian(&<[u8; 32]>::abi_decode(&raw_data).unwrap());
		assert!(balance > U256::zero(), "locked balance should be non-zero");
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
