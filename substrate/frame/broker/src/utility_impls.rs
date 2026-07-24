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
	pallet_prelude::*,
	traits::{
		fungible::Balanced,
		tokens::{Fortitude::Polite, Precision::Exact, Preservation::Expendable},
		OnUnbalanced,
	},
};
use sp_arithmetic::{
	traits::{SaturatedConversion, Saturating, Zero},
	FixedPointNumber, FixedU128, FixedU64,
};
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

	pub fn sale_price(sale: &SaleInfoRecordOf<T>, now: RelayBlockNumberOf<T>) -> BalanceOf<T> {
		let num = now.saturating_sub(sale.sale_start).min(sale.leadin_length).saturated_into();
		let through = FixedU64::from_rational(num, sale.leadin_length.saturated_into());
		T::PriceAdapter::leadin_factor_at(through).saturating_mul_int(sale.end_price)
	}

	pub(crate) fn charge(who: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
		let credit = T::Currency::withdraw(&who, amount, Exact, Expendable, Polite)?;
		T::OnRevenue::on_unbalanced(credit);
		Ok(())
	}

	/// Bring the local estimate of the Relay-chain on-demand market up to date.
	///
	/// First drains the backlog estimate by the number of orders the Relay-chain can have
	/// served since the last update (one order per pool core per Relay-chain block; pool sizes
	/// are in core-mask bits, [`CORE_MASK_BITS`] of which make up a full core, matching the
	/// backlog's unit), then recomputes the traffic multiplier with the same formula the
	/// Relay-chain uses for its own order queue.
	///
	/// Does not emit events or touch storage; callers persist `record` and announce a changed
	/// price as appropriate.
	pub(crate) fn update_on_demand_traffic(
		record: &mut OnDemandStatusRecordOf<T>,
		status: &StatusRecord,
		config: &ConfigRecordOf<T>,
	) {
		let now = RCBlockNumberProviderOf::<T::Coretime>::current_block_number();
		let elapsed: u64 = now.saturating_sub(record.last_updated).saturated_into();
		let pool_bits = status.private_pool_size.saturating_add(status.system_pool_size) as u64;
		let consumed = elapsed.saturating_mul(pool_bits).min(u32::MAX as u64) as u32;
		record.queue_size = record.queue_size.saturating_sub(consumed);
		record.last_updated = now;

		// `validate()` ensures this cannot overflow.
		let capacity = config.on_demand_queue_max_size.saturating_mul(CORE_MASK_BITS as u32);
		match fp_coretime::spot::calculate_spot_traffic(
			record.traffic,
			capacity,
			record.queue_size,
			config.on_demand_target_queue_utilization,
			config.on_demand_fee_variability,
			FixedU128::from_u32(1),
		) {
			Ok(new_traffic) => record.traffic = new_traffic,
			Err(err) => log::debug!(
				target: LOG_TARGET,
				"Error calculating on-demand spot traffic: {:?}", err
			),
		}
	}

	/// The price of an on-demand order placed on this chain, were it placed now.
	///
	/// Computed against an up-to-date congestion estimate without mutating storage.
	pub fn current_on_demand_price() -> Result<BalanceOf<T>, DispatchError> {
		let config = Configuration::<T>::get().ok_or(Error::<T>::Uninitialized)?;
		let status = Status::<T>::get().ok_or(Error::<T>::Uninitialized)?;
		ensure!(!config.on_demand_base_fee.is_zero(), Error::<T>::OnDemandUnavailable);

		let mut record = OnDemandStatus::<T>::get();
		Self::update_on_demand_traffic(&mut record, &status, &config);
		Ok(record.traffic.saturating_mul_int(config.on_demand_base_fee))
	}

	/// Buy a core at the specified price (price is to be determined by the caller).
	///
	/// Note: It is the responsibility of the caller to write back the changed `SaleInfoRecordOf` to
	/// storage.
	pub(crate) fn purchase_core(
		who: &T::AccountId,
		price: BalanceOf<T>,
		sale: &mut SaleInfoRecordOf<T>,
	) -> Result<CoreIndex, DispatchError> {
		Self::charge(who, price)?;
		log::debug!("Purchased core at: {:?}", price);
		let core = sale.first_core.saturating_add(sale.cores_sold);
		sale.cores_sold.saturating_inc();
		if sale.cores_sold <= sale.ideal_cores_sold || sale.sellout_price.is_none() {
			sale.sellout_price = Some(price);
		}
		Ok(core)
	}

	pub fn issue(
		core: CoreIndex,
		begin: Timeslice,
		mask: CoreMask,
		end: Timeslice,
		owner: Option<T::AccountId>,
		paid: Option<BalanceOf<T>>,
	) -> RegionId {
		let id = RegionId { begin, core, mask };
		let record = RegionRecord { end, owner, paid };
		Regions::<T>::insert(&id, &record);
		id
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
