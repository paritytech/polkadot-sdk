// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! The parachain on demand assignment module.
//!
//! Implements a mechanism for taking in orders for on-demand parachain (previously parathreads)
//! assignments. This module is not handled by the initializer but is instead instantiated in the
//! `construct_runtime` macro.
//!
//! The module currently limits parallel execution of blocks from the same `ParaId` via
//! a core affinity mechanism. As long as there exists an affinity for a `CoreIndex` for
//! a specific `ParaId`, orders for blockspace for that `ParaId` will only be assigned to
//! that `CoreIndex`.
//!
//! NOTE: Once we have elastic scaling implemented we might want to extend this module to support
//! ignoring core affinity up to a certain extend. This should be opt-in though as the parachain
//! needs to support multiple cores in the same block. If we want to enable a single parachain
//! occupying multiple cores in on-demand, we will likely add a separate order type, where the
//! intent can be made explicit.

use sp_runtime::traits::Zero;
mod benchmarking;
pub mod migration;
mod mock_helpers;
mod types;

extern crate alloc;

#[cfg(test)]
mod tests;

use crate::{configuration, paras, scheduler::common::Assignment};
use alloc::collections::BinaryHeap;
use core::mem::take;
use frame_support::{
	pallet_prelude::*,
	traits::{
		defensive_prelude::*,
		Currency,
		ExistenceRequirement::{self, AllowDeath, KeepAlive},
		WithdrawReasons,
	},
	PalletId,
};
use frame_system::{pallet_prelude::*, Pallet as System};
use polkadot_primitives::{CoreIndex, Id as ParaId};
use sp_runtime::{
	traits::{AccountIdConversion, One, SaturatedConversion},
	FixedPointNumber, FixedPointOperand, FixedU128, Perbill, Saturating,
};
use types::{
	BalanceOf, CoreAffinityCount, EnqueuedOrder, QueuePushDirection, QueueStatusType,
	SpotTrafficCalculationErr,
};

const LOG_TARGET: &str = "runtime::parachains::on-demand";

pub use pallet::*;

pub trait WeightInfo {
	fn place_order_allow_death(s: u32) -> Weight;
	fn place_order_keep_alive(s: u32) -> Weight;
}

/// A weight info that is only suitable for testing.
pub struct TestWeightInfo;

impl WeightInfo for TestWeightInfo {
	fn place_order_allow_death(_: u32) -> Weight {
		Weight::MAX
	}

	fn place_order_keep_alive(_: u32) -> Weight {
		Weight::MAX
	}
}

#[frame_support::pallet]
pub mod pallet {

	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + configuration::Config + paras::Config {
		/// The runtime's definition of an event.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The runtime's definition of a Currency.
		type Currency: Currency<Self::AccountId>;

		/// Something that provides the weight of this pallet.
		type WeightInfo: WeightInfo;

		/// The default value for the spot traffic multiplier.
		#[pallet::constant]
		type TrafficDefaultValue: Get<FixedU128>;

		/// The maximum number of blocks some historical revenue
		/// information stored for.
		#[pallet::constant]
		type MaxHistoricalRevenue: Get<u32>;

		/// Identifier for the internal revenue balance.
		#[pallet::constant]
		type PalletId: Get<PalletId>;
	}

	/// Creates an empty queue status for an empty queue with initial traffic value.
	#[pallet::type_value]
	pub(super) fn QueueStatusOnEmpty<T: Config>() -> QueueStatusType {
		QueueStatusType { traffic: T::TrafficDefaultValue::get(), ..Default::default() }
	}

	#[pallet::type_value]
	pub(super) fn EntriesOnEmpty<T: Config>() -> BinaryHeap<EnqueuedOrder> {
		BinaryHeap::new()
	}

	/// Maps a `ParaId` to `CoreIndex` and keeps track of how many assignments the scheduler has in
	/// it's lookahead. Keeping track of this affinity prevents parallel execution of the same
	/// `ParaId` on two or more `CoreIndex`es.
	#[pallet::storage]
	pub(super) type ParaIdAffinity<T: Config> =
		StorageMap<_, Twox64Concat, ParaId, CoreAffinityCount, OptionQuery>;

	/// Overall status of queue (both free + affinity entries)
	#[pallet::storage]
	pub(super) type QueueStatus<T: Config> =
		StorageValue<_, QueueStatusType, ValueQuery, QueueStatusOnEmpty<T>>;

	/// Priority queue for all orders which don't yet (or not any more) have any core affinity.
	#[pallet::storage]
	pub(super) type FreeEntries<T: Config> =
		StorageValue<_, BinaryHeap<EnqueuedOrder>, ValueQuery, EntriesOnEmpty<T>>;

	/// Queue entries that are currently bound to a particular core due to core affinity.
	#[pallet::storage]
	pub(super) type AffinityEntries<T: Config> = StorageMap<
		_,
		Twox64Concat,
		CoreIndex,
		BinaryHeap<EnqueuedOrder>,
		ValueQuery,
		EntriesOnEmpty<T>,
	>;

	/// Keeps track of accumulated revenue from on demand order sales.
	#[pallet::storage]
	pub type Revenue<T: Config> =
		StorageValue<_, BoundedVec<BalanceOf<T>, T::MaxHistoricalRevenue>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An order was placed at some spot price amount by orderer ordered_by
		OnDemandOrderPlaced { para_id: ParaId, spot_price: BalanceOf<T>, ordered_by: T::AccountId },
		/// The value of the spot price has likely changed
		SpotPriceSet { spot_price: BalanceOf<T> },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The order queue is full, `place_order` will not continue.
		QueueFull,
		/// The current spot price is higher than the max amount specified in the `place_order`
		/// call, making it invalid.
		SpotPriceHigherThanMaxAmount,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_now: BlockNumberFor<T>) -> Weight {
			// Update revenue information storage.
			Revenue::<T>::mutate(|revenue| {
				if let Some(overdue) =
					revenue.force_insert_keep_left(0, 0u32.into()).defensive_unwrap_or(None)
				{
					// We have some overdue revenue not claimed by the Coretime Chain, let's
					// accumulate it at the oldest stored block
					if let Some(last) = revenue.last_mut() {
						*last = last.saturating_add(overdue);
					}
				}
			});

			let config = configuration::ActiveConfig::<T>::get();
			// We need to update the spot traffic on block initialize in order to account for idle
			// blocks.
			QueueStatus::<T>::mutate(|queue_status| {
				Self::update_spot_traffic(&config, queue_status);
			});

			// Reads: `Revenue`, `ActiveConfig`, `QueueStatus`
			// Writes: `Revenue`, `QueueStatus`
			T::DbWeight::get().reads_writes(3, 2)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a single on demand core order.
		/// Will use the spot price for the current block and will reap the account if needed.
		///
		/// Parameters:
		/// - `origin`: The sender of the call, funds will be withdrawn from this account.
		/// - `max_amount`: The maximum balance to withdraw from the origin to place an order.
		/// - `para_id`: A `ParaId` the origin wants to provide blockspace for.
		///
		/// Errors:
		/// - `InsufficientBalance`: from the Currency implementation
		/// - `QueueFull`
		/// - `SpotPriceHigherThanMaxAmount`
		///
		/// Events:
		/// - `OnDemandOrderPlaced`
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::place_order_allow_death(QueueStatus::<T>::get().size()))]
		pub fn place_order_allow_death(
			origin: OriginFor<T>,
			max_amount: BalanceOf<T>,
			para_id: ParaId,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			Pallet::<T>::do_place_order(sender, max_amount, para_id, AllowDeath)
		}

		/// Same as the [`place_order_allow_death`](Self::place_order_allow_death) call , but with a
		/// check that placing the order will not reap the account.
		///
		/// Parameters:
		/// - `origin`: The sender of the call, funds will be withdrawn from this account.
		/// - `max_amount`: The maximum balance to withdraw from the origin to place an order.
		/// - `para_id`: A `ParaId` the origin wants to provide blockspace for.
		///
		/// Errors:
		/// - `InsufficientBalance`: from the Currency implementation
		/// - `QueueFull`
		/// - `SpotPriceHigherThanMaxAmount`
		///
		/// Events:
		/// - `OnDemandOrderPlaced`
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::place_order_keep_alive(QueueStatus::<T>::get().size()))]
		pub fn place_order_keep_alive(
			origin: OriginFor<T>,
			max_amount: BalanceOf<T>,
			para_id: ParaId,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			Pallet::<T>::do_place_order(sender, max_amount, para_id, KeepAlive)
		}
	}
}

// Internal functions and interface to scheduler/wrapping assignment provider.
impl<T: Config> Pallet<T>
where
	BalanceOf<T>: FixedPointOperand,
{
	/// Take the next queued entry that is available for a given core index.
	///
	/// Parameters:
	/// - `core_index`: The core index
	pub fn pop_assignment_for_core(core_index: CoreIndex) -> Option<Assignment> {
		let entry: Result<EnqueuedOrder, ()> = QueueStatus::<T>::try_mutate(|queue_status| {
			AffinityEntries::<T>::try_mutate(core_index, |affinity_entries| {
				let free_entry = FreeEntries::<T>::try_mutate(|free_entries| {
					let affinity_next = affinity_entries.peek();
					let free_next = free_entries.peek();
					let pick_free = match (affinity_next, free_next) {
						(None, _) => true,
						(Some(_), None) => false,
						(Some(a), Some(f)) => f < a,
					};
					if pick_free {
						let entry = free_entries.pop().ok_or(())?;
						let (mut affinities, free): (BinaryHeap<_>, BinaryHeap<_>) =
							take(free_entries)
								.into_iter()
								.partition(|e| e.para_id == entry.para_id);
						affinity_entries.append(&mut affinities);
						*free_entries = free;
						Ok(entry)
					} else {
						Err(())
					}
				});
				let entry = free_entry.or_else(|()| affinity_entries.pop().ok_or(()))?;
				queue_status.consume_index(entry.idx);
				Ok(entry)
			})
		});

		let assignment = entry.map(|e| Assignment::Pool { para_id: e.para_id, core_index }).ok()?;

		Pallet::<T>::increase_affinity(assignment.para_id(), core_index);
		Some(assignment)
	}

	/// Report that an assignment was duplicated by the scheduler.
	pub fn assignment_duplicated(para_id: ParaId, core_index: CoreIndex) {
		Pallet::<T>::increase_affinity(para_id, core_index);
	}

	/// Report that the `para_id` & `core_index` combination was processed.
	///
	/// This should be called once it is clear that the assignment won't get pushed back anymore.
	///
	/// In other words for each `pop_assignment_for_core` a call to this function or
	/// `push_back_assignment` must follow, but only one.
	pub fn report_processed(para_id: ParaId, core_index: CoreIndex) {
		Pallet::<T>::decrease_affinity_update_queue(para_id, core_index);
	}

	/// Push an assignment back to the front of the queue.
	///
	/// The assignment has not been processed yet. Typically used on session boundaries.
	///
	/// NOTE: We are not checking queue size here. So due to push backs it is possible that we
	/// exceed the maximum queue size slightly.
	///
	/// Parameters:
	/// - `para_id`: The para that did not make it.
	/// - `core_index`: The core the para was scheduled on.
	pub fn push_back_assignment(para_id: ParaId, core_index: CoreIndex) {
		Pallet::<T>::decrease_affinity_update_queue(para_id, core_index);
		QueueStatus::<T>::mutate(|queue_status| {
			Pallet::<T>::add_on_demand_order(queue_status, para_id, QueuePushDirection::Front);
		});
	}

	/// Helper function for `place_order_*` calls. Used to differentiate between placing orders
	/// with a keep alive check or to allow the account to be reaped. The amount charged is
	/// stored to the pallet account to be later paid out as revenue.
	///
	/// Parameters:
	/// - `sender`: The sender of the call, funds will be withdrawn from this account.
	/// - `max_amount`: The maximum balance to withdraw from the origin to place an order.
	/// - `para_id`: A `ParaId` the origin wants to provide blockspace for.
	/// - `existence_requirement`: Whether or not to ensure that the account will not be reaped.
	///
	/// Errors:
	/// - `InsufficientBalance`: from the Currency implementation
	/// - `QueueFull`
	/// - `SpotPriceHigherThanMaxAmount`
	///
	/// Events:
	/// - `OnDemandOrderPlaced`
	fn do_place_order(
		sender: <T as frame_system::Config>::AccountId,
		max_amount: BalanceOf<T>,
		para_id: ParaId,
		existence_requirement: ExistenceRequirement,
	) -> DispatchResult {
		let config = configuration::ActiveConfig::<T>::get();

		QueueStatus::<T>::mutate(|queue_status| {
			Self::update_spot_traffic(&config, queue_status);
			let traffic = queue_status.traffic;

			// Calculate spot price
			let spot_price: BalanceOf<T> = traffic.saturating_mul_int(
				config.scheduler_params.on_demand_base_fee.saturated_into::<BalanceOf<T>>(),
			);

			// Is the current price higher than `max_amount`
			ensure!(spot_price.le(&max_amount), Error::<T>::SpotPriceHigherThanMaxAmount);

			ensure!(
				queue_status.size() < config.scheduler_params.on_demand_queue_max_size,
				Error::<T>::QueueFull
			);

			// Charge the sending account the spot price. The amount will be teleported to the
			// broker chain once it requests revenue information.
			let amt = T::Currency::withdraw(
				&sender,
				spot_price,
				WithdrawReasons::FEE,
				existence_requirement,
			)?;

			// Consume the negative imbalance and deposit it into the pallet account. Make sure the
			// account preserves even without the existential deposit.
			let pot = Self::account_id();
			if !System::<T>::account_exists(&pot) {
				System::<T>::inc_providers(&pot);
			}
			T::Currency::resolve_creating(&pot, amt);

			// Add the amount to the current block's (index 0) revenue information.
			Revenue::<T>::mutate(|bounded_revenue| {
				if let Some(current_block) = bounded_revenue.get_mut(0) {
					*current_block = current_block.saturating_add(spot_price);
				} else {
					// Revenue has already been claimed in the same block, including the block
					// itself. It shouldn't normally happen as revenue claims in the future are
					// not allowed.
					bounded_revenue.try_push(spot_price).defensive_ok();
				}
			});

			Pallet::<T>::add_on_demand_order(queue_status, para_id, QueuePushDirection::Back);
			Pallet::<T>::deposit_event(Event::<T>::OnDemandOrderPlaced {
				para_id,
				spot_price,
				ordered_by: sender,
			});

			Ok(())
		})
	}

	/// Calculate and update spot traffic.
	fn update_spot_traffic(
		config: &configuration::HostConfiguration<BlockNumberFor<T>>,
		queue_status: &mut QueueStatusType,
	) {
		let old_traffic = queue_status.traffic;
		match Self::calculate_spot_traffic(
			old_traffic,
			config.scheduler_params.on_demand_queue_max_size,
			queue_status.size(),
			config.scheduler_params.on_demand_target_queue_utilization,
			config.scheduler_params.on_demand_fee_variability,
		) {
			Ok(new_traffic) => {
				// Only update storage on change
				if new_traffic != old_traffic {
					queue_status.traffic = new_traffic;

					// calculate the new spot price
					let spot_price: BalanceOf<T> = new_traffic.saturating_mul_int(
						config.scheduler_params.on_demand_base_fee.saturated_into::<BalanceOf<T>>(),
					);

					// emit the event for updated new price
					Pallet::<T>::deposit_event(Event::<T>::SpotPriceSet { spot_price });
				}
			},
			Err(err) => {
				log::debug!(
					target: LOG_TARGET,
					"Error calculating spot traffic: {:?}", err
				);
			},
		};
	}

	/// The spot price multiplier. This is based on the transaction fee calculations defined in:
	/// https://research.web3.foundation/Polkadot/overview/token-economics#setting-transaction-fees
	///
	/// Parameters:
	/// - `traffic`: The previously calculated multiplier, can never go below 1.0.
	/// - `queue_capacity`: The max size of the order book.
	/// - `queue_size`: How many orders are currently in the order book.
	/// - `target_queue_utilisation`: How much of the queue_capacity should be ideally occupied,
	///   expressed in percentages(perbill).
	/// - `variability`: A variability factor, i.e. how quickly the spot price adjusts. This number
	///   can be chosen by p/(k*(1-s)) where p is the desired ratio increase in spot price over k
	///   number of blocks. s is the target_queue_utilisation. A concrete example: v =
	///   0.05/(20*(1-0.25)) = 0.0033.
	///
	/// Returns:
	/// - A `FixedU128` in the range of  `Config::TrafficDefaultValue` - `FixedU128::MAX` on
	///   success.
	///
	/// Errors:
	/// - `SpotTrafficCalculationErr::QueueCapacityIsZero`
	/// - `SpotTrafficCalculationErr::QueueSizeLargerThanCapacity`
	/// - `SpotTrafficCalculationErr::Division`
	fn calculate_spot_traffic(
		traffic: FixedU128,
		queue_capacity: u32,
		queue_size: u32,
		target_queue_utilisation: Perbill,
		variability: Perbill,
	) -> Result<FixedU128, SpotTrafficCalculationErr> {
		// Return early if queue has no capacity.
		if queue_capacity == 0 {
			return Err(SpotTrafficCalculationErr::QueueCapacityIsZero)
		}

		// Return early if queue size is greater than capacity.
		if queue_size > queue_capacity {
			return Err(SpotTrafficCalculationErr::QueueSizeLargerThanCapacity)
		}

		// (queue_size / queue_capacity) - target_queue_utilisation
		let queue_util_ratio = FixedU128::from_rational(queue_size.into(), queue_capacity.into());
		let positive = queue_util_ratio >= target_queue_utilisation.into();
		let queue_util_diff = queue_util_ratio.max(target_queue_utilisation.into()) -
			queue_util_ratio.min(target_queue_utilisation.into());

		// variability * queue_util_diff
		let var_times_qud = queue_util_diff.saturating_mul(variability.into());

		// variability^2 * queue_util_diff^2
		let var_times_qud_pow = var_times_qud.saturating_mul(var_times_qud);

		// (variability^2 * queue_util_diff^2)/2
		let div_by_two: FixedU128;
		match var_times_qud_pow.const_checked_div(2.into()) {
			Some(dbt) => div_by_two = dbt,
			None => return Err(SpotTrafficCalculationErr::Division),
		}

		// traffic * (1 + queue_util_diff) + div_by_two
		if positive {
			let new_traffic = queue_util_diff
				.saturating_add(div_by_two)
				.saturating_add(One::one())
				.saturating_mul(traffic);
			Ok(new_traffic.max(<T as Config>::TrafficDefaultValue::get()))
		} else {
			let new_traffic = queue_util_diff.saturating_sub(div_by_two).saturating_mul(traffic);
			Ok(new_traffic.max(<T as Config>::TrafficDefaultValue::get()))
		}
	}

	/// Adds an order to the on demand queue.
	///
	/// Parameters:
	/// - `location`: Whether to push this entry to the back or the front of the queue. Pushing an
	///   entry to the front of the queue is only used when the scheduler wants to push back an
	///   entry it has already popped.
	fn add_on_demand_order(
		queue_status: &mut QueueStatusType,
		para_id: ParaId,
		location: QueuePushDirection,
	) {
		let idx = match location {
			QueuePushDirection::Back => queue_status.push_back(),
			QueuePushDirection::Front => queue_status.push_front(),
		};

		let affinity = ParaIdAffinity::<T>::get(para_id);
		let order = EnqueuedOrder::new(idx, para_id);
		#[cfg(test)]
		log::debug!(target: LOG_TARGET, "add_on_demand_order, order: {:?}, affinity: {:?}, direction: {:?}", order, affinity, location);

		match affinity {
			None => FreeEntries::<T>::mutate(|entries| entries.push(order)),
			Some(affinity) =>
				AffinityEntries::<T>::mutate(affinity.core_index, |entries| entries.push(order)),
		}
	}

	/// Decrease core affinity for para and update queue
	///
	/// if affinity dropped to 0, moving entries back to `FreeEntries`.
	fn decrease_affinity_update_queue(para_id: ParaId, core_index: CoreIndex) {
		let affinity = Pallet::<T>::decrease_affinity(para_id, core_index);
		#[cfg(not(test))]
		debug_assert_ne!(
			affinity, None,
			"Decreased affinity for a para that has not been served on a core?"
		);
		if affinity != Some(0) {
			return;
		}
		// No affinity more for entries on this core, free any entries:
		//
		// This is necessary to ensure them being served as the core might no longer exist at all.
		AffinityEntries::<T>::mutate(core_index, |affinity_entries| {
			FreeEntries::<T>::mutate(|free_entries| {
				let (mut freed, affinities): (BinaryHeap<_>, BinaryHeap<_>) =
					take(affinity_entries).into_iter().partition(|e| e.para_id == para_id);
				free_entries.append(&mut freed);
				*affinity_entries = affinities;
			})
		});
	}

	/// Decreases the affinity of a `ParaId` to a specified `CoreIndex`.
	///
	/// Subtracts from the count of the `CoreAffinityCount` if an entry is found and the core_index
	/// matches. When the count reaches 0, the entry is removed.
	/// A non-existent entry is a no-op.
	///
	/// Returns: The new affinity of the para on that core. `None` if there is no affinity on this
	/// core.
	fn decrease_affinity(para_id: ParaId, core_index: CoreIndex) -> Option<u32> {
		ParaIdAffinity::<T>::mutate(para_id, |maybe_affinity| {
			let affinity = maybe_affinity.as_mut()?;
			if affinity.core_index == core_index {
				let new_count = affinity.count.saturating_sub(1);
				if new_count > 0 {
					*maybe_affinity = Some(CoreAffinityCount { core_index, count: new_count });
				} else {
					*maybe_affinity = None;
				}
				return Some(new_count);
			} else {
				None
			}
		})
	}

	/// Increases the affinity of a `ParaId` to a specified `CoreIndex`.
	/// Adds to the count of the `CoreAffinityCount` if an entry is found and the core_index
	/// matches. A non-existent entry will be initialized with a count of 1 and uses the  supplied
	/// `CoreIndex`.
	fn increase_affinity(para_id: ParaId, core_index: CoreIndex) {
		ParaIdAffinity::<T>::mutate(para_id, |maybe_affinity| match maybe_affinity {
			Some(affinity) =>
				if affinity.core_index == core_index {
					*maybe_affinity = Some(CoreAffinityCount {
						core_index,
						count: affinity.count.saturating_add(1),
					});
				},
			None => {
				*maybe_affinity = Some(CoreAffinityCount { core_index, count: 1 });
			},
		})
	}

	/// Collect the revenue from the `when` blockheight
	pub fn claim_revenue_until(when: BlockNumberFor<T>) -> BalanceOf<T> {
		let now = <frame_system::Pallet<T>>::block_number();
		let mut amount: BalanceOf<T> = BalanceOf::<T>::zero();
		Revenue::<T>::mutate(|revenue| {
			while !revenue.is_empty() {
				let index = (revenue.len() - 1) as u32;
				if when > now.saturating_sub(index.into()) {
					amount = amount.saturating_add(revenue.pop().defensive_unwrap_or(0u32.into()));
				} else {
					break
				}
			}
		});

		amount
	}

	/// Account of the pallet pot, where the funds from instantaneous coretime sale are accumulated.
	pub fn account_id() -> T::AccountId {
		T::PalletId::get().into_account_truncating()
	}

	/// Getter for the affinity tracker.
	#[cfg(test)]
	fn get_affinity_map(para_id: ParaId) -> Option<CoreAffinityCount> {
		ParaIdAffinity::<T>::get(para_id)
	}

	/// Getter for the affinity entries.
	#[cfg(test)]
	fn get_affinity_entries(core_index: CoreIndex) -> BinaryHeap<EnqueuedOrder> {
		AffinityEntries::<T>::get(core_index)
	}

	/// Getter for the free entries.
	#[cfg(test)]
	fn get_free_entries() -> BinaryHeap<EnqueuedOrder> {
		FreeEntries::<T>::get()
	}

	#[cfg(feature = "runtime-benchmarks")]
	pub fn populate_queue(para_id: ParaId, num: u32) {
		QueueStatus::<T>::mutate(|queue_status| {
			for _ in 0..num {
				Pallet::<T>::add_on_demand_order(queue_status, para_id, QueuePushDirection::Back);
			}
		});
	}

	#[cfg(test)]
	fn set_queue_status(new_status: QueueStatusType) {
		QueueStatus::<T>::set(new_status);
	}

	#[cfg(test)]
	fn get_queue_status() -> QueueStatusType {
		QueueStatus::<T>::get()
	}

	#[cfg(test)]
	fn get_traffic_default_value() -> FixedU128 {
		<T as Config>::TrafficDefaultValue::get()
	}

	#[cfg(test)]
	fn get_revenue() -> Vec<BalanceOf<T>> {
		Revenue::<T>::get().to_vec()
	}
}
