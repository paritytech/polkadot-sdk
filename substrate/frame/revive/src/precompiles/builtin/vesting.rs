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
	Config, H160, U256,
	precompiles::{BuiltinAddressMatcher, BuiltinPrecompile, Error, Ext},
	vm::RuntimeCosts,
};
use alloc::vec::Vec;
use alloy_core::sol_types::SolValue;
use core::{marker::PhantomData, num::NonZero};
use frame_support::{
	dispatch::GetDispatchInfo,
	traits::{Get, VestingSchedule},
};
use pallet_revive_uapi::precompiles::vesting::IVesting;
use sp_runtime::traits::StaticLookup;

pub struct Vesting<T>(PhantomData<T>);

/// The balance type used by `pallet-vesting`'s currency.
type VestingBalance<T> = <<T as pallet_vesting::Config>::Currency as frame_support::traits::Currency<
	<T as frame_system::Config>::AccountId,
>>::Balance;

impl<T: Config + pallet_vesting::Config> BuiltinPrecompile for Vesting<T>
where
	VestingBalance<T>: Into<U256>,
	// Enforce that pallet-vesting and pallet-revive operate on the same balance denomination.
	// Both pallets expose Currency via different trait families (LockableCurrency vs
	// fungibles), so a direct Currency-equality bound is not expressible. Instead we require
	// mutual From conversions between their Balance types, which is only satisfied when the
	// types are identical. A compile error here means the runtime has configured mismatched
	// currencies, which would cause vestingBalance() to return amounts in the wrong denomination.
	VestingBalance<T>: From<<T as Config>::Balance>,
	<T as Config>::Balance: From<VestingBalance<T>>,
{
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
		use IVesting::IVestingCalls;
		match input {
			IVestingCalls::vest(_) if env.is_read_only() => {
				Err(crate::Error::<T>::StateChangeDenied.into())
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
				Err(crate::Error::<T>::StateChangeDenied.into())
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

			let vesting_info = pallet_vesting::VestingInfo::new(locked, per_block, starting_block);
			assert!(vesting_info.is_valid());

			// Write vesting schedule directly to storage.
			let schedules: frame_support::BoundedVec<
				_,
				pallet_vesting::MaxVestingSchedulesGet<Test>,
			> = alloc::vec![vesting_info].try_into().expect("single schedule; qed");
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
			let schedules: frame_support::BoundedVec<
				_,
				pallet_vesting::MaxVestingSchedulesGet<Test>,
			> = alloc::vec![vesting_info].try_into().expect("single schedule; qed");
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
				other => panic!("expected Error::Revert for no vesting schedule, got: {:?}", other),
			}

			// Confirm BOB's vesting is unchanged.
			assert!(
				pallet_vesting::Vesting::<Test>::get(&BOB).is_some(),
				"BOB's vesting schedule should be untouched"
			);
		})
	}

	#[test]
	fn vesting_balance_returns_locked_amount() {
		ExtBuilder::default().build().execute_with(|| {
			use crate::test_utils::ALICE;
			use alloy_core::sol_types::SolValue;
			use frame_support::traits::{Currency, WithdrawReasons};

			let alice_account = ALICE;

			let total_balance = 1_000_000u128;
			let locked = 500_000u128;
			let per_block = 100u128;
			let starting_block = 0u64;

			<pallet_balances::Pallet<Test> as Currency<_>>::make_free_balance_be(
				&alice_account,
				total_balance,
			);

			let vesting_info = pallet_vesting::VestingInfo::new(locked, per_block, starting_block);
			let schedules: frame_support::BoundedVec<
				_,
				pallet_vesting::MaxVestingSchedulesGet<Test>,
			> = alloc::vec![vesting_info].try_into().expect("single schedule; qed");
			pallet_vesting::Vesting::<Test>::insert(&alice_account, schedules);

			let reasons = WithdrawReasons::except(
				<Test as pallet_vesting::Config>::UnvestedFundsAllowedWithdrawReasons::get(),
			);
			<pallet_balances::Pallet<Test> as frame_support::traits::LockableCurrency<_>>::set_lock(
				*b"vesting ",
				&alice_account,
				locked,
				reasons,
			);

			// At block 1000: 1000 * 100 = 100_000 vested, so 400_000 still locked.
			frame_system::Pallet::<Test>::set_block_number(1000);

			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			let input = IVesting::IVestingCalls::vestingBalance(IVesting::vestingBalanceCall {});
			let result =
				<Vesting<Test>>::call(&<Vesting<Test>>::MATCHER.base_address(), &input, &mut ext);
			assert!(result.is_ok(), "vestingBalance should succeed: {:?}", result.err());

			let bytes = <[u8; 32]>::abi_decode(&result.unwrap()).expect("should decode as bytes32");
			let returned = crate::U256::from_big_endian(&bytes);
			assert_eq!(
				returned,
				crate::U256::from(400_000u128),
				"at block 1000, 100_000 should have vested leaving 400_000 locked"
			);
		})
	}

	#[test]
	fn vest_reverts_on_delegate_call() {
		ExtBuilder::default().build().execute_with(|| {
			use crate::{DelegateInfo, exec::Origin};
			use sp_core::H160;

			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();
			ext.set_delegate_call(DelegateInfo {
				caller: Origin::from_account_id(ext.account_id().clone()),
				callee: H160::from_low_u64_be(0x902),
			});

			let input = IVesting::IVestingCalls::vest(IVesting::vestCall {});
			let result =
				<Vesting<Test>>::call(&<Vesting<Test>>::MATCHER.base_address(), &input, &mut ext);
			match result {
				Err(Error::Revert(revert)) => {
					assert!(
						revert.reason.contains("cannot be called via delegate call"),
						"unexpected revert message: {}",
						revert.reason
					);
				},
				other => panic!("expected Error::Revert for delegate call, got: {:?}", other),
			}
		})
	}

	#[test]
	fn vesting_balance_succeeds_on_delegate_call() {
		ExtBuilder::default().build().execute_with(|| {
			use crate::{DelegateInfo, exec::Origin};
			use alloy_core::sol_types::SolValue;
			use sp_core::H160;

			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();
			ext.set_delegate_call(DelegateInfo {
				caller: Origin::from_account_id(ext.account_id().clone()),
				callee: H160::from_low_u64_be(0x902),
			});

			let input = IVesting::IVestingCalls::vestingBalance(IVesting::vestingBalanceCall {});
			let result =
				<Vesting<Test>>::call(&<Vesting<Test>>::MATCHER.base_address(), &input, &mut ext);
			assert!(
				result.is_ok(),
				"vestingBalance should be callable via delegate call: {:?}",
				result.err()
			);
			let bytes = <[u8; 32]>::abi_decode(&result.unwrap()).expect("should decode as bytes32");
			let returned = crate::U256::from_big_endian(&bytes);
			assert_eq!(returned, crate::U256::zero(), "no vesting schedule should return 0");
		})
	}

	#[test]
	fn vesting_balance_returns_zero_for_no_schedule() {
		ExtBuilder::default().build().execute_with(|| {
			use alloy_core::sol_types::SolValue;

			// ALICE has no vesting schedule by default.
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			let input = IVesting::IVestingCalls::vestingBalance(IVesting::vestingBalanceCall {});
			let result =
				<Vesting<Test>>::call(&<Vesting<Test>>::MATCHER.base_address(), &input, &mut ext);
			assert!(result.is_ok(), "vestingBalance should succeed: {:?}", result.err());

			let bytes = <[u8; 32]>::abi_decode(&result.unwrap()).expect("should decode as bytes32");
			let returned = crate::U256::from_big_endian(&bytes);
			assert_eq!(
				returned,
				crate::U256::from(0u128),
				"account with no vesting schedule should return 0"
			);
		})
	}

	#[test]
	fn vest_other_succeeds_after_vesting_period() {
		ExtBuilder::default().build().execute_with(|| {
			use crate::test_utils::{BOB, BOB_ADDR};
			use frame_support::traits::{Currency, WithdrawReasons};

			let locked = 500_000u128;
			let per_block = 100u128;
			let starting_block = 0u64;

			<pallet_balances::Pallet<Test> as Currency<_>>::make_free_balance_be(
				&BOB,
				1_000_000u128,
			);

			let vesting_info = pallet_vesting::VestingInfo::new(locked, per_block, starting_block);
			let schedules: frame_support::BoundedVec<
				_,
				pallet_vesting::MaxVestingSchedulesGet<Test>,
			> = alloc::vec![vesting_info].try_into().expect("single schedule; qed");
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

			frame_system::Pallet::<Test>::set_block_number(1000);

			// ALICE (the default caller) calls vestOther on BOB's behalf.
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			let input = IVesting::IVestingCalls::vestOther(IVesting::vestOtherCall {
				target: alloy_core::primitives::Address::from(BOB_ADDR.0),
			});
			let result =
				<Vesting<Test>>::call(&<Vesting<Test>>::MATCHER.base_address(), &input, &mut ext);
			assert!(result.is_ok(), "vestOther precompile should succeed: {:?}", result.err());
			assert!(result.unwrap().is_empty(), "vestOther returns empty bytes for void");
		})
	}

	#[test]
	fn vest_reverts_in_read_only_context() {
		ExtBuilder::default().build().execute_with(|| {
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();
			ext.set_read_only(true);

			let input = IVesting::IVestingCalls::vest(IVesting::vestCall {});
			let result =
				<Vesting<Test>>::call(&<Vesting<Test>>::MATCHER.base_address(), &input, &mut ext);
			match result {
				Err(Error::Error(e)) => {
					assert_eq!(
						e.error,
						crate::Error::<Test>::StateChangeDenied.into(),
						"expected StateChangeDenied"
					);
				},
				other => panic!("expected StateChangeDenied error, got: {:?}", other),
			}
		})
	}

	#[test]
	fn vest_other_reverts_in_read_only_context() {
		ExtBuilder::default().build().execute_with(|| {
			use crate::test_utils::BOB_ADDR;

			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();
			ext.set_read_only(true);

			let input = IVesting::IVestingCalls::vestOther(IVesting::vestOtherCall {
				target: alloy_core::primitives::Address::from(BOB_ADDR.0),
			});
			let result =
				<Vesting<Test>>::call(&<Vesting<Test>>::MATCHER.base_address(), &input, &mut ext);
			match result {
				Err(Error::Error(e)) => {
					assert_eq!(
						e.error,
						crate::Error::<Test>::StateChangeDenied.into(),
						"expected StateChangeDenied"
					);
				},
				other => panic!("expected StateChangeDenied error, got: {:?}", other),
			}
		})
	}

	#[test]
	fn vesting_balance_of_returns_locked_amount_for_target() {
		ExtBuilder::default().build().execute_with(|| {
			use crate::test_utils::{BOB, BOB_ADDR};
			use alloy_core::sol_types::SolValue;
			use frame_support::traits::{Currency, WithdrawReasons};

			let locked = 500_000u128;
			let per_block = 100u128;
			let starting_block = 0u64;

			<pallet_balances::Pallet<Test> as Currency<_>>::make_free_balance_be(
				&BOB,
				1_000_000u128,
			);

			let vesting_info = pallet_vesting::VestingInfo::new(locked, per_block, starting_block);
			let schedules: frame_support::BoundedVec<
				_,
				pallet_vesting::MaxVestingSchedulesGet<Test>,
			> = alloc::vec![vesting_info].try_into().expect("single schedule; qed");
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

			// At block 1000: 1000 * 100 = 100_000 vested, 400_000 still locked.
			frame_system::Pallet::<Test>::set_block_number(1000);

			// ALICE is the caller but we query BOB's balance.
			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			let input = IVesting::IVestingCalls::vestingBalanceOf(IVesting::vestingBalanceOfCall {
				target: alloy_core::primitives::Address::from(BOB_ADDR.0),
			});
			let result =
				<Vesting<Test>>::call(&<Vesting<Test>>::MATCHER.base_address(), &input, &mut ext);
			assert!(result.is_ok(), "vestingBalanceOf should succeed: {:?}", result.err());

			let bytes = <[u8; 32]>::abi_decode(&result.unwrap()).expect("should decode as bytes32");
			let returned = crate::U256::from_big_endian(&bytes);
			assert_eq!(
				returned,
				crate::U256::from(400_000u128),
				"at block 1000, 100_000 should have vested leaving 400_000 locked"
			);
		})
	}

	#[test]
	fn vesting_balance_of_returns_zero_for_no_schedule() {
		ExtBuilder::default().build().execute_with(|| {
			use crate::test_utils::BOB_ADDR;
			use alloy_core::sol_types::SolValue;

			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			let input = IVesting::IVestingCalls::vestingBalanceOf(IVesting::vestingBalanceOfCall {
				target: alloy_core::primitives::Address::from(BOB_ADDR.0),
			});
			let result =
				<Vesting<Test>>::call(&<Vesting<Test>>::MATCHER.base_address(), &input, &mut ext);
			assert!(result.is_ok(), "vestingBalanceOf should succeed: {:?}", result.err());

			let bytes = <[u8; 32]>::abi_decode(&result.unwrap()).expect("should decode as bytes32");
			let returned = crate::U256::from_big_endian(&bytes);
			assert_eq!(returned, crate::U256::zero(), "no vesting schedule should return 0");
		})
	}

	#[test]
	fn vest_other_reverts_when_target_has_no_schedule() {
		ExtBuilder::default().build().execute_with(|| {
			use crate::test_utils::BOB_ADDR;

			frame_system::Pallet::<Test>::set_block_number(1000);

			let mut call_setup = CallSetup::<Test>::default();
			let (mut ext, _) = call_setup.ext();

			let input = IVesting::IVestingCalls::vestOther(IVesting::vestOtherCall {
				target: alloy_core::primitives::Address::from(BOB_ADDR.0),
			});
			let result =
				<Vesting<Test>>::call(&<Vesting<Test>>::MATCHER.base_address(), &input, &mut ext);
			match result {
				Err(Error::Revert(revert)) => {
					assert!(
						revert.reason.contains("vestOther failed"),
						"unexpected revert message: {}",
						revert.reason
					);
				},
				other => panic!("expected Error::Revert for no vesting schedule, got: {:?}", other),
			}
		})
	}
}
