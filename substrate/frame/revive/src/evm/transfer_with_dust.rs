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
	address::AddressMapper,
	exec::AccountIdOf,
	primitives::BalanceWithDust,
	storage::AccountInfo,
	AccountInfoOf, BalanceOf, Config, Error, LOG_TARGET,
};
use frame_support::{
	dispatch::DispatchResult,
	traits::{
		fungible::Mutate,
		Get,
	},
};

pub(crate) fn transfer_with_dust<T: Config>(
	from: &AccountIdOf<T>,
	to: &AccountIdOf<T>,
	value: BalanceWithDust<BalanceOf<T>>,
) -> DispatchResult {
	use frame_support::traits::tokens::{Fortitude, Precision, Preservation};
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

	fn transfer_dust<T: Config>(
		from: &mut AccountInfo<T>,
		to: &mut AccountInfo<T>,
		dust: u32,
	) -> DispatchResult {
		from.dust = from.dust.checked_sub(dust).ok_or_else(|| Error::<T>::TransferFailed)?;
		to.dust = to.dust.checked_add(dust).ok_or_else(|| Error::<T>::TransferFailed)?;
		Ok(())
	}

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

	let plank = T::NativeToEthRatio::get();

	if from_info.dust < dust {
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

		from_info.dust =
			from_info.dust.checked_add(plank).ok_or_else(|| Error::<T>::TransferFailed)?;
	}

	transfer_balance::<T>(from, to, value)?;
	transfer_dust::<T>(&mut from_info, &mut to_info, dust)?;

	if to_info.dust >= plank {
		T::Currency::mint_into(to, 1u32.into())?;
		to_info.dust =
			to_info.dust.checked_sub(plank).ok_or_else(|| Error::<T>::TransferFailed)?;
	}

	AccountInfoOf::<T>::set(&from_addr, Some(from_info));
	AccountInfoOf::<T>::set(&to_addr, Some(to_info));

	Ok(())
}
