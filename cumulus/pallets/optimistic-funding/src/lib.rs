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

//! # Optimistic Funding Pallet
//!
//! A pallet that implements an optimistic funding mechanism for the Polkadot Ambassador Fellowship.
//! This allows members to submit funding requests and vote on them, with funds being allocated
//! based on the votes received.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
pub mod types;
pub mod constants;
pub use constants::*;

pub mod weights;

pub struct Instance1;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use crate::weights::WeightInfo;
use frame_support::{
	pallet_prelude::*,
	traits::{Currency, ExistenceRequirement, ReservableCurrency},
	PalletId,
};
use frame_system::pallet_prelude::{BlockNumberFor, OriginFor, ensure_signed};
use sp_runtime::{
	traits::{AccountIdConversion, Hash, Zero},
	Saturating,
};
use sp_std::prelude::*;

pub use types::{FundingRequest, Vote, VoteStatus};

type BalanceOf<T, I = ()> = <<T as Config<I>>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

pub type FundingRequestOf<T, I = ()> = FundingRequest<
    <T as frame_system::Config>::AccountId,
    BalanceOf<T, I>,
    BlockNumberFor<T>,
    BoundedVec<u8, ConstU32<100>>
>;

/// Get the rank of an account.
pub trait GetRank<AccountId> {
	/// Returns the rank of the given account, if it has one.
	fn get_rank(who: &AccountId) -> Option<u16>;
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self, I>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The currency trait.
		type Currency: ReservableCurrency<Self::AccountId>;

		/// The period for which a funding request is active.
		#[pallet::constant]
		type FundingPeriod: Get<BlockNumberFor<Self>>;

		/// The minimum amount that can be requested.
		#[pallet::constant]
		type MinimumRequestAmount: Get<BalanceOf<Self, I>>;

		/// The maximum amount that can be requested.
		#[pallet::constant]
		type MaximumRequestAmount: Get<BalanceOf<Self, I>>;

		/// The deposit required to submit a funding request.
		#[pallet::constant]
		type RequestDeposit: Get<BalanceOf<Self, I>>;

		/// The maximum number of active requests.
		#[pallet::constant]
		type MaxActiveRequests: Get<u32>;

		/// The origin that can access the treasury.
		type TreasuryOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// The pallet ID, used for deriving its sovereign account ID.
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// Member ranks are used to weight votes based on the voter's rank.
		type RankedMembers: GetRank<Self::AccountId>;
	}

	#[pallet::storage]
	pub type FundingRequests<T, I = ()> where T: Config<I> = StorageMap<
		_,
		Blake2_128Concat,
		T::Hash,
		FundingRequestOf<T, I>,
	>;

	#[pallet::storage]
	pub type Votes<T, I = ()> where T: Config<I> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::Hash,
		Blake2_128Concat,
		T::AccountId,
		Vote<BalanceOf<T, I>>,
	>;

	#[pallet::storage]
	pub type TreasuryBalance<T, I = ()> where T: Config<I> = StorageValue<_, BalanceOf<T, I>, ValueQuery>;

	#[pallet::storage]
	pub type ActiveRequestCount<T, I = ()> where T: Config<I> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	pub type CurrentPeriodEnd<T, I = ()> where T: Config<I> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T, I = ()>
	where
		T: Config<I>,
		I: 'static,
	{
		RequestSubmitted { request_hash: T::Hash, proposer: T::AccountId, amount: BalanceOf<T, I> },
		VoteCast { request_hash: T::Hash, voter: T::AccountId, amount: BalanceOf<T, I> },
		VoteCancelled { request_hash: T::Hash, voter: T::AccountId },
		FundsAllocated { request_hash: T::Hash, recipient: T::AccountId, amount: BalanceOf<T, I> },
		TreasuryTopUp { amount: BalanceOf<T, I> },
		RequestRejected { request_hash: T::Hash },
		PeriodEnded { period_end: BlockNumberFor<T> },
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The funding request does not exist.
		RequestDoesNotExist,
		/// The funding request amount is too small.
		RequestAmountTooSmall,
		/// The funding request amount is too large.
		RequestAmountTooLarge,
		/// The treasury does not have enough funds.
		InsufficientTreasuryFunds,
		/// The voter does not have enough funds.
		InsufficientVoterFunds,
		/// The voter has already voted on this request.
		AlreadyVoted,
		/// The vote does not exist.
		VoteDoesNotExist,
		/// The vote has already been cancelled.
		VoteAlreadyCancelled,
		/// The maximum number of active requests has been reached.
		TooManyActiveRequests,
		/// The funding period has ended.
		FundingPeriodEnded,
		/// The funding period has not been set.
		FundingPeriodNotSet,
		/// The voter does not have a sufficient rank to vote.
		InsufficientRank,
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I>
	{
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			let period_end = <CurrentPeriodEnd<T, I>>::get();
			if !period_end.is_zero() && n >= period_end {
				// Set the new period end
				let new_period_end = n.saturating_add(T::FundingPeriod::get());
				<CurrentPeriodEnd<T, I>>::put(new_period_end);

				Self::deposit_event(Event::<T, I>::PeriodEnded { period_end: new_period_end });

				T::WeightInfo::on_initialize_end_period()
			} else {
				T::WeightInfo::on_initialize_no_op()
			}
		}
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I>
	where
		T: Config<I>,
	{
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::submit_request())]
		pub fn submit_request(
			origin: OriginFor<T>,
			amount: BalanceOf<T, I>,
			description: BoundedVec<u8, ConstU32<100>>,
		) -> DispatchResult {
			let proposer = ensure_signed(origin)?;

			// Check if the funding period has been set
			let period_end = <CurrentPeriodEnd<T, I>>::get();
			ensure!(!period_end.is_zero(), Error::<T, I>::FundingPeriodNotSet);
			ensure!(
				frame_system::Pallet::<T>::block_number() < period_end,
				Error::<T, I>::FundingPeriodEnded
			);

			// Check if the request amount is within the allowed range
			ensure!(
				amount >= T::MinimumRequestAmount::get(),
				Error::<T, I>::RequestAmountTooSmall
			);
			ensure!(
				amount <= T::MaximumRequestAmount::get(),
				Error::<T, I>::RequestAmountTooLarge
			);

			// Check if we have reached the maximum number of active requests
			let active_requests = <ActiveRequestCount<T, I>>::get();
			ensure!(
				active_requests < T::MaxActiveRequests::get(),
				Error::<T, I>::TooManyActiveRequests
			);

			// Reserve the deposit
			let deposit = T::RequestDeposit::get();
			T::Currency::reserve(&proposer, deposit)?;

			// Create the funding request
			let request = FundingRequestOf::<T, I> {
				proposer: proposer.clone(),
				amount,
				description,
				submitted_at: frame_system::Pallet::<T>::block_number(),
				period_end,
				votes_count: 0,
				votes_amount: Zero::zero(),
			};

			// Calculate the hash of the request
			let request_hash = T::Hashing::hash_of(&request);

			// Store the request
			<FundingRequests<T, I>>::insert(request_hash, request);
			<ActiveRequestCount<T, I>>::mutate(|count| *count += 1);

			// Emit an event
			Self::deposit_event(Event::<T, I>::RequestSubmitted {
				request_hash,
				proposer,
				amount,
			});

			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::vote())]
		pub fn vote(
			origin: OriginFor<T>,
			request_hash: T::Hash,
			amount: BalanceOf<T, I>,
		) -> DispatchResult {
			let voter = ensure_signed(origin)?;

			// Check if the request exists
			let mut request =
				<FundingRequests<T, I>>::get(request_hash).ok_or(Error::<T, I>::RequestDoesNotExist)?;

			// Check if the funding period has ended
			ensure!(
				frame_system::Pallet::<T>::block_number() < request.period_end,
				Error::<T, I>::FundingPeriodEnded
			);

			// Check if the voter has already voted
			ensure!(
				!<Votes<T, I>>::contains_key(request_hash, &voter),
				Error::<T, I>::AlreadyVoted
			);

			// Reserve the vote amount
			T::Currency::reserve(&voter, amount)?;

			// Get the voter's rank (default to 0 if not a ranked member)
			let rank = T::RankedMembers::get_rank(&voter).unwrap_or(0);

			// Calculate vote weight based on rank according to the following table:
			// Rank 0 (Advocate Ambassador): Not eligible for voting
			// Rank 1 (Associate Ambassador): weight = 1
			// Rank 2 (Lead Ambassador): weight = 3
			// Rank 3 (Senior Ambassador): weight = 9
			// Rank 4 (Principal Ambassador): weight = 27
			// Rank 5 (Global Ambassador): weight = 81
			// Rank 6 (Global Head Ambassador): weight = 243
			let weighted_amount = if rank == 0 {
				// Rank 0 members cannot vote
				return Err(Error::<T, I>::InsufficientRank.into());
			} else {
				// Calculate 3^(rank-1) for ranks 1-6
				let weight = 3_u32.saturating_pow(rank.saturating_sub(1) as u32);
				amount.saturating_mul(BalanceOf::<T, I>::from(weight))
			};

			// Create vote
			let vote = Vote { amount, status: VoteStatus::Active };

			// Store vote
			<Votes<T, I>>::insert(request_hash, &voter, vote);

			// Update request
			request.votes_count = request.votes_count.saturating_add(1);
			request.votes_amount = request.votes_amount.saturating_add(weighted_amount);
			<FundingRequests<T, I>>::insert(request_hash, request);

			// Emit event
			Self::deposit_event(Event::<T, I>::VoteCast {
				request_hash,
				voter,
				amount,
			});

			Ok(())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::cancel_vote())]
		pub fn cancel_vote(origin: OriginFor<T>, request_hash: T::Hash) -> DispatchResult {
			let voter = ensure_signed(origin)?;

			// Check if request exists
			let mut request =
				<FundingRequests<T, I>>::get(request_hash).ok_or(Error::<T, I>::RequestDoesNotExist)?;

			// Check if vote exists
			let vote =
				<Votes<T, I>>::get(request_hash, &voter).ok_or(Error::<T, I>::VoteDoesNotExist)?;

			// Check if vote has already been cancelled
			ensure!(vote.status == VoteStatus::Active, Error::<T, I>::VoteAlreadyCancelled);

			// Get voter's rank (default to 0 if not a ranked member)
			let rank = T::RankedMembers::get_rank(&voter).unwrap_or(0);

			// Calculate original weighted amount that was added
			// For ranks 1-6, the weight is 3^(rank-1)
			// Rank 0 shouldn't have been able to vote, but we handle it defensively
			let weighted_amount = if rank == 0 {
				// This shouldn't happen as rank 0 members can't vote, but handle it defensively
				vote.amount
			} else {
				let weight = 3_u32.saturating_pow(rank.saturating_sub(1) as u32);
				vote.amount.saturating_mul(BalanceOf::<T, I>::from(weight))
			};

			// Update vote status
			let mut updated_vote = vote.clone();
			updated_vote.status = VoteStatus::Cancelled;
			<Votes<T, I>>::insert(request_hash, &voter, updated_vote);

			// Unreserve vote amount
			T::Currency::unreserve(&voter, vote.amount);

			// Update request's vote count and amount
			request.votes_count = request.votes_count.saturating_sub(1);
			request.votes_amount = request.votes_amount.saturating_sub(weighted_amount);
			<FundingRequests<T, I>>::insert(request_hash, request);

			// Emit event
			Self::deposit_event(Event::<T, I>::VoteCancelled { request_hash, voter });

			Ok(())
		}

		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::top_up_treasury())]
		pub fn top_up_treasury(
			origin: OriginFor<T>,
			amount: BalanceOf<T, I>,
		) -> DispatchResultWithPostInfo {
			T::TreasuryOrigin::ensure_origin(origin)?;

			// Get treasury account
			let treasury_account = Self::treasury_account();

			// Transfer funds to the treasury account
			T::Currency::transfer(
				&T::PalletId::get().into_account_truncating(),
				&treasury_account,
				amount,
				ExistenceRequirement::KeepAlive,
			)?;

			// Update treasury balance
			<TreasuryBalance<T, I>>::mutate(|balance| *balance = balance.saturating_add(amount));

			// Emit an event
			Self::deposit_event(Event::<T, I>::TreasuryTopUp { amount });

			Ok(().into())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::reject_request())]
		pub fn reject_request(origin: OriginFor<T>, request_hash: T::Hash) -> DispatchResult {
			T::TreasuryOrigin::ensure_origin(origin)?;

			// Check if request exists
			let request =
				<FundingRequests<T, I>>::get(request_hash).ok_or(Error::<T, I>::RequestDoesNotExist)?;

			// Remove request
			<FundingRequests<T, I>>::remove(request_hash);
			<ActiveRequestCount<T, I>>::mutate(|count| *count = count.saturating_sub(1));

			// Unreserve deposit
			T::Currency::unreserve(&request.proposer, T::RequestDeposit::get());

			// Emit event
			Self::deposit_event(Event::<T, I>::RequestRejected { request_hash });

			Ok(())
		}

		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::allocate_funds())]
		pub fn allocate_funds(origin: OriginFor<T>, request_hash: T::Hash) -> DispatchResult {
			T::TreasuryOrigin::ensure_origin(origin)?;

			// Check if request exists
			let request =
				<FundingRequests<T, I>>::get(request_hash).ok_or(Error::<T, I>::RequestDoesNotExist)?;

			// Check if treasury has enough funds
			let treasury_balance = <TreasuryBalance<T, I>>::get();
			ensure!(
				treasury_balance >= request.amount,
				Error::<T, I>::InsufficientTreasuryFunds
			);

			// Get treasury account
			let treasury_account = Self::treasury_account();

			// Transfer funds from treasury account to recipient
			T::Currency::transfer(
				&treasury_account,
				&request.proposer,
				request.amount,
				ExistenceRequirement::KeepAlive,
			)?;

			// Update treasury balance
			<TreasuryBalance<T, I>>::mutate(|balance| *balance = balance.saturating_sub(request.amount));

			// Remove request
			<FundingRequests<T, I>>::remove(request_hash);
			<ActiveRequestCount<T, I>>::mutate(|count| *count = count.saturating_sub(1));

			// Unreserve deposit
			T::Currency::unreserve(&request.proposer, T::RequestDeposit::get());

			// Emit event
			Self::deposit_event(Event::<T, I>::FundsAllocated {
				request_hash,
				recipient: request.proposer,
				amount: request.amount,
			});

			Ok(())
		}

		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::set_period_end())]
		pub fn set_period_end(
			origin: OriginFor<T>,
			period_end: BlockNumberFor<T>,
		) -> DispatchResult {
			T::TreasuryOrigin::ensure_origin(origin)?;

			// Set period end
			<CurrentPeriodEnd<T, I>>::put(period_end);

			// Emit event
			Self::deposit_event(Event::<T, I>::PeriodEnded { period_end });

			Ok(())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I>
where
	T: Config<I>,
{
	/// Get treasury account.
	pub fn treasury_account() -> T::AccountId {
		T::PalletId::get().into_account_truncating()
	}

	/// Get treasury balance.
	pub fn treasury_balance() -> BalanceOf<T, I> {
		<TreasuryBalance<T, I>>::get()
	}

	/// Get active request count.
	pub fn active_request_count() -> u32 {
		<ActiveRequestCount<T, I>>::get()
	}

	/// Get current period end.
	pub fn current_period_end() -> BlockNumberFor<T> {
		<CurrentPeriodEnd<T, I>>::get()
	}
}
