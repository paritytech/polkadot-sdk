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
//! 1. **Market Phase**: A descending clock auction where bidders place bids at or below the current
//!    descending price. Bids are binding and can only be raised, not cancelled.
//!
//! 2. **Renewal Phase**: Existing tenants with renewal rights can exercise them. If all cores are
//!    allocated from the auction, renewers may displace the lowest non-renewer auction winner. A
//!    penalty applies to renewers who did not participate in the auction when the market was
//!    oversubscribed.
//!
//! 3. **Settlement Phase**: No primary sales occur. The pallet waits until the next sale's region
//!    begins before rotating into a new market cycle. Regions are issued at the transition from
//!    Renewal to Settlement.
//!
//! ## Design
//!
//! This pallet implements the [`Market`] trait from `fp-coretime`, allowing it to be used
//! by the broker pallet without direct coupling. The broker calls into the market trait for
//! order placement and processes [`TickAction`]s returned by [`Market::tick`] to perform
//! fund transfers, region issuance, and sale rotation.
//!
//! Key design decisions:
//! - **Clearing-price auction**: All winners pay the same uniform price (the Kth highest bid).
//! - **Lock-then-charge**: Funds are locked at bid time by the broker. At settlement, excess is
//!   refunded via [`TickAction::Refund`]. Winners are charged the clearing price.
//! - **Binding bids**: Bids cannot be cancelled, only raised.
//! - **Displacement protection**: Auction winners with renewal rights cannot be displaced.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
pub use types::*;

mod types;

mod weights;
use weights::*;

pub mod runtime_api;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

extern crate alloc;

use alloc::{vec, vec::Vec};
use fp_coretime::{
	market::{
		AdjustBidResult, CoreRangeProvider, Market, OrderResult, RenewalOrderResult, SalesStarted,
		SoldCoresRange, TickAction, TimesliceProvider,
	},
	CoreIndex, CoreMask, PotentialRenewalId, RegionId, Timeslice,
};
use frame_support::{
	ensure,
	traits::{tokens::Balance as BalanceT, Defensive, Get, Randomness},
	weights::WeightMeter,
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_arithmetic::{FixedPointNumber, Perbill};
use sp_runtime::{
	traits::{AtLeast32BitUnsigned, SaturatedConversion, Saturating, Zero},
	BoundedVec, FixedPointOperand, FixedU64,
};

#[frame_support::pallet]
pub mod pallet {
	use crate::weights::WeightInfo;

	use super::*;
	use frame_support::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config:
		frame_system::Config<
		RuntimeEvent: From<Event<Self>>
		                  + IsType<<Self as frame_system::Config>::RuntimeEvent>
		                  + TryInto<Event<Self>>,
	>
	{
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

		/// Maximum number of bids that can be placed in a single sale.
		#[pallet::constant]
		type MaxBids: Get<BidId>;

		/// Source of randomness for shuffling marginal bids at settlement.
		type Randomness: Randomness<Self::Hash, BlockNumberFor<Self>>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new bid was placed during the Market phase.
		BidPlaced {
			/// Who have placed the bid.
			who: T::AccountId,
			/// Id of the bid that was placed.
			bid_id: BidId,
			/// Amount of the bid.
			amount: BalanceOf<T>,
		},
		/// An existing bid was raised to a higher price.
		BidRaised {
			/// A bidder who raised the bid.
			who: T::AccountId,
			/// Id of the bid that was raised.
			bid_id: BidId,
			/// A price of the bid before it was raised.
			old_price: BalanceOf<T>,
			/// A price of the bid after it was raised.
			new_price: BalanceOf<T>,
		},
		/// The Market phase auction has been settled with a clearing price.
		AuctionSettled {
			/// A clearing price that was determined when the market phase was settled.
			clearing_price: BalanceOf<T>,
			/// The number of auction winners.
			winners: u32,
		},
		/// Regions have been issued to auction winners at the end of the Renewal phase.
		SaleFinalized {
			/// How much regions were issued.
			regions_issued: u32,
		},
		/// A renewal right was exercised during the Renewal phase.
		RenewalExercised {
			/// The renewer who exercised his renewal right.
			who: T::AccountId,
			/// Price that was paid for the renewal.
			price: BalanceOf<T>,
			/// Id of the region that's being renewed.
			region_id: RegionId,
		},
		/// An auction winner was displaced by a renewer during the Renewal phase.
		BidDisplaced {
			/// Whose bid was displaced.
			who: T::AccountId,
			/// Id of the bid that was displaced.
			bid_id: BidId,
			/// Amount that will be returned to the bidder whose bid was displaced(the full amount
			/// of the bid).
			refund: BalanceOf<T>,
		},
		/// The sale phase has changed.
		PhaseTransitioned {
			/// A sale phase before the transition.
			from: SalePhase,
			/// A sale phase after the transition.
			to: SalePhase,
		},
		/// A new sale has been initialized.
		SaleInitialized {
			/// The relay block number at which the sale starts.
			sale_start: RelayBlockNumberOf<T>,
			/// The length in blocks of the Market (auction) phase.
			market_period: RelayBlockNumberOf<T>,
			/// The opening price of the descending Dutch auction.
			start_price: BalanceOf<T>,
			/// The floor price of the descending Dutch auction.
			reserve_price: BalanceOf<T>,
			/// The first timeslice of the Regions which are being sold in this sale.
			region_begin: Timeslice,
			/// The timeslice on which the Regions being sold in this sale expire.
			region_end: Timeslice,
			/// The number of cores we want to sell, ideally. Selling this amount would result in
			/// no change to the price for the next sale.
			ideal_cores_sold: CoreIndex,
			/// Number of cores offered for sale.
			cores_offered: CoreIndex,
		},
	}

	#[pallet::error]
	#[derive(PartialEq)]
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
		/// Bid price is below the reserve price.
		BidTooLow,
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

	/// Active bids during the Market phase, sorted by price descending.
	#[pallet::storage]
	pub type Bids<T: Config> =
		StorageValue<_, BoundedVec<BidRecord<T::AccountId, BalanceOf<T>>, T::MaxBids>, ValueQuery>;

	/// Auction winners after settlement. May be displaced by renewers during the Renewal phase.
	#[pallet::storage]
	pub type Allocations<T: Config> = StorageValue<
		_,
		BoundedVec<AllocationRecord<T::AccountId, BalanceOf<T>>, T::MaxBids>,
		ValueQuery,
	>;

	/// Per-account quota tracking: auction wins and renewals used this sale.
	#[pallet::storage]
	pub type Quotas<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, AccountQuota, ValueQuery>;

	/// Actions accumulated during the Renewal phase, resolved at sale finalization.
	#[pallet::storage]
	pub type PendingDisplacements<T: Config> = StorageValue<
		_,
		BoundedVec<BidDisplacement<T::AccountId, BalanceOf<T>>, T::MaxBids>,
		ValueQuery,
	>;
}

impl<T: Config> Pallet<T> {
	/// Set the current sale phase.
	pub fn set_phase(phase: SalePhase) {
		SaleInfo::<T>::mutate_extant(|sale| sale.phase = phase);
	}

	/// Get the current price at a given block number.
	pub fn current_price(block_number: RelayBlockNumberOf<T>) -> Option<BalanceOf<T>> {
		let sale = SaleInfo::<T>::get()?;
		match sale.phase {
			SalePhase::Market => descending_price::<T>(block_number, &sale).ok(),
			SalePhase::Renewal | SalePhase::Settlement => sale.clearing_price,
		}
	}
}

impl<T: Config> Market<RelayBlockNumberOf<T>, BalanceOf<T>, T::AccountId> for Pallet<T> {
	type Error = Error<T>;
	type BidId = BidId;
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
		let core_range = Self::CoreRangeProvider::core_range().ok_or(Error::<T>::Uninitialized)?;

		let reserve_price = init_data.reserve_price;
		let commit_timeslice = T::TimesliceProvider::latest_timeslice_ready_to_commit()
			.ok_or(Error::<T>::Uninitialized)?;

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
			renewal_count: 0,
			phase: SalePhase::Settlement, // Dummy — rotate_sale will create the real one.
		};

		let new_sale = rotate_sale::<T>(&old_sale, &config, &core_range, block_number);

		SaleInfo::<T>::put(&new_sale);

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
		let sale = SaleInfo::<T>::get().ok_or(Error::<T>::NoSales)?;
		ensure!(sale.phase == SalePhase::Market, Error::<T>::WrongPhase);
		ensure!(block_number >= sale.sale_start, Error::<T>::TooEarly);
		ensure!(price_limit >= sale.reserve_price, Error::<T>::BidTooLow);

		let current_price = descending_price::<T>(block_number, &sale)?;
		let bid_price = price_limit.min(current_price);

		let bid_id = Bids::<T>::try_mutate(|bids| {
			let bid_id = bids.len() as BidId;
			let record = BidRecord { bid_id, who: who.clone(), price: bid_price };
			let pos = bids.partition_point(|b| b.price > bid_price);
			bids.try_insert(pos, record).map_err(|_| Error::<T>::TooManyBids)?;
			Ok::<_, Error<T>>(bid_id)
		})?;

		Self::deposit_event(Event::BidPlaced { who: who.clone(), bid_id, amount: bid_price });

		Ok(OrderResult::BidPlaced { id: bid_id, bid_price })
	}

	fn place_renewal_order(
		_block_number: RelayBlockNumberOf<T>,
		who: &T::AccountId,
		renewal: PotentialRenewalId,
	) -> Result<RenewalOrderResult<BalanceOf<T>, Self::BidId>, Self::Error> {
		let config = Configuration::<T>::get().ok_or(Error::<T>::Uninitialized)?;
		let sale = SaleInfo::<T>::get().ok_or(Error::<T>::NoSales)?;
		ensure!(sale.phase == SalePhase::Renewal, Error::<T>::WrongPhase);
		ensure!(renewal.when == sale.region_begin, Error::<T>::Unavailable);

		// RFC-17: auction wins count against renewal quota.
		// remaining = total_rights - auction_wins - renewals_already_used
		let total_rights = T::RenewalRights::renewal_rights_count(who, sale.region_begin);
		let quota = Quotas::<T>::get(who);
		let remaining = total_rights
			.saturating_sub(quota.auction_wins)
			.saturating_sub(quota.renewals_used);
		ensure!(remaining > 0, Error::<T>::Unavailable);

		// TODO: Put `clearing_price` into `SalePhase::Renewal` to have the guarantee that it will
		// be present.
		let clearing = sale.clearing_price.defensive_unwrap_or(sale.reserve_price);

		let allocations = Allocations::<T>::get();
		// Use cores_sold from settlement (not Allocations len, which shrinks
		// after displacement) to determine if the auction was oversubscribed.
		let oversubscribed = sale.cores_sold == sale.cores_offered;

		let penalty = if oversubscribed { config.penalty * clearing } else { Zero::zero() };
		let renewal_price = clearing.saturating_add(penalty);

		let allocated_count = (allocations.len() as u16).saturating_add(sale.renewal_count as u16);

		let core = if allocated_count < sale.cores_offered {
			// Unallocated core available: direct renewal.
			sale.first_core.saturating_add(allocated_count)
		} else if oversubscribed {
			// All cores allocated: displace the lowest-priced winner whose renewal
			// rights are fully consumed by auction wins.
			let mut allocations = allocations;

			let displace_idx = allocations
				.iter()
				.enumerate()
				.filter(|(_, a)| {
					let rights = T::RenewalRights::renewal_rights_count(&a.who, sale.region_begin);
					let wins = Quotas::<T>::get(&a.who).auction_wins;
					rights < wins
				})
				.min_by_key(|(_, a)| a.bid_price)
				.map(|(i, _)| i)
				.ok_or(Error::<T>::Unavailable)?;

			let displaced = allocations.remove(displace_idx);
			let core = displaced.core;
			let refund = clearing;

			PendingDisplacements::<T>::try_mutate(|displacements| {
				displacements
					.try_push(BidDisplacement { who: displaced.who.clone(), refund })
					.map_err(|_| Error::<T>::TooManyBids)
			})?;

			Quotas::<T>::mutate(&displaced.who, |quota| quota.auction_wins.saturating_dec());
			Self::deposit_event(Event::BidDisplaced {
				who: displaced.who,
				bid_id: displaced.bid_id,
				refund,
			});
			Allocations::<T>::put(allocations);

			core
		} else {
			return Err(Error::<T>::Unavailable);
		};

		let region_id = RegionId { begin: sale.region_begin, core, mask: CoreMask::complete() };

		SaleInfo::<T>::mutate_extant(|sale| sale.renewal_count.saturating_inc());
		Quotas::<T>::mutate(who, |quota| quota.renewals_used.saturating_inc());

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
	}

	fn adjust_bid(
		block_number: RelayBlockNumberOf<T>,
		id: Self::BidId,
		who: &T::AccountId,
		new_price: Option<BalanceOf<T>>,
	) -> Result<AdjustBidResult<BalanceOf<T>>, Self::Error> {
		// RFC-17: bids are binding and cannot be cancelled.
		let new_price = new_price.ok_or(Error::<T>::NotAllowed)?;

		let sale = SaleInfo::<T>::get().ok_or(Error::<T>::NoSales)?;
		ensure!(sale.phase == SalePhase::Market, Error::<T>::WrongPhase);

		let mut bids = Bids::<T>::get();
		let idx = bids.iter().position(|b| b.bid_id == id).ok_or(Error::<T>::BidNotExist)?;
		ensure!(&bids[idx].who == who, Error::<T>::NotAllowed);

		let old_price = bids[idx].price;
		// RFC-17: bids cannot be lowered, only raised.
		ensure!(new_price > old_price, Error::<T>::Overpriced);

		let current_price = descending_price::<T>(block_number, &sale)?;
		// RFC-17: bid price cannot be higher than the current auction price.
		ensure!(new_price <= current_price, Error::<T>::BidTooHigh);

		let mut record = bids.remove(idx);
		record.price = new_price;
		let new_pos = bids.partition_point(|b| b.price > new_price);
		// Re-insert cannot fail — we just removed one element.
		bids.try_insert(new_pos, record)
			.expect("just removed one element; capacity cannot be exceeded; qed");
		Bids::<T>::put(bids);

		Self::deposit_event(Event::BidRaised {
			who: who.clone(),
			bid_id: id,
			old_price,
			new_price,
		});

		Ok(AdjustBidResult::Lock { amount: new_price.saturating_sub(old_price) })
	}

	fn tick(
		block_number: RelayBlockNumberOf<T>,
		weight_meter: &mut WeightMeter,
	) -> Vec<TickActionOf<T>> {
		weight_meter.consume(T::WeightInfo::tick_base());

		let mut actions: Vec<TickActionOf<T>> = vec![];

		let Some(config) = Configuration::<T>::get() else {
			return actions;
		};
		let Some(sale) = SaleInfo::<T>::get() else {
			return actions;
		};

		match sale.phase {
			SalePhase::Market => {
				let market_end = sale.sale_start.saturating_add(config.market_period);

				if block_number >= market_end {
					weight_meter.consume(
						T::WeightInfo::sale_phase_transition_to_renewal()
							.saturating_sub(T::WeightInfo::tick_base()),
					);

					let mut settlement_actions = settle_auction::<T>(&sale);
					Self::set_phase(SalePhase::Renewal);

					Self::deposit_event(Event::PhaseTransitioned {
						from: SalePhase::Market,
						to: SalePhase::Renewal,
					});

					actions.append(&mut settlement_actions);
					actions.push(TickAction::ProcessAutoRenewals {
						after_timeslice: sale.region_begin,
						next_renewal_at: sale.region_end,
					});
				}
			},
			SalePhase::Renewal => {
				let market_end = sale.sale_start.saturating_add(config.market_period);
				let renewal_end = market_end.saturating_add(config.renewal_period);

				if block_number >= renewal_end {
					weight_meter.consume(
						T::WeightInfo::sale_phase_transition_to_settlement()
							.saturating_sub(T::WeightInfo::tick_base()),
					);

					let mut finalize_actions = finalize_sale::<T>(&sale);
					Self::set_phase(SalePhase::Settlement);
					let _ = Quotas::<T>::clear(u32::MAX, None);

					Self::deposit_event(Event::PhaseTransitioned {
						from: SalePhase::Renewal,
						to: SalePhase::Settlement,
					});

					actions.append(&mut finalize_actions);
				}
			},
			SalePhase::Settlement => {
				let ready = Self::TimesliceProvider::latest_timeslice_ready_to_commit();
				if ready.map_or(false, |ts| ts >= sale.region_begin) {
					weight_meter.consume(
						T::WeightInfo::sale_phase_transition_to_market()
							.saturating_sub(T::WeightInfo::tick_base()),
					);

					let Some(range) = Self::CoreRangeProvider::core_range() else {
						return actions;
					};

					let new_sale = rotate_sale::<T>(&sale, &config, &range, block_number);

					SaleInfo::<T>::put(&new_sale);

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
) -> Result<BalanceOf<T>, Error<T>> {
	let config = Configuration::<T>::get().ok_or(Error::<T>::Uninitialized)?;
	let market_period = config.market_period;

	let elapsed = now.saturating_sub(sale.sale_start).min(market_period);
	if market_period.is_zero() {
		return Ok(sale.reserve_price);
	}

	let price_range = sale.opening_price.saturating_sub(sale.reserve_price);
	let elapsed_u128: u128 = elapsed.saturated_into();
	let period_u128: u128 = market_period.saturated_into();
	let descent =
		FixedU64::from_rational(elapsed_u128, period_u128).saturating_mul_int(price_range);

	Ok(sale.opening_price.saturating_sub(descent))
}

/// Fisher-Yates shuffle of the sub-slice of bids that tie at the clearing price.
fn shuffle_marginal_bids<T: Config>(
	bids: &mut [BidRecord<T::AccountId, BalanceOf<T>>],
	clearing_price: BalanceOf<T>,
) {
	let start = bids.partition_point(|b| b.price > clearing_price);
	let end = bids.partition_point(|b| b.price >= clearing_price);

	if end.saturating_sub(start) <= 1 {
		return;
	}

	let slice = &mut bids[start..end];
	let n = slice.len();

	let (seed, _) = T::Randomness::random(b"coretime-market/shuffle");
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
///
/// Bids are already sorted by price descending in storage. Determines the clearing price,
/// shuffles marginal bids for fair selection, then splits into winners and losers.
fn settle_auction<T: Config>(sale: &SaleInfoRecordOf<T>) -> Vec<TickActionOf<T>> {
	let mut bids: Vec<_> = Bids::<T>::take().into_inner();
	let k = sale.cores_offered as usize;
	let reserve = sale.reserve_price;

	// Clearing price: the Kth highest bid, floored at reserve. Falls back to reserve
	// when fewer than K bids are placed.
	let clearing_price = bids
		.get(k.saturating_sub(1))
		.filter(|_| k > 0)
		.map(|b| b.price.max(reserve))
		.unwrap_or(reserve);

	shuffle_marginal_bids::<T>(&mut bids, clearing_price);

	let (winners, losers): (Vec<_>, Vec<_>) = bids
		.into_iter()
		.enumerate()
		.partition(|(i, bid)| *i < k && bid.price >= clearing_price);

	let mut actions = Vec::with_capacity(winners.len() + losers.len());

	// Refund losers in full.
	for (_, bid) in &losers {
		actions.push(TickAction::Refund { amount: bid.price, who: bid.who.clone() });
	}

	// Process winners: refund excess, track auction wins, build allocations.
	let allocations: Vec<_> = winners
		.into_iter()
		.map(|(i, bid)| {
			let excess = bid.price.saturating_sub(clearing_price);
			if !excess.is_zero() {
				actions.push(TickAction::Refund { amount: excess, who: bid.who.clone() });
			}
			Quotas::<T>::mutate(&bid.who, |q| q.auction_wins.saturating_inc());
			AllocationRecord {
				who: bid.who,
				bid_price: bid.price,
				bid_id: bid.bid_id,
				core: sale.first_core.saturating_add(i as u16),
			}
		})
		.collect();

	let winner_count = allocations.len() as u32;
	Allocations::<T>::put(
		BoundedVec::try_from(allocations)
			.expect("Auction winner count cannot exceed bid count; qed"),
	);

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
/// cores_sold to the final number of occupied cores.
fn finalize_sale<T: Config>(sale: &SaleInfoRecordOf<T>) -> Vec<TickActionOf<T>> {
	let mut actions = vec![];
	let allocations = Allocations::<T>::take();
	let count = allocations.len() as u32;
	let clearing_price = sale.clearing_price.unwrap_or(sale.reserve_price);

	for alloc in allocations.into_iter() {
		let region_id =
			RegionId { begin: sale.region_begin, core: alloc.core, mask: CoreMask::complete() };

		actions.push(TickAction::SellRegion {
			owner: alloc.who,
			paid: clearing_price,
			region_id,
			region_end: sale.region_end,
		});
	}

	for displacement in PendingDisplacements::<T>::take() {
		actions.push(TickAction::Refund { who: displacement.who, amount: displacement.refund });
	}

	let remaining_auction_wins: CoreIndex = count.saturated_into();
	let renewals: CoreIndex = sale.renewal_count.saturated_into();
	let cores_sold = remaining_auction_wins.saturating_add(renewals);
	debug_assert!(cores_sold <= sale.cores_offered);
	SaleInfo::<T>::mutate_extant(|sale| {
		sale.cores_sold = cores_sold;
	});

	Pallet::<T>::deposit_event(Event::SaleFinalized { regions_issued: count });

	actions
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

	let consumption_rate = Perbill::from_rational(old_sale.cores_sold as u32, cores_offered as u32);
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
	range: &SoldCoresRange,
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
		renewal_count: 0,
		phase: SalePhase::Market,
	}
}
