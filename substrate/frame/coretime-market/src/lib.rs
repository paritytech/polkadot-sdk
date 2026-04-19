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

//! # Pallet Coretime Market
//!
//! Implements RFC-17: Coretime Market Redesign.
//!
//! This pallet provides the market logic for bulk coretime sales using a clearing-price
//! descending Dutch auction model. It operates in three phases per sale cycle:
//!
//! 1. **Market Phase**: A descending clock auction where bidders place bids at or below
//!    the current descending price. Bids are binding and can only be raised, not cancelled.
//!
//! 2. **Renewal Phase**: Existing tenants with renewal rights can exercise them. If all
//!    cores are allocated from the auction, renewers may displace the lowest non-renewer
//!    auction winner. A penalty applies to renewers who did not participate in the auction
//!    when the market was oversubscribed.
//!
//! 3. **Settlement Phase**: No primary sales occur. The pallet waits until the next
//!    sale's region begins before rotating into a new market cycle. Regions are issued
//!    at the transition from Renewal to Settlement.
//!
//! ## Design
//!
//! This pallet implements the [`Market`] trait from `pallet-broker`, allowing it to be used
//! by the broker pallet without direct coupling. The broker calls into the market trait for
//! order placement and processes [`TickAction`]s returned by [`Market::tick`] to perform
//! fund transfers, region issuance, and sale rotation.
//!
//! Key design decisions:
//! - **Clearing-price auction**: All winners pay the same uniform price (the Kth highest bid).
//! - **Lock-then-charge**: Funds are locked at bid time by the broker. At settlement, excess
//!   is refunded via [`TickAction::Refund`]. Winners are charged the clearing price.
//! - **Binding bids**: Bids cannot be cancelled, only raised.
//! - **Displacement protection**: Auction winners with renewal rights cannot be displaced.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
pub use types::*;

mod types;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

extern crate alloc;

use alloc::{vec, vec::Vec};
use frame_support::{
	ensure,
	traits::{tokens::Balance as BalanceT, Get},
	weights::{Weight, WeightMeter},
};
use pallet_broker::{
	market::{
		AdjustBidResult, CoreRangeProvider, Market, OrderResult, RenewalOrderResult,
		SalesStarted, TickAction, TimesliceProvider,
	},
	CoreIndex, CoreMask, PotentialRenewalId, RegionId, Timeslice,
};
use sp_arithmetic::{FixedPointNumber, Perbill};
use sp_runtime::{
	traits::{AtLeast32BitUnsigned, SaturatedConversion, Saturating, Zero},
	BoundedVec, FixedPointOperand, FixedU64,
};


type BalanceOf<T> = <T as pallet::Config>::Balance;
type RelayBlockNumberOf<T> = <T as pallet::Config>::RelayBlockNumber;
type ConfigRecordOf<T> = ConfigRecord<RelayBlockNumberOf<T>, BalanceOf<T>>;
type SaleInfoRecordOf<T> = SaleInfoRecord<BalanceOf<T>, RelayBlockNumberOf<T>>;
type TickActionOf<T> =
	TickAction<<T as frame_system::Config>::AccountId, BalanceOf<T>, RelayBlockNumberOf<T>>;

/// Weight functions needed by the market pallet.
pub trait WeightInfo {
	fn place_order() -> Weight;
	fn adjust_bid() -> Weight;
	fn place_renewal_order_market() -> Weight;
	fn place_renewal_order_renewal() -> Weight;
	fn place_renewal_order_displacement() -> Weight;
	fn settle_auction() -> Weight;
	fn finalize_sale() -> Weight;
	fn rotate_sale() -> Weight;
}

impl WeightInfo for () {
	fn place_order() -> Weight {
		Weight::zero()
	}
	fn adjust_bid() -> Weight {
		Weight::zero()
	}
	fn place_renewal_order_market() -> Weight {
		Weight::zero()
	}
	fn place_renewal_order_renewal() -> Weight {
		Weight::zero()
	}
	fn place_renewal_order_displacement() -> Weight {
		Weight::zero()
	}
	fn settle_auction() -> Weight {
		Weight::zero()
	}
	fn finalize_sale() -> Weight {
		Weight::zero()
	}
	fn rotate_sale() -> Weight {
		Weight::zero()
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Balance type used for bid amounts and prices.
		type Balance: BalanceT + FixedPointOperand;

		/// Relay chain block number type.
		type RelayBlockNumber: Parameter
			+ MaxEncodedLen
			+ AtLeast32BitUnsigned
			+ FixedPointOperand
			+ Copy;

		/// Weight information for market operations.
		type WeightInfo: WeightInfo;

		/// Provides information about the range of cores available for sale.
		type CoreRangeProvider: CoreRangeProvider;

		/// Provides timeslice scheduling information.
		type TimesliceProvider: TimesliceProvider;

		/// Provider of renewal rights information from the broker pallet.
		type RenewalRights: RenewalRightsProvider<Self::AccountId>;

		/// The number of relay chain blocks in a timeslice.
		#[pallet::constant]
		type TimeslicePeriod: Get<Self::RelayBlockNumber>;

		/// Maximum number of bids that can be placed in a single sale.
		#[pallet::constant]
		type MaxBids: Get<u32>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new bid was placed during the Market phase.
		BidPlaced {
			who: T::AccountId,
			bid_id: u32,
			amount: BalanceOf<T>,
		},
		/// An existing bid was raised to a higher price.
		BidRaised {
			who: T::AccountId,
			bid_id: u32,
			new_price: BalanceOf<T>,
			additional: BalanceOf<T>,
		},
		/// The Market phase auction has been settled with a clearing price.
		AuctionSettled {
			clearing_price: BalanceOf<T>,
			winners: u32,
		},
		/// Regions have been issued to auction winners at the end of the Renewal phase.
		SaleFinalized {
			regions_issued: u32,
		},
		/// A renewal right was exercised during the Renewal phase.
		RenewalExercised {
			who: T::AccountId,
			price: BalanceOf<T>,
			region_id: RegionId,
		},
		/// An auction winner was displaced by a renewer during the Renewal phase.
		BidDisplaced {
			who: T::AccountId,
			bid_id: u32,
			refund: BalanceOf<T>,
		},
		/// The sale phase has changed.
		PhaseTransitioned {
			from: SalePhase,
			to: SalePhase,
		},
		/// A new sale has been initialized.
		SaleInitialized {
			sale_start: RelayBlockNumberOf<T>,
			market_period: RelayBlockNumberOf<T>,
			start_price: BalanceOf<T>,
			reserve_price: BalanceOf<T>,
			region_begin: Timeslice,
			region_end: Timeslice,
			ideal_cores_sold: CoreIndex,
			cores_offered: CoreIndex,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// No active sales.
		NoSales,
		/// Bid price exceeds current price.
		Overpriced,
		/// Bid does not exist or does not belong to caller.
		BidNotExist,
		/// Market configuration or state not initialized.
		Uninitialized,
		/// Sales have not started yet.
		TooEarly,
		/// No cores available for renewal.
		Unavailable,
		/// Maximum number of bids exceeded.
		TooManyBids,
		/// Operation not allowed in the current sale phase.
		WrongPhase,
		/// Bid price is above the current descending price.
		BidTooHigh,
		/// Invalid configuration.
		InvalidConfig,
		/// Operation not allowed (e.g., bid withdrawal in RFC-17).
		NotAllowed,
	}

	/// The market configuration.
	#[pallet::storage]
	pub type Configuration<T> = StorageValue<_, ConfigRecordOf<T>, OptionQuery>;

	/// Information about the current sale.
	#[pallet::storage]
	pub type SaleInfo<T> = StorageValue<_, SaleInfoRecordOf<T>, OptionQuery>;

	/// The current phase of the sale cycle. `None` before sales are started.
	#[pallet::storage]
	pub type CurrentPhase<T> = StorageValue<_, SalePhase, OptionQuery>;

	/// Active bids during the Market phase. Keyed by bid ID.
	#[pallet::storage]
	pub type Bids<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		u32,
		BidRecord<T::AccountId, BalanceOf<T>>,
		OptionQuery,
	>;

	/// The next bid ID to assign. Also serves as the count of bids placed in this sale.
	#[pallet::storage]
	pub type NextBidId<T> = StorageValue<_, u32, ValueQuery>;

	/// Auction winners after settlement, awaiting region issuance at the end of Renewal phase.
	#[pallet::storage]
	pub type Allocations<T: Config> = StorageValue<
		_,
		BoundedVec<AllocationRecord<T::AccountId, BalanceOf<T>>, T::MaxBids>,
		ValueQuery,
	>;

	/// The clearing price from the most recent auction settlement.
	#[pallet::storage]
	pub type AuctionClearingPrice<T: Config> = StorageValue<_, BalanceOf<T>, OptionQuery>;

	/// Number of renewals exercised in the current Renewal phase.
	#[pallet::storage]
	pub type RenewalCount<T> = StorageValue<_, u32, ValueQuery>;

	/// Number of cores won per account in the auction. Set during settlement, used to reduce
	/// renewal quota (RFC-17: auction wins count against renewal rights).
	#[pallet::storage]
	pub type AuctionWins<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

	/// Number of renewal rights consumed per account in the current Renewal phase.
	#[pallet::storage]
	pub type RenewalsUsed<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

	/// Completed renewals during the Renewal phase. Drained in `finalize_sale` to emit
	/// `TickAction::RenewRegion`.
	#[pallet::storage]
	pub type CompletedRenewals<T: Config> = StorageValue<
		_,
		BoundedVec<(T::AccountId, PotentialRenewalId), T::MaxBids>,
		ValueQuery,
	>;

	/// Displaced auction winners during the Renewal phase. Refunded in `finalize_sale`.
	#[pallet::storage]
	pub type DisplacedBids<T: Config> = StorageValue<
		_,
		BoundedVec<(T::AccountId, BalanceOf<T>), T::MaxBids>,
		ValueQuery,
	>;
}

impl<T: Config> Pallet<T> {
	/// Get the current price at a given block number.
	pub fn current_price(block_number: RelayBlockNumberOf<T>) -> Option<BalanceOf<T>> {
		let sale = SaleInfo::<T>::get()?;
		match CurrentPhase::<T>::get()? {
			SalePhase::Market => Some(descending_price::<T>(block_number, &sale)),
			SalePhase::Renewal | SalePhase::Settlement => AuctionClearingPrice::<T>::get(),
		}
	}
}

impl<T: Config> Market<RelayBlockNumberOf<T>, BalanceOf<T>, T::AccountId> for Pallet<T> {
	type Error = Error<T>;
	type BidId = u32;
	type InitData = InitData<BalanceOf<T>>;
	type Configuration = ConfigRecordOf<T>;
	type CoreRangeProvider = T::CoreRangeProvider;
	type TimesliceProvider = T::TimesliceProvider;

	fn configure(configuration: Self::Configuration) -> Result<(), Self::Error> {
		configuration.validate().map_err(|_| Error::<T>::InvalidConfig)?;
		Configuration::<T>::put(configuration);
		Ok(())
	}

	fn start_sales(
		block_number: RelayBlockNumberOf<T>,
		init_data: Self::InitData,
	) -> Result<SalesStarted<RelayBlockNumberOf<T>>, Self::Error> {
		let config = Configuration::<T>::get().ok_or(Error::<T>::Uninitialized)?;
		let range =
			Self::CoreRangeProvider::core_range().ok_or(Error::<T>::Uninitialized)?;

		let reserve_price = init_data.reserve_price;
		let commit_timeslice = latest_timeslice_ready_to_commit::<T>(block_number, &config);

		// Bootstrap with an imaginary previous sale.
		let old_sale = SaleInfoRecord {
			sale_start: block_number,
			opening_price: reserve_price,
			reserve_price,
			clearing_price: None,
			region_begin: commit_timeslice,
			region_end: commit_timeslice.saturating_add(config.region_length),
			first_core: 0,
			ideal_cores_sold: 0,
			cores_offered: 0,
			cores_sold: 0,
		};

		let new_sale = rotate_sale::<T>(&old_sale, &config, &range, block_number);

		SaleInfo::<T>::put(&new_sale);
		CurrentPhase::<T>::put(SalePhase::Market);

		Self::deposit_event(Event::SaleInitialized {
			sale_start: new_sale.sale_start,
			market_period: config.market_period,
			start_price: new_sale.opening_price,
			reserve_price: new_sale.reserve_price,
			region_begin: new_sale.region_begin,
			region_end: new_sale.region_end,
			ideal_cores_sold: new_sale.ideal_cores_sold,
			cores_offered: new_sale.cores_offered,
		});

		Ok(SalesStarted { sale: new_sale.to_market_sale_info() })
	}

	fn place_order(
		block_number: RelayBlockNumberOf<T>,
		who: &T::AccountId,
		price_limit: BalanceOf<T>,
	) -> Result<OrderResult<BalanceOf<T>, Self::BidId>, Self::Error> {
		ensure!(CurrentPhase::<T>::get() == Some(SalePhase::Market), Error::<T>::WrongPhase);
		let sale = SaleInfo::<T>::get().ok_or(Error::<T>::NoSales)?;
		ensure!(block_number >= sale.sale_start, Error::<T>::TooEarly);

		let bid_count = NextBidId::<T>::get();
		ensure!(bid_count < T::MaxBids::get(), Error::<T>::TooManyBids);

		let current_price = descending_price::<T>(block_number, &sale);
		let bid_price = price_limit.min(current_price);

		let bid_id = bid_count;
		NextBidId::<T>::put(bid_id.saturating_add(1));

		Bids::<T>::insert(bid_id, BidRecord { who: who.clone(), price: bid_price });

		Self::deposit_event(Event::BidPlaced {
			who: who.clone(),
			bid_id,
			amount: bid_price,
		});

		Ok(OrderResult::BidPlaced { id: bid_id, bid_price })
	}

	fn place_renewal_order(
		_block_number: RelayBlockNumberOf<T>,
		who: &T::AccountId,
		renewal: PotentialRenewalId,
	) -> Result<RenewalOrderResult<BalanceOf<T>, Self::BidId>, Self::Error> {
		let config = Configuration::<T>::get().ok_or(Error::<T>::Uninitialized)?;
		let sale = SaleInfo::<T>::get().ok_or(Error::<T>::NoSales)?;

		ensure!(
			CurrentPhase::<T>::get() == Some(SalePhase::Renewal),
			Error::<T>::WrongPhase
		);

		// RFC-17: auction wins count against renewal quota.
		// remaining = total_rights - auction_wins - renewals_already_used
		let total_rights = T::RenewalRights::renewal_rights_count(who, renewal.when);
		let auction_wins = AuctionWins::<T>::get(who);
		let renewals_used = RenewalsUsed::<T>::get(who);
		let remaining = total_rights
			.saturating_sub(auction_wins)
			.saturating_sub(renewals_used);
		ensure!(remaining > 0, Error::<T>::Unavailable);

		let clearing = AuctionClearingPrice::<T>::get().unwrap_or(sale.reserve_price);

		let allocations = Allocations::<T>::get();
		// Use cores_sold from settlement (not current Allocations.len(), which shrinks
		// after displacement) to determine if the auction was oversubscribed.
		let oversubscribed = sale.cores_sold >= sale.cores_offered;

		let penalty =
			if oversubscribed { config.penalty * clearing } else { Zero::zero() };
		let renewal_price = clearing.saturating_add(penalty);

		let allocated_count =
			allocations.len() as u16 + RenewalCount::<T>::get() as u16;

		if allocated_count < sale.cores_offered {
			// Unallocated core available: direct renewal.
			let core = sale.first_core.saturating_add(allocated_count);
			let region_id = RegionId {
				begin: sale.region_begin,
				core,
				mask: CoreMask::complete(),
			};
			RenewalCount::<T>::mutate(|c| c.saturating_inc());
			RenewalsUsed::<T>::mutate(who, |c| c.saturating_inc());
			CompletedRenewals::<T>::mutate(|r| {
				let _ = r.try_push((who.clone(), renewal));
			});

			Self::deposit_event(Event::RenewalExercised {
				who: who.clone(),
				price: renewal_price,
				region_id,
			});

			Ok(RenewalOrderResult::Renewed {
				price: renewal_price,
				region_id,
				effective_to: sale.region_end,
			})
		} else if oversubscribed {
			// All cores allocated: displace lowest non-renewer auction winner.
			let mut allocs = allocations;

			// Only displace winners who are NOT existing tenants.
			let displace_idx = allocs
				.iter()
				.enumerate()
				.filter(|(_, a)| !a.is_existing_tenant)
				.min_by_key(|(_, a)| a.bid_price)
				.map(|(i, _)| i);

			if let Some(idx) = displace_idx {
				let displaced_alloc = allocs.remove(idx);

				let region_id = RegionId {
					begin: sale.region_begin,
					core: displaced_alloc.core,
					mask: CoreMask::complete(),
				};

				let refund = sale.clearing_price.unwrap_or_default();

				Self::deposit_event(Event::BidDisplaced {
					who: displaced_alloc.who.clone(),
					bid_id: displaced_alloc.bid_id,
					refund,
				});

				// Track displaced bid; refunded in finalize_sale.
				DisplacedBids::<T>::mutate(|bids| {
					let _ = bids.try_push((displaced_alloc.who, refund));
				});

				Allocations::<T>::put(allocs);
				RenewalCount::<T>::mutate(|c| c.saturating_inc());
				RenewalsUsed::<T>::mutate(who, |c| c.saturating_inc());
				CompletedRenewals::<T>::mutate(|r| {
					let _ = r.try_push((who.clone(), renewal));
				});

				Self::deposit_event(Event::RenewalExercised {
					who: who.clone(),
					price: renewal_price,
					region_id,
				});

				Ok(RenewalOrderResult::Renewed {
					price: renewal_price,
					region_id,
					effective_to: sale.region_end,
				})
			} else {
				Err(Error::<T>::Unavailable)
			}
		} else {
			Err(Error::<T>::Unavailable)
		}
	}

	fn adjust_bid(
		block_number: RelayBlockNumberOf<T>,
		id: Self::BidId,
		who: &T::AccountId,
		new_price: Option<BalanceOf<T>>,
	) -> Result<AdjustBidResult<BalanceOf<T>>, Self::Error> {
		// RFC-17: bids are binding and cannot be cancelled.
		let new_price = new_price.ok_or(Error::<T>::NotAllowed)?;

		ensure!(CurrentPhase::<T>::get() == Some(SalePhase::Market), Error::<T>::WrongPhase);
		let sale = SaleInfo::<T>::get().ok_or(Error::<T>::NoSales)?;

		let mut bid = Bids::<T>::get(id).ok_or(Error::<T>::BidNotExist)?;
		ensure!(&bid.who == who, Error::<T>::BidNotExist);
		ensure!(new_price > bid.price, Error::<T>::Overpriced);

		let current_price = descending_price::<T>(block_number, &sale);
		ensure!(new_price <= current_price, Error::<T>::BidTooHigh);

		let additional = new_price.saturating_sub(bid.price);
		bid.price = new_price;
		Bids::<T>::insert(id, bid);

		Self::deposit_event(Event::BidRaised {
			who: who.clone(),
			bid_id: id,
			new_price,
			additional,
		});

		Ok(AdjustBidResult::Lock { amount: additional })
	}

	fn tick(
		block_number: RelayBlockNumberOf<T>,
		weight_meter: &mut WeightMeter,
	) -> Vec<TickActionOf<T>> {
		let mut actions: Vec<TickActionOf<T>> = vec![];

		let Some(config) = Configuration::<T>::get() else {
			return actions;
		};
		let Some(sale) = SaleInfo::<T>::get() else {
			return actions;
		};
		let Some(phase) = CurrentPhase::<T>::get() else {
			return actions;
		};

		match phase {
			SalePhase::Market => {
				let market_end = sale.sale_start.saturating_add(config.market_period);

				if block_number >= market_end {
					if !weight_meter.can_consume(T::WeightInfo::settle_auction()) {
						return actions;
					}
					weight_meter.consume(T::WeightInfo::settle_auction());

					let mut settlement_actions = settle_auction::<T>(&sale);
					CurrentPhase::<T>::put(SalePhase::Renewal);

					Self::deposit_event(Event::PhaseTransitioned {
						from: SalePhase::Market,
						to: SalePhase::Renewal,
					});

					settlement_actions.push(TickAction::ProcessAutoRenewals {
						after_timeslice: sale.region_begin,
						next_renewal_at: sale.region_end,
					});
					actions.append(&mut settlement_actions);
					return actions;
				}
			},
			SalePhase::Renewal => {
				let market_end = sale.sale_start.saturating_add(config.market_period);
				let renewal_end = market_end.saturating_add(config.renewal_period);

				if block_number >= renewal_end {
					if !weight_meter.can_consume(T::WeightInfo::finalize_sale()) {
						return actions;
					}
					weight_meter.consume(T::WeightInfo::finalize_sale());

					let mut finalize_actions = finalize_sale::<T>(&sale);
					CurrentPhase::<T>::put(SalePhase::Settlement);
					RenewalCount::<T>::kill();
					let _ = RenewalsUsed::<T>::clear(u32::MAX, None);
					let _ = AuctionWins::<T>::clear(u32::MAX, None);

					Self::deposit_event(Event::PhaseTransitioned {
						from: SalePhase::Renewal,
						to: SalePhase::Settlement,
					});

					actions.append(&mut finalize_actions);
					return actions;
				}
			},
			SalePhase::Settlement => {
				let ready = Self::TimesliceProvider::latest_timeslice_ready_to_commit();
				if ready.map_or(false, |ts| ts >= sale.region_begin) {
					if !weight_meter.can_consume(T::WeightInfo::rotate_sale()) {
						return actions;
					}
					weight_meter.consume(T::WeightInfo::rotate_sale());

					let Some(range) = Self::CoreRangeProvider::core_range() else {
						return actions;
					};

					let new_sale =
						rotate_sale::<T>(&sale, &config, &range, block_number);

					SaleInfo::<T>::put(&new_sale);
					CurrentPhase::<T>::put(SalePhase::Market);

					// Clean up state from previous sale.
					NextBidId::<T>::kill();
					AuctionClearingPrice::<T>::kill();

					Self::deposit_event(Event::PhaseTransitioned {
						from: SalePhase::Settlement,
						to: SalePhase::Market,
					});
					Self::deposit_event(Event::SaleInitialized {
						sale_start: new_sale.sale_start,
						market_period: config.market_period,
						start_price: new_sale.opening_price,
						reserve_price: new_sale.reserve_price,
						region_begin: new_sale.region_begin,
						region_end: new_sale.region_end,
						ideal_cores_sold: new_sale.ideal_cores_sold,
						cores_offered: new_sale.cores_offered,
					});
					actions.push(TickAction::SaleRotated {
						old_sale: sale.to_market_sale_info(),
						new_sale: new_sale.to_market_sale_info(),
					});
					return actions;
				}
			},
		}

		actions
	}
}

// ---------------------------------------------------------------------------
// Internal functions
// ---------------------------------------------------------------------------

/// Compute the descending price at the given block during the Market phase.
fn descending_price<T: Config>(
	now: RelayBlockNumberOf<T>,
	sale: &SaleInfoRecordOf<T>,
) -> BalanceOf<T> {
	let config = Configuration::<T>::get();
	let market_period = config.map(|c| c.market_period).unwrap_or_else(|| now);

	let elapsed = now.saturating_sub(sale.sale_start).min(market_period);
	if market_period.is_zero() {
		return sale.reserve_price;
	}

	let price_range = sale.opening_price.saturating_sub(sale.reserve_price);
	let elapsed_u128: u128 = elapsed.saturated_into();
	let period_u128: u128 = market_period.saturated_into();
	let descent =
		FixedU64::from_rational(elapsed_u128, period_u128).saturating_mul_int(price_range);

	sale.opening_price.saturating_sub(descent)
}

/// Fisher-Yates shuffle of the sub-slice of bids that tie at the clearing price.
fn shuffle_marginal_bids<T: Config>(
	bids: &mut [(u32, BidRecord<T::AccountId, BalanceOf<T>>)],
	clearing_price: BalanceOf<T>,
) {
	let start = bids.partition_point(|b| b.1.price > clearing_price);
	let end = bids.partition_point(|b| b.1.price >= clearing_price);

	if end.saturating_sub(start) <= 1 {
		return;
	}

	let slice = &mut bids[start..end];
	let n = slice.len();

	let seed = frame_system::Pallet::<T>::parent_hash();
	let seed_bytes: &[u8] = seed.as_ref();

	let hash_len = seed_bytes.len().saturating_sub(3);
	if hash_len == 0 {
		return;
	}
	for i in (1..n).rev() {
		let offset = ((i - 1) * 4) % hash_len;
		let rand_val = u32::from_le_bytes(
			seed_bytes[offset..offset + 4]
				.try_into()
				.expect("offset + 4 is within bounds; qed"),
		);
		let j = (rand_val as usize) % (i + 1);
		slice.swap(i, j);
	}
}

/// Settle the auction at the end of the Market phase.
fn settle_auction<T: Config>(sale: &SaleInfoRecordOf<T>) -> Vec<TickActionOf<T>> {
	let mut actions = vec![];

	let mut all_bids: Vec<(u32, BidRecord<T::AccountId, BalanceOf<T>>)> = Vec::new();
	for (id, bid) in Bids::<T>::iter() {
		all_bids.push((id, bid));
	}
	all_bids.sort_by(|a, b| b.1.price.cmp(&a.1.price));

	let k = sale.cores_offered as usize;
	let reserve = sale.reserve_price;

	let clearing_price = if all_bids.len() >= k && k > 0 {
		all_bids[k - 1].1.price.max(reserve)
	} else {
		reserve
	};

	shuffle_marginal_bids::<T>(&mut all_bids, clearing_price);

	AuctionClearingPrice::<T>::put(clearing_price);

	let mut allocations: Vec<AllocationRecord<T::AccountId, BalanceOf<T>>> = Vec::new();
	let mut winner_count = 0u32;

	for (i, (bid_id, bid)) in all_bids.into_iter().enumerate() {
		Bids::<T>::remove(bid_id);

		if i < k && bid.price >= clearing_price {
			let excess = bid.price.saturating_sub(clearing_price);
			if !excess.is_zero() {
				actions.push(TickAction::Refund { amount: excess, who: bid.who.clone() });
			}

			let core = sale.first_core.saturating_add(i as u16);
			let is_existing_tenant =
				T::RenewalRights::renewal_rights_count(&bid.who, sale.region_begin) > 0;

			AuctionWins::<T>::mutate(&bid.who, |c| c.saturating_inc());

			allocations.push(AllocationRecord {
				who: bid.who,
				bid_price: bid.price,
				bid_id,
				core,
				is_existing_tenant,
			});

			winner_count += 1;
		} else {
			actions.push(TickAction::Refund { amount: bid.price, who: bid.who });
		}
	}

	let bounded: BoundedVec<_, T::MaxBids> = BoundedVec::truncate_from(allocations);
	Allocations::<T>::put(bounded);

	let mut updated_sale = sale.clone();
	updated_sale.cores_sold = winner_count as u16;
	updated_sale.clearing_price = Some(clearing_price);
	SaleInfo::<T>::put(updated_sale);

	Pallet::<T>::deposit_event(Event::AuctionSettled { clearing_price, winners: winner_count });

	actions
}

/// Finalize the sale at the end of the Renewal phase.
///
/// Issues regions for auction winners, refunds displaced bids, and updates
/// cores_sold to include renewals for the next sale's price adjustment.
fn finalize_sale<T: Config>(sale: &SaleInfoRecordOf<T>) -> Vec<TickActionOf<T>> {
	let mut actions = vec![];
	let allocations = Allocations::<T>::take();
	let count = allocations.len() as u32;
	let clearing_price = sale.clearing_price.unwrap_or(sale.reserve_price);

	for alloc in allocations.into_iter() {
		let region_id = RegionId {
			begin: sale.region_begin,
			core: alloc.core,
			mask: CoreMask::complete(),
		};

		actions.push(TickAction::SellRegion {
			owner: alloc.who,
			paid: clearing_price,
			region_id,
			region_end: sale.region_end,
		});
	}

	// Emit RenewRegion for each completed renewal.
	for (owner, renewal_id) in CompletedRenewals::<T>::take() {
		actions.push(TickAction::RenewRegion { owner, renewal_id });
	}

	// Refund displaced auction winners.
	for (who, amount) in DisplacedBids::<T>::take() {
		actions.push(TickAction::Refund { amount, who });
	}

	let renewal_count = RenewalCount::<T>::get() as u16;
	if renewal_count > 0 {
		let mut updated_sale = sale.clone();
		updated_sale.cores_sold = updated_sale.cores_sold.saturating_add(renewal_count);
		SaleInfo::<T>::put(updated_sale);
	}

	Pallet::<T>::deposit_event(Event::SaleFinalized { regions_issued: count });

	actions
}

fn latest_timeslice_ready_to_commit<T: Config>(
	now: RelayBlockNumberOf<T>,
	config: &ConfigRecordOf<T>,
) -> Timeslice {
	let advanced = now.saturating_add(config.advance_notice);
	let timeslice_period = T::TimeslicePeriod::get();
	(advanced / timeslice_period).saturated_into()
}

/// Compute the new reserve price per RFC-17's exponential adjustment.
fn adjust_reserve_price<T: Config>(
	old_sale: &SaleInfoRecordOf<T>,
	config: &ConfigRecordOf<T>,
) -> BalanceOf<T> {
	let cores_offered = old_sale.cores_offered;
	if cores_offered == 0 {
		return old_sale.reserve_price;
	}

	let consumption_rate =
		Perbill::from_rational(old_sale.cores_sold as u32, cores_offered as u32);
	let target = config.target_consumption_rate;

	let k = FixedU64::from_rational(config.sensitivity_millis as u128, 1000);

	let (deviation, positive) = if consumption_rate >= target {
		(consumption_rate - target, true)
	} else {
		(target - consumption_rate, false)
	};

	let dev = FixedU64::from_rational(deviation.deconstruct() as u128, 1_000_000_000);
	let exponent = k.saturating_mul(dev);

	let x = exponent;
	let mut sum = FixedU64::from(1);
	let mut term = FixedU64::from(1);
	for n in 1..30u64 {
		term = term.saturating_mul(x) / FixedU64::saturating_from_integer(n);
		if term.into_inner() == 0 {
			break;
		}
		sum = sum.saturating_add(term);
	}
	let exp_approx = if positive {
		sum
	} else {
		FixedU64::saturating_from_rational(FixedU64::from(1).into_inner(), sum.into_inner())
	};

	let mut price_candidate = exp_approx.saturating_mul_int(old_sale.reserve_price);

	if price_candidate < config.min_reserve_price {
		price_candidate = config.min_reserve_price;
	}

	if consumption_rate == Perbill::one() {
		let increase = price_candidate.saturating_sub(old_sale.reserve_price);
		if increase < config.min_increment {
			price_candidate = old_sale.reserve_price.saturating_add(config.min_increment);
		}
	}

	price_candidate
}

/// Rotate to a new sale based on the previous sale's performance.
fn rotate_sale<T: Config>(
	old_sale: &SaleInfoRecordOf<T>,
	config: &ConfigRecordOf<T>,
	range: &pallet_broker::market::SoldCoresRange,
	now: RelayBlockNumberOf<T>,
) -> SaleInfoRecordOf<T> {
	let new_reserve = adjust_reserve_price::<T>(old_sale, config);

	let max_possible_sales = range.to.saturating_sub(range.from);
	let limit_cores_offered = config.limit_cores_offered.unwrap_or(CoreIndex::max_value());
	let cores_offered = limit_cores_offered.min(max_possible_sales);
	let ideal_cores_sold = (config.ideal_bulk_proportion * cores_offered as u32) as u16;

	let region_begin = old_sale.region_end;
	let region_end = region_begin + config.region_length;

	let opening_price = new_reserve
		.saturating_mul(config.price_multiplier.into())
		.max(config.min_opening_price);

	SaleInfoRecord {
		sale_start: now,
		opening_price,
		reserve_price: new_reserve,
		clearing_price: None,
		region_begin,
		region_end,
		first_core: range.from,
		ideal_cores_sold,
		cores_offered,
		cores_sold: 0,
	}
}
