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

use frame_support::weights::WeightMeter;
use sp_runtime::DispatchError;

use crate::{
	AdaptedPrices, BalanceOf, Config, CoreIndex, PotentialRenewalId, RegionId, RelayBlockNumberOf,
	SaleInfoRecordOf, Timeslice,
};

/// Trait representig generic market logic.
///
/// The assumptions for this generic market are:
/// - Every order will either create a bid or will be resolved immediately.
/// - There're two types of orders: bulk coretime purchase and bulk coretime renewal.
/// - Coretime regions are fungible.
pub trait Market<T: Config> {
	type Error: Into<DispatchError>;
	/// Unique ID assigned to every bid.
	type BidId;
	type CoreCount: CoreCountProvider<T>;
	type TimesliceProvider: TimesliceProvider;

	// TODO: Unify the interface.
	fn start_sales(
		block_number: RelayBlockNumberOf<T>,
		end_price: BalanceOf<T>,
	) -> Result<SalesStarted<T>, Self::Error>;

	/// Place an order for one bulk coretime region purchase.
	///
	/// This method may or may not create a bid, according to the market rules.
	///
	/// - `price_limit` - maximum price which the buyer is willing to pay
	fn place_order(
		block_number: RelayBlockNumberOf<T>,
		who: &T::AccountId,
		price_limit: BalanceOf<T>,
	) -> Result<OrderResult<T, Self::BidId>, Self::Error>;

	/// Place an order for bulk coretime renewal.
	///
	/// This method may or may not create a bid, according to the market rules.
	fn place_renewal_order(
		block_number: RelayBlockNumberOf<T>,
		who: &T::AccountId,
		renewal: PotentialRenewalId,
		recorded_price: BalanceOf<T>,
	) -> Result<RenewalOrderResult<T, Self::BidId>, Self::Error>;

	fn raise_bid(
		block_number: RelayBlockNumberOf<T>,
		id: Self::BidId,
		who: &T::AccountId,
		new_price: BalanceOf<T>,
	) -> Result<RaiseBidResult<T>, Self::Error>;

	/// Logic that gets called in `on_initialize` hook.
	fn tick(
		now: RelayBlockNumberOf<T>,
		weight_meter: &mut WeightMeter,
	) -> Vec<TickAction<T, Self::BidId>>;
}

pub trait CoreCountProvider<T: Config> {
	fn reserved_core_count() -> CoreIndex;

	fn core_count() -> Option<CoreIndex>;
}

pub trait TimesliceProvider {
	fn next_timeslice_to_commit() -> Option<Timeslice>;
}

pub enum OrderResult<T: Config, BidId> {
	BidPlaced { id: BidId, bid_price: BalanceOf<T> },
	Sold { price: BalanceOf<T>, region_id: RegionId, region_end: Timeslice },
}

pub enum RenewalOrderResult<T: Config, BidId> {
	BidPlaced {
		id: BidId,
		bid_price: BalanceOf<T>,
	},
	Sold {
		price: BalanceOf<T>,
		next_renewal_price: BalanceOf<T>,
		region_id: RegionId,
		effective_to: Timeslice,
	},
}

pub struct CloseBidResult<T: Config> {
	pub owner: T::AccountId,
	pub refund: BalanceOf<T>,
}

pub struct RaiseBidResult<T: Config> {
	pub payment_due: BalanceOf<T>,
}

pub enum TickAction<T: Config, BidId> {
	SellRegion {
		owner: T::AccountId,
		/// How much was paid for this region in total.
		paid: BalanceOf<T>,
		region_id: RegionId,
		region_end: Timeslice,
	},
	RenewRegion {
		owner: T::AccountId,
		renewal_id: PotentialRenewalId,
	},
	Refund {
		amount: BalanceOf<T>,
		who: T::AccountId,
	},
	BidClosed {
		id: BidId,
		owner: T::AccountId,
	},
	SaleRotated {
		old_sale: SaleInfoRecordOf<T>,
		new_sale: SaleInfoRecordOf<T>,
		new_prices: AdaptedPrices<BalanceOf<T>>,
		// TODO: Deprecate it as it doesn't fit into the general market impl but used when emitting
		// an event.
		start_price: BalanceOf<T>,
	},
}

pub struct SalesStarted<T: Config> {
	pub imaginary_old_sale: SaleInfoRecordOf<T>,
	pub new_sale: SaleInfoRecordOf<T>,
	pub new_prices: AdaptedPrices<BalanceOf<T>>,
	// TODO: Deprecate it as it doesn't fit into the general market impl but used when emitting
	// an event.
	pub start_price: BalanceOf<T>,
}
