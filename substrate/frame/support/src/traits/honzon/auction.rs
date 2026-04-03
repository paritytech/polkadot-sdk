// This file is part of Substrate.
//
// Copyright (C) 2020-2025 Acala Foundation.
// SPDX-License-Identifier: Apache-2.0
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Auction traits and supporting types.
//!
//! These abstractions encapsulate the lifecycle of on-chain auctions and the reactions to
//! bidding activity. Pallets implement [`Auction`] to manage storage and scheduling for auction
//! instances, while [`AuctionHandler`] allows downstream logic to respond to bids and
//! completion events.
//!
//! # Provided Types
//! - [`AuctionInfo`]: Captures the live state of an auction, including the active bid and block
//!   schedule.
//! - [`Auction`]: Primary interface for creating, updating, and removing auction instances.
//! - [`AuctionHandler`]: Callback interface for integrating business logic when bids arrive or
//!   auctions conclude.
//! - [`OnNewBidResult`]: Return value describing whether a bid is accepted and how the end block
//!   should change.
//! - [`Change`]: Helper enum indicating whether a value should remain untouched or be replaced.

use codec::{Decode, DecodeWithMemTracking, Encode, FullCodec, MaxEncodedLen};
use core::fmt::Debug;
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{AtLeast32Bit, Bounded, MaybeSerializeDeserialize},
	DispatchError, DispatchResult, RuntimeDebug,
};
use sp_std::result;

/// Snapshot of an auction's state.
///
/// Implementers typically persist this structure alongside an `AuctionId` to track the active bid
/// and configured schedule.
#[derive(PartialEq, Eq, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct AuctionInfo<AccountId, Balance, BlockNumber> {
	/// The current bidder and their bid, if any.
	pub bid: Option<(AccountId, Balance)>,
	/// The block number at which the auction started.
	pub start: BlockNumber,
	/// The block number at which the auction will end, if set.
	pub end: Option<BlockNumber>,
}

/// Core interface for managing auction instances.
pub trait Auction<AccountId, BlockNumber> {
	/// The type used to identify an auction.
	type AuctionId: FullCodec
		+ Default
		+ Copy
		+ Eq
		+ PartialEq
		+ MaybeSerializeDeserialize
		+ Bounded
		+ Debug;
	/// The type used to represent the bid price.
	type Balance: AtLeast32Bit + FullCodec + Copy + MaybeSerializeDeserialize + Debug + Default;

	/// Returns the information for a given auction.
	fn auction_info(
		id: Self::AuctionId,
	) -> Option<AuctionInfo<AccountId, Self::Balance, BlockNumber>>;
	/// Updates the information for a given auction.
	fn update_auction(
		id: Self::AuctionId,
		info: AuctionInfo<AccountId, Self::Balance, BlockNumber>,
	) -> DispatchResult;
	/// Creates a new auction.
	///
	/// Returns the ID of the new auction.
	fn new_auction(
		start: BlockNumber,
		end: Option<BlockNumber>,
	) -> result::Result<Self::AuctionId, DispatchError>;
	/// Removes an auction.
	fn remove_auction(id: Self::AuctionId);
}

/// Outcome of processing a new bid.
pub struct OnNewBidResult<BlockNumber> {
	/// Whether the bid was accepted.
	pub accept_bid: bool,
	/// A potential change to the auction's end time.
	pub auction_end_change: Change<Option<BlockNumber>>,
}

/// Represents a potential change to a value.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Clone,
	Eq,
	PartialEq,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum Change<Value> {
	/// No change is required.
	NoChange,
	/// The value should be changed to the new value.
	NewValue(Value),
}

/// Callbacks invoked in response to auction events.
pub trait AuctionHandler<AccountId, Balance, BlockNumber, AuctionId> {
	/// Called when a new bid is received.
	///
	/// The return value determines whether the bid should be accepted and whether
	/// the auction's end time should be updated. The implementation should
	/// reserve funds from the new bidder and refund the previous bidder.
	fn on_new_bid(
		now: BlockNumber,
		id: AuctionId,
		new_bid: (AccountId, Balance),
		last_bid: Option<(AccountId, Balance)>,
	) -> OnNewBidResult<BlockNumber>;
	/// Called when an auction has ended.
	fn on_auction_ended(id: AuctionId, winner: Option<(AccountId, Balance)>);
}
