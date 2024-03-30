// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Traits used across pallets for Polkadot.

use frame_support::{
	dispatch::DispatchResult,
	traits::{Currency, ReservableCurrency},
};
pub use pallet_paras_registrar::traits::{OnSwap, Registrar};
use primitives::{HeadData, Id as ParaId, ValidationCode};
use sp_std::vec::*;

/// Error type for something that went wrong with leasing.
#[derive(Debug)]
pub enum LeaseError {
	/// Unable to reserve the funds in the leaser's account.
	ReserveFailed,
	/// There is already a lease on at least one period for the given para.
	AlreadyLeased,
	/// The period to be leased has already ended.
	AlreadyEnded,
	/// A lease period has not started yet, due to an offset in the starting block.
	NoLeasePeriod,
}

/// Lease manager. Used by the auction module to handle parachain slot leases.
pub trait Leaser<BlockNumber> {
	/// An account identifier for a leaser.
	type AccountId;

	/// The measurement type for counting lease periods (generally just a `BlockNumber`).
	type LeasePeriod;

	/// The currency type in which the lease is taken.
	type Currency: ReservableCurrency<Self::AccountId>;

	/// Lease a new parachain slot for `para`.
	///
	/// `leaser` shall have a total of `amount` balance reserved by the implementer of this trait.
	///
	/// Note: The implementer of the trait (the leasing system) is expected to do all
	/// reserve/unreserve calls. The caller of this trait *SHOULD NOT* pre-reserve the deposit
	/// (though should ensure that it is reservable).
	///
	/// The lease will last from `period_begin` for `period_count` lease periods. It is undefined if
	/// the `para` already has a slot leased during those periods.
	///
	/// Returns `Err` in the case of an error, and in which case nothing is changed.
	fn lease_out(
		para: ParaId,
		leaser: &Self::AccountId,
		amount: <Self::Currency as Currency<Self::AccountId>>::Balance,
		period_begin: Self::LeasePeriod,
		period_count: Self::LeasePeriod,
	) -> Result<(), LeaseError>;

	/// Return the amount of balance currently held in reserve on `leaser`'s account for leasing
	/// `para`. This won't go down outside a lease period.
	fn deposit_held(
		para: ParaId,
		leaser: &Self::AccountId,
	) -> <Self::Currency as Currency<Self::AccountId>>::Balance;

	/// The length of a lease period, and any offset which may be introduced.
	/// This is only used in benchmarking to automate certain calls.
	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn lease_period_length() -> (BlockNumber, BlockNumber);

	/// Returns the lease period at `block`, and if this is the first block of a new lease period.
	///
	/// Will return `None` if the first lease period has not started yet, for example when an offset
	/// is placed.
	fn lease_period_index(block: BlockNumber) -> Option<(Self::LeasePeriod, bool)>;

	/// Returns true if the parachain already has a lease in any of lease periods in the inclusive
	/// range `[first_period, last_period]`, intersected with the unbounded range
	/// [`current_lease_period`..] .
	fn already_leased(
		para_id: ParaId,
		first_period: Self::LeasePeriod,
		last_period: Self::LeasePeriod,
	) -> bool;
}

/// An enum which tracks the status of the auction system, and which phase it is in.
#[derive(PartialEq, Debug)]
pub enum AuctionStatus<BlockNumber> {
	/// An auction has not started yet.
	NotStarted,
	/// We are in the starting period of the auction, collecting initial bids.
	StartingPeriod,
	/// We are in the ending period of the auction, where we are taking snapshots of the winning
	/// bids. This state supports "sampling", where we may only take a snapshot every N blocks.
	/// In this case, the first number is the current sample number, and the second number
	/// is the sub-sample. i.e. for sampling every 20 blocks, the 25th block in the ending period
	/// will be `EndingPeriod(1, 5)`.
	EndingPeriod(BlockNumber, BlockNumber),
	/// We have completed the bidding process and are waiting for the VRF to return some acceptable
	/// randomness to select the winner. The number represents how many blocks we have been
	/// waiting.
	VrfDelay(BlockNumber),
}

impl<BlockNumber> AuctionStatus<BlockNumber> {
	/// Returns true if the auction is in any state other than `NotStarted`.
	pub fn is_in_progress(&self) -> bool {
		!matches!(self, Self::NotStarted)
	}
	/// Return true if the auction is in the starting period.
	pub fn is_starting(&self) -> bool {
		matches!(self, Self::StartingPeriod)
	}
	/// Returns `Some(sample, sub_sample)` if the auction is in the `EndingPeriod`,
	/// otherwise returns `None`.
	pub fn is_ending(self) -> Option<(BlockNumber, BlockNumber)> {
		match self {
			Self::EndingPeriod(sample, sub_sample) => Some((sample, sub_sample)),
			_ => None,
		}
	}
	/// Returns true if the auction is in the `VrfDelay` period.
	pub fn is_vrf(&self) -> bool {
		matches!(self, Self::VrfDelay(_))
	}
}

pub trait Auctioneer<BlockNumber> {
	/// An account identifier for a leaser.
	type AccountId;

	/// The measurement type for counting lease periods (generally the same as `BlockNumber`).
	type LeasePeriod;

	/// The currency type in which the lease is taken.
	type Currency: ReservableCurrency<Self::AccountId>;

	/// Create a new auction.
	///
	/// This can only happen when there isn't already an auction in progress. Accepts the `duration`
	/// of this auction and the `lease_period_index` of the initial lease period of the four that
	/// are to be auctioned.
	fn new_auction(duration: BlockNumber, lease_period_index: Self::LeasePeriod) -> DispatchResult;

	/// Given the current block number, return the current auction status.
	fn auction_status(now: BlockNumber) -> AuctionStatus<BlockNumber>;

	/// Place a bid in the current auction.
	///
	/// - `bidder`: The account that will be funding this bid.
	/// - `para`: The para to bid for.
	/// - `first_slot`: The first lease period index of the range to be bid on.
	/// - `last_slot`: The last lease period index of the range to be bid on (inclusive).
	/// - `amount`: The total amount to be the bid for deposit over the range.
	///
	/// The account `Bidder` must have at least `amount` available as a free balance in `Currency`.
	/// The implementation *MUST* remove or reserve `amount` funds from `bidder` and those funds
	/// should be returned or freed once the bid is rejected or lease has ended.
	fn place_bid(
		bidder: Self::AccountId,
		para: ParaId,
		first_slot: Self::LeasePeriod,
		last_slot: Self::LeasePeriod,
		amount: <Self::Currency as Currency<Self::AccountId>>::Balance,
	) -> DispatchResult;

	/// The length of a lease period, and any offset which may be introduced.
	/// This is only used in benchmarking to automate certain calls.
	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn lease_period_length() -> (BlockNumber, BlockNumber);

	/// Returns the lease period at `block`, and if this is the first block of a new lease period.
	///
	/// Will return `None` if the first lease period has not started yet, for example when an offset
	/// is placed.
	fn lease_period_index(block: BlockNumber) -> Option<(Self::LeasePeriod, bool)>;

	/// Check if the para and user combination has won an auction in the past.
	fn has_won_an_auction(para: ParaId, bidder: &Self::AccountId) -> bool;
}
