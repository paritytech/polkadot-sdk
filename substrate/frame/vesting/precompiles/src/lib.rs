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
use codec::Decode;
use core::{marker::PhantomData, num::NonZero};
use frame_support::traits::{Get, LockableCurrency, PalletInfoAccess, VestingSchedule};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_revive::{
	Config,
	precompiles::{
		AddressMatcher, Error, Ext, H160, Precompile, RuntimeCosts, U256, ensure_not_delegate_call,
		ensure_not_read_only,
	},
};
use pallet_vesting::{VestingInfo, WeightInfo as _};
use sp_runtime::{DispatchError, traits::StaticLookup};

alloy_core::sol!("IVesting.sol");

pub use pallet::Pallet;
pub mod weights;
pub use weights::WeightInfo;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

#[cfg(all(test, feature = "runtime-benchmarks"))]
pub mod mock;

#[cfg(all(test, feature = "runtime-benchmarks"))]
mod tests;

fn ensure_mutable<T: Config>(env: &impl Ext<T = T>) -> Result<(), Error> {
	ensure_not_delegate_call::<T>(env)?;
	ensure_not_read_only::<T>(env)
}

fn caller_account_id<T: Config>(env: &impl Ext<T = T>) -> Result<T::AccountId, Error> {
	env.caller().account_id().map_err(Error::try_to_revert::<T>).cloned()
}

const ERR_UNEXPECTED: &str = "unexpected error";

/// Every pallet failure of a state-changing call is a Solidity revert.
fn vesting_failure<T: pallet_vesting::Config>(prefix: &str, e: DispatchError) -> Error {
	Error::Revert(alloc::format!("{prefix}: {}", vesting_dispatch_reason::<T>(e)).into())
}

fn vesting_dispatch_reason<T: pallet_vesting::Config>(e: DispatchError) -> alloc::string::String {
	match e {
		DispatchError::Token(token) => <&'static str>::from(token).into(),
		DispatchError::Arithmetic(arith) => <&'static str>::from(arith).into(),
		DispatchError::Other(msg) => msg.into(),
		DispatchError::Module(module) => match decode_vesting_error::<T>(e) {
			Some(err) => vesting_pallet_reason(err).into(),
			None => module.message.unwrap_or(ERR_UNEXPECTED).into(),
		},
		_ => ERR_UNEXPECTED.into(),
	}
}

fn decode_vesting_error<T: pallet_vesting::Config>(
	e: DispatchError,
) -> Option<pallet_vesting::Error<T>> {
	let DispatchError::Module(module) = e else { return None };
	let index = <pallet_vesting::Pallet<T> as PalletInfoAccess>::index() as u8;
	if module.index != index {
		return None;
	}
	pallet_vesting::Error::<T>::decode(&mut &module.error[..]).ok()
}

fn vesting_pallet_reason<T>(err: pallet_vesting::Error<T>) -> &'static str {
	use pallet_vesting::Error::*;
	match err {
		NotVesting => "Account is not vesting",
		AtMaxVestingSchedules => "Maximum vesting schedules reached",
		AmountLow => "Amount too low to create a vesting schedule",
		ScheduleIndexOutOfBounds => "Vesting schedule index out of bounds",
		InvalidScheduleParams => "Invalid vesting schedule parameters",
	}
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

/// Mirror of `pallet_vesting::MaxLocksOf` (which is crate-private).
type MaxLocksOf<T> = <<T as pallet_vesting::Config>::Currency as LockableCurrency<
	<T as frame_system::Config>::AccountId,
>>::MaxLocks;

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
				// TODO: pallet_vesting::vest returns DispatchResult, not
				// DispatchResultWithPostInfo, so we can't refund the difference
				// between vest_locked and vest_unlocked. Once the pallet is
				// updated to return actual weight, use adjust_gas here.
				let max_locks = MaxLocksOf::<T>::get();
				let dispatch_weight = <T as pallet_vesting::Config>::WeightInfo::vest_locked(
					max_locks,
					T::MAX_VESTING_SCHEDULES,
				)
				.max(<T as pallet_vesting::Config>::WeightInfo::vest_unlocked(
					max_locks,
					T::MAX_VESTING_SCHEDULES,
				));
				env.frame_meter_mut()
					.charge_weight_token(RuntimeCosts::Precompile(dispatch_weight))?;

				ensure_mutable::<T>(env)?;

				let account_id = caller_account_id(env)?;
				let origin = frame_system::RawOrigin::Signed(account_id).into();
				pallet_vesting::Pallet::<T>::vest(origin)
					.map_err(|e| vesting_failure::<T>("vest failed", e))?;
				Ok(Vec::new())
			},
			IVestingCalls::vestOther(IVesting::vestOtherCall { target }) => {
				// TODO: same as vest — pallet returns DispatchResult so we
				// can't refund the locked vs unlocked weight difference.
				let max_locks = MaxLocksOf::<T>::get();
				let dispatch_weight = <T as pallet_vesting::Config>::WeightInfo::vest_other_locked(
					max_locks,
					T::MAX_VESTING_SCHEDULES,
				)
				.max(<T as pallet_vesting::Config>::WeightInfo::vest_other_unlocked(
					max_locks,
					T::MAX_VESTING_SCHEDULES,
				));
				env.frame_meter_mut()
					.charge_weight_token(RuntimeCosts::Precompile(dispatch_weight))?;

				ensure_mutable::<T>(env)?;

				let caller_account = caller_account_id(env)?;
				let target_account = env.to_account_id(&H160::from_slice(target.as_slice()));
				let target_lookup = T::Lookup::unlookup(target_account);

				let origin = frame_system::RawOrigin::Signed(caller_account).into();
				pallet_vesting::Pallet::<T>::vest_other(origin, target_lookup)
					.map_err(|e| vesting_failure::<T>("vestOther failed", e))?;
				Ok(Vec::new())
			},
			IVestingCalls::vestedTransfer(IVesting::vestedTransferCall {
				target,
				locked,
				perBlock,
				startingBlock,
			}) => {
				// Charge weight upfront before any conversion work. The pallet weight
				// is constant (depends only on MaxLocks and MAX_VESTING_SCHEDULES).
				let max_locks = MaxLocksOf::<T>::get();
				let dispatch_weight = <T as pallet_vesting::Config>::WeightInfo::vested_transfer(
					max_locks,
					T::MAX_VESTING_SCHEDULES,
				);
				env.frame_meter_mut()
					.charge_weight_token(RuntimeCosts::Precompile(dispatch_weight))?;

				ensure_mutable::<T>(env)?;

				let caller_account = caller_account_id(env)?;
				let target_account = env.to_account_id(&H160::from_slice(target.as_slice()));
				let target_lookup = T::Lookup::unlookup(target_account);

				let locked: VestingBalance<T> = {
					let balance: <T as Config>::Balance =
						U256::from_big_endian(&locked.to_be_bytes::<32>())
							.try_into()
							.map_err(|_| Error::Revert("vestedTransfer: locked overflow".into()))?;
					<VestingBalance<T> as From<<T as Config>::Balance>>::from(balance)
				};
				let per_block: VestingBalance<T> = {
					let balance: <T as Config>::Balance =
						U256::from_big_endian(&perBlock.to_be_bytes::<32>()).try_into().map_err(
							|_| Error::Revert("vestedTransfer: perBlock overflow".into()),
						)?;
					<VestingBalance<T> as From<<T as Config>::Balance>>::from(balance)
				};
				let starting_block: BlockNumberFor<T> =
					U256::from_big_endian(&startingBlock.to_be_bytes::<32>()).try_into().map_err(
						|_| Error::Revert("vestedTransfer: startingBlock overflow".into()),
					)?;

				let schedule = VestingInfo::new(locked, per_block, starting_block);
				let origin = frame_system::RawOrigin::Signed(caller_account).into();
				pallet_vesting::Pallet::<T>::vested_transfer(origin, target_lookup, schedule)
					.map_err(|e| vesting_failure::<T>("vestedTransfer failed", e))?;
				Ok(Vec::new())
			},
			// View function to query the currently locked (unvested) balance for the caller.
			// vesting_balance() returns Option<Balance>: None means no schedule exists,
			// Some(0) means a schedule exists but all funds are already unlocked. Both
			// collapse to 0 here — in either case there is nothing left to vest.
			IVestingCalls::vestingBalance(IVesting::vestingBalanceCall {}) => {
				env.frame_meter_mut().charge_weight_token(RuntimeCosts::Precompile(
					<<T as pallet::Config>::WeightInfo as weights::WeightInfo>::vesting_balance(),
				))?;

				let account_id = caller_account_id(env)?;

				let maybe_locked =
					<pallet_vesting::Pallet<T> as VestingSchedule<T::AccountId>>::vesting_balance(
						&account_id,
					);

				let locked = maybe_locked.unwrap_or_default();
				Ok(U256::from(locked.into()).to_big_endian().abi_encode())
			},
			IVestingCalls::vestingBalanceOf(IVesting::vestingBalanceOfCall { target }) => {
				env.frame_meter_mut().charge_weight_token(RuntimeCosts::Precompile(
					<<T as pallet::Config>::WeightInfo as weights::WeightInfo>::vesting_balance_of(
					),
				))?;

				let account_id = env.to_account_id(&H160::from_slice(target.as_slice()));

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
