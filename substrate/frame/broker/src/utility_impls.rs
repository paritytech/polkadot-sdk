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
use fp_coretime::market::{
	CoreRangeProvider, RenewalRightsProvider, SoldCoresRange, TimesliceProvider,
};
use frame_support::{
	pallet_prelude::*,
	traits::{
		fungible::{Balanced, MutateHold},
		tokens::{Fortitude::Polite, Precision::Exact, Preservation::Expendable},
		OnUnbalanced,
	},
};
use sp_arithmetic::traits::{SaturatedConversion, Saturating};
use sp_runtime::traits::{AccountIdConversion, BlockNumberProvider};

impl<T: Config> Pallet<T> {
	pub fn current_timeslice() -> Timeslice {
		let latest = RCBlockNumberProviderOf::<T::Coretime>::current_block_number();
		let timeslice_period = T::TimeslicePeriod::get();
		(latest / timeslice_period).saturated_into()
	}

	pub fn latest_timeslice_ready_to_commit(config: &ConfigRecordOf<T>) -> Timeslice {
		let latest = RCBlockNumberProviderOf::<T::Coretime>::current_block_number();
		let advanced = latest.saturating_add(config.advance_notice);
		let timeslice_period = T::TimeslicePeriod::get();
		(advanced / timeslice_period).saturated_into()
	}

	pub fn next_timeslice_to_commit(
		config: &ConfigRecordOf<T>,
		status: &StatusRecord,
	) -> Option<Timeslice> {
		if status.last_committed_timeslice < Self::latest_timeslice_ready_to_commit(config) {
			Some(status.last_committed_timeslice + 1)
		} else {
			None
		}
	}

	pub fn account_id() -> T::AccountId {
		T::PalletId::get().into_account_truncating()
	}

	pub(crate) fn charge(who: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
		let credit = T::Currency::withdraw(&who, amount, Exact, Expendable, Polite)?;
		T::OnRevenue::on_unbalanced(credit);
		Ok(())
	}

	pub(crate) fn lock_funds(who: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
		T::Currency::hold(&HoldReason::CoretimeBid.into(), who, amount)
	}

	pub(crate) fn refund(who: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
		T::Currency::release(&HoldReason::CoretimeBid.into(), who, amount, Exact).map(|_| ())
	}

	pub fn issue(
		region_id: RegionId,
		end: Timeslice,
		owner: Option<T::AccountId>,
		paid: Option<BalanceOf<T>>,
	) {
		let record = RegionRecord { end, owner, paid };
		Regions::<T>::insert(&region_id, &record);
	}

	pub(crate) fn utilize(
		mut region_id: RegionId,
		maybe_check_owner: Option<T::AccountId>,
		finality: Finality,
	) -> Result<Option<(RegionId, RegionRecordOf<T>)>, Error<T>> {
		let status = Status::<T>::get().ok_or(Error::<T>::Uninitialized)?;
		let region = Regions::<T>::get(&region_id).ok_or(Error::<T>::UnknownRegion)?;

		if let Some(check_owner) = maybe_check_owner {
			ensure!(Some(check_owner) == region.owner, Error::<T>::NotOwner);
		}

		Regions::<T>::remove(&region_id);

		let last_committed_timeslice = status.last_committed_timeslice;
		if region_id.begin <= last_committed_timeslice {
			let duration = region.end.saturating_sub(region_id.begin);
			region_id.begin = last_committed_timeslice + 1;
			if region_id.begin >= region.end {
				Self::deposit_event(Event::RegionDropped { region_id, duration });
				return Ok(None);
			}
		} else {
			Workplan::<T>::mutate_extant((region_id.begin, region_id.core), |p| {
				p.retain(|i| (i.mask & region_id.mask).is_void())
			});
		}
		if finality == Finality::Provisional {
			Regions::<T>::insert(&region_id, &region);
		}

		Ok(Some((region_id, region)))
	}

	// Remove a region from on-demand pool contributions. Useful in cases where it was pooled
	// provisionally and it is being redispatched (partition/interlace/assign).
	//
	// Takes both the region_id and (a reference to) the region as arguments to avoid another DB
	// read. No-op for regions which have not been pooled.
	pub(crate) fn force_unpool_region(
		region_id: RegionId,
		region: &RegionRecordOf<T>,
		status: &StatusRecord,
	) {
		// We don't care if this fails or not, just that it is removed if present. This is to
		// account for the case where a region is pooled provisionally and redispatched.
		if InstaPoolContribution::<T>::take(region_id).is_some() {
			// `InstaPoolHistory` is calculated from the `InstaPoolIo` one timeslice in advance.
			// Therefore we need to schedule this for the timeslice after that.
			let end_timeslice = status.last_committed_timeslice + 1;

			// InstaPoolIo has already accounted for regions that have already ended. Regions ending
			// this timeslice would have region.end == unpooled_at below.
			if region.end <= end_timeslice {
				return;
			}

			// Account for the change in `InstaPoolIo` either from the start of the region or from
			// the current timeslice if we are already part-way through the region.
			let size = region_id.mask.count_ones() as i32;
			let unpooled_at = end_timeslice.max(region_id.begin);
			InstaPoolIo::<T>::mutate(unpooled_at, |a| a.private.saturating_reduce(size));
			InstaPoolIo::<T>::mutate(region.end, |a| a.private.saturating_accrue(size));

			Self::deposit_event(Event::<T>::RegionUnpooled { region_id, when: unpooled_at });
		};
	}
}

pub struct CoreRangeProviderImpl<T: Config>(PhantomData<T>);

impl<T: Config> CoreRangeProvider for CoreRangeProviderImpl<T> {
	fn core_range() -> Option<SoldCoresRange> {
		let reserved = Reservations::<T>::decode_len().unwrap_or_default() as CoreIndex +
			Leases::<T>::decode_len().unwrap_or_default() as CoreIndex;
		let core_count = Status::<T>::get()?.core_count;

		Some(SoldCoresRange { from: reserved, to: core_count })
	}
}

pub struct TimesliceProviderImpl<T: Config>(PhantomData<T>);

impl<T: Config> TimesliceProvider for TimesliceProviderImpl<T> {
	fn next_timeslice_to_commit() -> Option<Timeslice> {
		if let (Some(status), Some(config)) = (Status::<T>::get(), Configuration::<T>::get()) {
			Pallet::<T>::next_timeslice_to_commit(&config, &status)
		} else {
			None
		}
	}

	fn latest_timeslice_ready_to_commit() -> Option<Timeslice> {
		let Some(config) = Configuration::<T>::get() else {
			return None;
		};

		let latest = RCBlockNumberProviderOf::<T::Coretime>::current_block_number();
		let advanced = latest.saturating_add(config.advance_notice);
		let timeslice_period = T::TimeslicePeriod::get();
		Some((advanced / timeslice_period).saturated_into())
	}
}

impl<T: Config> RenewalRightsProvider<T::AccountId> for Pallet<T> {
	fn renewal_rights_count(who: &T::AccountId, when: Timeslice) -> RenewalRightsCount {
		RenewalRights::<T>::get(RenewalRightsId { owner: who.clone(), when }).unwrap_or_default()
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_rights_count(who: &T::AccountId, when: Timeslice, count: RenewalRightsCount) {
		RenewalRights::<T>::insert(RenewalRightsId { owner: who.clone(), when }, count);
	}
}
