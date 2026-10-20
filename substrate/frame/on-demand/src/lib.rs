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

//! # On-demand Coretime pallet
//!
//! Sale of on-demand Coretime from the Coretime chain.
//!
//! The Relay chain owns the actual on-demand order queue; this pallet is a front-end for it which
//! lives alongside [`pallet-broker`](https://docs.rs/pallet-broker) on the Coretime chain:
//!
//! - Users call [`Pallet::place_order`] here and pay the spot price in the local currency.
//! - Because the real queue is one hop away, the spot price cannot be read directly. Instead the
//!   pallet keeps a local estimate of the queue's depth ([`QueueState`]) which grows with every
//!   order placed and shrinks by an assumed drain rate per Relay-chain block. The spot price is
//!   derived from that estimate using [`PriceParameters`].
//! - Orders accepted within a block are accumulated in [`PendingBatch`] and forwarded to the Relay
//!   chain in one go on finalization, via [`QueueOnDemandOrders`].
//! - The spot prices paid are collected in the pallet's pot and booked as [`Revenue`] against the
//!   timeslice the order was placed in. `pallet-broker` claims the revenue of each timeslice once
//!   it has ended, along with that of any earlier timeslice which could not be paid out before,
//!   through [`fp_coretime::revenue::OnDemandRevenue`], and pays it out to the contributors to the
//!   Instantaneous Coretime Pool.
//!
//!   NOTE: It is important to make sure that this pallet is ordered before `ParachainSystem` in the
//!   runtime - otherwise the messages will not be sent in the block in which they are created,
//!   introducing additional latency.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

mod benchmarking;
mod types;
mod weightinfo_extension;
pub mod weights;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use alloc::vec::Vec;
use fp_coretime::{TaskId, Timeslice};
use frame_support::traits::EnsureOrigin;
use sp_runtime::traits::BlockNumberProvider;

pub use types::*;
pub use weightinfo_extension::WeightInfoExt;
pub use weights::WeightInfo;

/// The logging target of this pallet.
const LOG_TARGET: &str = "runtime::on-demand";

/// The default maximum number of outstanding on-demand orders beyond which new orders will be
/// rejected.
const DEFAULT_ORDER_CAP: u32 = 100;

/// The default number of orders assumed to be drained out of the order queue per Relay-chain
/// block.
const DEFAULT_DRAIN_RATE_PER_BLOCK: u32 = 1;

/// The default percentage by which every additional on-demand order in the queue increases
/// the spot price for new orders.
const DEFAULT_PRICE_STEP: u32 = 3;

/// The default base fee for an on-demand order, which will be the spot price when the queue
/// is empty.
const DEFAULT_BASE_FEE: u32 = 10_000_000;

/// The Relay-chain block number, as seen by this pallet.
pub type RelayBlockNumberOf<T> =
	<<T as Config>::RelayBlockNumberProvider as BlockNumberProvider>::BlockNumber;

/// Instructs the Relay chain to enqueue a batch of on-demand orders.
///
/// On the Coretime chain this is implemented by sending an XCM `Transact` to the Relay chain's
/// coretime pallet.
pub trait QueueOnDemandOrders<RelayBlockNumber> {
	/// Enqueue `batch` on the Relay chain.
	///
	/// Each entry is the parachain the order was placed for and the Relay-chain block number it was
	/// ordered at.
	fn queue_batch(batch: Vec<(TaskId, RelayBlockNumber)>);

	/// Make sure that the batches queued afterwards reach the Relay chain.
	///
	/// Used to set up the benchmarks, so that they measure a successful delivery.
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful_delivery();
}

impl<RelayBlockNumber> QueueOnDemandOrders<RelayBlockNumber> for () {
	fn queue_batch(_batch: Vec<(TaskId, RelayBlockNumber)>) {}

	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful_delivery() {}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use fp_coretime::revenue::{OnDemandRevenue, RevenueClaim};
	use frame_support::{
		pallet_prelude::*,
		traits::{
			fungible::{Inspect, Mutate},
			tokens::{Fortitude::Polite, Preservation::Preserve},
		},
		PalletId,
	};
	use frame_system::pallet_prelude::*;
	use sp_arithmetic::traits::{SaturatedConversion, Saturating, Zero};
	use sp_runtime::traits::AccountIdConversion;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Weight information for all calls of this pallet.
		type WeightInfo: WeightInfo;

		/// Currency used to pay for on-demand Coretime.
		type Currency: Mutate<Self::AccountId>;

		/// The origin for administrating this pallet.
		type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Provider of the current Relay-chain block number.
		type RelayBlockNumberProvider: BlockNumberProvider;

		/// Provider of the on-demand pool capacity.
		type PoolCapacityProvider: PoolCapacityProvider;

		/// Provider of the order pricing algorithm.
		type PricingProvider: PricingProvider<BalanceOf<Self>>;

		/// Used to instruct the Relay chain to enqueue the orders placed here.
		type OrderQueue: QueueOnDemandOrders<RelayBlockNumberOf<Self>>;

		/// Maximum pending batch size.
		/// NOTE: Since we don't do chunking, this number of pending orders needs to fit within a
		/// single XCM message. This way `on_finalize` will never send more than one message.
		/// Since this pallet is intended to be a temporary solution, and current (as of September
		/// 2026) usage of the on-demand feature is low to moderate, it is expected not to be
		/// necessary to extend this functionality to allow more orders in a single block.
		#[pallet::constant]
		type MaxBatchSize: Get<u32>;

		/// The number of Relay-chain blocks in a timeslice.
		///
		/// Revenue is booked against the timeslice an order was placed in, so this must match the
		/// `TimeslicePeriod` of the `pallet-broker` instance claiming it.
		#[pallet::constant]
		type TimeslicePeriod: Get<RelayBlockNumberOf<Self>>;

		/// The maximum number of timeslices whose unclaimed revenue is kept apart.
		///
		/// Normally only the revenue of the current timeslice and of the one which has just ended
		/// is unclaimed, but more builds up while the revenue cannot be paid out. Once this many
		/// timeslices have unclaimed revenue, the revenue of the oldest one is merged into the
		/// next oldest one, so it is never lost, but is attributed to a later timeslice. This may
		/// cause the revenue to be paid to a different set of contributors, which is why this
		/// number should be big enough to minimize the probability of that happening.
		#[pallet::constant]
		type MaxUnclaimedTimeslices: Get<u32>;

		/// Identifier from which the internal Pot is generated.
		#[pallet::constant]
		type PalletId: Get<PalletId>;
	}

	/// The configuration used for pricing on-demand Coretime orders.
	#[pallet::storage]
	pub type PriceConfig<T> = StorageValue<_, PriceParametersOf<T>, ValueQuery>;

	/// The local estimate of the Relay chain's on-demand order queue.
	#[pallet::storage]
	pub type QueueState<T> = StorageValue<_, QueueTrackerOf<T>, OptionQuery>;

	/// Orders placed in the current block, forwarded to the Relay chain on finalization.
	#[pallet::storage]
	pub type PendingBatch<T: Config> = StorageValue<
		_,
		BoundedVec<EnqueuedOrder<RelayBlockNumberOf<T>>, T::MaxBatchSize>,
		ValueQuery,
	>;

	/// Revenue from on-demand sales which has not been claimed yet, by the timeslice the orders
	/// were placed in, newest first.
	///
	/// The funds themselves sit in the pallet's pot until they are claimed through
	/// [`OnDemandRevenue::claim_revenue`].
	#[pallet::storage]
	pub type Revenue<T: Config> =
		StorageValue<_, BoundedVec<RevenueRecordOf<T>, T::MaxUnclaimedTimeslices>, ValueQuery>;

	/// Whether on-demand Coretime is sold here.
	///
	/// Orders are only accepted while it is, as the revenue from them is only claimed through
	/// [`OnDemandRevenue::claim_revenue`] while it is; otherwise it would never be distributed.
	/// Revenue booked before it was disabled stays booked until it is enabled again.
	///
	/// It is `false` until enabled with [`Pallet::set_enabled`].
	#[pallet::storage]
	pub type Enabled<T> = StorageValue<_, bool, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An on-demand order was placed at `spot_price` by `ordered_by`.
		OrderPlaced {
			/// The parachain the order was placed for.
			para_id: TaskId,
			/// The spot price that was paid for the order.
			spot_price: BalanceOf<T>,
			/// The account that placed and paid for the order.
			ordered_by: T::AccountId,
		},
		/// Revenue from on-demand sales was claimed and paid out to `beneficiary`.
		RevenueClaimed {
			/// The timeslice the orders the revenue came from were placed in.
			when: Timeslice,
			/// The amount that was paid out.
			amount: BalanceOf<T>,
			/// The account the revenue was paid out to.
			beneficiary: T::AccountId,
		},
		/// The funds in pot were unexpectedly too low to handle a revenue claim. The revenue of
		/// the oldest timeslices was paid out as far as the funds allowed; the rest stays booked
		/// until the pot is topped up.
		UnexpectedLowFunds {
			/// The amount that was being claimed.
			attempted_to_claim: BalanceOf<T>,
			/// The claimable amount in the pot.
			claimable: BalanceOf<T>,
			/// The amount that was actually paid out.
			paid_out: BalanceOf<T>,
		},
		/// On-demand Coretime sales were enabled or disabled.
		EnabledSet {
			/// Whether on-demand Coretime is sold now.
			enabled: bool,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The estimated on-demand order queue has reached its order cap.
		QueueFull,
		/// The batch of orders pending to be sent to the Relay chain is full.
		BatchFull,
		/// The spot price was higher than the maximum amount declared in `place_order`.
		SpotPriceHigherThanMaxAmount,
		/// The on-demand pool has no cores assigned.
		EmptyPool,
		/// The funds in the account submitting the order were not sufficient to cover the declared
		/// `max_amount`.
		/// Note: this can be returned even if the funds are sufficient to cover the actual spot
		/// price.
		InsufficientFunds,
		/// The price parameters allow the price to overflow with too many outstanding orders.
		OrderPriceCanOverflow,
		/// On-demand Coretime sales are disabled.
		Disabled,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_now: BlockNumberFor<T>) -> Weight {
			T::WeightInfo::on_finalize_block_fixed()
		}

		fn integrity_test() {
			assert!(
				T::MaxUnclaimedTimeslices::get() > 0,
				"`MaxUnclaimedTimeslices` must allow for the revenue of at least one timeslice"
			);
			assert!(
				T::TimeslicePeriod::get() > 0u32.into(),
				"The length of a timeslice cannot be 0"
			);
		}

		fn on_finalize(_now: BlockNumberFor<T>) {
			let batch = PendingBatch::<T>::take().into_inner();
			if batch.is_empty() {
				return;
			}

			T::OrderQueue::queue_batch(
				batch.into_iter().map(|order| (order.para_id, order.ordered_at)).collect(),
			);
		}
	}

	#[pallet::call(weight(<T as Config>::WeightInfo))]
	impl<T: Config> Pallet<T> {
		/// Configure the pallet.
		///
		/// - `origin`: Must be Root or pass `AdminOrigin`.
		/// - `config`: The configuration for this pallet.
		#[pallet::call_index(0)]
		pub fn configure(
			origin: OriginFor<T>,
			config: PriceParametersOf<T>,
		) -> DispatchResultWithPostInfo {
			T::AdminOrigin::ensure_origin_or_root(origin)?;
			config.validate::<T>()?;
			PriceConfig::<T>::put(config);
			Ok(Pays::No.into())
		}

		/// Place an on-demand Coretime order for `para_id`.
		///
		/// The caller is charged the current estimated spot price, which must not exceed
		/// `max_amount`. The order is forwarded to the Relay chain at the end of the block.
		///
		/// - `origin`: Must be a signed account with enough funds to pay the spot price.
		/// - `para_id`: The parachain to schedule.
		/// - `max_amount`: The maximum spot price the caller is willing to pay.
		#[pallet::call_index(1)]
		#[pallet::weight(
			<T as Config>::WeightInfo::place_order()
			.saturating_add(T::WeightInfo::on_finalize_block_per_order())
		)]
		pub fn place_order(
			origin: OriginFor<T>,
			para_id: TaskId,
			max_amount: BalanceOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			// TODO(ahm-v2): add a check that the para_id is valid
			Self::do_place_order(who, para_id, max_amount)
		}

		/// Enable or disable on-demand Coretime sales.
		///
		/// While they are disabled, no orders are accepted, and the revenue from the earlier ones
		/// is not claimed.
		///
		/// - `origin`: Must be Root or pass `AdminOrigin`.
		/// - `enabled`: Whether on-demand Coretime is to be sold.
		#[pallet::call_index(2)]
		pub fn set_enabled(origin: OriginFor<T>, enabled: bool) -> DispatchResultWithPostInfo {
			T::AdminOrigin::ensure_origin_or_root(origin)?;
			Enabled::<T>::put(enabled);
			Self::deposit_event(Event::<T>::EnabledSet { enabled });
			Ok(Pays::No.into())
		}
	}

	impl<T: Config> Pallet<T> {
		/// The account holding the revenue from on-demand Coretime sales.
		pub fn account_id() -> T::AccountId {
			T::PalletId::get().into_account_truncating()
		}

		pub(crate) fn do_place_order(
			who: T::AccountId,
			para_id: TaskId,
			max_amount: BalanceOf<T>,
		) -> DispatchResult {
			// Fail early if the revenue of the order would never be distributed.
			ensure!(Enabled::<T>::get(), Error::<T>::Disabled);
			// Fail early if the batch is already full.
			ensure!(
				PendingBatch::<T>::decode_len().unwrap_or(0) < T::MaxBatchSize::get() as usize,
				Error::<T>::BatchFull
			);
			// Fail early if the account can't cover the declared max_amount.
			ensure!(
				T::Currency::reducible_balance(&who, Preserve, Polite) >= max_amount,
				Error::<T>::InsufficientFunds
			);

			let pool_cores = T::PoolCapacityProvider::pool_cores();
			// Fail early if the pool is empty.
			ensure!(pool_cores > 0, Error::<T>::EmptyPool);

			let now = T::RelayBlockNumberProvider::current_block_number();
			let mut queue_state = QueueState::<T>::get()
				.unwrap_or(QueueTracker { outstanding_orders: 0, last_updated: now });
			let pricing_config = PriceConfig::<T>::get();

			// Assume the Relay chain has drained part of the queue since we last looked at it.
			let elapsed = now.saturating_sub(queue_state.last_updated).saturated_into();

			let drained_orders = pricing_config
				.drain_rate_per_block
				.saturating_mul(elapsed)
				.saturating_mul(pool_cores);
			let outstanding_orders = queue_state.outstanding_orders.saturating_sub(drained_orders);

			ensure!(outstanding_orders < pricing_config.order_cap, Error::<T>::QueueFull);

			let spot_price = T::PricingProvider::spot_price(&pricing_config, outstanding_orders)?;

			ensure!(spot_price <= max_amount, Error::<T>::SpotPriceHigherThanMaxAmount);

			// Charge the sending account the spot price. The funds stay in the pot until the
			// revenue they represent is claimed.
			T::Currency::transfer(&who, &Self::account_id(), spot_price, Preserve)?;
			let when: Timeslice = (now / T::TimeslicePeriod::get()).saturated_into();
			Self::book_revenue(when, spot_price);

			// Add the order to the batch that gets sent to the Relay chain on finalization.
			PendingBatch::<T>::try_mutate(|batch| {
				batch
					.try_push(EnqueuedOrder { para_id, ordered_at: now })
					// should not happen (we check if it's full at the start), but won't hurt to
					// handle the error
					.map_err(|_| Error::<T>::BatchFull)
			})?;

			queue_state.outstanding_orders = outstanding_orders.saturating_add(1);
			queue_state.last_updated = now;
			QueueState::<T>::put(queue_state);

			Self::deposit_event(Event::<T>::OrderPlaced { para_id, spot_price, ordered_by: who });

			Ok(())
		}

		/// Book `amount` of revenue against timeslice `when`.
		fn book_revenue(when: Timeslice, amount: BalanceOf<T>) {
			Revenue::<T>::mutate(|revenue| {
				// The records are newest first, so this is where the record for `when` either is,
				// or belongs.
				let index = revenue.iter().position(|record| record.when <= when);
				if let Some(record) =
					index.and_then(|index| revenue.get_mut(index)).filter(|r| r.when == when)
				{
					record.amount.saturating_accrue(amount);
					return;
				}

				let index = index.unwrap_or(revenue.len());
				let overdue = if revenue.is_full() && index == revenue.len() {
					// `when` is older than every timeslice kept apart, so it is the one which is
					// overdue.
					RevenueRecord { when, amount }
				} else {
					match revenue.force_insert_keep_left(index, RevenueRecord { when, amount }) {
						Ok(None) => return,
						Ok(Some(overdue)) => overdue,
						// Only possible with `MaxUnclaimedTimeslices` set to zero, which the
						// integrity test rules out.
						Err(_) => {
							frame_support::defensive!("Revenue could not be booked");
							return;
						},
					}
				};

				// The revenue of the oldest timeslice has not been claimed for too long; keep it
				// with the next oldest one.
				log::warn!(
					target: LOG_TARGET,
					"Unclaimed revenue of timeslice {} merged into a later timeslice",
					overdue.when,
				);
				// We just ensured that we couldn't insert the overdue amount because the revenue
				// vec was full, so it can't be empty, so there will always be a last entry. qed
				let oldest = revenue.last_mut().unwrap();
				oldest.amount.saturating_accrue(overdue.amount);
			});
		}
	}

	impl<T: Config> OnDemandRevenue<BalanceOf<T>, T::AccountId> for Pallet<T> {
		fn is_enabled() -> bool {
			Enabled::<T>::get()
		}

		fn claim_revenue(
			up_to: Timeslice,
			beneficiary: &T::AccountId,
		) -> RevenueClaim<BalanceOf<T>> {
			let (paid, claimable) = Self::do_claim_revenue(up_to, beneficiary);
			RevenueClaim { paid, weight: <T as Config>::WeightInfo::claim_revenue(claimable) }
		}
	}

	impl<T: Config> Pallet<T> {
		/// Implements [`OnDemandRevenue::claim_revenue`], returning the paid out revenue and the
		/// number of timeslices whose revenue was claimable.
		fn do_claim_revenue(
			up_to: Timeslice,
			beneficiary: &T::AccountId,
		) -> (Vec<(Timeslice, BalanceOf<T>)>, u32) {
			let mut revenue = Revenue::<T>::get();
			// The records are newest first, so the claimable ones are at the end.
			let claimable = revenue.iter().filter(|record| record.when <= up_to).count() as u32;
			// Early return when nothing to be claimed: everything up to `up_to` is already paid
			// out, so it is settled even without revenue.
			if claimable == 0 {
				return (Vec::from([(up_to, Zero::zero())]), 0);
			}

			let pot = Self::account_id();
			// The pot is kept alive so that the (possibly sub-existential) payments for individual
			// orders keep working.
			let reducible = T::Currency::reducible_balance(&pot, Preserve, Polite);

			// Take the revenue of as many timeslices as can be paid out in full, oldest first.
			let mut total = BalanceOf::<T>::zero();
			let mut paid = Vec::new();
			while let Some(oldest) = revenue.last().filter(|record| record.when <= up_to) {
				let with_oldest = total.saturating_add(oldest.amount);
				if with_oldest > reducible {
					break;
				}
				total = with_oldest;
				paid.push((oldest.when, oldest.amount));
				revenue.pop();
			}

			let complete = revenue.last().map_or(true, |record| record.when > up_to);
			if complete && paid.last().map_or(true, |&(when, _)| when != up_to) {
				// Everything up to `up_to` was paid out, so it is settled even without revenue.
				paid.push((up_to, Zero::zero()));
			}
			if !complete {
				// Some revenue can only be paid out once the pot has been topped up.
				let booked = revenue
					.iter()
					.filter(|record| record.when <= up_to)
					.fold(total, |booked, record| booked.saturating_add(record.amount));
				log::error!(
					target: LOG_TARGET,
					"The pot holds less than the {booked:?} of revenue booked up to timeslice \
					 {up_to}; paying out {total:?} instead.",
				);
				Self::deposit_event(Event::<T>::UnexpectedLowFunds {
					attempted_to_claim: booked,
					claimable: reducible,
					paid_out: total,
				});
			}

			if !total.is_zero() {
				if let Err(err) = T::Currency::transfer(&pot, beneficiary, total, Preserve) {
					// The revenue stays booked, to be claimed along with a later timeslice.
					log::error!(
						target: LOG_TARGET,
						"Paying out {total:?} of revenue failed: {err:?}",
					);
					return (Vec::new(), claimable);
				}
			}

			Revenue::<T>::put(revenue);
			for &(when, amount) in paid.iter().filter(|(_, amount)| !amount.is_zero()) {
				Self::deposit_event(Event::<T>::RevenueClaimed {
					when,
					amount,
					beneficiary: beneficiary.clone(),
				});
			}

			(paid, claimable)
		}
	}
}
