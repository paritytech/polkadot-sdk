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

use crate::{
	Config,
	precompiles::{BuiltinAddressMatcher, BuiltinPrecompile, Error, Ext},
	vm::RuntimeCosts,
};
use alloc::vec::Vec;
use core::{marker::PhantomData, num::NonZero};
use frame_support::dispatch::GetDispatchInfo;
use pallet_revive_uapi::precompiles::vesting::IVesting;

pub struct Vesting<T>(PhantomData<T>);

impl<T: Config + pallet_vesting::Config> BuiltinPrecompile for Vesting<T> {
	type T = T;
	type Interface = IVesting::IVestingCalls;
	const MATCHER: BuiltinAddressMatcher =
		BuiltinAddressMatcher::Fixed(NonZero::new(0x902).unwrap());
	const HAS_CONTRACT_INFO: bool = false;

	fn call(
		_address: &[u8; 20],
		input: &Self::Interface,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		if env.is_delegate_call() {
			return Err(Error::Revert(
				"vesting precompile cannot be called via delegate call".into(),
			));
		}

		use IVesting::IVestingCalls;
		match input {
			IVestingCalls::vest(_) if env.is_read_only() => {
				Err(crate::Error::<T>::StateChangeDenied.into())
			},
			IVestingCalls::vest(IVesting::vestCall {}) => {
				// Derive the beneficiary from the immediate caller (not the tx origin).
				let account_id = env.caller().account_id().map_err(|e| {
					Error::Revert(
						alloc::format!("vest: caller has no account id: {:?}", e).into(),
					)
				})?.clone();

				// Determine and charge the dispatch weight before calling.
				let dispatch_weight = pallet_vesting::Call::<T>::vest {}
					.get_dispatch_info()
					.call_weight;
				env.frame_meter_mut()
					.charge_weight_token(RuntimeCosts::Precompile(dispatch_weight))?;

				// Construct a signed RuntimeOrigin and dispatch vest().
				let origin = frame_system::RawOrigin::Signed(account_id).into();
				pallet_vesting::Pallet::<T>::vest(origin).map_err(|e| {
					Error::Revert(
						alloc::format!("vest failed: {:?}", e).into(),
					)
				})?;
				Ok(Vec::new())
			},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		call_builder::CallSetup,
		precompiles::BuiltinPrecompile,
		tests::{ExtBuilder, Test},
	};

	#[test]
	fn vest_succeeds_after_vesting_period() {
		ExtBuilder::default().build().execute_with(|| {
			use crate::test_utils::ALICE;
			use frame_support::traits::{Currency, WithdrawReasons};

			let alice_account = ALICE;

			// Fund alice and set up a vesting schedule.
			let total_balance = 1_000_000u128;
			let locked = 500_000u128;
			let per_block = 100u128;
			let starting_block = 0u64;

			<pallet_balances::Pallet<Test> as Currency<_>>::make_free_balance_be(
				&alice_account,
				total_balance,
			);

			let vesting_info =
				pallet_vesting::VestingInfo::new(locked, per_block, starting_block);
			assert!(vesting_info.is_valid());

			// Write vesting schedule directly to storage.
			let schedules: frame_support::BoundedVec<_, pallet_vesting::MaxVestingSchedulesGet<Test>> =
				alloc::vec![vesting_info].try_into().expect("single schedule; qed");
			pallet_vesting::Vesting::<Test>::insert(&alice_account, schedules);

			// Apply the vesting lock.
			let reasons = WithdrawReasons::except(
				<Test as pallet_vesting::Config>::UnvestedFundsAllowedWithdrawReasons::get(),
			);
			<pallet_balances::Pallet<Test> as frame_support::traits::LockableCurrency<_>>::set_lock(
				*b"vesting ",
				&alice_account,
				locked,
				reasons,
			);

			// Advance blocks past the cliff so some funds are vested.
			let blocks_to_advance = 1000u64;
			frame_system::Pallet::<Test>::set_block_number(blocks_to_advance);

			// Call the precompile.
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			let input = IVesting::IVestingCalls::vest(IVesting::vestCall {});
			let result =
				<Vesting<Test>>::call(&<Vesting<Test>>::MATCHER.base_address(), &input, &mut ext);
			assert!(result.is_ok(), "vest precompile should succeed: {:?}", result.err());
			assert!(result.unwrap().is_empty(), "vest returns empty bytes for void");
		})
	}

	#[test]
	fn vest_reverts_when_caller_is_root() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			call_setup.set_origin(crate::exec::Origin::Root);
			let (mut ext, _) = call_setup.ext();

			let input = IVesting::IVestingCalls::vest(IVesting::vestCall {});
			let result =
				<Vesting<Test>>::call(&<Vesting<Test>>::MATCHER.base_address(), &input, &mut ext);
			match result {
				Err(Error::Revert(revert)) => {
					assert!(
						revert.reason.contains("caller has no account id"),
						"unexpected revert message: {}",
						revert.reason
					);
				},
				other => panic!("expected Error::Revert, got: {:?}", other),
			}
		})
	}

	#[test]
	fn vest_cannot_vest_other_accounts_funds() {
		// Verify that the precompile derives the origin from the caller (ALICE by default)
		// and cannot be used to vest funds belonging to a different account (BOB).
		ExtBuilder::default().build().execute_with(|| {
			use crate::test_utils::BOB;
			use frame_support::traits::{Currency, WithdrawReasons};

			// Set up a vesting schedule for BOB only (not ALICE, who is the default caller).
			let locked = 500_000u128;
			let per_block = 100u128;

			<pallet_balances::Pallet<Test> as Currency<_>>::make_free_balance_be(
				&BOB,
				1_000_000u128,
			);

			let vesting_info = pallet_vesting::VestingInfo::new(locked, per_block, 0u64);
			let schedules: frame_support::BoundedVec<_, pallet_vesting::MaxVestingSchedulesGet<Test>> =
				alloc::vec![vesting_info].try_into().expect("single schedule; qed");
			pallet_vesting::Vesting::<Test>::insert(&BOB, schedules);

			let reasons = WithdrawReasons::except(
				<Test as pallet_vesting::Config>::UnvestedFundsAllowedWithdrawReasons::get(),
			);
			<pallet_balances::Pallet<Test> as frame_support::traits::LockableCurrency<_>>::set_lock(
				*b"vesting ",
				&BOB,
				locked,
				reasons,
			);

			// The default CallSetup caller is ALICE (not BOB).
			// Calling vest via the precompile will try to vest ALICE's funds.
			// Since ALICE has no vesting schedule, this must fail.
			// This proves the precompile cannot be used to vest another account's funds.
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			frame_system::Pallet::<Test>::set_block_number(1000);

			let input = IVesting::IVestingCalls::vest(IVesting::vestCall {});
			let result =
				<Vesting<Test>>::call(&<Vesting<Test>>::MATCHER.base_address(), &input, &mut ext);

			// Should fail because ALICE (the caller) has no vesting schedule,
			// even though BOB does. The origin is always derived from the caller.
			match &result {
				Err(Error::Revert(revert)) => {
					assert!(
						revert.reason.contains("vest failed"),
						"unexpected revert message: {}",
						revert.reason
					);
				},
				other => panic!(
					"expected Error::Revert for no vesting schedule, got: {:?}",
					other
				),
			}

			// Confirm BOB's vesting is unchanged.
			assert!(
				pallet_vesting::Vesting::<Test>::get(&BOB).is_some(),
				"BOB's vesting schedule should be untouched"
			);
		})
	}
}
