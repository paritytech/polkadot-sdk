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

use crate::PotentialRenewalId;

/// Trait representig generic market logic.
///
/// The assumptions for this generic market are:
/// - Every order will either create a bid or will be resolved immediately.
/// - There're two types of orders: bulk coretime purchase and bulk coretime renewal.
/// - Coretime regions are fungible.
pub trait Market<Balance, BlockNumber, AccountId> {
	type Error;
	/// Internal market state that must be preserved between the method calls. If the market logic
	/// allows creating bids they should be stored there as well as the bid structure depends on the
	/// market implementation.
	type State;
	/// Unique ID assigned to every bid.
	type BidId;

	/// Place an order for bulk coretime purchase.
	///
	/// This method may or may not create a bid, according to the market rules.
	///
	/// - `since_timeslice_start` - amount of blocks passed since the current timeslice start
	/// - `amount` - maximum price which the buyer is willing to pay (or None if it's defined by the
	///   market itself)
	/// - `state` - market state, the caller is responsible for storing it
	fn place_order(
		since_timeslice_start: BlockNumber,
		who: AccountId,
		amount: Option<Balance>,
		state: &mut Self::State,
	) -> Result<PlaceOrderOutcome<Balance, Self::BidId>, Self::Error>;

	/// Place an order for bulk coretime renewal.
	///
	/// This method may or may not create a bid, according to the market rules.
	///
	/// - `since_timeslice_start` - amount of blocks passed since the current timeslice start
	/// - `buying_price` - price which was paid for this region the last time it was sold
	/// - `state` - market state, the caller is responsible for storing it
	fn place_renewal_order(
		since_timeslice_start: BlockNumber,
		who: AccountId,
		renewal: PotentialRenewalId,
		buying_price: Balance,
		state: &mut Self::State,
	) -> Result<PlaceRenewalOrderOutcome<Balance, Self::BidId>, Self::Error>;

	/// Close the bid given its `BidId`.
	///
	/// If the market logic allows creating the bids this method allows to close any bids (either
	/// forcefully if `maybe_check_owner` is `None` or checking the bid owner if it's `Some`).
	fn close_bid(
		id: Self::BidId,
		maybe_check_owner: Option<AccountId>,
		state: &mut Self::State,
	) -> Result<(), Self::Error>;

	/// Logic that gets called in `on_initialize` hook.
	fn tick(
		since_timeslice_start: BlockNumber,
		state: &mut Self::State,
	) -> Result<Vec<TickAction<AccountId, Balance, Self::BidId>>, Self::Error>;
}

enum PlaceOrderOutcome<Balance, BidId> {
	BidPlaced { id: BidId, bid_amount: Balance },
	Sold { price: Balance },
}

enum PlaceRenewalOrderOutcome<Balance, BidId> {
	BidPlaced { id: BidId, bid_amount: Balance },
	Sold { price: Balance },
}

enum TickAction<AccountId, Balance, BidId> {
	SellRegion { who: AccountId, refund: Balance },
	RenewRegion { who: AccountId, renewal_id: PotentialRenewalId, refund: Balance },
	CloseBid { id: BidId, amount: Balance },
}
