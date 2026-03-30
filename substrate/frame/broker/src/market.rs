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

//! Generic coretime market interface.
//!
//! Contains [`Market`] trait - the abstraction that allows `pallet-broker` to use any market
//! logic that's implemented as [`Market`].

use frame_support::weights::WeightMeter;
use sp_runtime::DispatchError;

use crate::{
	AdaptedPrices, BalanceOf, Config, CoreIndex, PotentialRenewalId, RegionId, RelayBlockNumberOf,
	SaleInfoRecordOf, Timeslice,
};

/// Trait representig generic coretime market logic.
///
/// It introduces some assumptions about the market implementation:
/// - There're two types of orders: purchase and renewal.
/// - Every successful order will either create a bid or will be resolved immediately.
/// - Coretime regions are equivalent from the user's perspective.
/// - Once the bid is placed it can only be raised and never lowered or closed.
///
/// ## Assumed market lifecycle
/// 1. [`Market::start_sales`] - here the initialization of the market happens(if required).
/// 2. [`Market::place_order`], [`Market::place_renewal_order`] and [`Market::raise_bid`] - users
///    purchase the coretime reguons (or make bids, depends on the market implementation) and renew
///    them.
/// 3. [`Market::tick`] - gets called periodically at `on_initialize` and can be used to execute the
///    logic that needs to be executed at a specific time rather than as a response to some user
///    interaction.
pub trait Market<T: Config> {
	/// Error type that's returned by the market functions.
	type Error: Into<DispatchError>;
	/// Unique ID assigned to every bid.
	type BidId;
	/// Data that's used in [`Market::start_sales`] to initialize the market.
	type InitData;
	/// Type providing information about the core count to the market.
	type CoreCountProvider: CoreCountProvider<T>;
	/// Type providing information about the timeslice scheduling to the market.
	type TimesliceProvider: TimesliceProvider;

	/// Start the sales on coretime market.
	///
	/// - `block_number` - current relay chain block number
	/// - `init_data` - the data specific to the market implementation that's used to initialize it.
	fn start_sales(
		block_number: RelayBlockNumberOf<T>,
		init_data: Self::InitData,
	) -> Result<SalesStarted<T>, Self::Error>;

	/// Place an order for one coretime region purchase.
	///
	/// Depending on the specific market implementation this function will either place a bid or
	/// will indicate that coretime region can be sold immediately.
	///
	/// - `block_number` - current relay chain block number
	/// - `who` - who is placing the order
	/// - `price_limit` - maximum price which the buyer is willing to pay
	fn place_order(
		block_number: RelayBlockNumberOf<T>,
		who: &T::AccountId,
		price_limit: BalanceOf<T>,
	) -> Result<OrderResult<T, Self::BidId>, Self::Error>;

	/// Place an order for coretime region renewal.
	///
	/// Depending on the specific market implementation this function will either place a bid or
	/// will indicate that coretime region can be renewed immediately.
	///
	/// - `block_number` - current relay chain block number
	/// - `who` - who is placing the order
	/// - `renewal` - renewal id
	/// - `recorded_price` - a price for which the next renewal can be made.
	fn place_renewal_order(
		block_number: RelayBlockNumberOf<T>,
		who: &T::AccountId,
		renewal: PotentialRenewalId,
		recorded_price: BalanceOf<T>,
	) -> Result<RenewalOrderResult<T, Self::BidId>, Self::Error>;

	/// Increase the price for the bid.
	///
	/// - `block_number` - current relay chain block number
	/// - `id` - identifier of the bid that the user wants to raise
	/// - `who` - who is raising the bid
	/// - `new_price` - the price a bid should have afterwards
	fn raise_bid(
		block_number: RelayBlockNumberOf<T>,
		id: Self::BidId,
		who: &T::AccountId,
		new_price: BalanceOf<T>,
	) -> Result<RaiseBidResult<T>, Self::Error>;

	/// Logic that gets called in `on_initialize` hook.
	///
	/// - `now` - current relay chain block number
	/// - `weight_meter` - weight meter for a more precise weight accounting in implementation
	fn tick(now: RelayBlockNumberOf<T>, weight_meter: &mut WeightMeter) -> Vec<TickAction<T>>;
}

/// Type that provides information about reserved and total core count available for sale on
/// coretime market.
pub trait CoreCountProvider<T: Config> {
	/// Amount of cores that are reserved, usually for the system workloads.
	fn reserved_core_count() -> CoreIndex;
	/// The total available amount of cores, including reserved.
	///
	/// Returns `None` when the core count is unknown.
	fn core_count() -> Option<CoreIndex>;
}

/// Type that provides information about timeslices to the market implementation.
pub trait TimesliceProvider {
	/// Returns a timeslice pending to be commited, if any.
	fn next_timeslice_to_commit() -> Option<Timeslice>;
	/// Latest timeslice that's ready to be commited to the relay chain. If the `None` is returned
	/// then the requested timeslice is unknown, which may happen when `pallet-broker` is in
	/// uninitialized state.
	fn latest_timeslice_ready_to_commit() -> Option<Timeslice>;
}

/// Outcome of the [`Market::start_sales`].
pub struct SalesStarted<T: Config> {
	/// The sale that never actually was active but used for bootsrapping the first actual sale.
	pub imaginary_old_sale: SaleInfoRecordOf<T>,
	/// The first sale that will be active from now on.
	pub new_sale: SaleInfoRecordOf<T>,
	/// Prices that are actual for the `new_sale`.
	pub new_prices: AdaptedPrices<BalanceOf<T>>,
	/// Starting price for the auction. It's used only to emit event in `pallet-broker`, so
	/// valid only for pre-RFC-17 implementation.
	pub start_price: BalanceOf<T>,
}

/// Possible outcomes of [`Market::place_order`].
pub enum OrderResult<T: Config, BidId> {
	/// A bid for the coretime purchase was placed.
	BidPlaced {
		/// Id of the bid that was placed.
		id: BidId,
		/// Amount that needs to be locked when this bid is placed.
		bid_price: BalanceOf<T>,
	},
	/// Coretime region was sold immediately.
	Sold {
		/// Price that's paid for this region.
		price: BalanceOf<T>,
		/// A purchased region id.
		region_id: RegionId,
		/// When the purchased region ends.
		region_end: Timeslice,
	},
}

/// Possible outcomes of [`Market::place_renewal_order`].
pub enum RenewalOrderResult<T: Config, BidId> {
	/// A bid for the coretime renewal was placed.
	BidPlaced {
		/// Id of the bid that was placed.
		id: BidId,
		/// Amount that needs to be locked when this bid is placed.
		bid_price: BalanceOf<T>,
	},
	/// Coretime region was renewed immediately.
	Renewed {
		/// Price that's paid for the region renewal.
		price: BalanceOf<T>,
		/// Price for which this region can be renewed again in future.
		next_renewal_price: BalanceOf<T>,
		/// Id of the renewed region.
		region_id: RegionId,
		/// When the renewal ends.
		effective_to: Timeslice,
	},
}

/// Outcome of the [`Market::raise_bid`].
pub struct RaiseBidResult<T: Config> {
	/// How much the payer should additionally lock.
	pub payment_due: BalanceOf<T>,
}

/// Outcome of the [`Market::tick`].
///
/// When the `pallet-broker` calls [`Market::tick`] it will get a `Vec<TickAction>` and will execute
/// every action in the order they're placed in the vector.
///
/// All of these actons are outside of the responsibility of the market, so the market relies on the
/// `pallet-broker` to execute them. These actons include, for example, region manipulations and
/// balance transfers.
pub enum TickAction<T: Config> {
	/// Sell region to the specified account.
	SellRegion {
		/// A new owner of the region.
		owner: T::AccountId,
		/// How much was paid for this region in total.
		paid: BalanceOf<T>,
		/// Region that's being sold.
		region_id: RegionId,
		/// When the region ends.
		region_end: Timeslice,
	},
	/// Renew specified region.
	RenewRegion {
		/// Who owns the region that's being renewed.
		owner: T::AccountId,
		/// Renewal corresponding to the region being renewed.
		renewal_id: PotentialRenewalId,
	},
	/// Release balance held(usually by the bid) back to the user.
	Refund {
		/// Amound that needs to be returned to the user.
		amount: BalanceOf<T>,
		/// Who to return to.
		who: T::AccountId,
	},
	/// A new sale cycle have started. This action is required to notify `pallet-broker` about the
	/// sale boundary, so it can execute the logic that's required in this case.
	SaleRotated {
		/// A previously active sale.
		old_sale: SaleInfoRecordOf<T>,
		/// A sale that will be active starting from now on.
		new_sale: SaleInfoRecordOf<T>,
		/// Prices actual for the `new_sale`.
		new_prices: AdaptedPrices<BalanceOf<T>>,
		/// Starting price for the auction. It's used only to emit event in `pallet-broker`, so
		/// valid only for pre-RFC-17 implementation.
		start_price: BalanceOf<T>,
	},
}
