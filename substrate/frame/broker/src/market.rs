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

use core::cmp;
use frame_support::{ensure, weights::WeightMeter};
use frame_system::pallet_prelude::AccountIdFor;
use sp_arithmetic::FixedPointNumber;
use sp_runtime::{traits::Zero, DispatchError, FixedU64, SaturatedConversion, Saturating};

use crate::{
	utility_impls::{CoreCountProviderImpl, TimesliceProviderImpl},
	weights::WeightInfo,
	AdaptPrice, AdaptedPrices, BalanceOf, BidIdOf, Config, ConfigRecordOf, Configuration,
	CoreIndex, CoreMask, Pallet, PotentialRenewalId, RegionId, RelayBlockNumberOf, SaleInfo,
	SaleInfoRecord, SaleInfoRecordOf, SalePerformance, Timeslice,
};

// TODO: Extend the documentation.

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
	type InitData;

	fn start_sales(
		block_number: RelayBlockNumberOf<T>,
		init_data: Self::InitData,
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
	fn tick(now: RelayBlockNumberOf<T>, weight_meter: &mut WeightMeter) -> Vec<TickAction<T>>;
}

pub trait CoreCountProvider<T: Config> {
	fn reserved_core_count() -> CoreIndex;

	fn core_count() -> Option<CoreIndex>;
}

pub trait TimesliceProvider {
	fn next_timeslice_to_commit() -> Option<Timeslice>;

	fn latest_timeslice_ready_to_commit() -> Option<Timeslice>;
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

pub enum TickAction<T: Config> {
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
		id: BidIdOf<T>,
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

pub enum MarketError {
	NoSales,
	Overpriced,
	BidNotExist,
	Uninitialized,
	CoreCountUnknown,
	TooEarly,
	Unavailable,
	SoldOut,
}

impl From<MarketError> for DispatchError {
	fn from(value: MarketError) -> Self {
		match value {
			MarketError::NoSales => Self::Other("NoSales"),
			MarketError::Overpriced => Self::Other("Overpriced"),
			MarketError::BidNotExist => Self::Other("BidNotExist"),
			MarketError::Uninitialized => Self::Other("Uninitialized"),
			MarketError::CoreCountUnknown => Self::Other("CoreCountUnknown"),
			MarketError::TooEarly => Self::Other("TooEarly"),
			MarketError::Unavailable => Self::Other("Unavailable"),
			MarketError::SoldOut => Self::Other("SoldOut"),
		}
	}
}

impl<T: Config> Market<T> for Pallet<T> {
	type Error = MarketError;
	/// Must be unique.
	type BidId = ();
	type CoreCount = CoreCountProviderImpl<T>;
	type TimesliceProvider = TimesliceProviderImpl<T>;
	type InitData = BalanceOf<T>;

	fn start_sales(
		block_number: RelayBlockNumberOf<T>,
		end_price: Self::InitData,
	) -> Result<SalesStarted<T>, Self::Error> {
		let config = Configuration::<T>::get().ok_or(MarketError::Uninitialized)?;

		let commit_timeslice = Self::TimesliceProvider::latest_timeslice_ready_to_commit()
			.ok_or(MarketError::Uninitialized)?;

		// Imaginary old sale for bootstrapping the first actual sale:
		let old_sale = SaleInfoRecord {
			sale_start: block_number,
			leadin_length: Zero::zero(),
			end_price,
			sellout_price: None,
			region_begin: commit_timeslice,
			region_end: commit_timeslice.saturating_add(config.region_length),
			first_core: 0,
			ideal_cores_sold: 0,
			cores_offered: 0,
			cores_sold: 0,
		};

		let reserved_cores = Self::CoreCount::reserved_core_count();
		let core_count = Self::CoreCount::core_count().ok_or(MarketError::CoreCountUnknown)?;
		let (new_prices, new_sale) =
			rotate_sale::<T>(&old_sale, &config, core_count, reserved_cores, block_number);
		SaleInfo::<T>::put(&new_sale);

		let start_price = sell_price::<T>(block_number, &new_sale);

		Ok(SalesStarted { imaginary_old_sale: old_sale, new_sale, new_prices, start_price })
	}

	fn place_order(
		block_number: RelayBlockNumberOf<T>,
		_who: &AccountIdFor<T>,
		price_limit: BalanceOf<T>,
	) -> Result<OrderResult<T, Self::BidId>, Self::Error> {
		let mut sale = SaleInfo::<T>::get().ok_or(MarketError::NoSales)?;
		let core_count = Self::CoreCount::core_count().ok_or(MarketError::CoreCountUnknown)?;

		ensure!(sale.first_core < core_count, MarketError::Unavailable);
		ensure!(sale.cores_sold < sale.cores_offered, MarketError::SoldOut);

		ensure!(block_number > sale.sale_start, MarketError::TooEarly);

		let sell_price = sell_price::<T>(block_number, &sale);

		if price_limit < sell_price {
			return Err(MarketError::Overpriced);
		};

		let core = purchase_core::<T>(sell_price, &mut sale);
		SaleInfo::<T>::put(&sale);

		let region_id = RegionId { begin: sale.region_begin, core, mask: CoreMask::complete() };

		Ok(OrderResult::Sold { price: sell_price, region_id, region_end: sale.region_end })
	}

	fn place_renewal_order(
		block_number: RelayBlockNumberOf<T>,
		_who: &AccountIdFor<T>,
		_renewal: PotentialRenewalId,
		recorded_price: BalanceOf<T>,
	) -> Result<RenewalOrderResult<T, Self::BidId>, Self::Error> {
		let config = Configuration::<T>::get().ok_or(MarketError::Uninitialized)?;
		let core_count = Self::CoreCount::core_count().ok_or(MarketError::CoreCountUnknown)?;
		let mut sale = SaleInfo::<T>::get().ok_or(MarketError::NoSales)?;

		ensure!(sale.first_core < core_count, MarketError::Unavailable);
		ensure!(sale.cores_sold < sale.cores_offered, MarketError::SoldOut);

		let price_cap =
			cmp::max(recorded_price + config.renewal_bump * recorded_price, sale.end_price);
		let sell_price = sell_price::<T>(block_number, &sale);
		let next_renewal_price = sell_price.min(price_cap);

		let core = purchase_core::<T>(recorded_price, &mut sale);
		SaleInfo::<T>::put(&sale);

		let region_id = RegionId { core, begin: sale.region_begin, mask: CoreMask::complete() };

		return Ok(RenewalOrderResult::Sold {
			price: recorded_price,
			next_renewal_price,
			region_id,
			effective_to: sale.region_end,
		});
	}

	fn raise_bid(
		_block_number: RelayBlockNumberOf<T>,
		_id: Self::BidId,
		_who: &AccountIdFor<T>,
		_new_price: BalanceOf<T>,
	) -> Result<RaiseBidResult<T>, Self::Error> {
		Err(MarketError::BidNotExist)
	}

	fn tick(
		block_number: RelayBlockNumberOf<T>,
		weight_meter: &mut WeightMeter,
	) -> Vec<TickAction<T>> {
		let (Some(config), Some(core_count)) =
			(Configuration::<T>::get(), Self::CoreCount::core_count())
		else {
			return vec![];
		};

		let mut actions = vec![];

		if let Some(timeslice) = Self::TimesliceProvider::next_timeslice_to_commit() {
			if let Some(sale) = SaleInfo::<T>::get() {
				if timeslice >= sale.region_begin {
					weight_meter.consume(T::WeightInfo::market_sale_rotated());
					sale_rotated::<T, Self>(sale, &config, core_count, block_number, &mut actions);
				}
			}
		};

		actions
	}
}

pub(crate) fn sale_rotated<T: Config, M: Market<T>>(
	sale: SaleInfoRecordOf<T>,
	config: &ConfigRecordOf<T>,
	core_count: CoreIndex,
	block_number: RelayBlockNumberOf<T>,
	actions: &mut Vec<TickAction<T>>,
) {
	let reserved_cores = M::CoreCount::reserved_core_count();
	let (new_prices, new_sale) =
		rotate_sale::<T>(&sale, config, core_count, reserved_cores, block_number);
	SaleInfo::<T>::put(&new_sale);

	let start_price = sell_price::<T>(block_number, &new_sale);
	actions.push(TickAction::SaleRotated { old_sale: sale, new_sale, new_prices, start_price });
}

fn purchase_core<T: Config>(price: BalanceOf<T>, sale: &mut SaleInfoRecordOf<T>) -> CoreIndex {
	let core = sale.first_core.saturating_add(sale.cores_sold);
	sale.cores_sold.saturating_inc();
	if sale.cores_sold <= sale.ideal_cores_sold || sale.sellout_price.is_none() {
		sale.sellout_price = Some(price);
	}
	core
}

pub(crate) fn sell_price<T: Config>(
	now: RelayBlockNumberOf<T>,
	sale: &SaleInfoRecordOf<T>,
) -> BalanceOf<T> {
	let num = now.saturating_sub(sale.sale_start).min(sale.leadin_length).saturated_into();
	let through = FixedU64::from_rational(num, sale.leadin_length.saturated_into());
	leadin_factor_at(through).saturating_mul_int(sale.end_price)
}

pub(crate) fn leadin_factor_at(when: FixedU64) -> FixedU64 {
	if when <= FixedU64::from_rational(1, 2) {
		FixedU64::from(100).saturating_sub(when.saturating_mul(180.into()))
	} else {
		FixedU64::from(19).saturating_sub(when.saturating_mul(18.into()))
	}
}

// TODO: Don't rely on the pallet config?
fn adapt_prices<T: Config>(old_sale: &SaleInfoRecordOf<T>) -> AdaptedPrices<BalanceOf<T>> {
	// Calculate the start price for the upcoming sale.
	let new_prices = T::PriceAdapter::adapt_price(SalePerformance::from_sale(&old_sale));

	log::debug!(
		"Rotated sale, new prices: {:?}, {:?}",
		new_prices.end_price,
		new_prices.target_price
	);

	new_prices
}

pub(crate) fn rotate_sale<T: Config>(
	old_sale: &SaleInfoRecordOf<T>,
	config: &ConfigRecordOf<T>,
	core_count: CoreIndex,
	reserved_cores: CoreIndex,
	now: RelayBlockNumberOf<T>,
) -> (AdaptedPrices<BalanceOf<T>>, SaleInfoRecordOf<T>) {
	let new_prices = adapt_prices::<T>(&old_sale);

	let max_possible_sales = core_count.saturating_sub(reserved_cores);
	let limit_cores_offered = config.limit_cores_offered.unwrap_or(CoreIndex::max_value());
	let cores_offered = limit_cores_offered.min(max_possible_sales);
	let sale_start = now.saturating_add(config.interlude_length);
	let leadin_length = config.leadin_length;
	let ideal_cores_sold = (config.ideal_bulk_proportion * cores_offered as u32) as u16;
	let sellout_price = if cores_offered > 0 {
		// No core sold -> price was too high -> we have to adjust downwards.
		Some(new_prices.end_price)
	} else {
		None
	};

	let region_begin = old_sale.region_end;
	let region_end = region_begin + config.region_length;

	let new_sale = SaleInfoRecord {
		sale_start,
		leadin_length,
		end_price: new_prices.end_price,
		sellout_price,
		region_begin,
		region_end,
		first_core: reserved_cores,
		ideal_cores_sold,
		cores_offered,
		cores_sold: 0,
	};

	(new_prices, new_sale)
}
