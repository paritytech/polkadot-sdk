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

//! # Voting Pallet
//!
//! - [`Config`]
//! - [`Call`]
//!
//! ## Overview
//!
//! Pallet for managing actual voting in polls.

#![recursion_limit = "256"]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use frame_support::{
	dispatch::DispatchResult,
	ensure,
	traits::{
		fungible, Currency, Get, LockIdentifier, LockableCurrency, PollStatus, Polling,
		ReservableCurrency, WithdrawReasons,
	},
	BoundedVec,
};
use sp_runtime::{
	traits::{AtLeast32BitUnsigned, Saturating, StaticLookup, Zero},
	ArithmeticError, DispatchError, Perbill,
};

mod conviction;
pub mod migrations;
mod traits;
mod types;
mod vote;
pub mod weights;

pub use self::{
	conviction::Conviction,
	pallet::*,
	traits::{Status, VotingHooks},
	types::{Delegations, Tally, UnvoteScope},
	vote::{AccountVote, PriorLock, Vote, VoteRecord, Voting},
	weights::WeightInfo,
};
use sp_runtime::traits::BlockNumberProvider;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

const CONVICTION_VOTING_ID: LockIdentifier = *b"pyconvot";

pub type BlockNumberFor<T, I> =
	<<T as Config<I>>::BlockNumberProvider as BlockNumberProvider>::BlockNumber;

type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;
pub type BalanceOf<T, I = ()> =
	<<T as Config<I>>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
pub type VotingOf<T, I = ()> = Voting<
	BalanceOf<T, I>,
	<T as frame_system::Config>::AccountId,
	BlockNumberFor<T, I>,
	PollIndexOf<T, I>,
	<T as Config<I>>::MaxVotes,
>;
pub type TallyOf<T, I = ()> = Tally<BalanceOf<T, I>, <T as Config<I>>::MaxTurnout>;
pub type VotesOf<T, I = ()> = BalanceOf<T, I>;
pub type PollIndexOf<T, I = ()> = <<T as Config<I>>::Polls as Polling<TallyOf<T, I>>>::Index;
#[cfg(feature = "runtime-benchmarks")]
pub type IndexOf<T, I = ()> = <<T as Config<I>>::Polls as Polling<TallyOf<T, I>>>::Index;
pub type ClassOf<T, I = ()> = <<T as Config<I>>::Polls as Polling<TallyOf<T, I>>>::Class;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{
		pallet_prelude::{
			DispatchResultWithPostInfo, IsType, StorageDoubleMap, StorageMap, StorageValue,
			StorageVersion, ValueQuery,
		},
		traits::ClassCountOf,
		Twox64Concat,
	};
	use frame_system::pallet_prelude::{ensure_signed, OriginFor};
	use sp_runtime::BoundedVec;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config + Sized {
		// System level stuff.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
		/// Currency type with which voting happens.
		type Currency: ReservableCurrency<Self::AccountId>
			+ LockableCurrency<Self::AccountId, Moment = BlockNumberFor<Self, I>>
			+ fungible::Inspect<Self::AccountId>;

		/// The implementation of the logic which conducts polls.
		type Polls: Polling<
			TallyOf<Self, I>,
			Votes = BalanceOf<Self, I>,
			Moment = BlockNumberFor<Self, I>,
		>;

		/// The maximum amount of tokens which may be used for voting. May just be
		/// `Currency::total_issuance`, but you might want to reduce this in order to account for
		/// funds in the system which are unable to vote (e.g. parachain auction deposits).
		type MaxTurnout: Get<BalanceOf<Self, I>>;

		/// The maximum number of concurrent votes an account may have.
		///
		/// Also used to compute weight, an overly large value can lead to extrinsics with large
		/// weight estimation: see `delegate` for instance.
		#[pallet::constant]
		type MaxVotes: Get<u32>;

		/// The minimum period of vote locking.
		///
		/// It should be no shorter than enactment period to ensure that in the case of an approval,
		/// those successful voters are locked into the consequences that their votes entail.
		#[pallet::constant]
		type VoteLockingPeriod: Get<BlockNumberFor<Self, I>>;
		/// Provider for the block number. Normally this is the `frame_system` pallet.
		type BlockNumberProvider: BlockNumberProvider;
		/// Hooks are called when a new vote is registered or an existing vote is removed.
		///
		/// The trait does not expose weight information.
		/// The weight of each hook is assumed to be benchmarked as part of the function that calls
		/// it. Hooks should never recursively call into functions that called,
		/// directly or indirectly, the function that called them.
		/// This could lead to infinite recursion and stack overflow.
		/// Note that this also means to not call into other generic functionality like batch or
		/// similar. Also, anything that a hook did will be subject to the transactional semantics
		/// of the calling function. This means that if the calling function fails, the hook will
		/// be rolled back without further notice.
		type VotingHooks: VotingHooks<Self::AccountId, PollIndexOf<Self, I>, BalanceOf<Self, I>>;
	}

	/// All voting for a particular voter in a particular voting class. We store the balance for the
	/// number of votes that we have recorded.
	#[pallet::storage]
	pub type VotingFor<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		_,
		Twox64Concat,
		T::AccountId,
		Twox64Concat,
		ClassOf<T, I>,
		VotingOf<T, I>,
		ValueQuery,
	>;

	/// The voting classes which have a non-zero lock requirement and the lock amounts which they
	/// require. The actual amount locked on behalf of this pallet should always be the maximum of
	/// this list.
	#[pallet::storage]
	pub type ClassLocksFor<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		T::AccountId,
		BoundedVec<(ClassOf<T, I>, BalanceOf<T, I>), ClassCountOf<T::Polls, TallyOf<T, I>>>,
		ValueQuery,
	>;

	/// Is a pallet storage migration currently ongoing?
	#[pallet::storage]
	pub type MigrationOngoing<T: Config<I>, I: 'static = ()> = StorageValue<_, bool, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// An account has delegated their vote to another account. \[who, target\]
		Delegated(T::AccountId, T::AccountId, ClassOf<T, I>),
		/// An \[account\] has cancelled a previous delegation operation.
		Undelegated(T::AccountId, ClassOf<T, I>),
		/// An account has voted
		Voted {
			who: T::AccountId,
			vote: AccountVote<BalanceOf<T, I>>,
			poll_index: PollIndexOf<T, I>,
		},
		/// A vote has been removed
		VoteRemoved {
			who: T::AccountId,
			vote: AccountVote<BalanceOf<T, I>>,
			poll_index: PollIndexOf<T, I>,
		},
		/// The lockup period of a conviction vote expired, and the funds have been unlocked.
		VoteUnlocked { who: T::AccountId, class: ClassOf<T, I> },
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// Poll is not ongoing.
		NotOngoing,
		/// The given account did not vote on the poll.
		NotVoter,
		/// The actor has no permission to conduct the action.
		NoPermission,
		/// The actor has no permission to conduct the action right now but will do in the future.
		NoPermissionYet,
		/// The account is already delegating.
		AlreadyDelegating,
		/// The account currently has votes attached to it and the operation cannot succeed until
		/// these are removed through `remove_vote`.
		#[deprecated(
			note = "This error is no longer used. Delegating while voting is now permitted."
		)]
		AlreadyVoting,
		/// Too high a balance was provided that the account cannot afford.
		InsufficientFunds,
		/// The account is not currently delegating.
		NotDelegating,
		/// Delegation to oneself makes no sense.
		Nonsense,
		/// Maximum number of votes reached.
		MaxVotesReached,
		/// The class must be supplied since it is not easily determinable from the state.
		ClassNeeded,
		/// The class ID supplied is invalid.
		BadClass,
		/// The voter's delegate has reached the maximum number of votes.
		DelegateMaxVotesReached,
		/// The delegate does not allow for delegator voting.
		DelegatorVotingNotAllowed,
		/// A storage migration is currently ongoing.
		MigrationOngoing,
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Vote in a poll. If `vote.is_aye()`, the vote is to enact the proposal;
		/// otherwise it is a vote to keep the status quo.
		///
		/// The dispatch origin of this call must be _Signed_.
		///
		/// - `poll_index`: The index of the poll to vote for.
		/// - `vote`: The vote configuration.
		///
		/// Weight: `O(R)` where R is Max(voter's number of votes, their (possible) delegate's
		/// number of votes).
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::vote_new().max(T::WeightInfo::vote_existing()))]
		pub fn vote(
			origin: OriginFor<T>,
			#[pallet::compact] poll_index: PollIndexOf<T, I>,
			vote: AccountVote<BalanceOf<T, I>>,
		) -> DispatchResult {
			Self::ensure_migration_not_ongoing()?;
			let who = ensure_signed(origin)?;
			Self::try_vote(&who, poll_index, vote)
		}

		/// Delegate the voting power (with some given conviction) of the sending account for a
		/// particular class of polls.
		///
		/// The balance delegated is locked for as long as it's delegated, and thereafter for the
		/// time appropriate for the conviction's lock period.
		///
		/// The dispatch origin of this call must be _Signed_.
		///
		/// - `to`: The account whose voting the `target` account's voting power will follow.
		/// - `class`: The class of polls to delegate. To delegate multiple classes, multiple calls
		///   to this function are required.
		/// - `conviction`: The conviction that will be attached to the delegated votes. When the
		///   account is undelegated, the funds will be locked for the corresponding period.
		/// - `balance`: The amount of the account's balance to be used in delegating. This must not
		///   be more than the account's current balance.
		///
		/// Emits `Delegated`.
		///
		/// Weight: `O(R + S)` where R and S are the number of polls the delegate and delegator have
		///   voted on, respectively. Weight is initially charged as if maximum votes, but is
		/// refunded later.
		// NOTE: weight must cover an incorrect voting of origin with max votes, this is ensure
		// because a valid delegation cover decoding a direct voting with max votes.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::delegate(T::MaxVotes::get(), T::MaxVotes::get()))]
		pub fn delegate(
			origin: OriginFor<T>,
			class: ClassOf<T, I>,
			to: AccountIdLookupOf<T>,
			conviction: Conviction,
			balance: BalanceOf<T, I>,
		) -> DispatchResultWithPostInfo {
			Self::ensure_migration_not_ongoing()?;
			let who = ensure_signed(origin)?;
			let to = T::Lookup::lookup(to)?;
			let (delegate_votes, delegator_votes) =
				Self::try_delegate(who, class, to, conviction, balance)?;

			Ok(Some(T::WeightInfo::delegate(delegate_votes, delegator_votes)).into())
		}

		/// Undelegate the voting power of the sending account for a particular class of polls.
		///
		/// Tokens may be unlocked following once an amount of time consistent with the lock period
		/// of the conviction with which the delegation was issued has passed.
		///
		/// The dispatch origin of this call must be _Signed_ and the signing account must be
		/// currently delegating.
		///
		/// - `class`: The class of polls to remove the delegation from.
		///
		/// Emits `Undelegated`.
		///
		/// Weight: `O(R + S)` where R and S are the number of polls the delegate and delegator have
		///   voted on, respectively. Weight is initially charged as if maximum votes, but is
		/// refunded later.
		// NOTE: weight must cover an incorrect voting of origin with max votes, this is ensure
		// because a valid delegation cover decoding a direct voting with max votes.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::undelegate(T::MaxVotes::get(), T::MaxVotes::get()))]
		pub fn undelegate(
			origin: OriginFor<T>,
			class: ClassOf<T, I>,
		) -> DispatchResultWithPostInfo {
			Self::ensure_migration_not_ongoing()?;
			let who = ensure_signed(origin)?;
			let (delegate_votes, delegator_votes) = Self::try_undelegate(who, class)?;
			Ok(Some(T::WeightInfo::undelegate(delegate_votes, delegator_votes)).into())
		}

		/// Remove the lock caused by prior voting/delegating which has expired within a particular
		/// class.
		///
		/// The dispatch origin of this call must be _Signed_.
		///
		/// - `class`: The class of polls to unlock.
		/// - `target`: The account to remove the lock on.
		///
		/// Weight: `O(R)` where R is the number of votes of the target.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::unlock())]
		pub fn unlock(
			origin: OriginFor<T>,
			class: ClassOf<T, I>,
			target: AccountIdLookupOf<T>,
		) -> DispatchResult {
			Self::ensure_migration_not_ongoing()?;
			ensure_signed(origin)?;
			let target = T::Lookup::lookup(target)?;
			Self::update_lock(&class, &target);
			Self::deposit_event(Event::VoteUnlocked { who: target, class });
			Ok(())
		}

		/// Remove a vote for a poll.
		///
		/// If:
		/// - the poll was cancelled, or
		/// - the poll is ongoing, or
		/// - the poll has ended such that
		///   - the vote of the account was in opposition to the result; or
		///   - there was no conviction to the account's vote; or
		///   - the account made a split vote
		/// ...then the vote is removed cleanly and a following call to `unlock` may result in more
		/// funds being available.
		///
		/// If, however, the poll has ended and:
		/// - it finished corresponding to the vote of the account, and
		/// - the account made a standard vote with conviction, and
		/// - the lock period of the conviction is not over
		/// ...then the lock will be aggregated into the overall account's lock, which may involve
		/// *overlocking* (where the two locks are combined into a single lock that is the maximum
		/// of both the amount locked and the time is it locked for).
		///
		/// The dispatch origin of this call must be _Signed_, and the signer must have a vote
		/// registered for poll `index`.
		///
		/// - `index`: The index of poll of the vote to be removed.
		/// - `class`: Optional parameter, if given it indicates the class of the poll. For polls
		///   which have finished or are cancelled, this must be `Some`.
		///
		/// Weight: `O(R + log R)` where R is Max(targets's number of votes, their (possible)
		/// delegate's number of votes).   Weight is calculated for the maximum number of votes.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::remove_vote())]
		pub fn remove_vote(
			origin: OriginFor<T>,
			class: Option<ClassOf<T, I>>,
			index: PollIndexOf<T, I>,
		) -> DispatchResult {
			Self::ensure_migration_not_ongoing()?;
			let who = ensure_signed(origin)?;
			Self::try_remove_vote(&who, index, class, UnvoteScope::Any)
		}

		/// Remove a vote for a poll.
		///
		/// If the `target` is equal to the signer, then this function is exactly equivalent to
		/// `remove_vote`. If not equal to the signer, then the vote must have expired,
		/// either because the poll was cancelled, because the voter lost the poll or
		/// because the conviction period is over.
		///
		/// The dispatch origin of this call must be _Signed_.
		///
		/// - `target`: The account of the vote to be removed; this account must have voted for poll
		///   `index`.
		/// - `index`: The index of poll of the vote to be removed.
		/// - `class`: The class of the poll.
		///
		/// Weight: `O(R + log R)` where R is Max(targets's number of votes, their (possible)
		/// delegate's number of votes).   Weight is calculated for the maximum number of votes
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::remove_other_vote())]
		pub fn remove_other_vote(
			origin: OriginFor<T>,
			target: AccountIdLookupOf<T>,
			class: ClassOf<T, I>,
			index: PollIndexOf<T, I>,
		) -> DispatchResult {
			Self::ensure_migration_not_ongoing()?;
			let who = ensure_signed(origin)?;
			let target = T::Lookup::lookup(target)?;
			let scope = if target == who { UnvoteScope::Any } else { UnvoteScope::OnlyExpired };
			Self::try_remove_vote(&target, index, Some(class), scope)?;
			Ok(())
		}

		/// Toggle `allow_delegator_voting` for the class.
		///
		/// The dispatch origin of this call must be _Signed_.
		///
		/// - `class`: The class for which to toggle the functionality.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::toggle_allow_delegator_voting())]
		pub fn toggle_allow_delegator_voting(
			origin: OriginFor<T>,
			class: ClassOf<T, I>,
		) -> DispatchResult {
			Self::ensure_migration_not_ongoing()?;
			let who = ensure_signed(origin)?;
			Self::toggle_delegator_voting(who, class);
			Ok(())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// Actually enact a vote, if legitimate.
	fn try_vote(
		who: &T::AccountId,
		poll_index: PollIndexOf<T, I>,
		vote: AccountVote<BalanceOf<T, I>>,
	) -> DispatchResult {
		ensure!(
			vote.balance() <= T::Currency::total_balance(who),
			Error::<T, I>::InsufficientFunds
		);
		// Call on_vote hook.
		T::VotingHooks::on_before_vote(who, poll_index, vote)?;

		T::Polls::try_access_poll(poll_index, |poll_status| {
			let (tally, class) = poll_status.ensure_ongoing().ok_or(Error::<T, I>::NotOngoing)?;
			VotingFor::<T, I>::try_mutate(who, &class, |voting| {
				let votes = &mut voting.votes;
				let mut vote_introduced = false;
				// Search for the vote.
				let index = match votes.binary_search_by_key(&poll_index, |i| i.poll_index) {
					// If found.
					Ok(i) => {
						// And they currently have a vote.
						if let Some(old_vote) = votes[i].maybe_vote {
							// Reduce tally by the vote, shouldn't be possible to fail, but we
							// handle it gracefully.
							tally.remove(old_vote).ok_or(ArithmeticError::Underflow)?;
							// Remove delegations from tally only if vote was standard aye nay.
							if let Some(approve) = old_vote.as_standard() {
								// But first adjust by the current clawback amount.
								let final_delegations =
									voting.delegations.saturating_sub(votes[i].retracted_votes);
								tally.reduce(approve, final_delegations);
							}
						} else {
							vote_introduced = true;
						}

						// Set their vote.
						votes[i].maybe_vote = Some(vote);
						i
					},
					// If not found.
					Err(i) => {
						// Add vote data, unless max vote reached.
						let vote_record = VoteRecord {
							poll_index,
							maybe_vote: Some(vote),
							retracted_votes: Default::default(),
						};
						votes
							.try_insert(i, vote_record)
							.map_err(|_| Error::<T, I>::MaxVotesReached)?;
						vote_introduced = true;
						i
					},
				};

				// Now that pre-existing votes have been handled.
				// Update tally with new vote, shouldn't be possible to fail, but we handle it
				// gracefully.
				tally.add(vote).ok_or(ArithmeticError::Overflow)?;
				// If vote is standard, add delegations to tally.
				if let Some(approve) = vote.as_standard() {
					// But first adjust by current clawbacks.
					let final_delegations =
						voting.delegations.saturating_sub(votes[index].retracted_votes);
					tally.increase(approve, final_delegations);
				}

				// If delegating, update delegate's info.
				if let (Some(delegate), Some(conviction)) =
					(&voting.maybe_delegate, &voting.maybe_conviction)
				{
					// But only if delegator's vote went from None to Some, otherwise the vote
					// clawback data will already exist.
					if vote_introduced {
						let amount_delegated = conviction.votes(voting.delegated_balance);
						VotingFor::<T, I>::try_mutate(
							delegate,
							&class,
							|delegate_voting| -> Result<(), DispatchError> {
								if !delegate_voting.allow_delegator_voting {
									return Err(Error::<T, I>::DelegatorVotingNotAllowed.into());
								}

								let delegates_votes = &mut delegate_voting.votes;
								// Search for data about poll in delegate's voting info.
								match delegates_votes
									.binary_search_by_key(&poll_index, |i| i.poll_index)
								{
									// If found.
									Ok(i) => {
										// Update delegate's clawback amount for this poll.
										delegates_votes[i].retracted_votes = delegates_votes[i]
											.retracted_votes
											.saturating_add(amount_delegated);

										// And update tally if delegate has standard vote recorded.
										if let Some(delegates_vote) = delegates_votes[i].maybe_vote
										{
											if let Some(approve) = delegates_vote.as_standard() {
												// By delegated amount.
												tally.reduce(approve, amount_delegated);
											}
										}
										Ok(())
									},
									// If not found.
									Err(i) => {
										// Add empty vote and clawback amount.
										let vote_record = VoteRecord {
											poll_index,
											maybe_vote: None,
											retracted_votes: amount_delegated,
										};
										delegates_votes.try_insert(i, vote_record).map_err(|_| {
											Error::<T, I>::DelegateMaxVotesReached.into()
										})
									},
								}
							},
						)?;
					}
				}

				// Extend the lock to `balance` (rather than setting it) since we don't know what
				// other votes are in place.
				Self::extend_lock(who, &class, vote.balance());
				Self::deposit_event(Event::Voted { who: who.clone(), vote, poll_index });
				Ok(())
			})
		})
	}

	/// Remove the account's vote for the given poll if possible. This is possible when:
	/// - The poll has not finished.
	/// - The poll has finished and the voter lost their direction.
	/// - The poll has finished and the voter's lock period is up.
	///
	/// This will generally be combined with a call to `unlock`.
	fn try_remove_vote(
		who: &T::AccountId,
		poll_index: PollIndexOf<T, I>,
		class_hint: Option<ClassOf<T, I>>,
		scope: UnvoteScope,
	) -> DispatchResult {
		let class = class_hint
			.or_else(|| Some(T::Polls::as_ongoing(poll_index)?.1))
			.ok_or(Error::<T, I>::ClassNeeded)?;
		VotingFor::<T, I>::try_mutate(who, &class, |voting| {
			let votes = &mut voting.votes;
			let i = votes
				.binary_search_by_key(&poll_index, |i| i.poll_index)
				.map_err(|_| Error::<T, I>::NotVoter)?;

			T::Polls::try_access_poll(poll_index, |poll_status| match poll_status {
				PollStatus::Ongoing(tally, _) => {
					// If the vote data exists.
					if let Some(account_vote) = votes[i].maybe_vote {
						ensure!(matches!(scope, UnvoteScope::Any), Error::<T, I>::NoPermission);

						// Remove vote from tally, shouldn't be possible to fail, but we handle it
						// gracefully.
						tally.remove(account_vote).ok_or(ArithmeticError::Underflow)?;

						// If standard aye nay vote, remove delegated votes.
						if let Some(approval) = account_vote.as_standard() {
							let final_delegations =
								voting.delegations.saturating_sub(votes[i].retracted_votes);
							tally.reduce(approval, final_delegations);
						}

						// Remove vote and fully remove record if there are no retracted votes to
						// track.
						votes[i].maybe_vote = None;
						if votes[i].retracted_votes == Default::default() {
							votes.remove(i);
						}

						// If delegating, update delegate's voting state.
						if let (Some(delegate), Some(conviction)) =
							(&voting.maybe_delegate, &voting.maybe_conviction)
						{
							VotingFor::<T, I>::mutate(delegate, &class, |delegate_voting| {
								let delegates_votes = &mut delegate_voting.votes;
								// Check vote data exists, shouldn't be possible for it not to if
								// delegator has voted & poll is ongoing.
								if let Ok(i) = delegates_votes
									.binary_search_by_key(&poll_index, |i| i.poll_index)
								{
									// Remove clawback from delegates vote record.
									let amount_delegated =
										conviction.votes(voting.delegated_balance);
									delegates_votes[i].retracted_votes = delegates_votes[i]
										.retracted_votes
										.saturating_sub(amount_delegated);

									// If delegate had voted and was standard vote.
									if let Some(approval) = delegates_votes[i]
										.maybe_vote
										.as_ref()
										.and_then(|v| v.as_standard())
									{
										// Increase tally by delegated amount.
										tally.increase(
											approval,
											conviction.votes(voting.delegated_balance),
										);
									}

									// And remove the voting data if there's no longer a reason
									// to hold.
									if delegates_votes[i].maybe_vote.is_none() &&
										delegates_votes[i].retracted_votes == Default::default()
									{
										delegates_votes.remove(i);
									}
								}
							});
						}

						Self::deposit_event(Event::VoteRemoved {
							who: who.clone(),
							vote: account_vote,
							poll_index,
						});
						T::VotingHooks::on_remove_vote(who, poll_index, Status::Ongoing);
					}
					Ok(())
				},
				PollStatus::Completed(end, approved) => {
					let old_vote = votes.remove(i);

					if let Some(account_vote) = old_vote.maybe_vote {
						if let Some((lock_periods, balance)) =
							account_vote.locked_if(vote::LockedIf::Status(approved))
						{
							Self::update_prior_lock(
								&mut voting.prior,
								end,
								lock_periods.into(),
								balance,
								scope,
							)?;
						} else if let AccountVote::Standard { vote, .. } = account_vote {
							if vote.aye != approved {
								// Unsuccessful vote, use hook to lock the funds too in case of
								// conviction.
								if let Some(to_lock) =
									T::VotingHooks::lock_balance_on_unsuccessful_vote(
										who, poll_index,
									) {
									Self::update_prior_lock(
										&mut voting.prior,
										end,
										vote.conviction.lock_periods().into(),
										to_lock,
										scope,
									)?;
								}
							}
						}

						// If delegating, update delegate's voting state.
						if let (Some(delegate), Some(_)) =
							(&voting.maybe_delegate, &voting.maybe_conviction)
						{
							Self::clean_delegate_vote_record(delegate, &class, poll_index);
						}

						// Call on_remove_vote hook.
						T::VotingHooks::on_remove_vote(who, poll_index, Status::Completed);
					}
					Ok(())
				},
				PollStatus::None => {
					// Poll was cancelled.
					let old_vote = votes.remove(i);
					// If had voted.
					if old_vote.maybe_vote.is_some() {
						// If delegating, update delegate's voting state.
						if let (Some(delegate), Some(_)) =
							(&voting.maybe_delegate, &voting.maybe_conviction)
						{
							Self::clean_delegate_vote_record(delegate, &class, poll_index);
						}

						T::VotingHooks::on_remove_vote(who, poll_index, Status::None);
					}
					Ok(())
				},
			})
		})
	}

	/// Removes the voting record from the delegator's delegate if no longer needed.
	///
	/// Only called in try_remove_vote PollStatus::Completed or PollStatus::None paths.
	fn clean_delegate_vote_record(
		delegate: &T::AccountId,
		class: &ClassOf<T, I>,
		poll_index: PollIndexOf<T, I>,
	) {
		VotingFor::<T, I>::mutate(delegate, class, |delegate_voting| {
			// Find the matching poll record on the delegate's account.
			if let Ok(idx) =
				delegate_voting.votes.binary_search_by_key(&poll_index, |v| v.poll_index)
			{
				// Remove vote record if delegate has no vote for it.
				if delegate_voting.votes[idx].maybe_vote.is_none() {
					delegate_voting.votes.remove(idx);
				}
			}
		});
	}

	/// Update a prior lock during vote removal.
	fn update_prior_lock(
		prior_lock: &mut PriorLock<BlockNumberFor<T, I>, BalanceOf<T, I>>,
		ending_block: BlockNumberFor<T, I>,
		lock_period_mult: BlockNumberFor<T, I>,
		balance: BalanceOf<T, I>,
		scope: UnvoteScope,
	) -> DispatchResult {
		let unlock_at = ending_block
			.saturating_add(T::VoteLockingPeriod::get().saturating_mul(lock_period_mult));
		if T::BlockNumberProvider::current_block_number() < unlock_at {
			ensure!(matches!(scope, UnvoteScope::Any), Error::<T, I>::NoPermissionYet);
			prior_lock.accumulate(unlock_at, balance);
		}
		Ok(())
	}

	/// Increase the amount delegated to `who` and update tallies accordingly.
	///
	/// This implementation uses an O(R + S) linear merge, where R is the number of
	/// delegator's votes and S is the number of delegate's votes.
	///
	/// Returns the number of (delegator, delegate) votes accessed in the process.
	fn increase_upstream_delegation(
		who: &T::AccountId,
		class: &ClassOf<T, I>,
		amount: Delegations<BalanceOf<T, I>>,
		delegators_ongoing_votes: Vec<PollIndexOf<T, I>>,
	) -> Result<(u32, u32), DispatchError> {
		VotingFor::<T, I>::try_mutate(who, class, |voting| {
			// Can't delegate if have votes & delegatee doesn't allow for so.
			if delegators_ongoing_votes.len() > 0 && !voting.allow_delegator_voting {
				return Err(Error::<T, I>::DelegatorVotingNotAllowed.into());
			}

			// Increase delegate's delegation counter.
			voting.delegations = voting.delegations.saturating_add(amount);

			let delegate_votes = core::mem::take(&mut voting.votes).into_inner();
			let (r_len, s_len) = (delegators_ongoing_votes.len(), delegate_votes.len());

			// First update all of delegates votes. Clawbacks will be added and adjusted next.
			for VoteRecord { poll_index, maybe_vote, .. } in delegate_votes.iter() {
				if let Some(AccountVote::Standard { vote, .. }) = maybe_vote {
					T::Polls::access_poll(*poll_index, |poll_status| {
						if let PollStatus::Ongoing(tally, _) = poll_status {
							tally.increase(vote.aye, amount);
						}
					});
				}
			}

			let mut new_votes = Vec::with_capacity(r_len + s_len);
			let mut r_iter = delegators_ongoing_votes.iter().peekable();
			let mut s_iter = delegate_votes.iter().peekable();

			// Iterate while both lists have items
			while let (Some(&r_poll_index), Some(&s_vote_record)) = (r_iter.peek(), s_iter.peek()) {
				if r_poll_index < &s_vote_record.poll_index {
					// Delegator vote not in delegate's list. Create new record.
					new_votes.push(VoteRecord {
						poll_index: *r_poll_index,
						maybe_vote: None,
						retracted_votes: amount,
					});
					r_iter.next(); // Consume delegator vote.
				} else if r_poll_index > &s_vote_record.poll_index {
					// Delegate vote not in delegator's list. Copy it.
					new_votes.push(s_vote_record.clone());
					s_iter.next(); // Consume delegate vote.
				} else {
					// Both have a vote for this poll.
					let mut matched_record = s_vote_record.clone();

					// Add the clawback amount.
					matched_record.retracted_votes =
						matched_record.retracted_votes.saturating_add(amount);

					// Apply the clawback to the tally.
					if let Some(AccountVote::Standard { vote, .. }) = matched_record.maybe_vote {
						T::Polls::access_poll(matched_record.poll_index, |poll_status| {
							if let PollStatus::Ongoing(tally, _) = poll_status {
								// Reduce the tally, counteracting the increase from earlier.
								tally.reduce(vote.aye, amount);
							}
						});
					}

					new_votes.push(matched_record);
					r_iter.next(); // Consume both.
					s_iter.next();
				}
			}

			// Exhaust the remaining items from which either iterator.
			for r_poll_index in r_iter {
				// Delegator-only votes left.
				new_votes.push(VoteRecord {
					poll_index: *r_poll_index,
					maybe_vote: None,
					retracted_votes: amount,
				});
			}

			for s_vote_record in s_iter {
				// Delegate-only votes left.
				new_votes.push(s_vote_record.clone());
			}

			// Replace the old, empty vote vec with the new merged vec.
			voting.votes = BoundedVec::try_from(new_votes)
				.map_err(|_| Error::<T, I>::DelegateMaxVotesReached)?;

			// Return the original (R, S) lengths for the weight calculation
			Ok((r_len as u32, s_len as u32))
		})
	}

	/// Reduce the amount delegated to `who` and update tallies accordingly.
	///
	/// This implementation uses an O(R + S) linear merge, where R is the number of
	/// delegator's votes and S is the number of delegate's votes.
	///
	/// Returns the number of (delegator, delegate) votes accessed in the process.
	fn reduce_upstream_delegation(
		who: &T::AccountId,
		class: &ClassOf<T, I>,
		amount: Delegations<BalanceOf<T, I>>,
		delegator_votes: Vec<PollIndexOf<T, I>>,
	) -> Result<(u32, u32), DispatchError> {
		// Grab the delegate's voting data.
		VotingFor::<T, I>::try_mutate(who, class, |voting| {
			// Reduce amount delegated to this delegate.
			voting.delegations = voting.delegations.saturating_sub(amount);

			let delegate_votes = core::mem::take(&mut voting.votes).into_inner();
			let (r_len, s_len) = (delegator_votes.len(), delegate_votes.len());

			// First preemptively update all of delegates votes. Clawbacks will be handled next.
			for VoteRecord { poll_index, maybe_vote, .. } in delegate_votes.iter() {
				if let Some(AccountVote::Standard { vote, .. }) = maybe_vote {
					T::Polls::access_poll(*poll_index, |poll_status| {
						if let PollStatus::Ongoing(tally, _) = poll_status {
							tally.reduce(vote.aye, amount);
						}
					});
				}
			}

			// Capacity will be at most delegate_votes.len(), as we are only removing items.
			let mut new_votes = Vec::with_capacity(s_len);
			let mut r_iter = delegator_votes.iter().peekable();
			let mut s_iter = delegate_votes.iter().peekable();

			while let (Some(&&r_poll_index), Some(&s_vote_record)) = (r_iter.peek(), s_iter.peek())
			{
				if r_poll_index < s_vote_record.poll_index {
					// Delegator vote not in delegate's list.
					r_iter.next(); // Consume delegator vote.
				} else if r_poll_index > s_vote_record.poll_index {
					// Delegate vote not in delegator's list. Copy it.
					new_votes.push(s_vote_record.clone());
					s_iter.next(); // Consume delegate vote.
				} else {
					// Both have a vote record for this poll.
					let mut matched_record = s_vote_record.clone();

					// Remove the delegator's contribution from the clawback amount.
					matched_record.retracted_votes =
						matched_record.retracted_votes.saturating_sub(amount);

					// Check poll status to reverse the tally reduction (if needed) and decide on
					// cleanup.
					let poll_has_ended =
						T::Polls::access_poll(matched_record.poll_index, |poll_status| {
							match poll_status {
								PollStatus::Ongoing(tally, _) => {
									// Give back the tally contribution.
									if let Some(AccountVote::Standard { vote, .. }) =
										matched_record.maybe_vote
									{
										tally.increase(vote.aye, amount);
									}
									false
								},
								_ => true, // Completed or None.
							}
						});

					// Only keep the record if still necessary.
					if matched_record.maybe_vote.is_some() ||
						(matched_record.retracted_votes.votes > Zero::zero() && !poll_has_ended)
					{
						new_votes.push(matched_record);
					}

					r_iter.next(); // Consume both.
					s_iter.next();
				}
			}

			for s_vote_record in s_iter {
				// Any remaining delegate votes are unaffected, copy.
				new_votes.push(s_vote_record.clone());
			}

			voting.votes = BoundedVec::try_from(new_votes)
				.expect("new_votes len is <= s_len, which is <= T::MaxVotes; qed");

			Ok((r_len as u32, s_len as u32))
		})
	}

	/// Attempt to delegate `balance` times `conviction` of voting power from `who` to `target`.
	///
	/// Returns the number of (delegate, delegator) votes accessed in the process.
	fn try_delegate(
		who: T::AccountId,
		class: ClassOf<T, I>,
		target: T::AccountId,
		conviction: Conviction,
		balance: BalanceOf<T, I>,
	) -> Result<(u32, u32), DispatchError> {
		// Sanity checks
		ensure!(who != target, Error::<T, I>::Nonsense);
		T::Polls::classes().binary_search(&class).map_err(|_| Error::<T, I>::BadClass)?;
		ensure!(balance <= T::Currency::total_balance(&who), Error::<T, I>::InsufficientFunds);

		let votes_accessed = VotingFor::<T, I>::try_mutate(
			&who,
			&class,
			|voting| -> Result<(u32, u32), DispatchError> {
				// Ensure not already delegating.
				if voting.maybe_delegate.is_some() {
					return Err(Error::<T, I>::AlreadyDelegating.into());
				}

				// Set delegation related info.
				voting.set_delegate_info(Some(target.clone()), balance, Some(conviction));

				// Collect all of the delegator's votes that are for ongoing polls.
				let delegators_ongoing_votes: Vec<_> = voting
					.votes
					.iter()
					.filter(|v| {
						v.maybe_vote.is_some() && T::Polls::as_ongoing(v.poll_index).is_some()
					})
					.map(|v| v.poll_index)
					.collect();

				// Update voting data of the chosen delegate.
				let votes_accessed = Self::increase_upstream_delegation(
					&target,
					&class,
					conviction.votes(balance),
					delegators_ongoing_votes,
				)?;

				// Extend the lock to `balance` (rather than setting it) since we don't know what
				// other votes are in place.
				Self::extend_lock(&who, &class, balance);
				Ok(votes_accessed)
			},
		)?;
		Self::deposit_event(Event::<T, I>::Delegated(who, target, class));
		Ok(votes_accessed)
	}

	/// Attempt to end the current delegation.
	///
	/// Returns the number of (delegate, delegator) votes accessed in the process.
	fn try_undelegate(
		who: T::AccountId,
		class: ClassOf<T, I>,
	) -> Result<(u32, u32), DispatchError> {
		let votes_accessed = VotingFor::<T, I>::try_mutate(
			&who,
			&class,
			|voting| -> Result<(u32, u32), DispatchError> {
				// If they're currently delegating.
				let (delegate, conviction) =
					match (&voting.maybe_delegate, &voting.maybe_conviction) {
						(Some(d), Some(c)) => (d, c),
						_ => return Err(Error::<T, I>::NotDelegating.into()),
					};

				// Collect all of the delegator's votes.
				let delegators_votes: Vec<_> = voting
					.votes
					.iter()
					.filter_map(|poll_vote| {
						poll_vote.maybe_vote.as_ref().map(|_| poll_vote.poll_index)
					})
					.collect();

				// Update their delegate's voting data.
				let votes_accessed = Self::reduce_upstream_delegation(
					&delegate,
					&class,
					conviction.votes(voting.delegated_balance),
					delegators_votes,
				)?;

				// Accumulate the locks.
				let now = T::BlockNumberProvider::current_block_number();
				let lock_periods = conviction.lock_periods().into();
				voting.prior.accumulate(
					now.saturating_add(T::VoteLockingPeriod::get().saturating_mul(lock_periods)),
					voting.delegated_balance,
				);

				// Set the delegator's delegate info.
				voting.set_delegate_info(None, Default::default(), None);
				Ok(votes_accessed)
			},
		)?;
		Self::deposit_event(Event::<T, I>::Undelegated(who, class));
		Ok(votes_accessed)
	}

	/// Toggle `allow_delegator_voting` for the specific class.
	fn toggle_delegator_voting(who: T::AccountId, class: ClassOf<T, I>) {
		VotingFor::<T, I>::mutate(&who, &class, |voting| {
			voting.allow_delegator_voting = !voting.allow_delegator_voting;
		});
	}

	// Update the lock for this class to be max(old, amount).
	fn extend_lock(who: &T::AccountId, class: &ClassOf<T, I>, amount: BalanceOf<T, I>) {
		ClassLocksFor::<T, I>::mutate(who, |locks| {
			match locks.iter().position(|x| &x.0 == class) {
				Some(i) => locks[i].1 = locks[i].1.max(amount),
				None => {
					let ok = locks.try_push((class.clone(), amount)).is_ok();
					debug_assert!(
						ok,
						"Vec bounded by number of classes; \
						all items in Vec associated with a unique class; \
						qed"
					);
				},
			}
		});
		T::Currency::extend_lock(
			CONVICTION_VOTING_ID,
			who,
			amount,
			WithdrawReasons::except(WithdrawReasons::RESERVE),
		);
	}

	/// Rejig the lock on an account. It will never get more stringent (since that would indicate
	/// a security hole) but may be reduced from what they are currently.
	fn update_lock(class: &ClassOf<T, I>, who: &T::AccountId) {
		let class_lock_needed = VotingFor::<T, I>::mutate(who, class, |voting| {
			voting.rejig(T::BlockNumberProvider::current_block_number());
			voting.locked_balance()
		});
		let lock_needed = ClassLocksFor::<T, I>::mutate(who, |locks| {
			locks.retain(|x| &x.0 != class);
			if !class_lock_needed.is_zero() {
				let ok = locks.try_push((class.clone(), class_lock_needed)).is_ok();
				debug_assert!(
					ok,
					"Vec bounded by number of classes; \
					all items in Vec associated with a unique class; \
					qed"
				);
			}
			locks.iter().map(|x| x.1).max().unwrap_or(Zero::zero())
		});
		if lock_needed.is_zero() {
			T::Currency::remove_lock(CONVICTION_VOTING_ID, who);
		} else {
			T::Currency::set_lock(
				CONVICTION_VOTING_ID,
				who,
				lock_needed,
				WithdrawReasons::except(WithdrawReasons::RESERVE),
			);
		}
	}

	/// Ensures that the pallet is not currently undergoing a multi-block storage migration.
	fn ensure_migration_not_ongoing() -> Result<(), DispatchError> {
		if MigrationOngoing::<T, I>::get() {
			return Err(Error::<T, I>::MigrationOngoing.into());
		}
		Ok(())
	}
}
