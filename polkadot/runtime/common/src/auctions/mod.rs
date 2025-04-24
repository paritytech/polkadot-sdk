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

//! Auctioning system to determine the set of Parachains in operation. This includes logic for the
//! auctioning mechanism and for reserving balance as part of the "payment". Unreserving the balance
//! happens elsewhere.

use crate::{
	slot_range::SlotRange,
	traits::{AuctionStatus, Auctioneer, LeaseError, Leaser, Registrar},
};
use alloc::{vec, vec::Vec};
use codec::Decode;
use core::mem::swap;
use frame_support::{
	dispatch::DispatchResult,
	ensure,
	traits::{Currency, Get, Randomness, ReservableCurrency},
	weights::Weight,
};
use frame_system::pallet_prelude::BlockNumberFor;
pub use pallet::*;
use polkadot_primitives::Id as ParaId;
use sp_runtime::traits::{CheckedSub, One, Saturating, Zero};

type CurrencyOf<T> = <<T as Config>::Leaser as Leaser<BlockNumberFor<T>>>::Currency;
type BalanceOf<T> = <<<T as Config>::Leaser as Leaser<BlockNumberFor<T>>>::Currency as Currency<
	<T as frame_system::Config>::AccountId,
>>::Balance;

pub trait WeightInfo {
	fn new_auction() -> Weight;
	fn bid() -> Weight;
	fn cancel_auction() -> Weight;
	fn on_initialize() -> Weight;
}

pub struct TestWeightInfo;
impl WeightInfo for TestWeightInfo {
	fn new_auction() -> Weight {
		Weight::zero()
	}
	fn bid() -> Weight {
		Weight::zero()
	}
	fn cancel_auction() -> Weight {
		Weight::zero()
	}
	fn on_initialize() -> Weight {
		Weight::zero()
	}
}

/// An auction index. We count auctions in this type.
pub type AuctionIndex = u32;

type LeasePeriodOf<T> = <<T as Config>::Leaser as Leaser<BlockNumberFor<T>>>::LeasePeriod;

// Winning data type. This encodes the top bidders of each range together with their bid.
type WinningData<T> = [Option<(<T as frame_system::Config>::AccountId, ParaId, BalanceOf<T>)>;
	SlotRange::SLOT_RANGE_COUNT];
// Winners data type. This encodes each of the final winners of a parachain auction, the parachain
// index assigned to them, their winning bid and the range that they won.
type WinnersData<T> =
	Vec<(<T as frame_system::Config>::AccountId, ParaId, BalanceOf<T>, SlotRange)>;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{dispatch::DispatchClass, pallet_prelude::*, traits::EnsureOrigin};
	use frame_system::{ensure_root, ensure_signed, pallet_prelude::*};

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// The module's configuration trait.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The type representing the leasing system.
		type Leaser: Leaser<
			BlockNumberFor<Self>,
			AccountId = Self::AccountId,
			LeasePeriod = BlockNumberFor<Self>,
		>;

		/// The parachain registrar type.
		type Registrar: Registrar<AccountId = Self::AccountId>;

		/// The number of blocks over which an auction may be retroactively ended.
		#[pallet::constant]
		type EndingPeriod: Get<BlockNumberFor<Self>>;

		/// The length of each sample to take during the ending period.
		///
		/// `EndingPeriod` / `SampleLength` = Total # of Samples
		#[pallet::constant]
		type SampleLength: Get<BlockNumberFor<Self>>;

		/// Something that provides randomness in the runtime.
		type Randomness: Randomness<Self::Hash, BlockNumberFor<Self>>;

		/// The origin which may initiate auctions.
		type InitiateOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Weight Information for the Extrinsics in the Pallet
		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An auction started. Provides its index and the block number where it will begin to
		/// close and the first lease period of the quadruplet that is auctioned.
		AuctionStarted {
			auction_index: AuctionIndex,
			lease_period: LeasePeriodOf<T>,
			ending: BlockNumberFor<T>,
		},
		/// An auction ended. All funds become unreserved.
		AuctionClosed { auction_index: AuctionIndex },
		/// Funds were reserved for a winning bid. First balance is the extra amount reserved.
		/// Second is the total.
		Reserved { bidder: T::AccountId, extra_reserved: BalanceOf<T>, total_amount: BalanceOf<T> },
		/// Funds were unreserved since bidder is no longer active. `[bidder, amount]`
		Unreserved { bidder: T::AccountId, amount: BalanceOf<T> },
		/// Someone attempted to lease the same slot twice for a parachain. The amount is held in
		/// reserve but no parachain slot has been leased.
		ReserveConfiscated { para_id: ParaId, leaser: T::AccountId, amount: BalanceOf<T> },
		/// A new bid has been accepted as the current winner.
		BidAccepted {
			bidder: T::AccountId,
			para_id: ParaId,
			amount: BalanceOf<T>,
			first_slot: LeasePeriodOf<T>,
			last_slot: LeasePeriodOf<T>,
		},
		/// The winning offset was chosen for an auction. This will map into the `Winning` storage
		/// map.
		WinningOffset { auction_index: AuctionIndex, block_number: BlockNumberFor<T> },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// This auction is already in progress.
		AuctionInProgress,
		/// The lease period is in the past.
		LeasePeriodInPast,
		/// Para is not registered
		ParaNotRegistered,
		/// Not a current auction.
		NotCurrentAuction,
		/// Not an auction.
		NotAuction,
		/// Auction has already ended.
		AuctionEnded,
		/// The para is already leased out for part of this range.
		AlreadyLeasedOut,
	}

	/// Number of auctions started so far.
	#[pallet::storage]
	pub type AuctionCounter<T> = StorageValue<_, AuctionIndex, ValueQuery>;

	/// Information relating to the current auction, if there is one.
	///
	/// The first item in the tuple is the lease period index that the first of the four
	/// contiguous lease periods on auction is for. The second is the block number when the
	/// auction will "begin to end", i.e. the first block of the Ending Period of the auction.
	#[pallet::storage]
	pub type AuctionInfo<T: Config> = StorageValue<_, (LeasePeriodOf<T>, BlockNumberFor<T>)>;

	/// Amounts currently reserved in the accounts of the bidders currently winning
	/// (sub-)ranges.
	#[pallet::storage]
	pub type ReservedAmounts<T: Config> =
		StorageMap<_, Twox64Concat, (T::AccountId, ParaId), BalanceOf<T>>;

	/// The winning bids for each of the 10 ranges at each sample in the final Ending Period of
	/// the current auction. The map's key is the 0-based index into the Sample Size. The
	/// first sample of the ending period is 0; the last is `Sample Size - 1`.
	#[pallet::storage]
	pub type Winning<T: Config> = StorageMap<_, Twox64Concat, BlockNumberFor<T>, WinningData<T>>;

	#[pallet::extra_constants]
	impl<T: Config> Pallet<T> {
		#[pallet::constant_name(SlotRangeCount)]
		fn slot_range_count() -> u32 {
			SlotRange::SLOT_RANGE_COUNT as u32
		}

		#[pallet::constant_name(LeasePeriodsPerSlot)]
		fn lease_periods_per_slot() -> u32 {
			SlotRange::LEASE_PERIODS_PER_SLOT as u32
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			let mut weight = T::DbWeight::get().reads(1);

			// If the current auction was in its ending period last block, then ensure that the
			// (sub-)range winner information is duplicated from the previous block in case no bids
			// happened in the last block.
			if let AuctionStatus::EndingPeriod(offset, _sub_sample) = Self::auction_status(n) {
				weight = weight.saturating_add(T::DbWeight::get().reads(1));
				if !Winning::<T>::contains_key(&offset) {
					weight = weight.saturating_add(T::DbWeight::get().writes(1));
					let winning_data = offset
						.checked_sub(&One::one())
						.and_then(Winning::<T>::get)
						.unwrap_or([Self::EMPTY; SlotRange::SLOT_RANGE_COUNT]);
					Winning::<T>::insert(offset, winning_data);
				}
			}

			// Check to see if an auction just ended.
			if let Some((winning_ranges, auction_lease_period_index)) = Self::check_auction_end(n) {
				// Auction is ended now. We have the winning ranges and the lease period index which
				// acts as the offset. Handle it.
				Self::manage_auction_end(auction_lease_period_index, winning_ranges);
				weight = weight.saturating_add(T::WeightInfo::on_initialize());
			}

			weight
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a new auction.
		///
		/// This can only happen when there isn't already an auction in progress and may only be
		/// called by the root origin. Accepts the `duration` of this auction and the
		/// `lease_period_index` of the initial lease period of the four that are to be auctioned.
		#[pallet::call_index(0)]
		#[pallet::weight((T::WeightInfo::new_auction(), DispatchClass::Operational))]
		pub fn new_auction(
			origin: OriginFor<T>,
			#[pallet::compact] duration: BlockNumberFor<T>,
			#[pallet::compact] lease_period_index: LeasePeriodOf<T>,
		) -> DispatchResult {
			T::InitiateOrigin::ensure_origin(origin)?;
			Self::do_new_auction(duration, lease_period_index)
		}

		/// Make a new bid from an account (including a parachain account) for deploying a new
		/// parachain.
		///
		/// Multiple simultaneous bids from the same bidder are allowed only as long as all active
		/// bids overlap each other (i.e. are mutually exclusive). Bids cannot be redacted.
		///
		/// - `sub` is the sub-bidder ID, allowing for multiple competing bids to be made by (and
		/// funded by) the same account.
		/// - `auction_index` is the index of the auction to bid on. Should just be the present
		/// value of `AuctionCounter`.
		/// - `first_slot` is the first lease period index of the range to bid on. This is the
		/// absolute lease period index value, not an auction-specific offset.
		/// - `last_slot` is the last lease period index of the range to bid on. This is the
		/// absolute lease period index value, not an auction-specific offset.
		/// - `amount` is the amount to bid to be held as deposit for the parachain should the
		/// bid win. This amount is held throughout the range.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::bid())]
		pub fn bid(
			origin: OriginFor<T>,
			#[pallet::compact] para: ParaId,
			#[pallet::compact] auction_index: AuctionIndex,
			#[pallet::compact] first_slot: LeasePeriodOf<T>,
			#[pallet::compact] last_slot: LeasePeriodOf<T>,
			#[pallet::compact] amount: BalanceOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Self::handle_bid(who, para, auction_index, first_slot, last_slot, amount)?;
			Ok(())
		}

		/// Cancel an in-progress auction.
		///
		/// Can only be called by Root origin.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::cancel_auction())]
		pub fn cancel_auction(origin: OriginFor<T>) -> DispatchResult {
			ensure_root(origin)?;
			// Unreserve all bids.
			for ((bidder, _), amount) in ReservedAmounts::<T>::drain() {
				CurrencyOf::<T>::unreserve(&bidder, amount);
			}
			#[allow(deprecated)]
			Winning::<T>::remove_all(None);
			AuctionInfo::<T>::kill();
			Ok(())
		}
	}
}

impl<T: Config> Auctioneer<BlockNumberFor<T>> for Pallet<T> {
	type AccountId = T::AccountId;
	type LeasePeriod = BlockNumberFor<T>;
	type Currency = CurrencyOf<T>;

	fn new_auction(
		duration: BlockNumberFor<T>,
		lease_period_index: LeasePeriodOf<T>,
	) -> DispatchResult {
		Self::do_new_auction(duration, lease_period_index)
	}

	// Returns the status of the auction given the current block number.
	fn auction_status(now: BlockNumberFor<T>) -> AuctionStatus<BlockNumberFor<T>> {
		let early_end = match AuctionInfo::<T>::get() {
			Some((_, early_end)) => early_end,
			None => return AuctionStatus::NotStarted,
		};

		let after_early_end = match now.checked_sub(&early_end) {
			Some(after_early_end) => after_early_end,
			None => return AuctionStatus::StartingPeriod,
		};

		let ending_period = T::EndingPeriod::get();
		if after_early_end < ending_period {
			let sample_length = T::SampleLength::get().max(One::one());
			let sample = after_early_end / sample_length;
			let sub_sample = after_early_end % sample_length;
			return AuctionStatus::EndingPeriod(sample, sub_sample)
		} else {
			// This is safe because of the comparison operator above
			return AuctionStatus::VrfDelay(after_early_end - ending_period)
		}
	}

	fn place_bid(
		bidder: T::AccountId,
		para: ParaId,
		first_slot: LeasePeriodOf<T>,
		last_slot: LeasePeriodOf<T>,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		Self::handle_bid(bidder, para, AuctionCounter::<T>::get(), first_slot, last_slot, amount)
	}

	fn lease_period_index(b: BlockNumberFor<T>) -> Option<(Self::LeasePeriod, bool)> {
		T::Leaser::lease_period_index(b)
	}

	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn lease_period_length() -> (BlockNumberFor<T>, BlockNumberFor<T>) {
		T::Leaser::lease_period_length()
	}

	fn has_won_an_auction(para: ParaId, bidder: &T::AccountId) -> bool {
		!T::Leaser::deposit_held(para, bidder).is_zero()
	}
}

impl<T: Config> Pallet<T> {
	// A trick to allow me to initialize large arrays with nothing in them.
	const EMPTY: Option<(<T as frame_system::Config>::AccountId, ParaId, BalanceOf<T>)> = None;

	/// Create a new auction.
	///
	/// This can only happen when there isn't already an auction in progress. Accepts the `duration`
	/// of this auction and the `lease_period_index` of the initial lease period of the four that
	/// are to be auctioned.
	fn do_new_auction(
		duration: BlockNumberFor<T>,
		lease_period_index: LeasePeriodOf<T>,
	) -> DispatchResult {
		let maybe_auction = AuctionInfo::<T>::get();
		ensure!(maybe_auction.is_none(), Error::<T>::AuctionInProgress);
		let now = frame_system::Pallet::<T>::block_number();
		if let Some((current_lease_period, _)) = T::Leaser::lease_period_index(now) {
			// If there is no active lease period, then we don't need to make this check.
			ensure!(lease_period_index >= current_lease_period, Error::<T>::LeasePeriodInPast);
		}

		// Bump the counter.
		let n = AuctionCounter::<T>::mutate(|n| {
			*n += 1;
			*n
		});

		// Set the information.
		let ending = frame_system::Pallet::<T>::block_number().saturating_add(duration);
		AuctionInfo::<T>::put((lease_period_index, ending));

		Self::deposit_event(Event::<T>::AuctionStarted {
			auction_index: n,
			lease_period: lease_period_index,
			ending,
		});
		Ok(())
	}

	/// Actually place a bid in the current auction.
	///
	/// - `bidder`: The account that will be funding this bid.
	/// - `auction_index`: The auction index of the bid. For this to succeed, must equal
	/// the current value of `AuctionCounter`.
	/// - `first_slot`: The first lease period index of the range to be bid on.
	/// - `last_slot`: The last lease period index of the range to be bid on (inclusive).
	/// - `amount`: The total amount to be the bid for deposit over the range.
	pub fn handle_bid(
		bidder: T::AccountId,
		para: ParaId,
		auction_index: u32,
		first_slot: LeasePeriodOf<T>,
		last_slot: LeasePeriodOf<T>,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		// Ensure para is registered before placing a bid on it.
		ensure!(T::Registrar::is_registered(para), Error::<T>::ParaNotRegistered);
		// Bidding on latest auction.
		ensure!(auction_index == AuctionCounter::<T>::get(), Error::<T>::NotCurrentAuction);
		// Assume it's actually an auction (this should never fail because of above).
		let (first_lease_period, _) = AuctionInfo::<T>::get().ok_or(Error::<T>::NotAuction)?;

		// Get the auction status and the current sample block. For the starting period, the sample
		// block is zero.
		let auction_status = Self::auction_status(frame_system::Pallet::<T>::block_number());
		// The offset into the ending samples of the auction.
		let offset = match auction_status {
			AuctionStatus::NotStarted => return Err(Error::<T>::AuctionEnded.into()),
			AuctionStatus::StartingPeriod => Zero::zero(),
			AuctionStatus::EndingPeriod(o, _) => o,
			AuctionStatus::VrfDelay(_) => return Err(Error::<T>::AuctionEnded.into()),
		};

		// We also make sure that the bid is not for any existing leases the para already has.
		ensure!(
			!T::Leaser::already_leased(para, first_slot, last_slot),
			Error::<T>::AlreadyLeasedOut
		);

		// Our range.
		let range = SlotRange::new_bounded(first_lease_period, first_slot, last_slot)?;
		// Range as an array index.
		let range_index = range as u8 as usize;

		// The current winning ranges.
		let mut current_winning = Winning::<T>::get(offset)
			.or_else(|| offset.checked_sub(&One::one()).and_then(Winning::<T>::get))
			.unwrap_or([Self::EMPTY; SlotRange::SLOT_RANGE_COUNT]);

		// If this bid beat the previous winner of our range.
		if current_winning[range_index].as_ref().map_or(true, |last| amount > last.2) {
			// Ok; we are the new winner of this range - reserve the additional amount and record.

			// Get the amount already held on deposit if this is a renewal bid (i.e. there's
			// an existing lease on the same para by the same leaser).
			let existing_lease_deposit = T::Leaser::deposit_held(para, &bidder);
			let reserve_required = amount.saturating_sub(existing_lease_deposit);

			// Get the amount already reserved in any prior and still active bids by us.
			let bidder_para = (bidder.clone(), para);
			let already_reserved = ReservedAmounts::<T>::get(&bidder_para).unwrap_or_default();

			// If these don't already cover the bid...
			if let Some(additional) = reserve_required.checked_sub(&already_reserved) {
				// ...then reserve some more funds from their account, failing if there's not
				// enough funds.
				CurrencyOf::<T>::reserve(&bidder, additional)?;
				// ...and record the amount reserved.
				ReservedAmounts::<T>::insert(&bidder_para, reserve_required);

				Self::deposit_event(Event::<T>::Reserved {
					bidder: bidder.clone(),
					extra_reserved: additional,
					total_amount: reserve_required,
				});
			}

			// Return any funds reserved for the previous winner if we are not in the ending period
			// and they no longer have any active bids.
			let mut outgoing_winner = Some((bidder.clone(), para, amount));
			swap(&mut current_winning[range_index], &mut outgoing_winner);
			if let Some((who, para, _amount)) = outgoing_winner {
				if auction_status.is_starting() &&
					current_winning
						.iter()
						.filter_map(Option::as_ref)
						.all(|&(ref other, other_para, _)| other != &who || other_para != para)
				{
					// Previous bidder is no longer winning any ranges: unreserve their funds.
					if let Some(amount) = ReservedAmounts::<T>::take(&(who.clone(), para)) {
						// It really should be reserved; there's not much we can do here on fail.
						let err_amt = CurrencyOf::<T>::unreserve(&who, amount);
						debug_assert!(err_amt.is_zero());
						Self::deposit_event(Event::<T>::Unreserved { bidder: who, amount });
					}
				}
			}

			// Update the range winner.
			Winning::<T>::insert(offset, &current_winning);
			Self::deposit_event(Event::<T>::BidAccepted {
				bidder,
				para_id: para,
				amount,
				first_slot,
				last_slot,
			});
		}
		Ok(())
	}

	/// Some when the auction's end is known (with the end block number). None if it is unknown.
	/// If `Some` then the block number must be at most the previous block and at least the
	/// previous block minus `T::EndingPeriod::get()`.
	///
	/// This mutates the state, cleaning up `AuctionInfo` and `Winning` in the case of an auction
	/// ending. An immediately subsequent call with the same argument will always return `None`.
	fn check_auction_end(now: BlockNumberFor<T>) -> Option<(WinningData<T>, LeasePeriodOf<T>)> {
		if let Some((lease_period_index, early_end)) = AuctionInfo::<T>::get() {
			let ending_period = T::EndingPeriod::get();
			let late_end = early_end.saturating_add(ending_period);
			let is_ended = now >= late_end;
			if is_ended {
				// auction definitely ended.
				// check to see if we can determine the actual ending point.
				let (raw_offset, known_since) = T::Randomness::random(&b"para_auction"[..]);

				if late_end <= known_since {
					// Our random seed was known only after the auction ended. Good to use.
					let raw_offset_block_number = <BlockNumberFor<T>>::decode(
						&mut raw_offset.as_ref(),
					)
					.expect("secure hashes should always be bigger than the block number; qed");
					let offset = (raw_offset_block_number % ending_period) /
						T::SampleLength::get().max(One::one());

					let auction_counter = AuctionCounter::<T>::get();
					Self::deposit_event(Event::<T>::WinningOffset {
						auction_index: auction_counter,
						block_number: offset,
					});
					let res = Winning::<T>::get(offset)
						.unwrap_or([Self::EMPTY; SlotRange::SLOT_RANGE_COUNT]);
					// This `remove_all` statement should remove at most `EndingPeriod` /
					// `SampleLength` items, which should be bounded and sensibly configured in the
					// runtime.
					#[allow(deprecated)]
					Winning::<T>::remove_all(None);
					AuctionInfo::<T>::kill();
					return Some((res, lease_period_index))
				}
			}
		}
		None
	}

	/// Auction just ended. We have the current lease period, the auction's lease period (which
	/// is guaranteed to be at least the current period) and the bidders that were winning each
	/// range at the time of the auction's close.
	fn manage_auction_end(
		auction_lease_period_index: LeasePeriodOf<T>,
		winning_ranges: WinningData<T>,
	) {
		// First, unreserve all amounts that were reserved for the bids. We will later re-reserve
		// the amounts from the bidders that ended up being assigned the slot so there's no need to
		// special-case them here.
		for ((bidder, _), amount) in ReservedAmounts::<T>::drain() {
			CurrencyOf::<T>::unreserve(&bidder, amount);
		}

		// Next, calculate the winning combination of slots and thus the final winners of the
		// auction.
		let winners = Self::calculate_winners(winning_ranges);

		// Go through those winners and re-reserve their bid, updating our table of deposits
		// accordingly.
		for (leaser, para, amount, range) in winners.into_iter() {
			let begin_offset = LeasePeriodOf::<T>::from(range.as_pair().0 as u32);
			let period_begin = auction_lease_period_index + begin_offset;
			let period_count = LeasePeriodOf::<T>::from(range.len() as u32);

			match T::Leaser::lease_out(para, &leaser, amount, period_begin, period_count) {
				Err(LeaseError::ReserveFailed) |
				Err(LeaseError::AlreadyEnded) |
				Err(LeaseError::NoLeasePeriod) => {
					// Should never happen since we just unreserved this amount (and our offset is
					// from the present period). But if it does, there's not much we can do.
				},
				Err(LeaseError::AlreadyLeased) => {
					// The leaser attempted to get a second lease on the same para ID, possibly
					// griefing us. Let's keep the amount reserved and let governance sort it out.
					if CurrencyOf::<T>::reserve(&leaser, amount).is_ok() {
						Self::deposit_event(Event::<T>::ReserveConfiscated {
							para_id: para,
							leaser,
							amount,
						});
					}
				},
				Ok(()) => {}, // Nothing to report.
			}
		}

		Self::deposit_event(Event::<T>::AuctionClosed {
			auction_index: AuctionCounter::<T>::get(),
		});
	}

	/// Calculate the final winners from the winning slots.
	///
	/// This is a simple dynamic programming algorithm designed by Al, the original code is at:
	/// `https://github.com/w3f/consensus/blob/master/NPoS/auctiondynamicthing.py`
	fn calculate_winners(mut winning: WinningData<T>) -> WinnersData<T> {
		let winning_ranges = {
			let mut best_winners_ending_at: [(Vec<SlotRange>, BalanceOf<T>);
				SlotRange::LEASE_PERIODS_PER_SLOT] = Default::default();
			let best_bid = |range: SlotRange| {
				winning[range as u8 as usize]
					.as_ref()
					.map(|(_, _, amount)| *amount * (range.len() as u32).into())
			};
			for i in 0..SlotRange::LEASE_PERIODS_PER_SLOT {
				let r = SlotRange::new_bounded(0, 0, i as u32).expect("`i < LPPS`; qed");
				if let Some(bid) = best_bid(r) {
					best_winners_ending_at[i] = (vec![r], bid);
				}
				for j in 0..i {
					let r = SlotRange::new_bounded(0, j as u32 + 1, i as u32)
						.expect("`i < LPPS`; `j < i`; `j + 1 < LPPS`; qed");
					if let Some(mut bid) = best_bid(r) {
						bid += best_winners_ending_at[j].1;
						if bid > best_winners_ending_at[i].1 {
							let mut new_winners = best_winners_ending_at[j].0.clone();
							new_winners.push(r);
							best_winners_ending_at[i] = (new_winners, bid);
						}
					} else {
						if best_winners_ending_at[j].1 > best_winners_ending_at[i].1 {
							best_winners_ending_at[i] = best_winners_ending_at[j].clone();
						}
					}
				}
			}
			best_winners_ending_at[SlotRange::LEASE_PERIODS_PER_SLOT - 1].0.clone()
		};

		winning_ranges
			.into_iter()
			.filter_map(|range| {
				winning[range as u8 as usize]
					.take()
					.map(|(bidder, para, amount)| (bidder, para, amount, range))
			})
			.collect::<Vec<_>>()
	}
}

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
