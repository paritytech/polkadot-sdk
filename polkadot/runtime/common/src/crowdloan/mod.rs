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

//! # Parachain `Crowdloaning` pallet
//!
//! The point of this pallet is to allow parachain projects to offer the ability to help fund a
//! deposit for the parachain. When the crowdloan has ended, the funds are returned.
//!
//! Each fund has a child-trie which stores all contributors account IDs together with the amount
//! they contributed; the root of this can then be used by the parachain to allow contributors to
//! prove that they made some particular contribution to the project (e.g. to be rewarded through
//! some token or badge). The trie is retained for later (efficient) redistribution back to the
//! contributors.
//!
//! Contributions must be of at least `MinContribution` (to account for the resources taken in
//! tracking contributions), and may never tally greater than the fund's `cap`, set and fixed at the
//! time of creation. The `create` call may be used to create a new fund. In order to do this, then
//! a deposit must be paid of the amount `SubmissionDeposit`. Substantial resources are taken on
//! the main trie in tracking a fund and this accounts for that.
//!
//! Funds may be set up during an auction period; their closing time is fixed at creation (as a
//! block number) and if the fund is not successful by the closing time, then it can be dissolved.
//! Funds may span multiple auctions, and even auctions that sell differing periods. However, for a
//! fund to be active in bidding for an auction, it *must* have had *at least one bid* since the end
//! of the last auction. Until a fund takes a further bid following the end of an auction, then it
//! will be inactive.
//!
//! Contributors will get a refund of their contributions from completed funds before the crowdloan
//! can be dissolved.
//!
//! Funds may accept contributions at any point before their success or end. When a parachain
//! slot auction enters its ending period, then parachains will each place a bid; the bid will be
//! raised once per block if the parachain had additional funds contributed since the last bid.
//!
//! Successful funds remain tracked (in the `Funds` storage item and the associated child trie) as
//! long as the parachain remains active. Users can withdraw their funds once the slot is completed
//! and funds are returned to the crowdloan account.

pub mod migration;

use crate::{
	slot_range::SlotRange,
	traits::{Auctioneer, Registrar},
};
use alloc::{vec, vec::Vec};
use codec::{Decode, Encode};
use frame_support::{
	ensure,
	pallet_prelude::{DispatchResult, Weight},
	storage::{child, ChildTriePrefixIterator},
	traits::{
		Currency, Defensive,
		ExistenceRequirement::{self, AllowDeath, KeepAlive},
		Get, ReservableCurrency,
	},
	Identity, PalletId,
};
use frame_system::pallet_prelude::BlockNumberFor;
pub use pallet::*;
use polkadot_primitives::Id as ParaId;
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{
		AccountIdConversion, CheckedAdd, Hash, IdentifyAccount, One, Saturating, Verify, Zero,
	},
	MultiSignature, MultiSigner, RuntimeDebug,
};

type CurrencyOf<T> = <<T as Config>::Auctioneer as Auctioneer<BlockNumberFor<T>>>::Currency;
type LeasePeriodOf<T> = <<T as Config>::Auctioneer as Auctioneer<BlockNumberFor<T>>>::LeasePeriod;
type BalanceOf<T> = <CurrencyOf<T> as Currency<<T as frame_system::Config>::AccountId>>::Balance;

type FundIndex = u32;

pub trait WeightInfo {
	fn create() -> Weight;
	fn contribute() -> Weight;
	fn withdraw() -> Weight;
	fn refund(k: u32) -> Weight;
	fn dissolve() -> Weight;
	fn edit() -> Weight;
	fn add_memo() -> Weight;
	fn on_initialize(n: u32) -> Weight;
	fn poke() -> Weight;
}

pub struct TestWeightInfo;
impl WeightInfo for TestWeightInfo {
	fn create() -> Weight {
		Weight::zero()
	}
	fn contribute() -> Weight {
		Weight::zero()
	}
	fn withdraw() -> Weight {
		Weight::zero()
	}
	fn refund(_k: u32) -> Weight {
		Weight::zero()
	}
	fn dissolve() -> Weight {
		Weight::zero()
	}
	fn edit() -> Weight {
		Weight::zero()
	}
	fn add_memo() -> Weight {
		Weight::zero()
	}
	fn on_initialize(_n: u32) -> Weight {
		Weight::zero()
	}
	fn poke() -> Weight {
		Weight::zero()
	}
}

#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub enum LastContribution<BlockNumber> {
	Never,
	PreEnding(u32),
	Ending(BlockNumber),
}

/// Information on a funding effort for a pre-existing parachain. We assume that the parachain ID
/// is known as it's used for the key of the storage item for which this is the value (`Funds`).
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
#[codec(dumb_trait_bound)]
pub struct FundInfo<AccountId, Balance, BlockNumber, LeasePeriod> {
	/// The owning account who placed the deposit.
	pub depositor: AccountId,
	/// An optional verifier. If exists, contributions must be signed by verifier.
	pub verifier: Option<MultiSigner>,
	/// The amount of deposit placed.
	pub deposit: Balance,
	/// The total amount raised.
	pub raised: Balance,
	/// Block number after which the funding must have succeeded. If not successful at this number
	/// then everyone may withdraw their funds.
	pub end: BlockNumber,
	/// A hard-cap on the amount that may be contributed.
	pub cap: Balance,
	/// The most recent block that this had a contribution. Determines if we make a bid or not.
	/// If this is `Never`, this fund has never received a contribution.
	/// If this is `PreEnding(n)`, this fund received a contribution sometime in auction
	/// number `n` before the ending period.
	/// If this is `Ending(n)`, this fund received a contribution during the current ending period,
	/// where `n` is how far into the ending period the contribution was made.
	pub last_contribution: LastContribution<BlockNumber>,
	/// First lease period in range to bid on; it's actually a `LeasePeriod`, but that's the same
	/// type as `BlockNumber`.
	pub first_period: LeasePeriod,
	/// Last lease period in range to bid on; it's actually a `LeasePeriod`, but that's the same
	/// type as `BlockNumber`.
	pub last_period: LeasePeriod,
	/// Unique index used to represent this fund.
	pub fund_index: FundIndex,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::{ensure_root, ensure_signed, pallet_prelude::*};

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// `PalletId` for the crowdloan pallet. An appropriate value could be
		/// `PalletId(*b"py/cfund")`
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// The amount to be held on deposit by the depositor of a crowdloan.
		type SubmissionDeposit: Get<BalanceOf<Self>>;

		/// The minimum amount that may be contributed into a crowdloan. Should almost certainly be
		/// at least `ExistentialDeposit`.
		#[pallet::constant]
		type MinContribution: Get<BalanceOf<Self>>;

		/// Max number of storage keys to remove per extrinsic call.
		#[pallet::constant]
		type RemoveKeysLimit: Get<u32>;

		/// The parachain registrar type. We just use this to ensure that only the manager of a para
		/// is able to start a crowdloan for its slot.
		type Registrar: Registrar<AccountId = Self::AccountId>;

		/// The type representing the auctioning system.
		type Auctioneer: Auctioneer<
			BlockNumberFor<Self>,
			AccountId = Self::AccountId,
			LeasePeriod = BlockNumberFor<Self>,
		>;

		/// The maximum length for the memo attached to a crowdloan contribution.
		type MaxMemoLength: Get<u8>;

		/// Weight Information for the Extrinsics in the Pallet
		type WeightInfo: WeightInfo;
	}

	/// Info on all of the funds.
	#[pallet::storage]
	pub type Funds<T: Config> = StorageMap<
		_,
		Twox64Concat,
		ParaId,
		FundInfo<T::AccountId, BalanceOf<T>, BlockNumberFor<T>, LeasePeriodOf<T>>,
	>;

	/// The funds that have had additional contributions during the last block. This is used
	/// in order to determine which funds should submit new or updated bids.
	#[pallet::storage]
	pub type NewRaise<T> = StorageValue<_, Vec<ParaId>, ValueQuery>;

	/// The number of auctions that have entered into their ending period so far.
	#[pallet::storage]
	pub type EndingsCount<T> = StorageValue<_, u32, ValueQuery>;

	/// Tracker for the next available fund index
	#[pallet::storage]
	pub type NextFundIndex<T> = StorageValue<_, u32, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Create a new crowdloaning campaign.
		Created { para_id: ParaId },
		/// Contributed to a crowd sale.
		Contributed { who: T::AccountId, fund_index: ParaId, amount: BalanceOf<T> },
		/// Withdrew full balance of a contributor.
		Withdrew { who: T::AccountId, fund_index: ParaId, amount: BalanceOf<T> },
		/// The loans in a fund have been partially dissolved, i.e. there are some left
		/// over child keys that still need to be killed.
		PartiallyRefunded { para_id: ParaId },
		/// All loans in a fund have been refunded.
		AllRefunded { para_id: ParaId },
		/// Fund is dissolved.
		Dissolved { para_id: ParaId },
		/// The result of trying to submit a new bid to the Slots pallet.
		HandleBidResult { para_id: ParaId, result: DispatchResult },
		/// The configuration to a crowdloan has been edited.
		Edited { para_id: ParaId },
		/// A memo has been updated.
		MemoUpdated { who: T::AccountId, para_id: ParaId, memo: Vec<u8> },
		/// A parachain has been moved to `NewRaise`
		AddedToNewRaise { para_id: ParaId },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The current lease period is more than the first lease period.
		FirstPeriodInPast,
		/// The first lease period needs to at least be less than 3 `max_value`.
		FirstPeriodTooFarInFuture,
		/// Last lease period must be greater than first lease period.
		LastPeriodBeforeFirstPeriod,
		/// The last lease period cannot be more than 3 periods after the first period.
		LastPeriodTooFarInFuture,
		/// The campaign ends before the current block number. The end must be in the future.
		CannotEndInPast,
		/// The end date for this crowdloan is not sensible.
		EndTooFarInFuture,
		/// There was an overflow.
		Overflow,
		/// The contribution was below the minimum, `MinContribution`.
		ContributionTooSmall,
		/// Invalid fund index.
		InvalidParaId,
		/// Contributions exceed maximum amount.
		CapExceeded,
		/// The contribution period has already ended.
		ContributionPeriodOver,
		/// The origin of this call is invalid.
		InvalidOrigin,
		/// This crowdloan does not correspond to a parachain.
		NotParachain,
		/// This parachain lease is still active and retirement cannot yet begin.
		LeaseActive,
		/// This parachain's bid or lease is still active and withdraw cannot yet begin.
		BidOrLeaseActive,
		/// The crowdloan has not yet ended.
		FundNotEnded,
		/// There are no contributions stored in this crowdloan.
		NoContributions,
		/// The crowdloan is not ready to dissolve. Potentially still has a slot or in retirement
		/// period.
		NotReadyToDissolve,
		/// Invalid signature.
		InvalidSignature,
		/// The provided memo is too large.
		MemoTooLarge,
		/// The fund is already in `NewRaise`
		AlreadyInNewRaise,
		/// No contributions allowed during the VRF delay
		VrfDelayInProgress,
		/// A lease period has not started yet, due to an offset in the starting block.
		NoLeasePeriod,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(num: BlockNumberFor<T>) -> frame_support::weights::Weight {
			if let Some((sample, sub_sample)) = T::Auctioneer::auction_status(num).is_ending() {
				// This is the very first block in the ending period
				if sample.is_zero() && sub_sample.is_zero() {
					// first block of ending period.
					EndingsCount::<T>::mutate(|c| *c += 1);
				}
				let new_raise = NewRaise::<T>::take();
				let new_raise_len = new_raise.len() as u32;
				for (fund, para_id) in
					new_raise.into_iter().filter_map(|i| Funds::<T>::get(i).map(|f| (f, i)))
				{
					// Care needs to be taken by the crowdloan creator that this function will
					// succeed given the crowdloaning configuration. We do some checks ahead of time
					// in crowdloan `create`.
					let result = T::Auctioneer::place_bid(
						Self::fund_account_id(fund.fund_index),
						para_id,
						fund.first_period,
						fund.last_period,
						fund.raised,
					);

					Self::deposit_event(Event::<T>::HandleBidResult { para_id, result });
				}
				T::WeightInfo::on_initialize(new_raise_len)
			} else {
				T::DbWeight::get().reads(1)
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a new crowdloaning campaign for a parachain slot with the given lease period
		/// range.
		///
		/// This applies a lock to your parachain configuration, ensuring that it cannot be changed
		/// by the parachain manager.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::create())]
		pub fn create(
			origin: OriginFor<T>,
			#[pallet::compact] index: ParaId,
			#[pallet::compact] cap: BalanceOf<T>,
			#[pallet::compact] first_period: LeasePeriodOf<T>,
			#[pallet::compact] last_period: LeasePeriodOf<T>,
			#[pallet::compact] end: BlockNumberFor<T>,
			verifier: Option<MultiSigner>,
		) -> DispatchResult {
			let depositor = ensure_signed(origin)?;
			let now = frame_system::Pallet::<T>::block_number();

			ensure!(first_period <= last_period, Error::<T>::LastPeriodBeforeFirstPeriod);
			let last_period_limit = first_period
				.checked_add(&((SlotRange::LEASE_PERIODS_PER_SLOT as u32) - 1).into())
				.ok_or(Error::<T>::FirstPeriodTooFarInFuture)?;
			ensure!(last_period <= last_period_limit, Error::<T>::LastPeriodTooFarInFuture);
			ensure!(end > now, Error::<T>::CannotEndInPast);

			// Here we check the lease period on the ending block is at most the first block of the
			// period after `first_period`. If it would be larger, there is no way we could win an
			// active auction, thus it would make no sense to have a crowdloan this long.
			let (lease_period_at_end, is_first_block) =
				T::Auctioneer::lease_period_index(end).ok_or(Error::<T>::NoLeasePeriod)?;
			let adjusted_lease_period_at_end = if is_first_block {
				lease_period_at_end.saturating_sub(One::one())
			} else {
				lease_period_at_end
			};
			ensure!(adjusted_lease_period_at_end <= first_period, Error::<T>::EndTooFarInFuture);

			// Can't start a crowdloan for a lease period that already passed.
			if let Some((current_lease_period, _)) = T::Auctioneer::lease_period_index(now) {
				ensure!(first_period >= current_lease_period, Error::<T>::FirstPeriodInPast);
			}

			// There should not be an existing fund.
			ensure!(!Funds::<T>::contains_key(index), Error::<T>::FundNotEnded);

			let manager = T::Registrar::manager_of(index).ok_or(Error::<T>::InvalidParaId)?;
			ensure!(depositor == manager, Error::<T>::InvalidOrigin);
			ensure!(T::Registrar::is_registered(index), Error::<T>::InvalidParaId);

			let fund_index = NextFundIndex::<T>::get();
			let new_fund_index = fund_index.checked_add(1).ok_or(Error::<T>::Overflow)?;

			let deposit = T::SubmissionDeposit::get();

			frame_system::Pallet::<T>::inc_providers(&Self::fund_account_id(fund_index));
			CurrencyOf::<T>::reserve(&depositor, deposit)?;

			Funds::<T>::insert(
				index,
				FundInfo {
					depositor,
					verifier,
					deposit,
					raised: Zero::zero(),
					end,
					cap,
					last_contribution: LastContribution::Never,
					first_period,
					last_period,
					fund_index,
				},
			);

			NextFundIndex::<T>::put(new_fund_index);

			Self::deposit_event(Event::<T>::Created { para_id: index });
			Ok(())
		}

		/// Contribute to a crowd sale. This will transfer some balance over to fund a parachain
		/// slot. It will be withdrawable when the crowdloan has ended and the funds are unused.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::contribute())]
		pub fn contribute(
			origin: OriginFor<T>,
			#[pallet::compact] index: ParaId,
			#[pallet::compact] value: BalanceOf<T>,
			signature: Option<MultiSignature>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Self::do_contribute(who, index, value, signature, KeepAlive)
		}

		/// Withdraw full balance of a specific contributor.
		///
		/// Origin must be signed, but can come from anyone.
		///
		/// The fund must be either in, or ready for, retirement. For a fund to be *in* retirement,
		/// then the retirement flag must be set. For a fund to be ready for retirement, then:
		/// - it must not already be in retirement;
		/// - the amount of raised funds must be bigger than the _free_ balance of the account;
		/// - and either:
		///   - the block number must be at least `end`; or
		///   - the current lease period must be greater than the fund's `last_period`.
		///
		/// In this case, the fund's retirement flag is set and its `end` is reset to the current
		/// block number.
		///
		/// - `who`: The account whose contribution should be withdrawn.
		/// - `index`: The parachain to whose crowdloan the contribution was made.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::withdraw())]
		pub fn withdraw(
			origin: OriginFor<T>,
			who: T::AccountId,
			#[pallet::compact] index: ParaId,
		) -> DispatchResult {
			ensure_signed(origin)?;

			let mut fund = Funds::<T>::get(index).ok_or(Error::<T>::InvalidParaId)?;
			let now = frame_system::Pallet::<T>::block_number();
			let fund_account = Self::fund_account_id(fund.fund_index);
			Self::ensure_crowdloan_ended(now, &fund_account, &fund)?;

			let (balance, _) = Self::contribution_get(fund.fund_index, &who);
			ensure!(balance > Zero::zero(), Error::<T>::NoContributions);

			CurrencyOf::<T>::transfer(&fund_account, &who, balance, AllowDeath)?;
			CurrencyOf::<T>::reactivate(balance);

			Self::contribution_kill(fund.fund_index, &who);
			fund.raised = fund.raised.saturating_sub(balance);

			Funds::<T>::insert(index, &fund);

			Self::deposit_event(Event::<T>::Withdrew { who, fund_index: index, amount: balance });
			Ok(())
		}

		/// Automatically refund contributors of an ended crowdloan.
		/// Due to weight restrictions, this function may need to be called multiple
		/// times to fully refund all users. We will refund `RemoveKeysLimit` users at a time.
		///
		/// Origin must be signed, but can come from anyone.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::refund(T::RemoveKeysLimit::get()))]
		pub fn refund(
			origin: OriginFor<T>,
			#[pallet::compact] index: ParaId,
		) -> DispatchResultWithPostInfo {
			ensure_signed(origin)?;

			let mut fund = Funds::<T>::get(index).ok_or(Error::<T>::InvalidParaId)?;
			let now = frame_system::Pallet::<T>::block_number();
			let fund_account = Self::fund_account_id(fund.fund_index);
			Self::ensure_crowdloan_ended(now, &fund_account, &fund)?;

			let mut refund_count = 0u32;
			// Try killing the crowdloan child trie
			let contributions = Self::contribution_iterator(fund.fund_index);
			// Assume everyone will be refunded.
			let mut all_refunded = true;
			for (who, (balance, _)) in contributions {
				if refund_count >= T::RemoveKeysLimit::get() {
					// Not everyone was able to be refunded this time around.
					all_refunded = false;
					break
				}
				CurrencyOf::<T>::transfer(&fund_account, &who, balance, AllowDeath)?;
				CurrencyOf::<T>::reactivate(balance);
				Self::contribution_kill(fund.fund_index, &who);
				fund.raised = fund.raised.saturating_sub(balance);
				refund_count += 1;
			}

			// Save the changes.
			Funds::<T>::insert(index, &fund);

			if all_refunded {
				Self::deposit_event(Event::<T>::AllRefunded { para_id: index });
				// Refund for unused refund count.
				Ok(Some(T::WeightInfo::refund(refund_count)).into())
			} else {
				Self::deposit_event(Event::<T>::PartiallyRefunded { para_id: index });
				// No weight to refund since we did not finish the loop.
				Ok(().into())
			}
		}

		/// Remove a fund after the retirement period has ended and all funds have been returned.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::dissolve())]
		pub fn dissolve(origin: OriginFor<T>, #[pallet::compact] index: ParaId) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let fund = Funds::<T>::get(index).ok_or(Error::<T>::InvalidParaId)?;
			let pot = Self::fund_account_id(fund.fund_index);
			let now = frame_system::Pallet::<T>::block_number();

			// Only allow dissolution when the raised funds goes to zero,
			// and the caller is the fund creator or we are past the end date.
			let permitted = who == fund.depositor || now >= fund.end;
			let can_dissolve = permitted && fund.raised.is_zero();
			ensure!(can_dissolve, Error::<T>::NotReadyToDissolve);

			// Assuming state is not corrupted, the child trie should already be cleaned up
			// and all funds in the crowdloan account have been returned. If not, governance
			// can take care of that.
			debug_assert!(Self::contribution_iterator(fund.fund_index).count().is_zero());

			// Crowdloan over, burn all funds.
			let _imba = CurrencyOf::<T>::make_free_balance_be(&pot, Zero::zero());
			let _ = frame_system::Pallet::<T>::dec_providers(&pot).defensive();

			CurrencyOf::<T>::unreserve(&fund.depositor, fund.deposit);
			Funds::<T>::remove(index);
			Self::deposit_event(Event::<T>::Dissolved { para_id: index });
			Ok(())
		}

		/// Edit the configuration for an in-progress crowdloan.
		///
		/// Can only be called by Root origin.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::edit())]
		pub fn edit(
			origin: OriginFor<T>,
			#[pallet::compact] index: ParaId,
			#[pallet::compact] cap: BalanceOf<T>,
			#[pallet::compact] first_period: LeasePeriodOf<T>,
			#[pallet::compact] last_period: LeasePeriodOf<T>,
			#[pallet::compact] end: BlockNumberFor<T>,
			verifier: Option<MultiSigner>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let fund = Funds::<T>::get(index).ok_or(Error::<T>::InvalidParaId)?;

			Funds::<T>::insert(
				index,
				FundInfo {
					depositor: fund.depositor,
					verifier,
					deposit: fund.deposit,
					raised: fund.raised,
					end,
					cap,
					last_contribution: fund.last_contribution,
					first_period,
					last_period,
					fund_index: fund.fund_index,
				},
			);

			Self::deposit_event(Event::<T>::Edited { para_id: index });
			Ok(())
		}

		/// Add an optional memo to an existing crowdloan contribution.
		///
		/// Origin must be Signed, and the user must have contributed to the crowdloan.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::add_memo())]
		pub fn add_memo(origin: OriginFor<T>, index: ParaId, memo: Vec<u8>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			ensure!(memo.len() <= T::MaxMemoLength::get().into(), Error::<T>::MemoTooLarge);
			let fund = Funds::<T>::get(index).ok_or(Error::<T>::InvalidParaId)?;

			let (balance, _) = Self::contribution_get(fund.fund_index, &who);
			ensure!(balance > Zero::zero(), Error::<T>::NoContributions);

			Self::contribution_put(fund.fund_index, &who, &balance, &memo);
			Self::deposit_event(Event::<T>::MemoUpdated { who, para_id: index, memo });
			Ok(())
		}

		/// Poke the fund into `NewRaise`
		///
		/// Origin must be Signed, and the fund has non-zero raise.
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::poke())]
		pub fn poke(origin: OriginFor<T>, index: ParaId) -> DispatchResult {
			ensure_signed(origin)?;
			let fund = Funds::<T>::get(index).ok_or(Error::<T>::InvalidParaId)?;
			ensure!(!fund.raised.is_zero(), Error::<T>::NoContributions);
			ensure!(!NewRaise::<T>::get().contains(&index), Error::<T>::AlreadyInNewRaise);
			NewRaise::<T>::append(index);
			Self::deposit_event(Event::<T>::AddedToNewRaise { para_id: index });
			Ok(())
		}

		/// Contribute your entire balance to a crowd sale. This will transfer the entire balance of
		/// a user over to fund a parachain slot. It will be withdrawable when the crowdloan has
		/// ended and the funds are unused.
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::contribute())]
		pub fn contribute_all(
			origin: OriginFor<T>,
			#[pallet::compact] index: ParaId,
			signature: Option<MultiSignature>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let value = CurrencyOf::<T>::free_balance(&who);
			Self::do_contribute(who, index, value, signature, AllowDeath)
		}
	}
}

impl<T: Config> Pallet<T> {
	/// The account ID of the fund pot.
	///
	/// This actually does computation. If you need to keep using it, then make sure you cache the
	/// value and only call this once.
	pub fn fund_account_id(index: FundIndex) -> T::AccountId {
		T::PalletId::get().into_sub_account_truncating(index)
	}

	pub fn id_from_index(index: FundIndex) -> child::ChildInfo {
		let mut buf = Vec::new();
		buf.extend_from_slice(b"crowdloan");
		buf.extend_from_slice(&index.encode()[..]);
		child::ChildInfo::new_default(T::Hashing::hash(&buf[..]).as_ref())
	}

	pub fn contribution_put(
		index: FundIndex,
		who: &T::AccountId,
		balance: &BalanceOf<T>,
		memo: &[u8],
	) {
		who.using_encoded(|b| child::put(&Self::id_from_index(index), b, &(balance, memo)));
	}

	pub fn contribution_get(index: FundIndex, who: &T::AccountId) -> (BalanceOf<T>, Vec<u8>) {
		who.using_encoded(|b| {
			child::get_or_default::<(BalanceOf<T>, Vec<u8>)>(&Self::id_from_index(index), b)
		})
	}

	pub fn contribution_kill(index: FundIndex, who: &T::AccountId) {
		who.using_encoded(|b| child::kill(&Self::id_from_index(index), b));
	}

	pub fn crowdloan_kill(index: FundIndex) -> child::KillStorageResult {
		#[allow(deprecated)]
		child::kill_storage(&Self::id_from_index(index), Some(T::RemoveKeysLimit::get()))
	}

	pub fn contribution_iterator(
		index: FundIndex,
	) -> ChildTriePrefixIterator<(T::AccountId, (BalanceOf<T>, Vec<u8>))> {
		ChildTriePrefixIterator::<_>::with_prefix_over_key::<Identity>(
			&Self::id_from_index(index),
			&[],
		)
	}

	/// This function checks all conditions which would qualify a crowdloan has ended.
	/// * If we have reached the `fund.end` block OR the first lease period the fund is trying to
	///   bid for has started already.
	/// * And, if the fund has enough free funds to refund full raised amount.
	fn ensure_crowdloan_ended(
		now: BlockNumberFor<T>,
		fund_account: &T::AccountId,
		fund: &FundInfo<T::AccountId, BalanceOf<T>, BlockNumberFor<T>, LeasePeriodOf<T>>,
	) -> sp_runtime::DispatchResult {
		// `fund.end` can represent the end of a failed crowdloan or the beginning of retirement
		// If the current lease period is past the first period they are trying to bid for, then
		// it is already too late to win the bid.
		let (current_lease_period, _) =
			T::Auctioneer::lease_period_index(now).ok_or(Error::<T>::NoLeasePeriod)?;
		ensure!(
			now >= fund.end || current_lease_period > fund.first_period,
			Error::<T>::FundNotEnded
		);
		// free balance must greater than or equal amount raised, otherwise funds are being used
		// and a bid or lease must be active.
		ensure!(
			CurrencyOf::<T>::free_balance(&fund_account) >= fund.raised,
			Error::<T>::BidOrLeaseActive
		);

		Ok(())
	}

	fn do_contribute(
		who: T::AccountId,
		index: ParaId,
		value: BalanceOf<T>,
		signature: Option<MultiSignature>,
		existence: ExistenceRequirement,
	) -> DispatchResult {
		ensure!(value >= T::MinContribution::get(), Error::<T>::ContributionTooSmall);
		let mut fund = Funds::<T>::get(index).ok_or(Error::<T>::InvalidParaId)?;
		fund.raised = fund.raised.checked_add(&value).ok_or(Error::<T>::Overflow)?;
		ensure!(fund.raised <= fund.cap, Error::<T>::CapExceeded);

		// Make sure crowdloan has not ended
		let now = frame_system::Pallet::<T>::block_number();
		ensure!(now < fund.end, Error::<T>::ContributionPeriodOver);

		// Make sure crowdloan is in a valid lease period
		let now = frame_system::Pallet::<T>::block_number();
		let (current_lease_period, _) =
			T::Auctioneer::lease_period_index(now).ok_or(Error::<T>::NoLeasePeriod)?;
		ensure!(current_lease_period <= fund.first_period, Error::<T>::ContributionPeriodOver);

		// Make sure crowdloan has not already won.
		let fund_account = Self::fund_account_id(fund.fund_index);
		ensure!(
			!T::Auctioneer::has_won_an_auction(index, &fund_account),
			Error::<T>::BidOrLeaseActive
		);

		// We disallow any crowdloan contributions during the VRF Period, so that people do not
		// sneak their contributions into the auction when it would not impact the outcome.
		ensure!(!T::Auctioneer::auction_status(now).is_vrf(), Error::<T>::VrfDelayInProgress);

		let (old_balance, memo) = Self::contribution_get(fund.fund_index, &who);

		if let Some(ref verifier) = fund.verifier {
			let signature = signature.ok_or(Error::<T>::InvalidSignature)?;
			let payload = (index, &who, old_balance, value);
			let valid = payload.using_encoded(|encoded| {
				signature.verify(encoded, &verifier.clone().into_account())
			});
			ensure!(valid, Error::<T>::InvalidSignature);
		}

		CurrencyOf::<T>::transfer(&who, &fund_account, value, existence)?;
		CurrencyOf::<T>::deactivate(value);

		let balance = old_balance.saturating_add(value);
		Self::contribution_put(fund.fund_index, &who, &balance, &memo);

		if T::Auctioneer::auction_status(now).is_ending().is_some() {
			match fund.last_contribution {
				// In ending period; must ensure that we are in NewRaise.
				LastContribution::Ending(n) if n == now => {
					// do nothing - already in NewRaise
				},
				_ => {
					NewRaise::<T>::append(index);
					fund.last_contribution = LastContribution::Ending(now);
				},
			}
		} else {
			let endings_count = EndingsCount::<T>::get();
			match fund.last_contribution {
				LastContribution::PreEnding(a) if a == endings_count => {
					// Not in ending period and no auctions have ended ending since our
					// previous bid which was also not in an ending period.
					// `NewRaise` will contain our ID still: Do nothing.
				},
				_ => {
					// Not in ending period; but an auction has been ending since our previous
					// bid, or we never had one to begin with. Add bid.
					NewRaise::<T>::append(index);
					fund.last_contribution = LastContribution::PreEnding(endings_count);
				},
			}
		}

		Funds::<T>::insert(index, &fund);

		Self::deposit_event(Event::<T>::Contributed { who, fund_index: index, amount: value });
		Ok(())
	}
}

impl<T: Config> crate::traits::OnSwap for Pallet<T> {
	fn on_swap(one: ParaId, other: ParaId) {
		Funds::<T>::mutate(one, |x| Funds::<T>::mutate(other, |y| core::mem::swap(x, y)))
	}
}

#[cfg(any(feature = "runtime-benchmarks", test))]
mod crypto;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
