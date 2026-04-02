// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use alloy_core::sol_types::SolValue;
use core::{marker::PhantomData, num::NonZero};
use frame_support::{dispatch::GetDispatchInfo, traits::VestingSchedule};
use pallet_revive::{
	Config,
	precompiles::{AddressMatcher, Error, Ext, H160, Precompile, RuntimeCosts, U256},
};
use sp_runtime::traits::StaticLookup;

alloy_core::sol!("IVesting.sol");

pub use pallet::Pallet;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

#[cfg(all(test, feature = "runtime-benchmarks"))]
pub mod mock;

#[cfg(all(test, feature = "runtime-benchmarks"))]
mod tests;

fn ensure_mutable<T: Config>(env: &impl Ext<T = T>) -> Result<(), Error> {
	if env.is_read_only() {
		return Err(pallet_revive::Error::<T>::StateChangeDenied.into());
	}
	if env.is_delegate_call() {
		return Err(pallet_revive::Error::<T>::PrecompileDelegateDenied.into());
	}
	Ok(())
}

fn caller_account_id<T: Config>(
	env: &impl Ext<T = T>,
	context: &str,
) -> Result<T::AccountId, Error> {
	env.caller()
		.account_id()
		.map_err(|e| {
			Error::Revert(alloc::format!("{context}: caller has no account id: {e:?}").into())
		})
		.cloned()
}

/// Minimal pallet providing a `Pallet<T>` type for the FRAME benchmarking machinery.
#[frame_support::pallet]
pub mod pallet {
	#[pallet::config]
	pub trait Config:
		frame_system::Config + pallet_revive::Config + pallet_vesting::Config
	{
		/// Weight information for the precompile operations.
		type WeightInfo: crate::weights::WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);
}

pub struct Vesting<T>(PhantomData<T>);

/// The balance type used by `pallet-vesting`'s currency.
type VestingBalance<T> =
	<<T as pallet_vesting::Config>::Currency as frame_support::traits::Currency<
		<T as frame_system::Config>::AccountId,
	>>::Balance;

impl<T: Config + pallet_vesting::Config + pallet::Config> Precompile for Vesting<T>
where
	VestingBalance<T>: Into<U256>,
	VestingBalance<T>: From<<T as Config>::Balance>,
	<T as Config>::Balance: From<VestingBalance<T>>,
{
	type T = T;
	type Interface = IVesting::IVestingCalls;
	const MATCHER: AddressMatcher = AddressMatcher::Fixed(NonZero::new(0x0902).unwrap());
	const HAS_CONTRACT_INFO: bool = false;

	fn call(
		_address: &[u8; 20],
		input: &Self::Interface,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		use IVesting::IVestingCalls;
		match input {
			IVestingCalls::vest(IVesting::vestCall {}) => {
				ensure_mutable::<T>(env)?;
				// Derive the beneficiary from the immediate caller (not the tx origin).
				let account_id = caller_account_id(env, "vest")?;

				// Charge the pallet's own benchmarked dispatch weight.
				let dispatch_weight =
					pallet_vesting::Call::<T>::vest {}.get_dispatch_info().call_weight;
				env.frame_meter_mut()
					.charge_weight_token(RuntimeCosts::Precompile(dispatch_weight))?;

				// Construct a signed RuntimeOrigin and dispatch vest().
				let origin = frame_system::RawOrigin::Signed(account_id).into();
				pallet_vesting::Pallet::<T>::vest(origin)
					.map_err(|e| Error::Revert(alloc::format!("vest failed: {:?}", e).into()))?;
				Ok(Vec::new())
			},
			IVestingCalls::vestOther(IVesting::vestOtherCall { target }) => {
				ensure_mutable::<T>(env)?;
				let caller_account = caller_account_id(env, "vestOther")?;

				let target_account = env.to_account_id(&H160::from_slice(target.as_slice()));
				let target_lookup = T::Lookup::unlookup(target_account);

				let dispatch_weight =
					pallet_vesting::Call::<T>::vest_other { target: target_lookup.clone() }
						.get_dispatch_info()
						.call_weight;
				env.frame_meter_mut()
					.charge_weight_token(RuntimeCosts::Precompile(dispatch_weight))?;

				let origin = frame_system::RawOrigin::Signed(caller_account).into();
				pallet_vesting::Pallet::<T>::vest_other(origin, target_lookup).map_err(|e| {
					Error::Revert(alloc::format!("vestOther failed: {:?}", e).into())
				})?;
				Ok(Vec::new())
			},
			// View function to query the currently locked (unvested) balance for the caller.
			// vesting_balance() returns Option<Balance>: None means no schedule exists,
			// Some(0) means a schedule exists but all funds are already unlocked. Both
			// collapse to 0 here — in either case there is nothing left to vest.
			IVestingCalls::vestingBalance(IVesting::vestingBalanceCall {}) => {
				let account_id = caller_account_id(env, "vestingBalance")?;

				env.frame_meter_mut().charge_weight_token(RuntimeCosts::Precompile(
					<<T as pallet::Config>::WeightInfo as weights::WeightInfo>::vesting_balance(),
				))?;

				let maybe_locked =
					<pallet_vesting::Pallet<T> as VestingSchedule<T::AccountId>>::vesting_balance(
						&account_id,
					);

				let locked = maybe_locked.unwrap_or_default();
				Ok(U256::from(locked.into()).to_big_endian().abi_encode())
			},
			IVestingCalls::vestingBalanceOf(IVesting::vestingBalanceOfCall { target }) => {
				let account_id = env.to_account_id(&H160::from_slice(target.as_slice()));

				env.frame_meter_mut().charge_weight_token(RuntimeCosts::Precompile(
					<<T as pallet::Config>::WeightInfo as weights::WeightInfo>::vesting_balance_of(
					),
				))?;

				let maybe_locked =
					<pallet_vesting::Pallet<T> as VestingSchedule<T::AccountId>>::vesting_balance(
						&account_id,
					);

				let locked = maybe_locked.unwrap_or_default();
				Ok(U256::from(locked.into()).to_big_endian().abi_encode())
			},
		}
	}
}
