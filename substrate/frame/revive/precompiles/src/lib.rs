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
use frame_support::{
	dispatch::GetDispatchInfo,
	traits::{Get, VestingSchedule},
};
use pallet_revive::{
	Config,
	precompiles::{AddressMatcher, Error, Ext, H160, Precompile, RuntimeCosts, U256},
};
use pallet_revive_uapi::precompiles::vesting::IVesting;
use sp_runtime::traits::StaticLookup;

pub use pallet::Pallet;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

#[cfg(test)]
pub mod mock;

#[cfg(test)]
mod tests;

/// Minimal pallet providing a `Pallet<T>` type for the FRAME benchmarking machinery.
#[frame_support::pallet]
pub mod pallet {
	#[pallet::config]
	pub trait Config:
		frame_system::Config + pallet_revive::Config + pallet_vesting::Config
	{
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

impl<T: Config + pallet_vesting::Config> Precompile for Vesting<T>
where
	VestingBalance<T>: Into<U256>,
	// Weak proxy for type identity: mutual From bounds do not guarantee the types are the same
	// (e.g. From<u64> for u128 exists in core), but they are the best constraint expressible
	// in stable Rust without a custom sealed trait. A misconfigured runtime that satisfies
	// these bounds with distinct types will compile but return wrong-denomination values.
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
			IVestingCalls::vest(_) if env.is_read_only() => {
				Err(pallet_revive::Error::<T>::StateChangeDenied.into())
			},
			IVestingCalls::vest(IVesting::vestCall {}) => {
				if env.is_delegate_call() {
					return Err(Error::Revert(
						"vesting precompile cannot be called via delegate call".into(),
					));
				}
				// Derive the beneficiary from the immediate caller (not the tx origin).
				let account_id = env
					.caller()
					.account_id()
					.map_err(|e| {
						Error::Revert(
							alloc::format!("vest: caller has no account id: {:?}", e).into(),
						)
					})?
					.clone();

				// Determine and charge the dispatch weight before calling.
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
			IVestingCalls::vestOther(_) if env.is_read_only() => {
				Err(pallet_revive::Error::<T>::StateChangeDenied.into())
			},
			IVestingCalls::vestOther(IVesting::vestOtherCall { target }) => {
				if env.is_delegate_call() {
					return Err(Error::Revert(
						"vesting precompile cannot be called via delegate call".into(),
					));
				}
				let caller_account = env
					.caller()
					.account_id()
					.map_err(|e| {
						Error::Revert(
							alloc::format!("vestOther: caller has no account id: {:?}", e).into(),
						)
					})?
					.clone();

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
				let account_id = env
					.caller()
					.account_id()
					.map_err(|e| {
						Error::Revert(
							alloc::format!("vestingBalance: caller has no account id: {:?}", e)
								.into(),
						)
					})?
					.clone();

				// Charge upfront for the worst case: Vesting map read + free_balance read.
				// If no schedule exists only the Vesting map is read; refund the unused read.
				let charged = env.frame_meter_mut().charge_weight_token(
					RuntimeCosts::Precompile(<T as frame_system::Config>::DbWeight::get().reads(2)),
				)?;

				let maybe_locked =
					<pallet_vesting::Pallet<T> as VestingSchedule<T::AccountId>>::vesting_balance(
						&account_id,
					);

				if maybe_locked.is_none() {
					env.frame_meter_mut().adjust_weight(
						charged,
						RuntimeCosts::Precompile(
							<T as frame_system::Config>::DbWeight::get().reads(1),
						),
					);
				}

				let locked = maybe_locked.unwrap_or_default();
				Ok(U256::from(locked.into()).to_big_endian().abi_encode())
			},
			IVestingCalls::vestingBalanceOf(IVesting::vestingBalanceOfCall { target }) => {
				let account_id = env.to_account_id(&H160::from_slice(target.as_slice()));

				// Same worst-case weight as vestingBalance(): Vesting map read + free_balance read.
				// Refund one read if no schedule exists (only the map was accessed).
				let charged = env.frame_meter_mut().charge_weight_token(
					RuntimeCosts::Precompile(<T as frame_system::Config>::DbWeight::get().reads(2)),
				)?;

				let maybe_locked =
					<pallet_vesting::Pallet<T> as VestingSchedule<T::AccountId>>::vesting_balance(
						&account_id,
					);

				if maybe_locked.is_none() {
					env.frame_meter_mut().adjust_weight(
						charged,
						RuntimeCosts::Precompile(
							<T as frame_system::Config>::DbWeight::get().reads(1),
						),
					);
				}

				let locked = maybe_locked.unwrap_or_default();
				Ok(U256::from(locked.into()).to_big_endian().abi_encode())
			},
		}
	}
}
