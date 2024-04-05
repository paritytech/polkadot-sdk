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

use frame_support::traits::fungibles::{Inspect, InspectFreeze, MutateFreeze};
use sp_core::Get;
use sp_runtime::traits::Zero;

// Implement fungibles::Inspect as it is required. To do so, we'll re-export
// all of `pallet-assets`' implementation of the same trait.
impl<T: Config<I>, I: 'static> Inspect<AccountIdOf<T>> for Pallet<T, I> {
	type AssetId = AssetIdOf<T, I>;
	type Balance = AssetBalanceOf<T, I>;

	fn total_issuance(asset: Self::AssetId) -> Self::Balance {
		pallet_assets::Pallet::<T, I>::total_issuance(asset)
	}

	fn minimum_balance(asset: Self::AssetId) -> Self::Balance {
		pallet_assets::Pallet::<T, I>::minimum_balance(asset)
	}

	fn total_balance(asset: Self::AssetId, who: &AccountIdOf<T>) -> Self::Balance {
		pallet_assets::Pallet::<T, I>::total_balance(asset, who)
	}

	fn balance(asset: Self::AssetId, who: &AccountIdOf<T>) -> Self::Balance {
		pallet_assets::Pallet::<T, I>::balance(asset, who)
	}

	fn reducible_balance(
		asset: Self::AssetId,
		who: &AccountIdOf<T>,
		preservation: frame_support::traits::tokens::Preservation,
		force: frame_support::traits::tokens::Fortitude,
	) -> Self::Balance {
		pallet_assets::Pallet::<T, I>::reducible_balance(asset, who, preservation, force)
	}

	fn can_deposit(
		asset: Self::AssetId,
		who: &AccountIdOf<T>,
		amount: Self::Balance,
		provenance: frame_support::traits::tokens::Provenance,
	) -> frame_support::traits::tokens::DepositConsequence {
		pallet_assets::Pallet::<T, I>::can_deposit(asset, who, amount, provenance)
	}

	fn can_withdraw(
		asset: Self::AssetId,
		who: &AccountIdOf<T>,
		amount: Self::Balance,
	) -> frame_support::traits::tokens::WithdrawConsequence<Self::Balance> {
		pallet_assets::Pallet::<T, I>::can_withdraw(asset, who, amount)
	}

	fn asset_exists(asset: Self::AssetId) -> bool {
		pallet_assets::Pallet::<T, I>::asset_exists(asset)
	}
}

impl<T: Config<I>, I: 'static> InspectFreeze<AccountIdOf<T>> for Pallet<T, I> {
	type Id = T::RuntimeFreezeReason;

	fn balance_frozen(asset: Self::AssetId, id: &Self::Id, who: &AccountIdOf<T>) -> Self::Balance {
		if let Some(i) = Freezes::<T, I>::get(asset, who).iter().find(|i| i.id == *id) {
			i.amount
		} else {
			Zero::zero()
		}
	}

	fn can_freeze(asset: Self::AssetId, id: &Self::Id, who: &AccountIdOf<T>) -> bool {
		let freezes = Freezes::<T, I>::get(asset, who);
		freezes.len()
			< T::MaxFreezes::get()
				.try_into()
				.expect("MaxFreezes is the same type as S within Freezes<S>; qed")
			|| freezes.into_iter().any(|i| i.id == *id)
	}
}

impl<T: Config<I>, I: 'static> MutateFreeze<AccountIdOf<T>> for Pallet<T, I> {
	fn set_freeze(
		asset: Self::AssetId,
		id: &Self::Id,
		who: &AccountIdOf<T>,
		amount: Self::Balance,
	) -> sp_runtime::DispatchResult {
		if amount.is_zero() {
			return Self::thaw(asset, id, who);
		}
		let mut freezes = Freezes::<T, I>::get(asset.clone(), who);
		if let Some(i) = freezes.iter_mut().find(|i| &i.id == id) {
			i.amount = amount;
		} else {
			freezes
				.try_push(IdAmount { id: id.clone(), amount })
				.map_err(|_| Error::<T, I>::TooManyFreezes)?;
		}
		Self::update_freezes(asset, who, freezes.as_bounded_slice())
	}

	fn extend_freeze(
		asset: Self::AssetId,
		id: &Self::Id,
		who: &AccountIdOf<T>,
		amount: Self::Balance,
	) -> sp_runtime::DispatchResult {
		if amount.is_zero() {
			return Ok(());
		}
		let mut freezes = Freezes::<T, I>::get(asset.clone(), who);
		if let Some(i) = freezes.iter_mut().find(|x| &x.id == id) {
			i.amount = i.amount.max(amount);
		} else {
			freezes
				.try_push(IdAmount { id: id.clone(), amount })
				.map_err(|_| Error::<T, I>::TooManyFreezes)?;
		}
		Self::update_freezes(asset, who, freezes.as_bounded_slice())
	}

	fn thaw(
		asset: Self::AssetId,
		id: &Self::Id,
		who: &AccountIdOf<T>,
	) -> sp_runtime::DispatchResult {
		let mut freezes = Freezes::<T, I>::get(asset.clone(), who);
		freezes.retain(|f| &f.id != id);
		Self::update_freezes(asset, who, freezes.as_bounded_slice())
	}
}
