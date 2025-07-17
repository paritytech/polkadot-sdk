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

use super::*;

use frame_support::traits::{
	fungibles::{Dust, Inspect, InspectHold, MutateHold, Unbalanced, UnbalancedHold},
	tokens::{
		DepositConsequence, Fortitude, Precision, Preservation, Provenance, WithdrawConsequence,
	},
};
use pallet_assets::BalanceOnHold;
use sp_runtime::{
	traits::{CheckedAdd, CheckedSub, Zero},
	ArithmeticError,
};
use storage::StorageDoubleMap;

// Implements [`BalanceOnHold`] from [`pallet-assets`], so it can understand whether there's some
// balance on hold for an asset account, and is able to signal to this pallet when to clear the
// state of an account.
impl<T: Config<I>, I: 'static> BalanceOnHold<T::AssetId, T::AccountId, T::Balance>
	for Pallet<T, I>
{
	fn balance_on_hold(asset: T::AssetId, who: &T::AccountId) -> Option<T::Balance> {
		BalancesOnHold::<T, I>::get(asset, who)
	}

	fn died(asset: T::AssetId, who: &T::AccountId) {
		defensive_assert!(
			Holds::<T, I>::get(asset.clone(), who).is_empty(),
			"The list of Holds should be empty before allowing an account to die"
		);
		defensive_assert!(
			BalancesOnHold::<T, I>::get(asset.clone(), who).is_none(),
			"The should not be a balance on hold before allowing to die"
		);

		Holds::<T, I>::remove(asset.clone(), who);
		BalancesOnHold::<T, I>::remove(asset, who);
	}

	fn contains_holds(asset: T::AssetId) -> bool {
		Holds::<T, I>::contains_prefix(asset)
	}
}

// Implement [`fungibles::Inspect`](frame_support::traits::fungibles::Inspect) as it is bound by
// [`fungibles::InspectHold`](frame_support::traits::fungibles::InspectHold) and
// [`fungibles::MutateHold`](frame_support::traits::fungibles::MutateHold). To do so, we'll
// re-export all of `pallet-assets` implementation of the same trait.
impl<T: Config<I>, I: 'static> Inspect<T::AccountId> for Pallet<T, I> {
	type AssetId = T::AssetId;
	type Balance = T::Balance;

	fn total_issuance(asset: Self::AssetId) -> Self::Balance {
		pallet_assets::Pallet::<T, I>::total_issuance(asset)
	}

	fn minimum_balance(asset: Self::AssetId) -> Self::Balance {
		pallet_assets::Pallet::<T, I>::minimum_balance(asset)
	}

	fn total_balance(asset: Self::AssetId, who: &T::AccountId) -> Self::Balance {
		pallet_assets::Pallet::<T, I>::total_balance(asset, who)
	}

	fn balance(asset: Self::AssetId, who: &T::AccountId) -> Self::Balance {
		pallet_assets::Pallet::<T, I>::balance(asset, who)
	}

	fn reducible_balance(
		asset: Self::AssetId,
		who: &T::AccountId,
		preservation: Preservation,
		force: Fortitude,
	) -> Self::Balance {
		pallet_assets::Pallet::<T, I>::reducible_balance(asset, who, preservation, force)
	}

	fn can_deposit(
		asset: Self::AssetId,
		who: &T::AccountId,
		amount: Self::Balance,
		provenance: Provenance,
	) -> DepositConsequence {
		pallet_assets::Pallet::<T, I>::can_deposit(asset, who, amount, provenance)
	}

	fn can_withdraw(
		asset: Self::AssetId,
		who: &T::AccountId,
		amount: Self::Balance,
	) -> WithdrawConsequence<Self::Balance> {
		pallet_assets::Pallet::<T, I>::can_withdraw(asset, who, amount)
	}

	fn asset_exists(asset: Self::AssetId) -> bool {
		pallet_assets::Pallet::<T, I>::asset_exists(asset)
	}
}

impl<T: Config<I>, I: 'static> InspectHold<T::AccountId> for Pallet<T, I> {
	type Reason = T::RuntimeHoldReason;

	fn total_balance_on_hold(asset: Self::AssetId, who: &T::AccountId) -> Self::Balance {
		BalancesOnHold::<T, I>::get(asset, who).unwrap_or_else(Zero::zero)
	}

	fn balance_on_hold(
		asset: Self::AssetId,
		reason: &Self::Reason,
		who: &T::AccountId,
	) -> Self::Balance {
		Holds::<T, I>::get(asset, who)
			.iter()
			.find(|x| &x.id == reason)
			.map(|x| x.amount)
			.unwrap_or_else(Zero::zero)
	}
}

impl<T: Config<I>, I: 'static> Unbalanced<T::AccountId> for Pallet<T, I> {
	fn handle_dust(dust: Dust<T::AccountId, Self>) {
		let Dust(id, balance) = dust;
		pallet_assets::Pallet::<T, I>::handle_dust(Dust(id, balance));
	}

	fn write_balance(
		asset: Self::AssetId,
		who: &T::AccountId,
		amount: Self::Balance,
	) -> Result<Option<Self::Balance>, DispatchError> {
		pallet_assets::Pallet::<T, I>::write_balance(asset, who, amount)
	}

	fn set_total_issuance(asset: Self::AssetId, amount: Self::Balance) {
		pallet_assets::Pallet::<T, I>::set_total_issuance(asset, amount)
	}

	fn decrease_balance(
		asset: Self::AssetId,
		who: &T::AccountId,
		amount: Self::Balance,
		precision: Precision,
		preservation: Preservation,
		force: Fortitude,
	) -> Result<Self::Balance, DispatchError> {
		pallet_assets::Pallet::<T, I>::decrease_balance(
			asset,
			who,
			amount,
			precision,
			preservation,
			force,
		)
	}

	fn increase_balance(
		asset: Self::AssetId,
		who: &T::AccountId,
		amount: Self::Balance,
		precision: Precision,
	) -> Result<Self::Balance, DispatchError> {
		pallet_assets::Pallet::<T, I>::increase_balance(asset, who, amount, precision)
	}
}

impl<T: Config<I>, I: 'static> UnbalancedHold<T::AccountId> for Pallet<T, I> {
	fn set_balance_on_hold(
		asset: Self::AssetId,
		reason: &Self::Reason,
		who: &T::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		let mut holds = Holds::<T, I>::get(asset.clone(), who);
		let amount_on_hold =
			BalancesOnHold::<T, I>::get(asset.clone(), who).unwrap_or_else(Zero::zero);

		let amount_on_hold = if amount.is_zero() {
			if let Some(pos) = holds.iter().position(|x| &x.id == reason) {
				let item = &mut holds[pos];
				let amount = item.amount;

				holds.swap_remove(pos);
				amount_on_hold.checked_sub(&amount).ok_or(ArithmeticError::Underflow)?
			} else {
				amount_on_hold
			}
		} else {
			let (increase, delta) = if let Some(pos) = holds.iter().position(|x| &x.id == reason) {
				let item = &mut holds[pos];
				let (increase, delta) =
					(amount > item.amount, item.amount.max(amount) - item.amount.min(amount));

				item.amount = amount;
				if item.amount.is_zero() {
					holds.swap_remove(pos);
				}

				(increase, delta)
			} else {
				holds
					.try_push(IdAmount { id: *reason, amount })
					.map_err(|_| Error::<T, I>::TooManyHolds)?;
				(true, amount)
			};

			let amount_on_hold = if increase {
				amount_on_hold.checked_add(&delta).ok_or(ArithmeticError::Overflow)?
			} else {
				amount_on_hold.checked_sub(&delta).ok_or(ArithmeticError::Underflow)?
			};

			amount_on_hold
		};

		if !holds.is_empty() {
			Holds::<T, I>::insert(asset.clone(), who, holds);
		} else {
			Holds::<T, I>::remove(asset.clone(), who);
		}

		if amount_on_hold.is_zero() {
			BalancesOnHold::<T, I>::remove(asset.clone(), who);
		} else {
			BalancesOnHold::<T, I>::insert(asset.clone(), who, amount_on_hold);
		}

		Ok(())
	}
}

impl<T: Config<I>, I: 'static> MutateHold<T::AccountId> for Pallet<T, I> {
	fn done_hold(
		asset_id: Self::AssetId,
		reason: &Self::Reason,
		who: &T::AccountId,
		amount: Self::Balance,
	) {
		Self::deposit_event(Event::<T, I>::Held {
			asset_id,
			who: who.clone(),
			reason: *reason,
			amount,
		});
	}

	fn done_release(
		asset_id: Self::AssetId,
		reason: &Self::Reason,
		who: &T::AccountId,
		amount: Self::Balance,
	) {
		Self::deposit_event(Event::<T, I>::Released {
			asset_id,
			who: who.clone(),
			reason: *reason,
			amount,
		});
	}

	fn done_burn_held(
		asset_id: Self::AssetId,
		reason: &Self::Reason,
		who: &T::AccountId,
		amount: Self::Balance,
	) {
		Self::deposit_event(Event::<T, I>::Burned {
			asset_id,
			who: who.clone(),
			reason: *reason,
			amount,
		});
	}
}
