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

//! Transfer with dust functionality for pallet-revive.

use crate::{
	address::AddressMapper, exec::AccountIdOf, primitives::BalanceWithDust, storage::AccountInfo,
	AccountInfoOf, BalanceOf, Config, Error, LOG_TARGET,
};
use frame_support::{
	dispatch::DispatchResult,
	traits::{
		fungible::Mutate,
		tokens::{Fortitude, Precision, Preservation},
		Get,
	},
};

/// Transfer balance between two accounts.
fn transfer_balance<T: Config>(
	from: &AccountIdOf<T>,
	to: &AccountIdOf<T>,
	value: BalanceOf<T>,
) -> DispatchResult {
	T::Currency::transfer(from, to, value, Preservation::Preserve)
			.map_err(|err| {
				log::debug!(target: LOG_TARGET, "Transfer failed: from {from:?} to {to:?} (value: ${value:?}). Err: {err:?}");
				Error::<T>::TransferFailed
			})?;
	Ok(())
}

/// Transfer dust between two account infos.
fn transfer_dust<T: Config>(
	from: &mut AccountInfo<T>,
	to: &mut AccountInfo<T>,
	dust: u32,
) -> DispatchResult {
	from.dust = from.dust.checked_sub(dust).ok_or_else(|| Error::<T>::TransferFailed)?;
	to.dust = to.dust.checked_add(dust).ok_or_else(|| Error::<T>::TransferFailed)?;
	Ok(())
}

/// Ensure an account has sufficient dust to perform an operation.
///
/// If the account doesn't have enough dust, this function will burn one unit of the native
/// currency (1 plank) and convert it to dust by adding `NativeToEthRatio` worth of dust
/// to the account's dust balance.
fn ensure_sufficient_dust<T: Config>(
	from: &AccountIdOf<T>,
	from_info: &mut AccountInfo<T>,
	required_dust: u32,
) -> DispatchResult {
	if from_info.dust >= required_dust {
		return Ok(())
	}

	let plank = T::NativeToEthRatio::get();

	T::Currency::burn_from(
		from,
		1u32.into(),
		Preservation::Preserve,
		Precision::Exact,
		Fortitude::Polite,
	)
	.map_err(|err| {
		log::debug!(target: LOG_TARGET, "Burning 1 plank from {from:?} failed. Err: {err:?}");
		Error::<T>::TransferFailed
	})?;

	from_info.dust = from_info.dust.checked_add(plank).ok_or_else(|| Error::<T>::TransferFailed)?;

	Ok(())
}

/// Transfer a balance with dust between two accounts.
pub(crate) fn transfer_with_dust<T: Config>(
	from: &AccountIdOf<T>,
	to: &AccountIdOf<T>,
	value: BalanceWithDust<BalanceOf<T>>,
) -> DispatchResult {
	let from_addr = <T::AddressMapper as AddressMapper<T>>::to_address(from);
	let mut from_info = AccountInfoOf::<T>::get(&from_addr).unwrap_or_default();

	if from_info.balance(from) < value {
		log::debug!(target: LOG_TARGET, "Insufficient balance: from {from:?} to {to:?} (value: ${value:?}). Balance: ${:?}", from_info.balance(from));
		return Err(Error::<T>::TransferFailed.into())
	} else if from == to || value.is_zero() {
		return Ok(())
	}

	let (value, dust) = value.deconstruct();
	if dust == 0 {
		return transfer_balance::<T>(from, to, value)
	}

	let to_addr = <T::AddressMapper as AddressMapper<T>>::to_address(to);
	let mut to_info = AccountInfoOf::<T>::get(&to_addr).unwrap_or_default();

	ensure_sufficient_dust::<T>(from, &mut from_info, dust)?;
	transfer_balance::<T>(from, to, value)?;
	transfer_dust::<T>(&mut from_info, &mut to_info, dust)?;

	let plank = T::NativeToEthRatio::get();
	if to_info.dust >= plank {
		T::Currency::mint_into(to, 1u32.into())?;
		to_info.dust = to_info.dust.checked_sub(plank).ok_or_else(|| Error::<T>::TransferFailed)?;
	}

	AccountInfoOf::<T>::set(&from_addr, Some(from_info));
	AccountInfoOf::<T>::set(&to_addr, Some(to_info));

	Ok(())
}

/// Burn a balance with dust from an account.
pub(crate) fn burn_with_dust<T: Config>(
	from: &AccountIdOf<T>,
	value: BalanceWithDust<BalanceOf<T>>,
) -> DispatchResult {
	let from_addr = <T::AddressMapper as AddressMapper<T>>::to_address(from);
	let mut from_info = AccountInfoOf::<T>::get(&from_addr).unwrap_or_default();

	if from_info.balance(from) < value {
		log::debug!(target: LOG_TARGET, "Insufficient balance: from {from:?} (value: ${value:?}). Balance: ${:?}", from_info.balance(from));
		return Err(Error::<T>::TransferFailed.into())
	} else if value.is_zero() {
		return Ok(())
	}

	let (value, dust) = value.deconstruct();
	if dust == 0 {
		// No dust to handle, just burn the balance
		T::Currency::burn_from(
			from,
			value,
			Preservation::Preserve,
			Precision::Exact,
			Fortitude::Polite,
		)
		.map_err(|err| {
			log::debug!(target: LOG_TARGET, "Burning {value:?} from {from:?} failed. Err: {err:?}");
			Error::<T>::TransferFailed
		})?;
		return Ok(())
	}

	ensure_sufficient_dust::<T>(from, &mut from_info, dust)?;
	T::Currency::burn_from(
		from,
		value,
		Preservation::Preserve,
		Precision::Exact,
		Fortitude::Polite,
	)
	.map_err(|err| {
		log::debug!(target: LOG_TARGET, "Burning {value:?} from {from:?} failed. Err: {err:?}");
		Error::<T>::TransferFailed
	})?;

	from_info.dust = from_info.dust.checked_sub(dust).ok_or_else(|| Error::<T>::TransferFailed)?;
	AccountInfoOf::<T>::set(&from_addr, Some(from_info));
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		test_utils::{ALICE_ADDR, BOB_ADDR},
		tests::{builder, test_utils::set_balance_with_dust, ExtBuilder, Test},
		Config, Error, Pallet, H160,
	};
	use frame_support::{assert_err, traits::Get};
	use sp_runtime::{traits::Zero, DispatchError};

	#[test]
	fn transfer_with_dust_works() {
		struct TestCase {
			description: &'static str,
			from: H160,
			to: H160,
			from_balance: BalanceWithDust<u64>,
			to_balance: BalanceWithDust<u64>,
			amount: BalanceWithDust<u64>,
			expected_from_balance: BalanceWithDust<u64>,
			expected_to_balance: BalanceWithDust<u64>,
			total_issuance_diff: i64,
			expected_error: Option<DispatchError>,
		}

		let plank: u32 = <Test as Config>::NativeToEthRatio::get();

		let test_cases = vec![
			TestCase {
				description: "without dust",
				from: ALICE_ADDR,
				to: BOB_ADDR,
				from_balance: BalanceWithDust::new_unchecked::<Test>(100, 0),
				to_balance: BalanceWithDust::new_unchecked::<Test>(0, 0),
				amount: BalanceWithDust::new_unchecked::<Test>(1, 0),
				expected_from_balance: BalanceWithDust::new_unchecked::<Test>(99, 0),
				expected_to_balance: BalanceWithDust::new_unchecked::<Test>(1, 0),
				total_issuance_diff: 0,
				expected_error: None,
			},
			TestCase {
				description: "with dust",
				from: ALICE_ADDR,
				to: BOB_ADDR,
				from_balance: BalanceWithDust::new_unchecked::<Test>(100, 0),
				to_balance: BalanceWithDust::new_unchecked::<Test>(0, 0),
				amount: BalanceWithDust::new_unchecked::<Test>(1, 10),
				expected_from_balance: BalanceWithDust::new_unchecked::<Test>(98, plank - 10),
				expected_to_balance: BalanceWithDust::new_unchecked::<Test>(1, 10),
				total_issuance_diff: 1,
				expected_error: None,
			},
			TestCase {
				description: "just dust",
				from: ALICE_ADDR,
				to: BOB_ADDR,
				from_balance: BalanceWithDust::new_unchecked::<Test>(100, 0),
				to_balance: BalanceWithDust::new_unchecked::<Test>(0, 0),
				amount: BalanceWithDust::new_unchecked::<Test>(0, 10),
				expected_from_balance: BalanceWithDust::new_unchecked::<Test>(99, plank - 10),
				expected_to_balance: BalanceWithDust::new_unchecked::<Test>(0, 10),
				total_issuance_diff: 1,
				expected_error: None,
			},
			TestCase {
				description: "with existing dust",
				from: ALICE_ADDR,
				to: BOB_ADDR,
				from_balance: BalanceWithDust::new_unchecked::<Test>(100, 5),
				to_balance: BalanceWithDust::new_unchecked::<Test>(0, plank - 5),
				amount: BalanceWithDust::new_unchecked::<Test>(1, 10),
				expected_from_balance: BalanceWithDust::new_unchecked::<Test>(98, plank - 5),
				expected_to_balance: BalanceWithDust::new_unchecked::<Test>(2, 5),
				total_issuance_diff: 0,
				expected_error: None,
			},
			TestCase {
				description: "with enough existing dust",
				from: ALICE_ADDR,
				to: BOB_ADDR,
				from_balance: BalanceWithDust::new_unchecked::<Test>(100, 10),
				to_balance: BalanceWithDust::new_unchecked::<Test>(0, plank - 10),
				amount: BalanceWithDust::new_unchecked::<Test>(1, 10),
				expected_from_balance: BalanceWithDust::new_unchecked::<Test>(99, 0),
				expected_to_balance: BalanceWithDust::new_unchecked::<Test>(2, 0),
				total_issuance_diff: -1,
				expected_error: None,
			},
			TestCase {
				description: "receiver dust less than 1 plank",
				from: ALICE_ADDR,
				to: BOB_ADDR,
				from_balance: BalanceWithDust::new_unchecked::<Test>(100, plank / 10),
				to_balance: BalanceWithDust::new_unchecked::<Test>(0, plank / 2),
				amount: BalanceWithDust::new_unchecked::<Test>(1, plank / 10 * 3),
				expected_from_balance: BalanceWithDust::new_unchecked::<Test>(98, plank / 10 * 8),
				expected_to_balance: BalanceWithDust::new_unchecked::<Test>(1, plank / 10 * 8),
				total_issuance_diff: 1,
				expected_error: None,
			},
			TestCase {
				description: "insufficient balance",
				from: ALICE_ADDR,
				to: BOB_ADDR,
				from_balance: BalanceWithDust::new_unchecked::<Test>(10, 0),
				to_balance: BalanceWithDust::new_unchecked::<Test>(10, 0),
				amount: BalanceWithDust::new_unchecked::<Test>(20, 0),
				expected_from_balance: BalanceWithDust::new_unchecked::<Test>(10, 0),
				expected_to_balance: BalanceWithDust::new_unchecked::<Test>(10, 0),
				total_issuance_diff: 0,
				expected_error: Some(Error::<Test>::TransferFailed.into()),
			},
			TestCase {
				description: "from = to with insufficient balance",
				from: ALICE_ADDR,
				to: ALICE_ADDR,
				from_balance: BalanceWithDust::new_unchecked::<Test>(10, 0),
				to_balance: BalanceWithDust::new_unchecked::<Test>(10, 0),
				amount: BalanceWithDust::new_unchecked::<Test>(20, 0),
				expected_from_balance: BalanceWithDust::new_unchecked::<Test>(10, 0),
				expected_to_balance: BalanceWithDust::new_unchecked::<Test>(10, 0),
				total_issuance_diff: 0,
				expected_error: Some(Error::<Test>::TransferFailed.into()),
			},
			TestCase {
				description: "from = to with insufficient balance",
				from: ALICE_ADDR,
				to: ALICE_ADDR,
				from_balance: BalanceWithDust::new_unchecked::<Test>(0, 10),
				to_balance: BalanceWithDust::new_unchecked::<Test>(0, 10),
				amount: BalanceWithDust::new_unchecked::<Test>(0, 20),
				expected_from_balance: BalanceWithDust::new_unchecked::<Test>(0, 10),
				expected_to_balance: BalanceWithDust::new_unchecked::<Test>(0, 10),
				total_issuance_diff: 0,
				expected_error: Some(Error::<Test>::TransferFailed.into()),
			},
			TestCase {
				description: "from = to",
				from: ALICE_ADDR,
				to: ALICE_ADDR,
				from_balance: BalanceWithDust::new_unchecked::<Test>(0, 10),
				to_balance: BalanceWithDust::new_unchecked::<Test>(0, 10),
				amount: BalanceWithDust::new_unchecked::<Test>(0, 5),
				expected_from_balance: BalanceWithDust::new_unchecked::<Test>(0, 10),
				expected_to_balance: BalanceWithDust::new_unchecked::<Test>(0, 10),
				total_issuance_diff: 0,
				expected_error: None,
			},
		];

		for TestCase {
			description,
			from,
			to,
			from_balance,
			to_balance,
			amount,
			expected_from_balance,
			expected_to_balance,
			total_issuance_diff,
			expected_error,
		} in test_cases.into_iter()
		{
			ExtBuilder::default().build().execute_with(|| {
				set_balance_with_dust(&from, from_balance);
				set_balance_with_dust(&to, to_balance);

				let total_issuance = <Test as Config>::Currency::total_issuance();
				let evm_value = Pallet::<Test>::convert_native_to_evm(amount);

				let (value, dust) = amount.deconstruct();
				assert_eq!(Pallet::<Test>::has_dust(evm_value), !dust.is_zero());
				assert_eq!(Pallet::<Test>::has_balance(evm_value), !value.is_zero());

				let result = builder::bare_call(to).evm_value(evm_value).build();

				if let Some(expected_error) = expected_error {
					assert_err!(result.result, expected_error);
				} else {
					assert_eq!(result.result.unwrap(), Default::default(), "{description} tx failed");
				}

				assert_eq!(
					Pallet::<Test>::evm_balance(&from),
					Pallet::<Test>::convert_native_to_evm(expected_from_balance),
					"{description}: invalid from balance"
				);

				assert_eq!(
					Pallet::<Test>::evm_balance(&to),
					Pallet::<Test>::convert_native_to_evm(expected_to_balance),
					"{description}: invalid to balance"
				);

				assert_eq!(
					total_issuance as i64 - total_issuance_diff,
					<Test as Config>::Currency::total_issuance() as i64,
					"{description}: total issuance should match"
				);
			});
		}
	}
}
