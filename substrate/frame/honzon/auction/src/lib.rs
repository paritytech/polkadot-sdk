// This file is part of Substrate.

// Copyright (C) 2020-2025 Acala Foundation.
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

//! # Auction Pallet
//!
//! A generic pallet for on-chain auctions that enables creation and management of auctions for
//! any type of asset.
//!
//! ## Pallet API
//!
//! See the [`pallet`] module for more information about the interfaces this pallet exposes,
//! including its configuration trait, dispatchables, storage items, events and errors.
//!
//! ## Overview
//!
//! This pallet provides a generic framework for on-chain auctions. It allows for the creation
//! and management of auctions for any type of asset. The core logic of the auction, such as
//! bid validation and what happens when an auction ends, is customizable through the
//! [`AuctionHandler`] trait.
//!
//! This pallet is designed to be flexible and can be used to implement various auction
//! types, such as English auctions, Dutch auctions, or other custom formats.
//!
//! ### Features
//!
//! - **Generic Auction Mechanism:** Can be used for auctioning any asset.
//! - **Customizable Logic:** The [`AuctionHandler`] trait allows for custom implementation of
//!   auction logic.
//! - **Scheduled Auctions:** Auctions can be scheduled to start at a future block number.
//! - **Automatic Auction Closing:** Auctions are automatically closed at their end block number in
//!   the on_finalize hook.
//! - **Standard Interface:** Implements the [`Auction`] trait so other pallets can query and update
//!   auctions via a shared API.
//!
//! ## Low Level / Implementation Details
//!
//!
//! ### Design
//!
//! The pallet uses a simple but effective storage design:
//!
//! - **`Auctions<T>`:** Maps auction IDs to auction information including current bid and timing
//! - **`AuctionEndTime<T>`:** Double map from end block to auction ID for efficient batch
//!   processing of ending auctions
//! - **`AuctionsIndex<T>`:** Tracks the next available auction ID for new auction creation
//!
//! ### Auction Lifecycle
//!
//! 1. **Creation:** Auctions are created with a start block and optional end block
//! 2. **Active Phase:** Bids can be placed during the active period
//! 3. **Bid Processing:** Each bid is validated by the handler and may extend the auction
//! 4. **Conclusion:** Auctions are automatically concluded at their end block
//!
//!
//! ### Terminology
//!
//! - **Auction:** A process where participants place bids for an item, with the highest bidder
//!   winning
//! - **Bid:** An offer of a specific price by a participant
//! - **Auction Handler:** A [`AuctionHandler`] implementation that defines custom auction logic and
//!   validation rules

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]

use codec::MaxEncodedLen;
use frame_support::{
	pallet_prelude::*,
	traits::honzon::{Auction, AuctionHandler, AuctionInfo, Change},
};
use frame_system::{ensure_signed, pallet_prelude::*};
use sp_runtime::{
	traits::{
		AtLeast32BitUnsigned, Bounded, CheckedAdd, MaybeSerializeDeserialize, Member, One, Zero,
	},
	DispatchError, DispatchResult,
};

mod benchmarking;
mod mock;
mod tests;
mod weights;

pub use pallet::*;
pub use weights::WeightInfo;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The balance type for bidding in auctions.
		type Balance: Parameter
			+ Member
			+ AtLeast32BitUnsigned
			+ Default
			+ Copy
			+ MaybeSerializeDeserialize
			+ MaxEncodedLen;

		/// The type for identifying auctions.
		type AuctionId: Parameter
			+ Member
			+ AtLeast32BitUnsigned
			+ Default
			+ Copy
			+ MaybeSerializeDeserialize
			+ Bounded
			+ codec::FullCodec
			+ codec::MaxEncodedLen;

		/// The handler for custom auction logic.
		type Handler: AuctionHandler<
			Self::AccountId,
			Self::Balance,
			BlockNumberFor<Self>,
			Self::AuctionId,
		>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The specified auction does not exist.
		AuctionNotExist,
		/// The auction has not started yet.
		AuctionNotStarted,
		/// The bid was not accepted by the
		/// [`AuctionHandler`] implementation.
		BidNotAccepted,
		/// The bid price is invalid. It might be lower than or equal to the
		/// current highest bid, or it might be zero.
		InvalidBidPrice,
		/// There are no available auction IDs to be assigned to a new auction.
		NoAvailableAuctionId,
	}

	#[pallet::event]
	#[pallet::generate_deposit(fn deposit_event)]
	pub enum Event<T: Config> {
		/// A bid was successfully placed in an auction.
		Bid {
			/// The ID of the auction.
			auction_id: T::AuctionId,
			/// The account that placed the bid.
			bidder: T::AccountId,
			/// The amount of the bid.
			amount: T::Balance,
		},
	}

	/// Stores ongoing and future auctions. Closed auctions are removed.
	///
	/// Key: Auction ID
	/// Value: Auction information
	#[pallet::storage]
	#[pallet::getter(fn auctions)]
	pub type Auctions<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::AuctionId,
		AuctionInfo<T::AccountId, T::Balance, BlockNumberFor<T>>,
		OptionQuery,
	>;

	/// Tracks the next available auction ID.
	#[pallet::storage]
	#[pallet::getter(fn auctions_index)]
	pub type AuctionsIndex<T: Config> = StorageValue<_, T::AuctionId, ValueQuery>;

	/// A mapping from block number to a list of auctions that end at that block.
	/// This is used to efficiently process auctions that have ended.
	#[pallet::storage]
	#[pallet::getter(fn auction_end_time)]
	pub type AuctionEndTime<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		BlockNumberFor<T>,
		Blake2_128Concat,
		T::AuctionId,
		(),
		OptionQuery,
	>;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(now: BlockNumberFor<T>) -> Weight {
			T::WeightInfo::on_finalize(AuctionEndTime::<T>::iter_prefix(now).count() as u32)
		}

		fn on_finalize(now: BlockNumberFor<T>) {
			for (auction_id, _) in AuctionEndTime::<T>::drain_prefix(now) {
				if let Some(auction) = Auctions::<T>::take(auction_id) {
					T::Handler::on_auction_ended(auction_id, auction.bid);
				}
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Place a bid in an ongoing auction.
		///
		/// ## Dispatch Origin
		///
		/// The dispatch origin of this call must be `Signed`.
		///
		/// ## Details
		///
		/// This function allows a signed account to place a bid in an auction that has already
		/// started. The bid amount must be higher than any existing bid and will be validated by
		/// the [`AuctionHandler`] implementation.
		///
		/// ## Errors
		///
		/// - BadOrigin: The dispatch origin is not a signed account
		/// - [`Error::AuctionNotExist`]: The specified auction ID does not exist
		/// - [`Error::AuctionNotStarted`]: The auction has not started yet
		/// - [`Error::BidNotAccepted`]: The bid was rejected by the auction handler
		/// - [`Error::InvalidBidPrice`]: The bid amount is zero or not higher than the current
		///   highest bid
		///
		/// ## Events
		///
		/// - [`Event::Bid`]: Emitted when a bid is successfully placed
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::bid())]
		pub fn bid(
			origin: OriginFor<T>,
			id: T::AuctionId,
			#[pallet::compact] value: T::Balance,
		) -> DispatchResult {
			let from = ensure_signed(origin)?;

			Auctions::<T>::try_mutate_exists(id, |auction| -> DispatchResult {
				let auction = auction.as_mut().ok_or(Error::<T>::AuctionNotExist)?;

				let block_number = <frame_system::Pallet<T>>::block_number();

				// make sure auction is started
				ensure!(block_number >= auction.start, Error::<T>::AuctionNotStarted);

				if let Some(ref current_bid) = auction.bid {
					ensure!(value > current_bid.1, Error::<T>::InvalidBidPrice);
				} else {
					ensure!(!value.is_zero(), Error::<T>::InvalidBidPrice);
				}
				let bid_result = T::Handler::on_new_bid(
					block_number,
					id,
					(from.clone(), value),
					auction.bid.clone(),
				);

				ensure!(bid_result.accept_bid, Error::<T>::BidNotAccepted);
				match bid_result.auction_end_change {
					Change::NewValue(new_end) => {
						if let Some(old_end_block) = auction.end {
							AuctionEndTime::<T>::remove(old_end_block, id);
						}
						if let Some(new_end_block) = new_end {
							AuctionEndTime::<T>::insert(new_end_block, id, ());
						}
						auction.end = new_end;
					},
					Change::NoChange => {},
				}
				auction.bid = Some((from.clone(), value));

				Ok(())
			})?;

			Self::deposit_event(Event::Bid { auction_id: id, bidder: from, amount: value });
			Ok(())
		}
	}
}

impl<T: Config> Auction<T::AccountId, BlockNumberFor<T>> for Pallet<T> {
	type AuctionId = T::AuctionId;
	type Balance = T::Balance;

	fn auction_info(
		id: T::AuctionId,
	) -> Option<AuctionInfo<T::AccountId, T::Balance, BlockNumberFor<T>>> {
		Self::auctions(id)
	}

	fn update_auction(
		id: T::AuctionId,
		info: AuctionInfo<T::AccountId, T::Balance, BlockNumberFor<T>>,
	) -> DispatchResult {
		let auction = Auctions::<T>::get(id).ok_or(Error::<T>::AuctionNotExist)?;
		if let Some(old_end) = auction.end {
			AuctionEndTime::<T>::remove(old_end, id);
		}
		if let Some(new_end) = info.end {
			AuctionEndTime::<T>::insert(new_end, id, ());
		}
		Auctions::<T>::insert(id, info);
		Ok(())
	}

	fn new_auction(
		start: BlockNumberFor<T>,
		end: Option<BlockNumberFor<T>>,
	) -> sp_std::result::Result<T::AuctionId, DispatchError> {
		let auction = AuctionInfo { bid: None, start, end };
		let auction_id = <AuctionsIndex<T>>::try_mutate(
			|n| -> sp_std::result::Result<T::AuctionId, DispatchError> {
				let id = *n;
				*n = n.checked_add(&One::one()).ok_or(Error::<T>::NoAvailableAuctionId)?;
				Ok(id)
			},
		)?;
		Auctions::<T>::insert(auction_id, auction);
		if let Some(end_block) = end {
			AuctionEndTime::<T>::insert(end_block, auction_id, ());
		}

		Ok(auction_id)
	}

	fn remove_auction(id: T::AuctionId) {
		if let Some(auction) = Auctions::<T>::take(id) {
			if let Some(end_block) = auction.end {
				AuctionEndTime::<T>::remove(end_block, id);
			}
		}
	}
}
