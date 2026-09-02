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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

mod types;
pub mod weights;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use alloc::vec::Vec;
use fp_coretime::TaskId;
use frame_support::traits::EnsureOrigin;
use sp_runtime::traits::BlockNumberProvider;

pub use types::*;
pub use weights::WeightInfo;

/// Maximum pending batch size.
const MAX_BATCH_SIZE: u32 = 1000;

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
}

impl<RelayBlockNumber> QueueOnDemandOrders<RelayBlockNumber> for () {
	fn queue_batch(_batch: Vec<(TaskId, RelayBlockNumber)>) {}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{
		pallet_prelude::*,
		traits::{fungible::Mutate, tokens::Preservation::Expendable},
		PalletId,
	};
	use frame_system::pallet_prelude::*;
	use sp_arithmetic::{
		traits::{ensure_pow, One, SaturatedConversion, Saturating},
		FixedPointNumber, FixedU128,
	};
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

		/// The origin test needed for administrating this pallet.
		type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Provider of the current Relay-chain block number.
		type RelayBlockNumberProvider: BlockNumberProvider;

		/// Used to instruct the Relay chain to enqueue the orders placed here.
		type OrderQueue: QueueOnDemandOrders<RelayBlockNumberOf<Self>>;

		/// Identifier from which the internal Pot is generated.
		#[pallet::constant]
		type PalletId: Get<PalletId>;
	}

	/// The configuration used for pricing on-demand Coretime orders.
	#[pallet::storage]
	pub type PriceConfig<T> = StorageValue<_, PriceParametersOf<T>, OptionQuery>;

	/// The local estimate of the Relay chain's on-demand order queue.
	#[pallet::storage]
	pub type QueueState<T> = StorageValue<_, QueueTrackerOf<T>, OptionQuery>;

	/// Orders placed in the current block, forwarded to the Relay chain on finalization.
	#[pallet::storage]
	pub type PendingBatch<T> = StorageValue<
		_,
		BoundedVec<EnqueuedOrder<RelayBlockNumberOf<T>>, ConstU32<MAX_BATCH_SIZE>>,
		ValueQuery,
	>;

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
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The estimated on-demand order queue has reached its order cap.
		QueueFull,
		/// The batch of orders pending to be sent to the Relay chain is full.
		BatchFull,
		/// The spot price was higher than the maximum amount declared in `place_order`.
		SpotPriceHigherThanMaxAmount,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_now: BlockNumberFor<T>) -> Weight {
			// TODO: benchmark `on_finalize`.
			T::DbWeight::get().reads_writes(1, 2)
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
		pub fn place_order(
			origin: OriginFor<T>,
			para_id: TaskId,
			max_amount: BalanceOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Self::do_place_order(who, para_id, max_amount)
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
			let now = T::RelayBlockNumberProvider::current_block_number();
			let mut queue_state = QueueState::<T>::get()
				.unwrap_or(QueueTracker { outstanding_orders: 0, last_updated: now });
			let pricing_config = PriceConfig::<T>::get().unwrap_or_default();

			// Assume the Relay chain has drained part of the queue since we last looked at it.
			let elapsed = now.saturating_sub(queue_state.last_updated).saturated_into();
			let drained_orders = pricing_config.drain_rate_per_block.saturating_mul(elapsed);
			let outstanding_orders = queue_state.outstanding_orders.saturating_sub(drained_orders);

			ensure!(outstanding_orders < pricing_config.order_cap, Error::<T>::QueueFull);

			// Every order already outstanding raises the price by `price_step`.
			let price_adjustment = ensure_pow(
				FixedU128::one().saturating_add(FixedU128::from_perbill(pricing_config.price_step)),
				outstanding_orders as usize,
			)?;
			let spot_price = price_adjustment.saturating_mul_int(pricing_config.base_fee);

			ensure!(spot_price <= max_amount, Error::<T>::SpotPriceHigherThanMaxAmount);

			// Charge the sending account the spot price.
			T::Currency::transfer(&who, &Self::account_id(), spot_price, Expendable)?;

			// Add the order to the batch that gets sent to the Relay chain on finalization.
			PendingBatch::<T>::try_mutate(|batch| {
				batch
					.try_push(EnqueuedOrder { para_id, ordered_at: now })
					.map_err(|_| Error::<T>::BatchFull)
			})?;

			queue_state.outstanding_orders = outstanding_orders.saturating_add(1);
			queue_state.last_updated = now;
			QueueState::<T>::put(queue_state);

			Self::deposit_event(Event::<T>::OrderPlaced { para_id, spot_price, ordered_by: who });

			Ok(())
		}
	}
}
