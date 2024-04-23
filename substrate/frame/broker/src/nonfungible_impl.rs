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
use frame_support::{
	pallet_prelude::{DispatchResult, *},
	traits::nonfungible::{Inspect, Mutate, Transfer},
};
use sp_std::vec::Vec;

impl<T: Config> Inspect<T::AccountId> for Pallet<T> {
	type ItemId = u128;

	fn owner(item: &Self::ItemId) -> Option<T::AccountId> {
		let record = Regions::<T>::get(RegionId::from(*item))?;
		record.owner
	}

	fn attribute(item: &Self::ItemId, key: &[u8]) -> Option<Vec<u8>> {
		let id = RegionId::from(*item);
		let item = Regions::<T>::get(id)?;
		match key {
			b"begin" => Some(id.begin.encode()),
			b"end" => Some(item.end.encode()),
			b"length" => Some(item.end.saturating_sub(id.begin).encode()),
			b"core" => Some(id.core.encode()),
			b"part" => Some(id.mask.encode()),
			b"owner" => Some(item.owner.encode()),
			b"paid" => Some(item.paid.encode()),
			_ => None,
		}
	}
}

impl<T: Config> Transfer<T::AccountId> for Pallet<T> {
	fn transfer(item: &Self::ItemId, dest: &T::AccountId) -> DispatchResult {
		Self::do_transfer((*item).into(), None, dest.clone()).map_err(Into::into)
	}
}

/// We don't really support burning and minting.
///
/// We only need this to allow the region to be reserve transferable.
///
/// For reserve transfers that are not 'local', the asset must first be withdrawn to the holding
/// register and then deposited into the designated account. This process necessitates that the
/// asset is capable of being 'burned' and 'minted'.
///
/// Since each region is associated with specific record data, we will not actually burn the asset.
/// If we did, we wouldn't know what record to assign to the newly minted region. Therefore, instead
/// of burning, we set the asset's owner to `None`. In essence, 'burning' a region involves setting
/// its owner to `None`, whereas 'minting' the region assigns its owner to an actual account. This
/// way we never lose track of the associated record data.
impl<T: Config> Mutate<T::AccountId> for Pallet<T> {
	/// Deposit a region into an account.
	fn mint_into(item: &Self::ItemId, who: &T::AccountId) -> DispatchResult {
		let region_id: RegionId = (*item).into();
		let record = Regions::<T>::get(&region_id).ok_or(Error::<T>::UnknownRegion)?;

		// 'Minting' can only occur if the asset has previously been burned (i.e. moved to the
		// holding register)
		ensure!(record.owner.is_none(), Error::<T>::NotAllowed);
		Self::issue(region_id.core, region_id.begin, record.end, Some(who.clone()), record.paid);

		Ok(())
	}

	/// Withdraw a region from account.
	fn burn(item: &Self::ItemId, maybe_check_owner: Option<&T::AccountId>) -> DispatchResult {
		let region_id: RegionId = (*item).into();
		let mut record = Regions::<T>::get(&region_id).ok_or(Error::<T>::UnknownRegion)?;
		if let Some(owner) = maybe_check_owner {
			ensure!(Some(owner.clone()) == record.owner, Error::<T>::NotOwner);
		}

		record.owner = None;
		Regions::<T>::insert(region_id, record);

		Ok(())
	}
}
